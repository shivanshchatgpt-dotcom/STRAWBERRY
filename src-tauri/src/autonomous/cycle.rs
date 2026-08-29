//! Autonomy cycle — one full pass of OBSERVE → VERIFY → LEARN.

use std::time::Instant;
use serde::{Deserialize, Serialize};
use super::ids::CycleId;
use super::world_state::WorldStateVersion;

/// One full autonomy cycle, from observation through verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyCycle {
    pub cycle_id: CycleId,
    pub started_at_ms: i64,
    pub world_state_version: WorldStateVersion,
    pub events_consumed: usize,
    pub outcome: CycleOutcome,
}

impl AutonomyCycle {
    pub fn new(world_state_version: WorldStateVersion) -> Self {
        let now = now_ms() as i64;
        Self {
            cycle_id: CycleId::new(now as u64),
            started_at_ms: now,
            world_state_version,
            events_consumed: 0,
            outcome: CycleOutcome::Pending,
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        let now = now_ms() as i64;
        (now.saturating_sub(self.started_at_ms)).max(0) as u64
    }
}

/// Possible outcomes of a single autonomy cycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CycleOutcome {
    /// No action taken — nothing interesting happened.
    Pending,
    /// Observed events, generated a goal, planned and executed an action.
    Executed,
    /// Observed events, generated a goal, but no plan was feasible.
    NoPlan,
    /// Plan was generated but the safety gate denied it.
    SafetyDenied,
    /// Plan was executed but verification failed.
    VerificationFailed,
    /// The runtime was paused or shut down mid-cycle.
    Aborted,
    /// An internal error occurred.
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleResult {
    pub cycle: AutonomyCycle,
    pub success: bool,
    pub message: String,
}

impl CycleResult {
    pub fn ok(cycle: AutonomyCycle, message: impl Into<String>) -> Self {
        Self { cycle, success: true, message: message.into() }
    }
    pub fn fail(cycle: AutonomyCycle, message: impl Into<String>) -> Self {
        Self { cycle, success: false, message: message.into() }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_starts_pending() {
        let c = AutonomyCycle::new(0);
        assert_eq!(c.outcome, CycleOutcome::Pending);
        assert_eq!(c.events_consumed, 0);
    }

    #[test]
    fn elapsed_non_negative() {
        let c = AutonomyCycle::new(0);
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(c.elapsed_ms() >= 1);
    }
}
