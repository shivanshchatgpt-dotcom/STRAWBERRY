//! ⏮️ Intelligent Resume — Phase D of the Strawberry platform.
//!
//! Reconstructs "what was I doing" from existing signals and produces a
//! narrative + actionable resume plan. Deterministic, no LLM:
//!
//!   last frozen session  → when/where you left
//!   open todos          → what was pending
//!   resume intents     → what you were trying to achieve
//!   captured errors     → what was blocking you
//!   habit completions   → routine state
//!   day summary chats   → the thread you were pulling
//!
//! Output: a short Hinglish-flavoured narrative (matching the rest of the
//! app's tone) plus an ordered resume plan.

use serde::{Deserialize, Serialize};

use super::brain;
use super::changed;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeNarrative {
    /// Human summary, e.g. "Kal 11:42 PM par tum alpha project ke parser
    /// ko debug kar rahe the. Sabse pehle: E0308 wala error clear karo."
    pub headline: String,
    /// When the diff baseline was.
    pub since: String,
    /// What changed while you were away (summary line from Phase E).
    pub changed_summary: String,
    /// Ordered next steps (max 5).
    pub plan: Vec<String>,
    /// Project most likely being worked on (best-effort).
    pub focus_project: Option<String>,
}

/// Build the resume narrative. Read-only aggregation.
pub fn narrative(conn: &rusqlite::Connection) -> Result<ResumeNarrative, String> {
    let changes = changed::what_changed(conn)?;
    let brain_snap = brain::snapshot(conn)?;

    // ---- focus project: most recently seen with open work ----
    let focus_project = brain_snap
        .projects
        .iter()
        .find(|p| !p.open_tasks.is_empty() || !p.recent_errors.is_empty())
        .or_else(|| brain_snap.projects.first())
        .map(|p| p.name.clone());

    // ---- top intent (most recent resume point) ----
    let top_intent: Option<String> = conn
        .query_row(
            "SELECT intent FROM chat_resume_points ORDER BY updated_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    // ---- last captured error (the usual blocker) ----
    let last_error: Option<String> = conn
        .query_row(
            "SELECT title FROM chats WHERE source='capture' AND tags='error'
             ORDER BY created_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    // ---- open tasks, priority order ----
    let open_tasks: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT title FROM todos WHERE completed=0
                 ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END
                 LIMIT 5",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    // ---- last chat title (the thread you were pulling) ----
    let last_chat: Option<String> = conn
        .query_row(
            "SELECT title FROM chats WHERE source != 'capture'
             ORDER BY updated_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    // ---- headline assembly (deterministic template) ----
    let since_local = changes
        .since
        .get(..16)
        .map(|s| s.replace('T', " "))
        .unwrap_or_else(|| changes.since.clone());

    let mut headline = String::new();
    if let Some(name) = &focus_project {
        headline.push_str(&format!(
            "You were working on **{name}** (last seen {since_local}). "
        ));
    } else {
        headline.push_str(&format!("Last activity {since_local}. "));
    }
    if let Some(err) = &last_error {
        let short: String = err.chars().take(60).collect();
        headline.push_str(&format!("Last blocker captured: {short}… "));
    }
    if let Some(intent) = &top_intent {
        let short: String = intent.chars().take(60).collect();
        headline.push_str(&format!("Last goal on record: “{short}”."));
    }
    if headline.is_empty() {
        headline = "Fresh start — koi pending kaam nahi mila.".to_string();
    }

    // ---- resume plan (ordered heuristics) ----
    let mut plan: Vec<String> = Vec::new();
    if let Some(err) = &last_error {
        plan.push(format!("🩹 Clear the last captured error: {}", truncate(err, 70)));
    }
    for t in open_tasks.iter().take(3) {
        plan.push(format!("📋 Open task: {}", truncate(t, 70)));
    }
    if let Some(chat) = &last_chat {
        plan.push(format!("💬 Re-read “{}” for context", truncate(chat, 60)));
    }
    if plan.is_empty() {
        plan.push("🌱 Nothing pending — pick a fresh goal from the Dashboard".to_string());
    }
    plan.truncate(5);

    Ok(ResumeNarrative {
        headline,
        since: changes.since.clone(),
        changed_summary: changes.summary.clone(),
        plan,
        focus_project,
    })
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
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
    fn empty_db_gives_fresh_start() {
        let conn = setup();
        let n = narrative(&conn).unwrap();
        assert!(n.headline.contains("Last activity"));
        assert!(n.plan.iter().any(|p| p.contains("Nothing pending")));
        assert!(n.focus_project.is_none());
    }

    #[test]
    fn narrative_mentions_project_error_intent_and_tasks() {
        let conn = setup();

        // Project alpha via a vscode item.
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

        // Open high-priority task.
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('fix alpha parser','high',0)",
            [],
        )
        .unwrap();

        // Captured error.
        conn.execute(
            "INSERT INTO roots(id,name,created_at,updated_at) VALUES('rt','R','2026-09-03T09:00:00Z','2026-09-03T09:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n1','rt',NULL,'chat','C',0,'2026-09-03T09:00:00Z','2026-09-03T09:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chats(id,node_id,title,source,raw_path,tags,brief_text,created_at,updated_at)
             VALUES('c1','n1','E0308 mismatched types in alpha','capture','/x','error','b','2026-09-03T10:00:00Z','2026-09-03T10:00:00Z')",
            [],
        )
        .unwrap();

        // Resume intent.
        conn.execute(
            "INSERT INTO chat_resume_points(id,chat_id,intent,open_items,context_refs,created_at,updated_at)
             VALUES('r1',NULL,'make alpha production ready','[]','[]','2026-09-03T11:00:00Z','2026-09-03T11:00:00Z')",
            [],
        )
        .unwrap();

        let n = narrative(&conn).unwrap();
        assert_eq!(n.focus_project.as_deref(), Some("alpha"));
        assert!(n.headline.contains("alpha"));
        assert!(n.headline.contains("mismatched types"), "error in headline: {}", n.headline);
        assert!(n.headline.contains("production ready"), "intent in headline");
        // Plan: error first, then task, then re-read.
        assert!(n.plan[0].contains("E0308"));
        assert!(n.plan.iter().any(|p| p.contains("fix alpha parser")));
    }
}
