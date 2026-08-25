//! 🍓 STRAWBERRY Capture Daemon — clipboard capture + AI-to-AI handoff.
//!
//! Cross-platform core (Rust): Windows/macOS via arboard,
//! Linux auto-detects Wayland (wl-clipboard-rs) vs X11 (arboard).
//!
//! Run: ./target/release/strawberry-daemon

mod clip;
mod db;
mod handoff;
mod semantic;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use strawberry_core::handoff as core_handoff;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // One-shot mode: `--save-once <kind> <text>` → DB insert + JSON, then exit.
    // Used for scripted verification of the Layer-1 pipeline.
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

    // Handoff on demand: `--handoff [budget]` compresses whatever is on the
    // clipboard right now and writes the packet back to the clipboard.
    //
    // On Wayland this stays in the foreground serving the selection (exactly
    // like `wl-copy` without `-f`), so it does not return until another app
    // replaces the clipboard. Press Ctrl+C once you have pasted.
    if args.len() >= 2 && args[1] == "--handoff" {
        let budget: usize = args
            .get(2)
            .and_then(|b| b.parse().ok())
            .unwrap_or(core_handoff::DEFAULT_TOKEN_BUDGET);
        let backend = clip::detect();
        let Some(text) = clip::read(backend) else {
            eprintln!("❌ Clipboard is empty or not text");
            std::process::exit(1);
        };
        if !handoff::is_compressible(&text) {
            eprintln!("❌ Clipboard text too short to compress");
            std::process::exit(1);
        }
        match compress_to_clipboard(backend, &text, budget, clip::Persist::BlockUntilReplaced) {
            Ok(report) => println!("{report}"),
            Err(e) => {
                eprintln!("❌ Handoff failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Print the packet to stdout instead of the clipboard (for piping/tests).
    if args.len() >= 2 && args[1] == "--handoff-stdout" {
        let budget: usize = args
            .get(2)
            .and_then(|b| b.parse().ok())
            .unwrap_or(core_handoff::DEFAULT_TOKEN_BUDGET);
        let mut text = String::new();
        use std::io::Read;
        if std::io::stdin().read_to_string(&mut text).is_err() || text.trim().is_empty() {
            eprintln!("❌ No text on stdin");
            std::process::exit(1);
        }
        let packet =
            core_handoff::build_from_raw(&handoff::title_for(&text), None, &text, budget);
        print!("{}", core_handoff::render(&packet));
        return;
    }

    println!("╔═══════════════════════════════════════════╗");
    println!("║  🍓 STRAWBERRY CAPTURE DAEMON — LIVE      ║");
    println!("║  Copy anything → popup → click to save    ║");
    println!("║  Copy chat, then copy  sb!  → handoff     ║");
    println!("╚═══════════════════════════════════════════╝");

    let backend = clip::detect();
    println!("🪟 Backend: {}", backend.label());
    println!(
        "🔑 Handoff trigger: copy your chat, then copy one of {:?}",
        handoff::TRIGGERS
    );

    // Last substantial clipboard text — the compression candidate.
    // Seeded from the current clipboard so a packet left over from a previous
    // session is never re-captured or treated as pending on startup.
    let mut pending: Option<String> = None;
    let mut last = clip::read(backend).unwrap_or_default();
    println!("📋 Clipboard on start: {} chars", last.chars().count());
    let poll = match backend {
        clip::Backend::Wayland => Duration::from_millis(500),
        clip::Backend::Native => Duration::from_millis(400),
    };

    loop {
        if let Some(text) = clip::read(backend) {
            if text != last {
                last = text.clone();

                if handoff::is_trigger(&text) {
                    match pending.clone() {
                        Some(source) => {
                            println!("🍓 Handoff trigger — compressing {} chars…", source.chars().count());
                            match compress_to_clipboard(
                                backend,
                                &source,
                                core_handoff::DEFAULT_TOKEN_BUDGET,
                                clip::Persist::WhileRunning,
                            ) {
                                Ok(report) => {
                                    println!("{report}");
                                    // The clipboard now holds the packet, not
                                    // the trigger; keep `last` in sync so the
                                    // next poll does not re-capture it.
                                    last = clip::read(backend).unwrap_or_default();
                                }
                                Err(e) => {
                                    eprintln!("❌ Handoff failed: {e}");
                                    notify("🍓 Handoff failed", &e);
                                }
                            }
                        }
                        None => {
                            println!("⚠️ Trigger seen but nothing copied yet");
                            notify(
                                "🍓 Nothing to compress",
                                "Copy the chat first, then copy the trigger again.",
                            );
                        }
                    }
                } else if text.trim().chars().count() >= 4 && text.len() <= 200_000 {
                    // Never re-capture our own handoff packets (e.g. the user
                    // pasted one back, or it resurfaced via a clipboard manager).
                    if handoff::is_trigger(&text) || text.contains("[STRAWBERRY HANDOFF v1") {
                        last = text;
                        continue;
                    }
                    if handoff::is_compressible(&text) {
                        pending = Some(text.clone());
                    }
                    // Capture popups stay limited to modest snippets; a whole
                    // pasted chat is a handoff candidate, not a capture.
                    if text.len() <= 20_000 {
                        handle_capture(text);
                    }
                }
            }
        }
        std::thread::sleep(poll);
    }
}

/// Compress `source`, put the packet on the clipboard, notify, and return a
/// one-line report for the console.
///
/// With [`clip::Persist::UntilPasted`] the clipboard write blocks on Wayland,
/// so the notification is raised *before* it: otherwise the user would see no
/// feedback until they had already pasted.
fn compress_to_clipboard(
    backend: clip::Backend,
    source: &str,
    budget: usize,
    persist: clip::Persist,
) -> Result<String, String> {
    let title = handoff::title_for(source);
    let packet = core_handoff::build_from_raw(&title, None, source, budget);
    let rendered = core_handoff::render(&packet);

    let b = &packet.budget;
    let summary = format!(
        "{} → {} tokens ({}% smaller) · {} rejected, {} ids kept",
        b.original_tokens,
        b.packet_tokens,
        b.reduction_pct,
        packet.rejected.len(),
        packet.identifiers.len()
    );
    notify("🍓 Handoff ready — paste it", &summary);

    clip::write(backend, &rendered, persist)?;
    Ok(format!("✅ Clipboard now holds the handoff packet: {summary}"))
}

fn notify(summary: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .summary(summary)
        .body(body)
        .timeout(notify_rust::Timeout::Milliseconds(5000))
        .show();
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
        "npm ", "pnpm ", "yarn ", "cargo ", "git ", "docker ", "pip ", "pip3 ", "python",
        "node ", "make ", "pacman ", "systemctl ", "sudo ",
    ];
    for c in CMDS {
        if low.starts_with(c) {
            return "command";
        }
    }
    const ERRS: &[&str] = &["error", "exception", "traceback", "failed", "panic:", "fatal"];
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
        let preview: String = text
            .chars()
            .take(80)
            .map(|c| if c == '\n' { ' ' } else { c })
            .collect();
        let body = format!("Add this {kind} to Strawberry?\n“{preview}…”");

        match notify_rust::Notification::new()
            .summary("🍓 Strawberry")
            .body(&body)
            .action("save", "🍓 Add to Strawberry")
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

    /// The trigger token must never be mistaken for a compression candidate,
    /// or copying it would overwrite the very text it is meant to compress.
    #[test]
    fn trigger_is_not_compressible() {
        for t in super::handoff::TRIGGERS {
            assert!(super::handoff::is_trigger(t));
            assert!(!super::handoff::is_compressible(t));
        }
    }
}
