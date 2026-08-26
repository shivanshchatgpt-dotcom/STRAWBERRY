//! 📖 Export My Story — buildathon-ready narrative generator.
//! Pulls Strawberry's own data (chats, captures, tasks, resume intents) and
//! optionally a git repo's recent commits into one markdown timeline.
//! 100% local: git via std::process::Command, stats via SQL.

use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::error;
use super::blocking;

type Cmd<T> = Result<T, String>;

fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryStats {
    pub chats_total: i64,
    pub captures_total: i64,
    pub tasks_done: i64,
    pub tasks_open: i64,
    pub habits_tracked: i64,
    pub days_active: i64,
    pub commits: Vec<(String, String)>, // (date, message)
}

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
}

/// Generate the "My Story" markdown. `repo_path` is optional; when it points
/// at a git repository, the last 14 days of commits join the timeline.
#[tauri::command]
pub async fn export_my_story(
    state: State<'_, Arc<AppState>>,
    repo_path: Option<String>,
    days: Option<u32>,
) -> Cmd<String> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = conn_of(app)?;
        let window = days.unwrap_or(14).clamp(1, 90);

        let stats = StoryStats {
            chats_total: count(&conn, "SELECT COUNT(*) FROM chats WHERE source != 'capture'"),
            captures_total: count(&conn, "SELECT COUNT(*) FROM chats WHERE source = 'capture'"),
            tasks_done: count(&conn, "SELECT COUNT(*) FROM todos WHERE completed = 1"),
            tasks_open: count(&conn, "SELECT COUNT(*) FROM todos WHERE completed = 0"),
            habits_tracked: count(&conn, "SELECT COUNT(DISTINCT name) FROM habits"),
            days_active: count(
                &conn,
                "SELECT COUNT(DISTINCT substr(created_at,1,10)) FROM chats",
            ),
            commits: Vec::new(),
        };

        // Optional git log.
        let mut commits: Vec<(String, String)> = Vec::new();
        if let Some(repo) = repo_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            if let Ok(out) = std::process::Command::new("git")
                .args([
                    "-C", repo,
                    "log",
                    "--since", &format!("{window} days ago"),
                    "--pretty=%ad%x1f%s",
                    "--date=short",
                ])
                .output()
            {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    for line in text.lines().take(200) {
                        if let Some((date, msg)) = line.split_once('\u{1f}') {
                            commits.push((date.to_string(), msg.to_string()));
                        }
                    }
                }
            }
        }

        // Per-day activity from chats (any kind).
        let mut activity: Vec<(String, i64)> = Vec::new();
        {
            let mut s = conn
                .prepare(
                    "SELECT substr(created_at,1,10) AS d, COUNT(*)
                     FROM chats
                     WHERE created_at >= date('now', ?1)
                     GROUP BY d ORDER BY d DESC",
                )
                .map_err(error::to_string_err("activity query"))?;
            let rows = s
                .query_map([format!("-{window} days")], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map_err(error::to_string_err("activity map"))?;
            for r in rows.flatten() {
                activity.push(r);
            }
        }

        // Top resume intents (what I was working on).
        let mut intents: Vec<String> = Vec::new();
        {
            let mut s = conn
                .prepare(
                    "SELECT intent FROM chat_resume_points ORDER BY updated_at DESC LIMIT 5",
                )
                .map_err(error::to_string_err("intent query"))?;
            let rows = s
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(error::to_string_err("intent map"))?;
            for r in rows.flatten() {
                intents.push(r);
            }
        }

        // ---- Render markdown ----
        let mut md = String::with_capacity(2048);
        md.push_str("# 🍓 My Story — Strawberry Export\n\n");
        md.push_str(&format!(
            "_Generated locally on {} · window: last {window} days · zero cloud, zero LLM_\n\n",
            chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
        ));

        md.push_str("## Headline numbers\n\n");
        md.push_str(&format!(
            "- 💬 Chats saved: **{}**\n- 📋 Clipboard captures: **{}**\n- ✅ Tasks completed: **{}** ({} still open)\n- 🔥 Habits tracked: **{}**\n- 📅 Active days: **{}**\n",
            stats.chats_total, stats.captures_total, stats.tasks_done, stats.tasks_open, stats.habits_tracked, stats.days_active
        ));

        if !activity.is_empty() {
            md.push_str("\n## Daily activity\n\n| Day | Items |\n|---|---|\n");
            for (d, n) in &activity {
                md.push_str(&format!("| {d} | {n} |\n"));
            }
        }

        if !intents.is_empty() {
            md.push_str("\n## What I was working on (resume intents)\n\n");
            for i in &intents {
                md.push_str(&format!("- {i}\n"));
            }
        }

        if !commits.is_empty() {
            md.push_str("\n## Git timeline (last commits)\n\n");
            let mut cur_date = String::new();
            for (date, msg) in &commits {
                if *date != cur_date {
                    md.push_str(&format!("\n**{date}**\n"));
                    cur_date = date.clone();
                }
                md.push_str(&format!("- {msg}\n"));
            }
        }

        md.push_str("\n---\n_Generated by Strawberry 🍓 — chats, captures, tasks and git history, all from my own machine._\n");

        Ok(md)
    })
    .await
}
