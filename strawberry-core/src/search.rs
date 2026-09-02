//! 🔍 Universal Search — federated search across all Strawberry data sources.
//!
//! Search is a federated query surface, NOT one giant duplicated index.
//! It fans out to existing domain indexes and combines results.
//!
//! Sources searched:
//! - chats (chat_fts)
//! - memory (unified_memories)
//! - tasks (todos)
//! - calendar (events)
//! - clipboard (chats with source='capture')
//! - screen memory (screen_fts)
//! - sessions (workspace_sessions)
//! - ghost insights (ghost_insights)

use serde::{Deserialize, Serialize};

/// Which source a search result came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    Chats,
    Memory,
    Tasks,
    Calendar,
    Clipboard,
    Screens,
    Sessions,
    Ghost,
    Files,
    Git,
}

impl std::fmt::Display for SearchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chats => write!(f, "chats"),
            Self::Memory => write!(f, "memory"),
            Self::Tasks => write!(f, "tasks"),
            Self::Calendar => write!(f, "calendar"),
            Self::Clipboard => write!(f, "clipboard"),
            Self::Screens => write!(f, "screens"),
            Self::Sessions => write!(f, "sessions"),
            Self::Ghost => write!(f, "ghost"),
            Self::Files => write!(f, "files"),
            Self::Git => write!(f, "git"),
        }
    }
}

/// A single search result from any source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Unique result identifier.
    pub id: String,

    /// Which source this result came from.
    pub source: SearchSource,

    /// Title / heading of the result.
    pub title: String,

    /// Snippet / preview of matching content.
    pub snippet: String,

    /// Relevance score (0.0–1.0).
    pub score: f32,

    /// When the underlying item was created (unix ms).
    pub created_at_ms: i64,

    /// Optional reference to the domain object.
    pub reference: Option<String>,

    /// Optional project context.
    pub project_id: Option<String>,

    /// Source-specific metadata (JSON).
    pub metadata: Option<String>,
}

/// Search query parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// The search text.
    pub query: String,

    /// Filter to specific sources (empty = all sources).
    pub sources: Vec<SearchSource>,

    /// Filter to specific project.
    pub project_id: Option<String>,

    /// Maximum results per source.
    pub limit_per_source: usize,

    /// Maximum total results.
    pub total_limit: usize,

    /// Whether to boost current-session results.
    pub boost_current_session: bool,

    /// Current session ID (for boosting).
    pub current_session_id: Option<String>,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            sources: Vec::new(),
            project_id: None,
            limit_per_source: 10,
            total_limit: 50,
            boost_current_session: false,
            current_session_id: None,
        }
    }
}

impl SearchQuery {
    /// Create a simple text search across all sources.
    pub fn text(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Default::default()
        }
    }

    /// Builder: filter to specific sources.
    pub fn with_sources(mut self, sources: Vec<SearchSource>) -> Self {
        self.sources = sources;
        self
    }

    /// Builder: filter to specific project.
    pub fn with_project(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    /// Builder: set limits.
    pub fn with_limits(mut self, per_source: usize, total: usize) -> Self {
        self.limit_per_source = per_source;
        self.total_limit = total;
        self
    }

    /// Builder: boost current session.
    pub fn with_session_boost(mut self, session_id: impl Into<String>) -> Self {
        self.boost_current_session = true;
        self.current_session_id = Some(session_id.into());
        self
    }
}

/// Aggregated search results from all sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// The query that produced these results.
    pub query: String,

    /// All results, sorted by relevance.
    pub results: Vec<SearchResult>,

    /// Total results before limiting.
    pub total_before_limit: usize,

    /// How many sources were queried.
    pub sources_queried: usize,

    /// How many sources returned results.
    pub sources_with_results: usize,

    /// Search duration in milliseconds.
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_defaults() {
        let q = SearchQuery::default();
        assert!(q.query.is_empty());
        assert!(q.sources.is_empty());
        assert_eq!(q.limit_per_source, 10);
        assert_eq!(q.total_limit, 50);
    }

    #[test]
    fn search_query_builder() {
        let q = SearchQuery::text("rust ownership")
            .with_sources(vec![SearchSource::Chats, SearchSource::Memory])
            .with_project("root-1")
            .with_limits(5, 20)
            .with_session_boost("session-abc");

        assert_eq!(q.query, "rust ownership");
        assert_eq!(q.sources.len(), 2);
        assert_eq!(q.project_id.as_deref(), Some("root-1"));
        assert_eq!(q.limit_per_source, 5);
        assert!(q.boost_current_session);
    }

    #[test]
    fn search_source_display() {
        assert_eq!(SearchSource::Chats.to_string(), "chats");
        assert_eq!(SearchSource::Git.to_string(), "git");
    }

    #[test]
    fn search_result_creation() {
        let r = SearchResult {
            id: "r1".into(),
            source: SearchSource::Memory,
            title: "Rust ownership".into(),
            snippet: "Values have one owner...".into(),
            score: 0.95,
            created_at_ms: 1234567890,
            reference: Some("mem-42".into()),
            project_id: None,
            metadata: None,
        };
        assert_eq!(r.source, SearchSource::Memory);
        assert_eq!(r.score, 0.95);
    }

    #[test]
    fn serialization_roundtrip() {
        let q = SearchQuery::text("test").with_sources(vec![SearchSource::Chats]);
        let json = serde_json::to_string(&q).unwrap();
        let back: SearchQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(back.query, "test");
        assert_eq!(back.sources.len(), 1);
    }
}
