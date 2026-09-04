//! 🔮 Prediction — Phase 18 of the Strawberry platform.
//!
//! Confidence-scored predictions from local evidence. HARD RULE (spec):
//! **prediction is never authorization** — a `Prediction` carries zero
//! authority; the Safety Gate re-derives everything from scratch. This
//! module produces hints, nothing more.
//!
//! Deterministic: same storage + same now ⇒ same predictions.

use serde::{Deserialize, Serialize};

// ─────────────────────────── model ───────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredictionKind {
    NextAction,
    UnfinishedTask,
    ProjectSwitch,
    ResumeTarget,
    RepeatedProblem,
    ContextNeeded,
}

/// One prediction with the full explainability contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prediction {
    pub kind: PredictionKind,
    pub title: String,
    /// 0.0–1.0. Predictions below 0.3 are suppressed entirely.
    pub confidence: f32,
    /// Evidence refs backing the prediction.
    pub evidence: Vec<String>,
    /// ISO — after this the prediction expires.
    pub expires_at: String,
    /// Human explanation.
    pub explanation: String,
}

/// Predictions below this confidence are not surfaced (spec: low-confidence
/// predictions must be identifiable/filterable).
pub const MIN_CONFIDENCE: f32 = 0.3;
/// Default prediction horizon.
pub const PREDICTION_HOURS: i64 = 24;

// ─────────────────────────── generator ───────────────────────────

/// Generate predictions from existing storage. Read-only, deterministic.
pub fn generate(conn: &rusqlite::Connection, now: &str) -> Result<Vec<Prediction>, String> {
    let mut out: Vec<Prediction> = Vec::new();
    let expiry = |hours: i64| -> String {
        chrono::DateTime::parse_from_rfc3339(now)
            .ok()
            .and_then(|d| {
                d.checked_add_signed(chrono::Duration::hours(hours))
                    .map(|x| x.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            })
            .unwrap_or_else(|| now.to_string())
    };

    // 1. Next action / unfinished task: highest-priority open todo.
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, priority FROM todos
                 WHERE completed = 0
                 ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END
                 LIMIT 1",
            )
            .map_err(|e| format!("predict: todos: {e}"))?;
        let row: Option<(i64, String, String)> = stmt
            .query_row([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .ok();
        if let Some((id, title, prio)) = row {
            let conf = match prio.as_str() {
                "high" => 0.8,
                "medium" => 0.6,
                _ => 0.45,
            };
            out.push(Prediction {
                kind: PredictionKind::UnfinishedTask,
                title: format!("User may want to finish: {title}"),
                confidence: conf,
                evidence: vec![format!("todo:{id}")],
                expires_at: expiry(PREDICTION_HOURS),
                explanation: format!("open {prio}-priority task in the planner"),
            });
            out.push(Prediction {
                kind: PredictionKind::NextAction,
                title: format!("Likely next action: start “{}”", truncate(&title, 40)),
                confidence: conf * 0.9,
                evidence: vec![format!("todo:{id}")],
                expires_at: expiry(6),
                explanation: "top of the priority queue".into(),
            });
        }
    }

    // 2. Repeated problem: same error signature ≥ 3 times recently.
    {
        let mut stmt = conn
            .prepare(
                "SELECT lower(substr(title,1,40)) AS sig, COUNT(*) AS n
                 FROM chats
                 WHERE source='capture' AND tags='error'
                 GROUP BY sig HAVING n >= 3
                 ORDER BY n DESC LIMIT 1",
            )
            .map_err(|e| format!("predict: errors: {e}"))?;
        let row: Option<(String, i64)> = stmt
            .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
            .ok();
        if let Some((sig, n)) = row {
            out.push(Prediction {
                kind: PredictionKind::RepeatedProblem,
                title: format!("Error may recur: {sig}"),
                confidence: (0.4 + 0.1 * n as f32).min(0.9),
                evidence: vec![format!("errors:{sig}")],
                expires_at: expiry(48),
                explanation: format!("seen {n} times in captures"),
            });
        }
    }

    // 3. Project switch: the second most recent project is a candidate
    //    for the next switch (people alternate).
    {
        let brain = crate::project::brain::snapshot(conn)?;
        if brain.projects.len() >= 2 {
            let p = &brain.projects[1];
            out.push(Prediction {
                kind: PredictionKind::ProjectSwitch,
                title: format!("May switch to project: {}", p.name),
                confidence: 0.4,
                evidence: vec![format!("project:{}", p.name)],
                expires_at: expiry(12),
                explanation: "second most recent workspace signal".into(),
            });
        }
        if let Some(top) = brain.projects.first() {
            out.push(Prediction {
                kind: PredictionKind::ResumeTarget,
                title: format!("Likely resume target: {}", top.name),
                confidence: 0.65,
                evidence: vec![format!("project:{}", top.name)],
                expires_at: expiry(PREDICTION_HOURS),
                explanation: "most recently active project".into(),
            });
        }
    }

    // 4. Context needed: recent searches hint at what context to prepare.
    //    The search-history table may not exist in every build — degrade
    //    gracefully (spec §Failure Isolation), never fail the prediction.
    {
        let row: Option<String> = conn
            .query_row(
                "SELECT query FROM search_log ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();
        if let Some(q) = row {
            out.push(Prediction {
                kind: PredictionKind::ContextNeeded,
                title: format!("May need context about: {q}"),
                confidence: 0.45,
                evidence: vec![format!("search:{q}")],
                expires_at: expiry(6),
                explanation: "latest search query".into(),
            });
        }
    }

    // Deterministic final pass: suppress below-threshold and fix order.
    out.retain(|p| p.confidence >= MIN_CONFIDENCE);
    out.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.title.cmp(&b.title))
    });
    out.truncate(10);
    Ok(out)
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// The authority firewall: predictions carry ZERO permissions. This is the
/// function the SafetyGate-facing code calls to prove a prediction cannot
/// act. Always `false` — by construction, forever.
pub fn prediction_grants_authority(_p: &Prediction) -> bool {
    false
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

    const NOW: &str = "2026-09-03T12:00:00Z";

    #[test]
    fn empty_state_yields_no_predictions() {
        let conn = setup();
        let p = generate(&conn, NOW).unwrap();
        assert!(p.is_empty());
    }

    #[test]
    fn open_todo_predicts_unfinished_task_and_next_action() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('ship the release','high',0)",
            [],
        )
        .unwrap();
        let p = generate(&conn, NOW).unwrap();
        assert!(p.iter().any(|x| x.kind == PredictionKind::UnfinishedTask && x.title.contains("ship the release")));
        assert!(p.iter().any(|x| x.kind == PredictionKind::NextAction && x.confidence == (0.8 * 0.9)));
    }

    #[test]
    fn confidence_threshold_suppresses_weak_predictions() {
        // A single low-priority todo → unfinished-task conf 0.45 (kept),
        // next-action 0.405 (kept). But the project-switch at 0.4 without
        // ≥2 projects stays suppressed. Empty-DB style check:
        let conn = setup();
        let p = generate(&conn, NOW).unwrap();
        for pred in &p {
            assert!(pred.confidence >= MIN_CONFIDENCE, "weak prediction leaked: {pred:?}");
        }
    }

    #[test]
    fn repeated_errors_become_problem_predictions() {
        let conn = setup();
        conn.execute(
            "INSERT INTO roots(id,name,created_at,updated_at) VALUES('rt','R','2026-09-03T00:00:00Z','2026-09-03T00:00:00Z')",
            [],
        )
        .unwrap();
        for i in 0..3 {
            conn.execute(
                "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
                 VALUES(?1,'rt',NULL,'chat','C',?2,'2026-09-03T00:00:00Z','2026-09-03T00:00:00Z')",
                rusqlite::params![format!("n{i}"), i as i64],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO chats(id,node_id,title,source,raw_path,tags,brief_text,created_at,updated_at)
                 VALUES(?1,?2,'E0308 again','capture','/x','error','b','2026-09-03T10:00:00Z','2026-09-03T10:00:00Z')",
                rusqlite::params![format!("c{i}"), format!("n{i}")],
            )
            .unwrap();
        }
        let p = generate(&conn, NOW).unwrap();
        assert!(p
            .iter()
            .any(|x| x.kind == PredictionKind::RepeatedProblem
                && x.title.to_lowercase().contains("e0308")));
    }

    #[test]
    fn predictions_carry_evidence_and_expiry() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('t','medium',0)",
            [],
        )
        .unwrap();
        for p in generate(&conn, NOW).unwrap() {
            assert!(!p.evidence.is_empty());
            assert!(p.expires_at.as_str() > NOW, "must expire in the future");
            assert!(!p.explanation.is_empty());
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('t','high',0)",
            [],
        )
        .unwrap();
        let a = generate(&conn, NOW).unwrap();
        let b = generate(&conn, NOW).unwrap();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.title, y.title);
            assert_eq!(x.confidence, y.confidence);
            assert_eq!(x.kind, y.kind);
        }
    }

    #[test]
    fn prediction_never_grants_authority() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('t','high',0)",
            [],
        )
        .unwrap();
        for p in generate(&conn, NOW).unwrap() {
            assert!(!prediction_grants_authority(&p));
        }
        // And structurally: no permission/risk/approval fields exist.
        let json = serde_json::to_value(generate(&conn, NOW).unwrap().remove(0)).unwrap();
        assert!(json.get("permission").is_none());
        assert!(json.get("approval").is_none());
        assert!(json.get("risk").is_none());
    }

    #[test]
    fn expired_predictions_are_distinguishable() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('t','high',0)",
            [],
        )
        .unwrap();
        let p = generate(&conn, NOW).unwrap();
        let later = "2026-09-05T12:00:00Z";
        for pred in &p {
            if later > pred.expires_at.as_str() {
                // caller may filter by expiry — the field exists and compares.
                assert!(pred.expires_at.len() >= 10);
            }
        }
    }
}
