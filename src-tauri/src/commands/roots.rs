use std::sync::Arc;

use crate::db::{self, models::{BreadcrumbItem, NodeSummary, Root, TreeNode}};
use crate::error;
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};
use tauri::State;

use super::{blocking, Cmd};

// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_roots(state: State<'_, Arc<AppState>>) -> Cmd<Vec<Root>> {
    let st = state.inner().clone();
    blocking(st, |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, color, icon, created_at, updated_at
                 FROM roots ORDER BY lower(name) ASC",
            )
            .map_err(error::to_string_err("database failure loading roots"))?;
        let rows = stmt
            .query_map([], Root::from_row)
            .map_err(error::to_string_err("database failure loading roots"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(error::to_string_err("database failure loading roots"))
    })
    .await
}

#[tauri::command]
pub async fn create_root(
    state: State<'_, Arc<AppState>>,
    name: String,
    color: Option<String>,
    icon: Option<String>,
) -> Cmd<Root> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let name = db::valid_name(&name)?;
        let now = db::now_iso();
        let id = db::new_uuid();
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        db::ensure_unique_name(&conn, db::NameScope::Root(&name), &name, None)?;
        conn.execute(
            "INSERT INTO roots (id, name, color, icon, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, name, color, icon, now],
        )
        .map_err(error::to_string_err("failed to create index"))?;
        db::get_root(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn rename_root(
    state: State<'_, Arc<AppState>>,
    root_id: String,
    name: String,
) -> Cmd<Root> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let name = db::valid_name(&name)?;
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        // Ensure target exists first for a friendly error.
        db::get_root(&conn, &root_id)?;
        db::ensure_unique_name(&conn, db::NameScope::Root(&name), &name, Some(&root_id))?;
        conn.execute(
            "UPDATE roots SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, db::now_iso(), root_id],
        )
        .map_err(error::to_string_err("failed to rename index"))?;
        db::get_root(&conn, &root_id)
    })
    .await
}

#[tauri::command]
pub async fn delete_root(state: State<'_, Arc<AppState>>, root_id: String) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let chat_node_ids = {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            db::get_root(&conn, &root_id)?;
            db::root_chat_dirs(&conn, &root_id)?
        };
        {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            conn.execute("DELETE FROM roots WHERE id = ?1", [&root_id])
                .map_err(error::to_string_err("failed to delete index"))?;
        }
        // Remove stored files after DB commit.
        let dir = app.files_root().join(&root_id);
        crate::storage::files::remove_dir_tree(&dir)?;
        let _ = chat_node_ids; // directories live under the root folder; removed above
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------------------
// Tree / navigation
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_children(
    state: State<'_, Arc<AppState>>,
    root_id: String,
    parent_id: Option<String>,
) -> Cmd<Vec<NodeSummary>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        db::get_root(&conn, &root_id)?;
        load_children(&conn, &root_id, parent_id.as_deref())
    })
    .await
}

pub(crate) fn load_children(
    conn: &rusqlite::Connection,
    root_id: &str,
    parent_id: Option<&str>,
) -> Result<Vec<NodeSummary>, String> {
    let sql = "SELECT n.id, n.root_id, n.parent_id, n.type, n.name, n.position,
                      n.created_at, n.updated_at, ch.id AS chat_id
               FROM nodes n
               LEFT JOIN chats ch ON ch.node_id = n.id
               WHERE n.root_id = ?1 AND n.parent_id IS ?2
               ORDER BY n.type DESC, n.position ASC, lower(n.name) ASC";
    let mut stmt = conn
        .prepare(sql)
        .map_err(error::to_string_err("database failure loading children"))?;
    let rows = stmt
        .query_map(params![root_id, parent_id], |r| {
            let mut node = NodeSummary::from_row(r)?;
            node.chat_id = r.get("chat_id").ok().flatten();
            Ok(node)
        })
        .map_err(error::to_string_err("database failure loading children"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(error::to_string_err("database failure loading children"))
}

#[tauri::command]
pub async fn get_node_path(
    state: State<'_, Arc<AppState>>,
    node_id: String,
) -> Cmd<Vec<BreadcrumbItem>> {
    breadcrumb_impl(state.inner().clone(), node_id).await
}

#[tauri::command]
pub async fn get_breadcrumb(
    state: State<'_, Arc<AppState>>,
    node_id: String,
) -> Cmd<Vec<BreadcrumbItem>> {
    breadcrumb_impl(state.inner().clone(), node_id).await
}

async fn breadcrumb_impl(st: Arc<AppState>, node_id: String) -> Cmd<Vec<BreadcrumbItem>> {
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        db::breadcrumb_for_node(&conn, &node_id)
    })
    .await
}

#[tauri::command]
pub async fn get_root_tree(
    state: State<'_, Arc<AppState>>,
    root_id: String,
) -> Cmd<Vec<TreeNode>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        db::get_root(&conn, &root_id)?;
        let all = {
            let mut stmt = conn
                .prepare(
                    "SELECT id, root_id, parent_id, type, name, position, created_at, updated_at
                     FROM nodes WHERE root_id = ?1
                     ORDER BY type DESC, position ASC, lower(name) ASC",
                )
                .map_err(error::to_string_err("database failure loading tree"))?;
            let rows = stmt
                .query_map([&root_id], NodeSummary::from_row)
                .map_err(error::to_string_err("database failure loading tree"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(error::to_string_err("database failure loading tree"))?
        };

        use std::collections::HashMap;
        let mut by_parent: HashMap<Option<String>, Vec<TreeNode>> = HashMap::new();
        let mut nodes: HashMap<String, TreeNode> = HashMap::new();
        for n in all {
            nodes.insert(n.id.clone(), n.into());
        }
        for tn in nodes.values() {
            let key: Option<String> = match &tn.parent_id {
                Some(pid) if nodes.contains_key(pid) => Some(pid.clone()),
                _ => None,
            };
            by_parent.entry(key).or_default().push(tn.clone());
        }
        fn attach(map: &HashMap<Option<String>, Vec<TreeNode>>, parent: Option<&String>) -> Vec<TreeNode> {
            match map.get(&parent.cloned()).map(|v| v.clone()) {
                None => Vec::new(),
                Some(children) => children
                    .into_iter()
                    .map(|mut c| {
                        c.children = attach(map, Some(&c.id));
                        c
                    })
                    .collect(),
            }
        }
        Ok(attach(&by_parent, None))
    })
    .await
}

/// Used by folders.rs to re-load a node row.
pub(crate) fn get_node(
    conn: &rusqlite::Connection,
    node_id: &str,
) -> Result<NodeSummary, String> {
    conn.query_row(
        "SELECT id, root_id, parent_id, type, name, position, created_at, updated_at
         FROM nodes WHERE id = ?1",
        [node_id],
        NodeSummary::from_row,
    )
    .optional()
    .map_err(error::to_string_err("database failure loading node"))?
    .ok_or_else(|| error::ERR_MISSING_FOLDER.to_string())
}
