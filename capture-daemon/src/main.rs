//! 🍓 STRAWBERRY Capture Daemon — REAL popup on every copy.
//! Cross-platform core (Rust): Windows/macOS via arboard,
//! Linux auto-detects Wayland (wl-clipboard-rs) vs X11 (arboard).
//!
//! Run: ./target/release/strawberry-daemon

mod db;
mod semantic;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() {
    // One-shot mode: `--save-once <kind> <text>` → DB insert + JSON, then exit.
    // Used for scripted verification of the Layer-1 pipeline.
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 4 && args[1] == "--save-once" {
        let kind = Box::leak(args[2].clone().into_boxed_str());
        match db::insert_capture(kind, &args[3], std::path::Path::new("/tmp/sb-once.txt")) {
            Ok(id) => {
                println!("INSERTED {id}");
                if let Err(e) = semantic::index_chat(&id, &args[3]) {
                    eprintln!("EMBED SKIPPED: {e}");
                } else {
                    println!("EMBEDDED {id}");
                }
                return;
            }
            Err(e) => {
                eprintln!("FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Semantic search mode: `--search "natural language query" [limit]`
    if args.len() >= 3 && args[1] == "--search" {
        let limit: usize = args.get(3).and_then(|l| l.parse().ok()).unwrap_or(5);
        match semantic::search(&args[2], limit) {
            Ok(results) => {
                println!("🔍 Semantic results for “{}”:", args[2]);
                for (title, snippet, score) in results {
                    println!("  [{score:.3}] {title}");
                    println!("          {snippet}…");
                }
            }
            Err(e) => {
                eprintln!("SEARCH FAILED: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    println!("╔═══════════════════════════════════════════╗");
    println!("║  🍓 STRAWBERRY CAPTURE DAEMON — LIVE      ║");
    println!("║  Copy anything → popup → click to save    ║");
    println!("╚═══════════════════════════════════════════╝");

    let on_wayland = std::env::var("WAYLAND_DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(false);

    let mut last = String::new();

    if on_wayland {
        println!("🪟 Backend: Wayland (wl-clipboard poll)");
        use std::io::Read;
        use wl_clipboard_rs::paste::ClipboardType;
        loop {
            let ok = wl_clipboard_rs::paste::get_contents(
                ClipboardType::Regular,
                wl_clipboard_rs::paste::Seat::Unspecified,
                wl_clipboard_rs::paste::MimeType::Specific("text/plain"),
            )
            .ok()
            .and_then(|(mut pipe, _)| {
                let mut buf = String::new();
                pipe.read_to_string(&mut buf).ok()?;
                Some(buf)
            });
            if let Some(text) = ok {
                if text != last && text.trim().chars().count() >= 4 && text.len() <= 20_000 {
                    last = text.clone();
                    handle_capture(text);
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    } else {
        println!("🪟 Backend: X11 / native OS (arboard)");
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("❌ Cannot access clipboard: {e}");
                std::process::exit(1);
            }
        };
        loop {
            if let Ok(text) = clipboard.get_text() {
                if text != last && text.trim().chars().count() >= 4 && text.len() <= 20_000 {
                    last = text.clone();
                    handle_capture(text);
                }
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    }
}

/// Classify → popup → save on action click.
fn handle_capture(text: String) {
    let kind = classify(&text);
    println!(
        "📋 Copied [{}] {} chars — showing popup…",
        kind,
        text.chars().count()
    );
    show_popup(kind, text);
}

/// Rule-based classification (instant, zero network).
fn classify(text: &str) -> &'static str {
    let t = text.trim();
    let low = text.to_lowercase();

    if t.starts_with('{') || t.starts_with('[') {
        if serde_json::from_str::<serde_json::Value>(t).is_ok() {
            return "json";
        }
    }
    if low.starts_with("http://") || low.starts_with("https://") {
        return "url";
    }
    const CMDS: &[&str] = &[
        "npm ", "pnpm ", "yarn ", "cargo ", "git ", "docker ", "pip ", "pip3 ",
        "python", "node ", "make ", "pacman ", "systemctl ", "sudo ",
    ];
    for c in CMDS {
        if low.starts_with(c) {
            return "command";
        }
    }
    const ERRS: &[&str] =
        &["error", "exception", "traceback", "failed", "panic:", "fatal"];
    if ERRS.iter().any(|k| low.contains(k)) {
        return "error";
    }
    if text.contains("fn ")
        || text.contains("def ")
        || text.contains("function ")
        || text.contains("class ")
        || text.contains("const ")
        || text.contains("```")
        || (text.contains(';') && text.contains('(') && text.contains(')'))
    {
        return "code";
    }
    "note"
}

/// Show the strawberry popup. Clicking the action saves the capture.
fn show_popup(kind: &'static str, text: String) {
    std::thread::spawn(move || {
        let preview: String = text.chars().take(80).map(|c| if c == '\n' { ' ' } else { c }).collect();
        let body = format!("Add this {kind} to Strawberry?\n“{preview}…”");

        match notify_rust::Notification::new()
            .summary("🍓 Strawberry")
            .body(&body)
            .actions(vec!["save".to_string(), "🍓 Add to Strawberry".to_string()])
            .timeout(notify_rust::Timeout::Milliseconds(9000))
            .show()
        {
            Ok(handle) => {
                handle.wait_for_action(|action| {
                    if action == "save" || action == "__default" {
                        match save_capture(kind, &text) {
                            Ok(path) => {
                                println!("✅ Saved → {}", path.display());
                                let _ = notify_rust::Notification::new()
                                    .summary("🍓 Strawberry")
                                    .body(&format!("Saved {kind} capture!\n{}", path.display()))
                                    .timeout(notify_rust::Timeout::Milliseconds(3000))
                                    .show();
                            }
                            Err(e) => eprintln!("❌ Save failed: {e}"),
                        }
                    } else {
                        println!("🙈 Ignored by user");
                    }
                });
            }
            Err(e) => {
                eprintln!("⚠️ Popup unavailable ({e}) — auto-saving instead");
                let _ = save_capture(kind, &text);
            }
        }
    });
}

/// Persist capture: raw text on disk + full insert into STRAWBERRY SQLite (FTS5).
fn save_capture(kind: &str, text: &str) -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = std::path::PathBuf::from(home)
        .join(".strawberry")
        .join("captures");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("{ts}_{kind}.json"));

    let keywords: Vec<String> = text
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .take(6)
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();

    let json = serde_json::json!({
        "type": kind,
        "keywords": keywords,
        "char_count": text.chars().count(),
        "captured_at": ts,
        "source": "clipboard-popup",
        "text": text,
    });

    std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap())
        .map_err(|e| e.to_string())?;

    // 🔴 PRIMARY storage: straight into the app's FTS5 database.
    match db::insert_capture(kind, text, &path) {
        Ok(chat_id) => {
            println!("🗄️  Indexed in Strawberry DB → {chat_id}");
            // 🧠 Layer 2: semantic index (local Ollama embeddings). Best-effort:
            // FTS5 already indexed the text; vectors add meaning-based search.
            match semantic::index_chat(&chat_id, text) {
                Ok(()) => println!("🧠 Semantic vector stored"),
                Err(e) => eprintln!("⚠️ Embedding skipped ({e})"),
            }
        }
        Err(e) => eprintln!("⚠️ DB index failed (JSON backup saved anyway): {e}"),
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn classify_all_types() {
        assert_eq!(super::classify("[{\"a\":1}]"), "json");
        assert_eq!(super::classify("https://example.com/docs"), "url");
        assert_eq!(super::classify("npm install left-pad"), "command");
        assert_eq!(super::classify("Error: cannot find module"), "error");
        assert_eq!(super::classify("fn main() { println!(\"hi\"); }"), "code");
        assert_eq!(super::classify("remember to buy strawberries"), "note");
    }
}
