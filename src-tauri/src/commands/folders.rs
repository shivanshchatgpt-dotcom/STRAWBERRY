use std::sync::Arc;

use crate::db::{self};
use crate::db::models::NodeSummary;
use crate::error;
use crate::state::AppState;
use rusqlite::params;
use tauri::State;

use super::{blocking, Cmd};
use super::roots::get_node;

#[tauri::command]
pub async fn create_folder(
    state: State<'_, Arc<AppState>>,
    root_id: String,
    parent_id: Option<String>,
    name: String,
) -> Cmd<NodeSummary> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let name = db::valid_name(&name)?;
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        db::root_exists(&conn, &root_id)?;
        let parent = parent_id.as_deref();
        db::parent_exists(&conn, &root_id, parent)?;
        db::ensure_unique_name(
            &conn,
            db::NameScope::Node { root_id: &root_id, parent_id: parent },
            &name,
            None,
        )?;
        let id = db::new_uuid();
        let now = db::now_iso();
        let position = db::next_position(&conn, parent)?;
        conn.execute(
            "INSERT INTO nodes (id, root_id, parent_id, type, name, position, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'folder', ?4, ?5, ?6, ?6)",
            params![id, root_id, parent_id, name, position, now],
        )
        .map_err(error::to_string_err("failed to create folder"))?;
        get_node(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn rename_folder(
    state: State<'_, Arc<AppState>>,
    node_id: String,
    name: String,
) -> Cmd<NodeSummary> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let name = db::valid_name(&name)?;
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let node = get_node(&conn, &node_id)?;
        if node.node_type != "folder" {
            return Err("This item is not a folder.".to_string());
        }
        db::ensure_unique_name(
            &conn,
            db::NameScope::Node {
                root_id: &node.root_id.clone(),
                parent_id: node.parent_id.as_deref(),
            },
            &name,
            Some(&node_id),
        )?;
        conn.execute(
            "UPDATE nodes SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, db::now_iso(), node_id],
        )
        .map_err(error::to_string_err("failed to rename folder"))?;
        get_node(&conn, &node_id)
    })
    .await
}

#[tauri::command]
pub async fn delete_folder(state: State<'_, Arc<AppState>>, node_id: String) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        // Snapshot (root_id, chat-node-id) pairs before the cascade delete.
        let chat_nodes: Vec<(String, String)> = {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            let node = get_node(&conn, &node_id)?;
            if node.node_type != "folder" {
                return Err("This item is not a folder.".to_string());
            }
            db::subtree_chat_dirs(&conn, &node_id)?
        };
        {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            conn.execute("DELETE FROM nodes WHERE id = ?1", [&node_id])
                .map_err(error::to_string_err("failed to delete folder"))?;
        }
        // Files are removed only after the database commit succeeded.
        let files_root = app.files_root();
        for (root_id, chat_node) in &chat_nodes {
            let dir = crate::storage::files::chat_dir(&files_root, root_id, chat_node);
            let _ = std::fs::remove_dir_all(dir);
            crate::storage::files::prune_empty_dirs(files_root.join(root_id));
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn move_folder(
    state: State<'_, Arc<AppState>>,
    node_id: String,
    new_parent_id: Option<String>,
) -> Cmd<NodeSummary> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let node = get_node(&conn, &node_id)?;
        if node.node_type != "folder" {
            return Err("This item is not a folder.".to_string());
        }
        validate_move_target(&conn, &node.id, &node.root_id, new_parent_id.as_deref())?;
        let position = db::next_position(&conn, new_parent_id.as_deref())?;
        conn.execute(
            "UPDATE nodes SET parent_id = ?1, position = ?2, updated_at = ?3 WHERE id = ?4",
            params![new_parent_id, position, db::now_iso(), node_id],
        )
        .map_err(error::to_string_err("failed to move folder"))?;
        get_node(&conn, &node_id)
    })
    .await
}

/// Shared validation for moving a node to an optional new parent:
/// the target must exist, live in the same root and not be inside the
/// moved item's own subtree.
pub(crate) fn validate_move_target(
    conn: &rusqlite::Connection,
    moving_id: &str,
    root_id: &str,
    new_parent_id: Option<&str>,
) -> Result<(), String> {
    if let Some(pid) = new_parent_id {
        if pid == moving_id {
            return Err(error::ERR_MOVE_INTO_OWN_DESCENDANT.to_string());
        }
        let target = get_node(conn, pid)?;
        if target.root_id != root_id {
            return Err("Items can only be moved within the same index (root).".to_string());
        }
        if target.node_type != "folder" {
            return Err(error::ERR_MISSING_FOLDER.to_string());
        }
        if db::is_self_or_descendant(conn, moving_id, pid)? {
            return Err(error::ERR_MOVE_INTO_OWN_DESCENDANT.to_string());
        }
    }
    Ok(())
}
