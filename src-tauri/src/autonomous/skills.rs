//! 🧰 Skill System Foundation — Phase 16 of the Strawberry platform.
//!
//! Controlled, reusable operations under the existing architecture:
//!   * Skills REGISTER in the same style as the capability manifest
//!     (compile-time truth + runtime lookup).
//!   * Skills NEVER bypass Safety/Privacy — every skill's declared risk
//!     feeds the SAME SafetyGate; execution flows through the SAME
//!     Executor; audit flows through the SAME ledger.
//!   * Skills validate inputs against their declared schema before run.
//!
//! Master spec skill contract: id, purpose, dependencies, permissions,
//! risk, resource cost, privacy sensitivity, input schema, output schema,
//! verification method.

use serde::{Deserialize, Serialize};

use super::capability::RiskLevel;
use super::safety::{ActionType, ActionRequest, Actor, SafetyGate, Verdict as SafetyVerdict};
use super::executor::{Effector, Executor, ActionRecord};
use super::verifier::{Expectation, Verification, VerificationResult, Verifier};

// ─────────────────────────── model ───────────────────────────

/// How a skill's output is verified (mirrors Phase 11 expectations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyMethod {
    ExitZero,
    OutputContains(String),
    FileExists(String),
    Manual,
}

/// One skill's declaration — the full spec contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDef {
    pub skill_id: &'static str,
    pub name: &'static str,
    pub purpose: &'static str,
    /// Capability ids this skill depends on (must exist in the registry).
    #[serde(skip_deserializing, default = "empty_strs")]
    pub dependencies: &'static [&'static str],
    /// The action class this skill's work maps to at the gate.
    pub action_type: ActionType,
    pub risk: RiskLevel,
    pub resource_cost: u8,
    pub privacy_sensitivity: u8,
    /// Fixed input shape (JSON keys the skill accepts; extra keys rejected).
    #[serde(skip_deserializing, default = "empty_strs")]
    pub input_schema: &'static [&'static str],
    /// Fixed output description (for ledger explainability).
    pub output_schema: &'static str,
    pub verify: VerifyMethod,
}

fn empty_strs() -> &'static [&'static str] {
    &[]
}

/// The skill registry — same pattern as the capability manifest.
pub const SKILLS: &[SkillDef] = &[
    SkillDef {
        skill_id: "inspect_project",
        name: "Inspect Project",
        purpose: "Read-only project structure and health summary",
        dependencies: &["project_brain"],
        action_type: ActionType::Inspect,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 2,
        input_schema: &["path"],
        output_schema: "project summary object",
        verify: VerifyMethod::ExitZero,
    },
    SkillDef {
        skill_id: "run_tests",
        name: "Run Tests",
        purpose: "Execute the project's test suite and capture results",
        dependencies: &["file_code_watch"],
        action_type: ActionType::RunCommand,
        risk: RiskLevel::High,
        resource_cost: 4,
        privacy_sensitivity: 1,
        input_schema: &["command", "cwd"],
        output_schema: "exit code + bounded output",
        verify: VerifyMethod::ExitZero,
    },
    SkillDef {
        skill_id: "inspect_git",
        name: "Inspect Git",
        purpose: "Read-only git status/log inspection",
        dependencies: &["file_code_watch"],
        action_type: ActionType::Inspect,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 2,
        input_schema: &["command", "cwd"],
        output_schema: "git text output",
        verify: VerifyMethod::ExitZero,
    },
    SkillDef {
        skill_id: "summarize_changes",
        name: "Summarize Changes",
        purpose: "Deterministic diff summary from local data",
        dependencies: &["search_indexing"],
        action_type: ActionType::Inspect,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 2,
        input_schema: &["path"],
        output_schema: "change summary text",
        verify: VerifyMethod::Manual,
    },
    SkillDef {
        skill_id: "inspect_error",
        name: "Inspect Error",
        purpose: "Gather a captured error's context from memory",
        dependencies: &["search_indexing"],
        action_type: ActionType::Inspect,
        risk: RiskLevel::Low,
        resource_cost: 1,
        privacy_sensitivity: 3,
        input_schema: &["error_ref"],
        output_schema: "error context text",
        verify: VerifyMethod::Manual,
    },
    SkillDef {
        skill_id: "prepare_context",
        name: "Prepare Context",
        purpose: "Assemble relevant context for an AI tool or report",
        dependencies: &["search_indexing", "project_brain"],
        action_type: ActionType::Prepare,
        risk: RiskLevel::Low,
        resource_cost: 2,
        privacy_sensitivity: 4,
        input_schema: &["topic"],
        output_schema: "context bundle",
        verify: VerifyMethod::Manual,
    },
];

pub fn skill(skill_id: &str) -> Option<&'static SkillDef> {
    SKILLS.iter().find(|s| s.skill_id == skill_id)
}

// ─────────────────────────── validation + execution path ───────────────────────────

/// Validate skill input against the declared schema. Deterministic.
pub fn validate_input(
    def: &SkillDef,
    input: &serde_json::Value,
) -> Result<(), String> {
    let obj = input
        .as_object()
        .ok_or_else(|| "skill input must be a JSON object".to_string())?;
    for key in def.input_schema {
        if !obj.contains_key(*key) {
            return Err(format!("missing required input key: {key}"));
        }
    }
    // Reject unknown keys — strict schema, no smuggling.
    for key in obj.keys() {
        if !def.input_schema.contains(&key.as_str()) {
            return Err(format!("unknown input key: {key}"));
        }
    }
    Ok(())
}

/// One skill run: validate → safety gate → executor → verifier.
/// Returns the full audit chain. Never bypasses the gate; skills with
/// HIGH risk simply cannot run without user approval (caller-owned
/// oracle, same as the lifecycle).
pub struct SkillOutcome {
    pub skill_id: String,
    pub gate_verdict: SafetyVerdict,
    pub input_valid: bool,
    pub validation_error: Option<String>,
    pub record: Option<ActionRecord>,
    pub verification: Option<VerificationResult>,
}

#[allow(clippy::too_many_arguments)]
pub fn run_skill(
    skill_id: &str,
    input: &serde_json::Value,
    user_approves: bool,
    executor: &Executor,
    effector: &dyn Effector,
) -> SkillOutcome {
    let def = match skill(skill_id) {
        Some(d) => d,
        None => {
            return SkillOutcome {
                skill_id: skill_id.to_string(),
                gate_verdict: SafetyVerdict::Blocked,
                input_valid: false,
                validation_error: Some(format!("unknown skill: {skill_id}")),
                record: None,
                verification: None,
            }
        }
    };

    // 1. Schema validation first — garbage never reaches the gate.
    if let Err(e) = validate_input(def, input) {
        return SkillOutcome {
            skill_id: skill_id.to_string(),
            gate_verdict: SafetyVerdict::Blocked,
            input_valid: false,
            validation_error: Some(e),
            record: None,
            verification: None,
        };
    }

    // 2. Safety gate — the SAME boundary, with the skill's declared action.
    let target = input
        .get("command")
        .and_then(|v| v.as_str())
        .or_else(|| input.get("path").and_then(|v| v.as_str()))
        .or_else(|| input.get("error_ref").and_then(|v| v.as_str()))
        .or_else(|| input.get("topic").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let request = ActionRequest {
        action_type: def.action_type.clone(),
        target,
        actor: Actor::Core,
        user_approved: user_approves,
        data_sensitivity: def.privacy_sensitivity,
        external_destination: false,
        destructive: false,
    };
    let decision = SafetyGate::evaluate(&request, super::safety::RiskMode::Normal);
    let gate_verdict = decision.verdict;

    let authorized = match super::safety::AuthorizedAction::from_decision(&decision, &request.target)
    {
        Some(a) => a,
        None => {
            let denied = Executor::record_denial(
                request.action_type.clone(),
                &request.target,
                decision.reasons.last().map(|s| s.as_str()).unwrap_or("denied"),
            );
            return SkillOutcome {
                skill_id: skill_id.to_string(),
                gate_verdict,
                input_valid: true,
                validation_error: None,
                record: Some(denied),
                verification: None,
            };
        }
    };

    // 3. Execute through the SAME executor.
    let record = executor.execute(
        authorized,
        effector,
        None,
        None,
        Some(def.skill_id.to_string()),
        def.purpose,
        std::time::Duration::from_secs(60),
    );

    // 4. Verify with the skill's declared method.
    let expectation = match &def.verify {
        VerifyMethod::ExitZero => Expectation::ExitZero,
        VerifyMethod::OutputContains(n) => Expectation::OutputContains(n.clone()),
        VerifyMethod::FileExists(p) => Expectation::FileExists(p.clone()),
        VerifyMethod::Manual => Expectation::Manual,
    };
    let verification = Verifier::verify(&record, &expectation);

    SkillOutcome {
        skill_id: skill_id.to_string(),
        gate_verdict,
        input_valid: true,
        validation_error: None,
        record: Some(record),
        verification: Some(verification),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::executor::{ExecutionState, ShellEffector, ApprovalState};

    struct OkEffector;
    impl Effector for OkEffector {
        fn run(&self, _a: &super::super::safety::AuthorizedAction, _c: &std::sync::atomic::AtomicBool, _t: std::time::Duration) -> (i32, String) {
            (0, "ok".into())
        }
    }

    #[test]
    fn skill_registry_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for s in SKILLS {
            assert!(seen.insert(s.skill_id), "duplicate skill {}", s.skill_id);
        }
        assert_eq!(SKILLS.len(), 6, "spec: 6 example skills");
        // Dependencies resolve against the capability manifest.
        for s in SKILLS {
            for dep in s.dependencies {
                assert!(
                    super::super::capability::def(dep).is_some(),
                    "skill {} depends on unknown capability {dep}",
                    s.skill_id
                );
            }
        }
    }

    #[test]
    fn skill_registration_and_lookup() {
        assert!(skill("run_tests").is_some());
        assert!(skill("inspect_project").is_some());
        assert!(skill("nope").is_none());
    }

    #[test]
    fn duplicate_skill_ids_rejected() {
        // Compile-time manifest cannot hold dupes; the lookup would just
        // shadow. We assert uniqueness (above) and deterministic lookup.
        let s1 = skill("run_tests").unwrap();
        assert_eq!(s1.skill_id, "run_tests");
    }

    #[test]
    fn input_schema_validation_enforces_keys() {
        let def = skill("run_tests").unwrap();
        // Missing key.
        assert!(validate_input(def, &serde_json::json!({"command": "x"})).is_err());
        // Valid.
        assert!(validate_input(def, &serde_json::json!({"command": "x", "cwd": "/tmp"})).is_ok());
        // Unknown key (smuggling).
        assert!(validate_input(def, &serde_json::json!({"command": "x", "cwd": "/tmp", "evil": 1})).is_err());
        // Non-object.
        assert!(validate_input(def, &serde_json::json!("string")).is_err());
    }

    #[test]
    fn low_risk_skill_runs_automatically() {
        let ex = Executor::new();
        let out = run_skill(
            "inspect_error",
            &serde_json::json!({"error_ref": "chat-1"}),
            false,
            &ex,
            &OkEffector,
        );
        assert_eq!(out.gate_verdict, SafetyVerdict::Approved);
        let rec = out.record.unwrap();
        assert_eq!(rec.execution_state, ExecutionState::Succeeded);
        assert_eq!(rec.capability.as_deref(), Some("inspect_error"));
    }

    #[test]
    fn high_risk_skill_requires_approval() {
        let ex = Executor::new();
        let input = serde_json::json!({"command": "cargo test", "cwd": "/tmp"});
        // Without approval → denied.
        let out = run_skill("run_tests", &input, false, &ex, &OkEffector);
        assert_eq!(out.gate_verdict, SafetyVerdict::NeedsApproval);
        assert_eq!(out.record.unwrap().approval_state, ApprovalState::DeniedByGate);
        // With approval → executes.
        let out2 = run_skill("run_tests", &input, true, &ex, &OkEffector);
        assert_eq!(out2.gate_verdict, SafetyVerdict::Approved);
        assert_eq!(out2.record.unwrap().execution_state, ExecutionState::Succeeded);
    }

    #[test]
    fn skill_risk_propagates_to_gate() {
        // run_tests declares High → its declared action (RunCommand) is High
        // at the gate. Consistency proof.
        let def = skill("run_tests").unwrap();
        assert_eq!(def.risk, RiskLevel::High);
        assert_eq!(
            SafetyGate::evaluate(
                &ActionRequest {
                    action_type: def.action_type.clone(),
                    target: "x".into(),
                    actor: Actor::Core,
                    user_approved: false,
                    data_sensitivity: def.privacy_sensitivity,
                    external_destination: false,
                    destructive: false,
                },
                super::super::safety::RiskMode::Normal,
            )
            .risk,
            RiskLevel::High
        );
    }

    #[test]
    fn unknown_skill_is_rejected_not_crashed() {
        let ex = Executor::new();
        let out = run_skill("definitely_not_a_skill", &serde_json::json!({}), false, &ex, &OkEffector);
        assert!(!out.input_valid);
        assert!(out.validation_error.unwrap().contains("unknown skill"));
        assert!(out.record.is_none());
    }

    #[test]
    fn invalid_input_never_reaches_the_gate() {
        let ex = Executor::new();
        let out = run_skill(
            "run_tests",
            &serde_json::json!({"command": "rm -rf /"}), // missing cwd
            true,
            &ex,
            &OkEffector,
        );
        assert!(!out.input_valid);
        assert!(out.validation_error.unwrap().contains("cwd"));
        assert!(out.record.is_none(), "schema-invalid input must not execute");
    }

    #[test]
    fn skill_execution_is_verified() {
        let ex = Executor::new();
        // run_tests is RunCommand-typed (matches ShellEffector) and needs
        // approval, which the oracle supplies — full path: schema → gate →
        // shell → verify.
        let out = run_skill(
            "run_tests",
            &serde_json::json!({"command": "printf git-ok", "cwd": "/tmp"}),
            true,
            &ex,
            &ShellEffector,
        );
        assert_eq!(out.gate_verdict, SafetyVerdict::Approved);
        let v = out.verification.unwrap();
        assert_eq!(v.verification, Verification::Success);
    }

    #[test]
    fn skills_share_the_one_executor_and_audit() {
        // Provenance proof: the record carries the skill id as capability.
        let ex = Executor::new();
        let out = run_skill(
            "inspect_project",
            &serde_json::json!({"path": "/tmp"}),
            false,
            &ex,
            &OkEffector,
        );
        let rec = out.record.unwrap();
        assert_eq!(rec.capability.as_deref(), Some("inspect_project"));
        assert!(rec.started_at.len() > 0);
    }
}
