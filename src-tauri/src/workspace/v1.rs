//! 🍓 Workspace Resume v0.1 Domain Models, Adapters, and Action Engine.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Capturing,
    Frozen,
    Restoring,
    Restored,
    Partial,
    Failed,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Capturing => "capturing",
            Self::Frozen => "frozen",
            Self::Restoring => "restoring",
            Self::Restored => "restored",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "capturing" => Self::Capturing,
            "frozen" => Self::Frozen,
            "restoring" => Self::Restoring,
            "restored" => Self::Restored,
            "partial" => Self::Partial,
            "failed" => Self::Failed,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Pending,
    Launching,
    Restored,
    Skipped,
    Failed,
}

impl ItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Launching => "launching",
            Self::Restored => "restored",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "launching" => Self::Launching,
            "restored" => Self::Restored,
            "skipped" => Self::Skipped,
            "failed" => Self::Failed,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSession {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub frozen_at: Option<i64>,
    pub resumed_at: Option<i64>,
    pub status: SessionStatus,
    pub trigger: String,
    pub metadata_json: Option<String>,
    pub items: Vec<WorkspaceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItem {
    pub id: String,
    pub session_id: String,
    pub item_type: String,
    pub app_name: Option<String>,
    pub process_name: Option<String>,
    pub window_title: Option<String>,
    pub window_geometry: Option<String>,
    pub workspace: Option<String>,
    pub cwd: Option<String>,
    pub command: Option<String>,
    pub browser_url: Option<String>,
    pub browser_title: Option<String>,
    pub restore_strategy: String,
    pub restore_status: ItemStatus,
    pub error_message: Option<String>,
    pub action_type: Option<String>,
    pub action_target: Option<String>,
    pub action_payload: Option<String>,
    pub display_label: Option<String>,
    pub last_action_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRestoreAttempt {
    pub id: String,
    pub session_id: String,
    pub item_id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub success: bool,
    pub item_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceAction {
    OpenUrl { url: String },
    OpenFolder { path: String },
    OpenVsCodeProject { path: String },
    OpenVsCodeFile {
        path: String,
        line: Option<u32>,
        column: Option<u32>,
    },
    OpenTerminal { cwd: String },
    RunTerminalCommand {
        cwd: String,
        command: String,
        confirmed: bool,
    },
}

impl WorkspaceAction {
    pub fn action_type_str(&self) -> &'static str {
        match self {
            Self::OpenUrl { .. } => "open_url",
            Self::OpenFolder { .. } => "open_folder",
            Self::OpenVsCodeProject { .. } => "open_vscode_project",
            Self::OpenVsCodeFile { .. } => "open_vscode_file",
            Self::OpenTerminal { .. } => "open_terminal",
            Self::RunTerminalCommand { .. } => "run_terminal_command",
        }
    }

    pub fn display_label(&self) -> String {
        match self {
            Self::OpenUrl { url } => format!("Open Tab: {url}"),
            Self::OpenFolder { path } => format!("Jump to Folder: {path}"),
            Self::OpenVsCodeProject { path } => format!("Open Project in VS Code: {path}"),
            Self::OpenVsCodeFile { path, line, .. } => match line {
                Some(l) => format!("Open File: {path}:{l}"),
                None => format!("Open File: {path}"),
            },
            Self::OpenTerminal { cwd } => format!("Open Terminal Here: {cwd}"),
            Self::RunTerminalCommand { cwd, command, .. } => format!("Run Command in {cwd}: {command}"),
        }
    }

    pub fn execute(&self) -> Result<String, String> {
        match self {
            Self::OpenUrl { url } => {
                let u = url.trim();
                if !u.starts_with("http://") && !u.starts_with("https://") {
                    return Err(format!("Disallowed URL protocol: {url}"));
                }
                Command::new("xdg-open")
                    .arg(u)
                    .spawn()
                    .map_err(|e| format!("Failed to launch browser URL: {e}"))?;
                Ok(format!("Opened URL {u}"))
            }
            Self::OpenFolder { path } => {
                let p = Path::new(path);
                if !p.exists() || !p.is_dir() {
                    return Err(format!("Directory does not exist: {path}"));
                }
                Command::new("xdg-open")
                    .arg(p)
                    .spawn()
                    .map_err(|e| format!("Failed to open folder: {e}"))?;
                Ok(format!("Opened folder {path}"))
            }
            Self::OpenVsCodeProject { path } => {
                let p = Path::new(path);
                if !p.exists() {
                    return Err(format!("VS Code path does not exist: {path}"));
                }
                Command::new("code")
                    .arg(p)
                    .spawn()
                    .map_err(|e| format!("Failed to launch 'code': {e}"))?;
                Ok(format!("Opened VS Code project {path}"))
            }
            Self::OpenVsCodeFile { path, line, column } => {
                let p = Path::new(path);
                if !p.exists() {
                    return Err(format!("File does not exist: {path}"));
                }
                let mut target = path.clone();
                if let Some(l) = line {
                    target.push_str(&format!(":{l}"));
                    if let Some(c) = column {
                        target.push_str(&format!(":{c}"));
                    }
                }
                Command::new("code")
                    .arg("-g")
                    .arg(&target)
                    .spawn()
                    .map_err(|e| format!("Failed to launch 'code -g {target}': {e}"))?;
                Ok(format!("Opened file {target}"))
            }
            Self::OpenTerminal { cwd } => {
                let p = Path::new(cwd);
                if !p.exists() || !p.is_dir() {
                    return Err(format!("Terminal working directory does not exist: {cwd}"));
                }
                // Try konsole, gnome-terminal, kitty, alacritty, or x-terminal-emulator
                let launchers = [
                    ("konsole", vec!["--workdir", cwd]),
                    ("gnome-terminal", vec!["--working-directory", cwd]),
                    ("kitty", vec!["--directory", cwd]),
                    ("alacritty", vec!["--working-directory", cwd]),
                    ("x-terminal-emulator", vec![]),
                ];
                for (bin, args) in launchers {
                    if Command::new(bin).args(&args).spawn().is_ok() {
                        return Ok(format!("Opened terminal ({bin}) in {cwd}"));
                    }
                }
                Err("No supported Linux terminal emulator found (konsole, gnome-terminal, kitty, alacritty)".into())
            }
            Self::RunTerminalCommand { cwd, command, confirmed } => {
                if !confirmed {
                    return Err("Running captured terminal command requires explicit user confirmation.".into());
                }
                let p = Path::new(cwd);
                if !p.exists() || !p.is_dir() {
                    return Err(format!("Directory does not exist: {cwd}"));
                }
                Command::new("konsole")
                    .args(["--workdir", cwd, "-e", "bash", "-c", &format!("{command}; exec bash")])
                    .spawn()
                    .or_else(|_| {
                        Command::new("gnome-terminal")
                            .args(["--working-directory", cwd, "--", "bash", "-c", &format!("{command}; exec bash")])
                            .spawn()
                    })
                    .or_else(|_| {
                        Command::new("kitty")
                            .args(["--directory", cwd, "bash", "-c", &format!("{command}; exec bash")])
                            .spawn()
                    })
                    .map_err(|e| format!("Failed to execute terminal command: {e}"))?;
                Ok(format!("Executed '{command}' in {cwd}"))
            }
        }
    }
}

pub trait WorkspaceCollector {
    fn capture(&self) -> Result<Vec<WorkspaceItem>, String>;
}

pub trait WorkspaceRestorer {
    fn restore(&self, item: &WorkspaceItem) -> Result<ActionResult, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_action_labels() {
        let action1 = WorkspaceAction::OpenVsCodeProject {
            path: "/home/user/projects/strawberry".into(),
        };
        assert_eq!(action1.action_type_str(), "open_vscode_project");
        assert!(action1.display_label().contains("VS Code"));

        let action2 = WorkspaceAction::OpenUrl {
            url: "https://tauri.app".into(),
        };
        assert_eq!(action2.action_type_str(), "open_url");
        assert!(action2.display_label().contains("https://tauri.app"));

        let action3 = WorkspaceAction::RunTerminalCommand {
            cwd: "/home/user".into(),
            command: "npm start".into(),
            confirmed: false,
        };
        assert_eq!(action3.action_type_str(), "run_terminal_command");
        assert!(action3.execute().is_err()); // Unconfirmed command execution must fail
    }

    #[test]
    fn test_url_protocol_validation() {
        let unsafe_url = WorkspaceAction::OpenUrl {
            url: "file:///etc/passwd".into(),
        };
        assert!(unsafe_url.execute().is_err());
    }
}
