pub mod alpha;
pub mod ambient;
pub mod autonomy;
pub mod chats;
pub mod credentials;
pub mod dbview;
pub mod docx;
pub mod docx_link;
pub mod folders;
pub mod ghost;
pub mod health;
pub mod handoff;
pub mod images;
pub mod inbox;
pub mod intelligence;
pub mod memory;
pub mod news;
pub mod planner;
pub mod project;
pub mod resume;
pub mod roots;
pub mod screen;
pub mod search;
pub mod snapshot;
pub mod story;
pub mod tabs;
pub mod watcher;
pub mod wellness;
pub mod workspace;

use std::sync::Arc;

use crate::db::models::AppInfo;
use crate::state::AppState;

pub type Cmd<T> = Result<T, String>;

/// Helper to get database connection from AppState
pub fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())
}

/// Run a blocking closure against app state inside Tauri's blocking thread pool.
pub(crate) async fn blocking<T, F>(st: Arc<AppState>, f: F) -> Cmd<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&st))
        .await
        .map_err(|_| crate::error::ERR_JOIN.to_string())?
}

/// Application information for the Metadata/about surfaces.
#[tauri::command]
pub async fn get_app_info(state: tauri::State<'_, Arc<AppState>>) -> Cmd<AppInfo> {
    let st = state.inner().clone();
    blocking(st, |app| {
        let conn = app
            .conn
            .lock()
            .map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        Ok(AppInfo {
            app_name: "Chat Memory Tree".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            data_dir: app.data_dir.display().to_string(),
            db_path: app.db_path().display().to_string(),
            files_dir: app.files_root().display().to_string(),
            fts_enabled: crate::db::fts_enabled(&conn),
            sqlite_version: rusqlite::version().to_string(),
        })
    })
    .await
}