use std::sync::Arc;

use crate::db;
use crate::db::models::SearchResultItem;
use crate::error;
use crate::state::AppState;
use tauri::State;

use super::{blocking, Cmd};

const MAX_RESULTS: usize = 100;

fn fts_match_expression(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

fn like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Search chats by title, first idea, tags and brief text.
///
/// `scope_kind` is one of `global`, `root`, `folder`; for scoped variants
/// `scope_id` carries the root id or folder node id.
#[tauri::command]
pub async fn search_chats(
    state: State<'_, Arc<AppState>>,
    query: String,
    scope_kind: Option<String>,
    scope_id: Option<String>,
) -> Cmd<Vec<SearchResultItem>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let q = query.trim().to_string();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let scope = scope_kind.unwrap_or_else(|| "global".to_string());
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let use_fts = db::fts_enabled(&conn);

        let rows: Vec<SearchResultItem> = if use_fts {
            match fts_search(&conn, &q, &scope, scope_id.as_deref()) {
                Ok(r) => r,
                // Malformed MATCH expressions etc. fall back to LIKE silently.
                Err(_) => like_search(&conn, &q, &scope, scope_id.as_deref())?,
            }
        } else {
            like_search(&conn, &q, &scope, scope_id.as_deref())?
        };
        Ok(rows)
    })
    .await
}

fn is_scoped(scope: &str) -> bool {
    matches!(scope, "root" | "folder")
}

fn scope_clause(scope: &str) -> String {
    match scope {
        "root" => " AND n.root_id = ?scope".to_string(),
        "folder" => " AND n.id IN (
                        WITH RECURSIVE down AS (
                            SELECT id FROM nodes WHERE id = ?scope
                            UNION ALL
                            SELECT c.id FROM nodes c JOIN down d ON c.parent_id = d.id
                        ) SELECT id FROM down
                     )"
            .to_string(),
        _ => String::new(),
    }
}

fn map_result_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SearchResultItem> {
    Ok(SearchResultItem {
        chat_id: r.get(0)?,
        node_id: r.get(1)?,
        title: r.get(2)?,
        root_name: r.get(3)?,
        snippet: r.get(4)?,
        created_at: r.get(5)?,
        folder_path: String::new(),
    })
}

fn collect_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<SearchResultItem>>,
    conn: &rusqlite::Connection,
) -> Result<Vec<SearchResultItem>, String> {
    let mut items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(error::to_string_err("database failure searching"))?;
    for item in items.iter_mut() {
        item.folder_path = db::folder_path_for_node(conn, &item.node_id)?;
    }
    Ok(items)
}

fn fts_search(
    conn: &rusqlite::Connection,
    query: &str,
    scope: &str,
    scope_id: Option<&str>,
) -> Result<Vec<SearchResultItem>, String> {
    let match_expr =
        fts_match_expression(query).ok_or_else(|| "empty query".to_string())?;

    let sql = format!(
        "SELECT ch.id, ch.node_id, ch.title, r.name,
                snippet(chat_fts, -1, '', '', ' … ', 14),
                ch.created_at
         FROM chat_fts f
         JOIN chats ch ON ch.rowid = f.rowid
         JOIN nodes n ON n.id = ch.node_id
         JOIN roots r ON r.id = n.root_id
         WHERE chat_fts MATCH :match{clause}
         ORDER BY rank LIMIT {limit}",
        clause = scope_clause(scope),
        limit = MAX_RESULTS
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(error::to_string_err("database failure searching"))?;
    let sid: String = scope_id.unwrap_or("").to_string();
    let mut named: Vec<(&str, &dyn rusqlite::ToSql)> =
        vec![(":match", &match_expr as &dyn rusqlite::ToSql)];
    if is_scoped(scope) {
        named.push((":scope", &sid));
    }
    let rows = stmt
        .query_map(named.as_slice(), map_result_row)
        .map_err(error::to_string_err("database failure searching"))?;
    collect_rows(rows, conn)
}

fn like_search(
    conn: &rusqlite::Connection,
    query: &str,
    scope: &str,
    scope_id: Option<&str>,
) -> Result<Vec<SearchResultItem>, String> {
    let pattern = like_pattern(query);
    let sql = format!(
        "SELECT ch.id, ch.node_id, ch.title, r.name,
                substr(coalesce(ch.brief_text, ''), 1, 160),
                ch.created_at
         FROM chats ch
         JOIN nodes n ON n.id = ch.node_id
         JOIN roots r ON r.id = n.root_id
         WHERE (
             lower(ch.title) LIKE :pat ESCAPE '\\'
             OR lower(coalesce(ch.first_idea, '')) LIKE :pat ESCAPE '\\'
             OR lower(coalesce(ch.tags, '')) LIKE :pat ESCAPE '\\'
             OR lower(coalesce(ch.brief_text, '')) LIKE :pat ESCAPE '\\'
         ){clause}
         ORDER BY ch.created_at DESC LIMIT {limit}",
        clause = scope_clause(scope),
        limit = MAX_RESULTS
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(error::to_string_err("database failure searching"))?;
    let sid: String = scope_id.unwrap_or("").to_string();
    let mut named: Vec<(&str, &dyn rusqlite::ToSql)> =
        vec![(":pat", &pattern as &dyn rusqlite::ToSql)];
    if is_scoped(scope) {
        named.push((":scope", &sid));
    }
    let rows = stmt
        .query_map(named.as_slice(), map_result_row)
        .map_err(error::to_string_err("database failure searching"))?;
    collect_rows(rows, conn)
}
