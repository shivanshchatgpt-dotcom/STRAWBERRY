//! 🖼️ Image Memory Tauri commands
//!
//! Provides a thin UI-facing API over the image memory module.
//! Image files live in the filesystem; the DB stores metadata and OCR text.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::commands::Cmd;
use crate::memory::image as imgs;
use crate::state::AppState;

fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| "db lock poisoned".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterImageArgs {
    pub path: String,
    pub mime_type: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub byte_size: Option<i64>,
    pub caption: Option<String>,
    pub source_app: Option<String>,
    pub source_window: Option<String>,
    pub source_project: Option<String>,
    /// Whether the image content is privacy-sensitive (e.g. screenshot of
    /// a banking app, password manager, etc). When true, OCR is skipped
    /// and the image body is not indexed.
    pub privacy_blocked: bool,
}

#[tauri::command]
pub async fn image_register(
    state: tauri::State<'_, Arc<AppState>>,
    args: RegisterImageArgs,
) -> Cmd<String> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        imgs::register(
            &conn,
            &args.path,
            args.mime_type.as_deref(),
            args.width,
            args.height,
            args.byte_size,
            args.caption.as_deref(),
            args.source_app.as_deref(),
            args.source_window.as_deref(),
            args.source_project.as_deref(),
            args.privacy_blocked,
        )
    })
    .await
}

#[tauri::command]
pub async fn image_get(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<Option<imgs::ImageAsset>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        imgs::get(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn image_list(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Cmd<Vec<imgs::ImageAsset>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let limit = limit.unwrap_or(50);
        // Use a helper closure to iterate and collect within stmt's borrow scope.
        let mut out: Vec<imgs::ImageAsset> = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT id, memory_id, original_path, thumbnail_path, mime_type, width, height, byte_size,
                    caption, source_app, source_window, source_project, ocr_text, ocr_status,
                    ocr_completed_at_ms, thumbnail_status, thumbnail_completed_at_ms, privacy_blocked,
                    created_at_ms, updated_at_ms
             FROM image_assets ORDER BY created_at_ms DESC LIMIT ?1"
        ).map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(rusqlite::params![limit as i64])
            .map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            out.push(imgs::row_to_image_for_test(row).map_err(|e| e.to_string())?);
        }
        Ok(out)
    })
    .await
}#[tauri::command]
pub async fn image_delete(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        imgs::delete(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn image_set_ocr_text(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    text: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        imgs::set_ocr_text(&conn, &id, &text)
    })
    .await
}

#[tauri::command]
pub async fn image_mark_ocr_failed(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        imgs::mark_ocr_failed(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn image_mark_ocr_unavailable(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        imgs::mark_ocr_unavailable(&conn, &id)
    })
    .await
}

/// Trigger an OCR run for the next pending image. Returns the OCR result.
#[tauri::command]
pub async fn image_ocr_run_next(
    state: tauri::State<'_, Arc<AppState>>,
) -> Cmd<Option<OcrRunResult>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        // Read the next pending image.
        let next = imgs::next_ocr_pending(&conn)?;
        let next = match next {
            Some(n) => n,
            None => return Ok(None),
        };
        // Move to 'running'.
        crate::autonomous::ocr::set_running(&conn, &next.id).ok();
        // Run OCR.
        let result = crate::autonomous::ocr::run_local_ocr(Path::new(&next.original_path));
        // Persist result with redaction.
        let (status, text) = match result.status {
            imgs::OcrStatus::Done => {
                let safe = result
                    .text
                    .as_ref()
                    .map(|t| crate::autonomous::ocr::redact_ocr_text(t))
                    .unwrap_or_default();
                crate::autonomous::ocr::set_done(&conn, &next.id, &safe).ok();
                (imgs::OcrStatus::Done, Some(safe))
            }
            imgs::OcrStatus::Failed => {
                crate::autonomous::ocr::set_failed(&conn, &next.id).ok();
                (imgs::OcrStatus::Failed, None)
            }
            imgs::OcrStatus::Unavailable => {
                crate::autonomous::ocr::set_unavailable(&conn, &next.id).ok();
                (imgs::OcrStatus::Unavailable, None)
            }
            other => (other, None),
        };
        Ok(Some(OcrRunResult {
            image_id: next.id,
            status,
            text,
            engine: result.engine,
            error: result.error,
        }))
    })
    .await
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRunResult {
    pub image_id: String,
    pub status: imgs::OcrStatus,
    pub text: Option<String>,
    pub engine: String,
    pub error: Option<String>,
}
