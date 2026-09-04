//! 🌳 Project Brain — Phase C of the Strawberry platform.
//!
//! Pure AGGREGATION over existing storage. No new tables, no new writers.
//! Every query here reads tables another module already owns:
//!
//!   Identity        → workspace_items (vscode action targets, terminal cwds)
//!   Open tasks      → todos
//!   Errors          → chats WHERE source='capture' AND tags='error'
//!   Decisions       → chat_resume_points (intent lines)
//!   Recent activity → ghost_events, ambient_events
//!   Last session    → workspace_sessions / workspace_items
//!   Likely next     → heuristic from the above
//!
//! The module is deterministic, offline and side-effect free: it can never
//! corrupt user data because it only ever holds a read lock.

use serde::{Deserialize, Serialize};

/// One project Strawberry knows about.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// Canonical absolute path when known (VS Code target / terminal cwd).
    pub path: String,
    /// Last path segment, e.g. "chat-memory-tree".
    pub name: String,
    /// Where we discovered it: "vscode" | "terminal" | "both".
    pub origin: String,
    /// Unix seconds of the most recent signal mentioning this project.
    pub last_seen_at: i64,
    /// Open todo titles (max 5, priority order).
    pub open_tasks: Vec<String>,
    /// Done todo count (all time).
    pub tasks_done: i64,
    /// Captured error snippets whose title mentions the project name.
    pub recent_errors: Vec<String>,
    /// Resume-point intents recorded while working in this project.
    pub decisions: Vec<String>,
    /// Ghost event types + counts (last 30 days) touching this project.
    pub activity: Vec<(String, i64)>,
    /// Best-effort "what you'll probably do next".
    pub next_likely_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBrainSnapshot {
    pub projects: Vec<ProjectSummary>,
}

/// Discover projects from frozen workspace items.
///
/// Two signals per session row:
///   • `item_type='vscode'`   → action_target holds the folder opened in VS Code
///   • `item_type='terminal'` → cwd holds the shell working directory
///
/// Both are absolute paths. We normalise (dedup trailing slashes) and keep
/// the most recent `last_seen`.
fn discover_projects(conn: &rusqlite::Connection) -> Result<Vec<(String, String, i64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT coalesce(action_target,''), coalesce(cwd,''), item_type,
                    coalesce(created_at, 0)
             FROM workspace_items
             WHERE (item_type='vscode' AND coalesce(action_target,'') != '')
                OR (item_type='terminal' AND coalesce(cwd,'') != '')
             ORDER BY created_at ASC",
        )
        .map_err(|e| format!("project discovery: {e}"))?;

    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| format!("project discovery: {e}"))?;

    // path → (origin set, last_seen)
    let mut map: std::collections::HashMap<String, (std::collections::HashSet<String>, i64)> =
        std::collections::HashMap::new();

    for row in rows {
        let (target, cwd, item_type, at) = row.map_err(|e| e.to_string())?;
        let (path, origin) = if item_type == "vscode" {
            (target, "vscode")
        } else {
            (cwd, "terminal")
        };
        let clean = path.trim_end_matches('/').to_string();
        if clean.is_empty() {
            continue;
        }
        let e = map.entry(clean).or_insert_with(|| (std::collections::HashSet::new(), 0));
        e.0.insert(origin.to_string());
        if at > e.1 {
            e.1 = at;
        }
    }

    let mut out: Vec<(String, String, i64)> = map
        .into_iter()
        .map(|(path, (origins, last))| {
            let origin = if origins.len() > 1 {
                "both".to_string()
            } else {
                origins.into_iter().next().unwrap_or_default()
            };
            (path, origin, last)
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2));
    Ok(out)
}

/// Build the full brain snapshot. Read-only; safe to call from any thread
/// while holding the AppState connection lock.
pub fn snapshot(conn: &rusqlite::Connection) -> Result<ProjectBrainSnapshot, String> {
    let discovered = discover_projects(conn)?;
    let mut projects = Vec::with_capacity(discovered.len());

    for (path, origin, last_seen) in discovered {
        let name = path
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&path)
            .to_string();

        // Open todos (priority order, max 5) — todos have no project column,
        // so we match by title mentioning the project name.
        let like = format!("%{}%", name);
        let open_tasks: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT title FROM todos
                     WHERE completed=0 AND (title LIKE ?1 OR coalesce(description,'') LIKE ?1)
                     ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END
                     LIMIT 5",
                )
                .map_err(|e| format!("open tasks: {e}"))?;
            let rows = stmt
                .query_map([&like], |r| r.get::<_, String>(0))
                .map_err(|e| format!("open tasks: {e}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };

        // Tasks completed (all time, same title match).
        let tasks_done: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM todos
                 WHERE completed=1 AND (title LIKE ?1 OR coalesce(description,'') LIKE ?1)",
                [&like],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // Captured errors mentioning the project (max 3).
        let recent_errors: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT title FROM chats
                     WHERE source='capture' AND tags='error' AND title LIKE ?1
                     ORDER BY created_at DESC LIMIT 3",
                )
                .map_err(|e| format!("errors: {e}"))?;
            let rows = stmt
                .query_map([&like], |r| r.get::<_, String>(0))
                .map_err(|e| format!("errors: {e}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };

        // Decisions = resume intents mentioning the project (max 3).
        let decisions: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT intent FROM chat_resume_points
                     WHERE intent LIKE ?1 ORDER BY updated_at DESC LIMIT 3",
                )
                .map_err(|e| format!("decisions: {e}"))?;
            let rows = stmt
                .query_map([&like], |r| r.get::<_, String>(0))
                .map_err(|e| format!("decisions: {e}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };

        // Ghost activity touching the project (type + count, max 4 kinds).
        let activity: Vec<(String, i64)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT event_type, COUNT(*) FROM ghost_events
                     WHERE coalesce(metadata,'') LIKE ?1 OR coalesce(source_id,'') LIKE ?1
                     GROUP BY event_type ORDER BY 2 DESC LIMIT 4",
                )
                .map_err(|e| format!("activity: {e}"))?;
            let rows = stmt
                .query_map([&like], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                .map_err(|e| format!("activity: {e}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };

        // Next likely action — deterministic heuristic, no LLM.
        let next_likely_action = if !open_tasks.is_empty() {
            format!("Finish open task: “{}”", open_tasks[0])
        } else if !recent_errors.is_empty() {
            format!("Fix the last captured error ({})", name)
        } else {
            format!("Open {name} and continue where you left off")
        };

        projects.push(ProjectSummary {
            path,
            name,
            origin,
            last_seen_at: last_seen,
            open_tasks,
            tasks_done,
            recent_errors,
            decisions,
            activity,
            next_likely_action,
        });
    }

    // Cap the snapshot at the 20 most recently active projects.
    projects.truncate(20);
    Ok(ProjectBrainSnapshot { projects })
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

    fn secs(days_ago: i64) -> i64 {
        1_800_000_000 - days_ago * 86_400
    }

    #[test]
    fn discovers_projects_from_vscode_and_terminal_items() {
        let conn = setup();
        conn.execute(
            "INSERT INTO workspace_sessions(id,name,created_at,status) VALUES('s1','sess',1000,'frozen')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_items(id,session_id,item_type,action_target,cwd,created_at,restore_status)
             VALUES('i1','s1','vscode','/mnt/storage/STRAWBERRY/chat-memory-tree','',?1,'pending')",
            [secs(0)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_items(id,session_id,item_type,action_target,cwd,created_at,restore_status)
             VALUES('i2','s1','vscode','/mnt/storage/STRAWBERRY/chat-memory-tree/','',?1,'pending')",
            [secs(5)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_items(id,session_id,item_type,action_target,cwd,created_at,restore_status)
             VALUES('i3','s1','terminal','','/mnt/storage/STRAWBERRY/chat-memory-tree',?1,'pending')",
            [secs(2)],
        )
        .unwrap();

        let snap = snapshot(&conn).unwrap();
        assert_eq!(snap.projects.len(), 1, "trailing slash must dedup");
        let p = &snap.projects[0];
        assert_eq!(p.name, "chat-memory-tree");
        assert_eq!(p.origin, "both", "vscode + terminal both seen");
        assert_eq!(p.last_seen_at, secs(0));
    }

    #[test]
    fn aggregates_tasks_errors_decisions_per_project() {
        let conn = setup();
        conn.execute(
            "INSERT INTO workspace_sessions(id,name,created_at,status) VALUES('s1','sess',1000,'frozen')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_items(id,session_id,item_type,action_target,created_at,restore_status)
             VALUES('i1','s1','vscode','/home/u/alpha',?1,'pending')",
            [secs(0)],
        )
        .unwrap();

        // Two open tasks + one done for alpha.
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('alpha: fix parser','high',0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('alpha: write docs','low',0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('alpha: done thing','low',1)",
            [],
        )
        .unwrap();
        // Unrelated task must not leak in.
        conn.execute(
            "INSERT INTO todos(title,priority,completed) VALUES('unrelated beta task','high',0)",
            [],
        )
        .unwrap();

        // Captured error mentioning alpha (root + node + chat, FK-safe).
        conn.execute(
            "INSERT INTO roots(id,name,created_at,updated_at) VALUES('rt','R',?1,?1)",
            ["2026-09-03T09:00:00Z"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n1','rt',NULL,'chat','C',0,?1,?1)",
            ["2026-09-03T09:00:00Z"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chats(id,node_id,title,source,raw_path,tags,brief_text,created_at,updated_at)
             VALUES('c1','n1','E0308 in alpha parser','capture','/x','error','boom',?1,?1)",
            ["2026-09-03T10:00:00Z"],
        )
        .unwrap();

        // Resume decision mentioning alpha.
        conn.execute(
            "INSERT INTO chat_resume_points(id,chat_id,intent,open_items,context_refs,created_at,updated_at)
             VALUES('r1',NULL,'ship alpha v2','[]','[]',?1,?1)",
            ["2026-09-03T11:00:00Z"],
        )
        .unwrap();

        let snap = snapshot(&conn).unwrap();
        let p = snap.projects.iter().find(|p| p.name == "alpha").unwrap();
        assert_eq!(p.open_tasks.len(), 2);
        assert_eq!(p.tasks_done, 1);
        assert_eq!(p.recent_errors.len(), 1);
        assert_eq!(p.decisions.len(), 1);
        assert!(p.next_likely_action.contains("fix parser"));
    }

    #[test]
    fn empty_db_yields_empty_snapshot() {
        let conn = setup();
        let snap = snapshot(&conn).unwrap();
        assert!(snap.projects.is_empty());
    }
}
