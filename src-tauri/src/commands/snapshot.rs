//! 🧠 Context Recall commands — one-click workspace snapshot + recall.

use std::sync::Arc;

use crate::snapshot::{self, WorkSnapshot};
use crate::state::AppState;

use super::Cmd;

/// One click: capture windows + tabs + recent web + clipboard,
/// generate the story, persist it.
#[tauri::command]
pub async fn capture_work_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Cmd<WorkSnapshot> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let snap = snapshot::collect(&conn);
        snapshot::save(&conn, &snap)?;
        Ok(snap)
    })
    .await
}

/// "Load previous work" — latest snapshot with its story.
#[tauri::command]
pub async fn get_latest_work_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Cmd<Option<WorkSnapshot>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        snapshot::latest(&conn)
    })
    .await
}

/// History strip of past snapshots (id, timestamp, active app).
#[tauri::command]
pub async fn list_work_snapshots(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<u32>,
) -> Cmd<Vec<(String, String, Option<String>)>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        snapshot::list_recent(&conn, limit.unwrap_or(10).max(1) as usize)
    })
    .await
}
