use std::sync::Arc;

use crate::db;
use crate::db::models::SearchResultItem;
use crate::db::models::UnifiedSearchItem;
use crate::error;
use crate::state::AppState;
use tauri::State;

use super::{blocking, Cmd};

const MAX_RESULTS: usize = 100;
/// Per-entity cap so one table (e.g. hundreds of chats) can't crowd
/// everything else out of the unified results.
const PER_KIND_LIMIT: usize = 25;

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

// ---------------------------------------------------------------------------
// Unified search — "search everything"
// ---------------------------------------------------------------------------

/// Small helper: truncate a string to a snippet of at most `n` chars,
/// cutting on a word boundary when possible.
fn snippet_of(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    let cut: String = t.chars().take(n).collect();
    match cut.rfind(' ') {
        Some(i) if i > n / 2 => format!("{} …", &cut[..i]),
        _ => format!("{} …", cut),
    }
}

/// Search EVERYTHING: chats (FTS + LIKE), todos, habits, calendar events,
/// ghost insights and alpha-hunter candidates. Each entity is matched by
/// its own keyword-bearing text columns. Results are interleaved so the
/// user sees a mix, not 25 chats followed by everything else.
#[tauri::command]
pub async fn search_all(
    state: State<'_, Arc<AppState>>,
    query: String,
    _scope_kind: Option<String>,
    _scope_id: Option<String>,
) -> Cmd<Vec<UnifiedSearchItem>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let pattern = like_pattern(&q);
        let pat: &dyn rusqlite::ToSql = &pattern;
        let mut out: Vec<UnifiedSearchItem> = Vec::new();

        // ---- chats (reuse the existing FTS/LIKE machinery) ----
        let use_fts = db::fts_enabled(&conn);
        let chat_rows: Vec<SearchResultItem> = if use_fts {
            match fts_search(&conn, &q, "global", None) {
                Ok(r) => r,
                Err(_) => like_search(&conn, &q, "global", None)?,
            }
        } else {
            like_search(&conn, &q, "global", None)?
        };
        for c in chat_rows.into_iter().take(PER_KIND_LIMIT) {
            out.push(UnifiedSearchItem {
                kind: "chat".into(),
                entity_id: c.chat_id,
                title: c.title,
                snippet: snippet_of(&c.snippet, 140),
                location: [c.root_name, c.folder_path]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" / "),
                emoji: "💬".into(),
                created_at: c.created_at,
            });
        }

        // ---- todos ----
        {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, coalesce(description,''), coalesce(due_date,''),
                            completed, coalesce(created_at,'')
                     FROM todos
                     WHERE lower(title) LIKE ?1 ESCAPE '\\'
                        OR lower(coalesce(description,'')) LIKE ?1 ESCAPE '\\'
                     ORDER BY created_at DESC LIMIT ?2",
                )
                .map_err(error::to_string_err("database failure searching todos"))?;
            let rows = stmt
                .query_map(rusqlite::params![pat, PER_KIND_LIMIT as i64], |r| {
                    let done: i64 = r.get(4)?;
                    Ok(UnifiedSearchItem {
                        kind: "todo".into(),
                        entity_id: r.get::<_, i64>(0)?.to_string(),
                        title: r.get(1)?,
                        snippet: snippet_of(&r.get::<_, String>(2)?, 120),
                        location: if done != 0 { "Tasks · done".into() } else { "Tasks".into() },
                        emoji: if done != 0 { "✅".into() } else { "📋".into() },
                        created_at: r.get(5)?,
                    })
                })
                .map_err(error::to_string_err("database failure searching todos"))?;
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
        }

        // ---- habits ----
        {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, coalesce(icon,'🔥'), coalesce(description,''),
                            coalesce(created_at,'')
                     FROM habits
                     WHERE lower(name) LIKE ?1 ESCAPE '\\'
                        OR lower(coalesce(description,'')) LIKE ?1 ESCAPE '\\'
                     ORDER BY id DESC LIMIT ?2",
                )
                .map_err(error::to_string_err("database failure searching habits"))?;
            let rows = stmt
                .query_map(rusqlite::params![pat, PER_KIND_LIMIT as i64], |r| {
                    Ok(UnifiedSearchItem {
                        kind: "habit".into(),
                        entity_id: r.get::<_, i64>(0)?.to_string(),
                        title: r.get(1)?,
                        snippet: snippet_of(&r.get::<_, String>(3)?, 120),
                        location: "Habits".into(),
                        emoji: r.get::<_, String>(2)?,
                        created_at: r.get(4)?,
                    })
                })
                .map_err(error::to_string_err("database failure searching habits"))?;
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
        }

        // ---- calendar events ----
        {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, coalesce(description,''), coalesce(category,''),
                            coalesce(start_at,'')
                     FROM events
                     WHERE lower(title) LIKE ?1 ESCAPE '\\'
                        OR lower(coalesce(description,'')) LIKE ?1 ESCAPE '\\'
                     ORDER BY start_at DESC LIMIT ?2",
                )
                .map_err(error::to_string_err("database failure searching events"))?;
            let rows = stmt
                .query_map(rusqlite::params![pat, PER_KIND_LIMIT as i64], |r| {
                    Ok(UnifiedSearchItem {
                        kind: "event".into(),
                        entity_id: r.get(0)?,
                        title: r.get(1)?,
                        snippet: snippet_of(&r.get::<_, String>(2)?, 120),
                        location: format!("Calendar · {}", r.get::<_, String>(3)?),
                        emoji: "📅".into(),
                        created_at: r.get(4)?,
                    })
                })
                .map_err(error::to_string_err("database failure searching events"))?;
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
        }

        // ---- ghost insights ----
        {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, body, coalesce(created_at,'')
                     FROM ghost_insights
                     WHERE lower(title) LIKE ?1 ESCAPE '\\'
                        OR lower(body) LIKE ?1 ESCAPE '\\'
                     ORDER BY id DESC LIMIT ?2",
                )
                .map_err(error::to_string_err("database failure searching insights"))?;
            let rows = stmt
                .query_map(rusqlite::params![pat, PER_KIND_LIMIT as i64], |r| {
                    Ok(UnifiedSearchItem {
                        kind: "insight".into(),
                        entity_id: r.get::<_, i64>(0)?.to_string(),
                        title: r.get(1)?,
                        snippet: snippet_of(&r.get::<_, String>(2)?, 140),
                        location: "Ghost insights".into(),
                        emoji: "👻".into(),
                        created_at: r.get(3)?,
                    })
                })
                .map_err(error::to_string_err("database failure searching insights"))?;
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
        }

        // ---- alpha hunter candidates ----
        {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, coalesce(notes,''), coalesce(provider, model_id, source),
                            coalesce(detected_at,'')
                     FROM alpha_candidates
                     WHERE lower(title) LIKE ?1 ESCAPE '\\'
                        OR lower(coalesce(notes,'')) LIKE ?1 ESCAPE '\\'
                        OR lower(coalesce(provider,'')) LIKE ?1 ESCAPE '\\'
                        OR lower(coalesce(model_id,'')) LIKE ?1 ESCAPE '\\'
                     ORDER BY detected_at DESC LIMIT ?2",
                )
                .map_err(error::to_string_err("database failure searching alpha"))?;
            let rows = stmt
                .query_map(rusqlite::params![pat, PER_KIND_LIMIT as i64], |r| {
                    Ok(UnifiedSearchItem {
                        kind: "alpha".into(),
                        entity_id: r.get(0)?,
                        title: r.get(1)?,
                        snippet: snippet_of(&r.get::<_, String>(2)?, 140),
                        location: format!("Alpha Hunter · {}", r.get::<_, String>(3)?),
                        emoji: "🎯".into(),
                        created_at: r.get(4)?,
                    })
                })
                .map_err(error::to_string_err("database failure searching alpha"))?;
            for row in rows {
                out.push(row.map_err(|e| e.to_string())?);
            }
        }

        // Interleave: sort newest first for a natural "most recent first" list,
        // then cap. (ISO timestamps sort lexicographically.)
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out.truncate(MAX_RESULTS);
        Ok(out)
    })
    .await
}
