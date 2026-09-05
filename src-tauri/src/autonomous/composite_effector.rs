//! 🔌 CompositeEffector — dispatches AuthorizedAction to the right Effector.
//!
//! Before this, the lifecycle.rs ran with only ShellEffector — meaning
//! every file operation was rejected because ShellEffector only handles
//! RunCommand. After this:
//!
//!   ActionType::FileRead    → SafeFileEffector
//!   ActionType::FileWrite   → SafeFileEffector
//!   ActionType::RunCommand  → ShellEffector
//!   anything else           → returns -1 with a clear reason
//!
//! Both effectors are real (not test doubles), and the composite is what
//! the autonomous worker now installs in the lifecycle.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::executor::Effector;
use super::file_effector::SafeFileEffector;
use super::safety::{ActionType, AuthorizedAction};

pub struct CompositeEffector {
    pub shell: super::executor::ShellEffector,
    pub file: SafeFileEffector,
}

impl CompositeEffector {
    pub fn new() -> Self {
        Self {
            shell: super::executor::ShellEffector,
            file: SafeFileEffector::new(),
        }
    }
}

impl Default for CompositeEffector {
    fn default() -> Self {
        Self::new()
    }
}

impl Effector for CompositeEffector {
    fn run(
        &self,
        action: &AuthorizedAction,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> (i32, String) {
        match action.action_type {
            ActionType::RunCommand => self.shell.run(action, cancel, timeout),
            ActionType::FileRead | ActionType::FileWrite => self.file.run(action, cancel, timeout),
            _ => (
                -1,
                format!(
                    "no effector for action {} (target={})",
                    action.action_type.label(),
                    action.target
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::safety::{ActionRequest, Actor, RiskMode, SafetyGate};

    fn approved_run(target: &str) -> AuthorizedAction {
        let r = ActionRequest {
            action_type: ActionType::RunCommand,
            target: target.into(),
            actor: Actor::User,
            user_approved: true,
            data_sensitivity: 1,
            external_destination: false,
            destructive: false,
        };
        let dec = SafetyGate::evaluate(&r, RiskMode::Normal);
        AuthorizedAction::from_decision(&dec, target).unwrap()
    }

    #[test]
    fn composite_runs_shell_commands() {
        let eff = CompositeEffector::new();
        let a = approved_run("printf composite-ok");
        let (code, out) = eff.run(&a, &AtomicBool::new(false), Duration::from_secs(5));
        assert_eq!(code, 0);
        assert!(out.contains("composite-ok"));
    }

    #[test]
    fn composite_runs_file_reads() {
        let path = std::env::temp_dir().join(format!(
            "strawberry-comp-read-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::write(&path, "data here").unwrap();
        let eff = CompositeEffector::new();
        let a = AuthorizedAction {
            action_type: ActionType::FileRead,
            target: path.display().to_string(),
            authorization_reasons: vec!["test".into()],
        };
        let (code, out) = eff.run(&a, &AtomicBool::new(false), Duration::from_secs(5));
        assert_eq!(code, 0);
        assert!(out.contains("data here"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn composite_runs_file_writes() {
        let path = std::env::temp_dir().join(format!(
            "strawberry-comp-write-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let target = format!("{}|written by composite", path.display());
        let eff = CompositeEffector::new();
        let a = AuthorizedAction {
            action_type: ActionType::FileWrite,
            target,
            authorization_reasons: vec!["test".into()],
        };
        let (code, _) = eff.run(&a, &AtomicBool::new(false), Duration::from_secs(5));
        assert_eq!(code, 0);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("written by composite"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn composite_rejects_unhandled_action() {
        let eff = CompositeEffector::new();
        let a = AuthorizedAction {
            action_type: ActionType::SendMessage,
            target: "somewhere".into(),
            authorization_reasons: vec!["test".into()],
        };
        let (code, out) = eff.run(&a, &AtomicBool::new(false), Duration::from_secs(1));
        assert_eq!(code, -1);
        assert!(out.contains("no effector"));
    }
}
