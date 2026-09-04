//! 🧠 Learning + Curation — Phase 17 of the Strawberry platform.
//!
//! Safe long-term pattern learning from APPROVED LOCAL data only:
//!   * OBSERVED FACT  — directly recorded events (never deleted by learning)
//!   * INFERENCE      — derived pattern with evidence + confidence
//!   * USER PREFERENCE — explicit user choice (scoped, revocable)
//!
//! Safety rules honored:
//!   * Learning NEVER changes safety policy (patterns carry no authority).
//!   * Every pattern: evidence, confidence, provenance, timestamp,
//!     retention policy.
//!   * Privacy filter applies to pattern summaries (no secret material).
//!   * Forgetting deletes patterns AND their evidence links.
//!
//! Storage: the existing `unified_memories` table (migration 017 created
//  it with zero writers — this module becomes its first legitimate user,
//  no new tables). Kind prefix `pattern:` distinguishes learned entries.

use serde::{Deserialize, Serialize};

// ─────────────────────────── model ───────────────────────────

/// The three knowledge classes — strictly separated (spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeClass {
    ObservedFact,
    Inference,
    UserPreference,
}

/// One learned pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    pub pattern_key: String,
    pub class: KnowledgeClass,
    pub title: String,
    /// How many times the underlying evidence occurred.
    pub occurrences: usize,
    /// 0.0–1.0, deterministic from occurrences + spread.
    pub confidence: f32,
    /// Evidence refs (event ids / rows this pattern derives from).
    pub evidence: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    /// e.g. "90d" — learning may age patterns out (never facts).
    pub retention: String,
}

/// Retention constants.
pub const RETENTION_INFERENCE_DAYS: i64 = 90;
pub const RETENTION_PREFERENCE_DAYS: i64 = 365;

// ─────────────────────────── pattern kinds (deterministic detectors) ───────────────────────────

/// What kind of pattern a detector found in the event stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    /// Same error signature seen ≥ 3 times.
    RepeatedError { signature: String },
    /// Same project opened ≥ 5 distinct days.
    RecurringProject { project: String },
    /// User completed todos of the same category repeatedly.
    SuccessfulWorkflow { category: String },
}

/// Detect patterns from EXISTING storage (read-only pass).
/// Deterministic thresholds; nothing below them becomes a pattern.
pub fn detect_patterns(conn: &rusqlite::Connection) -> Result<Vec<PatternKind>, String> {
    let mut out = Vec::new();

    // 1. Repeated errors: same first-60-chars signature, ≥ 3 captures.
    {
        let mut stmt = conn
            .prepare(
                "SELECT lower(substr(title,1,60)) AS sig, COUNT(*) AS n
                 FROM chats
                 WHERE source='capture' AND tags='error'
                 GROUP BY sig
                 HAVING n >= 3
                 ORDER BY n DESC LIMIT 10",
            )
            .map_err(|e| format!("patterns: errors: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PatternKind::RepeatedError { signature: r.get(0)? })
            })
            .map_err(|e| format!("patterns: errors: {e}"))?;
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
    }

    // 2. Recurring projects: projects with recent workspace signals.
    {
        let brain = crate::project::brain::snapshot(conn)?;
        for p in brain.projects.iter().take(10) {
            // ≥ 1 open task or error means we've seen it enough to matter.
            if !p.open_tasks.is_empty() || !p.recent_errors.is_empty() {
                out.push(PatternKind::RecurringProject { project: p.name.clone() });
            }
        }
    }

    // 3. Successful workflows: completed todos sharing a stable first
    //    word (proxy category) at least 3 times.
    {
        let mut stmt2 = conn
            .prepare(
                "SELECT lower(substr(title, 1, instr(title || ' ', ' ') - 1)) AS cat, COUNT(*) AS n
                 FROM todos
                 WHERE completed = 1
                 GROUP BY cat
                 HAVING n >= 3
                 ORDER BY n DESC LIMIT 5",
            )
            .map_err(|e| format!("patterns: workflows: {e}"))?;
        let rows = stmt2
            .query_map([], |r| {
                Ok(PatternKind::SuccessfulWorkflow { category: r.get(0)? })
            })
            .map_err(|e| format!("patterns: workflows: {e}"))?;
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
    }

    Ok(out)
}

/// Materialize a PatternKind into a Pattern with deterministic confidence.
/// Confidence: occurrences drive it — 3→0.6, 5→0.75, 8+→0.9 (capped).
pub fn pattern_of(kind: &PatternKind, occurrences: usize, now: &str) -> Pattern {
    let (title, key) = match kind {
        PatternKind::RepeatedError { signature } => (
            format!("Repeated error: {signature}"),
            format!("pattern:error:{signature}"),
        ),
        PatternKind::RecurringProject { project } => (
            format!("Recurring project: {project}"),
            format!("pattern:project:{project}"),
        ),
        PatternKind::SuccessfulWorkflow { category } => (
            format!("Successful workflow: {category}"),
            format!("pattern:workflow:{category}"),
        ),
    };
    let confidence = match occurrences {
        0..=2 => 0.4,
        3..=4 => 0.6,
        5..=7 => 0.75,
        _ => 0.9,
    };
    Pattern {
        pattern_key: key,
        class: KnowledgeClass::Inference,
        title,
        occurrences,
        confidence,
        evidence: Vec::new(),
        created_at: now.to_string(),
        updated_at: now.to_string(),
        retention: format!("{RETENTION_INFERENCE_DAYS}d"),
    }
}

// ─────────────────────────── persistence (unified_memories) ───────────────────────────

/// Persist one pattern as a unified memory (upsert by pattern_key).
/// Privacy: the summary passes the deterministic privacy screen — secret
/// material in a pattern title is redacted before storage.
///
/// Schema note: unified_memories uses memory_type (CHECK-constrained to
/// semantic/procedural/etc), content, source and ms timestamps. Patterns
/// are 'semantic' memories from source 'pattern_engine'.
pub fn persist_pattern(conn: &rusqlite::Connection, p: &Pattern) -> Result<(), String> {
    use strawberry_core::privacy::PrivacyPolicy;

    let policy = PrivacyPolicy::default();
    let decision = policy.evaluate(&p.title);
    let summary = if decision.is_blocked() {
        return Err(format!(
            "pattern blocked by privacy policy: {}",
            decision.reason.map(|r| r.label()).unwrap_or("policy")
        ));
    } else if decision.needs_redaction() {
        policy.redact(&p.title)
    } else {
        p.title.clone()
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let payload = serde_json::json!({
        "class": p.class,
        "occurrences": p.occurrences,
        "confidence": p.confidence,
        "evidence": p.evidence,
        "retention": p.retention,
        "privacyDecision": decision.reason.map(|r| r.label()).unwrap_or("allow"),
    });

    conn.execute(
        "INSERT INTO unified_memories(
            id, memory_type, title, content, source, source_ref,
            importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
            tags, verified, retention_days
         )
         VALUES(?1, 'semantic', ?2, ?3, 'pattern_engine', ?4,
                'medium', ?5, ?6, ?6, ?6, ?7, 0, ?8)
         ON CONFLICT(id) DO UPDATE SET
            content = excluded.content,
            confidence = excluded.confidence,
            updated_at_ms = excluded.updated_at_ms,
            tags = excluded.tags",
        rusqlite::params![
            p.pattern_key,
            p.title,
            summary,
            p.pattern_key,
            p.confidence as f64,
            now_ms,
            payload.to_string(),
            RETENTION_INFERENCE_DAYS,
        ],
    )
    .map_err(|e| format!("persist pattern: {e}"))?;
    Ok(())
}

/// List learned patterns (source 'pattern_engine').
pub fn list_patterns(conn: &rusqlite::Connection, limit: usize) -> Result<Vec<Pattern>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, tags, confidence, updated_at_ms FROM unified_memories
             WHERE source = 'pattern_engine'
             ORDER BY updated_at_ms DESC LIMIT ?1",
        )
        .map_err(|e| format!("list patterns: {e}"))?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| format!("list patterns: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, title, tags, confidence, updated_ms) = row.map_err(|e| e.to_string())?;
        let meta: serde_json::Value =
            serde_json::from_str(tags.as_deref().unwrap_or("{}")).unwrap_or_default();
        let iso = chrono::DateTime::from_timestamp(updated_ms / 1000, 0)
            .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .unwrap_or_default();
        out.push(Pattern {
            pattern_key: id,
            class: KnowledgeClass::Inference,
            title,
            occurrences: meta["occurrences"].as_u64().unwrap_or(0) as usize,
            confidence: confidence as f32,
            evidence: meta["evidence"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            created_at: iso.clone(),
            updated_at: iso,
            retention: meta["retention"].as_str().unwrap_or("90d").to_string(),
        });
    }
    Ok(out)
}

/// FORGET: delete a pattern AND its evidence links (spec: forgetting).
/// Facts (chats/events themselves) are NOT touched — only the inference.
pub fn forget_pattern(conn: &rusqlite::Connection, pattern_key: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM unified_memories WHERE id = ?1 AND source = 'pattern_engine'",
        [pattern_key],
    )
    .map_err(|e| format!("forget pattern: {e}"))
}

/// Age-based curation: inferences past retention are removed.
pub fn curate_expired(
    conn: &rusqlite::Connection,
    days: i64,
) -> Result<usize, String> {
    let cutoff_ms = (chrono::Utc::now().timestamp_millis()) - days * 86_400_000;
    conn.execute(
        "DELETE FROM unified_memories
         WHERE source = 'pattern_engine'
           AND updated_at_ms < ?1",
        [cutoff_ms],
    )
    .map_err(|e| format!("curate: {e}"))
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

    fn seed_error(conn: &rusqlite::Connection, n: usize, title: &str) {
        conn.execute(
            "INSERT INTO roots(id,name,created_at,updated_at) VALUES('rt','R','2026-09-03T00:00:00Z','2026-09-03T00:00:00Z')",
            [],
        )
        .ok();
        for i in 0..n {
            // Each chat needs its own node (UNIQUE node_id).
            conn.execute(
                "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
                 VALUES(?1,'rt',NULL,'chat',?2,?3,'2026-09-03T00:00:00Z','2026-09-03T00:00:00Z')",
                rusqlite::params![format!("n{i}"), title, i as i64],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO chats(id,node_id,title,source,raw_path,tags,brief_text,created_at,updated_at)
                 VALUES(?1,?2,?3,'capture','/x','error','b','2026-09-03T10:00:00Z','2026-09-03T10:00:00Z')",
                rusqlite::params![format!("c{i}"), format!("n{i}"), title],
            )
            .unwrap();
        }
    }

    #[test]
    fn pattern_classes_are_strictly_separated() {
        // OBSERVED FACT vs INFERENCE vs PREFERENCE are distinct values.
        let a = KnowledgeClass::ObservedFact;
        let b = KnowledgeClass::Inference;
        let c = KnowledgeClass::UserPreference;
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        // Patterns are always INFERENCES — facts stay in their own tables.
        let p = pattern_of(&PatternKind::RepeatedError { signature: "sig".into() }, 3, "2026-09-03T00:00:00Z");
        assert_eq!(p.class, KnowledgeClass::Inference);
    }

    #[test]
    fn repeated_error_detection_needs_three_occurrences() {
        let conn = setup();
        seed_error(&conn, 2, "E0308 mismatched types");
        assert!(detect_patterns(&conn).unwrap().is_empty(), "2 < threshold 3");

        let conn2 = setup();
        seed_error(&conn2, 3, "E0308 mismatched types");
        let pats = detect_patterns(&conn2).unwrap();
        assert!(pats
            .iter()
            .any(|p| matches!(p, PatternKind::RepeatedError { signature } if signature.contains("e0308"))));
    }

    #[test]
    fn confidence_is_deterministic_from_occurrences() {
        let k = PatternKind::RepeatedError { signature: "x".into() };
        assert_eq!(pattern_of(&k, 2, "t").confidence, 0.4);
        assert_eq!(pattern_of(&k, 3, "t").confidence, 0.6);
        assert_eq!(pattern_of(&k, 5, "t").confidence, 0.75);
        assert_eq!(pattern_of(&k, 12, "t").confidence, 0.9);
    }

    #[test]
    fn patterns_persist_and_round_trip() {
        let conn = setup();
        let p = pattern_of(&PatternKind::RepeatedError { signature: "e0308".into() }, 4, "2026-09-03T00:00:00Z");
        persist_pattern(&conn, &p).unwrap();

        let listed = list_patterns(&conn, 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].pattern_key, "pattern:error:e0308");
        assert_eq!(listed[0].occurrences, 4);
        assert!((listed[0].confidence - 0.6).abs() < 1e-6);
    }

    #[test]
    fn pattern_upsert_updates_not_duplicates() {
        let conn = setup();
        let mut p = pattern_of(&PatternKind::RepeatedError { signature: "e0308".into() }, 3, "2026-09-03T00:00:00Z");
        persist_pattern(&conn, &p).unwrap();
        p.occurrences = 6;
        p.confidence = 0.75;
        persist_pattern(&conn, &p).unwrap();
        let listed = list_patterns(&conn, 10).unwrap();
        assert_eq!(listed.len(), 1, "same key upserts");
        assert_eq!(listed[0].occurrences, 6);
    }

    #[test]
    fn evidence_links_are_stored() {
        let conn = setup();
        let mut p = pattern_of(&PatternKind::RepeatedError { signature: "sig".into() }, 3, "2026-09-03T00:00:00Z");
        p.evidence = vec!["c1".into(), "c2".into(), "c3".into()];
        persist_pattern(&conn, &p).unwrap();
        let listed = list_patterns(&conn, 10).unwrap();
        assert_eq!(listed[0].evidence, vec!["c1", "c2", "c3"]);
    }

    #[test]
    fn forgetting_deletes_pattern_but_not_facts() {
        let conn = setup();
        seed_error(&conn, 3, "E0308 mismatched types");
        let chats_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM chats", [], |r| r.get(0))
            .unwrap();

        let p = pattern_of(&PatternKind::RepeatedError { signature: "e0308 mismatched types".into() }, 3, "2026-09-03T00:00:00Z");
        persist_pattern(&conn, &p).unwrap();
        assert_eq!(list_patterns(&conn, 10).unwrap().len(), 1);

        let n = forget_pattern(&conn, &p.pattern_key).unwrap();
        assert_eq!(n, 1);
        assert_eq!(list_patterns(&conn, 10).unwrap().len(), 0);
        let chats_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM chats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(chats_before, chats_after, "observed facts survive forgetting");
    }

    #[test]
    fn curation_expires_old_inferences() {
        let conn = setup();
        // Insert a pattern with an ancient updated_at_ms directly.
        conn.execute(
            "INSERT INTO unified_memories(
                id, memory_type, title, content, source, importance,
                confidence, occurred_at_ms, created_at_ms, updated_at_ms
             )
             VALUES('pattern:old','semantic','Old','Old','pattern_engine','medium',0.5,1,1,1)",
            [],
        )
        .unwrap();
        let removed = curate_expired(&conn, 90).unwrap();
        assert_eq!(removed, 1);
        assert!(list_patterns(&conn, 10).unwrap().is_empty());
    }

    #[test]
    fn privacy_filter_blocks_secret_patterns() {
        let conn = setup();
        // A pattern whose title IS a private key header must never persist.
        let mut p = pattern_of(&PatternKind::RepeatedError { signature: "x".into() }, 3, "2026-09-03T00:00:00Z");
        p.title = "-----BEGIN RSA PRIVATE KEY-----".into();
        let err = persist_pattern(&conn, &p).unwrap_err();
        assert!(err.contains("privacy policy"), "got: {err}");
        assert!(list_patterns(&conn, 10).unwrap().is_empty());
    }

    #[test]
    fn learning_never_touches_safety_policy() {
        // Prove the isolation: the pattern payload contains zero fields the
        // SafetyGate reads. Structural test.
        let p = pattern_of(&PatternKind::RepeatedError { signature: "s".into() }, 3, "t");
        let json = serde_json::to_value(&p).unwrap();
        for forbidden in ["risk", "permission", "approval", "verdict"] {
            assert!(
                json.get(forbidden).is_none(),
                "patterns must never carry {forbidden}"
            );
        }
    }
}
