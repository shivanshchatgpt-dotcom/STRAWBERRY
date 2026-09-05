//! 📁 File Watcher Tauri commands
//!
//! Uses a SHARED `FileWatcherRunner` from `AppState`. The runner is
//! polled by a background thread spawned in `lib.rs` that publishes
//! watcher events to the EventBus and into the file→memory indexer.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::autonomous::watcher_runner::FileWatcherRunner;
use crate::commands::Cmd;
use crate::state::AppState;

#[tauri::command]
pub async fn watcher_start(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Cmd<String> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        app.watcher
            .start_watcher(std::path::Path::new(&path))
            .map_err(|e| e.to_string())?;
        Ok(format!("watching {}", path))
    })
    .await
}

#[tauri::command]
pub async fn watcher_stop(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Cmd<String> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        app.watcher
            .stop_watcher(std::path::Path::new(&path))
            .map_err(|e| e.to_string())?;
        Ok(format!("stopped watching {}", path))
    })
    .await
}

#[tauri::command]
pub async fn watcher_list(
    state: tauri::State<'_, Arc<AppState>>,
) -> Cmd<Vec<PathBuf>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| Ok(app.watcher.watched_paths()))
    .await
}

/// Allow the UI to check whether a path would be allowed (privacy pre-check).
#[tauri::command]
pub async fn watcher_check_path(
    path: String,
) -> Cmd<bool> {
    let p = std::path::Path::new(&path);
    let runner = FileWatcherRunner::new();
    Ok(runner.privacy_check(p))
}
