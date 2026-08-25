//! 🌐 Tab commands — record (extension-lite) + list groups + topic search.

use std::sync::Arc;

use crate::state::AppState;
use crate::tabs::{self, TabGroup, TabVisit};

use super::Cmd;

#[tauri::command]
pub async fn record_tab_visit(
    state: tauri::State<'_, Arc<AppState>>,
    url: String,
    title: Option<String>,
) -> Cmd<()> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        tabs::record(
            &conn,
            &TabVisit {
                url,
                title,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn list_tab_groups(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<u32>,
) -> Cmd<Vec<TabGroup>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        tabs::recent_groups(&conn, limit.unwrap_or(10).max(1) as usize)
    })
    .await
}

#[tauri::command]
pub async fn find_tabs_for_topic(
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
    limit: Option<u32>,
) -> Cmd<Vec<(String, String)>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        tabs::find_for_topic(&conn, &query, limit.unwrap_or(10).max(1) as usize)
    })
    .await
}
