//! 🧠 Context Recall — one-click workspace snapshots.
//!
//! `collect()` grabs, fully locally:
//!   • every open window (app class + title + focused flag) via KWin scripting,
//!   • live Firefox tabs from the session store (mozLz4),
//!   • recent Chrome pages by copying the locked History db and reading it,
//!   • the current clipboard as a hint.
//!
//! `generate_story()` turns that into a deterministic Hinglish narrative —
//! kaha pe the, kya khula tha, aur kaunse purane notes judte hain — so
//! "load previous work" can answer without any LLM.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error;

const MAX_WINDOWS: usize = 60;
const MAX_TABS_PER_BROWSER: usize = 30;
const MAX_RELATED: usize = 5;
const KWIN_MARKER: &str = "SBWIN|";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub app: String,
    pub title: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserContext {
    /// "firefox" | "chrome"
    pub browser: String,
    /// "tabs" (live session) | "history" (recent pages)
    pub kind: String,
    pub items: Vec<TabInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedNote {
    pub chat_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkSnapshot {
    pub id: String,
    pub created_at: String,
    pub windows: Vec<WindowInfo>,
    pub browsers: Vec<BrowserContext>,
    pub clipboard_hint: Option<String>,
    pub related_notes: Vec<RelatedNote>,
    pub story: String,
}

// ---------------------------------------------------------------------------
// Collectors
// ---------------------------------------------------------------------------

/// Snapshot of everything we can see right now. Each collector degrades
/// gracefully — a missing browser or headless session just yields less data.
/// `conn` links the snapshot back to the user's own notes.
pub fn collect(conn: &rusqlite::Connection) -> WorkSnapshot {
    let windows = kwin_windows().unwrap_or_default();
    let active = windows.iter().find(|w| w.active).cloned();

    let mut browsers = Vec::new();
    if let Some(tabs) = firefox_tabs() {
        if !tabs.is_empty() {
            browsers.push(BrowserContext {
                browser: "firefox".into(),
                kind: "tabs".into(),
                items: tabs,
            });
        }
    }
    if let Some(items) = chrome_recent_pages() {
        if !items.is_empty() {
            browsers.push(BrowserContext {
                browser: "chrome".into(),
                kind: "history".into(),
                items,
            });
        }
    }

    let clipboard_hint = clipboard_head();
    let created_at = db::now_iso();
    let related_notes = related_notes(conn, &windows, &browsers);
    let story = generate_story(&created_at, active.as_ref(), &windows, &browsers);

    WorkSnapshot {
        id: db::new_uuid(),
        created_at,
        windows,
        browsers,
        clipboard_hint,
        related_notes,
        story,
    }
}

/// All normal windows via a throwaway KWin script whose `console.info`
/// lines we read back from the user journal.
fn kwin_windows() -> Result<Vec<WindowInfo>, String> {
    let script_name = format!("sb-snapshot-{}", db::new_uuid());
    let path = std::env::temp_dir().join(format!("{script_name}.js"));
    std::fs::write(
        &path,
        format!(
            "for (const w of workspace.windowList()) {{\n\
             \x20 if (!w.normalWindow) continue;\n\
             \x20 const t = (w.caption || '').split('|').join('/');\n\
             \x20 const act = (workspace.activeWindow === w) ? 1 : 0;\n\
             \x20 console.info('{KWIN_MARKER}' + w.resourceClass + '|' + t + '|' + act);\n\
             }}"
        ),
    )
    .map_err(error::to_string_err("kwin script write"))?;

    let qdbus = |args: &[&str]| -> Result<String, String> {
        let out = Command::new("qdbus6")
            .arg("org.kde.KWin")
            .arg("/Scripting")
            .args(args)
            .output()
            .map_err(error::to_string_err("qdbus6 spawn"))?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };

    let run = (|| -> Result<Vec<WindowInfo>, String> {
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
            .map_err(error::to_string_err("script run"))?;
        thread::sleep(Duration::from_millis(900));

        let journal = Command::new("journalctl")
            .args(["--user", "-n", "160", "--output=cat"])
            .output()
            .map_err(error::to_string_err("journalctl spawn"))?;
        let text = String::from_utf8_lossy(&journal.stdout);
        Ok(parse_window_lines(&text))
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

/// Parse `SBWIN|class|title|flag` lines (unit-tested).
fn parse_window_lines(journal: &str) -> Vec<WindowInfo> {
    let mut windows: Vec<WindowInfo> = Vec::new();
    for line in journal.lines() {
        let Some(rest) = line.trim().strip_prefix(KWIN_MARKER) else {
            continue;
        };
        let mut parts = rest.splitn(3, '|');
        let app = parts.next().unwrap_or("").trim().to_string();
        let title = parts.next().unwrap_or("").trim().to_string();
        let flag = parts.next().unwrap_or("0").trim() == "1";
        if app.is_empty() || app == "undefined" {
            continue;
        }
        if windows.iter().any(|w| w.app == app && w.title == title) {
            continue;
        }
        windows.push(WindowInfo { app, title, active: flag });
        if windows.len() >= MAX_WINDOWS {
            break;
        }
    }
    // Exactly one focused window: first flagged wins, rest demoted.
    let mut seen_active = false;
    for w in &mut windows {
        if w.active {
            if seen_active {
                w.active = false;
            } else {
                seen_active = true;
            }
        }
    }
    windows
}

/// Live Firefox tabs from the newest sessionstore backup.
fn firefox_tabs() -> Option<Vec<TabInfo>> {
    let base = home_dir()?.join(".mozilla/firefox");
    let mut best: Option<PathBuf> = None;
    let mut best_mtime = std::time::SystemTime::UNIX_EPOCH;
    for profile in std::fs::read_dir(base).ok()? {
        let p = profile.ok()?.path().join("sessionstore-backups/recovery.jsonlz4");
        let meta = std::fs::metadata(&p).ok()?;
        if let Ok(m) = meta.modified() {
            if m > best_mtime {
                best_mtime = m;
                best = Some(p);
            }
        }
    }
    let bytes = std::fs::read(best?).ok()?;
    let tabs = parse_moz_lz4_session(&bytes)?;
    Some(tabs.into_iter().take(MAX_TABS_PER_BROWSER).collect())
}

/// Decode `mozLz40\0` + u32 LE size + lz4 block → JSON → current tab entries.
fn parse_moz_lz4_session(bytes: &[u8]) -> Option<Vec<TabInfo>> {
    if bytes.len() < 12 || &bytes[..8] != b"mozLz40\0" {
        return None;
    }
    let size = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let json = lz4_flex::block::decompress(&bytes[12..], size).ok()?;
    let v: serde_json::Value = serde_json::from_str(std::str::from_utf8(&json).ok()?).ok()?;
    let mut tabs = Vec::new();
    for win in v.get("windows")?.as_array()? {
        for tab in win.get("tabs").and_then(|t| t.as_array()).unwrap_or(&vec![]) {
            let entries = tab.get("entries").and_then(|e| e.as_array())?;
            let Some(cur) = entries.last() else { continue };
            let url = cur.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let title = cur.get("title").and_then(|t| t.as_str()).unwrap_or(url);
            if !url.starts_with("http") && !url.starts_with("file") {
                continue;
            }
            tabs.push(TabInfo {
                title: title.chars().take(120).collect(),
                url: url.chars().take(300).collect(),
            });
        }
    }
    Some(tabs)
}

/// Recent Chrome pages: copy the locked History db, read urls table.
fn chrome_recent_pages() -> Option<Vec<TabInfo>> {
    let hist = home_dir()?.join(".config/google-chrome/Default/History");
    if !hist.exists() {
        return None;
    }
    let tmp = std::env::temp_dir().join(format!("sb-chrome-{}.db", db::new_uuid()));
    std::fs::copy(&hist, &tmp).ok()?;
    let conn = rusqlite::Connection::open_with_flags(
        &tmp,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    let cutoff = chrome_now_micros() - 12 * 3600 * 1_000_000;
    let mut stmt = conn
        .prepare(
            "SELECT url, title FROM urls
             WHERE last_visit_time > ?1 AND hidden = 0
             ORDER BY last_visit_time DESC LIMIT 25",
        )
        .ok()?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![cutoff], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    drop(conn);
    let _ = std::fs::remove_file(&tmp);
    Some(
        rows.into_iter()
            .map(|(url, title)| TabInfo {
                title: if title.is_empty() {
                    url.chars().take(80).collect()
                } else {
                    title.chars().take(120).collect()
                },
                url: url.chars().take(300).collect(),
            })
            .collect(),
    )
}

/// Chrome epoch is microseconds since 1601-01-01.
fn chrome_now_micros() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    unix + 11_644_473_600_000_000
}

fn clipboard_head() -> Option<String> {
    for (bin, args) in [
        ("wl-paste", vec!["--no-newline"]),
        ("xclip", vec!["-selection", "clipboard", "-o"]),
    ] {
        if let Ok(out) = Command::new(bin).args(args).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                let s: String = s.chars().filter(|c| !c.is_control()).take(140).collect();
                if !s.trim().is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Story generation + related notes
// ---------------------------------------------------------------------------

/// Deterministic narrative: kaha pe the, kya chal raha tha, kitna khula tha.
pub fn generate_story(
    created_at: &str,
    active: Option<&WindowInfo>,
    windows: &[WindowInfo],
    browsers: &[BrowserContext],
) -> String {
    // Display time must be the user's wall clock, not the stored UTC ISO.
    let time = chrono::Local::now()
        .format("%H:%M")
        .to_string()
        + " baje";

    let mut apps: BTreeMap<String, usize> = BTreeMap::new();
    for w in windows {
        *apps.entry(w.app.clone()).or_default() += 1;
    }
    let app_list: Vec<String> = apps.keys().cloned().collect();

    let where_part = match active {
        Some(a) => format!(
            "{time} tum {} me the — \"{}\"",
            a.app,
            a.title.chars().take(90).collect::<String>()
        ),
        None => format!("{time} {n} apps khule the", n = windows.len()),
    };

    let mut parts = vec![where_part];

    if app_list.len() > 1 || (active.is_some() && windows.len() > 1) {
        parts.push(format!(
            "kul {} apps open the: {}",
            windows.len(),
            app_list.join(", ")
        ));
    } else if !app_list.is_empty() {
        parts.push(format!("sirf {} focus me tha", app_list.join(", ")));
    }

    for b in browsers {
        match b.kind.as_str() {
            "tabs" => {
                let names: Vec<String> = b
                    .items
                    .iter()
                    .take(3)
                    .map(|t| format!("\"{}\"", t.title.chars().take(48).collect::<String>()))
                    .collect();
                parts.push(format!(
                    "{} ke {} live tabs the{}",
                    capitalize(&b.browser),
                    b.items.len(),
                    if names.is_empty() {
                        String::new()
                    } else {
                        format!(" — jaise {}", names.join(", "))
                    }
                ));
            }
            "history" => {
                let domains = top_domains(&b.items, 3);
                parts.push(format!(
                    "{} me pichhle ghanton me {} pages khule the ({})",
                    capitalize(&b.browser),
                    b.items.len(),
                    domains.join(", ")
                ));
            }
            _ => {}
        }
    }

    if !parts.iter().any(|p| p.contains("apps")) && app_list.is_empty() {
        return format!("{time}: kuch bhi open nahi mila — shayad headless session tha.");
    }
    parts.join(". ") + "."
}

fn top_domains(items: &[TabInfo], n: usize) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for t in items {
        if let Some(host) = t
            .url
            .strip_prefix("https://")
            .or_else(|| t.url.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next())
        {
            *counts
                .entry(host.trim_start_matches("www.").to_string())
                .or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.into_iter().take(n).map(|(d, _)| d).collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// WHY-links: user ke apne chats jinke titles/briefs snapshot ke tokens se
/// milte hain (distinctive words ≥5 chars from window/tab titles).
fn related_notes(
    conn: &rusqlite::Connection,
    windows: &[WindowInfo],
    browsers: &[BrowserContext],
) -> Vec<RelatedNote> {
    let mut corpus: Vec<String> = windows.iter().map(|w| w.title.to_lowercase()).collect();
    for b in browsers {
        for t in &b.items {
            corpus.push(t.title.to_lowercase());
        }
    }
    let mut tokens: Vec<String> = Vec::new();
    for line in &corpus {
        for word in line.split(|c: char| !c.is_alphanumeric()) {
            if word.len() >= 5 && !tokens.contains(&word.to_string()) {
                tokens.push(word.to_string());
            }
            if tokens.len() >= 24 {
                break;
            }
        }
    }
    related_from_db(conn, &tokens)
}

fn related_from_db(conn: &rusqlite::Connection, tokens: &[String]) -> Vec<RelatedNote> {
    let mut out: Vec<RelatedNote> = Vec::new();
    for tok in tokens {
        let Ok(mut stmt) = conn.prepare(
            "SELECT c.id, c.title FROM chats c
             JOIN nodes n ON n.id = c.node_id
             WHERE n.name NOT LIKE 'root-captures%'
               AND (lower(c.title) LIKE '%'||?1||'%'
                    OR lower(coalesce(c.brief_text,'')) LIKE '%'||?1||'%')
             ORDER BY c.updated_at DESC LIMIT 2",
        ) else {
            continue;
        };
        let pattern = format!("%{tok}%");
        let rows = stmt.query_map([&pattern], |r| {
            Ok(RelatedNote {
                chat_id: r.get(0)?,
                title: r.get(1)?,
            })
        });
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                if !out.iter().any(|n| n.chat_id == row.chat_id) {
                    out.push(row);
                }
                if out.len() >= MAX_RELATED {
                    return out;
                }
            }
        }
    }
    out
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn save(conn: &rusqlite::Connection, snap: &WorkSnapshot) -> Result<(), String> {
    conn.execute(
        "INSERT INTO work_snapshots
            (id, created_at, active_app, active_title, story_text, raw_json)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            snap.id,
            snap.created_at,
            snap.windows.iter().find(|w| w.active).map(|w| w.app.clone()),
            snap.windows.iter().find(|w| w.active).map(|w| w.title.clone()),
            snap.story,
            serde_json::to_string(snap).unwrap_or_default(),
        ],
    )
    .map_err(error::to_string_err("snapshot insert"))?;
    Ok(())
}

pub fn latest(conn: &rusqlite::Connection) -> Result<Option<WorkSnapshot>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT raw_json FROM work_snapshots ORDER BY created_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(error::to_string_err("snapshot latest"))?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn list_recent(
    conn: &rusqlite::Connection,
    limit: usize,
) -> Result<Vec<(String, String, Option<String>)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, created_at, active_app FROM work_snapshots
             ORDER BY created_at DESC LIMIT ?1",
        )
        .map_err(error::to_string_err("snapshot list"))?;
    let rows = stmt
        .query_map(params![limit.clamp(1, 100) as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        })
        .map_err(error::to_string_err("snapshot list map"))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kwin_lines_and_marks_single_active() {
        // journalctl --output=cat emits only the message itself.
        let journal = "\
SBWIN|code|ARE YOU THERE #2 - STRAWBERRY|0
SBWIN|google-chrome|ComfyUI workflow|1
SBWIN|chat-memory-tree|🍓 Strawberry — Second Brain|0
SBWIN|plasmashell||0";
        let wins = parse_window_lines(journal);
        assert_eq!(wins.len(), 4);
        assert_eq!(wins[0].app, "code");
        assert!(wins[1].active);
        assert!(!wins.iter().skip(1).skip(1).any(|w| w.active));
        assert_eq!(wins[2].title, "🍓 Strawberry — Second Brain");
    }

    #[test]
    fn story_mentions_active_app_time_and_tabs() {
        let wins = vec![
            WindowInfo { app: "code".into(), title: "STRAWBERRY - VS Code".into(), active: true },
            WindowInfo { app: "google-chrome".into(), title: "ComfyUI".into(), active: false },
        ];
        let browsers = vec![BrowserContext {
            browser: "firefox".into(),
            kind: "tabs".into(),
            items: vec![
                TabInfo { title: "Rust Book".into(), url: "https://doc.rust-lang.org".into() },
                TabInfo { title: "MDN".into(), url: "https://developer.mozilla.org".into() },
            ],
        }];
        let story = generate_story("2026-08-26T14:22:00Z", Some(&wins[0]), &wins, &browsers);
        assert!(story.contains("code"), "{story}");
        // Wall-clock time (HH:MM baje), not the stored UTC slice.
        assert!(story.contains("baje"), "{story}");
        assert!(!story.contains("kul 2 apps open the: code, google-chrome,") || story.contains("2 live tabs"), "{story}");
        assert!(story.contains("2 live tabs"), "{story}");
        assert!(story.contains("Rust Book"), "{story}");
    }

    #[test]
    fn moz_lz4_roundtrip_parses_tabs() {
        let session = serde_json::json!({
            "windows": [{
                "tabs": [
                    {"entries": [
                        {"url": "https://old.example.com", "title": "old"},
                        {"url": "https://doc.rust-lang.org/book/", "title": "The Rust Book"}
                    ]},
                    {"entries": [{"url": "about:blank", "title": ""}]},
                    {"entries": [{"url": "file:///mnt/notes/todo.txt", "title": "todo.txt"}]}
                ]
            }]
        });
        let plain = serde_json::to_string(&session).unwrap();
        let compressed = lz4_flex::block::compress(plain.as_bytes());
        let mut bytes = b"mozLz40\0".to_vec();
        bytes.extend_from_slice(&(plain.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&compressed);

        let tabs = parse_moz_lz4_session(&bytes).unwrap();
        assert_eq!(tabs.len(), 2); // about: skipped
        assert_eq!(tabs[0].title, "The Rust Book"); // last entry per tab wins
        assert!(tabs[1].url.starts_with("file://"));
    }

    #[test]
    fn chrome_epoch_conversion_is_sane() {
        let now_chrome = chrome_now_micros();
        let now_unix_secs = now_chrome / 1_000_000 - 11_644_473_600;
        use std::time::{SystemTime, UNIX_EPOCH};
        let real = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        assert!((now_unix_secs - real).abs() < 5);
    }

    #[test]
    fn top_domains_dedupes_www_and_ranks() {
        let items = vec![
            TabInfo { title: "a".into(), url: "https://docs.rs/x".into() },
            TabInfo { title: "b".into(), url: "https://www.docs.rs/y".into() },
            TabInfo { title: "c".into(), url: "https://github.com/rust-lang/rust".into() },
        ];
        let domains = top_domains(&items, 3);
        assert_eq!(domains[0], "docs.rs");
        assert!(domains.contains(&"github.com".to_string()));
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Real-system smoke run: cargo test --lib live_collect_smoke -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_collect_smoke() {
        let dir = std::env::temp_dir().join(format!("sb-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = crate::state::AppState::init(dir).unwrap();
        let snap = {
            let conn = st.conn.lock().unwrap();
            collect(&conn)
        };
        println!("=== STORY ===\n{}\n", snap.story);
        println!("windows: {} | browsers: {} | notes: {} | clip: {:?}",
            snap.windows.len(),
            snap.browsers.iter().map(|b| format!("{}({})", b.browser, b.items.len())).collect::<Vec<_>>().join(","),
            snap.related_notes.len(),
            snap.clipboard_hint.as_deref().unwrap_or("-"));
    }
}
