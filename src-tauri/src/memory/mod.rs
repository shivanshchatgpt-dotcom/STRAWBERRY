//! 🧠 Generic Memory — root module.
//!
//! Submodules:
//!   * `relationship` — generic typed relationships between memory IDs
//!   * `search`       — unified search with FTS, type, source, time, relationships
//!   * `credential`   — generic credential memory with secret protection
//!   * `image`        — image memory metadata + OCR queue
//!   * `docx_link`    — DOCX block → memory link

pub mod relationship;
pub mod search;
pub mod credential;
pub mod image;
pub mod docx_link;
pub mod secret_store;

// Re-exports for convenient access (e.g. `memory::SearchHit`).
pub use search::{search as search_fn, SearchHit, SearchQuery};
pub use relationship::Relationship;
pub use relationship::{create as create_relationship, list_all as list_all_relationships, neighbors};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ─────────────────────────── types ───────────────────────────

/// All memory types are generic — no domain is privileged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Working,
    Episodic,
    Semantic,
    Project,
    Procedural,
    Credential,
    Image,
    Document,
    Block,
    Generic,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Working => "working",
            MemoryKind::Episodic => "episodic",
            MemoryKind::Semantic => "semantic",
            MemoryKind::Project => "project",
            MemoryKind::Procedural => "procedural",
            MemoryKind::Credential => "credential",
            MemoryKind::Image => "image",
            MemoryKind::Document => "document",
            MemoryKind::Block => "block",
            MemoryKind::Generic => "generic",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "working" => MemoryKind::Working,
            "episodic" => MemoryKind::Episodic,
            "semantic" => MemoryKind::Semantic,
            "project" => MemoryKind::Project,
            "procedural" => MemoryKind::Procedural,
            "credential" => MemoryKind::Credential,
            "image" => MemoryKind::Image,
            "document" => MemoryKind::Document,
            "block" => MemoryKind::Block,
            "generic" => MemoryKind::Generic,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    Public,
    Normal,
    Sensitive,
    Private,
    Secret,
}

impl PrivacyLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacyLevel::Public => "public",
            PrivacyLevel::Normal => "normal",
            PrivacyLevel::Sensitive => "sensitive",
            PrivacyLevel::Private => "private",
            PrivacyLevel::Secret => "secret",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "public" => PrivacyLevel::Public,
            "normal" => PrivacyLevel::Normal,
            "sensitive" => PrivacyLevel::Sensitive,
            "private" => PrivacyLevel::Private,
            "secret" => PrivacyLevel::Secret,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionState {
    None,
    Redacted,
    Blocked,
}

impl RedactionState {
    pub fn as_str(self) -> &'static str {
        match self {
            RedactionState::None => "none",
            RedactionState::Redacted => "redacted",
            RedactionState::Blocked => "blocked",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "none" => RedactionState::None,
            "redacted" => RedactionState::Redacted,
            "blocked" => RedactionState::Blocked,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryState {
    Active,
    Stale,
    Deleted,
    Archived,
}

impl MemoryState {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryState::Active => "active",
            MemoryState::Stale => "stale",
            MemoryState::Deleted => "deleted",
            MemoryState::Archived => "archived",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "active" => MemoryState::Active,
            "stale" => MemoryState::Stale,
            "deleted" => MemoryState::Deleted,
            "archived" => MemoryState::Archived,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    RelatedTo,
    BelongsTo,
    CreatedFrom,
    CopiedFrom,
    DerivedFrom,
    SourceFor,
    ScreenshotOf,
    CapturedDuring,
    AttachedTo,
    References,
    PartOf,
    ProducedBy,
    UsedWith,
    Contains,
    ParentOf,
    ChildOf,
    DerivedRelationship,
}

impl RelationshipType {
    pub fn as_str(self) -> &'static str {
        match self {
            RelationshipType::RelatedTo => "related_to",
            RelationshipType::BelongsTo => "belongs_to",
            RelationshipType::CreatedFrom => "created_from",
            RelationshipType::CopiedFrom => "copied_from",
            RelationshipType::DerivedFrom => "derived_from",
            RelationshipType::SourceFor => "source_for",
            RelationshipType::ScreenshotOf => "screenshot_of",
            RelationshipType::CapturedDuring => "captured_during",
            RelationshipType::AttachedTo => "attached_to",
            RelationshipType::References => "references",
            RelationshipType::PartOf => "part_of",
            RelationshipType::ProducedBy => "produced_by",
            RelationshipType::UsedWith => "used_with",
            RelationshipType::Contains => "contains",
            RelationshipType::ParentOf => "parent_of",
            RelationshipType::ChildOf => "child_of",
            RelationshipType::DerivedRelationship => "derived_relationship",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "related_to" => RelationshipType::RelatedTo,
            "belongs_to" => RelationshipType::BelongsTo,
            "created_from" => RelationshipType::CreatedFrom,
            "copied_from" => RelationshipType::CopiedFrom,
            "derived_from" => RelationshipType::DerivedFrom,
            "source_for" => RelationshipType::SourceFor,
            "screenshot_of" => RelationshipType::ScreenshotOf,
            "captured_during" => RelationshipType::CapturedDuring,
            "attached_to" => RelationshipType::AttachedTo,
            "references" => RelationshipType::References,
            "part_of" => RelationshipType::PartOf,
            "produced_by" => RelationshipType::ProducedBy,
            "used_with" => RelationshipType::UsedWith,
            "contains" => RelationshipType::Contains,
            "parent_of" => RelationshipType::ParentOf,
            "child_of" => RelationshipType::ChildOf,
            "derived_relationship" => RelationshipType::DerivedRelationship,
            _ => return None,
        })
    }
}

// ─────────────────────────── new memory creation ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMemory {
    pub id: Option<String>,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub source: String,
    pub source_ref: Option<String>,
    pub source_application: Option<String>,
    pub source_window: Option<String>,
    pub source_workspace: Option<String>,
    pub source_file: Option<String>,
    pub source_url: Option<String>,
    pub source_session: Option<String>,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub sensitivity: u8,
    pub privacy_level: PrivacyLevel,
    pub redaction_state: RedactionState,
    pub retention_days: Option<i64>,
    pub parent_id: Option<String>,
    pub occurred_at_ms: Option<i64>,
    pub importance: Option<String>,
}

impl NewMemory {
    pub fn new(kind: MemoryKind, title: impl Into<String>, content: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: None,
            kind,
            title: title.into(),
            content: content.into(),
            source: source.into(),
            source_ref: None,
            source_application: None,
            source_window: None,
            source_workspace: None,
            source_file: None,
            source_url: None,
            source_session: None,
            project_id: None,
            session_id: None,
            category: None,
            tags: Vec::new(),
            confidence: 1.0,
            sensitivity: 1,
            privacy_level: PrivacyLevel::Normal,
            redaction_state: RedactionState::None,
            retention_days: None,
            parent_id: None,
            occurred_at_ms: None,
            importance: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub source: String,
    pub source_ref: Option<String>,
    pub importance: String,
    pub source_application: Option<String>,
    pub source_window: Option<String>,
    pub source_workspace: Option<String>,
    pub source_file: Option<String>,
    pub source_url: Option<String>,
    pub source_session: Option<String>,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub sensitivity: u8,
    pub privacy_level: String,
    pub redaction_state: String,
    pub app_state: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub occurred_at_ms: i64,
    pub first_seen_at_ms: Option<i64>,
    pub last_seen_at_ms: Option<i64>,
    pub last_viewed_at_ms: Option<i64>,
    pub last_copied_at_ms: Option<i64>,
    pub last_used_at_ms: Option<i64>,
    pub view_count: i64,
    pub copy_count: i64,
    pub use_count: i64,
    pub parent_id: Option<String>,
    pub retention_days: Option<i64>,
    pub content_hash: Option<String>,
}

// ─────────────────────────── CRUD ───────────────────────────

/// Insert a new memory. Returns the ID assigned.
pub fn create(conn: &Connection, m: &NewMemory) -> Result<String, String> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let occurred = m.occurred_at_ms.unwrap_or(now_ms);
    let id = m.id.clone().unwrap_or_else(|| {
        let h = fnv1a_64(format!("{}|{}|{}|{}",
            m.kind.as_str(), m.title, m.content, m.source).as_bytes());
        format!("mem-{:016x}", h)
    });
    let content_hash = Some(fnv1a_64(m.content.as_bytes()).to_string());
    let tags_json = serde_json::to_string(&m.tags).unwrap_or_else(|_| "[]".to_string());
    let importance = m.importance.clone().unwrap_or_else(|| "medium".to_string());

    conn.execute(
        "INSERT INTO unified_memories(
            id, memory_type, title, content, source, source_ref,
            importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
            first_seen_at_ms, last_seen_at_ms,
            project_id, session_id, tags, verified, stale, retention_days,
            view_count, copy_count, use_count,
            sensitivity, privacy_level, redaction_state, content_hash,
            app_state, source_application, source_window, source_workspace,
            source_file, source_url, source_session, category, parent_id
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11,
            ?10, ?10,
            ?12, ?13, ?14, 0, 0, ?15,
            0, 0, 0,
            ?16, ?17, ?18, ?19,
            'active', ?20, ?21, ?22,
            ?23, ?24, ?25, ?26, ?27
         ) ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            source = excluded.source,
            updated_at_ms = excluded.updated_at_ms,
            confidence = excluded.confidence,
            tags = excluded.tags,
            category = excluded.category,
            source_application = excluded.source_application,
            source_window = excluded.source_window,
            source_workspace = excluded.source_workspace,
            source_file = excluded.source_file,
            source_url = excluded.source_url,
            source_session = excluded.source_session,
            parent_id = excluded.parent_id",
        params![
            id,
            m.kind.as_str(),
            m.title,
            m.content,
            m.source,
            m.source_ref,
            importance,
            m.confidence as f64,
            occurred,
            now_ms,
            now_ms, // updated_at_ms = created_at_ms on first write
            m.project_id,
            m.session_id,
            tags_json,
            m.retention_days,
            m.sensitivity as i64,
            m.privacy_level.as_str(),
            m.redaction_state.as_str(),
            content_hash,
            m.source_application,
            m.source_window,
            m.source_workspace,
            m.source_file,
            m.source_url,
            m.source_session,
            m.category,
            m.parent_id,
        ],
    )
    .map_err(|e| format!("create memory: {e}"))?;
    Ok(id)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Memory>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_type, title, content, source, source_ref,
                importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
                first_seen_at_ms, last_seen_at_ms, last_viewed_at_ms, last_copied_at_ms,
                last_used_at_ms, view_count, copy_count, use_count,
                project_id, session_id, tags, sensitivity, privacy_level, redaction_state,
                app_state, source_application, source_window, source_workspace, source_file,
                source_url, source_session, category, parent_id, retention_days, content_hash
         FROM unified_memories WHERE id = ?1 AND app_state != 'deleted'"
    ).map_err(|e| format!("get memory: {e}"))?;
    let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        Ok(Some(row_to_memory(row)?))
    } else {
        Ok(None)
    }
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE unified_memories SET app_state='deleted', updated_at_ms=?2 WHERE id=?1",
        params![id, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("delete memory: {e}"))?;
    let _ = conn.execute(
        "DELETE FROM memory_relationships WHERE from_id=?1 OR to_id=?1",
        params![id],
    );
    let _ = conn.execute(
        "DELETE FROM doc_block_memory WHERE memory_id=?1",
        params![id],
    );
    let _ = conn.execute(
        "DELETE FROM memory_assets WHERE memory_id=?1",
        params![id],
    );
    let _ = conn.execute(
        "DELETE FROM credential_fts WHERE credential_id=?1",
        params![id],
    );
    let _ = conn.execute(
        "DELETE FROM credentials WHERE id=?1",
        params![id],
    );
    Ok(n > 0)
}

pub fn hard_delete(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "DELETE FROM unified_memories WHERE id=?1",
        params![id],
    ).map_err(|e| format!("hard delete: {e}"))?;
    Ok(n > 0)
}

// ─────────────────────────── update ───────────────────────────

/// Fields that can be updated on an existing memory.
/// All fields are optional; None means "leave unchanged".
///
/// Edits NEVER change:
///   * `id` (stable identifier)
///   * `created_at_ms` / `first_seen_at_ms` (origin timestamps)
///   * `occurred_at_ms` (unless explicitly changed)
///   * `view_count` / `copy_count` / `use_count` (interaction counters)
///   * `last_viewed_at_ms` / `last_copied_at_ms` / `last_used_at_ms` (user timestamps)
///   * relationships, DOCX block links, assets
#[derive(Debug, Default, Clone)]
pub struct MemoryUpdate {
    pub title: Option<String>,
    pub content: Option<String>,
    pub kind: Option<MemoryKind>,
    pub tags: Option<Vec<String>>,
    pub category: Option<Option<String>>,
    pub project_id: Option<Option<String>>,
    pub session_id: Option<Option<String>>,
    pub source_application: Option<Option<String>>,
    pub source_window: Option<Option<String>>,
    pub source_workspace: Option<Option<String>>,
    pub source_file: Option<Option<String>>,
    pub source_url: Option<Option<String>>,
    pub source_session: Option<Option<String>>,
    pub privacy_level: Option<PrivacyLevel>,
    pub sensitivity: Option<u8>,
    pub redaction_state: Option<RedactionState>,
    pub confidence: Option<f32>,
    pub retention_days: Option<Option<i64>>,
    pub parent_id: Option<Option<String>>,
    pub occurred_at_ms: Option<i64>,
}

/// Update an existing memory in-place by stable ID.
///
/// Returns the refreshed `Memory` on success. Returns `Err` if the memory
/// does not exist or has been soft-deleted.
///
/// Fields not present in the `update` are preserved. Fields explicitly set
/// to `None` (for triple-Option fields like `category`) are set to NULL.
pub fn update(
    conn: &Connection,
    id: &str,
    patch: &MemoryUpdate,
) -> Result<Memory, String> {
    // First, fetch the current memory so we can return it after update.
    let current = get(conn, id)?
        .ok_or_else(|| format!("memory not found: {id}"))?;

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Compute the new values (Option<Option<...>> means "explicit None" vs
    // "leave alone" — match the MemoryUpdate triple-Option semantics).
    let new_title = patch.title.clone().unwrap_or_else(|| current.title.clone());
    let new_content = patch.content.clone().unwrap_or_else(|| current.content.clone());
    let new_kind = patch
        .kind
        .map(|k| k.as_str().to_string())
        .unwrap_or_else(|| current.kind.clone());
    let new_tags_json = match &patch.tags {
        Some(t) => serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()),
        None => serde_json::to_string(&current.tags).unwrap_or_else(|_| "[]".to_string()),
    };
    let new_category = match &patch.category {
        Some(c) => c.clone(),
        None => current.category.clone(),
    };
    let new_project = match &patch.project_id {
        Some(p) => p.clone(),
        None => current.project_id.clone(),
    };
    let new_session = match &patch.session_id {
        Some(s) => s.clone(),
        None => current.session_id.clone(),
    };
    let new_source_app = match &patch.source_application {
        Some(s) => s.clone(),
        None => current.source_application.clone(),
    };
    let new_source_window = match &patch.source_window {
        Some(s) => s.clone(),
        None => current.source_window.clone(),
    };
    let new_source_workspace = match &patch.source_workspace {
        Some(s) => s.clone(),
        None => current.source_workspace.clone(),
    };
    let new_source_file = match &patch.source_file {
        Some(s) => s.clone(),
        None => current.source_file.clone(),
    };
    let new_source_url = match &patch.source_url {
        Some(s) => s.clone(),
        None => current.source_url.clone(),
    };
    let new_source_session = match &patch.source_session {
        Some(s) => s.clone(),
        None => current.source_session.clone(),
    };
    let new_privacy = patch
        .privacy_level
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| current.privacy_level.clone());
    let new_sensitivity = patch.sensitivity.map(|s| s as i64).unwrap_or(current.sensitivity as i64);
    let new_redaction = patch
        .redaction_state
        .map(|r| r.as_str().to_string())
        .unwrap_or_else(|| current.redaction_state.clone());
    let new_confidence = patch
        .confidence
        .map(|c| c as f64)
        .unwrap_or(current.confidence as f64);
    let new_retention = match &patch.retention_days {
        Some(r) => *r,
        None => current.retention_days,
    };
    let new_parent = match &patch.parent_id {
        Some(p) => p.clone(),
        None => current.parent_id.clone(),
    };
    let new_occurred = patch
        .occurred_at_ms
        .unwrap_or(current.occurred_at_ms);

    let n = conn.execute(
        "UPDATE unified_memories SET
            title = ?2,
            content = ?3,
            memory_type = ?4,
            tags = ?5,
            category = ?6,
            project_id = ?7,
            session_id = ?8,
            source_application = ?9,
            source_window = ?10,
            source_workspace = ?11,
            source_file = ?12,
            source_url = ?13,
            source_session = ?14,
            privacy_level = ?15,
            sensitivity = ?16,
            redaction_state = ?17,
            confidence = ?18,
            retention_days = ?19,
            parent_id = ?20,
            occurred_at_ms = ?21,
            updated_at_ms = ?22
         WHERE id = ?1 AND app_state != 'deleted'",
        params![
            id,
            new_title,
            new_content,
            new_kind,
            new_tags_json,
            new_category,
            new_project,
            new_session,
            new_source_app,
            new_source_window,
            new_source_workspace,
            new_source_file,
            new_source_url,
            new_source_session,
            new_privacy,
            new_sensitivity,
            new_redaction,
            new_confidence,
            new_retention,
            new_parent,
            new_occurred,
            now_ms,
        ],
    )
    .map_err(|e| format!("update memory: {e}"))?;

    if n == 0 {
        return Err(format!("memory not updated (deleted or missing): {id}"));
    }

    // Re-read to return the canonical updated memory.
    get(conn, id)?
        .ok_or_else(|| format!("memory vanished after update: {id}"))
}

// ─────────────────────────── interaction tracking ───────────────────────────

pub fn record_view(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE unified_memories
         SET view_count = view_count + 1,
             last_viewed_at_ms = ?2,
             updated_at_ms = ?2
         WHERE id=?1 AND app_state != 'deleted'",
        params![id, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("record view: {e}"))?;
    Ok(n > 0)
}

pub fn record_copy(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE unified_memories
         SET copy_count = copy_count + 1,
             last_copied_at_ms = ?2,
             updated_at_ms = ?2
         WHERE id=?1 AND app_state != 'deleted'",
        params![id, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("record copy: {e}"))?;
    Ok(n > 0)
}

pub fn record_use(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE unified_memories
         SET use_count = use_count + 1,
             last_used_at_ms = ?2,
             updated_at_ms = ?2
         WHERE id=?1 AND app_state != 'deleted'",
        params![id, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("record use: {e}"))?;
    Ok(n > 0)
}

pub fn record_seen(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE unified_memories
         SET last_seen_at_ms = ?2,
             updated_at_ms = ?2
         WHERE id=?1 AND app_state != 'deleted'",
        params![id, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("record seen: {e}"))?;
    Ok(n > 0)
}

pub fn unused_duration_secs(m: &Memory) -> Option<i64> {
    let last = m.last_used_at_ms?;
    let now = chrono::Utc::now().timestamp_millis();
    let diff_ms = now - last;
    if diff_ms < 0 { Some(0) } else { Some(diff_ms / 1000) }
}

pub fn unseen_duration_secs(m: &Memory) -> Option<i64> {
    let last = m.last_seen_at_ms?;
    let now = chrono::Utc::now().timestamp_millis();
    let diff_ms = now - last;
    if diff_ms < 0 { Some(0) } else { Some(diff_ms / 1000) }
}

pub fn list_by_project(conn: &Connection, project: &str, limit: usize) -> Result<Vec<Memory>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_type, title, content, source, source_ref,
                importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
                first_seen_at_ms, last_seen_at_ms, last_viewed_at_ms, last_copied_at_ms,
                last_used_at_ms, view_count, copy_count, use_count,
                project_id, session_id, tags, sensitivity, privacy_level, redaction_state,
                app_state, source_application, source_window, source_workspace, source_file,
                source_url, source_session, category, parent_id, retention_days, content_hash
         FROM unified_memories
         WHERE project_id = ?1 AND app_state != 'deleted'
         ORDER BY updated_at_ms DESC LIMIT ?2"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![project, limit as i64], row_to_memory_err).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for memory in rows {
        out.push(memory.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn list_by_app(conn: &Connection, app: &str, limit: usize) -> Result<Vec<Memory>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_type, title, content, source, source_ref,
                importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
                first_seen_at_ms, last_seen_at_ms, last_viewed_at_ms, last_copied_at_ms,
                last_used_at_ms, view_count, copy_count, use_count,
                project_id, session_id, tags, sensitivity, privacy_level, redaction_state,
                app_state, source_application, source_window, source_workspace, source_file,
                source_url, source_session, category, parent_id, retention_days, content_hash
         FROM unified_memories
         WHERE source_application = ?1 AND app_state != 'deleted'
         ORDER BY updated_at_ms DESC LIMIT ?2"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![app, limit as i64], row_to_memory_err).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for memory in rows {
        out.push(memory.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn list_unused_since(conn: &Connection, cutoff_ms: i64, limit: usize) -> Result<Vec<Memory>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_type, title, content, source, source_ref,
                importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
                first_seen_at_ms, last_seen_at_ms, last_viewed_at_ms, last_copied_at_ms,
                last_used_at_ms, view_count, copy_count, use_count,
                project_id, session_id, tags, sensitivity, privacy_level, redaction_state,
                app_state, source_application, source_window, source_workspace, source_file,
                source_url, source_session, category, parent_id, retention_days, content_hash
         FROM unified_memories
         WHERE last_used_at_ms IS NOT NULL
           AND last_used_at_ms < ?1
           AND app_state != 'deleted'
         ORDER BY last_used_at_ms ASC LIMIT ?2"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![cutoff_ms, limit as i64], row_to_memory_err).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for memory in rows {
        out.push(memory.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn row_to_memory(r: &rusqlite::Row<'_>) -> Result<Memory, String> {
    row_to_memory_impl(r)
}

/// Public-for-submodule helper to read a memory row.
pub(crate) fn row_to_memory_for_test(r: &rusqlite::Row<'_>) -> Result<Memory, String> {
    row_to_memory_impl(r)
}

/// Free-standing adapter so `query_map` can return `Result<Memory, rusqlite::Error>`.
fn row_to_memory_err(r: &rusqlite::Row<'_>) -> Result<Memory, rusqlite::Error> {
    row_to_memory_impl(r).map_err(|e| {
        rusqlite::Error::InvalidQuery
    })
}

fn row_to_memory_impl(r: &rusqlite::Row<'_>) -> Result<Memory, String> {
    let tags_str: String = r.get(21).map_err(|e| e.to_string())?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    let id: String = r.get(0).map_err(|e| e.to_string())?;
    let kind: String = r.get(1).map_err(|e| e.to_string())?;
    let title: String = r.get(2).map_err(|e| e.to_string())?;
    let content: String = r.get(3).map_err(|e| e.to_string())?;
    let source: String = r.get(4).map_err(|e| e.to_string())?;
    let source_ref: Option<String> = r.get(5).ok();
    let importance: String = r.get(6).map_err(|e| e.to_string())?;
    let confidence: f32 = r.get::<_, f64>(7).map_err(|e| e.to_string())? as f32;
    let occurred_at_ms: i64 = r.get(8).map_err(|e| e.to_string())?;
    let created_at_ms: i64 = r.get(9).map_err(|e| e.to_string())?;
    let updated_at_ms: i64 = r.get(10).map_err(|e| e.to_string())?;
    let first_seen_at_ms: Option<i64> = r.get(11).ok();
    let last_seen_at_ms: Option<i64> = r.get(12).ok();
    let last_viewed_at_ms: Option<i64> = r.get(13).ok();
    let last_copied_at_ms: Option<i64> = r.get(14).ok();
    let last_used_at_ms: Option<i64> = r.get(15).ok();
    let view_count: i64 = r.get(16).map_err(|e| e.to_string())?;
    let copy_count: i64 = r.get(17).map_err(|e| e.to_string())?;
    let use_count: i64 = r.get(18).map_err(|e| e.to_string())?;
    let project_id: Option<String> = r.get(19).ok();
    let session_id: Option<String> = r.get(20).ok();
    let sensitivity: u8 = r.get::<_, i64>(22).map_err(|e| e.to_string())? as u8;
    let privacy_level: String = r.get(23).map_err(|e| e.to_string())?;
    let redaction_state: String = r.get(24).map_err(|e| e.to_string())?;
    let app_state: String = r.get(25).map_err(|e| e.to_string())?;
    let source_application: Option<String> = r.get(26).ok();
    let source_window: Option<String> = r.get(27).ok();
    let source_workspace: Option<String> = r.get(28).ok();
    let source_file: Option<String> = r.get(29).ok();
    let source_url: Option<String> = r.get(30).ok();
    let source_session: Option<String> = r.get(31).ok();
    let category: Option<String> = r.get(32).ok();
    let parent_id: Option<String> = r.get(33).ok();
    let retention_days: Option<i64> = r.get(34).ok();
    let content_hash: Option<String> = r.get(35).ok();
    Ok(Memory {
        id, kind, title, content, source, source_ref, importance,
        source_application, source_window, source_workspace, source_file,
        source_url, source_session,
        project_id, session_id, category, tags,
        confidence, sensitivity, privacy_level, redaction_state, app_state,
        created_at_ms, updated_at_ms, occurred_at_ms,
        first_seen_at_ms, last_seen_at_ms, last_viewed_at_ms,
        last_copied_at_ms, last_used_at_ms,
        view_count, copy_count, use_count,
        parent_id, retention_days, content_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn create_and_get_memory() {
        let conn = setup();
        let mut m = NewMemory::new(MemoryKind::Semantic, "test title", "test content", "test_source");
        m.tags = vec!["a".to_string(), "b".to_string()];
        m.project_id = Some("MyProject".to_string());
        let id = create(&conn, &m).unwrap();
        let got = get(&conn, &id).unwrap().unwrap();
        assert_eq!(got.title, "test title");
        assert_eq!(got.content, "test content");
        assert_eq!(got.source, "test_source");
        assert_eq!(got.tags, vec!["a", "b"]);
        assert_eq!(got.project_id.as_deref(), Some("MyProject"));
        assert_eq!(got.privacy_level, "normal");
    }

    #[test]
    fn record_view_does_not_touch_last_used() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        record_view(&conn, &id).unwrap();
        let got = get(&conn, &id).unwrap().unwrap();
        assert!(got.last_viewed_at_ms.is_some());
        assert!(got.last_used_at_ms.is_none(), "view must NOT set last_used");
        assert_eq!(got.view_count, 1);
        assert_eq!(got.use_count, 0);
    }

    #[test]
    fn record_copy_does_not_touch_last_used() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        record_copy(&conn, &id).unwrap();
        let got = get(&conn, &id).unwrap().unwrap();
        assert!(got.last_copied_at_ms.is_some());
        assert!(got.last_used_at_ms.is_none());
        assert_eq!(got.copy_count, 1);
    }

    #[test]
    fn record_use_sets_last_used() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        record_use(&conn, &id).unwrap();
        let got = get(&conn, &id).unwrap().unwrap();
        assert!(got.last_used_at_ms.is_some());
        assert_eq!(got.use_count, 1);
    }

    #[test]
    fn record_seen_distinct_from_view() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        record_seen(&conn, &id).unwrap();
        let got = get(&conn, &id).unwrap().unwrap();
        assert!(got.last_seen_at_ms.is_some());
        assert!(got.last_viewed_at_ms.is_none(), "seen must NOT set last_viewed");
        assert_eq!(got.view_count, 0);
    }

    #[test]
    fn unused_duration_none_when_never_used() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        let got = get(&conn, &id).unwrap().unwrap();
        assert!(unused_duration_secs(&got).is_none());
    }

    #[test]
    fn unused_duration_computed_correctly() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        record_use(&conn, &id).unwrap();
        let got = get(&conn, &id).unwrap().unwrap();
        let d = unused_duration_secs(&got).unwrap();
        assert!(d <= 1, "just-used memory should have duration <= 1s, got {d}");
    }

    #[test]
    fn list_by_project_filters_correctly() {
        let conn = setup();
        let mut a = NewMemory::new(MemoryKind::Semantic, "a", "content A", "src");
        a.project_id = Some("ProjectA".to_string());
        create(&conn, &a).unwrap();
        let mut b = NewMemory::new(MemoryKind::Semantic, "b", "content B", "src");
        b.project_id = Some("ProjectB".to_string());
        create(&conn, &b).unwrap();
        let list_a = list_by_project(&conn, "ProjectA", 10).unwrap();
        let list_b = list_by_project(&conn, "ProjectB", 10).unwrap();
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].title, "a");
        assert_eq!(list_b.len(), 1);
        assert_eq!(list_b[0].title, "b");
    }

    #[test]
    fn soft_delete_filters_from_get() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        delete(&conn, &id).unwrap();
        let got = get(&conn, &id).unwrap();
        assert!(got.is_none(), "soft-deleted memory must not be returned");
    }

    #[test]
    fn hard_delete_removes_row() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        hard_delete(&conn, &id).unwrap();
        let got = get(&conn, &id).unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn list_unused_since_returns_correct_set() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        let id = create(&conn, &m).unwrap();
        record_use(&conn, &id).unwrap();
        // Cutoff in the future: the just-used memory (used at "now")
        // has last_used_at_ms < now + 60s, so it should appear in the list.
        let future = chrono::Utc::now().timestamp_millis() + 60_000;
        let list = list_unused_since(&conn, future, 10).unwrap();
        assert_eq!(list.len(), 1);
        // Cutoff in the past: nothing has been unused for negative time.
        let past = chrono::Utc::now().timestamp_millis() - 60_000;
        let list = list_unused_since(&conn, past, 10).unwrap();
        assert!(list.is_empty());
    }

    // ─────────── update tests ───────────

    #[test]
    fn update_preserves_id_and_relationships() {
        let conn = setup();
        let m = NewMemory::new(MemoryKind::Semantic, "original", "body", "src");
        let id = create(&conn, &m).unwrap();

        // Create a relationship before the update.
        let other = create(&conn, &NewMemory::new(MemoryKind::Semantic, "other", "x", "src")).unwrap();
        super::relationship::create(
            &conn, &id, &other, RelationshipType::RelatedTo, 0.9, Some("linked"), true,
        ).unwrap();

        let patch = MemoryUpdate {
            title: Some("updated".to_string()),
            ..Default::default()
        };
        let after = update(&conn, &id, &patch).unwrap();
        assert_eq!(after.id, id, "id must be preserved");
        assert_eq!(after.title, "updated");
        let rels = super::relationship::list_all(&conn, &id).unwrap();
        assert_eq!(rels.len(), 1, "relationship must survive update");
        assert_eq!(rels[0].to_id, other);
    }

    #[test]
    fn update_changes_privacy_and_sensitivity() {
        let conn = setup();
        let id = create(&conn, &NewMemory::new(MemoryKind::Semantic, "t", "c", "s")).unwrap();

        let patch = MemoryUpdate {
            privacy_level: Some(PrivacyLevel::Private),
            sensitivity: Some(7),
            ..Default::default()
        };
        let after = update(&conn, &id, &patch).unwrap();
        assert_eq!(after.privacy_level, "private");
        assert_eq!(after.sensitivity, 7);
    }

    #[test]
    fn update_changes_kind() {
        let conn = setup();
        let id = create(&conn, &NewMemory::new(MemoryKind::Semantic, "t", "c", "s")).unwrap();
        let patch = MemoryUpdate {
            kind: Some(MemoryKind::Document),
            ..Default::default()
        };
        let after = update(&conn, &id, &patch).unwrap();
        assert_eq!(after.kind, "document");
    }

    #[test]
    fn update_preserves_timestamps() {
        let conn = setup();
        let id = create(&conn, &NewMemory::new(MemoryKind::Semantic, "t", "c", "s")).unwrap();
        let before = get(&conn, &id).unwrap().unwrap();
        record_view(&conn, &id).unwrap();
        let after_view = get(&conn, &id).unwrap().unwrap();
        // view should set last_viewed_at_ms but not last_used_at_ms
        assert!(after_view.last_viewed_at_ms.is_some());
        assert!(after_view.last_used_at_ms.is_none());

        // Now perform an update
        let patch = MemoryUpdate { title: Some("t2".to_string()), ..Default::default() };
        let after_update = update(&conn, &id, &patch).unwrap();

        // Timestamps must be preserved
        assert_eq!(after_update.last_viewed_at_ms, after_view.last_viewed_at_ms);
        assert_eq!(after_update.view_count, after_view.view_count);
        assert_eq!(after_update.created_at_ms, before.created_at_ms);
        // updated_at_ms should advance
        assert!(after_update.updated_at_ms >= after_view.updated_at_ms);
    }

    #[test]
    fn update_tags_replace() {
        let conn = setup();
        let mut m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        m.tags = vec!["old1".to_string(), "old2".to_string()];
        let id = create(&conn, &m).unwrap();
        let patch = MemoryUpdate { tags: Some(vec!["new1".to_string()]), ..Default::default() };
        let after = update(&conn, &id, &patch).unwrap();
        assert_eq!(after.tags, vec!["new1"]);
    }

    #[test]
    fn update_explicit_none_clears_field() {
        let conn = setup();
        let mut m = NewMemory::new(MemoryKind::Semantic, "t", "c", "s");
        m.project_id = Some("OldProject".to_string());
        let id = create(&conn, &m).unwrap();
        let patch = MemoryUpdate { project_id: Some(None), ..Default::default() };
        let after = update(&conn, &id, &patch).unwrap();
        assert!(after.project_id.is_none(), "explicit None must clear");
    }

    #[test]
    fn update_missing_memory_errors() {
        let conn = setup();
        let patch = MemoryUpdate { title: Some("x".to_string()), ..Default::default() };
        let res = update(&conn, "mem-does-not-exist", &patch);
        assert!(res.is_err());
    }

    #[test]
    fn update_deleted_memory_errors() {
        let conn = setup();
        let id = create(&conn, &NewMemory::new(MemoryKind::Semantic, "t", "c", "s")).unwrap();
        delete(&conn, &id).unwrap();
        let patch = MemoryUpdate { title: Some("x".to_string()), ..Default::default() };
        let res = update(&conn, &id, &patch);
        assert!(res.is_err(), "cannot update soft-deleted memory");
    }

    #[test]
    fn update_changes_visible_in_search() {
        let conn = setup();
        let id = create(&conn, &NewMemory::new(MemoryKind::Semantic, "alpha", "content about aardvark", "src")).unwrap();
        let q = super::SearchQuery {
            text: "zebra".to_string(),
            kind: None, project: None, app: None,
            url: None, file: None, session: None, category: None,
            tags: Vec::new(), since_ms: None, until_ms: None,
            limit: 10, offset: 0,
        };
        let before = search_fn(&conn, &q).unwrap();
        assert!(before.is_empty(), "should not find 'zebra' before update");

        let patch = MemoryUpdate { content: Some("zebra with stripes".to_string()), ..Default::default() };
        let _ = update(&conn, &id, &patch).unwrap();
        let after = search_fn(&conn, &q).unwrap();
        assert_eq!(after.len(), 1, "should find 'zebra' after content update");
        assert_eq!(after[0].memory.id, id);
    }

    #[test]
    fn search_supports_offset_pagination() {
        let conn = setup();
        // Create 5 memories.
        for i in 0..5 {
            let m = NewMemory::new(MemoryKind::Semantic, format!("Pagination {i}"), "page body", "src");
            create(&conn, &m).unwrap();
        }
        let mut q = super::SearchQuery {
            text: "page".to_string(),
            kind: None, project: None, app: None,
            url: None, file: None, session: None, category: None,
            tags: Vec::new(), since_ms: None, until_ms: None,
            limit: 2, offset: 0,
        };
        let page1 = search_fn(&conn, &q).unwrap();
        assert_eq!(page1.len(), 2, "first page should have 2 results");
        q.offset = 2;
        let page2 = search_fn(&conn, &q).unwrap();
        assert_eq!(page2.len(), 2);
        q.offset = 4;
        let page3 = search_fn(&conn, &q).unwrap();
        assert_eq!(page3.len(), 1, "last page has the 5th result");
        // Pages are disjoint.
        let mut ids: std::collections::HashSet<_> = Default::default();
        for p in [&page1, &page2, &page3] {
            for h in p {
                assert!(ids.insert(h.memory.id.clone()), "duplicate across pages");
            }
        }
        assert_eq!(ids.len(), 5);
    }
}
