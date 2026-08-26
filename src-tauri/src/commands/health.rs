//! 🩺 Health Lens — local laptop health, read-only.
//! Disk free space + cache bloat + biggest home folders. No writes, no daemons,
//! plain `df`/`du` under the hood. Linux-first; other platforms get a graceful
//! "not supported yet" instead of an error.

use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use super::blocking;

type Cmd<T> = Result<T, String>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSize {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub supported: bool,
    pub disk_free_bytes: u64,
    pub disk_total_bytes: u64,
    pub caches: Vec<CacheSize>,
    pub top_home_dirs: Vec<CacheSize>,
    pub notes: Vec<String>,
}

fn du_bytes(path: &std::path::Path) -> Option<u64> {
    let out = std::process::Command::new("du")
        .args(["-s", "--block-size=1"])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().next()?.parse::<u64>().ok()
}

fn df_home() -> Option<(u64, u64)> {
    let home = std::env::var("HOME").ok()?;
    let out = std::process::Command::new("df")
        .args(["-P", &home])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let last = text.lines().last()?;
    let fields: Vec<&str> = last.split_whitespace().collect();
    // Filesystem 512-blocks Used Available Capacity Mounted-on
    let total = fields.get(1)?.parse::<u64>().ok()? * 512;
    let avail = fields.get(3)?.parse::<u64>().ok()? * 512;
    Some((total, avail))
}

/// Scan laptop health. Runs `du` on a handful of well-known cache locations —
/// may take a few seconds on big caches.
#[tauri::command]
pub async fn health_report(state: State<'_, Arc<AppState>>) -> Cmd<HealthReport> {
    let _st = state.inner().clone();
    blocking(_st, move |_app| {
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var("HOME").unwrap_or_default();
            let mut caches: Vec<CacheSize> = Vec::new();
            let candidates = [
                "~/.cache",
                "~/.cache/mozilla",
                "~/.cache/google-chrome",
                "~/.cache/chromium",
                "~/.cache/BraveSoftware",
                "~/.cache/yarn",
                "~/.cache/pip",
                "~/.npm",
                "~/.local/share/Trash",
                "/tmp",
            ];
            for c in candidates {
                let p = shellexpand_home(c);
                let path = std::path::Path::new(&p);
                if path.exists() {
                    if let Some(bytes) = du_bytes(path) {
                        if bytes > 0 {
                            caches.push(CacheSize { path: c.to_string(), bytes });
                        }
                    }
                }
            }
            caches.sort_by(|a, b| b.bytes.cmp(&a.bytes));

            // First-level home folders, top 6 by size.
            let mut top: Vec<CacheSize> = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&home) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        if let Some(bytes) = du_bytes(&p) {
                            top.push(CacheSize {
                                path: entry.file_name().to_string_lossy().to_string(),
                                bytes,
                            });
                        }
                    }
                }
            }
            top.sort_by(|a, b| b.bytes.cmp(&a.bytes));
            top.truncate(6);

            let (disk_total_bytes, disk_free_bytes) = df_home().unwrap_or((0, 0));

            let mut notes = Vec::new();
            if disk_free_bytes > 0 && disk_free_bytes < 10 * 1024 * 1024 * 1024 {
                notes.push("⚠️ Disk free < 10 GB — clean caches soon.".to_string());
            }
            if let Some(biggest) = caches.first() {
                if biggest.bytes > 2 * 1024 * 1024 * 1024 {
                    notes.push(format!(
                        "💡 {} is using {:.1} GB — safe to clear browser/app caches.",
                        biggest.path,
                        biggest.bytes as f64 / 1_073_741_824.0
                    ));
                }
            }
            if notes.is_empty() {
                notes.push("✅ Sab theek lag raha hai.".to_string());
            }

            Ok(HealthReport {
                supported: true,
                disk_free_bytes,
                disk_total_bytes,
                caches,
                top_home_dirs: top,
                notes,
            })
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(HealthReport {
                supported: false,
                disk_free_bytes: 0,
                disk_total_bytes: 0,
                caches: Vec::new(),
                top_home_dirs: Vec::new(),
                notes: vec!["Health Lens is Linux-only right now.".to_string()],
            })
        }
    })
    .await
}

fn shellexpand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

fn gb(bytes: u64) -> f64 {
    (bytes as f64 / 1_073_741_824.0 * 10.0).round() / 10.0
}

// keep gb used in future UI formatting; avoid dead-code warning
#[allow(dead_code)]
fn _gb_used() -> f64 {
    gb(0)
}
