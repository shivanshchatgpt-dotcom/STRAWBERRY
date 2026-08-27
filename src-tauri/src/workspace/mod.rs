//! 🧊 Freeze & Resume — capture the WHOLE live workspace and bring it back.
//!
//! freeze(): every normal window (class, title, geometry, desktop, focus),
//! browser tabs (Firefox session store / Chrome recent), terminal working
//! directories (/proc/<pid>/cwd of shells), listening dev servers
//! (`ss -tlnp` + /proc) and a launch command per app (.desktop lookup).
//!
//! restore(): relaunches everything — browsers with their URLs, terminals
//! in their directories, apps via resolved commands — then asks KWin to
//! put the windows back at their saved geometry. Dev servers are never
//! auto-executed; they come back as a pending checklist.

pub mod v1;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error;

const MAX_WINDOWS: usize = 40;
const MARKER: &str = "SBFRZ|";
const SELF_CLASS: &str = "chat-memory-tree";
const TERMINALS: &[&str] = &[
    "konsole", "kitty", "alacritty", "gnome-terminal-", "gnome-terminal-server",
    "xterm", "yakuake", "terminator", "wezterm", "foot",
];
const SHELLS: &[&str] = &["bash", "zsh", "fish", "sh", "pwsh"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenWindow {
    pub app: String,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub desktop: i32,
    pub active: bool,
    #[serde(default)]
    pub launch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRestore {
    pub browser: String,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServer {
    pub port: u16,
    #[serde(default)]
    pub pid: Option<i64>,
    pub proc_name: String,
    pub cwd: String,
    pub cmd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkSpace {
    pub id: String,
    pub name: String,
    pub story: String,
    pub created_at: String,
    pub windows: Vec<FrozenWindow>,
    pub browsers: Vec<BrowserRestore>,
    pub terminals: Vec<String>,
    pub dev_servers: Vec<DevServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub launched: Vec<String>,
    pub failed: Vec<String>,
    pub pending_servers: Vec<DevServer>,
}

// ---------------------------------------------------------------------------
// Freeze
// ---------------------------------------------------------------------------

pub fn collect() -> WorkSpace {
    let windows = kwin_frozen().unwrap_or_default();
    let active = windows.iter().find(|w| w.active).cloned();

    // resolve launch commands for non-self windows
    let desktop_map = desktop_exec_map();
    let mut resolved = windows.clone();
    for w in &mut resolved {
        if w.app == SELF_CLASS {
            continue; // it's us; already running
        }
        if is_terminal_class(&w.app) || is_browser_class(&w.app) {
            continue; // handled specially by restore
        }
        w.launch = resolve_launch(&w.app, &desktop_map);
    }

    let mut browsers: Vec<BrowserRestore> = Vec::new();
    if let Some(urls) = crate::snapshot::firefox_tab_urls() {
        if !urls.is_empty() {
            browsers.push(BrowserRestore { browser: "firefox".into(), urls });
        }
    }
    if let Some(items) = chrome_recent_pages_fast() {
        if !items.is_empty() {
            browsers.push(BrowserRestore { browser: "chrome".into(), urls: items });
        }
    }

    let terminals = shell_cwds();
    let dev_servers = dev_servers();

    let created_at = db::now_iso();
    let story = story_for(&created_at, active.as_ref(), &resolved, &browsers, &terminals, &dev_servers);
    let name = name_for(&created_at, active.as_ref(), &resolved);

    WorkSpace {
        id: db::new_uuid(),
        name,
        story,
        created_at,
        windows: resolved,
        browsers,
        terminals,
        dev_servers,
    }
}

fn story_for(
    created_at: &str,
    active: Option<&FrozenWindow>,
    windows: &[FrozenWindow],
    browsers: &[BrowserRestore],
    terminals: &[String],
    servers: &[DevServer],
) -> String {
    let time = chrono::Local::now().format("%H:%M").to_string() + " baje";
    let mut parts = Vec::new();
    match active {
        Some(a) => parts.push(format!(
            "{time} tum {} me the — \"{}\"",
            a.app,
            a.title.chars().take(80).collect::<String>()
        )),
        None => parts.push(format!("{time} {} windows khuli thi", windows.len())),
    }
    let apps: BTreeMap<String, usize> =
        windows.iter().fold(BTreeMap::new(), |mut m, w| {
            *m.entry(w.app.clone()).or_default() += 1;
            m
        });
    if !apps.is_empty() {
        parts.push(format!("{} apps: {}", windows.len(), apps.keys().cloned().collect::<Vec<_>>().join(", ")));
    }
    for b in browsers {
        parts.push(format!(
            "{} ke {} tabs ({})",
            capitalize(&b.browser),
            b.urls.len(),
            b.urls.first().map(|u| host_of(u)).unwrap_or_default()
        ));
    }
    if !terminals.is_empty() {
        parts.push(format!(
            "terminal{} me {}",
            if terminals.len() > 1 { "s" } else { "" },
            terminals.iter().take(2).map(|c| short_cwd(c)).collect::<Vec<_>>().join(", ")
        ));
    }
    if !servers.is_empty() {
        parts.push(format!(
            "dev server{} port {} pe chal rahe the",
            if servers.len() > 1 { "s" } else { "" },
            servers.iter().map(|s| s.port.to_string()).collect::<Vec<_>>().join(", ")
        ));
    }
    parts.join(". ") + "."
}

fn name_for(_created_at: &str, active: Option<&FrozenWindow>, windows: &[FrozenWindow]) -> String {
    let hhmm = chrono::Local::now().format("%H:%M").to_string();
    let anchor = active
        .map(|a| top_word(&a.title))
        .or_else(|| windows.iter().find(|w| w.app != SELF_CLASS).map(|w| top_word(&w.title)))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Session".into());
    format!("🧊 {anchor} · {hhmm}")
}

fn top_word(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 4)
        .max_by_key(|w| w.len())
        .map(|w| w.to_lowercase())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .unwrap_or_default()
}

fn short_cwd(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
        .trim_start_matches("www.")
        .chars()
        .take(28)
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}


/// Best-effort launch command: .desktop map (case-insensitive), else treat
/// the class (or its org.kde suffix) as a PATH binary.
fn resolve_launch(class: &str, map: &BTreeMap<String, String>) -> Option<String> {
    if let Some(e) = map.get(&class.to_lowercase()) {
        return Some(e.clone());
    }
    let mut candidates: Vec<&str> = vec![class];
    if let Some(suffix) = class.strip_prefix("org.kde.") {
        candidates.push(suffix);
    }
    for cand in candidates {
        if cand.is_empty() { continue; }
        let found = Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {cand}"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if found {
            return Some(cand.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Collectors
// ---------------------------------------------------------------------------

/// Windows + geometry via KWin script → journal lines:
/// SBFRZ|class|title|x,y,w,h|desktop|active
fn kwin_frozen() -> Result<Vec<FrozenWindow>, String> {
    let script_name = format!("sb-freeze-{}", db::new_uuid());
    let path = std::env::temp_dir().join(format!("{script_name}.js"));
    std::fs::write(
        &path,
        format!(
            "for (const w of workspace.windowList()) {{\n\
             \x20 if (!w.normalWindow) continue;\n\
             \x20 const t = (w.caption || '').split('|').join('/');\n\
             \x20 const g = w.frameGeometry;\n\
             \x20 const geo = Math.round(g.x) + ',' + Math.round(g.y) + ',' + Math.round(g.width) + ',' + Math.round(g.height);\n\
             \x20 const dsk = (typeof w.desktop === 'number') ? w.desktop : 0;\n\
             \x20 const act = (workspace.activeWindow === w) ? 1 : 0;\n\
             \x20 console.info('{MARKER}' + w.resourceClass + '|' + t + '|' + geo + '|' + dsk + '|' + act);\n\
             }}"
        ),
    )
    .map_err(error::to_string_err("freeze script write"))?;

    let qdbus = |args: &[&str]| -> Result<String, String> {
        let out = Command::new("qdbus6")
            .arg("org.kde.KWin")
            .arg("/Scripting")
            .args(args)
            .output()
            .map_err(error::to_string_err("qdbus6 spawn"))?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };

    let run = (|| -> Result<Vec<FrozenWindow>, String> {
        let id = qdbus(&[
            "org.kde.kwin.Scripting.loadScript",
            &path.to_string_lossy(),
            &script_name,
        ])?;
        if id.is_empty() {
            return Err("loadScript returned nothing".into());
        }
        Command::new("qdbus6")
            .arg("org.kde.KWin")
            .arg(format!("/Scripting/Script{id}"))
            .arg("org.kde.kwin.Script.run")
            .output()
            .map_err(error::to_string_err("freeze script run"))?;
        thread::sleep(Duration::from_millis(900));
        let journal = Command::new("journalctl")
            .args(["--user", "-n", "200", "--output=cat"])
            .output()
            .map_err(error::to_string_err("journalctl spawn"))?;
        Ok(parse_freeze_lines(&String::from_utf8_lossy(&journal.stdout)))
    })();

    let _ = Command::new("qdbus6")
        .arg("org.kde.KWin")
        .arg("/Scripting")
        .arg("org.kde.kwin.Scripting.unloadScript")
        .arg(&script_name)
        .output();
    let _ = std::fs::remove_file(&path);
    run
}

fn parse_freeze_lines(journal: &str) -> Vec<FrozenWindow> {
    let mut out: Vec<FrozenWindow> = Vec::new();
    for line in journal.lines() {
        let Some(rest) = line.trim().strip_prefix(MARKER) else { continue };
        let segs: Vec<&str> = rest.splitn(5, '|').collect();
        if segs.len() < 5 { continue; }
        let app = segs[0].trim().to_string();
        if app.is_empty() || app == "undefined" { continue; }
        let title = segs[1].trim().to_string();
        let nums: Vec<i32> = segs[2]
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        if nums.len() != 4 { continue; }
        let desktop = segs[3].trim().parse().unwrap_or(0);
        let active = segs[4].trim() == "1";
        if out.iter().any(|w| w.app == app && w.title == title) { continue; }
        out.push(FrozenWindow {
            app,
            title,
            x: nums[0], y: nums[1], w: nums[2], h: nums[3],
            desktop,
            active,
            launch: None,
        });
        if out.len() >= MAX_WINDOWS { break; }
    }
    let mut seen = false;
    for w in &mut out {
        if w.active {
            if seen { w.active = false; } else { seen = true; }
        }
    }
    out
}

fn is_terminal_class(class: &str) -> bool {
    TERMINALS.iter().any(|t| class.eq_ignore_ascii_case(t))
}
fn is_browser_class(class: &str) -> bool {
    ["firefox", "google-chrome", "chromium", "brave-browser", "microsoft-edge"]
        .iter()
        .any(|b| class.starts_with(b))
}

/// resourceClass → cleaned Exec line from installed .desktop files.
fn desktop_exec_map() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.filter_map(|e| e.ok()) {
            let Ok(content) = std::fs::read_to_string(entry.path()) else { continue };
            let mut wm_class: Option<String> = None;
            let mut name: Option<String> = None;
            let mut exec: Option<String> = None;
            for line in content.lines() {
                if let Some(v) = line.strip_prefix("StartupWMClass=") {
                    wm_class = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("Name=") {
                    name = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("Exec=") {
                    exec = Some(clean_exec(v));
                }
            }
            let Some(exec) = exec else { continue };
            if let Some(wmc) = wm_class {
                map.entry(wmc.to_lowercase()).or_insert(exec.clone());
            }
            if let Some(n) = name {
                let key = n.to_lowercase().replace(' ', "-");
                map.entry(key).or_insert(exec);
            }
        }
    }
    map
}

fn clean_exec(exec: &str) -> String {
    let mut out = exec.trim().to_string();
    for code in ["%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m"] {
        out = out.replace(code, "");
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
}

/// Unique working directories of interactive shells (terminals).
fn shell_cwds() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Ok(procfs) = std::fs::read_dir("/proc") else { return out };
    let mut pids: Vec<u32> = procfs
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_string_lossy().parse::<u32>().ok())
        .collect();
    pids.sort_unstable();
    for pid in pids {
        let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) else { continue };
        let name = comm.trim().to_string();
        if !SHELLS.contains(&name.as_str()) { continue; }
        if let Ok(cwd) = std::fs::read_link(format!("/proc/{pid}/cwd")) {
            let cwd = cwd.to_string_lossy().to_string();
            if cwd.starts_with("/proc") || cwd == "/" { continue; }
            if !out.contains(&cwd) && out.len() < 4 {
                out.push(cwd);
            }
        }
    }
    out
}

/// Listening dev servers from `ss -tlnp` enriched with /proc info.
fn dev_servers() -> Vec<DevServer> {
    let Ok(out) = Command::new("ss").args(["-tlnp"]).output() else { return Vec::new() };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut servers: Vec<DevServer> = Vec::new();
    for line in text.lines() {
        if !line.contains("LISTEN") { continue; }
        let Some((port, proc_name, pid)) = parse_ss_listen(line) else { continue };
        if port < 3000 || port > 65535 { continue; } // dev-ish range only
        if servers.iter().any(|s| s.port == port) { continue; }
        let (cwd, cmd) = pid
            .map(|p| proc_details(p))
            .unwrap_or_else(|| (String::new(), String::new()));
        if cwd.is_empty() { continue; }
        servers.push(DevServer { port, pid, proc_name, cwd, cmd });
        if servers.len() >= 8 { break; }
    }
    servers
}

fn parse_ss_listen(line: &str) -> Option<(u16, String, Option<i64>)> {
    let local = line.split_whitespace().nth(3)?;
    let port = local.rsplit(':').next()?.parse::<u16>().ok()?;
    let users_idx = line.find("users:((")?;
    let tail = &line[users_idx..];
    let name_start = tail.find('"')? + 1;
    let name_end = tail[name_start..].find('"')? + name_start;
    let proc_name = tail[name_start..name_end].to_string();
    let pid = tail[name_end..]
        .split("pid=")
        .nth(1)
        .and_then(|p| p.split(',').next())
        .and_then(|p| p.parse::<i64>().ok());
    Some((port, proc_name, pid))
}

fn proc_details(pid: i64) -> (String, String) {
    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd"))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let cmd = std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .unwrap_or_default()
        .split('\0')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect();
    (cwd, cmd)
}

/// Chrome URLs without full page fetch — reuse snapshot's recent pages.
fn chrome_recent_pages_fast() -> Option<Vec<String>> {
    let pages = crate::snapshot::chrome_recent_urls_public()?;
    Some(pages.into_iter().take(10).collect())
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

pub fn restore(ws: &WorkSpace) -> RestoreReport {
    let mut launched: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    // 1. browsers with URLs
    for b in &ws.browsers {
        if b.urls.is_empty() { continue; }
        let bin = match b.browser.as_str() {
            "firefox" => "firefox",
            "google-chrome" => "google-chrome-stable",
            other => other,
        };
        let res = Command::new(bin)
            .args(b.urls.iter().take(12))
            .spawn();
        match res {
            Ok(_) => launched.push(format!("{} ×{} tabs", b.browser, b.urls.len())),
            Err(e) => failed.push(format!("{}: {e}", b.browser)),
        }
        thread::sleep(Duration::from_millis(400));
    }

    // 2. terminals in their cwds
    for cwd in ws.terminals.iter().take(4) {
        let res = Command::new("konsole").arg("--workdir").arg(cwd).spawn();
        match res {
            Ok(_) => launched.push(format!("konsole @ {}", short_cwd(cwd))),
            Err(e) => failed.push(format!("konsole @{cwd}: {e}")),
        }
        thread::sleep(Duration::from_millis(250));
    }

    // 3. regular apps via resolved commands
    for w in &ws.windows {
        if w.app == SELF_CLASS || w.launch.is_none() { continue; }
        let exec = w.launch.as_deref().unwrap_or_default();
        if exec.is_empty() { continue; }
        let res = Command::new("sh").arg("-c").arg(exec).spawn();
        match res {
            Ok(_) => launched.push(w.app.clone()),
            Err(e) => failed.push(format!("{}: {e}", w.app)),
        }
        thread::sleep(Duration::from_millis(220));
    }

    // 4. window geometry back in place (best-effort after everything maps)
    thread::sleep(Duration::from_millis(2600));
    position_windows(&ws.windows);

    RestoreReport {
        launched,
        failed,
        pending_servers: ws.dev_servers.clone(),
    }
}

fn position_windows(targets: &[FrozenWindow]) {
    use std::collections::BTreeMap;
    let mut by_class: BTreeMap<String, Vec<[i32; 4]>> = BTreeMap::new();
    for w in targets {
        if w.app == SELF_CLASS { continue; }
        by_class
            .entry(w.app.clone())
            .or_default()
            .push([w.x, w.y, w.w, w.h]);
    }
    let Ok(json) = serde_json::to_string(&by_class) else { return };

    let script_name = format!("sb-position-{}", db::new_uuid());
    let path = std::env::temp_dir().join(format!("{script_name}.js"));
    let js = format!(
        "const T = {json};\n\
         for (const key of Object.keys(T)) {{\n\
         \x20 const list = T[key];\n\
         \x20 for (const w of workspace.windowList()) {{\n\
         \x20\x20\x20 if (list.length === 0) break;\n\
         \x20\x20\x20 if (w.resourceClass !== key) continue;\n\
         \x20\x20\x20 const g = list.shift();\n\
         \x20\x20\x20 try {{\n\
         \x20\x20\x20\x20\x20 w.frameGeometry = {{ x: g[0], y: g[1], width: g[2], height: g[3] }};\n\
         \x20\x20\x20 }} catch (e) {{ /* best effort */ }}\n\
         \x20 }}\n\
         }}"
    );
    if std::fs::write(&path, js).is_err() { return; }

    let id = Command::new("qdbus6")
        .arg("org.kde.KWin")
        .arg("/Scripting")
        .arg("org.kde.kwin.Scripting.loadScript")
        .arg(path.to_string_lossy().to_string())
        .arg(&script_name)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if !id.is_empty() {
        let _ = Command::new("qdbus6")
            .arg("org.kde.KWin")
            .arg(format!("/Scripting/Script{id}"))
            .arg("org.kde.kwin.Script.run")
            .output();
        thread::sleep(Duration::from_millis(700));
        let _ = Command::new("qdbus6")
            .arg("org.kde.KWin")
            .arg("/Scripting")
            .arg("org.kde.kwin.Scripting.unloadScript")
            .arg(&script_name)
            .output();
    }
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn save(conn: &rusqlite::Connection, ws: &WorkSpace) -> Result<(), String> {
    conn.execute(
        "INSERT INTO work_spaces(id,name,story,created_at,raw_json) VALUES(?1,?2,?3,?4,?5)",
        params![
            ws.id,
            ws.name,
            ws.story,
            ws.created_at,
            serde_json::to_string(ws).unwrap_or_default(),
        ],
    )
    .map_err(error::to_string_err("workspace insert"))?;
    Ok(())
}

pub fn list(conn: &rusqlite::Connection, limit: usize) -> Result<Vec<(String, String, String, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT id,name,story,created_at FROM work_spaces ORDER BY created_at DESC LIMIT ?1")
        .map_err(error::to_string_err("workspace list"))?;
    let rows = stmt
        .query_map(params![limit.clamp(1, 50) as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(error::to_string_err("workspace list map"))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get(conn: &rusqlite::Connection, id: &str) -> Result<Option<WorkSpace>, String> {
    use rusqlite::OptionalExtension;
    let raw: Option<String> = conn
        .query_row("SELECT raw_json FROM work_spaces WHERE id=?1", [id], |r| r.get(0))
        .optional()
        .map_err(error::to_string_err("workspace get"))?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn mark_restored(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE work_spaces SET restored_at=?1 WHERE id=?2",
        params![db::now_iso(), id],
    )
    .map_err(error::to_string_err("workspace restored_at"))?;
    Ok(())
}

pub fn delete(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM work_spaces WHERE id=?1", [id])
        .map_err(error::to_string_err("workspace delete"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_geometry_lines_and_single_active() {
        let journal = "\
SBFRZ|code|ARE YOU THERE #2 - STRAWBERRY|0,0,1920,1040|1|0
SBFRZ|google-chrome|ComfyUI workflow|100,80,1600,900|1|1
SBFRZ|chat-memory-tree|🍓 Strawberry — Second Brain|140,140,1270,860|0|0
SBFRZ|plasmashell||0,0,100,100|1|0";
        let wins = parse_freeze_lines(journal);
        assert_eq!(wins.len(), 4);
        assert_eq!((wins[0].x, wins[0].w), (0, 1920));
        assert!(wins[1].active);
        assert_eq!(wins[2].desktop, 0); // undefined desktop tolerated
        assert_eq!(wins[2].app, "chat-memory-tree");
        assert!(!wins[0].active && !wins[3].active);
    }

    #[test]
    fn cleans_field_codes_from_exec() {
        assert_eq!(clean_exec("code %F"), "code");
        assert_eq!(clean_exec("google-chrome-stable %u"), "google-chrome-stable");
        assert_eq!(clean_exec("konsole --workdir %f --hold"), "konsole --workdir --hold");
    }

    #[test]
    fn parses_ss_listen_line() {
        let line = "LISTEN 0      511        *:1420             *:*    users:((\"node\",pid=950652,fd=18))";
        let (port, name, pid) = parse_ss_listen(line).unwrap();
        assert_eq!(port, 1420);
        assert_eq!(name, "node");
        assert_eq!(pid, Some(950652));

        let line2 = "LISTEN 0      4096    127.0.0.53%lo:53        0.0.0.0:*  users:((\"systemd-resolve\",pid=700,fd=14))";
        let (port, ..) = parse_ss_listen(line2).unwrap();
        assert_eq!(port, 53);
    }

    #[test]
    fn names_derive_top_word_and_time() {
        let wins = vec![FrozenWindow {
            app: "code".into(),
            title: "STRAWBERRY - Visual Studio Code".into(),
            x: 0, y: 0, w: 800, h: 600, desktop: 1, active: true,
            launch: None,
        }];
        let n = name_for("2026-08-26T10:00:00Z", Some(&wins[0]), &wins);
        assert!(n.contains("Strawberry"), "{n}");
        assert!(n.contains('·'), "{n}");
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Real-system smoke: cargo test --lib live_freeze_smoke -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_freeze_smoke() {
        let space = collect();
        println!("=== NAME === {}\n=== STORY ===\n{}\n", space.name, space.story);
        println!("windows={} browsers={:?} terminals={:?} servers={:?}",
            space.windows.len(),
            space.browsers.iter().map(|b| format!("{}({})", b.browser, b.urls.len())).collect::<Vec<_>>(),
            space.terminals,
            space.dev_servers.iter().map(|s| format!(":{}({})", s.port, s.proc_name)).collect::<Vec<_>>());
        for w in space.windows.iter().take(6) {
            println!("  win {} @ {},{} {}x{} {}", w.app, w.x, w.y, w.w, w.h,
                w.launch.as_deref().unwrap_or("(special)"));
        }
    }
}
