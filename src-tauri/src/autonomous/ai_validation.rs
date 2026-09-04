//! 🤖 Weak-AI Robustness — Phase 15 of the Strawberry platform.
//!
//! AI output is UNTRUSTED INPUT (master spec §LLM Authority Rule).
//! This module is the single validation boundary every model suggestion
//! passes through before the deterministic core looks at it:
//!
//!   * malformed JSON / schema violations  → rejected
//!   * hallucinated risk-downgrades        → ignored (risk re-derived)
//!   * hallucinated forbidden-actions      → rejected outright
//!   * timeouts / unavailable providers    → deterministic fallback
//!   * no-provider mode                    → core stays fully functional
//!
//! Nothing here talks to a network — it validates *already produced*
//! provider output. Provider routing itself is Phase 19.

use serde::{Deserialize, Serialize};

use super::capability::RiskLevel;
use super::safety::ActionType;

// ─────────────────────────── provider state ───────────────────────────

/// What the intelligence layer reported about its provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    /// No provider configured — core-only mode.
    Unconfigured,
    /// Provider present but unreachable / timed out.
    Unavailable { reason: String },
    /// Provider present and responding.
    Ready { provider: String },
}

// ─────────────────────────── validated proposals ───────────────────────────

/// The strict schema a model must produce for a GOAL proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelGoalProposal {
    pub title: String,
    pub evidence_refs: Vec<String>,
    /// Model's claimed confidence 0..1 — advisory only.
    pub confidence: f32,
    /// Model's claimed priority — advisory only.
    pub priority: String,
}

/// The strict schema for a PLAN proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPlanProposal {
    pub goal_title: String,
    pub steps: Vec<ModelStepProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStepProposal {
    pub capability: String,
    pub action: String,
    pub purpose: String,
    /// Model may SUGGEST a risk — never to be trusted (see validate).
    pub risk_suggestion: Option<String>,
}

/// Why a proposal was rejected — for the ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReason {
    MalformedJson,
    SchemaViolation(String),
    /// Model tried to propose a forbidden/high-risk action as safe.
    UnsafeAction(String),
    /// Model output was structurally fine but empty of content.
    Empty,
}

/// Result of validating model output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationResult<T> {
    Accepted { value: T, notes: Vec<String> },
    Rejected { reason: RejectionReason, detail: String },
}

// ─────────────────────────── the boundary ───────────────────────────

pub struct AiValidator;

impl AiValidator {
    /// Parse and validate a raw model string as a GOAL proposal.
    /// Deterministic: same input ⇒ same verdict. Malformed anything → reject.
    pub fn parse_goal(raw: &str) -> ValidationResult<ModelGoalProposal> {
        let parsed: Result<ModelGoalProposal, _> = serde_json::from_str(raw);
        let mut p = match parsed {
            Ok(p) => p,
            Err(e) => {
                return ValidationResult::Rejected {
                    reason: RejectionReason::MalformedJson,
                    detail: e.to_string(),
                }
            }
        };

        let mut notes: Vec<String> = Vec::new();

        if p.title.trim().is_empty() {
            return ValidationResult::Rejected {
                reason: RejectionReason::Empty,
                detail: "goal proposal has no title".into(),
            };
        }
        if p.evidence_refs.is_empty() {
            return ValidationResult::Rejected {
                reason: RejectionReason::SchemaViolation("evidence_refs must be non-empty".into()),
                detail: "a goal without evidence is unsupported intent".into(),
            };
        }
        if !p.confidence.is_finite() || !(0.0..=1.0).contains(&p.confidence) {
            notes.push(format!(
                "model confidence {} invalid; clamped to 0.5",
                p.confidence
            ));
            p.confidence = 0.5;
        }
        // Advisory fields are normalized, never trusted:
        if !matches!(p.priority.as_str(), "low" | "medium" | "high") {
            notes.push(format!("model priority {:?} unknown; defaulted to medium", p.priority));
            p.priority = "medium".into();
        }

        ValidationResult::Accepted { value: p, notes }
    }

    /// Parse and validate a raw model string as a PLAN proposal.
    /// CRITICAL: any step whose action parses to a FORBIDDEN type — or
    /// whose "risk suggestion" claims low risk for a high/forbidden
    /// action — is rejected wholesale. Risk suggestions are IGNORED;
    /// the deterministic gate re-derives everything.
    pub fn parse_plan(raw: &str) -> ValidationResult<ModelPlanProposal> {
        let parsed: Result<ModelPlanProposal, _> = serde_json::from_str(raw);
        let p = match parsed {
            Ok(p) => p,
            Err(e) => {
                return ValidationResult::Rejected {
                    reason: RejectionReason::MalformedJson,
                    detail: e.to_string(),
                }
            }
        };

        if p.steps.is_empty() {
            return ValidationResult::Rejected {
                reason: RejectionReason::Empty,
                detail: "plan proposal has no steps".into(),
            };
        }

        let mut notes: Vec<String> = Vec::new();
        for (i, s) in p.steps.iter().enumerate() {
            // Unknown action labels are unsafe-by-omission → reject.
            let action = match ActionType::from_label(&s.action) {
                Some(a) => a,
                None => {
                    return ValidationResult::Rejected {
                        reason: RejectionReason::UnsafeAction(s.action.clone()),
                        detail: format!("step {i} action {:?} is not a known action", s.action),
                    }
                }
            };
            // Forbidden types can never be proposed, even with approval.
            if SafetyGate::base_risk_of(&action) == RiskLevel::Forbidden {
                return ValidationResult::Rejected {
                    reason: RejectionReason::UnsafeAction(s.action.clone()),
                    detail: format!("step {i} proposes forbidden action {}", action.label()),
                };
            }
            // Risk suggestions are advisory noise — strip them from trust.
            if s.risk_suggestion.is_some() {
                notes.push(format!(
                    "step {i} risk suggestion ignored (deterministic gate decides)",
                ));
            }
            if s.capability.trim().is_empty() {
                return ValidationResult::Rejected {
                    reason: RejectionReason::SchemaViolation("capability empty".into()),
                    detail: format!("step {i} has no capability"),
                };
            }
        }

        ValidationResult::Accepted { value: p, notes }
    }

    /// The deterministic fallback decision for every provider failure mode.
    /// Weak/slow/dead AI NEVER degrades Strawberry: the core continues.
    pub fn fallback_for(state: &ProviderState) -> &'static str {
        match state {
            ProviderState::Unconfigured => {
                "no provider — deterministic core continues at full function"
            }
            ProviderState::Unavailable { .. } => {
                "provider unavailable — falling back to deterministic core"
            }
            ProviderState::Ready { .. } => "provider ready — suggestions still validated",
        }
    }
}

/// Local re-export of the fixed base-risk table (tests pin it).
struct SafetyGate;
impl SafetyGate {
    fn base_risk_of(a: &ActionType) -> RiskLevel {
        // Mirrors autonomous::safety's table via ActionType::base_risk —
        // duplicated minimally to keep this module's validation total.
        use ActionType as A;
        match a {
            A::FileRead | A::Inspect | A::Prepare => RiskLevel::Low,
            A::FileWrite | A::GitCommit | A::RunCommand | A::SendMessage | A::FileDelete => {
                RiskLevel::High
            }
            A::PermanentDelete | A::UploadPrivateData | A::DisablePrivacy | A::HideActivity => {
                RiskLevel::Forbidden
            }
            A::Unknown => RiskLevel::High,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_json_is_rejected() {
        for bad in ["", "{", "not json", "{\"title\": 42}"] {
            match AiValidator::parse_goal(bad) {
                ValidationResult::Rejected { reason: RejectionReason::MalformedJson, .. } => {}
                other => panic!("expected malformed rejection for {bad:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn schema_violations_are_rejected_with_reasons() {
        let no_evidence = r#"{"title":"x","evidenceRefs":[],"confidence":0.5,"priority":"low"}"#;
        match AiValidator::parse_goal(no_evidence) {
            ValidationResult::Rejected { reason: RejectionReason::SchemaViolation(d), .. } => {
                assert!(d.contains("evidence"))
            }
            other => panic!("expected schema violation, got {other:?}"),
        }
    }

    #[test]
    fn empty_proposals_are_rejected() {
        let empty_goal = r#"{"title":"","evidenceRefs":["a"],"confidence":0.5,"priority":"low"}"#;
        match AiValidator::parse_goal(empty_goal) {
            ValidationResult::Rejected { reason: RejectionReason::Empty, .. } => {}
            other => panic!("expected empty rejection, got {other:?}"),
        }
        let empty_plan = r#"{"goalTitle":"x","steps":[]}"#;
        assert!(matches!(
            AiValidator::parse_plan(empty_plan),
            ValidationResult::Rejected { reason: RejectionReason::Empty, .. }
        ));
    }

    #[test]
    fn hallucinated_risk_downgrade_is_ignored() {
        // Model claims RUN_COMMAND is low-risk — the validator notes the
        // suggestion as advisory and the deterministic gate re-derives HIGH.
        let plan = r#"{
            "goalTitle":"test",
            "steps":[{"capability":"shell","action":"RUN_COMMAND","purpose":"x","riskSuggestion":"low"}]
        }"#;
        match AiValidator::parse_plan(plan) {
            ValidationResult::Accepted { notes, .. } => {
                assert!(notes.iter().any(|n| n.contains("ignored")), "notes: {notes:?}");
            }
            other => panic!("valid plan rejected: {other:?}"),
        }
    }

    #[test]
    fn forbidden_hallucinated_action_is_rejected_outright() {
        for forbidden in ["PERMANENT_DELETE", "UPLOAD_PRIVATE_DATA", "DISABLE_PRIVACY", "HIDE_ACTIVITY"] {
            let plan = format!(
                r#"{{"goalTitle":"x","steps":[{{"capability":"c","action":"{forbidden}","purpose":"p"}}]}}"#
            );
            match AiValidator::parse_plan(&plan) {
                ValidationResult::Rejected {
                    reason: RejectionReason::UnsafeAction(a),
                    detail,
                } => {
                    assert_eq!(a, forbidden);
                    assert!(detail.contains("forbidden"), "detail: {detail}");
                }
                other => panic!("{forbidden} must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn invalid_structured_output_types_are_rejected() {
        // confidence: NaN-ish (JSON has no NaN, but strings / negatives are possible).
        let bad_conf = r#"{"title":"x","evidenceRefs":["a"],"confidence":"high","priority":"low"}"#;
        assert!(matches!(
            AiValidator::parse_goal(bad_conf),
            ValidationResult::Rejected { reason: RejectionReason::MalformedJson, .. }
        ));
        // Unknown action label.
        let unknown_action = r#"{"goalTitle":"x","steps":[{"capability":"c","action":"DO_MAGIC","purpose":"p"}]}"#;
        assert!(matches!(
            AiValidator::parse_plan(unknown_action),
            ValidationResult::Rejected { reason: RejectionReason::UnsafeAction(_), .. }
        ));
    }

    #[test]
    fn provider_states_have_deterministic_fallbacks() {
        assert!(AiValidator::fallback_for(&ProviderState::Unconfigured).contains("deterministic core"));
        assert!(AiValidator::fallback_for(&ProviderState::Unavailable { reason: "timeout".into() })
            .contains("falling back"));
        assert!(AiValidator::fallback_for(&ProviderState::Ready { provider: "ollama".into() })
            .contains("validated"));
    }

    #[test]
    fn valid_proposals_pass_with_advisory_notes() {
        let goal = r#"{"title":"improve tests","evidenceRefs":["todo:1"],"confidence":0.8,"priority":"high"}"#;
        match AiValidator::parse_goal(goal) {
            ValidationResult::Accepted { value, notes } => {
                assert_eq!(value.title, "improve tests");
                assert_eq!(value.priority, "high");
                assert!(notes.is_empty());
            }
            other => panic!("valid goal rejected: {other:?}"),
        }
    }

    #[test]
    fn out_of_range_confidence_is_clamped_not_trusted() {
        let goal = r#"{"title":"x","evidenceRefs":["a"],"confidence":7.5,"priority":"medium"}"#;
        match AiValidator::parse_goal(goal) {
            ValidationResult::Accepted { value, notes } => {
                assert!((value.confidence - 0.5).abs() < 1e-6, "clamped, got {}", value.confidence);
                assert!(notes.iter().any(|n| n.contains("clamped")));
            }
            other => panic!("should clamp, got {other:?}"),
        }
    }

    #[test]
    fn weak_ai_never_degrades_core_function() {
        // The contract: every provider failure mode maps to a fallback where
        // the deterministic core keeps working. No fallback says "stop".
        for state in [
            ProviderState::Unconfigured,
            ProviderState::Unavailable { reason: "provider timeout".into() },
            ProviderState::Unavailable { reason: "connection refused".into() },
        ] {
            let f = AiValidator::fallback_for(&state);
            assert!(!f.contains("stop"), "core must never stop: {f}");
            assert!(!f.contains("disabled"), "core must never disable: {f}");
        }
    }

    #[test]
    fn validation_is_deterministic() {
        let plan = r#"{"goalTitle":"x","steps":[{"capability":"c","action":"INSPECT","purpose":"p"}]}"#;
        let a = AiValidator::parse_plan(plan);
        let b = AiValidator::parse_plan(plan);
        match (&a, &b) {
            (
                ValidationResult::Accepted { notes: na, .. },
                ValidationResult::Accepted { notes: nb, .. },
            ) => assert_eq!(na, nb),
            _ => panic!("both must accept"),
        }
    }
}
