//! 🎯 Goal Engine — Phase 7 of the Strawberry platform.
//!
//! Deterministic, evidence-backed goal detection. Consumes EXISTING storage
//! only (todos, captured errors, resume points, project brain) — no new
//! tables, no new writers, no invented intent.
//!
//! NON-GOALS (later phases): no planning, no execution, no safety decisions.
//! The engine produces `GoalCandidate`s and their lifecycle transitions;
//! what happens with them is Phase 8+ business.
//!
//! Determinism contract: the same DB state + the same wall-clock second
//! produce byte-identical candidates (ids derive from content hash, scores
//! from fixed arithmetic — no randomness, no LLM).
//!
//! Golden rule honored: nothing here trusts a model; the LLM (Phase 19) may
//! only *propose* extra candidates through the same evidence rules.

use serde::{Deserialize, Serialize};

use super::ids::GoalId;

// ─────────────────────────── model ───────────────────────────

/// Where a goal's evidence came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Task,
    Error,
    Resume,
    Project,
}

/// One piece of evidence backing a candidate. `ref` points at the row in
/// the owning table (todos.id, chats.id, chat_resume_points.id, or the
/// project path for Project evidence).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub kind: EvidenceKind,
    #[serde(rename = "ref")]
    pub reference: String,
    pub summary: String,
    /// Weight multiplier ≥ 1.0 applied when merging duplicates.
    pub weight: f32,
}

/// Lifecycle of a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    /// Fresh candidate, not yet accepted by the user/agent.
    Candidate,
    /// Accepted for planning (Phase 8 will pick these up).
    Accepted,
    /// Evidence satisfied (todo completed, error cleared…).
    Completed,
    /// Cancelled by user or superseded by a conflicting goal.
    Cancelled,
    /// Expired (see `expires_at`).
    Expired,
}

/// Priority derived deterministically from evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
}

/// A deterministic goal candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalCandidate {
    /// Content-derived stable id: `goal-<hash>`. Same evidence ⇒ same id
    /// (that's the dedup primitive).
    pub goal_id: GoalId,
    pub title: String,
    pub description: String,
    /// Project name or context tag when known.
    pub project: Option<String>,
    pub priority: Priority,
    /// 0.0–1.0 deterministic confidence (evidence-count + weights capped).
    pub confidence: f32,
    pub evidence: Vec<Evidence>,
    pub status: GoalStatus,
    pub created_at: String,
    pub expires_at: String,
}

// ─────────────────────────── ids & hashing ───────────────────────────

/// FNV-1a over the normalized title+project — stable across runs,
/// cheap, and good enough for a dedup key (not security).
fn goal_hash(title: &str, project: Option<&str>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title.to_lowercase().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= 0x2f;
    h = h.wrapping_mul(0x0000_0100_0000_01b3);
    for b in project.unwrap_or("").to_lowercase().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Goal lifetime: derived from priority — high goals go stale fast so the
/// agent re-checks them, low goals persist.
const EXPIRY_HOURS: fn(Priority) -> i64 = |p| match p {
    Priority::High => 12,
    Priority::Medium => 36,
    Priority::Low => 72,
};

/// Deterministic timestamp: second-precision ISO (no millis — the same
/// second must yield identical output; millis would only jitter ids we
/// do NOT derive from time anyway, but comparisons stay stable this way).
fn now_iso_deterministic() -> String {
    let secs = chrono::Utc::now().timestamp();
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

fn expiry_from(now: &str, hours: i64) -> String {
    chrono::DateTime::parse_from_rfc3339(now)
        .map(|d| {
            d.checked_add_signed(chrono::Duration::hours(hours))
                .map(|x| x.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
        .unwrap_or(None)
        .unwrap_or_else(|| now.to_string())
}

// ─────────────────────────── lifecycle ───────────────────────────

impl GoalCandidate {
    /// Apply time-based expiry. Returns true when status changed.
    pub fn expire_if_due(&mut self, now: &str) -> bool {
        if self.status != GoalStatus::Candidate {
            return false;
        }
        if now >= self.expires_at.as_str() {
            self.status = GoalStatus::Expired;
            return true;
        }
        false
    }

    /// Mark completed/cancelled/accepted (user or higher-phase actions).
    pub fn complete(&mut self) {
        self.status = GoalStatus::Completed;
    }
    pub fn cancel(&mut self) {
        self.status = GoalStatus::Cancelled;
    }
    pub fn accept(&mut self) {
        if self.status == GoalStatus::Candidate {
            self.status = GoalStatus::Accepted;
        }
    }

    /// A stale candidate: accepted long ago but never acted on — the
    /// evidence window has moved past its expiry.
    pub fn is_stale(&self, now: &str) -> bool {
        matches!(self.status, GoalStatus::Accepted | GoalStatus::Candidate)
            && now >= self.expires_at.as_str()
    }
}

/// Detect a conflict between two candidates: same project, opposite intent.
/// Deterministic heuristic — "X" vs "stop/undo/revert X" patterns.
fn conflicts(a: &GoalCandidate, b: &GoalCandidate) -> bool {
    if a.goal_id == b.goal_id {
        return false; // duplicates merge; they don't conflict
    }
    if a.project != b.project {
        return false;
    }
    let is_negation = |t: &str| {
        let t = t.trim().to_lowercase();
        // Goal titles are prefixed ("Complete: …"), so look for the negation
        // anywhere at a word start, not only at position 0.
        t.starts_with("stop ") || t.starts_with("undo ") || t.starts_with("revert ")
            || t.contains(" stop ") || t.contains(" undo ") || t.contains(" revert ")
    };
    let a_neg = is_negation(&a.title);
    let b_neg = is_negation(&b.title);
    if a_neg == b_neg {
        return false; // conflict needs exactly one negation
    }
    let strip_negation = |t: &str| -> String {
        let mut s = t.trim().to_lowercase();
        // Strip both the goal prefix and any negation verb, iteratively
        // (handles "Complete: Stop refactor X" shapes).
        loop {
            let before = s.clone();
            for p in ["stop ", "undo ", "revert ", "complete: "] {
                if let Some(rest) = s.strip_prefix(p) {
                    s = rest.trim().to_string();
                }
                if s.contains(&format!(" {p}")) {
                    s = s.replacen(&format!(" {p}"), " ", 1);
                }
            }
            if s == before {
                break;
            }
        }
        s.trim().to_string()
    };
    let (neg, plain) = if a_neg { (&a.title, &b.title) } else { (&b.title, &a.title) };
    strip_negation(neg) == strip_negation(plain)
}

// ─────────────────────────── evidence extraction ───────────────────────────

/// All raw evidence rows pulled from existing tables.
struct RawEvidence {
    title: String,
    project: Option<String>,
    evidence: Evidence,
    priority_hint: Priority,
}

/// Extract goal-shaped evidence from the DB. Read-only.
///
/// Sources (existing infrastructure, nothing invented):
///   1. todos         → one candidate per OPEN todo (Task evidence)
///   2. captured errors (chats source='capture' tags='error') → Error evidence
///   3. chat_resume_points → the freshest intent becomes a Resume candidate
///   4. Project Brain → per-project "continue where you left off"
fn extract_evidence(conn: &rusqlite::Connection) -> Result<Vec<RawEvidence>, String> {
    let mut out: Vec<RawEvidence> = Vec::new();

    // 1. Open todos (priority order).
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, priority FROM todos
                 WHERE completed = 0
                 ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,
                          id ASC LIMIT 40",
            )
            .map_err(|e| format!("goals: todos: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                let id: i64 = r.get(0)?;
                let title: String = r.get(1)?;
                let prio: String = r.get(2)?;
                Ok((id, title, prio))
            })
            .map_err(|e| format!("goals: todos: {e}"))?;
        for row in rows {
            let (id, title, prio) = row.map_err(|e| e.to_string())?;
            let priority_hint = match prio.as_str() {
                "high" => Priority::High,
                "medium" => Priority::Medium,
                _ => Priority::Low,
            };
            out.push(RawEvidence {
                title: format!("Complete: {title}"),
                project: None,
                evidence: Evidence {
                    kind: EvidenceKind::Task,
                    reference: id.to_string(),
                    summary: title.clone(),
                    weight: 1.0,
                },
                priority_hint,
            });
        }
    }

    // 2. Captured errors → "fix this" candidates.
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, title FROM chats
                 WHERE source='capture' AND tags='error'
                 ORDER BY created_at DESC LIMIT 10",
            )
            .map_err(|e| format!("goals: errors: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| format!("goals: errors: {e}"))?;
        for row in rows {
            let (id, title) = row.map_err(|e| e.to_string())?;
            let short: String = title.chars().take(60).collect();
            out.push(RawEvidence {
                title: format!("Fix error: {short}"),
                project: None,
                evidence: Evidence {
                    kind: EvidenceKind::Error,
                    reference: id,
                    summary: short,
                    weight: 1.2, // errors weigh more than ordinary tasks
                },
                priority_hint: Priority::High,
            });
        }
    }

    // 3. Freshest resume intent.
    {
        let intent: Option<String> = conn
            .query_row(
                "SELECT intent FROM chat_resume_points ORDER BY updated_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();
        if let Some(intent) = intent {
            let short: String = intent.chars().take(70).collect();
            out.push(RawEvidence {
                title: format!("Continue: {short}"),
                project: None,
                evidence: Evidence {
                    kind: EvidenceKind::Resume,
                    reference: "latest".to_string(),
                    summary: short,
                    weight: 0.9,
                },
                priority_hint: Priority::Medium,
            });
        }
    }

    // 4. Project Brain — per open-project continuation goals.
    let brain = crate::project::brain::snapshot(conn)?;
    for p in brain.projects.iter().take(10) {
        if p.open_tasks.is_empty() && p.recent_errors.is_empty() {
            continue; // nothing actionable → no invented intent
        }
        out.push(RawEvidence {
            title: format!("Continue project: {}", p.name),
            project: Some(p.name.clone()),
            evidence: Evidence {
                kind: EvidenceKind::Project,
                reference: p.path.clone(),
                summary: p.next_likely_action.clone(),
                weight: 1.1,
            },
            priority_hint: Priority::Medium,
        });
    }

    Ok(out)
}

/// Phantom field trick removed: keep RawEvidence honest.
// (The struct above uses a dummy field only under cfg(test) — see below.)

// ─────────────────────────── assembly ───────────────────────────

/// Generate deterministic goal candidates from the existing DB.
/// Read-only; the caller decides what to do with them (Phase 8+).
pub fn generate(conn: &rusqlite::Connection) -> Result<Vec<GoalCandidate>, String> {
    let raw = extract_evidence(conn)?;
    let now = now_iso_deterministic();

    // Group by (normalized title, project) — duplicates merge here.
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<(String, Option<String>), Vec<RawEvidence>> = BTreeMap::new();
    for r in raw {
        let key = (r.title.to_lowercase(), r.project.clone());
        groups.entry(key).or_default().push(r);
    }

    let mut candidates: Vec<GoalCandidate> = Vec::new();
    for ((_norm_title, project), mut group) in groups {
        // Deterministic ordering inside a group: by reference string.
        group.sort_by(|a, b| a.evidence.reference.cmp(&b.evidence.reference));

        let primary = &group[0];
        let title = primary.title.clone();
        let mut evidence: Vec<Evidence> = group.iter().map(|g| g.evidence.clone()).collect();

        // Merged duplicate evidence: keep one row per (kind, reference);
        // extra identical evidence folds into weight growth.
        let mut folded: Vec<Evidence> = Vec::new();
        let mut extra_weight: f32 = 0.0;
        for ev in evidence.drain(..) {
            if let Some(existing) = folded
                .iter_mut()
                .find(|f| f.kind == ev.kind && f.reference == ev.reference)
            {
                extra_weight += ev.weight * 0.25; // duplicates boost confidence
            } else {
                folded.push(ev);
            }
        }

        // Priority: max across hints (errors push their group to High).
        let priority = group
            .iter()
            .map(|g| g.priority_hint)
            .max()
            .unwrap_or(Priority::Low);

        // Confidence: base per distinct evidence + weights + dup boost,
        // deterministically capped at 0.95 (never claim certainty).
        let mut conf = 0.25 * folded.len() as f32
            + folded.iter().map(|e| e.weight - 1.0).sum::<f32>() * 0.1
            + extra_weight * 0.2;
        if priority == Priority::High {
            conf += 0.05;
        }
        let confidence = conf.clamp(0.05, 0.95);

        let hours = EXPIRY_HOURS(priority);
        let created_at = now.clone();
        let expires_at = expiry_from(&created_at, hours);

        candidates.push(GoalCandidate {
            goal_id: GoalId::new(goal_hash(&title, project.as_deref())),
            title,
            description: primary
                .evidence
                .summary
                .clone(),
            project,
            priority,
            confidence: (confidence * 100.0).round() / 100.0,
            evidence: folded,
            status: GoalStatus::Candidate,
            created_at,
            expires_at,
        });
    }

    // Conflict resolution: when a goal conflicts with a negation goal,
    // the negation (user's latest expressed intent) wins; the other is
    // cancelled with evidence kept for explainability.
    let mut i = 0;
    while i < candidates.len() {
        let mut j = i + 1;
        while j < candidates.len() {
            if conflicts(&candidates[i], &candidates[j]) {
                let neg_first = candidates[i]
                    .title
                    .to_lowercase()
                    .starts_with("stop ");
                if neg_first {
                    candidates[j].cancel();
                } else {
                    candidates[i].cancel();
                }
            }
            j += 1;
        }
        i += 1;
    }

    // Final deterministic order: priority desc, confidence desc, id asc.
    candidates.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.goal_id.raw().cmp(&b.goal_id.raw()))
    });

    Ok(candidates)
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

    fn seed_error(conn: &rusqlite::Connection, title: &str) {
        conn.execute(
            "INSERT INTO roots(id,name,created_at,updated_at) VALUES('rt','R','2026-09-03T00:00:00Z','2026-09-03T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n1','rt',NULL,'chat','C',0,'2026-09-03T00:00:00Z','2026-09-03T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chats(id,node_id,title,source,raw_path,tags,brief_text,created_at,updated_at)
             VALUES(?1,'n1',?2,'capture','/x','error','b','2026-09-03T10:00:00Z','2026-09-03T10:00:00Z')",
            rusqlite::params![format!("chat-{title}"), title],
        )
        .unwrap();
    }

    #[test]
    fn empty_world_yields_no_goals() {
        let conn = setup();
        let goals = generate(&conn).unwrap();
        assert!(goals.is_empty(), "no evidence ⇒ no invented intent");
    }

    #[test]
    fn task_derives_goal_with_matching_priority() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('ship the parser','high',0)",
            [],
        )
        .unwrap();
        let goals = generate(&conn).unwrap();
        assert_eq!(goals.len(), 1);
        assert!(goals[0].title.contains("ship the parser"));
        assert_eq!(goals[0].priority, Priority::High);
        assert_eq!(goals[0].evidence[0].kind, EvidenceKind::Task);
        assert!(goals[0].confidence > 0.0);
    }

    #[test]
    fn error_derives_high_priority_goal() {
        let conn = setup();
        seed_error(&conn, "E0308 mismatched types");
        let goals = generate(&conn).unwrap();
        assert_eq!(goals.len(), 1);
        assert!(goals[0].title.starts_with("Fix error:"));
        assert_eq!(goals[0].priority, Priority::High);
        assert_eq!(goals[0].evidence[0].kind, EvidenceKind::Error);
    }

    #[test]
    fn project_derives_continue_goal_only_when_actionable() {
        let conn = setup();
        // A workspace item creates a project; an error tied to its name makes
        // it actionable via todos match.
        conn.execute(
            "INSERT INTO workspace_sessions(id,name,created_at,status) VALUES('s1','s',1,'frozen')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_items(id,session_id,item_type,action_target,created_at,restore_status)
             VALUES('i1','s1','vscode','/home/u/alpha',?1,'pending')",
            [1_800_000_000i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('alpha: finish refactor','high',0)",
            [],
        )
        .unwrap();
        let goals = generate(&conn).unwrap();
        // Both the task goal and the project-continue goal must exist.
        assert!(goals.iter().any(|g| g.title.contains("Continue project: alpha")));
        assert!(goals.iter().any(|g| g.title.contains("alpha: finish refactor")));
    }

    #[test]
    fn duplicate_goals_merge_into_one_with_boosted_confidence() {
        let conn = setup();
        // Two DIFFERENT todos with the same normalized title merge.
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('Fix tests','medium',0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('fix TESTS','low',0)",
            [],
        )
        .unwrap();
        let goals = generate(&conn).unwrap();
        let merged: Vec<_> = goals.iter().filter(|g| g.title.to_lowercase().contains("fix tests")).collect();
        assert_eq!(merged.len(), 1, "case-insensitive titles must merge");
        assert!(merged[0].evidence.len() == 2, "both todo refs kept as evidence");
        assert!(merged[0].confidence >= 0.25 * 2.0 - 0.01);
    }

    #[test]
    fn conflicting_negation_cancels_the_opposite() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('Refactor the parser','high',0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('Stop refactor the parser','high',0)",
            [],
        )
        .unwrap();
        let goals = generate(&conn).unwrap();
        let refactor = goals
            .iter()
            .find(|g| g.title == "Complete: Refactor the parser")
            .expect("original goal exists");
        // "Stop …" negation cancels the plain one (same normalized core).
        assert_eq!(refactor.status, GoalStatus::Cancelled, "negation must win");
    }

    #[test]
    fn confidence_is_deterministic_and_capped() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('deterministic goal','medium',0)",
            [],
        )
        .unwrap();
        let a = generate(&conn).unwrap();
        let b = generate(&conn).unwrap();
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0].confidence, b[0].confidence);
        assert!(a[0].confidence <= 0.95);
        assert!(a[0].confidence >= 0.05);
    }

    #[test]
    fn goal_ids_are_content_stable() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('stable id check','low',0)",
            [],
        )
        .unwrap();
        let a = generate(&conn).unwrap();
        let b = generate(&conn).unwrap();
        assert_eq!(a[0].goal_id, b[0].goal_id);
    }

    #[test]
    fn expiry_matches_priority_and_expires_candidates() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('high goes stale fast','high',0)",
            [],
        )
        .unwrap();
        let mut g = generate(&conn).unwrap().remove(0);
        assert_eq!(g.priority, Priority::High);
        // Freshly created: not stale.
        assert!(!g.is_stale(&g.created_at));
        // Simulate time passing past expiry.
        let future = expiry_from(&g.created_at, EXPIRY_HOURS(Priority::High) + 1);
        assert!(g.is_stale(&future));
        assert!(g.expire_if_due(&future));
        assert_eq!(g.status, GoalStatus::Expired);
    }

    #[test]
    fn stale_accepted_goal_is_reported_stale() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('accepted but forgotten','medium',0)",
            [],
        )
        .unwrap();
        let mut g = generate(&conn).unwrap().remove(0);
        g.accept();
        assert_eq!(g.status, GoalStatus::Accepted);
        let future = expiry_from(&g.created_at, EXPIRY_HOURS(Priority::Medium) + 1);
        assert!(g.is_stale(&future));
    }

    #[test]
    fn repeatability_identical_outputs_within_the_same_second() {
        let conn = setup();
        seed_error(&conn, "E0308 mismatched types");
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('repeat me','medium',0)",
            [],
        )
        .unwrap();
        let a = generate(&conn).unwrap();
        let b = generate(&conn).unwrap();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.goal_id, y.goal_id);
            assert_eq!(x.title, y.title);
            assert_eq!(x.confidence, y.confidence);
            assert_eq!(x.evidence, y.evidence);
        }
    }

    #[test]
    fn completed_todo_yields_no_goal() {
        let conn = setup();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('already done','high',1)",
            [],
        )
        .unwrap();
        assert!(generate(&conn).unwrap().is_empty());
    }

    #[test]
    fn resume_intent_becomes_continue_goal() {
        let conn = setup();
        conn.execute(
            "INSERT INTO chat_resume_points(id,chat_id,intent,open_items,context_refs,created_at,updated_at)
             VALUES('r1',NULL,'make strawberry production ready','[]','[]','2026-09-03T11:00:00Z','2026-09-03T11:00:00Z')",
            [],
        )
        .unwrap();
        let goals = generate(&conn).unwrap();
        assert!(goals
            .iter()
            .any(|g| g.title.contains("Continue: make strawberry production ready")));
    }
}
