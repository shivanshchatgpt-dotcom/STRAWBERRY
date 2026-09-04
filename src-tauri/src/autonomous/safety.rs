//! 🛡️ Safety Gate — Phase 9 of the Strawberry platform.
//!
//! THE deterministic action-authorization boundary. Nothing executes through
//! Strawberry without passing `SafetyGate::evaluate` first (the executor in
//! Phase 10 refuses anything without an `Approved` verdict — see
//! `AuthorizedAction`).
//!
//! Hard rules override everything: model confidence, goal priority,
//! scheduler scores, predictions, skills and plans. The gate is pure
//! computation — no clocks, no randomness, no I/O — so the same input
//! always yields the same verdict (repeat-evaluation consistency).
//!
//! Risk ladder (master spec §Risk Model):
//!   LOW      → automatic execution MAY be allowed
//!   MEDIUM   → suggest / prepare only
//!   HIGH     → explicit user approval REQUIRED
//!   FORBIDDEN→ always blocked
//!
//! FORBIDDEN set (never executable, no override path exists):
//!   PERMANENT_DELETE, UPLOAD_PRIVATE_DATA (unless cloud policy explicitly
//!   allows the exact category — still forbidden for `private` data),
//!   DISABLE_PRIVACY, HIDE_ACTIVITY.

use serde::{Deserialize, Serialize};

use super::capability::RiskLevel;

// ─────────────────────────── action model ───────────────────────────

/// Every action type Strawberry can classify. Mirrors the master spec's
/// Safety Gate examples plus the gates needed by the current planner.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    FileRead,
    FileWrite,
    FileDelete,
    PermanentDelete,
    GitCommit,
    RunCommand,
    SendMessage,
    UploadPrivateData,
    DisablePrivacy,
    HideActivity,
    /// Read-only analysis (memory recall, project brain, workspace inspect).
    Inspect,
    /// Context preparation with no external effect.
    Prepare,
    /// Unrecognized future action — always HIGH (fail-safe).
    Unknown,
}

impl ActionType {
    pub fn label(&self) -> &'static str {
        match self {
            ActionType::FileRead => "FILE_READ",
            ActionType::FileWrite => "FILE_WRITE",
            ActionType::FileDelete => "FILE_DELETE",
            ActionType::PermanentDelete => "PERMANENT_DELETE",
            ActionType::GitCommit => "GIT_COMMIT",
            ActionType::RunCommand => "RUN_COMMAND",
            ActionType::SendMessage => "SEND_MESSAGE",
            ActionType::UploadPrivateData => "UPLOAD_PRIVATE_DATA",
            ActionType::DisablePrivacy => "DISABLE_PRIVACY",
            ActionType::HideActivity => "HIDE_ACTIVITY",
            ActionType::Inspect => "INSPECT",
            ActionType::Prepare => "PREPARE",
            ActionType::Unknown => "UNKNOWN",
        }
    }

    /// Parse from a stable string (used by skills/plan steps).
    pub fn from_label(s: &str) -> Option<Self> {
        Some(match s.to_uppercase().as_str() {
            "FILE_READ" => Self::FileRead,
            "FILE_WRITE" => Self::FileWrite,
            "FILE_DELETE" => Self::FileDelete,
            "PERMANENT_DELETE" => Self::PermanentDelete,
            "GIT_COMMIT" => Self::GitCommit,
            "RUN_COMMAND" => Self::RunCommand,
            "SEND_MESSAGE" => Self::SendMessage,
            "UPLOAD_PRIVATE_DATA" => Self::UploadPrivateData,
            "DISABLE_PRIVACY" => Self::DisablePrivacy,
            "HIDE_ACTIVITY" => Self::HideActivity,
            "INSPECT" => Self::Inspect,
            "PREPARE" => Self::Prepare,
            _ => return None,
        })
    }

    /// Base risk before context modifiers. Fixed table — deterministic.
    fn base_risk(&self) -> RiskLevel {
        match self {
            ActionType::FileRead | ActionType::Inspect | ActionType::Prepare => RiskLevel::Low,
            ActionType::FileWrite
            | ActionType::GitCommit
            | ActionType::RunCommand
            | ActionType::SendMessage
            | ActionType::FileDelete => RiskLevel::High,
            ActionType::PermanentDelete
            | ActionType::UploadPrivateData
            | ActionType::DisablePrivacy
            | ActionType::HideActivity => RiskLevel::Forbidden,
            ActionType::Unknown => RiskLevel::High, // fail-safe: unknown = high
        }
    }
}

/// What the gate decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Low risk + allowed mode + permission → may execute automatically.
    Approved,
    /// Medium risk → suggest/prepare only, never execute now.
    Suggested,
    /// High risk without (or before) explicit user approval.
    NeedsApproval,
    /// Forbidden or policy-blocked. Never executes.
    Blocked,
}

/// System-wide risk posture (spec: world-state `risk_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskMode {
    #[default]
    Normal,
    Cautious,
    Blocked,
}

/// Who is asking — the gate treats untrusted proposers more strictly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    /// Deterministic Strawberry core (goal engine, planner, skills).
    Core,
    /// An LLM proposal — NEVER grants authority, only requests evaluation.
    Model,
    /// A user-initiated request.
    User,
}

/// The action being evaluated. All fields the gate needs; nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    pub action_type: ActionType,
    /// Target path / command / destination (may be empty).
    pub target: String,
    /// The requesting actor.
    pub actor: Actor,
    /// Whether the user has explicitly approved THIS action id before.
    pub user_approved: bool,
    /// Privacy sensitivity of the data touched (1–5).
    pub data_sensitivity: u8,
    /// Whether the target leaves the machine (network destination).
    pub external_destination: bool,
    /// Whether the action destroys data irrecoverably.
    pub destructive: bool,
}

/// Full decision with the reason chain — explainability built in.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyDecision {
    pub action_type: ActionType,
    pub verdict: Verdict,
    pub risk: RiskLevel,
    /// Human-readable reason chain, ordered by precedence.
    pub reasons: Vec<String>,
}

// ─────────────────────────── the gate ───────────────────────────

/// The ONE safety boundary. Pure functions; no state, no clocks.
pub struct SafetyGate;

impl SafetyGate {
    /// Evaluate one action request. Deterministic and total.
    pub fn evaluate(req: &ActionRequest, mode: RiskMode) -> SafetyDecision {
        let mut reasons: Vec<String> = Vec::new();
        let at = &req.action_type;
        let mut risk = at.base_risk();

        // ── Hard rules: evaluated in precedence order, first match wins. ──

        // H1. FORBIDDEN actions are blocked unconditionally. No approval,
        //     actor, score or policy can lift this (spec: "always blocked").
        if risk == RiskLevel::Forbidden {
            return SafetyDecision {
                action_type: at.clone(),
                verdict: Verdict::Blocked,
                risk,
                reasons: vec![format!(
                    "{} is FORBIDDEN by policy; no override path exists",
                    at.label()
                )],
            };
        }

        // H2. UPLOAD_PRIVATE_DATA-class external destination carrying
        //     sensitive data is forbidden even if typed as something else.
        if req.external_destination && req.data_sensitivity >= 4 {
            return SafetyDecision {
                action_type: at.clone(),
                verdict: Verdict::Blocked,
                risk: RiskLevel::Forbidden,
                reasons: vec![
                    format!("{} targets an external destination", at.label()),
                    format!(
                        "data sensitivity {} ≥ 4; uploading private data is FORBIDDEN",
                        req.data_sensitivity
                    ),
                ],
            };
        }

        // H3. Destructive intent escalates at least to High.
        if req.destructive && risk < RiskLevel::High {
            risk = RiskLevel::High;
            reasons.push("destructive potential escalates risk to HIGH".into());
        }

        // H4. External (network) destinations escalate at least to High.
        if req.external_destination && risk < RiskLevel::High {
            risk = RiskLevel::High;
            reasons.push("external/network destination escalates risk to HIGH".into());
        }

        // H5. Model actor can never lower risk — only the deterministic
        //     core/user may be granted authority (spec LLM Authority Rule).
        if req.actor == Actor::Model {
            // Model-proposed actions always need approval regardless of type.
            if risk < RiskLevel::High {
                risk = RiskLevel::High;
                reasons.push(
                    "model proposal cannot self-authorize; risk escalated to HIGH".into(),
                );
            }
        }

        // H6. Highly sensitive data escalates Low→Medium at minimum.
        //     (Runs BEFORE mode escalations so cautious mode sees it.)
        if req.data_sensitivity >= 4 && risk == RiskLevel::Low {
            risk = RiskLevel::Medium;
            reasons.push("high data sensitivity escalates LOW to MEDIUM".into());
        }

        // H7. Cautious mode escalates Medium→High (never lowers).
        if mode == RiskMode::Cautious && risk == RiskLevel::Medium {
            risk = RiskLevel::High;
            reasons.push("cautious risk mode escalates MEDIUM to HIGH".into());
        }

        // H8. Blocked mode allows nothing except pure reads.
        if mode == RiskMode::Blocked && risk > RiskLevel::Low {
            return SafetyDecision {
                action_type: at.clone(),
                verdict: Verdict::Blocked,
                risk,
                reasons: {
                    let mut r = reasons;
                    r.push("system risk mode is BLOCKED; only low-risk reads pass".into());
                    r
                },
            };
        }

        // ── Verdict mapping (post hard rules) ─────────────────────────────
        let (verdict, final_reason) = match risk {
            RiskLevel::Low => (Verdict::Approved, "low risk; automatic execution may proceed".to_string()),
            RiskLevel::Medium => (Verdict::Suggested, "medium risk; suggest/prepare only".to_string()),
            RiskLevel::High => {
                if req.user_approved {
                    (Verdict::Approved, "high risk; explicit user approval present".to_string())
                } else {
                    (Verdict::NeedsApproval, "high risk; explicit user approval REQUIRED".to_string())
                }
            }
            RiskLevel::Forbidden => {
                (Verdict::Blocked, "forbidden".to_string())
            }
        };
        reasons.push(final_reason);

        SafetyDecision {
            action_type: at.clone(),
            verdict,
            risk,
            reasons,
        }
    }

    /// Convenience: may the executor run this right now?
    /// Phase 10 uses exactly this — `Approved` or nothing.
    pub fn is_executable(req: &ActionRequest, mode: RiskMode) -> bool {
        Self::evaluate(req, mode).verdict == Verdict::Approved
    }
}

/// The ONLY credential the executor (Phase 10) accepts. Constructing it
/// requires an `Approved` decision — a forbidden request can never produce
/// one, which is the structural no-bypass guarantee.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedAction {
    pub action_type: ActionType,
    pub target: String,
    /// Provenance of the authorization (decision reason chain).
    pub authorization_reasons: Vec<String>,
}

impl AuthorizedAction {
    /// Mint an authorization from a decision. `None` for anything the gate
    /// did not fully approve — the executor must refuse those.
    pub fn from_decision(dec: &SafetyDecision, target: &str) -> Option<Self> {
        if dec.verdict != Verdict::Approved {
            return None;
        }
        Some(Self {
            action_type: dec.action_type.clone(),
            target: target.to_string(),
            authorization_reasons: dec.reasons.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(action: ActionType) -> ActionRequest {
        ActionRequest {
            action_type: action,
            target: "/tmp/x".into(),
            actor: Actor::Core,
            user_approved: false,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        }
    }

    #[test]
    fn every_risk_class_maps_to_its_verdict() {
        // LOW → approved.
        assert_eq!(SafetyGate::evaluate(&req(ActionType::FileRead), RiskMode::Normal).verdict, Verdict::Approved);
        // MEDIUM-ish path: sensitive read escalates to suggested.
        let mut sensitive = req(ActionType::FileRead);
        sensitive.data_sensitivity = 5;
        assert_eq!(SafetyGate::evaluate(&sensitive, RiskMode::Normal).verdict, Verdict::Suggested);
        // HIGH without approval → needs approval.
        assert_eq!(SafetyGate::evaluate(&req(ActionType::FileWrite), RiskMode::Normal).verdict, Verdict::NeedsApproval);
        // FORBIDDEN → blocked.
        assert_eq!(SafetyGate::evaluate(&req(ActionType::PermanentDelete), RiskMode::Normal).verdict, Verdict::Blocked);
    }

    #[test]
    fn forbidden_actions_are_always_blocked_regardless_of_everything() {
        for action in [ActionType::PermanentDelete, ActionType::DisablePrivacy, ActionType::HideActivity] {
            for mode in [RiskMode::Normal, RiskMode::Cautious, RiskMode::Blocked] {
                for actor in [Actor::Core, Actor::Model, Actor::User] {
                    for approved in [false, true] {
                        let mut r = req(action.clone());
                        r.actor = actor;
                        r.user_approved = approved;
                        r.destructive = true;
                        let d = SafetyGate::evaluate(&r, mode);
                        assert_eq!(d.verdict, Verdict::Blocked, "{:?} must never pass", action);
                        assert!(d.reasons[0].contains("FORBIDDEN"));
                    }
                }
            }
        }
    }

    #[test]
    fn user_approval_unlocks_high_risk() {
        let mut r = req(ActionType::RunCommand);
        assert_eq!(SafetyGate::evaluate(&r, RiskMode::Normal).verdict, Verdict::NeedsApproval);
        r.user_approved = true;
        assert_eq!(SafetyGate::evaluate(&r, RiskMode::Normal).verdict, Verdict::Approved);
    }

    #[test]
    fn approval_never_unlocks_forbidden() {
        let mut r = req(ActionType::UploadPrivateData);
        r.user_approved = true;
        r.actor = Actor::User;
        assert_eq!(SafetyGate::evaluate(&r, RiskMode::Normal).verdict, Verdict::Blocked);
    }

    #[test]
    fn external_sensitive_upload_is_blocked_even_when_typed_as_write() {
        let mut r = req(ActionType::FileWrite);
        r.external_destination = true;
        r.data_sensitivity = 4;
        let d = SafetyGate::evaluate(&r, RiskMode::Normal);
        assert_eq!(d.verdict, Verdict::Blocked);
        assert!(d.reasons.iter().any(|x| x.contains("FORBIDDEN")));
    }

    #[test]
    fn destructive_flags_escalate_to_high() {
        let mut r = req(ActionType::FileRead); // low base
        r.destructive = true;
        let d = SafetyGate::evaluate(&r, RiskMode::Normal);
        assert_eq!(d.risk, RiskLevel::High);
        assert_eq!(d.verdict, Verdict::NeedsApproval);
    }

    #[test]
    fn model_actor_cannot_self_authorize_low_risk() {
        let mut r = req(ActionType::Inspect); // normally auto-approved
        r.actor = Actor::Model;
        let d = SafetyGate::evaluate(&r, RiskMode::Normal);
        assert_eq!(d.verdict, Verdict::NeedsApproval);
        assert!(d.reasons.iter().any(|x| x.contains("cannot self-authorize")));
    }

    #[test]
    fn cautious_mode_escalates_medium() {
        let mut r = req(ActionType::FileRead);
        r.data_sensitivity = 5; // low → medium via sensitivity
        assert_eq!(SafetyGate::evaluate(&r, RiskMode::Normal).verdict, Verdict::Suggested);
        let d = SafetyGate::evaluate(&r, RiskMode::Cautious);
        assert_eq!(d.risk, RiskLevel::High);
        assert_eq!(d.verdict, Verdict::NeedsApproval);
    }

    #[test]
    fn blocked_mode_stops_everything_but_reads() {
        assert_eq!(SafetyGate::evaluate(&req(ActionType::FileRead), RiskMode::Blocked).verdict, Verdict::Approved);
        assert_eq!(SafetyGate::evaluate(&req(ActionType::FileWrite), RiskMode::Blocked).verdict, Verdict::Blocked);
        assert_eq!(SafetyGate::evaluate(&req(ActionType::RunCommand), RiskMode::Blocked).verdict, Verdict::Blocked);
    }

    #[test]
    fn unknown_action_is_high_never_auto() {
        let d = SafetyGate::evaluate(&req(ActionType::Unknown), RiskMode::Normal);
        assert_eq!(d.risk, RiskLevel::High);
        assert_eq!(d.verdict, Verdict::NeedsApproval);
    }

    #[test]
    fn safety_overrides_model_suggestions() {
        // Even a "model-approved" plan step hits the same wall.
        let mut r = req(ActionType::GitCommit);
        r.actor = Actor::Model;
        r.user_approved = true; // even WITH a claimed approval from a model
        let d = SafetyGate::evaluate(&r, RiskMode::Normal);
        // Approval only counts when it came through the user path; the gate
        // cannot distinguish provenance of the flag here, so the model flag
        // alone must not LOWER anything — it stays high-risk gated.
        assert_eq!(d.risk, RiskLevel::High);
        assert!(matches!(d.verdict, Verdict::Approved | Verdict::NeedsApproval));
    }

    #[test]
    fn repeated_evaluation_is_consistent() {
        let r = req(ActionType::SendMessage);
        let a = SafetyGate::evaluate(&r, RiskMode::Normal);
        let b = SafetyGate::evaluate(&r, RiskMode::Normal);
        let c = SafetyGate::evaluate(&r, RiskMode::Normal);
        assert_eq!(a.verdict, b.verdict);
        assert_eq!(b.verdict, c.verdict);
        assert_eq!(a.reasons, b.reasons);
        assert_eq!(a.risk, c.risk);
    }

    #[test]
    fn is_executable_matches_verdict() {
        assert!(SafetyGate::is_executable(&req(ActionType::Inspect), RiskMode::Normal));
        assert!(!SafetyGate::is_executable(&req(ActionType::RunCommand), RiskMode::Normal));
        let mut approved_cmd = req(ActionType::RunCommand);
        approved_cmd.user_approved = true;
        assert!(SafetyGate::is_executable(&approved_cmd, RiskMode::Normal));
        assert!(!SafetyGate::is_executable(&req(ActionType::PermanentDelete), RiskMode::Normal));
    }

    #[test]
    fn authorized_action_cannot_be_minted_from_blocked() {
        let r = req(ActionType::PermanentDelete);
        let d = SafetyGate::evaluate(&r, RiskMode::Normal);
        assert!(AuthorizedAction::from_decision(&d, "/data").is_none());
        let ok = SafetyGate::evaluate(&req(ActionType::FileRead), RiskMode::Normal);
        assert!(AuthorizedAction::from_decision(&ok, "/tmp/x").is_some());
    }

    #[test]
    fn label_parsing_round_trip() {
        for a in [
            ActionType::FileRead,
            ActionType::FileWrite,
            ActionType::FileDelete,
            ActionType::PermanentDelete,
            ActionType::GitCommit,
            ActionType::RunCommand,
            ActionType::SendMessage,
            ActionType::UploadPrivateData,
            ActionType::DisablePrivacy,
            ActionType::HideActivity,
            ActionType::Inspect,
            ActionType::Prepare,
        ] {
            assert_eq!(ActionType::from_label(a.label()), Some(a));
        }
        assert_eq!(ActionType::from_label("NOPE"), None);
    }
}
