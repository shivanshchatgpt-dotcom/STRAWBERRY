//! 🗄️ Database overview — live counts + recent captures.
//! One command powering the "Database" left-panel view so the user can
//! SEE everything the capture-daemon and the app have saved.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;
use super::{blocking, Cmd};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbOverview {
    pub roots: i64,
    pub folders: i64,
    pub chats: i64,
    pub captures: i64,
    pub capture_notes: i64,
    pub capture_code: i64,
    pub capture_errors: i64,
    pub capture_urls: i64,
    pub todos_open: i64,
    pub todos_done: i64,
    pub habits: i64,
    pub events: i64,
    pub alpha_candidates: i64,
    pub insights: i64,
    pub db_size_bytes: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCapture {
    pub chat_id: String,
    pub title: String,
    pub kind: String,
    pub created_at: String,
}

/// Envelope matching the frontend's `DbOverviewData` shape exactly:
/// `{ overview: {...}, recent: [...] }`. A bare tuple would serialize as a
/// JSON array and the frontend's `data.overview` access would be undefined,
/// crashing the Database view (blank screen).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbOverviewData {
    pub overview: DbOverview,
    pub recent: Vec<RecentCapture>,
}

#[tauri::command]
pub async fn get_db_overview(
    state: State<'_, Arc<AppState>>,
) -> Cmd<DbOverviewData> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;

        let count = |sql: &str| -> Result<i64, String> {
            conn.query_row(sql, [], |r| r.get(0))
                .map_err(|e| e.to_string())
        };

        let overview = DbOverview {
            roots: count("SELECT COUNT(*) FROM roots")?,
            folders: count("SELECT COUNT(*) FROM nodes WHERE type='folder'")?,
            chats: count("SELECT COUNT(*) FROM chats")?,
            captures: count("SELECT COUNT(*) FROM chats WHERE source='capture'")?,
            capture_notes: count(
                "SELECT COUNT(*) FROM chats WHERE source='capture' AND tags='note'",
            )?,
            capture_code: count(
                "SELECT COUNT(*) FROM chats WHERE source='capture' AND tags='code'",
            )?,
            capture_errors: count(
                "SELECT COUNT(*) FROM chats WHERE source='capture' AND tags='error'",
            )?,
            capture_urls: count(
                "SELECT COUNT(*) FROM chats WHERE source='capture' AND tags='url'",
            )?,
            todos_open: count("SELECT COUNT(*) FROM todos WHERE completed=0")?,
            todos_done: count("SELECT COUNT(*) FROM todos WHERE completed=1")?,
            habits: count("SELECT COUNT(*) FROM habits")?,
            events: count("SELECT COUNT(*) FROM events")?,
            alpha_candidates: count("SELECT COUNT(*) FROM alpha_candidates")?,
            insights: count("SELECT COUNT(*) FROM ghost_insights")?,
            db_size_bytes: {
                let p = app.db_path();
                std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(0)
            },
        };

        let mut stmt = conn
            .prepare(
                "SELECT id, title, coalesce(tags,''), created_at
                 FROM chats WHERE source='capture'
                 ORDER BY created_at DESC LIMIT 20",
            )
            .map_err(|e| e.to_string())?;
        let recent = stmt
            .query_map([], |r| {
                Ok(RecentCapture {
                    chat_id: r.get(0)?,
                    title: r.get(1)?,
                    kind: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(DbOverviewData { overview, recent })
    })
    .await
}
