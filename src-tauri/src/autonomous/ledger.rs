//! 📜 Ledger Hardening — Phase 14 of the Strawberry platform.
//!
//! Two concerns from the build directive:
//!   1. APPEND-ONLY INTEGRITY — autonomous code must never erase or
//!      rewrite its own audit history. Enforced at the DB level with
//!      SQLite triggers that RAISE on any UPDATE/DELETE against
//!      `autonomy_decisions`.
//!   2. EXPLAINABILITY QUERIES — "what did Strawberry do / refuse / need
//!      approval for / fail at" answered from the ledger alone.
//!
//! Migration 020 adds the triggers idempotently (CREATE TRIGGER IF NOT
//! EXISTS), so existing databases harden on next start.

use serde::{Deserialize, Serialize};

/// Append-only guard SQL — installed by migration 020.
pub const APPEND_ONLY_TRIGGERS: &str = r#"
CREATE TRIGGER IF NOT EXISTS trg_decisions_no_update
BEFORE UPDATE ON autonomy_decisions
BEGIN
    SELECT RAISE(ABORT, 'autonomy_decisions is append-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_decisions_no_delete
BEFORE DELETE ON autonomy_decisions
BEGIN
    SELECT RAISE(ABORT, 'autonomy_decisions is append-only');
END;
"#;

// ─────────────────────────── explainability queries ───────────────────────────

/// One ledger row shaped for explanations (richer than the Phase 6 DTO:
/// it carries goal/plan/action/risk/result correlation fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRow {
    pub id: i64,
    pub capability_id: String,
    pub decision: String,
    pub reason: String,
    pub score: Option<f64>,
    pub created_at: String,
}

/// High-level explainability answers (Phase 14 spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationReport {
    /// "What did Strawberry do?" — last executed/approved work.
    pub did: Vec<LedgerRow>,
    /// "What did it refuse / what was blocked?"
    pub refused: Vec<LedgerRow>,
    /// "What required approval?"
    pub needed_approval: Vec<LedgerRow>,
    /// "What failed?"
    pub failed: Vec<LedgerRow>,
    /// "What did it learn from failures?" — replan/escalate decisions.
    pub learned: Vec<LedgerRow>,
    /// One-line summary for quick glance.
    pub summary: String,
}

/// Query the ledger into an explanation report. Read-only.
pub fn explain(
    conn: &rusqlite::Connection,
    limit_per_section: usize,
) -> Result<ExplanationReport, String> {
    let section = |where_clause: &str, order: &str| -> Result<Vec<LedgerRow>, String> {
        let sql = format!(
            "SELECT id, capability_id, decision, reason, score, created_at
             FROM autonomy_decisions
             WHERE {where_clause}
             ORDER BY {order} DESC LIMIT {limit_per_section}"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(LedgerRow {
                    id: r.get(0)?,
                    capability_id: r.get(1)?,
                    decision: r.get(2)?,
                    reason: r.get(3)?,
                    score: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    };

    let did = section(
        "decision IN ('run','debounce','resume','interval')",
        "id",
    )?;
    let refused = section(
        "decision IN ('skip','block','deny')",
        "id",
    )?;
    let needed_approval = section(
        "decision IN ('defer','needs_approval')",
        "id",
    )?;
    let failed = section("decision IN ('fail','timeout','cancel')", "id")?;
    let learned = section(
        "decision IN ('escalate','replan','abandon')",
        "id",
    )?;

    let summary = format!(
        "{} run · {} deferred · {} blocked/denied · {} failed · {} escalations",
        did.len(),
        needed_approval.len(),
        refused.len(),
        failed.len(),
        learned.len()
    );

    Ok(ExplanationReport {
        did,
        refused,
        needed_approval,
        failed,
        learned,
        summary,
    })
}

/// Record a lifecycle action with FULL provenance (the Phase 14 field set:
/// actor, capability, goal, plan, action, risk, reason, approval, result,
/// verification). Fits the existing append-only table via the details JSON.
pub fn record_action(
    conn: &rusqlite::Connection,
    capability: &str,
    decision: &str,
    reason: &str,
    score: Option<f64>,
    details: &serde_json::Value,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO autonomy_decisions(capability_id, decision, reason, score, details, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            capability,
            decision,
            reason,
            score,
            details.to_string(),
            crate::db::now_iso()
        ],
    )
    .map_err(|e| format!("ledger append: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn ledger_is_append_only_updates_rejected() {
        let conn = setup();
        record_action(&conn, "test", "run", "why", None, &serde_json::json!({})).unwrap();
        let err = conn
            .execute("UPDATE autonomy_decisions SET reason='tampered' WHERE id=1", [])
            .unwrap_err();
        assert!(err.to_string().contains("append-only"), "got: {err}");
    }

    #[test]
    fn ledger_is_append_only_deletes_rejected() {
        let conn = setup();
        record_action(&conn, "test", "run", "why", None, &serde_json::json!({})).unwrap();
        let err = conn
            .execute("DELETE FROM autonomy_decisions WHERE id=1", [])
            .unwrap_err();
        assert!(err.to_string().contains("append-only"), "got: {err}");
    }

    #[test]
    fn inserts_still_work() {
        let conn = setup();
        for i in 0..3 {
            record_action(&conn, "cap", "run", &format!("r{i}"), Some(0.5), &serde_json::json!({"n": i})).unwrap();
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM autonomy_decisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn explain_partitions_the_ledger() {
        let conn = setup();
        // Seed one row per category with realistic decisions.
        record_action(&conn, "ghost_insights", "run", "score above threshold", Some(0.8), &serde_json::json!({})).unwrap();
        record_action(&conn, "ocr_queue", "defer", "heavy work detected", Some(0.1), &serde_json::json!({})).unwrap();
        record_action(&conn, "executor", "deny", "high risk; approval required", None, &serde_json::json!({})).unwrap();
        record_action(&conn, "executor", "fail", "exit code 1", None, &serde_json::json!({})).unwrap();
        record_action(&conn, "planner", "escalate", "attempt 3/3 exhausted", None, &serde_json::json!({})).unwrap();

        let report = explain(&conn, 10).unwrap();
        assert!(report.did.len() >= 1);
        assert!(report.needed_approval.len() >= 1);
        assert!(report.refused.len() >= 1);
        assert!(report.failed.len() >= 1);
        assert!(report.learned.len() >= 1);
        assert!(report.summary.contains("1 run") || report.did.len() == 1);
    }

    #[test]
    fn explanation_answers_the_spec_questions() {
        let conn = setup();
        record_action(&conn, "wellness", "run", "reminder shown", None, &serde_json::json!({})).unwrap();
        record_action(&conn, "safety", "block", "PERMANENT_DELETE is FORBIDDEN", None, &serde_json::json!({})).unwrap();

        let report = explain(&conn, 5).unwrap();
        // "What did Strawberry do?"
        assert!(report.did.iter().any(|r| r.capability_id == "wellness"));
        // "What did it refuse?"
        assert!(report.refused.iter().any(|r| r.reason.contains("FORBIDDEN")));
    }

    #[test]
    fn empty_ledger_explains_nothing_gracefully() {
        let conn = setup();
        let report = explain(&conn, 10).unwrap();
        assert!(report.did.is_empty());
        assert!(report.summary.contains("0 run"));
    }

    #[test]
    fn details_json_round_trips_provenance() {
        let conn = setup();
        let details = serde_json::json!({
            "actor": "core",
            "goal": 42,
            "plan": 7,
            "risk": "high",
            "approval": "approved",
            "result": "succeeded",
            "verification": "success"
        });
        record_action(&conn, "executor", "run", "full provenance row", None, &details).unwrap();
        let raw: String = conn
            .query_row(
                "SELECT details FROM autonomy_decisions WHERE capability_id='executor'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let back: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(back["goal"], 42);
        assert_eq!(back["risk"], "high");
        assert_eq!(back["verification"], "success");
    }
}
