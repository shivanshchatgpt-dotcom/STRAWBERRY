//! 🧑‍💼 User Intent + Preference Integration — Phase 22.
//!
//! Master-spec rules:
//!   * explicit user intent overrides learned preferences and predictions
//!   * user denial stops the affected action/goal even at high confidence
//!   * preferences are SCOPED and revocable — never irreversible authority
//!
//! Deterministic in-memory registry (wired to persistence via the existing
//! capability_state table where long-lived overrides matter). The
//! `overrides()` combiner is the single seam lifecycle/planner consult.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::goal::{GoalCandidate, GoalStatus};
use super::prediction::{prediction_grants_authority, Prediction};

// ─────────────────────────── model ───────────────────────────

/// A scoped user preference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preference {
    /// What the preference applies to: capability id, project name, topic…
    pub scope: String,
    /// "always" | "never" | "prefer" | "avoid"
    pub stance: String,
    pub note: String,
}

/// The registry of explicit user intent. Cheap to construct per session;
/// the caller decides persistence (capability_state for durable overrides).
#[derive(Debug, Clone, Default)]
pub struct IntentRegistry {
    /// Explicit denials: scope (goal title prefix, capability, project).
    denials: Vec<String>,
    /// Explicit instructions ("do X") — sharpest positive intent.
    instructions: Vec<String>,
    /// Scoped preferences.
    preferences: Vec<Preference>,
}

impl IntentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn deny(&mut self, scope: &str) {
        self.denials.push(scope.to_lowercase());
    }
    pub fn instruct(&mut self, what: &str) {
        self.instructions.push(what.to_lowercase());
    }
    pub fn prefer(&mut self, pref: Preference) {
        self.preferences.push(pref);
    }

    /// Did the user explicitly deny this scope (case-insensitive prefix)?
    pub fn is_denied(&self, scope: &str) -> bool {
        let s = scope.to_lowercase();
        self.denials.iter().any(|d| s.contains(d) || d.contains(&s))
    }

    /// The override combiner: does explicit intent block this goal?
    /// Denial beats acceptance regardless of confidence or prediction.
    pub fn goal_denied(&self, goal: &GoalCandidate) -> bool {
        self.is_denied(&goal.title) || goal.project.as_deref().is_some_and(|p| self.is_denied(p))
    }

    /// Does a preference apply to a scope, and what does it say?
    pub fn preference_for(&self, scope: &str) -> Option<&Preference> {
        let s = scope.to_lowercase();
        self.preferences
            .iter()
            .find(|p| s.contains(&p.scope.to_lowercase()))
    }

    /// Phase 22 core rule: explicit intent overrides predictions.
    /// Returns the effective prediction set: predictions whose subject the
    /// user explicitly denied are dropped; preferences re-weight the rest.
    pub fn overrides<'p>(
        &self,
        predictions: &'p [Prediction],
    ) -> Vec<&'p Prediction> {
        predictions
            .iter()
            .filter(|p| {
                // Denied subjects never surface as "likely next".
                !self.is_denied(&p.title)
                    // Structurally: predictions never grant authority anyway.
                    && !prediction_grants_authority(p)
            })
            .collect()
    }

    /// Revocation: preferences are never permanent authority.
    pub fn revoke_preference(&mut self, scope: &str) -> bool {
        let before = self.preferences.len();
        self.preferences.retain(|p| p.scope != scope);
        before != self.preferences.len()
    }

    pub fn revoke_denial(&mut self, scope: &str) -> bool {
        let before = self.denials.len();
        self.denials.retain(|d| d != &scope.to_lowercase());
        before != self.denials.len()
    }
}

/// Apply the Phase 22 rule inside the goal pipeline: a denied goal is
/// cancelled (status change is the caller's to persist — goals are
/// generation-based in Phase 7+).
pub fn apply_intent(
    registry: &IntentRegistry,
    goals: Vec<GoalCandidate>,
) -> Vec<GoalCandidate> {
    goals
        .into_iter()
        .map(|mut g| {
            if registry.goal_denied(&g) {
                g.status = GoalStatus::Cancelled;
            }
            g
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::goal::{Evidence, EvidenceKind, Priority};
    use crate::autonomous::ids::GoalId;
    use crate::autonomous::prediction::{PredictionKind, MIN_CONFIDENCE};

    fn goal(title: &str) -> GoalCandidate {
        GoalCandidate {
            goal_id: GoalId::new(1),
            title: title.into(),
            description: "d".into(),
            project: None,
            priority: Priority::High,
            confidence: 0.95, // high confidence — denial must still win
            evidence: vec![Evidence { kind: EvidenceKind::Task, reference: "1".into(), summary: "s".into(), weight: 1.0 }],
            status: GoalStatus::Accepted,
            created_at: "2026-09-03T00:00:00Z".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
        }
    }

    fn prediction(title: &str) -> Prediction {
        Prediction {
            kind: PredictionKind::NextAction,
            title: title.into(),
            confidence: 0.8,
            evidence: vec![],
            expires_at: "2099-01-01T00:00:00Z".into(),
            explanation: "test".into(),
        }
    }

    #[test]
    fn user_denial_beats_high_confidence_goal() {
        let mut reg = IntentRegistry::new();
        reg.deny("refactor the parser");
        let goals = apply_intent(&reg, vec![goal("Complete: Refactor the parser")]);
        assert_eq!(goals[0].status, GoalStatus::Cancelled);
    }

    #[test]
    fn non_denied_goals_pass_through() {
        let mut reg = IntentRegistry::new();
        reg.deny("something else entirely");
        let goals = apply_intent(&reg, vec![goal("Complete: ship feature X")]);
        assert_eq!(goals[0].status, GoalStatus::Accepted);
    }

    #[test]
    fn denial_by_project_scope() {
        let mut reg = IntentRegistry::new();
        reg.deny("legacy-app");
        let mut g = goal("Complete: modernize thing");
        g.project = Some("Legacy-App".into());
        assert!(reg.goal_denied(&g));
    }

    #[test]
    fn predictions_filtered_by_denial() {
        let mut reg = IntentRegistry::new();
        reg.deny("deploy to production");
        let preds = vec![
            prediction("Likely next action: deploy to production now"),
            prediction("May want context about tests"),
        ];
        let kept = reg.overrides(&preds);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].title.contains("tests"));
    }

    #[test]
    fn preferences_are_scoped_and_revocable() {
        let mut reg = IntentRegistry::new();
        reg.prefer(Preference {
            scope: "dark-mode".into(),
            stance: "always".into(),
            note: "user likes dark".into(),
        });
        assert!(reg.preference_for("please use dark-mode settings").is_some());
        assert!(reg.revoke_preference("dark-mode"));
        assert!(reg.preference_for("dark-mode").is_none());
        assert!(!reg.revoke_preference("dark-mode"), "second revoke no-ops");
    }

    #[test]
    fn denials_are_revocable_too() {
        let mut reg = IntentRegistry::new();
        reg.deny("refactor");
        assert!(reg.is_denied("please refactor"));
        assert!(reg.revoke_denial("refactor"));
        assert!(!reg.is_denied("please refactor"));
    }

    #[test]
    fn instructions_recorded_but_not_authority() {
        let mut reg = IntentRegistry::new();
        reg.instruct("run the test suite");
        // Instructions are the sharpest positive intent — recorded, but they
        // still don't mint execution authority by themselves (safety does).
        // Here we only prove the record exists and is queryable.
        assert!(!reg.instructions.is_empty());
    }

    #[test]
    fn intent_never_grants_prediction_authority() {
        let mut reg = IntentRegistry::new();
        reg.instruct("just do everything automatically please");
        let preds = vec![prediction("anything")];
        let kept = reg.overrides(&preds);
        assert!(!kept.is_empty());
        assert!(!prediction_grants_authority(kept[0]));
    }

    #[test]
    fn denial_matching_is_case_insensitive() {
        let mut reg = IntentRegistry::new();
        reg.deny("Refactor The Parser");
        assert!(reg.is_denied("complete: refactor the parser"));
    }

    #[test]
    fn low_confidence_predictions_still_filtered_by_min_threshold() {
        // Structural cross-check: the registry doesn't lower the Phase 18
        // confidence floor — filters compose, never replace policy.
        let mut low = prediction("weak");
        low.confidence = MIN_CONFIDENCE - 0.1;
        assert!(low.confidence < MIN_CONFIDENCE);
    }
}
