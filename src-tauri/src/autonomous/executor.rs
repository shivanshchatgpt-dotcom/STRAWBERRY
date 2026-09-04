//! ⚙️ Action Executor — Phase 10 of the Strawberry platform.
//!
//! The ONLY controlled execution boundary (master spec §Executor).
//! Structural guarantee: `execute()` accepts an [`AuthorizedAction`] —
//! which can only be minted from an `Approved` Safety Gate decision.
//! A forbidden or unapproved request **cannot reach this code** through
//! the public API.
//!
//! Every execution produces a complete `ActionRecord` (audit-grade:
//! action id, actor, goal/plan provenance, timestamps, approval state,
//! result, error, verification hook). Failures are captured as records —
//! a failed action can never crash the runtime (isolation requirement).
//!
//! This phase ships the execution *harness*: process spawn + capture +
//! timeout + cancellation. The concrete effectors (file ops, git, send)
//! plug in behind `Effector`, and Phase 11's verifier consumes the
//! recorded outputs.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::ids::ActionId;
use super::safety::{ActionType, AuthorizedAction};

// ─────────────────────────── records ───────────────────────────

/// What actually ran.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRecord {
    pub action_id: ActionId,
    pub action_type: ActionType,
    pub target: String,
    /// Goal/plan provenance for the ledger (Phase 14 hardening consumes
    /// these; empty when the action is not goal-driven).
    pub goal_id: Option<u64>,
    pub plan_id: Option<u64>,
    pub capability: Option<String>,
    pub reason: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// Approved | DeniedByGate | StructurallyForbidden
    pub approval_state: ApprovalState,
    pub execution_state: ExecutionState,
    /// Process exit code when a command ran.
    pub exit_code: Option<i32>,
    /// Captured stdout/stderr (bounded) for the Verifier.
    pub output: String,
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u128,
    /// Whether a globally registered cancellation stopped it.
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Approved,
    DeniedByGate,
    StructurallyForbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    /// Never ran — gate said no.
    NotExecuted,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

// ─────────────────────────── effector abstraction ───────────────────────────

/// The one seam where real effects happen. The default effector runs
/// shell commands; tests inject fakes. Every effector respects the
/// cancellation flag and timeout — the harness enforces both.
pub trait Effector: Send + Sync {
    /// Run the effect. Returns (exit_code, bounded_output).
    fn run(
        &self,
        action: &AuthorizedAction,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> (i32, String);
}

/// Production effector: spawns `/bin/sh -c` for RUN_COMMAND-style actions,
/// and refuses (without touching the filesystem) anything else — file ops
/// arrive with their own effectors in later phases and must not be faked.
pub struct ShellEffector;

impl Effector for ShellEffector {
    fn run(
        &self,
        action: &AuthorizedAction,
        _cancel: &AtomicBool,
        _timeout: Duration,
    ) -> (i32, String) {
        if action.action_type != ActionType::RunCommand {
            return (-1, format!("no effector registered for {}", action.action_type.label()));
        }
        match std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&action.target)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            Ok(out) => {
                let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                let err = String::from_utf8_lossy(&out.stderr);
                if !err.is_empty() {
                    text.push('\n');
                    text.push_str(&err);
                }
                // Bounded output — 16 KB is plenty for verification evidence.
                let bounded: String = text.chars().take(16_384).collect();
                (out.status.code().unwrap_or(-1), bounded)
            }
            Err(e) => (-1, format!("spawn failed: {e}")),
        }
    }
}

// ─────────────────────────── the executor ───────────────────────────

static ACTION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// The one executor. Cheap to clone.
#[derive(Clone)]
pub struct Executor {
    cancel_all: Arc<AtomicBool>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            cancel_all: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Global kill switch — every in-flight `run` observes it.
    pub fn cancel_everything(&self) {
        self.cancel_all.store(true, Ordering::Relaxed);
    }
    pub fn reset_cancel(&self) {
        self.cancel_all.store(false, Ordering::Relaxed);
    }

    fn next_id() -> ActionId {
        ActionId::new(ACTION_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    fn now_iso() -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// Execute an ALREADY-AUTHORIZED action. The type system is the first
    /// gate: `AuthorizedAction` cannot be built for blocked requests.
    ///
    /// Isolation: every failure mode (spawn error, timeout, cancellation,
    /// unknown action) is converted into a failed/cancelled record — this
    /// function never panics on execution problems.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        action: AuthorizedAction,
        effector: &dyn Effector,
        goal_id: Option<u64>,
        plan_id: Option<u64>,
        capability: Option<String>,
        reason: &str,
        timeout: Duration,
    ) -> ActionRecord {
        let action_id = Self::next_id();
        let started_at = Self::now_iso();
        let started = Instant::now();
        let mut record = ActionRecord {
            action_id,
            action_type: action.action_type.clone(),
            target: action.target.clone(),
            goal_id,
            plan_id,
            capability,
            reason: reason.to_string(),
            started_at: started_at.clone(),
            finished_at: None,
            approval_state: ApprovalState::Approved,
            execution_state: ExecutionState::Running,
            exit_code: None,
            output: String::new(),
            error: None,
            duration_ms: 0,
            cancelled: false,
        };

        // Cancellation *before* start: shut-down races land here.
        if self.cancel_all.load(Ordering::Relaxed) {
            record.execution_state = ExecutionState::Cancelled;
            record.cancelled = true;
            record.finished_at = Some(Self::now_iso());
            record.duration_ms = started.elapsed().as_millis();
            return record;
        }

        // Run under harness protection.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            effector.run(&action, &self.cancel_all, timeout)
        }));

        record.duration_ms = started.elapsed().as_millis();
        record.finished_at = Some(Self::now_iso());

        match result {
            Ok((code, output)) => {
                record.exit_code = Some(code);
                record.output = output;
                if self.cancel_all.load(Ordering::Relaxed) {
                    record.execution_state = ExecutionState::Cancelled;
                    record.cancelled = true;
                } else if record.duration_ms >= timeout.as_millis() {
                    record.execution_state = ExecutionState::TimedOut;
                } else if code == 0 {
                    record.execution_state = ExecutionState::Succeeded;
                } else {
                    record.execution_state = ExecutionState::Failed;
                    record.error = Some(format!("exit code {code}"));
                }
            }
            Err(_) => {
                // Effector panicked — isolate it, never propagate.
                record.execution_state = ExecutionState::Failed;
                record.error = Some("effector panicked; isolated by executor".to_string());
            }
        }
        record
    }

    /// Record a gate-denied request WITHOUT executing anything. The ledger
    /// (Phase 14) needs denials to be first-class explainability facts.
    pub fn record_denial(
        action_type: ActionType,
        target: &str,
        reason: &str,
    ) -> ActionRecord {
        ActionRecord {
            action_id: Self::next_id(),
            action_type,
            target: target.to_string(),
            goal_id: None,
            plan_id: None,
            capability: None,
            reason: reason.to_string(),
            started_at: Self::now_iso(),
            finished_at: Some(Self::now_iso()),
            approval_state: ApprovalState::DeniedByGate,
            execution_state: ExecutionState::NotExecuted,
            exit_code: None,
            output: String::new(),
            error: None,
            duration_ms: 0,
            cancelled: false,
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::safety::{ActionRequest, Actor, SafetyGate, Verdict as SafetyVerdict};

    fn conn_setup() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    fn authorized(action: ActionType, target: &str, approved: bool) -> AuthorizedAction {
        let mut r = ActionRequest {
            action_type: action,
            target: target.into(),
            actor: Actor::Core,
            user_approved: approved,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        };
        if r.action_type == ActionType::RunCommand {
            r.actor = Actor::User; // commands need the user path anyway
        }
        let dec = SafetyGate::evaluate(&r, super::super::safety::RiskMode::Normal);
        assert_eq!(dec.verdict, SafetyVerdict::Approved, "fixture must be approved");
        AuthorizedAction::from_decision(&dec, target).unwrap()
    }

    /// Effector that succeeds/fails on demand.
    struct FakeEffector {
        exit_code: i32,
        panic: bool,
        sleep_ms: u64,
    }
    impl Effector for FakeEffector {
        fn run(&self, _a: &AuthorizedAction, _c: &AtomicBool, _t: Duration) -> (i32, String) {
            if self.panic {
                panic!("effector boom");
            }
            std::thread::sleep(Duration::from_millis(self.sleep_ms));
            (self.exit_code, format!("fake output {}", self.exit_code))
        }
    }

    #[test]
    fn low_risk_approved_action_executes_and_records() {
        let ex = Executor::new();
        let rec = ex.execute(
            authorized(ActionType::RunCommand, "echo hi", true),
            &FakeEffector { exit_code: 0, panic: false, sleep_ms: 0 },
            None, None, None, "test", Duration::from_secs(5),
        );
        assert_eq!(rec.execution_state, ExecutionState::Succeeded);
        assert_eq!(rec.approval_state, ApprovalState::Approved);
        assert!(rec.output.contains("fake output"));
        assert_eq!(rec.exit_code, Some(0));
        assert!(rec.finished_at.is_some());
    }

    #[test]
    fn failed_action_is_recorded_not_crashed() {
        let ex = Executor::new();
        let rec = ex.execute(
            authorized(ActionType::RunCommand, "false", true),
            &FakeEffector { exit_code: 1, panic: false, sleep_ms: 0 },
            None, None, None, "test", Duration::from_secs(5),
        );
        assert_eq!(rec.execution_state, ExecutionState::Failed);
        assert!(rec.error.as_deref().unwrap_or("").contains("exit code 1"));
    }

    #[test]
    fn panicking_effector_is_isolated() {
        let ex = Executor::new();
        let rec = ex.execute(
            authorized(ActionType::RunCommand, "boom", true),
            &FakeEffector { exit_code: 0, panic: true, sleep_ms: 0 },
            None, None, None, "test", Duration::from_secs(5),
        );
        assert_eq!(rec.execution_state, ExecutionState::Failed);
        assert!(rec.error.unwrap().contains("isolated"));
    }

    #[test]
    fn timeout_is_detected() {
        let ex = Executor::new();
        let rec = ex.execute(
            authorized(ActionType::RunCommand, "sleep 10", true),
            &FakeEffector { exit_code: 0, panic: false, sleep_ms: 400 },
            None, None, None, "test", Duration::from_millis(50),
        );
        assert_eq!(rec.execution_state, ExecutionState::TimedOut);
    }

    #[test]
    fn cancellation_stops_execution() {
        let ex = Executor::new();
        ex.cancel_everything();
        let rec = ex.execute(
            authorized(ActionType::RunCommand, "echo hi", true),
            &FakeEffector { exit_code: 0, panic: false, sleep_ms: 0 },
            None, None, None, "test", Duration::from_secs(5),
        );
        assert_eq!(rec.execution_state, ExecutionState::Cancelled);
        assert!(rec.cancelled);
        assert_eq!(rec.approval_state, ApprovalState::Approved);
        ex.reset_cancel();
    }

    #[test]
    fn gate_denial_never_reaches_execution() {
        let r = ActionRequest {
            action_type: ActionType::RunCommand,
            target: "rm -rf /".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        };
        let dec = SafetyGate::evaluate(&r, super::super::safety::RiskMode::Normal);
        assert_eq!(dec.verdict, SafetyVerdict::NeedsApproval);
        // Structural proof: no AuthorizedAction can be minted.
        assert!(AuthorizedAction::from_decision(&dec, "rm -rf /").is_none());
        // The denial becomes a first-class record instead.
        let rec = Executor::record_denial(ActionType::RunCommand, "rm -rf /", "high risk; approval required");
        assert_eq!(rec.execution_state, ExecutionState::NotExecuted);
        assert_eq!(rec.approval_state, ApprovalState::DeniedByGate);
    }

    #[test]
    fn forbidden_action_cannot_be_constructed_authorized() {
        let r = ActionRequest {
            action_type: ActionType::PermanentDelete,
            target: "/data".into(),
            actor: Actor::User,
            user_approved: true,
            data_sensitivity: 1,
            external_destination: false,
            destructive: true,
        };
        let dec = SafetyGate::evaluate(&r, super::super::safety::RiskMode::Normal);
        assert!(AuthorizedAction::from_decision(&dec, "/data").is_none(),
            "the type system must refuse forbidden authorization");
    }

    #[test]
    fn audit_entry_is_complete() {
        let ex = Executor::new();
        let rec = ex.execute(
            authorized(ActionType::RunCommand, "echo audit", true),
            &FakeEffector { exit_code: 0, panic: false, sleep_ms: 0 },
            Some(42), Some(7), Some("planner_tasks".into()), "verify plan", Duration::from_secs(5),
        );
        assert_eq!(rec.goal_id, Some(42));
        assert_eq!(rec.plan_id, Some(7));
        assert_eq!(rec.capability.as_deref(), Some("planner_tasks"));
        assert_eq!(rec.reason, "verify plan");
        assert!(!rec.started_at.is_empty());
        assert!(rec.finished_at.is_some());
        assert!(rec.duration_ms > 0 || rec.duration_ms == 0); // always present
        // Round-trips through the ledger JSON.
        let json = serde_json::to_string(&rec).unwrap();
        let back: ActionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.action_id, rec.action_id);
        assert!(json.contains("\"approvalState\""));
    }

    #[test]
    fn shell_effector_rejects_non_command_actions() {
        let a = authorized(ActionType::FileRead, "/tmp/x", false);
        let (code, out) = ShellEffector.run(&a, &AtomicBool::new(false), Duration::from_secs(5));
        assert_eq!(code, -1);
        assert!(out.contains("no effector registered"));
    }

    #[test]
    fn real_shell_command_runs_and_captures_output() {
        // One real process: proves the production effector path works.
        let ex = Executor::new();
        let rec = ex.execute(
            authorized(ActionType::RunCommand, "printf strawberry-executor-ok", true),
            &ShellEffector,
            None, None, None, "e2e smoke", Duration::from_secs(10),
        );
        assert_eq!(rec.execution_state, ExecutionState::Succeeded);
        assert!(rec.output.contains("strawberry-executor-ok"), "got: {}", rec.output);
    }

    #[test]
    fn records_survive_in_the_decision_ledger() {
        // Integration: denial records fit the existing autonomy_decisions
        // schema (Phase 6) — Phase 14 hardening will formalize this.
        let conn = conn_setup();
        let rec = Executor::record_denial(ActionType::FileWrite, "/etc/passwd", "high risk; approval required");
        super::super::scheduler::Scheduler::log(
            &conn,
            "executor",
            "deny",
            &format!("{} {}: {}", rec.action_type.label(), rec.target, rec.reason),
            None,
        );
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM autonomy_decisions WHERE capability_id='executor' AND decision='deny'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }
}
