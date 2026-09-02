//! 📡 Canonical Event Model — the unified event representation for Strawberry.
//!
//! Every event source (clipboard, screen, tabs, files, Git, chats, calendar,
//! tasks, workspace, agent actions) normalizes into this model before
//! persistence or bus publication.
//!
//! DESIGN RULES:
//! - Events are immutable once created
//! - Events carry provenance (source + adapter version)
//! - Events carry privacy classification
//! - Events carry deduplication keys
//! - Events do NOT contain full domain records — use `reference` for that
//! - The inline `payload` is small metadata; large data goes to domain tables

use serde::{Deserialize, Serialize};

// ─── Event Identity ─────────────────────────────────────────────────────────

/// Unique event identifier (UUID text).
pub type EventId = String;

// ─── Event Roles ────────────────────────────────────────────────────────────

/// The role an event plays in the system. Different roles have different
/// retention and processing semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventRole {
    /// Raw occurrence — append-only, time-retained.
    Raw,
    /// Normalized/filtered observation — append-only, deduped.
    Observation,
    /// Kept representation — user-visible, revisable, deletable.
    Memory,
    /// Derived finding — regenerable, score-bearing.
    Insight,
    /// Actionable unit — lifecycle-owned.
    Task,
    /// Taken step — audited, actor-gated.
    Action,
}

impl std::fmt::Display for EventRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "raw"),
            Self::Observation => write!(f, "observation"),
            Self::Memory => write!(f, "memory"),
            Self::Insight => write!(f, "insight"),
            Self::Task => write!(f, "task"),
            Self::Action => write!(f, "action"),
        }
    }
}

// ─── Privacy Classification ─────────────────────────────────────────────────

/// Privacy level for an event. Determines whether it can enter cloud
/// requests, memory, search, or reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    /// Safe for all processing and storage.
    Public,
    /// Contains sensitive data — requires redaction before cloud/reports.
    Sensitive,
    /// Contains secrets — must never leave the local machine.
    Secret,
}

impl std::fmt::Display for PrivacyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Sensitive => write!(f, "sensitive"),
            Self::Secret => write!(f, "secret"),
        }
    }
}

// ─── Retention ──────────────────────────────────────────────────────────────

/// How long an event should be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    /// Keep for a short period (e.g. 7 days).
    Short,
    /// Keep for a medium period (e.g. 90 days).
    Medium,
    /// Keep indefinitely.
    Permanent,
    /// Keep until explicitly deleted.
    UserManaged,
}

impl std::fmt::Display for RetentionClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Short => write!(f, "short"),
            Self::Medium => write!(f, "medium"),
            Self::Permanent => write!(f, "permanent"),
            Self::UserManaged => write!(f, "user_managed"),
        }
    }
}

// ─── Actor ──────────────────────────────────────────────────────────────────

/// Who or what initiated this event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    /// The human user.
    User,
    /// The system (deterministic processing).
    System,
    /// An agent with a specific ID.
    Agent(String),
}

impl std::fmt::Display for Actor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::System => write!(f, "system"),
            Self::Agent(id) => write!(f, "agent:{id}"),
        }
    }
}

// ─── Canonical Event ────────────────────────────────────────────────────────

/// The canonical event representation. Every event source normalizes into
/// this struct before persistence or bus publication.
///
/// Fields are designed to be:
/// - Compact (small inline payload, references for large data)
/// - Queryable (indexed fields for filtering)
/// - Safe (no secrets in payload, privacy level controls downstream)
/// - Auditable (provenance chain, dedup key, actor)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalEvent {
    /// Unique event identifier (UUID).
    pub id: EventId,

    /// The role this event plays (raw, observation, memory, etc.).
    pub role: EventRole,

    /// Namespaced event type, e.g. "clipboard.captured", "file.modified",
    /// "calendar.event.created", "ghost.insight.generated".
    pub event_type: String,

    /// Which subsystem produced this event (e.g. "clipboard", "screen",
    /// "ghost", "calendar", "agent").
    pub source: String,

    /// Adapter version that produced this event (for future migration).
    pub source_version: String,

    /// Optional reference to a domain object (chat_id, event_id, frame_id).
    /// The event does NOT contain the full object — use this to look it up.
    pub reference: Option<String>,

    /// The actor who initiated this event.
    pub actor: Actor,

    /// When the event actually happened (unix ms).
    pub occurred_at_ms: i64,

    /// When the event was created/recorded (unix ms).
    pub created_at_ms: i64,

    /// Optional project context (root_id or future project_id).
    pub project_id: Option<String>,

    /// Optional session context.
    pub session_id: Option<String>,

    /// Small inline payload as JSON. Large data stays in domain tables.
    pub payload: Option<String>,

    /// Privacy classification.
    pub privacy_level: PrivacyLevel,

    /// Deduplication key — events with the same key within a time window
    /// should not be duplicated.
    pub dedupe_key: Option<String>,

    /// Retention policy.
    pub retention: RetentionClass,

    /// Confidence score (0.0–1.0). Default 1.0 for deterministic events.
    pub confidence: f32,

    /// Provenance chain: how this event was derived (e.g. "clipboard→adapter→bus").
    pub provenance: Option<String>,
}

impl CanonicalEvent {
    /// Create a new canonical event with sensible defaults.
    pub fn new(
        role: EventRole,
        event_type: impl Into<String>,
        source: impl Into<String>,
        actor: Actor,
    ) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            event_type: event_type.into(),
            source: source.into(),
            source_version: "1.0".into(),
            reference: None,
            actor,
            occurred_at_ms: now_ms,
            created_at_ms: now_ms,
            project_id: None,
            session_id: None,
            payload: None,
            privacy_level: PrivacyLevel::Public,
            dedupe_key: None,
            retention: RetentionClass::Medium,
            confidence: 1.0,
            provenance: None,
        }
    }

    /// Builder: set the reference to a domain object.
    pub fn with_reference(mut self, r: impl Into<String>) -> Self {
        self.reference = Some(r.into());
        self
    }

    /// Builder: set the project context.
    pub fn with_project(mut self, p: impl Into<String>) -> Self {
        self.project_id = Some(p.into());
        self
    }

    /// Builder: set the session context.
    pub fn with_session(mut self, s: impl Into<String>) -> Self {
        self.session_id = Some(s.into());
        self
    }

    /// Builder: set the inline payload.
    pub fn with_payload(mut self, p: impl Into<String>) -> Self {
        self.payload = Some(p.into());
        self
    }

    /// Builder: set privacy level.
    pub fn with_privacy(mut self, p: PrivacyLevel) -> Self {
        self.privacy_level = p;
        self
    }

    /// Builder: set deduplication key.
    pub fn with_dedupe(mut self, k: impl Into<String>) -> Self {
        self.dedupe_key = Some(k.into());
        self
    }

    /// Builder: set retention class.
    pub fn with_retention(mut self, r: RetentionClass) -> Self {
        self.retention = r;
        self
    }

    /// Builder: set provenance.
    pub fn with_provenance(mut self, p: impl Into<String>) -> Self {
        self.provenance = Some(p.into());
        self
    }

    /// Builder: set confidence.
    pub fn with_confidence(mut self, c: f32) -> Self {
        self.confidence = c.clamp(0.0, 1.0);
        self
    }

    /// Builder: set source version.
    pub fn with_source_version(mut self, v: impl Into<String>) -> Self {
        self.source_version = v.into();
        self
    }

    /// Builder: set occurred_at explicitly.
    pub fn with_occurred_at(mut self, ms: i64) -> Self {
        self.occurred_at_ms = ms;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_minimal_event() {
        let ev = CanonicalEvent::new(
            EventRole::Raw,
            "clipboard.captured",
            "daemon",
            Actor::User,
        );
        assert_eq!(ev.role, EventRole::Raw);
        assert_eq!(ev.event_type, "clipboard.captured");
        assert_eq!(ev.source, "daemon");
        assert_eq!(ev.actor, Actor::User);
        assert_eq!(ev.confidence, 1.0);
        assert_eq!(ev.privacy_level, PrivacyLevel::Public);
        assert_eq!(ev.retention, RetentionClass::Medium);
        assert!(!ev.id.is_empty());
    }

    #[test]
    fn builder_chain() {
        let ev = CanonicalEvent::new(
            EventRole::Observation,
            "file.modified",
            "file_watcher",
            Actor::System,
        )
        .with_reference("chat-123")
        .with_project("root-456")
        .with_session("session-789")
        .with_payload(r#"{"path":"/tmp/foo.rs"}"#)
        .with_privacy(PrivacyLevel::Sensitive)
        .with_dedupe("file:/tmp/foo.rs:1234567890")
        .with_retention(RetentionClass::Permanent)
        .with_provenance("notify→adapter→bus")
        .with_confidence(0.95)
        .with_source_version("2.0");

        assert_eq!(ev.reference.as_deref(), Some("chat-123"));
        assert_eq!(ev.project_id.as_deref(), Some("root-456"));
        assert_eq!(ev.session_id.as_deref(), Some("session-789"));
        assert_eq!(ev.privacy_level, PrivacyLevel::Sensitive);
        assert_eq!(ev.dedupe_key.as_deref(), Some("file:/tmp/foo.rs:1234567890"));
        assert_eq!(ev.retention, RetentionClass::Permanent);
        assert_eq!(ev.confidence, 0.95);
        assert_eq!(ev.source_version, "2.0");
    }

    #[test]
    fn event_role_display() {
        assert_eq!(EventRole::Raw.to_string(), "raw");
        assert_eq!(EventRole::Action.to_string(), "action");
    }

    #[test]
    fn privacy_level_display() {
        assert_eq!(PrivacyLevel::Public.to_string(), "public");
        assert_eq!(PrivacyLevel::Secret.to_string(), "secret");
    }

    #[test]
    fn actor_display() {
        assert_eq!(Actor::User.to_string(), "user");
        assert_eq!(Actor::System.to_string(), "system");
        assert_eq!(Actor::Agent("research-1".into()).to_string(), "agent:research-1");
    }

    #[test]
    fn confidence_clamped() {
        let ev = CanonicalEvent::new(
            EventRole::Raw,
            "test",
            "test",
            Actor::System,
        )
        .with_confidence(5.0);
        assert_eq!(ev.confidence, 1.0);

        let ev2 = CanonicalEvent::new(
            EventRole::Raw,
            "test",
            "test",
            Actor::System,
        )
        .with_confidence(-1.0);
        assert_eq!(ev2.confidence, 0.0);
    }

    #[test]
    fn serialization_roundtrip() {
        let ev = CanonicalEvent::new(
            EventRole::Observation,
            "ghost.insight.generated",
            "ghost",
            Actor::System,
        )
        .with_reference("insight-42")
        .with_privacy(PrivacyLevel::Public);

        let json = serde_json::to_string(&ev).unwrap();
        let back: CanonicalEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, ev.id);
        assert_eq!(back.role, EventRole::Observation);
        assert_eq!(back.event_type, "ghost.insight.generated");
    }
}
