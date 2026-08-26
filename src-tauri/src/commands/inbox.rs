//! 📥 Universal Inbox — daemon captures (clipboard saves) as a filterable feed.
//! Captures are real chats with source='capture'; tags column holds the kind
//! (note | code | error | url) written by the capture-daemon.

use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::error;
use super::blocking;

type Cmd<T> = Result<T, String>;

fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub chat_id: String,
    pub title: String,
    pub kind: Option<String>,
    pub preview: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxCounts {
    pub all: i64,
    pub note: i64,
    pub code: i64,
    pub error: i64,
    pub url: i64,
}

/// List capture items, optionally filtered by kind.
#[tauri::command]
pub async fn get_inbox_items(
    state: State<'_, Arc<AppState>>,
    kind: Option<String>,
    limit: Option<u32>,
) -> Cmd<Vec<InboxItem>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = conn_of(app)?;
        let limit = limit.unwrap_or(100).clamp(1, 500) as i64;

        let (sql, has_kind): (String, bool) = match kind.as_deref() {
            Some(k) if !k.is_empty() => (
                "SELECT ch.id, ch.title, ch.tags, substr(coalesce(ch.brief_text,''),1,120), ch.created_at
                 FROM chats ch
                 WHERE ch.source='capture' AND ch.tags = ?1
                 ORDER BY ch.created_at DESC LIMIT ?2"
                    .to_string(),
                true,
            ),
            _ => (
                "SELECT ch.id, ch.title, ch.tags, substr(coalesce(ch.brief_text,''),1,120), ch.created_at
                 FROM chats ch
                 WHERE ch.source='capture'
                 ORDER BY ch.created_at DESC LIMIT ?1"
                    .to_string(),
                false,
            ),
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(error::to_string_err("inbox query"))?;
        let map = |r: &rusqlite::Row<'_>| {
            Ok(InboxItem {
                chat_id: r.get(0)?,
                title: r.get(1)?,
                kind: r.get(2)?,
                preview: r.get(3)?,
                created_at: r.get(4)?,
            })
        };
        let rows = if has_kind {
            let k = kind.unwrap_or_default();
            stmt.query_map(rusqlite::params![k, limit], map)
        } else {
            stmt.query_map(rusqlite::params![limit], map)
        }
        .map_err(error::to_string_err("inbox map"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(error::to_string_err("inbox rows"))
    })
    .await
}

/// Counts per kind for the filter chips.
#[tauri::command]
pub async fn get_inbox_counts(state: State<'_, Arc<AppState>>) -> Cmd<InboxCounts> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = conn_of(app)?;
        let one = |extra: &str| -> i64 {
            let sql = format!(
                "SELECT COUNT(*) FROM chats WHERE source='capture'{extra}"
            );
            conn.query_row(&sql, [], |r| r.get(0)).unwrap_or(0)
        };
        Ok(InboxCounts {
            all: one(""),
            note: one(" AND tags='note'"),
            code: one(" AND tags='code'"),
            error: one(" AND tags='error'"),
            url: one(" AND tags='url'"),
        })
    })
    .await
}

/// Delete an inbox capture permanently.
#[tauri::command]
pub async fn delete_inbox_item(state: State<'_, Arc<AppState>>, chat_id: String) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = conn_of(app)?;
        let node_id: Option<String> = conn
            .query_row(
                "SELECT node_id FROM chats WHERE id = ?1",
                [&chat_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(error::to_string_err("inbox lookup"))?;
        if let Some(node_id) = node_id {
            conn.execute("DELETE FROM nodes WHERE id = ?1", [&node_id])
                .map_err(error::to_string_err("inbox delete"))?;
        }
        Ok(())
    })
    .await
}

use rusqlite::OptionalExtension;
