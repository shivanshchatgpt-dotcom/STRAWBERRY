//! 🧠 Unified Temporal Memory — the durable knowledge layer for Strawberry.
//!
//! Memory is derived selectively from meaningful events and artifacts.
//! NOT everything becomes memory — the system distinguishes:
//! - EVENT ≠ MEMORY
//! - Memory is curated, revisable, and goal-laden
//! - Events are raw, immutable, and high-volume
//!
//! Memory types:
//! - WORKING: short-term context (current session)
//! - EPISODIC: what happened (past events worth remembering)
//! - SEMANTIC: facts and knowledge (things learned)
//! - PROJECT: project-specific knowledge
//! - PROCEDURAL: skill knowledge (how to do things)

use serde::{Deserialize, Serialize};

/// Memory types with distinct storage and retrieval semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Short-term context (current session, recent activity).
    Working,
    /// What happened — events worth remembering.
    Episodic,
    /// Facts and knowledge — things learned.
    Semantic,
    /// Project-specific knowledge.
    Project,
    /// Procedural / skill knowledge — how to do things.
    Procedural,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Working => write!(f, "working"),
            Self::Episodic => write!(f, "episodic"),
            Self::Semantic => write!(f, "semantic"),
            Self::Project => write!(f, "project"),
            Self::Procedural => write!(f, "procedural"),
        }
    }
}

/// How important this memory is (affects retention and retrieval ranking).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for Importance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// A single memory record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique memory ID (UUID).
    pub id: String,

    /// Memory type (working, episodic, semantic, project, procedural).
    pub memory_type: MemoryType,

    /// Short title / summary of this memory.
    pub title: String,

    /// Full content / body of the memory.
    pub content: String,

    /// Source of this memory (which subsystem produced it).
    pub source: String,

    /// Optional reference to the source domain object.
    pub source_ref: Option<String>,

    /// Importance level.
    pub importance: Importance,

    /// Confidence score (0.0–1.0).
    pub confidence: f32,

    /// When the remembered event occurred (unix ms).
    pub occurred_at_ms: i64,

    /// When this memory was created (unix ms).
    pub created_at_ms: i64,

    /// When this memory was last updated (unix ms).
    pub updated_at_ms: i64,

    /// Optional project context.
    pub project_id: Option<String>,

    /// Optional session context.
    pub session_id: Option<String>,

    /// Tags for categorization and search.
    pub tags: Option<String>,

    /// Whether this memory has been verified / confirmed.
    pub verified: bool,

    /// Whether this memory is stale / outdated.
    pub stale: bool,

    /// Retention period in days (None = forever).
    pub retention_days: Option<i64>,
}

impl Memory {
    /// Create a new memory with sensible defaults.
    pub fn new(
        memory_type: MemoryType,
        title: impl Into<String>,
        content: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type,
            title: title.into(),
            content: content.into(),
            source: source.into(),
            source_ref: None,
            importance: Importance::Medium,
            confidence: 1.0,
            occurred_at_ms: now_ms,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            project_id: None,
            session_id: None,
            tags: None,
            verified: false,
            stale: false,
            retention_days: None,
        }
    }

    /// Builder: set importance.
    pub fn with_importance(mut self, i: Importance) -> Self {
        self.importance = i;
        self
    }

    /// Builder: set confidence.
    pub fn with_confidence(mut self, c: f32) -> Self {
        self.confidence = c.clamp(0.0, 1.0);
        self
    }

    /// Builder: set source reference.
    pub fn with_source_ref(mut self, r: impl Into<String>) -> Self {
        self.source_ref = Some(r.into());
        self
    }

    /// Builder: set project context.
    pub fn with_project(mut self, p: impl Into<String>) -> Self {
        self.project_id = Some(p.into());
        self
    }

    /// Builder: set session context.
    pub fn with_session(mut self, s: impl Into<String>) -> Self {
        self.session_id = Some(s.into());
        self
    }

    /// Builder: set tags.
    pub fn with_tags(mut self, t: impl Into<String>) -> Self {
        self.tags = Some(t.into());
        self
    }

    /// Builder: set retention days.
    pub fn with_retention_days(mut self, d: i64) -> Self {
        self.retention_days = Some(d);
        self
    }

    /// Builder: set occurred_at explicitly.
    pub fn with_occurred_at(mut self, ms: i64) -> Self {
        self.occurred_at_ms = ms;
        self
    }

    /// Mark this memory as stale.
    pub fn mark_stale(&mut self) {
        self.stale = true;
        self.updated_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_episodic_memory() {
        let m = Memory::new(
            MemoryType::Episodic,
            "Fixed the login bug",
            "The issue was a race condition in the token refresh handler.",
            "ghost",
        );
        assert_eq!(m.memory_type, MemoryType::Episodic);
        assert_eq!(m.title, "Fixed the login bug");
        assert_eq!(m.source, "ghost");
        assert_eq!(m.importance, Importance::Medium);
        assert_eq!(m.confidence, 1.0);
        assert!(!m.stale);
        assert!(!m.verified);
    }

    #[test]
    fn builder_chain() {
        let m = Memory::new(
            MemoryType::Semantic,
            "Rust ownership rules",
            "Values have one owner at a time.",
            "research",
        )
        .with_importance(Importance::High)
        .with_confidence(0.9)
        .with_source_ref("chat-123")
        .with_project("root-456")
        .with_session("session-789")
        .with_tags("rust,ownership")
        .with_retention_days(365)
        .with_occurred_at(1234567890);

        assert_eq!(m.importance, Importance::High);
        assert_eq!(m.confidence, 0.9);
        assert_eq!(m.source_ref.as_deref(), Some("chat-123"));
        assert_eq!(m.project_id.as_deref(), Some("root-456"));
        assert_eq!(m.session_id.as_deref(), Some("session-789"));
        assert_eq!(m.tags.as_deref(), Some("rust,ownership"));
        assert_eq!(m.retention_days, Some(365));
        assert_eq!(m.occurred_at_ms, 1234567890);
    }

    #[test]
    fn mark_stale() {
        let mut m = Memory::new(
            MemoryType::Working,
            "Current task",
            "Working on login",
            "system",
        );
        assert!(!m.stale);
        m.mark_stale();
        assert!(m.stale);
    }

    #[test]
    fn memory_type_display() {
        assert_eq!(MemoryType::Working.to_string(), "working");
        assert_eq!(MemoryType::Procedural.to_string(), "procedural");
    }

    #[test]
    fn importance_display() {
        assert_eq!(Importance::Low.to_string(), "low");
        assert_eq!(Importance::Critical.to_string(), "critical");
    }

    #[test]
    fn confidence_clamped() {
        let m = Memory::new(MemoryType::Episodic, "t", "c", "s")
            .with_confidence(5.0);
        assert_eq!(m.confidence, 1.0);
    }

    #[test]
    fn serialization_roundtrip() {
        let m = Memory::new(
            MemoryType::Semantic,
            "Test memory",
            "Content here",
            "test",
        )
        .with_importance(Importance::High);

        let json = serde_json::to_string(&m).unwrap();
        let back: Memory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, m.id);
        assert_eq!(back.memory_type, MemoryType::Semantic);
        assert_eq!(back.importance, Importance::High);
    }
}
