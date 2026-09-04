//! 🔁 Full Agent Lifecycle — Phase 13 of the Strawberry platform.
//!
//! Connects the existing deterministic chain end-to-end (master spec
//! §Core Pipeline), reusing every Phase 7–12 component — no second
//! orchestrator:
//!
//!   OBSERVE → (goals) → PLAN → SAFETY → EXECUTE → VERIFY →
//!   REPLAN/CONTINUE or ABANDON — every transition recorded in the ledger.
//!
//! The lifecycle runs ONE goal at a time to completion (or escalation),
//! which is the minimal honest semantics: parallel goal racing needs
//! arbitration that later phases (21 priority rules) provide. One-at-a-
//! time keeps every decision explainable and the ledger ordered.
//!
//! Pause/resume: the driver takes a `paused` flag checked between every
//! stage — a paused lifecycle never starts new work and never leaves an
//! action half-issued (executor already isolates in-flight runs).

use serde::{Deserialize, Serialize};

use super::executor::{ActionRecord, ApprovalState, Effector, Executor};
use super::goal::{GoalCandidate, GoalStatus};
use super::planner::{plan as plan_goal, Planned, Plan, PlanStatus};
use super::replanner::{next_plan_after_failure, FailureContext, RecoveryDecision, Replanner};
use super::safety::{ActionRequest, ActionType, Actor, AuthorizedAction, RiskMode, SafetyGate, Verdict as SafetyVerdict};
use super::verifier::{Expectation, Verification, VerificationResult, Verifier};

// ─────────────────────────── run model ───────────────────────────

/// One goal's autonomous run, fully auditable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleRun {
    pub goal_id: u64,
    pub goal_title: String,
    /// completed | failed | escalated | abandoned | paused | no_goal | denied
    pub outcome: LifecycleOutcome,
    pub attempts: u32,
    /// Every action record produced during the run.
    pub actions: Vec<ActionRecord>,
    /// Every verification produced during the run.
    pub verifications: Vec<VerificationResult>,
    /// Explainability: the ordered decision trail.
    pub trail: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleOutcome {
    Completed,
    Failed,
    Escalated,
    Abandoned,
    Paused,
    NoGoal,
    Denied,
}

/// Configuration knobs for a run (all caller-owned; lifecycle stays pure).
pub struct LifecycleConfig<'a> {
    pub risk_mode: RiskMode,
    pub paused: bool,
    pub max_attempts: u32,
    pub timeout_secs: u64,
    /// The single approval oracle the caller owns. For tests this is a
    /// closure; production wires it to the user-approval flow (Phase 22).
    pub approve_high_risk: Box<dyn Fn(&ActionRequest) -> bool + 'a>,
}

// ─────────────────────────── the lifecycle driver ───────────────────────────

pub struct Lifecycle;

impl Lifecycle {
    /// Drive ONE goal through plan → safety → execute → verify → replan.
    ///
    /// Observes the pause flag between stages. Never executes forbidden
    /// actions (structurally impossible: they never become Authorized).
    pub fn run_goal(
        goal: &GoalCandidate,
        cfg: &LifecycleConfig<'_>,
        executor: &Executor,
        effector: &dyn Effector,
    ) -> LifecycleRun {
        let mut trail: Vec<String> = Vec::new();
        let mut actions: Vec<ActionRecord> = Vec::new();
        let mut verifications: Vec<VerificationResult> = Vec::new();
        let replanner = Replanner { max_attempts: cfg.max_attempts, ..Default::default() };

        trail.push(format!("goal {} accepted: {}", goal.goal_id.raw(), goal.title));

        // ── PLAN ─────────────────────────────────────────────────────────
        if cfg.paused {
            trail.push("paused before planning".into());
            return Self::finish(goal, LifecycleOutcome::Paused, 0, actions, verifications, trail);
        }
        let (mut plan, _rejections) = match plan_goal(goal) {
            Planned::Plan(p) => (p, ()),
            Planned::Rejected(r) => {
                trail.push(format!("planning rejected: {}", r.reason));
                return Self::finish(goal, LifecycleOutcome::Failed, 0, actions, verifications, trail);
            }
        };
        plan.accept();
        trail.push(format!("plan {} ready: {} steps", plan.plan_id.raw(), plan.steps.len()));

        // ── ATTEMPT LOOP ────────────────────────────────────────────────
        let mut attempt: u32 = 1;
        loop {
            if cfg.paused {
                trail.push(format!("paused at attempt {attempt}"));
                return Self::finish(goal, LifecycleOutcome::Paused, attempt, actions, verifications, trail);
            }

            // Stale check before each attempt.
            let now = now_iso();
            if Replanner::plan_is_stale(plan.status, goal, &now) {
                trail.push("plan/goal went stale; abandoning".into());
                return Self::finish(goal, LifecycleOutcome::Abandoned, attempt, actions, verifications, trail);
            }

            // ── SAFETY + EXECUTE the goal's WORK step (the effectful one).
            // Prepare/inspect steps are context-gathering; the run's outcome
            // hangs on the work step, so that's what the lifecycle drives.
            // An alternative plan that carries no work step cannot satisfy
            // the goal — that's an honest escalation, not completion.
            let using_alternative = plan
                .steps
                .iter()
                .all(|s| !matches!(s.action, super::planner::StepAction::RequiresApproval));
            let step = match plan.steps.iter().find(|s| {
                matches!(s.action, super::planner::StepAction::RequiresApproval)
            }) {
                Some(s) => s.clone(),
                None if using_alternative => {
                    trail.push(
                        "alternative plan is context-only; goal's work remains undone — escalating"
                            .into(),
                    );
                    return Self::finish(
                        goal,
                        LifecycleOutcome::Escalated,
                        attempt,
                        actions,
                        verifications,
                        trail,
                    );
                }
                None => {
                    trail.push("plan has no work step; treating goal as context-only complete".into());
                    return Self::finish(goal, LifecycleOutcome::Completed, attempt, actions, verifications, trail);
                }
            };

            // Work steps map to RunCommand (approval-gated); everything
            // else stays read-only Inspect.
            let action_type = match step.action {
                super::planner::StepAction::RequiresApproval => ActionType::RunCommand,
                _ => ActionType::Inspect,
            };

            let mut request = ActionRequest {
                action_type: action_type.clone(),
                target: step.targets.first().cloned().unwrap_or_default(),
                actor: Actor::Core,
                user_approved: false,
                data_sensitivity: 2,
                external_destination: false,
                destructive: false,
            };
            // High-risk steps consult the approval oracle.
            if action_type == ActionType::RunCommand {
                request.user_approved = (cfg.approve_high_risk)(&request);
            }

            let decision = SafetyGate::evaluate(&request, cfg.risk_mode);
            trail.push(format!(
                "safety verdict for {}: {:?} ({})",
                action_type.label(),
                decision.verdict,
                decision.reasons.last().cloned().unwrap_or_default()
            ));

            let authorized = match AuthorizedAction::from_decision(&decision, &request.target) {
                Some(a) => a,
                None => {
                    let denied = Executor::record_denial(
                        request.action_type.clone(),
                        &request.target,
                        decision.reasons.last().map(|s| s.as_str()).unwrap_or("denied"),
                    );
                    actions.push(denied);
                    trail.push("action denied by safety gate; lifecycle stops here".into());
                    return Self::finish(goal, LifecycleOutcome::Denied, attempt, actions, verifications, trail);
                }
            };

            // ── EXECUTE ──
            let record = executor.execute(
                authorized,
                effector,
                Some(goal.goal_id.raw()),
                Some(plan.plan_id.raw()),
                Some(step.capability.clone()),
                &step.purpose,
                std::time::Duration::from_secs(cfg.timeout_secs),
            );
            let exec_ok = record.execution_state == super::executor::ExecutionState::Succeeded;
            trail.push(format!(
                "executed step {} (order {}): {:?}",
                step.step_id, step.order, record.execution_state
            ));
            actions.push(record);

            if cfg.paused {
                trail.push("paused after execution".into());
                return Self::finish(goal, LifecycleOutcome::Paused, attempt, actions, verifications, trail);
            }

            // ── VERIFY ──
            let expectation = if step.capability == "planner_tasks" && exec_ok {
                Expectation::ExitZero
            } else {
                Expectation::Manual
            };
            let verification = Verifier::verify(actions.last().unwrap(), &expectation);
            trail.push(format!(
                "verification: {:?} (confidence {:.2})",
                verification.verification, verification.confidence
            ));
            let v_kind = verification.verification;
            verifications.push(verification);

            if v_kind == Verification::Success {
                // Goal-level completion for the single-work-step templates.
                trail.push("expected outcome verified; goal completed".into());
                return Self::finish(goal, LifecycleOutcome::Completed, attempt, actions, verifications, trail);
            }

            // ── REPLAN decision ──
            let fctx = FailureContext {
                goal_id: goal.goal_id.raw(),
                attempt,
                verification: v_kind,
                failure_reason: actions
                    .last()
                    .and_then(|a| a.error.clone())
                    .unwrap_or_else(|| "expected outcome not observed".into()),
                has_alternative: !plan.alternatives.is_empty(),
                environmental: actions
                    .last()
                    .map(|a| a.execution_state == super::executor::ExecutionState::TimedOut)
                    .unwrap_or(false),
            };
            let decision = replanner.advise(&fctx, goal, &now);
            trail.push(format!("recovery decision: {decision:?}"));

            match decision {
                RecoveryDecision::Retry { attempt: next, .. } => {
                    attempt = next;
                }
                RecoveryDecision::UseAlternative { .. } | RecoveryDecision::Replan { .. } => {
                    let fresh = || match plan_goal(goal) {
                        Planned::Plan(p) => Planned::Plan(p),
                        Planned::Rejected(r) => Planned::Rejected(r),
                    };
                    let alt = || match plan_goal(goal) {
                        Planned::Plan(mut p) => {
                            // Alternative = planner's lighter inspect-only path.
                            p.steps = p.alternatives.clone();
                            Planned::Plan(p)
                        }
                        Planned::Rejected(r) => Planned::Rejected(r),
                    };
                    if let Some(Planned::Plan(new_plan)) =
                        next_plan_after_failure(&decision, fresh, alt)
                    {
                        let mut np = new_plan;
                        np.accept();
                        plan = np;
                        attempt += 1;
                    } else {
                        attempt += 1;
                    }
                }
                RecoveryDecision::EscalateToUser { reason } => {
                    trail.push(format!("escalated: {reason}"));
                    return Self::finish(goal, LifecycleOutcome::Escalated, attempt, actions, verifications, trail);
                }
                RecoveryDecision::AbandonGoal { reason } => {
                    trail.push(format!("abandoned: {reason}"));
                    return Self::finish(goal, LifecycleOutcome::Abandoned, attempt, actions, verifications, trail);
                }
            }
        }
    }

    fn finish(
        goal: &GoalCandidate,
        outcome: LifecycleOutcome,
        attempts: u32,
        actions: Vec<ActionRecord>,
        verifications: Vec<VerificationResult>,
        mut trail: Vec<String>,
    ) -> LifecycleRun {
        trail.push(format!(
            "run finished: {outcome:?} after {attempts} attempt(s), {} action(s)",
            actions.len()
        ));
        LifecycleRun {
            goal_id: goal.goal_id.raw(),
            goal_title: goal.title.clone(),
            outcome,
            attempts,
            actions,
            verifications,
            trail,
        }
    }
}

fn now_iso() -> String {
    let secs = chrono::Utc::now().timestamp();
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::executor::ExecutionState;
    use super::super::goal::{Evidence, EvidenceKind, Priority};
    use super::super::ids::GoalId;

    fn goal() -> GoalCandidate {
        GoalCandidate {
            goal_id: GoalId::new(9),
            title: "Complete: e2e goal".into(),
            description: "e2e".into(),
            project: None,
            priority: Priority::High,
            confidence: 0.7,
            evidence: vec![Evidence { kind: EvidenceKind::Task, reference: "1".into(), summary: "s".into(), weight: 1.0 }],
            status: GoalStatus::Accepted,
            created_at: "2026-09-03T00:00:00Z".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
        }
    }

    struct OkEffector;
    impl Effector for OkEffector {
        fn run(&self, _a: &AuthorizedAction, _c: &std::sync::atomic::AtomicBool, _t: std::time::Duration) -> (i32, String) {
            (0, "ok".into())
        }
    }
    struct FailEffector;
    impl Effector for FailEffector {
        fn run(&self, _a: &AuthorizedAction, _c: &std::sync::atomic::AtomicBool, _t: std::time::Duration) -> (i32, String) {
            (1, "boom".into())
        }
    }

    fn cfg<'a>(approve: bool, paused: bool) -> LifecycleConfig<'a> {
        LifecycleConfig {
            risk_mode: RiskMode::Normal,
            paused,
            max_attempts: 3,
            timeout_secs: 5,
            approve_high_risk: Box::new(move |_r| approve),
        }
    }

    #[test]
    fn complete_successful_lifecycle() {
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&goal(), &cfg(true, false), &ex, &OkEffector);
        assert_eq!(run.outcome, LifecycleOutcome::Completed);
        assert_eq!(run.attempts, 1);
        assert!(!run.actions.is_empty());
        assert!(run.trail.iter().any(|t| t.contains("plan")));
        assert!(run.trail.iter().any(|t| t.contains("safety verdict")));
        assert!(run.trail.iter().any(|t| t.contains("verification")));
    }

    #[test]
    fn no_goal_lifecycle_reports_cleanly() {
        // A goal with no evidence can't plan → Failed (the honest "no plan"
        // path; Phase 7 already rejects evidence-less goals).
        let mut g = goal();
        g.evidence = vec![];
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&g, &cfg(true, false), &ex, &OkEffector);
        assert_eq!(run.outcome, LifecycleOutcome::Failed);
        assert!(run.trail.iter().any(|t| t.contains("planning rejected")));
    }

    #[test]
    fn denied_action_lifecycle() {
        // High-risk work step without approval → denied at the gate.
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&goal(), &cfg(false, false), &ex, &OkEffector);
        assert_eq!(run.outcome, LifecycleOutcome::Denied);
        assert!(run.actions.iter().all(|a| a.approval_state == ApprovalState::DeniedByGate));
        assert!(run.actions.iter().all(|a| a.execution_state == ExecutionState::NotExecuted));
        assert!(run.trail.iter().any(|t| t.contains("denied by safety gate")));
    }

    #[test]
    fn failed_action_lifecycle_escalates_after_budget() {
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&goal(), &cfg(true, false), &ex, &FailEffector);
        // With alternatives present the ladder runs: alt → replan → escalate.
        assert_eq!(run.outcome, LifecycleOutcome::Escalated);
        assert!(run.attempts >= 2, "must not give up after one failure");
        assert!(run.actions.iter().any(|a| a.execution_state == ExecutionState::Failed));
        assert!(
            run.trail.iter().any(|t| t.contains("escalat")),
            "trail: {}",
            run.trail.join(" | ")
        );
    }

    #[test]
    fn verification_failure_is_reported_not_assumed() {
        // Manual expectation on a failed execution → Failure verdict, then
        // the replanner ladder runs (same escalation path as above).
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&goal(), &cfg(true, false), &ex, &FailEffector);
        assert!(!run.verifications.is_empty());
        assert!(run.verifications.iter().all(|v| v.verification != Verification::Success));
    }

    #[test]
    fn replan_lifecycle_produces_alternative_plan() {
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&goal(), &cfg(true, false), &ex, &FailEffector);
        // Trail must contain the UseAlternative or Replan decision.
        assert!(run.trail.iter().any(|t| t.contains("UseAlternative") || t.contains("Replan")));
    }

    #[test]
    fn pause_before_work_stops_cleanly() {
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&goal(), &cfg(true, true), &ex, &OkEffector);
        assert_eq!(run.outcome, LifecycleOutcome::Paused);
        assert!(run.actions.is_empty(), "paused run must not act");
        assert!(run.trail.iter().any(|t| t.contains("paused")));
    }

    #[test]
    fn forbidden_action_never_reaches_executor() {
        // Structurally: the lifecycle only builds Inspect/RunCommand requests.
        let g = goal();
        let planned = match plan_goal(&g) {
            Planned::Plan(p) => p,
            Planned::Rejected(r) => panic!("fixture goal must plan: {r:?}"),
        };
        // Every actionable step maps to Inspect or RunCommand only.
        assert!(planned.steps.iter().all(|s| {
            matches!(
                s.action,
                crate::autonomous::planner::StepAction::Inspect
                    | crate::autonomous::planner::StepAction::Prepare
                    | crate::autonomous::planner::StepAction::RequiresApproval
            )
        }));
    }

    #[test]
    fn run_is_fully_auditable() {
        let ex = Executor::new();
        let run = Lifecycle::run_goal(&goal(), &cfg(true, false), &ex, &OkEffector);
        // Trail covers: goal, plan, safety, execution, verification, finish.
        let joined = run.trail.join(" | ");
        assert!(joined.contains("goal 9 accepted"));
        assert!(joined.contains("plan"));
        assert!(joined.contains("safety verdict"));
        assert!(joined.contains("executed step"));
        assert!(joined.contains("run finished"));
        // Actions carry goal+plan provenance for the ledger.
        assert!(run.actions.iter().all(|a| a.goal_id == Some(9)));
        assert!(run.actions.iter().all(|a| a.plan_id.is_some()));
    }
}
