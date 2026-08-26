//! Tauri commands for Screen Memory feature.

use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::error;
use crate::screen::capture::{CaptureConfig, CaptureHandle, CaptureService};
use rusqlite::OptionalExtension;
use std::sync::Mutex;

type Cmd<T> = Result<T, String>;

/// Helper to get database connection from AppState
fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())
}

/// Start the screen capture service
#[tauri::command]
pub async fn start_screen_capture(
    state: State<'_, Arc<AppState>>,
    _config: Option<CaptureConfig>,
) -> Cmd<()> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;
        // Initialize screen capture service
        // For now, just ensure tables exist
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS screen_frames (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                app_name TEXT,
                window_title TEXT,
                file_path TEXT NOT NULL,
                width INTEGER,
                height INTEGER,
                byte_size INTEGER,
                perceptual_hash TEXT,
                ocr_text TEXT,
                embedding BLOB,
                is_blurred INTEGER DEFAULT 0,
                thumbnail_path TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            );
            
            CREATE INDEX IF NOT EXISTS idx_screen_ts ON screen_frames(ts);
            CREATE INDEX IF NOT EXISTS idx_screen_app ON screen_frames(app_name);
            CREATE INDEX IF NOT EXISTS idx_screen_hash ON screen_frames(perceptual_hash);
            
            CREATE TABLE IF NOT EXISTS screen_blocklist (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                added_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
                reason TEXT
            );
            
            CREATE VIRTUAL TABLE IF NOT EXISTS screen_fts USING fts5(
                ocr_text, app_name, window_title,
                content='screen_frames', content_rowid='rowid'
            );"
        ).map_err(|e| format!("Screen tables: {}", e))?;
        
        Ok(())
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

/// Stop the screen capture service
#[tauri::command]
pub async fn stop_screen_capture(_state: State<'_, Arc<AppState>>) -> Cmd<()> {
    // TODO: Stop the capture service
    Ok(())
}

/// Get screen capture configuration
#[tauri::command]
pub async fn get_screen_config(_state: State<'_, Arc<AppState>>) -> Cmd<crate::screen::capture::CaptureConfig> {
    Ok(crate::screen::capture::CaptureConfig::default())
}

/// Update screen capture configuration
#[tauri::command]
pub async fn update_screen_config(
    _state: State<'_, Arc<AppState>>,
    _config: crate::screen::capture::CaptureConfig,
) -> Cmd<()> {
    // TODO: Update config
    Ok(())
}

/// List captured screen frames
#[tauri::command]
pub async fn list_screens(
    state: State<'_, Arc<AppState>>,
    _limit: Option<u32>,
    _offset: Option<u32>,
    _app_filter: Option<String>,
    _from_ts: Option<i64>,
    _to_ts: Option<i64>,
) -> Cmd<Vec<ScreenFrame>> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;
        
        let mut query = "SELECT id, ts, app_name, window_title, file_path, width, height, byte_size, perceptual_hash, ocr_text, is_blurred, thumbnail_path, created_at FROM screen_frames WHERE 1=1".to_string();
        
        query.push_str(" ORDER BY ts DESC LIMIT 100");
        
        let mut stmt = conn.prepare(&query).map_err(|e| format!("Screen query: {}", e))?;
        let rows = stmt.query_map([], |r| Ok(ScreenFrame {
            id: r.get(0)?,
            ts: r.get(1)?,
            app_name: r.get(2)?,
            window_title: r.get(3)?,
            file_path: r.get(4)?,
            width: r.get(5)?,
            height: r.get(6)?,
            byte_size: r.get(7)?,
            perceptual_hash: r.get(8)?,
            ocr_text: r.get(9)?,
            is_blurred: r.get(10)?,
            thumbnail_path: r.get(11)?,
            created_at: r.get(12)?,
        })).map_err(|e| format!("Screen query map: {}", e))?;
        
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("Screen rows: {}", e))
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

/// Search screens by OCR text or visual similarity
#[tauri::command]
pub async fn search_screens(
    state: State<'_, Arc<AppState>>,
    query: String,
    _limit: Option<u32>,
) -> Cmd<Vec<ScreenSearchHit>> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;

        // FTS5 text search on OCR text
        if !query.trim().is_empty() {
            let mut stmt = conn.prepare(
                "SELECT id, ts, app_name, window_title, file_path, perceptual_hash, ocr_text, created_at,
                        snippet(screen_fts, 0, '<mark>', '</mark>', '…', 20) as snippet
                 FROM screen_fts
                 JOIN screen_frames ON screen_frames.rowid = screen_fts.rowid
                 WHERE screen_fts MATCH ?1
                 ORDER BY rank LIMIT ?2"
            ).map_err(|e| format!("Screen FTS query: {}", e))?;
            
            let rows = stmt.query_map(
                rusqlite::params![query, 20i64],
                |r| Ok(ScreenSearchHit {
                id: r.get(0)?,
                ts: r.get(1)?,
                app_name: r.get(2)?,
                window_title: r.get(3)?,
                file_path: r.get(4)?,
                perceptual_hash: r.get(5)?,
                snippet: r.get(7)?,
                score: 1.0,
            })).map_err(|e| format!("Screen FTS map: {}", e))?;
            
            let mut hits = Vec::new();
            for row in rows {
                hits.push(row.map_err(|e| format!("Screen FTS row: {}", e))?);
            }
            
            Ok(hits)
        } else {
            Ok(Vec::new())
        }
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

/// Get a specific screen frame by ID
#[tauri::command]
pub async fn get_screen_frame(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Cmd<Option<ScreenFrame>> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;
        
        let frame = conn.query_row(
            "SELECT id, ts, app_name, window_title, file_path, width, height, byte_size, perceptual_hash, ocr_text, is_blurred, thumbnail_path, created_at
             FROM screen_frames WHERE id = ?1",
            [id],
            |r| Ok(ScreenFrame {
                id: r.get(0)?,
                ts: r.get(1)?,
                app_name: r.get(2)?,
                window_title: r.get(3)?,
                file_path: r.get(4)?,
                width: r.get(5)?,
                height: r.get(6)?,
                byte_size: r.get(7)?,
                perceptual_hash: r.get(8)?,
                ocr_text: r.get(9)?,
                is_blurred: r.get(10)?,
                thumbnail_path: r.get(11)?,
                created_at: r.get(12)?,
            }))
            .optional()
            .map_err(|e| format!("Get screen: {}", e))?;
        
        Ok(frame)
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

/// Delete a screen frame
#[tauri::command]
pub async fn delete_screen_frame(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Cmd<()> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;
        
        // Get file path first to delete file
        let file_path: Option<String> = conn.query_row(
            "SELECT file_path FROM screen_frames WHERE id = ?1",
            [id],
            |r| r.get(0),
        ).optional().map_err(|e| format!("Get path: {}", e))?;
        
        if let Some(path) = file_path {
            let _ = std::fs::remove_file(&path);
            // Also remove thumbnail
            let thumb = path.replace(".jpg", "_thumb.jpg");
            let _ = std::fs::remove_file(&thumb);
        }
        
        conn.execute("DELETE FROM screen_frames WHERE id = ?1", [id])
            .map_err(|e| format!("Delete screen: {}", e))?;
        
        Ok(())
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

/// Add pattern to blocklist
#[tauri::command]
pub async fn add_screen_blocklist(
    state: State<'_, Arc<AppState>>,
    pattern: String,
    reason: Option<String>,
) -> Cmd<i64> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;
        
        let now = crate::db::now_iso();
        conn.execute(
            "INSERT INTO screen_blocklist (pattern, added_at, reason) VALUES (?1, ?2, ?3)",
            rusqlite::params![pattern, now, reason],
        ).map_err(|e| format!("Add blocklist: {}", e))?;
        
        Ok(conn.last_insert_rowid())
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

/// Remove pattern from blocklist
#[tauri::command]
pub async fn remove_screen_blocklist(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Cmd<()> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;
        
        conn.execute("DELETE FROM screen_blocklist WHERE id = ?1", [id])
            .map_err(|e| format!("Remove blocklist: {}", e))?;
        Ok(())
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

/// List blocklist patterns
#[tauri::command]
pub async fn list_screen_blocklist(
    state: State<'_, Arc<AppState>>,
) -> Cmd<Vec<ScreenBlocklistItem>> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = conn_of(&st)?;
        
        let mut stmt = conn.prepare(
            "SELECT id, pattern, added_at, reason FROM screen_blocklist ORDER BY added_at DESC"
        ).map_err(|e| format!("Blocklist query: {}", e))?;
        
        let rows = stmt.query_map([], |r| Ok(ScreenBlocklistItem {
            id: r.get(0)?,
            pattern: r.get(1)?,
            added_at: r.get(2)?,
            reason: r.get(3)?,
        })).map_err(|e| format!("Blocklist map: {}", e))?;
        
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("Blocklist rows: {}", e))
    }).await.map_err(|_| error::ERR_JOIN.to_string())?
}

// Types for API
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenFrame {
    pub id: i64,
    pub ts: i64,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub file_path: String,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub perceptual_hash: String,
    pub ocr_text: Option<String>,
    pub is_blurred: bool,
    pub thumbnail_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSearchHit {
    pub id: i64,
    pub ts: i64,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub file_path: String,
    pub perceptual_hash: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenBlocklistItem {
    pub id: i64,
    pub pattern: String,
    pub added_at: String,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn screen_commands_compile() {
        // Compile check
    }
}