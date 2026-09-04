//! ✅ Verifier — Phase 11 of the Strawberry platform.
//!
//! Execution success ≠ goal success (master spec §Verifier). The verifier
//! compares EXPECTED vs ACTUAL outcomes using observable evidence only:
//! command output, exit codes, file state. It never mutates user state.
//!
//! Deterministic: same evidence + same expectation ⇒ same verdict.

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::executor::{ActionRecord, ExecutionState};
use super::safety::ActionType;

// ─────────────────────────── model ───────────────────────────

/// Final verification verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    /// Evidence positively proves the expected outcome.
    Success,
    /// Evidence positively disproves it.
    Failure,
    /// Not enough evidence either way — the honest third state.
    Unknown,
}

/// How to check an outcome. `contains` searches the captured output;
/// `exit_zero` checks the process result; `file_exists` stats the path;
/// `file_contains` reads a (bounded) slice of the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expectation {
    ExitZero,
    OutputContains(String),
    FileExists(String),
    FileContains { path: String, needle: String },
    /// Explicitly cannot be checked automatically — always Unknown.
    Manual,
}

/// The verification result with evidence for the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationResult {
    pub action_id: u64,
    pub verification: Verification,
    /// 0.0–1.0 — how strongly the evidence supports the verdict.
    pub confidence: f32,
    /// Human chain explaining HOW we verified.
    pub evidence: Vec<String>,
}

// ─────────────────────────── the verifier ───────────────────────────

pub struct Verifier;

impl Verifier {
    /// Verify one executed action against one expectation.
    /// Read-only by construction: FileExists/FileContains only stat/read.
    pub fn verify(record: &ActionRecord, expectation: &Expectation) -> VerificationResult {
        let mut evidence: Vec<String> = Vec::new();
        let verdict = Self::check(record, expectation, &mut evidence);

        // Confidence is deterministic per verdict class:
        //   Success with positive evidence → 0.9
        //   Failure with direct contradiction → 0.9
        //   Unknown → 0.4 (we know that we don't know)
        let confidence = match verdict {
            Verification::Success | Verification::Failure => 0.9,
            Verification::Unknown => 0.4,
        };

        VerificationResult {
            action_id: record.action_id.raw(),
            verification: verdict,
            confidence,
            evidence,
        }
    }

    fn check(
        record: &ActionRecord,
        exp: &Expectation,
        evidence: &mut Vec<String>,
    ) -> Verification {
        // First: did the action even run to completion?
        if record.execution_state == ExecutionState::Failed
            || record.execution_state == ExecutionState::TimedOut
            || record.execution_state == ExecutionState::Cancelled
        {
            evidence.push(format!(
                "execution state is {:?}; expected outcome cannot hold",
                record.execution_state
            ));
            return Verification::Failure;
        }
        if record.execution_state == ExecutionState::NotExecuted {
            evidence.push("action never executed; verification is unknown".into());
            return Verification::Unknown;
        }

        match exp {
            Expectation::ExitZero => {
                let ok = record.exit_code == Some(0);
                evidence.push(format!("exit code = {:?}, expected 0", record.exit_code));
                if ok {
                    Verification::Success
                } else {
                    Verification::Failure
                }
            }
            Expectation::OutputContains(needle) => {
                // Command output is the evidence.
                if record.action_type != ActionType::RunCommand {
                    evidence.push("no command output recorded for this action type".into());
                    return Verification::Unknown;
                }
                let found = record.output.contains(needle.as_str());
                evidence.push(format!(
                    "output {} contains {:?}",
                    if found { "does" } else { "does not" },
                    needle
                ));
                if found {
                    Verification::Success
                } else if record.output.trim().is_empty() {
                    evidence.push("output empty — cannot confirm".into());
                    Verification::Unknown
                } else {
                    Verification::Failure
                }
            }
            Expectation::FileExists(path) => {
                let ok = Path::new(path).exists();
                evidence.push(format!("file {path} {}", if ok { "exists" } else { "missing" }));
                if ok {
                    Verification::Success
                } else {
                    Verification::Failure
                }
            }
            Expectation::FileContains { path, needle } => {
                match std::fs::read_to_string(path) {
                    Ok(text) => {
                        let bounded: String = text.chars().take(64_000).collect();
                        let found = bounded.contains(needle.as_str());
                        evidence.push(format!(
                            "file {path} {} contains {:?}",
                            if found { "does" } else { "does not" },
                            needle
                        ));
                        if found {
                            Verification::Success
                        } else {
                            Verification::Failure
                        }
                    }
                    Err(e) => {
                        evidence.push(format!("cannot read {path}: {e}"));
                        Verification::Unknown
                    }
                }
            }
            Expectation::Manual => {
                evidence.push("manual expectation — no automatic evidence available".into());
                Verification::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::executor::ApprovalState;
    use super::super::ids::ActionId;
    use super::super::safety::AuthorizedAction;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    fn record(state: ExecutionState, exit: Option<i32>, output: &str) -> ActionRecord {
        ActionRecord {
            action_id: ActionId::new(1),
            action_type: ActionType::RunCommand,
            target: "test".into(),
            goal_id: None,
            plan_id: None,
            capability: None,
            reason: "test".into(),
            started_at: "2026-09-03T00:00:00Z".into(),
            finished_at: Some("2026-09-03T00:00:01Z".into()),
            approval_state: ApprovalState::Approved,
            execution_state: state,
            exit_code: exit,
            output: output.to_string(),
            error: None,
            duration_ms: 1,
            cancelled: false,
        }
    }

    #[test]
    fn successful_exit_zero_verification() {
        let r = record(ExecutionState::Succeeded, Some(0), "done");
        let v = Verifier::verify(&r, &Expectation::ExitZero);
        assert_eq!(v.verification, Verification::Success);
        assert!((v.confidence - 0.9).abs() < 1e-6);
        assert!(!v.evidence.is_empty());
    }

    #[test]
    fn failed_exit_code_is_a_failure() {
        let r = record(ExecutionState::Succeeded, Some(3), "partial");
        let v = Verifier::verify(&r, &Expectation::ExitZero);
        assert_eq!(v.verification, Verification::Failure);
    }

    #[test]
    fn output_match_verifies_success() {
        let r = record(ExecutionState::Succeeded, Some(0), "hello strawberry world");
        let v = Verifier::verify(&r, &Expectation::OutputContains("strawberry".into()));
        assert_eq!(v.verification, Verification::Success);
    }

    #[test]
    fn empty_output_is_unknown_not_failure() {
        // Honest evidence gap: a command may succeed silently.
        let r = record(ExecutionState::Succeeded, Some(0), "");
        let v = Verifier::verify(&r, &Expectation::OutputContains("anything".into()));
        assert_eq!(v.verification, Verification::Unknown);
        assert!((v.confidence - 0.4).abs() < 1e-6);
    }

    #[test]
    fn missing_evidence_is_unknown() {
        let r = record(ExecutionState::NotExecuted, None, "");
        let v = Verifier::verify(&r, &Expectation::ExitZero);
        assert_eq!(v.verification, Verification::Unknown);
    }

    #[test]
    fn failed_execution_verifies_as_failure() {
        let r = record(ExecutionState::Failed, Some(1), "boom");
        let v = Verifier::verify(&r, &Expectation::OutputContains("boom".into()));
        assert_eq!(v.verification, Verification::Failure);
        assert!(v.evidence[0].contains("execution state"));
    }

    #[test]
    fn file_state_verification_reads_without_mutating() {
        let dir = std::env::temp_dir().join(format!("sb-verif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("evidence.txt");
        std::fs::write(&f, "the answer is 42").unwrap();
        let before = std::fs::metadata(&f).unwrap().modified().ok();

        let r = record(ExecutionState::Succeeded, Some(0), "");

        let v = Verifier::verify(&r, &Expectation::FileExists(f.display().to_string()));
        assert_eq!(v.verification, Verification::Success);

        let v = Verifier::verify(
            &r,
            &Expectation::FileContains { path: f.display().to_string(), needle: "42".into() },
        );
        assert_eq!(v.verification, Verification::Success);

        let v = Verifier::verify(
            &r,
            &Expectation::FileContains { path: f.display().to_string(), needle: "99".into() },
        );
        assert_eq!(v.verification, Verification::Failure);

        // Read-only proof: mtime unchanged (or same nanos), content intact.
        let after = std::fs::metadata(&f).unwrap().modified().ok();
        assert_eq!(before, after, "verifier must not touch the file");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "the answer is 42");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_file_is_unknown() {
        let r = record(ExecutionState::Succeeded, Some(0), "");
        let v = Verifier::verify(
            &r,
            &Expectation::FileContains {
                path: "/definitely/not/here/xx.txt".into(),
                needle: "x".into(),
            },
        );
        assert_eq!(v.verification, Verification::Unknown);
    }

    #[test]
    fn manual_expectation_is_always_unknown() {
        let r = record(ExecutionState::Succeeded, Some(0), "ran");
        let v = Verifier::verify(&r, &Expectation::Manual);
        assert_eq!(v.verification, Verification::Unknown);
    }

    #[test]
    fn verification_is_deterministic() {
        let r = record(ExecutionState::Succeeded, Some(0), "abc");
        let a = Verifier::verify(&r, &Expectation::OutputContains("abc".into()));
        let b = Verifier::verify(&r, &Expectation::OutputContains("abc".into()));
        assert_eq!(a.verification, b.verification);
        assert_eq!(a.confidence, b.confidence);
        assert_eq!(a.evidence, b.evidence);
    }

    #[test]
    fn end_to_end_execute_then_verify() {
        // Real chain: gate → authorize → execute → verify. The core scenario
        // from the master spec.
        use super::super::executor::Executor;
        use super::super::safety::{ActionRequest, Actor, SafetyGate};

        let mut req = ActionRequest {
            action_type: ActionType::RunCommand,
            target: "printf verified-42".into(),
            actor: Actor::User,
            user_approved: true,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        };
        let _ = &mut req;
        let dec = SafetyGate::evaluate(&req, super::super::safety::RiskMode::Normal);
        let auth = AuthorizedAction::from_decision(&dec, "printf verified-42").unwrap();

        let ex = Executor::new();
        let rec = ex.execute(
            auth,
            &super::super::executor::ShellEffector,
            None, None, None, "e2e", Duration::from_secs(10),
        );
        assert_eq!(rec.execution_state, ExecutionState::Succeeded);

        let v = Verifier::verify(&rec, &Expectation::OutputContains("verified-42".into()));
        assert_eq!(v.verification, Verification::Success);
        assert_eq!(v.confidence, 0.9);
    }

    // Silence unused-import warnings for AtomicBool in test scope.
    #[allow(dead_code)]
    fn _touch() {
        let _ = AtomicBool::new(false);
    }
}
