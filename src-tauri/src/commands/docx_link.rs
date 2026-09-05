//! 📄 DOCX ↔ Memory Link Tauri commands
//!
//! Allows the BlockEditor to link an existing memory to a block, list
//! linked memories for a block, and unlink. All operations are idempotent
//! where possible and never silently fail.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::commands::Cmd;
use crate::memory::docx_link;
use crate::state::AppState;

fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| "db lock poisoned".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocBlockLinkDto {
    pub id: String,
    pub block_id: String,
    pub document_id: String,
    pub memory_id: String,
    pub block_type: Option<String>,
    pub created_at_ms: i64,
}

impl From<docx_link::DocBlockLink> for DocBlockLinkDto {
    fn from(l: docx_link::DocBlockLink) -> Self {
        Self {
            id: l.id,
            block_id: l.block_id,
            document_id: l.document_id,
            memory_id: l.memory_id,
            block_type: l.block_type,
            created_at_ms: l.created_at_ms,
        }
    }
}

#[tauri::command]
pub async fn docx_link_block_to_memory(
    state: tauri::State<'_, Arc<AppState>>,
    block_id: String,
    document_id: String,
    block_type: Option<String>,
    memory_id: String,
) -> Cmd<DocBlockLinkDto> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let link = docx_link::link_block_to_memory(
            &conn,
            &block_id,
            &document_id,
            block_type.as_deref(),
            &memory_id,
        )?;
        Ok(link.into())
    })
    .await
}

#[tauri::command]
pub async fn docx_unlink_block_memory(
    state: tauri::State<'_, Arc<AppState>>,
    block_id: String,
    memory_id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        docx_link::unlink_block_memory(&conn, &block_id, &memory_id)
    })
    .await
}

#[tauri::command]
pub async fn docx_list_block_memories(
    state: tauri::State<'_, Arc<AppState>>,
    block_id: String,
) -> Cmd<Vec<DocBlockLinkDto>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let links = docx_link::memories_for_block(&conn, &block_id)?;
        Ok(links.into_iter().map(DocBlockLinkDto::from).collect())
    })
    .await
}

#[tauri::command]
pub async fn docx_list_memory_blocks(
    state: tauri::State<'_, Arc<AppState>>,
    memory_id: String,
) -> Cmd<Vec<DocBlockLinkDto>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let links = docx_link::blocks_for_memory(&conn, &memory_id)?;
        Ok(links.into_iter().map(DocBlockLinkDto::from).collect())
    })
    .await
}
