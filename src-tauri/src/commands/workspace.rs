//! 🧊 Freeze & Resume commands.

use std::sync::Arc;

use crate::workspace::{self, RestoreReport, WorkSpace};
use crate::state::AppState;

use super::Cmd;

#[tauri::command]
pub async fn freeze_work_space(state: tauri::State<'_, Arc<AppState>>) -> Cmd<WorkSpace> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let ws = workspace::collect();
        workspace::save(&conn, &ws)?;
        Ok(ws)
    })
    .await
}

#[tauri::command]
pub async fn list_work_spaces(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<u32>,
) -> Cmd<Vec<(String, String, String, String)>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        workspace::list(&conn, limit.unwrap_or(20).max(1) as usize)
    })
    .await
}

#[tauri::command]
pub async fn restore_work_space(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<RestoreReport> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let Some(ws) = workspace::get(&conn, &id)? else {
            return Err(format!("workspace {id} not found"));
        };
        let report = workspace::restore(&ws);
        workspace::mark_restored(&conn, &id)?;
        Ok(report)
    })
    .await
}

#[tauri::command]
pub async fn delete_work_space(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<()> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        workspace::delete(&conn, &id)
    })
    .await
}
