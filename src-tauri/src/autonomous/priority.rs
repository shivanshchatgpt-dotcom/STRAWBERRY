//! ⚖️ Priority & Conflict Resolution — Phase 21 of the Strawberry platform.
//!
//! The master spec's 15-level precedence ladder, deterministic and
//! auditable. Every conflict between two decision inputs resolves through
//! `resolve()` — which returns a verdict AND the reasoning chain, so no
//! dangerous conflict is ever resolved randomly.
//!
//! Ambiguity policy (spec):
//!   * low ambiguity  → choose the SAFER option
//!   * medium         → defer
//!   * high           → ask the user
//!   * safety-ambiguous → BLOCK (always)

use serde::{Deserialize, Serialize};

// ─────────────────────────── model ───────────────────────────

/// The 15 precedence levels (1 = highest). Fixed ordering, spec-mandated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precedence {
    SafetySecurity = 1,
    Privacy = 2,
    ExplicitUserDenial = 3,
    ExplicitUserInstruction = 4,
    HardSystemConstraint = 5,
    ActiveCriticalBlocker = 6,
    UrgencyDeadline = 7,
    ActiveGoalRelevance = 8,
    ProjectImportance = 9,
    DependencyValue = 10,
    ExpectedUserValue = 11,
    Confidence = 12,
    ResourceEfficiency = 13,
    BackgroundValue = 14,
    Convenience = 15,
}

/// What kind of signal is competing in a conflict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    SafetyViolation { description: String },
    PrivacyConcern { description: String },
    UserDenied { what: String },
    UserInstructed { what: String },
    SystemConstraint { description: String },
    CriticalBlocker { description: String },
    Deadline { at: String },
    GoalRelevance { goal_id: u64, weight: f32 },
    ProjectValue { project: String, weight: f32 },
    Value { weight: f32 },
    HighConfidence { weight: f32 },
    Efficiency { weight: f32 },
    Background { weight: f32 },
    Convenience { weight: f32 },
}

impl Signal {
    /// Deterministic precedence of a signal.
    pub fn precedence(&self) -> Precedence {
        match self {
            Signal::SafetyViolation { .. } => Precedence::SafetySecurity,
            Signal::PrivacyConcern { .. } => Precedence::Privacy,
            Signal::UserDenied { .. } => Precedence::ExplicitUserDenial,
            Signal::UserInstructed { .. } => Precedence::ExplicitUserInstruction,
            Signal::SystemConstraint { .. } => Precedence::HardSystemConstraint,
            Signal::CriticalBlocker { .. } => Precedence::ActiveCriticalBlocker,
            Signal::Deadline { .. } => Precedence::UrgencyDeadline,
            Signal::GoalRelevance { .. } => Precedence::ActiveGoalRelevance,
            Signal::ProjectValue { .. } => Precedence::ProjectImportance,
            Signal::Value { .. } => Precedence::ExpectedUserValue,
            Signal::HighConfidence { .. } => Precedence::Confidence,
            Signal::Efficiency { .. } => Precedence::ResourceEfficiency,
            Signal::Background { .. } => Precedence::BackgroundValue,
            Signal::Convenience { .. } => Precedence::Convenience,
        }
    }
}

/// Conflict resolution outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// The higher-precedence (safer/more-authoritative) signal wins.
    Winner { signal: Signal, because: String },
    /// Medium ambiguity — nothing happens until more evidence arrives.
    Defer { reason: String },
    /// High ambiguity — the user must decide.
    AskUser { question: String },
    /// Safety itself was ambiguous — always block.
    Block { reason: String },
}

/// How ambiguous the conflict was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ambiguity {
    Low,
    Medium,
    High,
}

// ─────────────────────────── the resolver ───────────────────────────

pub struct ConflictResolver;

impl ConflictResolver {
    /// Resolve a conflict between two signals. Deterministic; every outcome
    /// carries the full explainability chain.
    pub fn resolve(a: &Signal, b: &Signal) -> Resolution {
        // 1. Safety ambiguity is ALWAYS block — even if the other side is
        //    also safety (we can't rank two safety violations safely).
        let safety_involved =
            matches!(a, Signal::SafetyViolation { .. }) || matches!(b, Signal::SafetyViolation { .. });
        if safety_involved {
            let desc = if let Signal::SafetyViolation { description } = a {
                description.clone()
            } else if let Signal::SafetyViolation { description } = b {
                description.clone()
            } else {
                "safety signal present".into()
            };
            return Resolution::Block {
                reason: format!("safety ambiguity is never resolved automatically: {desc}"),
            };
        }

        // 2. User denial beats everything except safety (already handled) —
        //    including user instructions (denial is the sharper intent).
        if let Signal::UserDenied { what } = a {
            return Resolution::Winner {
                signal: a.clone(),
                because: format!("explicit user denial of “{what}” wins"),
            };
        }
        if let Signal::UserDenied { what } = b {
            return Resolution::Winner {
                signal: b.clone(),
                because: format!("explicit user denial of “{what}” wins"),
            };
        }

        // 3. Same precedence level → weighted tiebreak, and if weights are
        //    also close, the ambiguity ladder decides.
        let (pa, pb) = (a.precedence(), b.precedence());
        if pa == pb {
            let (wa, wb) = (weight_of(a), weight_of(b));
            let diff = (wa - wb).abs();
            if diff > 0.2 {
                let (winner, because) = if wa > wb { (a, b) } else { (b, a) };
                return Resolution::Winner {
                    signal: winner.clone(),
                    because: format!(
                        "same precedence {pa:?}, weight {wa:.2} vs {wb:.2} favors the winner (loser: {because:?})"
                    ),
                };
            }
            // Weights indistinguishable → medium/high ambiguity ladder.
            return match ambiguity_of(pa) {
                Ambiguity::Low => Resolution::Winner {
                    signal: a.clone(),
                    because: format!(
                        "low-ambiguity tie at {pa:?} resolved to the first (deterministic) signal"
                    ),
                },
                Ambiguity::Medium => Resolution::Defer {
                    reason: format!("medium ambiguity at {pa:?}; wait for more evidence"),
                },
                Ambiguity::High => Resolution::AskUser {
                    question: format!("two {pa:?} signals compete; which should Strawberry follow?"),
                },
            };
        }

        // 4. Different precedence: higher (lower number) wins — never random.
        let (winner, loser) = if pa < pb { (a, b) } else { (b, a) };
        Resolution::Winner {
            signal: winner.clone(),
            because: format!(
                "{:?} (precedence {:?}) beats {:?} (precedence {:?})",
                winner,
                winner.precedence(),
                loser,
                loser.precedence()
            ),
        }
    }
}

fn weight_of(s: &Signal) -> f32 {
    match s {
        Signal::GoalRelevance { weight, .. }
        | Signal::ProjectValue { weight, .. }
        | Signal::Value { weight }
        | Signal::HighConfidence { weight }
        | Signal::Efficiency { weight }
        | Signal::Background { weight }
        | Signal::Convenience { weight } => *weight,
        _ => 1.0,
    }
}

fn ambiguity_of(p: Precedence) -> Ambiguity {
    // Deterministic mapping: value-vs-value conflicts are medium (defer),
    // convenience/background are low (pick deterministic), human-intent
    // levels are high (ask).
    match p {
        Precedence::ExpectedUserValue
        | Precedence::Confidence
        | Precedence::ResourceEfficiency => Ambiguity::Medium,
        Precedence::BackgroundValue | Precedence::Convenience => Ambiguity::Low,
        _ => Ambiguity::High,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_beats_everything() {
        for other in [
            Signal::UserInstructed { what: "do it now".into() },
            Signal::Deadline { at: "2026-09-04".into() },
            Signal::Convenience { weight: 1.0 },
        ] {
            let s = Signal::SafetyViolation { description: "would delete data".into() };
            match ConflictResolver::resolve(&s, &other) {
                Resolution::Block { reason } => assert!(reason.contains("never resolved automatically")),
                other_outcome => panic!("safety must block, got {other_outcome:?}"),
            }
        }
    }

    #[test]
    fn user_denial_beats_user_instruction_and_goals() {
        let deny = Signal::UserDenied { what: "refactor".into() };
        let instruct = Signal::UserInstructed { what: "refactor now".into() };
        match ConflictResolver::resolve(&deny, &instruct) {
            Resolution::Winner { because, .. } => assert!(because.contains("denial")),
            o => panic!("denial must win, got {o:?}"),
        }
        let goal = Signal::GoalRelevance { goal_id: 9, weight: 0.99 };
        match ConflictResolver::resolve(&goal, &deny) {
            Resolution::Winner { signal, .. } => assert!(matches!(signal, Signal::UserDenied { .. })),
            o => panic!("denial must win over goals, got {o:?}"),
        }
    }

    #[test]
    fn higher_precedence_wins_deterministically() {
        let privacy = Signal::PrivacyConcern { description: "sensitive prompt".into() };
        let value = Signal::Value { weight: 1.0 };
        match ConflictResolver::resolve(&value, &privacy) {
            Resolution::Winner { signal, .. } => {
                assert!(matches!(signal, Signal::PrivacyConcern { .. }))
            }
            o => panic!("privacy must beat value, got {o:?}"),
        }
    }

    #[test]
    fn same_precedence_weighted_tiebreak() {
        let a = Signal::Value { weight: 0.9 };
        let b = Signal::Value { weight: 0.3 };
        match ConflictResolver::resolve(&a, &b) {
            Resolution::Winner { signal, because } => {
                assert!(matches!(signal, Signal::Value { weight: 0.9 }));
                assert!(because.contains("weight"));
            }
            o => panic!("weighted tiebreak expected, got {o:?}"),
        }
    }

    #[test]
    fn medium_ambiguity_defers() {
        let a = Signal::Value { weight: 0.5 };
        let b = Signal::Value { weight: 0.55 };
        match ConflictResolver::resolve(&a, &b) {
            Resolution::Defer { reason } => assert!(reason.contains("medium ambiguity")),
            o => panic!("close weights must defer, got {o:?}"),
        }
    }

    #[test]
    fn high_ambiguity_asks_user() {
        let a = Signal::UserInstructed { what: "stop the server".into() };
        let b = Signal::UserInstructed { what: "restart the server".into() };
        match ConflictResolver::resolve(&a, &b) {
            Resolution::AskUser { question } => assert!(question.contains("which should Strawberry")),
            o => panic!("competing instructions must ask, got {o:?}"),
        }
    }

    #[test]
    fn convenience_never_beats_goals() {
        let conv = Signal::Convenience { weight: 1.0 };
        let goal = Signal::GoalRelevance { goal_id: 1, weight: 0.05 };
        match ConflictResolver::resolve(&conv, &goal) {
            Resolution::Winner { signal, .. } => assert!(matches!(signal, Signal::GoalRelevance { .. })),
            o => panic!("goals beat convenience, got {o:?}"),
        }
    }

    #[test]
    fn resolution_is_deterministic_and_auditable() {
        let a = Signal::Deadline { at: "2026-09-04".into() };
        let b = Signal::Background { weight: 0.5 };
        let r1 = ConflictResolver::resolve(&a, &b);
        let r2 = ConflictResolver::resolve(&a, &b);
        assert_eq!(r1, r2);
        if let Resolution::Winner { because, .. } = r1 {
            assert!(!because.is_empty(), "every winner carries its reason");
        }
    }

    #[test]
    fn safety_vs_safety_always_blocks() {
        // Two safety signals can't be ranked — block is the only safe answer.
        let a = Signal::SafetyViolation { description: "risk x".into() };
        let b = Signal::SafetyViolation { description: "risk y".into() };
        assert!(matches!(
            ConflictResolver::resolve(&a, &b),
            Resolution::Block { .. }
        ));
    }

    #[test]
    fn precedence_order_matches_spec() {
        assert!(Precedence::SafetySecurity < Precedence::Privacy);
        assert!(Precedence::Privacy < Precedence::ExplicitUserDenial);
        assert!(Precedence::ExplicitUserDenial < Precedence::ExplicitUserInstruction);
        assert!(Precedence::UrgencyDeadline < Precedence::ActiveGoalRelevance);
        assert!(Precedence::Confidence < Precedence::ResourceEfficiency);
        assert!(Precedence::BackgroundValue < Precedence::Convenience);
    }
}
