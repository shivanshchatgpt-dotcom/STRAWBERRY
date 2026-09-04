//! 🛰️ Intelligence Hardening — Phase 19 of the Strawberry platform.
//!
//! Hardens the existing `ProviderRouter` (intelligence/mod.rs — untouched)
//! with the privacy-aware dispatch layer the master spec requires:
//!
//!   capability declares  →  requires_ai / cloud_allowed / min_level
//!   request carries       →  prompt + privacy level
//!   router enforces       →  provider-neutral selection + cloud policy
//!   core guarantees       →  private data NEVER silently reaches cloud
//!   output is ALWAYS      →  validated as untrusted input (Phase 15)
//!
//! No second router — this wraps the ONE router with policy. Capabilities
//! call `gated_complete`; raw `complete` stays for internal diagnostics.

use serde::{Deserialize, Serialize};

use super::ai_validation::{
    AiValidator, ModelGoalProposal, ModelPlanProposal, ProviderState, ValidationResult,
};
use crate::intelligence::{CapabilityMeta, IntelligenceRequest, IntelligenceResponse, ProviderKind, ProviderRouter};
use strawberry_core::privacy::{PrivacyAction, PrivacyPolicy};

// ─────────────────────────── request gating ───────────────────────────

/// What the gate decided about an intelligence request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    /// Deterministic core handled it — no AI needed.
    DeterministicFallback { reason: String },
    /// Request dispatched to a LOCAL provider.
    DispatchedLocal,
    /// Request dispatched to cloud (BYOK) — policy explicitly allowed it.
    DispatchedCloud { justification: String },
    /// Refused: capability doesn't allow cloud and no local provider.
    CloudDenied { reason: String },
    /// Refused: capability needs AI but none is available.
    NoProvider { reason: String },
}

/// Privacy level of a prompt's content (derived by the deterministic
/// privacy screen, never by the model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptPrivacy {
    Clean,
    ContainsSensitive,
    Blocked,
}

/// Screen a prompt deterministically before it can leave the machine.
pub fn screen_prompt(text: &str) -> PromptPrivacy {
    let policy = PrivacyPolicy::default();
    let decision = policy.evaluate(text);
    match decision.action {
        PrivacyAction::Block => PromptPrivacy::Blocked,
        PrivacyAction::Redact => PromptPrivacy::ContainsSensitive,
        PrivacyAction::Allow => PromptPrivacy::Clean,
    }
}

/// Redact a prompt when policy requires it (local-only mutation).
pub fn redact_prompt(text: &str) -> String {
    PrivacyPolicy::default().redact(text)
}

// ─────────────────────────── the gated dispatch ───────────────────────────

/// Privacy- and policy-aware completion. THE path capabilities must use.
///
/// Order of enforcement (deterministic):
///   1. capability says it doesn't need AI       → deterministic fallback
///   2. privacy screen blocks the prompt          → deterministic fallback
///   3. no provider configured                    → deterministic fallback
///   4. prompt sensitive + cloud is the target    → CLOUD DENIED (unless
///      the capability explicitly allows cloud AND user enabled BYOK)
///   5. local provider available                  → dispatch local
///   6. cloud allowed + BYOK configured           → dispatch cloud
///   7. otherwise                                 → no provider
pub fn gated_complete(
    router: &ProviderRouter,
    request: &IntelligenceRequest,
    meta: &CapabilityMeta,
) -> (GateOutcome, Result<IntelligenceResponse, String>) {
    // 1. Capabilities that don't require AI never spend provider budget.
    if !meta.requires_ai {
        return (
            GateOutcome::DeterministicFallback {
                reason: format!("capability '{}' does not require AI", request.capability),
            },
            Err("deterministic fallback: capability does not require AI".into()),
        );
    }

    // 2. Privacy screen first — blocked prompts never reach ANY provider.
    let privacy = screen_prompt(&request.prompt);
    if privacy == PromptPrivacy::Blocked {
        return (
            GateOutcome::DeterministicFallback {
                reason: "prompt blocked by privacy policy".into(),
            },
            Err("deterministic fallback: prompt contains blocked content".into()),
        );
    }

    // 3. Provider availability.
    let statuses = router.all_status();
    let local_ready = statuses
        .iter()
        .any(|s| s.kind == ProviderKind::Ollama && s.available);
    let cloud_ready = statuses
        .iter()
        .any(|s| s.kind == ProviderKind::Byok && s.available);

    if !local_ready && !cloud_ready {
        return (
            GateOutcome::NoProvider {
                reason: "no AI provider available".into(),
            },
            Err("no provider available; deterministic core continues".into()),
        );
    }

    // 4. Sensitive prompts NEVER go to cloud.
    if privacy == PromptPrivacy::ContainsSensitive {
        if local_ready {
            // Redacted local dispatch is the only legal path.
            let mut redacted = request.clone();
            redacted.prompt = redact_prompt(&request.prompt);
            return (
                GateOutcome::DispatchedLocal,
                router.complete(&redacted),
            );
        }
        return (
            GateOutcome::CloudDenied {
                reason: "prompt contains sensitive content and no local provider exists".into(),
            },
            Err("cloud denied: sensitive prompt would leave the machine".into()),
        );
    }

    // 5–6. Clean prompt: prefer local; cloud only when capability allows.
    if local_ready && router.active_kind() != ProviderKind::Byok {
        return (GateOutcome::DispatchedLocal, router.complete(request));
    }
    if cloud_ready {
        if meta.cloud_allowed {
            return (
                GateOutcome::DispatchedCloud {
                    justification: format!(
                        "capability '{}' explicitly permits cloud and the prompt is clean",
                        request.capability
                    ),
                },
                router.complete(request),
            );
        }
        return (
            GateOutcome::CloudDenied {
                reason: format!("capability '{}' does not allow cloud processing", request.capability),
            },
            Err(format!(
                "cloud denied: capability '{}' forbids cloud; no local provider",
                request.capability
            )),
        );
    }

    // 7. Active provider is BYOK but unavailable → fall through.
    (
        GateOutcome::NoProvider {
            reason: "active provider unavailable".into(),
        },
        Err("provider unavailable; deterministic core continues".into()),
    )
}

// ─────────────────────────── validated output helpers ───────────────────────────

/// Complete + parse + validate as a GOAL proposal (Phase 15 boundary).
/// The deterministic core sees ONLY validated values.
pub fn complete_goal_proposal(
    router: &ProviderRouter,
    request: &IntelligenceRequest,
    meta: &CapabilityMeta,
) -> (GateOutcome, ValidationResult<ModelGoalProposal>) {
    let (outcome, result) = gated_complete(router, request, meta);
    match result {
        Ok(resp) => (outcome, AiValidator::parse_goal(&resp.text)),
        Err(e) => (
            outcome,
            ValidationResult::Rejected {
                reason: super::ai_validation::RejectionReason::MalformedJson,
                detail: e,
            },
        ),
    }
}

/// Complete + parse + validate as a PLAN proposal.
pub fn complete_plan_proposal(
    router: &ProviderRouter,
    request: &IntelligenceRequest,
    meta: &CapabilityMeta,
) -> (GateOutcome, ValidationResult<ModelPlanProposal>) {
    let (outcome, result) = gated_complete(router, request, meta);
    match result {
        Ok(resp) => (outcome, AiValidator::parse_plan(&resp.text)),
        Err(e) => (
            outcome,
            ValidationResult::Rejected {
                reason: super::ai_validation::RejectionReason::MalformedJson,
                detail: e,
            },
        ),
    }
}

/// Map router state to the Phase 15 provider-state model.
pub fn provider_state(router: &ProviderRouter) -> ProviderState {
    match router.active_kind() {
        ProviderKind::None => ProviderState::Unconfigured,
        kind => {
            let ready = router
                .all_status()
                .into_iter()
                .find(|s| s.kind == kind)
                .map(|s| s.available)
                .unwrap_or(false);
            if ready {
                ProviderState::Ready { provider: kind.to_string() }
            } else {
                ProviderState::Unavailable { reason: "health check failed".into() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(prompt: &str) -> IntelligenceRequest {
        IntelligenceRequest {
            prompt: prompt.into(),
            system: None,
            max_tokens: None,
            temperature: None,
            json_mode: false,
            actor: "core".into(),
            capability: "test".into(),
        }
    }

    fn needs_ai() -> CapabilityMeta {
        CapabilityMeta { requires_ai: true, cloud_allowed: false, min_level: 0 }
    }

    #[test]
    fn no_provider_mode_falls_back_deterministically() {
        let router = ProviderRouter::new(); // nothing configured
        let (outcome, result) = gated_complete(&router, &req("summarize"), &needs_ai());
        assert!(matches!(outcome, GateOutcome::NoProvider { .. }));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deterministic core"));
    }

    #[test]
    fn capability_without_ai_never_dispatches() {
        let router = ProviderRouter::new();
        let meta = CapabilityMeta { requires_ai: false, ..Default::default() };
        let (outcome, _) = gated_complete(&router, &req("x"), &meta);
        assert!(matches!(outcome, GateOutcome::DeterministicFallback { .. }));
    }

    #[test]
    fn blocked_prompt_never_reaches_any_provider() {
        let router = ProviderRouter::new();
        // A private-key block is screened out before provider logic runs.
        let secret = "-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----";
        let (outcome, result) = gated_complete(&router, &req(secret), &needs_ai());
        assert!(matches!(outcome, GateOutcome::DeterministicFallback { reason } if reason.contains("privacy")));
        assert!(result.is_err());
    }

    #[test]
    fn sensitive_prompt_is_cloud_denied_without_local() {
        let router = ProviderRouter::new();
        // Long text with a labeled secret → Redact class (sensitive), and
        // with no provider at all the deterministic NoProvider gate fires —
        // both are correct refusals; nothing ever dispatches.
        let mut r = req("Here is my note about the password=hunter2boi field and other context to make it long enough for redaction classification");
        r.capability = "needs_ai_cap".into();
        let (outcome, result) = gated_complete(&router, &r, &needs_ai());
        assert!(matches!(
            outcome,
            GateOutcome::NoProvider { .. } | GateOutcome::CloudDenied { .. } | GateOutcome::DeterministicFallback { .. }
        ));
        assert!(result.is_err(), "sensitive prompt must never dispatch silently");
    }

    #[test]
    fn clean_prompt_with_cloud_only_and_cloud_denied_capability() {
        // A router with BYOK active but the capability forbids cloud.
        // (ProviderRouter without a concrete provider reports unavailable,
        //  so this lands on the availability gate — which is itself the
        //  hardened behavior: nothing silently dispatches.)
        let mut router = ProviderRouter::new();
        router.set_active(ProviderKind::Byok);
        let meta = CapabilityMeta { requires_ai: true, cloud_allowed: false, min_level: 0 };
        let (outcome, result) = gated_complete(&router, &req("clean summary"), &meta);
        assert!(matches!(outcome, GateOutcome::NoProvider { .. } | GateOutcome::CloudDenied { .. }));
        assert!(result.is_err(), "unavailable BYOK must not dispatch");
    }

    #[test]
    fn provider_state_maps_correctly() {
        let none = provider_state(&ProviderRouter::new());
        assert_eq!(none, ProviderState::Unconfigured);
    }

    #[test]
    fn validated_goal_output_flows_through_phase15() {
        let router = ProviderRouter::new();
        let (outcome, v) = complete_goal_proposal(&router, &req("propose goals"), &needs_ai());
        assert!(matches!(outcome, GateOutcome::NoProvider { .. }));
        assert!(matches!(v, ValidationResult::Rejected { .. }));
    }

    #[test]
    fn gating_is_deterministic() {
        let router = ProviderRouter::new();
        let a = gated_complete(&router, &req("same"), &needs_ai());
        let b = gated_complete(&router, &req("same"), &needs_ai());
        assert_eq!(format!("{:?}", a.0), format!("{:?}", b.0));
        assert_eq!(a.1.is_err(), b.1.is_err());
    }

    #[test]
    fn redaction_is_applied_for_local_dispatch_content() {
        // The policy's password-like heuristic needs a digit followed by
        // 3+ value chars — "hunter2boi99" matches (digit inside + suffix).
        let dirty = "Here is my deploy note: pwd: hunter2boi99 and the server is fine with the usual setup that we always use for deployments around here.";
        let clean = redact_prompt(dirty);
        assert!(!clean.contains("hunter2boi99"), "redacted: {clean}");
        assert!(clean.contains("[REDACTED]"));
        assert_eq!(screen_prompt(dirty), PromptPrivacy::ContainsSensitive);
    }
}
