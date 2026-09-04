//! 🔄 Replanner — Phase 12 of the Strawberry platform.
//!
//! Controlled recovery, never blind retry (master spec §Replanning):
//!
//!   FAILURE → UPDATE WORLD STATE → RE-EVALUATE GOAL → ALTERNATIVE PLAN
//!   → SAFETY GATE → EXECUTE IF ALLOWED
//!
//! Policies implemented deterministically:
//!   * retry limit (default 3) with exponential backoff (2^n × base)
//!   * alternative-plan selection (planner's `alternatives` first, then
//!     a regenerated plan, then give up)
//!   * stale-plan detection (goal expiry kills the attempt chain)
//!   * user escalation (limit reached OR ambiguous failure)
//!
//! Pure computation: `Replanner::advise` returns a `RecoveryDecision`
//! the caller (Phase 13 lifecycle) executes. No state, no clocks, no I/O.

use serde::{Deserialize, Serialize};

use super::goal::GoalCandidate;
use super::planner::{Planned, PlanStatus};
use super::verifier::Verification;

// ─────────────────────────── model ───────────────────────────

/// What the replanner advises next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDecision {
    /// Retry the same plan after backoff_secs.
    Retry { attempt: u32, backoff_secs: u64 },
    /// Switch to the alternative (lighter) plan.
    UseAlternative { attempt: u32 },
    /// Regenerate a plan from the goal (context changed).
    Replan { attempt: u32 },
    /// Give up on this goal and tell the user why.
    EscalateToUser { reason: String },
    /// The goal itself expired / was completed — stop cleanly.
    AbandonGoal { reason: String },
}

/// Immutable failure snapshot the replanner reasons over.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureContext {
    pub goal_id: u64,
    pub attempt: u32,
    pub verification: Verification,
    /// Why the attempt failed (executor error / verifier evidence line).
    pub failure_reason: String,
    /// Whether the planner provided an alternative path.
    pub has_alternative: bool,
    /// True when the failure looks environmental (timeout) vs logical
    /// (assertion mismatch) — environmental retries are cheaper.
    pub environmental: bool,
}

/// Deterministic recovery policy knobs (all fixed here; configurable later
/// via capability registry when Phase 14 hardens settings).
pub struct Replanner {
    pub max_attempts: u32,
    pub base_backoff_secs: u64,
}

pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;
pub const DEFAULT_BASE_BACKOFF_SECS: u64 = 5;

impl Default for Replanner {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            base_backoff_secs: DEFAULT_BASE_BACKOFF_SECS,
        }
    }
}

impl Replanner {
    /// Exponential backoff: base × 2^(attempt-1), capped sanely.
    fn backoff(&self, attempt: u32) -> u64 {
        let shift = attempt.saturating_sub(1).min(6); // cap 2^6 = 64×
        (self.base_backoff_secs.saturating_mul(1 << shift)).min(3600)
    }

    /// Decide the next recovery move for a failed attempt.
    /// Deterministic decision ladder (first match wins):
    ///   1. goal gone (stale/expired/completed) → AbandonGoal
    ///   2. attempt ≥ max → EscalateToUser
    ///   3. verification Unknown (ambiguous) → escalate (never blind-retry
    ///      what we can't measure)
    ///   4. attempt 1 & alternative exists → UseAlternative (cheapest
    ///      recovery: known-lighter path)
    ///   5. attempt 2 → Replan (context likely changed)
    ///   6. else → Retry with backoff (environmental failures only;
    ///      logical failures at attempt 3 escalate)
    pub fn advise(&self, ctx: &FailureContext, goal: &GoalCandidate, now: &str) -> RecoveryDecision {
        // 1. Goal lifecycle first — dead goals produce no more attempts.
        if goal.is_stale(now)
            || matches!(
                goal.status,
                super::goal::GoalStatus::Cancelled
                    | super::goal::GoalStatus::Expired
                    | super::goal::GoalStatus::Completed
            )
        {
            return RecoveryDecision::AbandonGoal {
                reason: format!("goal {} no longer actionable ({:?})", goal.goal_id.raw(), goal.status),
            };
        }

        // 2. Retry budget exhausted.
        if ctx.attempt >= self.max_attempts {
            return RecoveryDecision::EscalateToUser {
                reason: format!(
                    "attempt {}/{} exhausted on goal {} after: {}",
                    ctx.attempt, self.max_attempts, goal.goal_id.raw(), ctx.failure_reason
                ),
            };
        }

        // 3. Ambiguous outcomes must not be blindly retried.
        if ctx.verification == Verification::Unknown && ctx.attempt >= 2 {
            return RecoveryDecision::EscalateToUser {
                reason: format!(
                    "verification UNKNOWN twice on goal {}; cannot measure progress",
                    goal.goal_id.raw()
                ),
            };
        }

        // 4–6. Escalation ladder by attempt number.
        let next = ctx.attempt + 1;
        match ctx.attempt {
            1 if ctx.has_alternative => RecoveryDecision::UseAlternative { attempt: next },
            2 => RecoveryDecision::Replan { attempt: next },
            1 => RecoveryDecision::Retry { attempt: next, backoff_secs: self.backoff(next) },
            _ => {
                if ctx.environmental {
                    RecoveryDecision::Retry { attempt: next, backoff_secs: self.backoff(next) }
                } else {
                    RecoveryDecision::EscalateToUser {
                        reason: format!(
                            "logical failure persisted on goal {} after attempt {}: {}",
                            goal.goal_id.raw(),
                            ctx.attempt,
                            ctx.failure_reason
                        ),
                    }
                }
            }
        }
    }

    /// Convenience: should a plan be discarded because its goal aged out?
    pub fn plan_is_stale(plan_status: PlanStatus, goal: &GoalCandidate, now: &str) -> bool {
        matches!(plan_status, PlanStatus::Rejected | PlanStatus::Stale)
            || goal.is_stale(now)
            || matches!(
                goal.status,
                super::goal::GoalStatus::Cancelled
                    | super::goal::GoalStatus::Expired
                    | super::goal::GoalStatus::Completed
            )
    }
}

/// Helper for Phase 13: run the recovery decision against the planner and
/// produce the next `Planned` artifact (pure — no execution here).
pub fn next_plan_after_failure(
    decision: &RecoveryDecision,
    regenerate: impl FnOnce() -> Planned,
    alternative_of: impl FnOnce() -> Planned,
) -> Option<Planned> {
    match decision {
        RecoveryDecision::UseAlternative { .. } => Some(alternative_of()),
        RecoveryDecision::Replan { .. } => Some(regenerate()),
        // Retry uses the SAME plan — caller keeps it; escalate/abandon stop.
        RecoveryDecision::Retry { .. } => None,
        RecoveryDecision::EscalateToUser { .. } => None,
        RecoveryDecision::AbandonGoal { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::goal::{Evidence, EvidenceKind, GoalStatus, Priority};
    use super::super::ids::GoalId;

    fn goal() -> GoalCandidate {
        GoalCandidate {
            goal_id: GoalId::new(9),
            title: "Complete: fix tests".into(),
            description: "fix".into(),
            project: None,
            priority: Priority::High,
            confidence: 0.7,
            evidence: vec![Evidence { kind: EvidenceKind::Task, reference: "1".into(), summary: "s".into(), weight: 1.0 }],
            status: GoalStatus::Accepted,
            created_at: "2026-09-03T00:00:00Z".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
        }
    }

    fn ctx(attempt: u32, verification: Verification, has_alt: bool, environmental: bool) -> FailureContext {
        FailureContext {
            goal_id: 9,
            attempt,
            verification,
            failure_reason: "command failed".into(),
            has_alternative: has_alt,
            environmental,
        }
    }

    #[test]
    fn first_failure_with_alternative_uses_it() {
        let r = Replanner::default();
        let d = r.advise(&ctx(1, Verification::Failure, true, false), &goal(), "2026-09-03T01:00:00Z");
        assert!(matches!(d, RecoveryDecision::UseAlternative { attempt: 2 }));
    }

    #[test]
    fn first_failure_without_alternative_retries_with_backoff() {
        let r = Replanner::default();
        let d = r.advise(&ctx(1, Verification::Failure, false, true), &goal(), "2026-09-03T01:00:00Z");
        match d {
            RecoveryDecision::Retry { attempt, backoff_secs } => {
                assert_eq!(attempt, 2);
                assert_eq!(backoff_secs, 10, "5 × 2^1");
            }
            other => panic!("expected retry, got {other:?}"),
        }
    }

    #[test]
    fn second_failure_triggers_replan() {
        let r = Replanner::default();
        let d = r.advise(&ctx(2, Verification::Failure, true, false), &goal(), "2026-09-03T01:00:00Z");
        assert!(matches!(d, RecoveryDecision::Replan { attempt: 3 }));
    }

    #[test]
    fn retry_limit_escalates_to_user() {
        let r = Replanner::default();
        let d = r.advise(&ctx(3, Verification::Failure, true, true), &goal(), "2026-09-03T01:00:00Z");
        match d {
            RecoveryDecision::EscalateToUser { reason } => {
                assert!(reason.contains("exhausted"), "got: {reason}");
            }
            other => panic!("expected escalation, got {other:?}"),
        }
    }

    #[test]
    fn backoff_grows_exponentially_and_is_capped() {
        let r = Replanner::default();
        assert_eq!(r.backoff(1), 5);
        assert_eq!(r.backoff(2), 10);
        assert_eq!(r.backoff(3), 20);
        assert_eq!(r.backoff(10), 320); // 5 × 2^6 (capped shift)
        assert!(r.backoff(99) <= 3600, "hard cap one hour");
    }

    #[test]
    fn unknown_verification_is_not_blindly_retried() {
        let r = Replanner::default();
        let d = r.advise(&ctx(2, Verification::Unknown, false, false), &goal(), "2026-09-03T01:00:00Z");
        match d {
            RecoveryDecision::EscalateToUser { reason } => assert!(reason.contains("UNKNOWN")),
            other => panic!("expected escalation, got {other:?}"),
        }
        // Attempt 1 unknown is still allowed to retry (cheap probe).
        let d1 = r.advise(&ctx(1, Verification::Unknown, false, true), &goal(), "2026-09-03T01:00:00Z");
        assert!(matches!(d1, RecoveryDecision::Retry { .. }));
    }

    #[test]
    fn stale_goal_abandons_instead_of_retrying() {
        let r = Replanner::default();
        let mut g = goal();
        g.expires_at = "2020-01-01T00:00:00Z".into();
        let d = r.advise(&ctx(1, Verification::Failure, true, true), &g, "2026-09-03T01:00:00Z");
        match d {
            RecoveryDecision::AbandonGoal { reason } => assert!(reason.contains("no longer actionable")),
            other => panic!("expected abandon, got {other:?}"),
        }
    }

    #[test]
    fn completed_goal_abandons() {
        let r = Replanner::default();
        let mut g = goal();
        g.status = GoalStatus::Completed;
        let d = r.advise(&ctx(1, Verification::Failure, false, true), &g, "2026-09-03T01:00:00Z");
        assert!(matches!(d, RecoveryDecision::AbandonGoal { .. }));
    }

    #[test]
    fn cancelled_goal_abandons() {
        let r = Replanner::default();
        let mut g = goal();
        g.status = GoalStatus::Cancelled;
        let d = r.advise(&ctx(1, Verification::Failure, false, true), &g, "2026-09-03T01:00:00Z");
        assert!(matches!(d, RecoveryDecision::AbandonGoal { .. }));
    }

    #[test]
    fn logical_failure_at_attempt3_escalates_even_with_alternatives() {
        let r = Replanner::default();
        let d = r.advise(&ctx(3, Verification::Failure, true, false), &goal(), "2026-09-03T01:00:00Z");
        assert!(matches!(d, RecoveryDecision::EscalateToUser { .. }));
    }

    #[test]
    fn environmental_failure_gets_one_more_retry_than_logical() {
        let r = Replanner::default();
        // Attempt 3 with budget 3 escalates; but with budget 4...
        let r2 = Replanner { max_attempts: 4, ..Default::default() };
        let env = r2.advise(&ctx(3, Verification::Failure, false, true), &goal(), "2026-09-03T01:00:00Z");
        assert!(matches!(env, RecoveryDecision::Retry { .. }), "environmental retries");
        let logical = r2.advise(&ctx(3, Verification::Failure, false, false), &goal(), "2026-09-03T01:00:00Z");
        assert!(matches!(logical, RecoveryDecision::EscalateToUser { .. }), "logical escalates");
    }

    #[test]
    fn stale_plan_detection() {
        let g = goal();
        let now = "2026-09-03T01:00:00Z";
        assert!(!Replanner::plan_is_stale(super::super::planner::PlanStatus::Ready, &g, now));
        assert!(Replanner::plan_is_stale(super::super::planner::PlanStatus::Rejected, &g, now));
        assert!(Replanner::plan_is_stale(super::super::planner::PlanStatus::Stale, &g, now));
        let mut dead = g.clone();
        dead.expires_at = "2020-01-01T00:00:00Z".into();
        assert!(Replanner::plan_is_stale(super::super::planner::PlanStatus::Ready, &dead, now));
    }

    #[test]
    fn next_plan_after_failure_maps_decisions() {
        use super::super::planner::plan as plan_goal;
        let g = goal();
        let fresh = || plan_goal(&g);
        let alt = || plan_goal(&g); // same planner; alternative is orthogonal here

        // Retry → keep the same plan (None means "no new plan needed").
        assert!(next_plan_after_failure(&RecoveryDecision::Retry { attempt: 2, backoff_secs: 5 }, fresh, alt).is_none());
        // Escalate → stop.
        assert!(next_plan_after_failure(&RecoveryDecision::EscalateToUser { reason: "x".into() }, fresh, alt).is_none());
        // Abandon → stop.
        assert!(next_plan_after_failure(&RecoveryDecision::AbandonGoal { reason: "x".into() }, fresh, alt).is_none());
        // UseAlternative / Replan → new artifact.
        assert!(next_plan_after_failure(&RecoveryDecision::UseAlternative { attempt: 2 }, fresh, alt).is_some());
        assert!(next_plan_after_failure(&RecoveryDecision::Replan { attempt: 2 }, fresh, alt).is_some());
    }

    #[test]
    fn decisions_serialize_for_the_ledger() {
        let d = RecoveryDecision::Retry { attempt: 2, backoff_secs: 10 };
        let j = serde_json::to_string(&d).unwrap();
        assert!(j.contains("\"retry\""));
        assert!(j.contains("\"backoff_secs\""));
    }
}
