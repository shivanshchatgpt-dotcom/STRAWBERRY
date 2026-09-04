//! 🌳 Project Brain commands — Phase C/D/E exposed to the frontend.
//! All read-only aggregations; see `src/project/` for the engines.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::project::brain::{self, ProjectBrainSnapshot};
use crate::project::changed::{self, ChangeSet};
use crate::project::resume_narrative::{self, ResumeNarrative};
use crate::state::AppState;

use super::{blocking, Cmd};

#[tauri::command]
pub async fn get_project_brain(
    state: State<'_, Arc<AppState>>,
) -> Cmd<ProjectBrainSnapshot> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        brain::snapshot(&conn)
    })
    .await
}

#[tauri::command]
pub async fn get_what_changed(state: State<'_, Arc<AppState>>) -> Cmd<ChangeSet> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        changed::what_changed(&conn)
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeBundle {
    pub narrative: ResumeNarrative,
    pub changes: ChangeSet,
}

#[tauri::command]
pub async fn get_intelligent_resume(
    state: State<'_, Arc<AppState>>,
) -> Cmd<ResumeBundle> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let narrative = resume_narrative::narrative(&conn)?;
        let changes = changed::what_changed(&conn)?;
        Ok(ResumeBundle { narrative, changes })
    })
    .await
}
