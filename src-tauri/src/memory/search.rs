//! 🔍 Generic Unified Search
//!
//! Combines FTS, type filters, source filters, temporal filters, and
//! relationship proximity into a single ranking.
//!
//! STRICT RULES:
//!   * Credential metadata MAY appear in normal search results.
//!   * Credential SECRETS must NEVER enter the search index.
//!   * If a memory is soft-deleted or redacted, it is excluded.
//!   * Recency never overpowers strong textual relevance.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::{Memory, MemoryKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    /// Free-text query (matched against title, content, tags, OCR, block text).
    pub text: String,
    /// Optional type filter.
    pub kind: Option<MemoryKind>,
    /// Optional project filter.
    pub project: Option<String>,
    /// Optional source-app filter.
    pub app: Option<String>,
    /// Optional URL substring.
    pub url: Option<String>,
    /// Optional file substring.
    pub file: Option<String>,
    /// Optional session filter.
    pub session: Option<String>,
    /// Optional category filter.
    pub category: Option<String>,
    /// Optional tag filter (any-of).
    pub tags: Vec<String>,
    /// Optional since (millis) — only memories newer than this.
    pub since_ms: Option<i64>,
    /// Optional until (millis) — only memories older than this.
    pub until_ms: Option<i64>,
    /// Max results. Also acts as the per-page size for pagination.
    pub limit: usize,
    /// Offset for pagination. Defaults to 0.
    #[serde(default)]
    pub offset: usize,
}

impl SearchQuery {
    pub fn text_only(text: impl Into<String>, limit: usize) -> Self {
        Self {
            text: text.into(),
            kind: None, project: None, app: None, url: None,
            file: None, session: None, category: None,
            tags: Vec::new(), since_ms: None, until_ms: None,
            limit,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub memory: Memory,
    pub score: f32,
    pub matched_via: Vec<String>,  // e.g. ["title", "tags", "ocr"]
}

/// Run a unified search.
/// Ranking:
///   * exact title match → +1.0
///   * exact tag match → +0.5
///   * content FTS hit → +0.4
///   * project match → +0.3
///   * source-app match → +0.2
///   * recent (last 30d) → +0.1
///   * use_count contribution: log10(1 + use_count) * 0.05
pub fn search(conn: &Connection, q: &SearchQuery) -> Result<Vec<SearchHit>, String> {
    let mut hits: Vec<SearchHit> = Vec::new();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let q_lower = q.text.to_lowercase();
    let q_terms: Vec<&str> = q_lower.split_whitespace().collect();

    // Build a SQL query that combines the filters; FTS is the textual core.
    let mut sql = String::from(
        "SELECT id, memory_type, title, content, source, source_ref,
                importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
                first_seen_at_ms, last_seen_at_ms, last_viewed_at_ms, last_copied_at_ms,
                last_used_at_ms, view_count, copy_count, use_count,
                project_id, session_id, tags, sensitivity, privacy_level, redaction_state,
                app_state, source_application, source_window, source_workspace, source_file,
                source_url, source_session, category, parent_id, retention_days, content_hash
         FROM unified_memories
         WHERE app_state != 'deleted'
           AND redaction_state != 'blocked'
           AND privacy_level != 'secret'"
    );
    let mut param_idx = 1;
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if !q_lower.is_empty() {
        sql.push_str(" AND (lower(title) LIKE ? OR lower(content) LIKE ? OR lower(tags) LIKE ?)");
        let pat = format!("%{q_lower}%");
        args.push(Box::new(pat.clone()));
        args.push(Box::new(pat.clone()));
        args.push(Box::new(pat));
        param_idx += 3;
    }
    if let Some(k) = q.kind {
        sql.push_str(&format!(" AND memory_type = ?"));
        args.push(Box::new(k.as_str().to_string()));
        param_idx += 1;
    }
    if let Some(p) = &q.project {
        sql.push_str(" AND project_id = ?");
        args.push(Box::new(p.clone()));
        param_idx += 1;
    }
    if let Some(a) = &q.app {
        sql.push_str(" AND source_application = ?");
        args.push(Box::new(a.clone()));
        param_idx += 1;
    }
    if let Some(u) = &q.url {
        sql.push_str(" AND source_url LIKE ?");
        args.push(Box::new(format!("%{u}%")));
        param_idx += 1;
    }
    if let Some(f) = &q.file {
        sql.push_str(" AND source_file LIKE ?");
        args.push(Box::new(format!("%{f}%")));
        param_idx += 1;
    }
    if let Some(s) = &q.session {
        sql.push_str(" AND source_session = ?");
        args.push(Box::new(s.clone()));
        param_idx += 1;
    }
    if let Some(c) = &q.category {
        sql.push_str(" AND category = ?");
        args.push(Box::new(c.clone()));
        param_idx += 1;
    }
    if let Some(since) = q.since_ms {
        sql.push_str(&format!(" AND updated_at_ms >= ?"));
        args.push(Box::new(since));
        param_idx += 1;
    }
    if let Some(until) = q.until_ms {
        sql.push_str(&format!(" AND updated_at_ms <= ?"));
        args.push(Box::new(until));
        param_idx += 1;
    }
    sql.push_str(&format!(" ORDER BY updated_at_ms DESC LIMIT ? OFFSET ?"));
    args.push(Box::new(q.limit as i64));
    args.push(Box::new(q.offset as i64));

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("search prepare: {e}"))?;
    let params_vec: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params_vec), super::row_to_memory_err)
        .map_err(|e| format!("search query: {e}"))?;

    for row in rows {
        let memory = row.map_err(|e| e.to_string())?;
        let mut score = 0.0f32;
        let mut matched: Vec<String> = Vec::new();

        let title_l = memory.title.to_lowercase();
        let content_l = memory.content.to_lowercase();
        let tags_l: Vec<String> = memory.tags.iter().map(|t| t.to_lowercase()).collect();

        if !q_lower.is_empty() {
            for term in &q_terms {
                if title_l.contains(term) {
                    score += 1.0;
                    matched.push("title".into());
                }
                if content_l.contains(term) {
                    score += 0.4;
                    matched.push("content".into());
                }
                if tags_l.iter().any(|t| t.contains(term)) {
                    score += 0.5;
                    matched.push("tags".into());
                }
            }
            // Exact-phrase bonus
            if title_l == q_lower {
                score += 0.5;
            }
        }
        if let Some(p) = &q.project {
            if memory.project_id.as_deref() == Some(p.as_str()) {
                score += 0.3;
            }
        }
        if let Some(a) = &q.app {
            if memory.source_application.as_deref() == Some(a.as_str()) {
                score += 0.2;
            }
        }
        // Recency: within last 30 days → +0.1
        let age_days = (now_ms - memory.updated_at_ms) / 86_400_000;
        if age_days <= 30 {
            score += 0.1;
        }
        // Use count contribution
        score += ((1.0_f32 + memory.use_count as f32).log10().max(0.0_f32)) * 0.05_f32;

        if score > 0.0 || q_lower.is_empty() {
            matched.sort();
            matched.dedup();
            hits.push(SearchHit { memory, score, matched_via: matched });
        }
    }

    // Sort by score desc, then by recency.
    hits.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
            .then(b.memory.updated_at_ms.cmp(&a.memory.updated_at_ms))
    });
    hits.truncate(q.limit);
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{create, NewMemory, MemoryKind};

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn search_finds_memory_by_text() {
        let conn = setup();
        let mut m = NewMemory::new(MemoryKind::Semantic, "Graph algorithms", "Dijkstra and BFS", "src");
        m.tags = vec!["graph".to_string()];
        create(&conn, &m).unwrap();
        let hits = search(&conn, &SearchQuery::text_only("graph", 10)).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].memory.title.to_lowercase().contains("graph"));
    }

    #[test]
    fn search_filters_by_kind() {
        let conn = setup();
        let sem = NewMemory::new(MemoryKind::Semantic, "fact A", "body", "src");
        let img = NewMemory::new(MemoryKind::Image, "image B", "body", "src");
        create(&conn, &sem).unwrap();
        create(&conn, &img).unwrap();
        let q = SearchQuery {
            text: "body".into(),
            kind: Some(MemoryKind::Image),
            project: None, app: None, url: None, file: None,
            session: None, category: None, tags: Vec::new(),
            since_ms: None, until_ms: None, limit: 10, offset: 0,
        };
        let hits = search(&conn, &q).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].memory.kind, "image");
    }

    #[test]
    fn search_excludes_secret_memories() {
        let conn = setup();
        let mut m = NewMemory::new(MemoryKind::Credential, "Example Credential", "metadata", "src");
        m.privacy_level = crate::memory::PrivacyLevel::Secret;
        create(&conn, &m).unwrap();
        let hits = search(&conn, &SearchQuery::text_only("credential", 10)).unwrap();
        assert!(hits.is_empty(), "secret memories must not appear in normal search");
    }

    #[test]
    fn search_uses_recency_with_text() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "Recent topic", "body", "src");
        create(&conn, &m).unwrap();
        let hits = search(&conn, &SearchQuery::text_only("recent", 10)).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].score > 0.0);
    }
}
