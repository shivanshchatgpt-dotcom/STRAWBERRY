//! World State — compact, strongly-typed, incrementally-updated world model.
//!
//! Per the architecture:
//!   * Use strongly typed Rust structs/enums (no JSON-y maps for hot fields).
//!   * Update incrementally — never rebuild the whole state on every event.
//!   * Bounded capacity for `recent_*` lists.
//!   * Versioned for safe concurrent reads.
//!
//! The runtime queries the world state at every cycle to decide what to do.

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use super::ids::{AppId, FileId, ProjectId, TaskId, WindowId};

/// Bump every time the world state changes. Used for snapshotting and
/// detecting stale reads.
pub type WorldStateVersion = u64;

/// A bounded recent-item list. Oldest items drop off automatically.
pub const RECENT_LIMIT: usize = 20;
pub const ERROR_LIMIT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPhase {
    Idle,
    Coding,
    Reading,
    Searching,
    Debugging,
    Planning,
    Reviewing,
    Writing,
    Unknown,
}

impl Default for WorkflowPhase {
    fn default() -> Self { Self::Unknown }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildState {
    Unknown,
    Idle,
    Running,
    Succeeded,
    Failed { message: String },
}

impl Default for BuildState {
    fn default() -> Self { Self::Unknown }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TestState {
    Unknown,
    Idle,
    Running,
    Passed { count: u32 },
    Failed { passed: u32, failed: u32, message: Option<String> },
}

impl Default for TestState {
    fn default() -> Self { Self::Unknown }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskState {
    pub id: TaskId,
    pub title: String,
    pub priority: u8,
    pub completed: bool,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorState {
    pub message: String,
    pub source: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentFile {
    pub path: String,
    pub project: Option<String>,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentCommand {
    pub command: String,
    pub project: Option<String>,
    pub at_ms: i64,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceState {
    pub cpu_pct: f32,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    pub at_ms: i64,
}

impl Default for ResourceState {
    fn default() -> Self {
        Self { cpu_pct: 0.0, mem_used_mb: 0, mem_total_mb: 0, at_ms: 0 }
    }
}

/// The single source of truth for the agent's current environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    pub version: WorldStateVersion,
    pub updated_at_ms: i64,

    pub active_app: Option<String>,
    pub active_window_title: Option<String>,
    pub active_project: Option<String>,
    pub active_file: Option<RecentFile>,
    pub workflow_phase: WorkflowPhase,
    pub build_state: BuildState,
    pub test_state: TestState,

    pub active_tasks: Vec<TaskState>,
    pub recent_files: VecDeque<RecentFile>,
    pub recent_commands: VecDeque<RecentCommand>,
    pub recent_errors: VecDeque<ErrorState>,
    pub recent_app_switches: VecDeque<String>,
    pub recent_searches: VecDeque<String>,

    pub resource: ResourceState,

    pub last_action_at_ms: i64,
    pub last_action_id: Option<u64>,
    pub last_goal_id: Option<u64>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            version: 0,
            updated_at_ms: chrono::Utc::now().timestamp_millis(),
            active_app: None,
            active_window_title: None,
            active_project: None,
            active_file: None,
            workflow_phase: WorkflowPhase::Unknown,
            build_state: BuildState::Unknown,
            test_state: TestState::Unknown,
            active_tasks: Vec::new(),
            recent_files: VecDeque::with_capacity(RECENT_LIMIT),
            recent_commands: VecDeque::with_capacity(RECENT_LIMIT),
            recent_errors: VecDeque::with_capacity(ERROR_LIMIT),
            recent_app_switches: VecDeque::with_capacity(RECENT_LIMIT),
            recent_searches: VecDeque::with_capacity(RECENT_LIMIT),
            resource: ResourceState::default(),
            last_action_at_ms: 0,
            last_action_id: None,
            last_goal_id: None,
        }
    }

    /// Snapshot the current world state.
    pub fn snapshot(&self) -> Self {
        self.clone()
    }

    /// Time since last state update, in seconds.
    pub fn age_secs(&self) -> i64 {
        let now = chrono::Utc::now().timestamp_millis();
        (now - self.updated_at_ms) / 1000
    }

    /// True if a build error has been observed recently.
    pub fn has_recent_build_error(&self) -> bool {
        matches!(self.build_state, BuildState::Failed { .. })
    }

    /// True if tests recently failed.
    pub fn has_recent_test_failure(&self) -> bool {
        matches!(self.test_state, TestState::Failed { .. })
    }

    /// Number of errors accumulated in the recent window.
    pub fn error_count(&self) -> usize {
        self.recent_errors.len()
    }

    /// True if a goal is currently "fresh" (created within `secs`).
    pub fn recently_acted(&self, secs: i64) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        (now - self.last_action_at_ms) / 1000 < secs
    }
}

impl Default for WorldState {
    fn default() -> Self { Self::new() }
}

/// A diff describes the changes applied to a `WorldState`. Used for
/// observability and incremental journaling.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldStateDiff {
    pub from_version: WorldStateVersion,
    pub to_version: WorldStateVersion,
    pub changed_fields: Vec<String>,
    pub new_errors: usize,
    pub new_recent_files: usize,
    pub new_recent_commands: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_state_starts_clean() {
        let ws = WorldState::new();
        assert_eq!(ws.version, 0);
        assert!(ws.recent_files.is_empty());
        assert!(!ws.has_recent_build_error());
    }

    #[test]
    fn workflow_phase_default() {
        let ws = WorldState::new();
        assert_eq!(ws.workflow_phase, WorkflowPhase::Unknown);
    }

    #[test]
    fn build_failed_detection() {
        let mut ws = WorldState::new();
        ws.build_state = BuildState::Failed { message: "x".into() };
        assert!(ws.has_recent_build_error());
    }

    #[test]
    fn recent_files_bounded() {
        let mut ws = WorldState::new();
        for i in 0..50 {
            ws.recent_files.push_back(RecentFile {
                path: format!("f{i}.rs"),
                project: None,
                at_ms: 0,
            });
            while ws.recent_files.len() > RECENT_LIMIT {
                ws.recent_files.pop_front();
            }
        }
        assert_eq!(ws.recent_files.len(), RECENT_LIMIT);
    }
}
