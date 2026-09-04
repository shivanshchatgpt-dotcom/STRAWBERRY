//! 📄 DOCX commands — CRUD + smart paste + search + export.
//! All local; heavy parsing stays in Rust (spec §PERFORMANCE).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::docx::{self, DocxDocument, PasteInput};
use crate::state::AppState;

use super::{blocking, Cmd};

/// Document list row (no block payload — cheap for the sidebar list).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    /// Plain-text preview (first ~120 chars) for the list.
    pub preview: String,
}

#[tauri::command]
pub async fn docx_list(state: State<'_, Arc<AppState>>) -> Cmd<Vec<DocxSummary>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, substr(coalesce(plain_text,''),1,120), updated_at
                 FROM docx_documents ORDER BY updated_at DESC LIMIT 200",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DocxSummary {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    preview: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn docx_new(state: State<'_, Arc<AppState>>) -> Cmd<DocxDocument> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let id = crate::db::new_uuid();
        let now = crate::db::now_iso();
        conn.execute(
            "INSERT INTO docx_documents(id, title, blocks_json, plain_text, created_at, updated_at)
             VALUES(?1, 'Untitled document', '[]', '', ?2, ?2)",
            rusqlite::params![&id, &now],
        )
        .map_err(|e| e.to_string())?;
        Ok(DocxDocument {
            id,
            title: "Untitled document".into(),
            blocks: vec![],
            created_at: now.clone(),
            updated_at: now,
        })
    })
    .await
}

#[tauri::command]
pub async fn docx_open(
    state: State<'_, Arc<AppState>>,
    document_id: String,
) -> Cmd<DocxDocument> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let (title, blocks_json, created, updated): (String, String, String, String) = conn
            .query_row(
                "SELECT title, blocks_json, created_at, updated_at
                 FROM docx_documents WHERE id = ?1",
                [&document_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|_| "document not found".to_string())?;
        let blocks: Vec<crate::docx::Block> =
            serde_json::from_str(&blocks_json).map_err(|e| e.to_string())?;
        Ok(DocxDocument {
            id: document_id,
            title,
            blocks,
            created_at: created,
            updated_at: updated,
        })
    })
    .await
}

/// Autosave target (debounced from the frontend).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxSaveArgs {
    pub document_id: String,
    pub title: String,
    pub blocks: serde_json::Value,
}

#[tauri::command]
pub async fn docx_save(state: State<'_, Arc<AppState>>, args: DocxSaveArgs) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let blocks: Vec<crate::docx::Block> =
            serde_json::from_value(args.blocks).map_err(|e| e.to_string())?;
        let plain = docx::blocks_to_plain_text(&blocks);
        let json = serde_json::to_string(&blocks).map_err(|e| e.to_string())?;
        let now = crate::db::now_iso();
        conn.execute(
            "UPDATE docx_documents
             SET title = ?1, blocks_json = ?2, plain_text = ?3, updated_at = ?4
             WHERE id = ?5",
            rusqlite::params![&args.title, &json, &plain, &now, &args.document_id],
        )
        .map_err(|e| e.to_string())?;
        // Keep the FTS index in sync (external-content table).
        let rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM docx_documents WHERE id = ?1",
                [&args.document_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let title_copy = args.title.clone();
        conn.execute(
            "INSERT INTO docx_fts(rowid, title, plain_text) VALUES(?1, ?2, ?3)
             ON CONFLICT(rowid) DO UPDATE SET title = excluded.title, plain_text = excluded.plain_text",
            rusqlite::params![rowid, &title_copy, &plain],
        )
        .map_err(|e| format!("fts sync: {e}"))?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn docx_delete(state: State<'_, Arc<AppState>>, document_id: String) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let rowid: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM docx_documents WHERE id = ?1",
                [&document_id],
                |r| r.get(0),
            )
            .ok();
        conn.execute(
            "DELETE FROM docx_documents WHERE id = ?1",
            [&document_id],
        )
        .map_err(|e| e.to_string())?;
        if let Some(rid) = rowid {
            let _ = conn.execute("DELETE FROM docx_fts WHERE rowid = ?1", [rid]);
        }
        Ok(())
    })
    .await
}

/// SMART PASTE: clipboard formats in, typed blocks out (spec §SMART PASTE).
#[tauri::command]
pub async fn docx_parse_paste(
    state: State<'_, Arc<AppState>>,
    input: PasteInput,
) -> Cmd<serde_json::Value> {
    let st = state.inner().clone();
    // Parsing is pure CPU with zero DB needs — run it on the blocking pool
    // anyway so large clipboard payloads never jank the UI thread.
    blocking(st, move |_app| {
        let blocks = docx::parse_paste(&input);
        serde_json::to_value(blocks).map_err(|e| e.to_string())
    })
    .await
}

/// Global search across documents (FTS5).
#[tauri::command]
pub async fn docx_search(
    state: State<'_, Arc<AppState>>,
    query: String,
) -> Cmd<Vec<DocxSummary>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let like = format!("%{q}%");
        // LIKE fallback keeps search working even before the first FTS sync.
        let mut stmt = conn
            .prepare(
                "SELECT id, title, substr(coalesce(plain_text,''),1,120), updated_at
                 FROM docx_documents
                 WHERE title LIKE ?1 OR plain_text LIKE ?1
                 ORDER BY updated_at DESC LIMIT 50",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&like], |r| {
                Ok(DocxSummary {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    preview: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await
}

/// Export formats: markdown | html | json (all offline, all local).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxExport {
    pub filename: String,
    pub content: String,
}

#[tauri::command]
pub async fn docx_export(
    state: State<'_, Arc<AppState>>,
    document_id: String,
    format: String,
) -> Cmd<DocxExport> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let (title, blocks_json): (String, String) = conn
            .query_row(
                "SELECT title, blocks_json FROM docx_documents WHERE id = ?1",
                [&document_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| "document not found".to_string())?;
        let blocks: Vec<crate::docx::Block> =
            serde_json::from_str(&blocks_json).map_err(|e| e.to_string())?;

        let safe_title: String = title
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '-' })
            .collect::<String>()
            .trim()
            .to_string();
        let base = if safe_title.is_empty() { "document".into() } else { safe_title };

        match format.as_str() {
            "markdown" | "md" => Ok(DocxExport {
                filename: format!("{base}.md"),
                content: docx::blocks_to_markdown(&blocks, &title),
            }),
            "html" => Ok(DocxExport {
                filename: format!("{base}.html"),
                content: docx::blocks_to_html(&blocks, &title),
            }),
            "json" => Ok(DocxExport {
                filename: format!("{base}.strawberry.json"),
                content: blocks_json,
            }),
            other => Err(format!("unsupported export format: {other}")),
        }
    })
    .await
}
