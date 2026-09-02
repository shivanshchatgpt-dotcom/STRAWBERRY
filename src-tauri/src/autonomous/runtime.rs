//! Autonomy Runtime — the persistent, event-driven control loop.
//!
//! Responsibilities:
//!   * start / pause / resume / shutdown
//!   * consume events from the bus
//!   * apply them to the world state (incremental)
//!   * run a cycle (currently observe + update; later: goal, plan, score, gate, exec, verify, learn)
//!   * recover state on startup
//!
//! Phase 1 only wires observe + update. Later phases will plug in goal/planner/safety
//! behind the same `run_cycle` entry point.

use std::sync::Arc;
use std::time::Duration;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use super::cycle::{AutonomyCycle, CycleOutcome, CycleResult};
use super::event::{EventBus, EventKind, NormalizedEvent};
use super::world_state::{BuildState, TestState, WorkflowPhase, WorldState, RECENT_LIMIT};

/// Runtime control state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Stopped,
    Running,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStats {
    pub mode: RuntimeMode,
    pub cycles_total: u64,
    pub cycles_with_action: u64,
    pub events_consumed_total: u64,
    pub last_cycle_at_ms: i64,
    pub world_state_version: u64,
    pub uptime_secs: u64,
}

/// The autonomy runtime. Cheap to clone (internal Arc).
#[derive(Clone)]
pub struct AutonomyRuntime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    bus: EventBus,
    state: Mutex<WorldState>,
    mode: Mutex<RuntimeMode>,
    stats: Mutex<RuntimeStatsInner>,
    started_at_ms: i64,
}

#[derive(Debug, Clone, Default)]
struct RuntimeStatsInner {
    cycles_total: u64,
    cycles_with_action: u64,
    events_consumed_total: u64,
    last_cycle_at_ms: i64,
}

impl AutonomyRuntime {
    /// Create a new runtime with a fresh world state.
    pub fn new() -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            inner: Arc::new(RuntimeInner {
                bus: EventBus::default(),
                state: Mutex::new(WorldState::new()),
                mode: Mutex::new(RuntimeMode::Stopped),
                stats: Mutex::new(RuntimeStatsInner::default()),
                started_at_ms: now,
            }),
        }
    }

    /// Acquire the event bus for observers to push into.
    pub fn bus(&self) -> EventBus {
        self.inner.bus.clone()
    }

    /// Get a snapshot of the current world state.
    pub fn world_state(&self) -> WorldState {
        self.inner.state.lock().unwrap().snapshot()
    }

    /// Current runtime mode.
    pub fn mode(&self) -> RuntimeMode {
        *self.inner.mode.lock().unwrap()
    }

    /// Snapshot of runtime stats.
    pub fn stats(&self) -> RuntimeStats {
        let s = self.inner.stats.lock().unwrap().clone();
        let m = *self.inner.mode.lock().unwrap();
        let v = self.inner.state.lock().unwrap().version;
        let now = chrono::Utc::now().timestamp_millis();
        RuntimeStats {
            mode: m,
            cycles_total: s.cycles_total,
            cycles_with_action: s.cycles_with_action,
            events_consumed_total: s.events_consumed_total,
            last_cycle_at_ms: s.last_cycle_at_ms,
            world_state_version: v,
            uptime_secs: ((now - self.inner.started_at_ms) / 1000).max(0) as u64,
        }
    }

    /// Start the runtime. Idempotent.
    pub fn start(&self) {
        *self.inner.mode.lock().unwrap() = RuntimeMode::Running;
    }

    /// Pause the runtime. Cycles will not be run while paused.
    pub fn pause(&self) {
        *self.inner.mode.lock().unwrap() = RuntimeMode::Paused;
    }

    /// Resume from a paused state.
    pub fn resume(&self) {
        *self.inner.mode.lock().unwrap() = RuntimeMode::Running;
    }

    /// Shutdown cleanly. No further cycles will be run.
    pub fn shutdown(&self) {
        *self.inner.mode.lock().unwrap() = RuntimeMode::Stopped;
    }

    /// Publish a normalized event into the bus. Convenience for observers.
    pub fn publish(&self, ev: NormalizedEvent) {
        self.inner.bus.publish(ev);
    }

    /// Run a single autonomy cycle synchronously.
    ///
    /// Returns a `CycleResult` describing what happened.
    ///
    /// Phase 1 implementation:
    ///   1. Check mode (Stopped/Paused → return Pending)
    ///   2. Drain up to N events from the bus
    ///   3. Apply each event to the world state (incremental)
    ///   4. Bump world state version
    ///   5. Return cycle result with the new world state version
    ///
    /// Later phases (goal/planner/safety/exec/verify) hook in here.
    pub fn run_cycle(&self, max_events: usize) -> CycleResult {
        let mut cycle = AutonomyCycle::new(self.inner.state.lock().unwrap().version);

        if self.mode() != RuntimeMode::Running {
            cycle.outcome = CycleOutcome::Pending;
            self.bump_stats(0, false);
            return CycleResult::ok(cycle, "Runtime not running");
        }

        let events = self.inner.bus.drain(max_events);
        cycle.events_consumed = events.len();

        if events.is_empty() {
            // No-op cycle; just bump version periodically so age_secs stays small.
            self.touch_state();
            cycle.outcome = CycleOutcome::Pending;
        } else {
            for ev in &events {
                self.apply_event(ev);
            }
            cycle.outcome = CycleOutcome::Executed;
        }

        cycle.world_state_version = self.inner.state.lock().unwrap().version;
        let did_action = cycle.outcome == CycleOutcome::Executed;
        self.bump_stats(events.len(), did_action);
        CycleResult::ok(cycle, format!("Processed {} events", events.len()))
    }

    /// Apply a single normalized event to the world state.
    /// This is the heart of the "OBSERVE → NORMALIZE → UPDATE WORLD STATE" loop.
    fn apply_event(&self, ev: &NormalizedEvent) {
        let mut ws = self.inner.state.lock().unwrap();
        let now = ev.timestamp_ms;

        match &ev.kind {
            EventKind::ActiveAppChanged { from: _, to } => {
                if ws.active_app.as_deref() != Some(to.as_str()) {
                    push_bounded(&mut ws.recent_app_switches, to.clone(), RECENT_LIMIT);
                    ws.active_app = Some(to.clone());
                }
            }
            EventKind::FileOpened { path, project } => {
                let rf = super::world_state::RecentFile { path: path.clone(), project: project.clone(), at_ms: now };
                push_recent_file(&mut ws.recent_files, rf, RECENT_LIMIT);
                ws.active_file = Some(super::world_state::RecentFile { path: path.clone(), project: project.clone(), at_ms: now });
                ws.workflow_phase = infer_phase_from_extension(path);
                if let Some(p) = project {
                    ws.active_project = Some(p.clone());
                }
            }
            EventKind::FileModified { path, project } => {
                let rf = super::world_state::RecentFile { path: path.clone(), project: project.clone(), at_ms: now };
                push_recent_file(&mut ws.recent_files, rf, RECENT_LIMIT);
                ws.active_project = ws.active_project.clone().or_else(|| project.clone());
            }
            EventKind::ChatOpened { title, .. } => {
                ws.workflow_phase = WorkflowPhase::Reading;
                if let Some(p) = ws.active_project.clone() {
                    ws.recent_searches.push_back(format!("open:{}", title));
                    if ws.recent_searches.len() > RECENT_LIMIT { ws.recent_searches.pop_front(); }
                    let _ = p;
                }
            }
            EventKind::ChatCreated { title, .. } => {
                ws.workflow_phase = WorkflowPhase::Writing;
                ws.recent_searches.push_back(format!("create:{}", title));
                if ws.recent_searches.len() > RECENT_LIMIT { ws.recent_searches.pop_front(); }
            }
            EventKind::FolderOpened { name, .. } => {
                ws.active_project = Some(name.clone());
            }
            EventKind::SearchExecuted { query, .. } => {
                ws.workflow_phase = WorkflowPhase::Searching;
                ws.recent_searches.push_back(query.clone());
                if ws.recent_searches.len() > RECENT_LIMIT { ws.recent_searches.pop_front(); }
            }
            EventKind::BuildStateChanged { state, project } => {
                ws.build_state = parse_build_state(state);
                if let Some(p) = project { ws.active_project = Some(p.clone()); }
            }
            EventKind::TodoToggled { id, completed } => {
                if let Some(t) = ws.active_tasks.iter_mut().find(|t| t.id.0 == *id) {
                    t.completed = *completed;
                }
            }
            EventKind::FocusSessionChanged { state, .. } => {
                if state == "running" {
                    ws.workflow_phase = WorkflowPhase::Coding;
                } else {
                    ws.workflow_phase = WorkflowPhase::Idle;
                }
            }
            EventKind::TabVisited { title, .. } => {
                if let Some(t) = title {
                    ws.recent_app_switches.push_back(t.clone());
                    if ws.recent_app_switches.len() > RECENT_LIMIT { ws.recent_app_switches.pop_front(); }
                }
            }
            EventKind::InboxAdded { preview, .. } => {
                ws.recent_searches.push_back(format!("inbox:{}", preview));
                if ws.recent_searches.len() > RECENT_LIMIT { ws.recent_searches.pop_front(); }
            }
            EventKind::ScreenCaptured { app, .. } => {
                if let Some(a) = app { ws.active_app = Some(a.clone()); }
            }
            EventKind::WellnessBreak { .. } => {
                ws.workflow_phase = WorkflowPhase::Idle;
            }
            EventKind::Heartbeat { .. } => { /* no-op */ }
            EventKind::ErrorObserved { message, source } => {
                ws.recent_errors.push_back(super::world_state::ErrorState {
                    message: message.clone(),
                    source: source.clone(),
                    at_ms: now,
                });
                if ws.recent_errors.len() > 10 { ws.recent_errors.pop_front(); }
                if source == "build" {
                    ws.build_state = BuildState::Failed { message: message.clone() };
                } else if source == "test" {
                    ws.test_state = TestState::Failed { passed: 0, failed: 1, message: Some(message.clone()) };
                }
            }
        }

        ws.version += 1;
        ws.updated_at_ms = now;
    }

    fn touch_state(&self) {
        let mut ws = self.inner.state.lock().unwrap();
        ws.version += 1;
        ws.updated_at_ms = chrono::Utc::now().timestamp_millis();
    }

    fn bump_stats(&self, consumed: usize, with_action: bool) {
        let mut s = self.inner.stats.lock().unwrap();
        s.cycles_total += 1;
        s.events_consumed_total += consumed as u64;
        if with_action { s.cycles_with_action += 1; }
        s.last_cycle_at_ms = chrono::Utc::now().timestamp_millis();
    }

    /// Suggest a sensible default cycle interval for the supervisor (Phase 17).
    pub fn suggested_cycle_interval(&self) -> Duration {
        if self.world_state().recent_errors.is_empty() {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(2)
        }
    }
}

impl Default for AutonomyRuntime {
    fn default() -> Self { Self::new() }
}

fn push_bounded<T>(q: &mut std::collections::VecDeque<T>, item: T, cap: usize) {
    q.push_back(item);
    while q.len() > cap { q.pop_front(); }
}

fn push_recent_file(q: &mut std::collections::VecDeque<super::world_state::RecentFile>, item: super::world_state::RecentFile, cap: usize) {
    if let Some(pos) = q.iter().position(|r| r.path == item.path) {
        q.remove(pos);
    }
    q.push_back(item);
    while q.len() > cap { q.pop_front(); }
}

fn infer_phase_from_extension(path: &str) -> WorkflowPhase {
    let lower = path.to_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".txt") || lower.ends_with(".pdf") {
        WorkflowPhase::Reading
    } else if lower.ends_with(".rs") || lower.ends_with(".ts") || lower.ends_with(".js")
        || lower.ends_with(".py") || lower.ends_with(".go") || lower.ends_with(".java") {
        WorkflowPhase::Coding
    } else if lower.ends_with(".log") {
        WorkflowPhase::Debugging
    } else {
        WorkflowPhase::Unknown
    }
}

fn parse_build_state(s: &str) -> BuildState {
    match s {
        "running" => BuildState::Running,
        "success" | "succeeded" => BuildState::Succeeded,
        "failed" | "failure" => BuildState::Failed { message: "build failed".into() },
        _ => BuildState::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::event::NormalizedEvent;

    fn make_runtime() -> AutonomyRuntime {
        AutonomyRuntime::new()
    }

    #[test]
    fn runtime_starts_stopped() {
        let rt = make_runtime();
        assert_eq!(rt.mode(), RuntimeMode::Stopped);
    }

    #[test]
    fn start_pause_resume_shutdown() {
        let rt = make_runtime();
        rt.start();
        assert_eq!(rt.mode(), RuntimeMode::Running);
        rt.pause();
        assert_eq!(rt.mode(), RuntimeMode::Paused);
        rt.resume();
        assert_eq!(rt.mode(), RuntimeMode::Running);
        rt.shutdown();
        assert_eq!(rt.mode(), RuntimeMode::Stopped);
    }

    #[test]
    fn cycle_when_stopped_returns_pending() {
        let rt = make_runtime();
        let r = rt.run_cycle(10);
        assert_eq!(r.cycle.outcome, CycleOutcome::Pending);
    }

    #[test]
    fn empty_cycle_bumps_version() {
        let rt = make_runtime();
        rt.start();
        let v0 = rt.world_state().version;
        let r = rt.run_cycle(10);
        assert_eq!(r.cycle.outcome, CycleOutcome::Pending);
        assert!(rt.world_state().version >= v0);
    }

    #[test]
    fn active_app_event_updates_world_state() {
        let rt = make_runtime();
        rt.start();
        rt.publish(NormalizedEvent::new(EventKind::ActiveAppChanged {
            from: None,
            to: "vscode".into(),
        }));
        let r = rt.run_cycle(10);
        assert_eq!(r.cycle.outcome, CycleOutcome::Executed);
        assert_eq!(r.cycle.events_consumed, 1);
        assert_eq!(rt.world_state().active_app.as_deref(), Some("vscode"));
    }

    #[test]
    fn file_opened_sets_workflow_phase() {
        let rt = make_runtime();
        rt.start();
        rt.publish(NormalizedEvent::new(EventKind::FileOpened {
            path: "src/lib.rs".into(),
            project: Some("strawberry".into()),
        }));
        let r = rt.run_cycle(10);
        assert_eq!(r.cycle.outcome, CycleOutcome::Executed);
        let ws = rt.world_state();
        assert_eq!(ws.workflow_phase, WorkflowPhase::Coding);
        assert_eq!(ws.active_file.as_ref().unwrap().path, "src/lib.rs");
        assert_eq!(ws.active_project.as_deref(), Some("strawberry"));
    }

    #[test]
    fn recent_files_bounded() {
        let rt = make_runtime();
        rt.start();
        for i in 0..50 {
            rt.publish(NormalizedEvent::new(EventKind::FileModified {
                path: format!("/tmp/f{i}.rs"),
                project: Some("p".into()),
            }));
        }
        rt.run_cycle(100);
        assert!(rt.world_state().recent_files.len() <= RECENT_LIMIT);
    }

    #[test]
    fn error_observed_updates_recent_errors() {
        let rt = make_runtime();
        rt.start();
        rt.publish(NormalizedEvent::new(EventKind::ErrorObserved {
            message: "E0308 mismatched types".into(),
            source: "build".into(),
        }));
        rt.run_cycle(10);
        let ws = rt.world_state();
        assert_eq!(ws.recent_errors.len(), 1);
        assert!(ws.has_recent_build_error());
    }

    #[test]
    fn stats_track_cycles_and_events() {
        let rt = make_runtime();
        rt.start();
        for _ in 0..3 {
            rt.publish(NormalizedEvent::new(EventKind::Heartbeat { source: "x".into() }));
        }
        rt.run_cycle(10);
        rt.run_cycle(10);
        let s = rt.stats();
        assert_eq!(s.cycles_total, 2);
        assert_eq!(s.events_consumed_total, 3);
        assert_eq!(s.cycles_with_action, 1); // only first had events
    }
}
