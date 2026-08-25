//! ⏯️ Resume commands for the Tauri frontend.

use std::sync::Arc;

use crate::resume::{self, ResumePoint};
use crate::state::AppState;

use super::Cmd;

#[tauri::command]
pub async fn get_resume_suggestions(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<u32>,
) -> Cmd<Vec<ResumePoint>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        resume::suggestions(&conn, limit.unwrap_or(5).max(1) as usize)
    })
    .await
}

#[tauri::command]
pub async fn save_resume_point(
    state: tauri::State<'_, Arc<AppState>>,
    chat_id: String,
) -> Cmd<ResumePoint> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        resume::save_for_chat(&conn, &chat_id)
    })
    .await
}

#[tauri::command]
pub async fn dismiss_resume_point(
    state: tauri::State<'_, Arc<AppState>>,
    resume_id: String,
) -> Cmd<()> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        app.conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?
            .execute("DELETE FROM chat_resume_points WHERE id=?1", [resume_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn get_day_summary(
    state: tauri::State<'_, Arc<AppState>>,
) -> Cmd<crate::resume::DaySummary> {
    let st = state.inner().clone();
    super::blocking(st, |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        crate::resume::day_summary(&conn)
    })
    .await
}
