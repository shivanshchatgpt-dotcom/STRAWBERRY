//! 🤖 Autonomous Core — Strawberry's Rust-native, no-LLM, event-driven worker.
//!
//! Architecture (see prompt §30):
//!   Phase 1: World State
//!   Phase 2: Runtime
//!   Phase 3: Goal Engine
//!   Phase 4: Action Registry
//!   Phase 5: Safety Gate
//!   Phase 6: HTN Planner
//!   Phase 7: Executor
//!   Phase 8: Verifier
//!   Phase 9: Replanning
//!   Phase 10+: Memory, Skills, Tree-sitter, Graph, Prediction, Learning
//!
//! **Constraints (non-negotiable):**
//!   * NO LLM
//!   * NO API key
//!   * NO cloud
//!   * NO Python
//!   * NO OLLAMA
//!   * Rust-native
//!   * Event-driven
//!   * Deterministic
//!   * Safety-gated
//!   * Locally persistent
//!
//! The core loop:
//!   OBSERVE → NORMALIZE → UPDATE WORLD STATE → RECALL MEMORY
//!   → DETECT GOAL → PREDICT → PLAN → SCORE → SAFETY GATE
//!   → EXECUTE → VERIFY → LEARN → REPLAN OR CONTINUE

pub mod world_state;
pub mod runtime;
pub mod cycle;
pub mod event;
pub mod adapter;
pub mod file_watcher;
pub mod session;

pub use world_state::{WorldState, WorldStateDiff, WorldStateVersion};
pub use runtime::{AutonomyRuntime, RuntimeMode, RuntimeStats};
pub use cycle::{AutonomyCycle, CycleResult, CycleOutcome};
pub use event::{NormalizedEvent, EventKind, EventBus};
pub use adapter::{SourceAdapter, RawSignal, AdapterInfo};

/// Strongly-typed identifiers used across the autonomy stack.
pub mod ids {
    use std::fmt;

    macro_rules! typed_id {
        ($name:ident) => {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
            #[serde(transparent)]
            pub struct $name(pub u64);

            impl $name {
                pub const fn new(v: u64) -> Self { Self(v) }
                pub const fn raw(self) -> u64 { self.0 }
            }

            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(f, "{}({})", stringify!($name), self.0)
                }
            }
        };
    }

    typed_id!(AppId);
    typed_id!(WindowId);
    typed_id!(ProjectId);
    typed_id!(FileId);
    typed_id!(TaskId);
    typed_id!(ActionId);
    typed_id!(PlanId);
    typed_id!(GoalId);
    typed_id!(SkillId);
    typed_id!(CycleId);
    typed_id!(EventId);
}

#[cfg(test)]
mod tests {
    use super::ids::*;

    #[test]
    fn typed_ids_distinct() {
        // AppId and WindowId are different types, so they can't even be compared
        // with assert_ne!. Just confirm the types are constructible and equal to themselves.
        assert_eq!(AppId(7), AppId(7));
        assert_eq!(WindowId(1), WindowId(1));
    }
}
