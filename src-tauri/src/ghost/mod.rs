//! 👻 The Strawberry Ghost — a parallel AI that watches, learns, and surfaces
//! hidden connections in your knowledge tree.
//!
//! Architecture:
//! - **Tracker**: records every meaningful user action (open, create, search, capture).
//! - **Graph**: builds a knowledge graph from chats/folders/roots/tags with edges
//!   for shared tags, co-access, parent-child, and temporal proximity.
//! - **Insights**: surfaces serendipities, patterns, resurfaces, clusters, warnings.
//! - **Attention**: tracks time spent on each source to build an attention heatmap.
//!
//! The Ghost never modifies the user's data. It only reads + records + suggests.

pub mod tracker;
pub mod graph;
pub mod insights;
pub mod attention;

use serde::{Deserialize, Serialize};

/// Kinds of events the Ghost can record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    OpenChat,
    OpenFolder,
    OpenRoot,
    CreateChat,
    CreateFolder,
    Search,
    Capture,
    TodoAdd,
    TodoDone,
    HabitDone,
    FocusSession,
    TabVisit,
    NoteView,
    InboxAdd,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenChat => "open_chat",
            Self::OpenFolder => "open_folder",
            Self::OpenRoot => "open_root",
            Self::CreateChat => "create_chat",
            Self::CreateFolder => "create_folder",
            Self::Search => "search",
            Self::Capture => "capture",
            Self::TodoAdd => "todo_add",
            Self::TodoDone => "todo_done",
            Self::HabitDone => "habit_done",
            Self::FocusSession => "focus_session",
            Self::TabVisit => "tab_visit",
            Self::NoteView => "note_view",
            Self::InboxAdd => "inbox_add",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostEvent {
    pub id: i64,
    pub event_type: String,
    pub source_id: Option<String>,
    pub source_kind: Option<String>,
    pub duration_ms: i64,
    pub metadata: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub weight: f64,
    pub color: Option<String>,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub id: i64,
    pub source_id: String,
    pub target_id: String,
    pub weight: f64,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostInsight {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub source_ids: Option<String>,
    pub score: f64,
    pub seen: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionCell {
    /// Day of week (0=Mon, 6=Sun)
    pub day: u8,
    /// Hour of day (0-23)
    pub hour: u8,
    /// Number of events in that bucket
    pub count: i64,
    /// Total duration_ms in that bucket
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostStats {
    pub total_events: i64,
    pub total_chats: i64,
    pub total_folders: i64,
    pub total_insights: i64,
    pub graph_nodes: i64,
    pub graph_edges: i64,
    pub most_visited: Vec<(String, String, i64)>, // (id, label, count)
    pub top_tags: Vec<(String, i64)>,              // (tag, count)
    pub streak_days: i64,
    pub peak_hour: Option<u8>,
    pub peak_day: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostSnapshot {
    pub stats: GhostStats,
    pub graph: Graph,
    pub insights: Vec<GhostInsight>,
    pub heatmap: Vec<AttentionCell>,
    pub recent_events: Vec<GhostEvent>,
}

/// Run a full ghost cycle (rebuild graph + regenerate insights) on a
/// **dedicated** connection. This avoids holding the `AppState` connection
/// lock for the duration of the rebuild, which would freeze the app every
/// 5 minutes.
pub fn run_cycle_offline(
    db_path: &std::path::Path,
    shutdown: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    // Reasonable busy timeout for writes that may briefly conflict.
    conn.busy_timeout(std::time::Duration::from_secs(2)).ok();
    graph::rebuild(&conn)?;
    // Check shutdown between the two heavy operations.
    if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    insights::regenerate(&conn)?;
    Ok(())
}
