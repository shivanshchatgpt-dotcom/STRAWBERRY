use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::models::{BreadcrumbItem, ChatArtifact, NodeSummary, Root};
use crate::error;

pub mod migrations;
pub mod models;

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn open_db(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path)
        .map_err(error::to_string_err("failed to open SQLite database"))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )
    .map_err(error::to_string_err("failed to configure SQLite pragmas"))?;
    Ok(conn)
}

pub fn fts_enabled(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT value FROM app_meta WHERE key = 'fts_enabled'",
        [],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .map(|v| v == "1")
    .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

pub fn valid_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() || name.len() > 200 {
        return Err(error::ERR_INVALID_NAME.to_string());
    }
    Ok(name.to_string())
}

pub fn root_exists(conn: &Connection, root_id: &str) -> Result<(), String> {
    let found: Option<String> = conn
        .query_row("SELECT id FROM roots WHERE id = ?1", [root_id], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .map_err(error::to_string_err("database failure checking root"))?;
    if found.is_none() {
        return Err(error::ERR_MISSING_ROOT.to_string());
    }
    Ok(())
}

/// Ensure an optional parent node exists, belongs to the given root and is a folder.
pub fn parent_exists(
    conn: &Connection,
    root_id: &str,
    parent_id: Option<&str>,
) -> Result<(), String> {
    if let Some(pid) = parent_id {
        let node_type: Option<String> = conn
            .query_row(
                "SELECT type FROM nodes WHERE id = ?1 AND root_id = ?2",
                params![pid, root_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(error::to_string_err("database failure checking folder"))?;
        match node_type.as_deref() {
            None => Err(error::ERR_MISSING_FOLDER.to_string()),
            Some("folder") => Ok(()),
            Some(_) => Err("The selected parent is not a folder.".to_string()),
        }
    } else {
        Ok(())
    }
}

/// Case-insensitive duplicate-name check among siblings (roots or nodes).
pub fn ensure_unique_name(
    conn: &Connection,
    scope: NameScope<'_>,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<(), String> {
    let kind = match scope {
        NameScope::Root(_) => "index",
        NameScope::Node { .. } => "item",
    };
    let hit: Option<String> = match scope {
        NameScope::Root(_) => {
            let mut stmt = conn
                .prepare("SELECT id FROM roots WHERE lower(name) = lower(?1)")
                .map_err(error::to_string_err("database failure checking names"))?;
            stmt.query_row([name], |r| r.get::<_, String>(0))
                .optional()
                .map_err(error::to_string_err("database failure checking names"))?
        }
        NameScope::Node {
            root_id,
            parent_id: Some(parent),
        } => {
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM nodes
                     WHERE root_id = ?1 AND lower(name) = lower(?2) AND parent_id = ?3",
                )
                .map_err(error::to_string_err("database failure checking names"))?;
            stmt.query_row(params![root_id, name, parent], |r| r.get::<_, String>(0))
                .optional()
                .map_err(error::to_string_err("database failure checking names"))?
        }
        NameScope::Node {
            root_id,
            parent_id: None,
        } => {
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM nodes
                     WHERE root_id = ?1 AND lower(name) = lower(?2) AND parent_id IS NULL",
                )
                .map_err(error::to_string_err("database failure checking names"))?;
            stmt.query_row(params![root_id, name], |r| r.get::<_, String>(0))
                .optional()
                .map_err(error::to_string_err("database failure checking names"))?
        }
    };
    match hit {
        Some(id) if exclude_id == Some(id.as_str()) => Ok(()),
        Some(_) => Err(error::duplicate_name(kind, name)),
        None => Ok(()),
    }
}

#[derive(Clone, Copy)]
pub enum NameScope<'a> {
    #[allow(dead_code)]
    Root(&'a str),
    Node {
        root_id: &'a str,
        parent_id: Option<&'a str>,
    },
}

pub fn next_position(conn: &Connection, parent_id: Option<&str>) -> Result<i64, String> {
    let next: i64 = match parent_id {
        Some(p) => conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM nodes WHERE parent_id = ?1",
                [p],
                |r| r.get(0),
            )
            .map_err(error::to_string_err("database failure computing position"))?,
        None => conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM nodes WHERE parent_id IS NULL",
                [],
                |r| r.get(0),
            )
            .map_err(error::to_string_err("database failure computing position"))?,
    };
    Ok(next)
}

// ---------------------------------------------------------------------------
// Tree helpers
// ---------------------------------------------------------------------------

/// Ancestors of a node from the node itself up to (and excluding) the root.
fn ancestors_up(conn: &Connection, node_id: &str) -> Result<Vec<NodeSummary>, String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE up AS (
                 SELECT id, root_id, parent_id, type, name, position, created_at, updated_at
                 FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id, n.root_id, n.parent_id, n.type, n.name, n.position,
                        n.created_at, n.updated_at
                 FROM nodes n JOIN up ON up.parent_id = n.id
             )
             SELECT id, root_id, parent_id, type, name, position, created_at, updated_at
             FROM up",
        )
        .map_err(error::to_string_err("database failure loading path"))?;
    let rows = stmt
        .query_map([node_id], NodeSummary::from_row)
        .map_err(error::to_string_err("database failure loading path"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(error::to_string_err("database failure loading path"))
}

/// Breadcrumb for a chat-node: [Root, folder, ..., currentFolderOrChat].
/// For chat nodes the final element is the containing folder; the reader view
/// appends the chat title itself.
pub fn breadcrumb_for_node(
    conn: &Connection,
    node_id: &str,
) -> Result<Vec<BreadcrumbItem>, String> {
    let chain = ancestors_up(conn, node_id)?;
    // chain[0] is the node itself; last has parent_id None. Build reversed.
    let mut rev = chain.clone();
    rev.reverse(); // now topmost ancestor first (the root-level folder or the node)

    let root_id = chain
        .first()
        .map(|n| n.root_id.clone())
        .ok_or_else(|| error::ERR_MISSING_FOLDER.to_string())?;
    let root = get_root(conn, &root_id)?;

    let mut items = vec![BreadcrumbItem {
        id: root.id.clone(),
        label: root.name.clone(),
        kind: "root".to_string(),
    }];

    // Walk from topmost down to just above the node itself.
    for i in (1..rev.len()).rev() {
        items.push(BreadcrumbItem {
            id: rev[i].id.clone(),
            label: rev[i].name.clone(),
            kind: "folder".to_string(),
        });
    }
    Ok(items)
}

/// Slash-separated ancestor folder path for a chat's node (excluding itself),
/// e.g. "Physics / Gravity" — used in search results.
pub fn folder_path_for_node(conn: &Connection, node_id: &str) -> Result<String, String> {
    let chain = ancestors_up(conn, node_id)?;
    // chain[0] is the node itself; the rest are ancestors ordered
    // child → parent, so reverse for a readable root-first path.
    let parts: Vec<String> = chain
        .iter()
        .skip(1)
        .rev()
        .map(|n| n.name.clone())
        .collect();
    Ok(parts.join(" / "))
}

/// True when `candidate` is inside the subtree rooted at `ancestor`
/// (or equals it). Used to block moving folders into their own descendants.
pub fn is_self_or_descendant(
    conn: &Connection,
    ancestor: &str,
    candidate: &str,
) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE up AS (
                 SELECT id, parent_id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT n.id, n.parent_id FROM nodes n JOIN up ON up.parent_id = n.id
             )
             SELECT 1 FROM up WHERE id = ?2 LIMIT 1",
        )
        .map_err(error::to_string_err("database failure validating move"))?;
    let hit: Option<i64> = stmt
        .query_row(params![candidate, ancestor], |r| r.get(0))
        .optional()
        .map_err(error::to_string_err("database failure validating move"))?;
    Ok(hit.is_some())
}

/// All chat nodes under a subtree (including the node itself if it is a chat).
/// Returns `(root_id, node_id)` pairs so callers can locate on-disk folders.
pub fn subtree_chat_dirs(
    conn: &Connection,
    node_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE down AS (
                 SELECT id FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT c.id FROM nodes c JOIN down d ON c.parent_id = d.id
             )
             SELECT n.root_id, n.id
             FROM nodes n JOIN down d ON d.id = n.id
             WHERE n.type = 'chat'"
        )
        .map_err(error::to_string_err("database failure collecting chats"))?;
    let rows = stmt
        .query_map([node_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(error::to_string_err("database failure collecting chats"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(error::to_string_err("database failure collecting chats"))
}

pub fn root_chat_dirs(conn: &Connection, root_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id FROM nodes n WHERE n.root_id = ?1 AND n.type = 'chat'",
        )
        .map_err(error::to_string_err("database failure collecting chats"))?;
    let rows = stmt
        .query_map([root_id], |r| r.get::<_, String>(0))
        .map_err(error::to_string_err("database failure collecting chats"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(error::to_string_err("database failure collecting chats"))
}

// ---------------------------------------------------------------------------
// Row loaders
// ---------------------------------------------------------------------------

pub fn get_root(conn: &Connection, root_id: &str) -> Result<Root, String> {
    conn.query_row(
        "SELECT id, name, color, icon, created_at, updated_at FROM roots WHERE id = ?1",
        [root_id],
        Root::from_row,
    )
    .optional()
    .map_err(error::to_string_err("database failure loading root"))?
    .ok_or_else(|| error::ERR_MISSING_ROOT.to_string())
}

pub fn artifacts_for_chat(
    conn: &Connection,
    chat_id: &str,
) -> Result<Vec<ChatArtifact>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, chat_id, artifact_type, content, created_at
             FROM chat_artifacts WHERE chat_id = ?1 ORDER BY rowid ASC",
        )
        .map_err(error::to_string_err("database failure loading artifacts"))?;
    let rows = stmt
        .query_map([chat_id], |r| {
            Ok(ChatArtifact {
                id: r.get(0)?,
                chat_id: r.get(1)?,
                artifact_type: r.get(2)?,
                content: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(error::to_string_err("database failure loading artifacts"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(error::to_string_err("database failure loading artifacts"))
}

pub fn insert_artifacts(
    conn: &Connection,
    chat_id: &str,
    artifacts: &[(String, String)],
    created_at: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO chat_artifacts (id, chat_id, artifact_type, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(error::to_string_err("database failure saving artifacts"))?;
    for (kind, content) in artifacts {
        stmt.execute(params![new_uuid(), chat_id, kind, content, created_at])
            .map_err(error::to_string_err("database failure saving artifact"))?;
    }
    Ok(())
}

pub fn delete_artifacts(conn: &Connection, chat_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM chat_artifacts WHERE chat_id = ?1", [chat_id])
        .map_err(error::to_string_err("database failure clearing artifacts"))?;
    Ok(())
}

/// Remove the on-disk directory of one chat node plus any empty parents up to
/// the files dir. Best-effort: errors are ignored on purpose.
pub fn remove_chat_files_quiet(files_root: &Path, root_id: &str, node_id: &str) {
    let dir = files_root.join(root_id).join(node_id);
    let _ = std::fs::remove_dir_all(dir);
    crate::storage::files::prune_empty_dirs(files_root.join(root_id));
}
