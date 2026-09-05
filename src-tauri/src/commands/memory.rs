//! 🧠 Generic Memory Tauri commands
//!
//! Exposes the generic unified memory layer to the frontend.
//! All operations are local-first and respect privacy/credentials rules.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::autonomous::safety::Actor;
use crate::commands::Cmd;
use crate::memory as mem_mod;
use crate::state::AppState;

fn conn_of(
    app: &AppState,
) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| "db lock poisoned".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMemoryArgs {
    pub kind: String,
    pub title: String,
    pub content: String,
    pub source: String,
    pub project: Option<String>,
    pub tags: Option<Vec<String>>,
    pub source_application: Option<String>,
    pub source_url: Option<String>,
    pub source_file: Option<String>,
}

#[tauri::command]
pub async fn memory_create(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    args: CreateMemoryArgs,
) -> Cmd<String> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let kind = mem_mod::MemoryKind::from_str(&args.kind)
            .ok_or_else(|| format!("invalid kind: {}", args.kind))?;
        let mut m = mem_mod::NewMemory::new(kind, args.title, args.content, args.source);
        m.project_id = args.project;
        m.tags = args.tags.unwrap_or_default();
        m.source_application = args.source_application;
        m.source_url = args.source_url;
        m.source_file = args.source_file;
        let id = mem_mod::create(&conn, &m)?;
        Ok(id)
    })
    .await
}

#[tauri::command]
pub async fn memory_get(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<Option<mem_mod::Memory>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        mem_mod::get(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn memory_delete(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        mem_mod::delete(&conn, &id)
    })
    .await
}

// ─────────────────────── Update (real path) ───────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMemoryArgs {
    pub id: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub kind: Option<String>,
    pub tags: Option<Vec<String>>,
    pub category: Option<Option<String>>,
    pub project: Option<Option<String>>,
    pub session: Option<Option<String>>,
    pub source_application: Option<Option<String>>,
    pub source_window: Option<Option<String>>,
    pub source_workspace: Option<Option<String>>,
    pub source_file: Option<Option<String>>,
    pub source_url: Option<Option<String>>,
    pub source_session: Option<Option<String>>,
    pub privacy_level: Option<String>,
    pub sensitivity: Option<u8>,
    pub redaction_state: Option<String>,
    pub confidence: Option<f32>,
    pub retention_days: Option<Option<i64>>,
    pub parent_id: Option<Option<String>>,
    pub occurred_at_ms: Option<i64>,
}

#[tauri::command]
pub async fn memory_update(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    args: UpdateMemoryArgs,
) -> Cmd<mem_mod::Memory> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let kind = match args.kind {
            Some(k) => Some(mem_mod::MemoryKind::from_str(&k)
                .ok_or_else(|| format!("invalid kind: {k}"))?),
            None => None,
        };
        let privacy = match args.privacy_level {
            Some(p) => Some(mem_mod::PrivacyLevel::from_str(&p)
                .ok_or_else(|| format!("invalid privacy_level: {p}"))?),
            None => None,
        };
        let redaction = match args.redaction_state {
            Some(r) => Some(mem_mod::RedactionState::from_str(&r)
                .ok_or_else(|| format!("invalid redaction_state: {r}"))?),
            None => None,
        };
        let patch = mem_mod::MemoryUpdate {
            title: args.title,
            content: args.content,
            kind,
            tags: args.tags,
            category: args.category,
            project_id: args.project,
            session_id: args.session,
            source_application: args.source_application,
            source_window: args.source_window,
            source_workspace: args.source_workspace,
            source_file: args.source_file,
            source_url: args.source_url,
            source_session: args.source_session,
            privacy_level: privacy,
            sensitivity: args.sensitivity,
            redaction_state: redaction,
            confidence: args.confidence,
            retention_days: args.retention_days,
            parent_id: args.parent_id,
            occurred_at_ms: args.occurred_at_ms,
        };
        mem_mod::update(&conn, &args.id, &patch)
    })
    .await
}

#[tauri::command]
pub async fn memory_record_view(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        mem_mod::record_view(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn memory_record_copy(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        mem_mod::record_copy(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn memory_record_use(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        mem_mod::record_use(&conn, &id)
    })
    .await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchArgs {
    pub text: String,
    pub kind: Option<String>,
    pub project: Option<String>,
    pub app: Option<String>,
    pub limit: Option<usize>,
    /// Offset for pagination. Defaults to 0. UI uses this for "load more".
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub hits: Vec<mem_mod::SearchHit>,
    pub total: i64,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

#[tauri::command]
pub async fn memory_search(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    args: SearchArgs,
) -> Cmd<SearchPage> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let kind = match args.kind {
            Some(k) => Some(mem_mod::MemoryKind::from_str(&k)
                .ok_or_else(|| format!("invalid kind: {k}"))?),
            None => None,
        };
        let limit = args.limit.unwrap_or(50);
        let offset = args.offset.unwrap_or(0);
        let q = mem_mod::SearchQuery {
            text: args.text,
            kind,
            project: args.project,
            app: args.app,
            url: None, file: None, session: None, category: None,
            tags: Vec::new(), since_ms: None, until_ms: None,
            limit,
            offset,
        };
        let hits = mem_mod::search_fn(&conn, &q)?;
        // Total count of matching memories (without limit). Used for
        // UI "showing N of M" / has-more indicators.
        let total: i64 = conn
            .query_row(
                "SELECT count(*) FROM unified_memories
                 WHERE app_state != 'deleted'
                   AND redaction_state != 'blocked'
                   AND privacy_level != 'secret'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let has_more = (offset as i64 + hits.len() as i64) < total;
        Ok(SearchPage { hits, total, limit, offset, has_more })
    })
    .await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRelationshipArgs {
    pub from_id: String,
    pub to_id: String,
    pub rel_type: String,
    pub confidence: Option<f32>,
    pub evidence: Option<String>,
}

#[tauri::command]
pub async fn memory_create_relationship(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    args: CreateRelationshipArgs,
) -> Cmd<String> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let rel = mem_mod::RelationshipType::from_str(&args.rel_type)
            .ok_or_else(|| format!("invalid rel_type: {}", args.rel_type))?;
        mem_mod::relationship::create(
            &conn,
            &args.from_id,
            &args.to_id,
            rel,
            args.confidence.unwrap_or(0.5),
            args.evidence.as_deref(),
            true,
        )
    })
    .await
}

#[tauri::command]
pub async fn memory_list_relationships(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Cmd<Vec<mem_mod::relationship::Relationship>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        mem_mod::relationship::list_all(&conn, &id)
    })
    .await
}

/// Count total memories in the unified_memories table.
#[tauri::command]
pub async fn memory_count(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Cmd<i64> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| "db lock poisoned".to_string())?;
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM unified_memories WHERE app_state != 'deleted'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    })
    .await
}
