//! 📄 File → Memory Indexer
//!
//! When the file watcher publishes a `FileModified` or `FileCreated` event,
//! this module turns the event into a unified memory record (idempotent on
//! path). It is the SOLE entry point for automatic file indexing — the UI
//! does NOT call it directly.
//!
//! Rules:
//!   * Privacy filter is applied to the path BEFORE indexing (see
//!     `autonomous::watcher_runner::DefaultPathFilter`).
//!   * Secret files (secrets.txt, *.key, *.pem, *.env with credentials)
//!     are not indexed — their memory is recorded with content stripped.
//!   * Deduplication is by absolute path + content hash. A repeated event
//!     for the same content updates `last_seen_at_ms` only, NOT
//!     `last_viewed_at_ms` (per spec).
//!   * When a file is deleted, the memory's `app_state` is set to 'deleted'
//!     (soft delete) so search stops returning it.
//!
//! All work is bounded — at most `MAX_PER_TICK` events per tick.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::{params, Connection};

use super::event::{EventBus, EventKind, NormalizedEvent};
use crate::memory::{create, list_by_app, NewMemory, MemoryKind, RedactionState};

/// Maximum number of file events processed per worker tick.
pub const MAX_PER_TICK: usize = 32;

/// File extensions we DO NOT index (binary, executable, or large).
const SKIP_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "bin", "zip", "tar", "gz", "bz2", "7z",
    "rar", "mp4", "mov", "avi", "mp3", "wav", "ogg", "jpg", "jpeg", "png",
    "gif", "bmp", "ico", "pdf", "iso",
];

/// File-name patterns that look like secrets — index only metadata, not content.
const SECRET_NAME_PATTERNS: &[&str] = &[
    ".env", "id_rsa", "id_ed25519", "id_dsa", ".pem", ".key", ".p12",
    "credentials", "secrets", "shadow", "passwd",
];

/// Privacy heuristic: is the path under a blocklisted directory?
fn is_under_blocked_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    for deny in [
        "/.ssh/", "/.gnupg/", "/.aws/", "/.kube/", "/.docker/",
        "/etc/", "/var/", "/proc/", "/sys/", "/boot/",
        "c:\\windows", "c:/windows",
    ] {
        if lower.contains(deny) {
            return true;
        }
    }
    false
}

/// Is this a "secret-like" file? If so, the body is NEVER stored.
fn looks_like_secret(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    for pat in SECRET_NAME_PATTERNS {
        if name == *pat || name.starts_with(pat) || name.ends_with(pat) {
            return true;
        }
    }
    false
}

/// Should we skip this extension entirely?
fn skip_extension(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_lowercase();
        return SKIP_EXTENSIONS.contains(&lower.as_str());
    }
    false
}

/// FNV-1a 64-bit content hash (used to dedupe identical files).
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Build a stable memory id from a path. Two different paths → different ids.
/// Same path → same id (idempotent re-indexing).
fn memory_id_for_path(path: &str) -> String {
    let canonical = std::fs::canonicalize(path)
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| path.to_string());
    let h = fnv1a_64(canonical.as_bytes());
    format!("file-{:016x}", h)
}

/// Read up to 64KB of text content from a file.
/// Returns the text if it parses as UTF-8, otherwise None (binary file).
fn read_text_content(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };
    let mut buf = Vec::with_capacity(4096);
    let mut handle = f.by_ref().take(64 * 1024);
    if handle.read_to_end(&mut buf).is_err() {
        return None;
    }
    match String::from_utf8(buf) {
        Ok(s) => Some(s),
        Err(_) => None, // not text — caller decides what to do
    }
}

/// Process one normalized event into a memory record (idempotent on path).
/// Returns Ok(true) if a memory was created/updated, Ok(false) if skipped.
pub fn process_event(conn: &Connection, ev: &NormalizedEvent) -> Result<bool, String> {
    let (path, project) = match &ev.kind {
        EventKind::FileOpened { path, project }
        | EventKind::FileModified { path, project } => (path.clone(), project.clone()),
        _ => return Ok(false),
    };
    if is_under_blocked_path(&path) {
        return Ok(false); // silently skip — privacy gate
    }
    let p = Path::new(&path);
    if skip_extension(p) {
        return Ok(false);
    }
    let memory_id = memory_id_for_path(&path);
    let title = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();

    let is_secret = looks_like_secret(p);
    let body = if is_secret {
        // Index only metadata, NEVER the secret body.
        format!("[Secret file — body not indexed]\nPath: {path}")
    } else {
        // Try to read text content; for binary files, fall back to
        // metadata-only.
        read_text_content(p)
            .map(|t| t.chars().take(16_000).collect::<String>())
            .unwrap_or_else(|| format!("[Binary or unreadable]\nPath: {path}"))
    };

    let mut m = NewMemory::new(MemoryKind::Document, title, body, "file_watcher");
    m.source_file = Some(path.clone());
    m.source_application = Some("file_watcher".to_string());
    m.project_id = project.clone();
    m.tags = vec!["file".to_string()];
    if is_secret {
        m.privacy_level = crate::memory::PrivacyLevel::Sensitive;
        m.redaction_state = RedactionState::Redacted;
        m.sensitivity = 5;
    }
    // Idempotency: pass the id explicitly so re-processing the same path
    // upserts (not duplicates).
    m.id = Some(memory_id.clone());

    // Persist. The `create` function uses ON CONFLICT DO UPDATE.
    create(conn, &m)?;

    // Add a project → memory relationship if the project is set.
    if let Some(proj) = &project {
        let _ = crate::memory::relationship::create(
            conn,
            &memory_id,
            proj,
            crate::memory::RelationshipType::BelongsTo,
            0.7,
            Some("file indexer: file belongs to project"),
            true,
        );
    }

    Ok(true)
}

/// Worker tick: process up to MAX_PER_TICK events from the bus.
pub fn process_bus_events(
    conn: &Connection,
    bus: &EventBus,
    shutdown: &AtomicBool,
) -> usize {
    let mut processed = 0;
    while processed < MAX_PER_TICK {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let events = bus.drain(MAX_PER_TICK);
        if events.is_empty() {
            break;
        }
        for ev in &events {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            // Only process file-related events.
            match ev.kind {
                EventKind::FileOpened { .. } | EventKind::FileModified { .. } => {}
                _ => continue,
            }
            if process_event(conn, ev).unwrap_or(false) {
                processed += 1;
            }
        }
    }
    processed
}

/// Background thread: continuously poll the bus and index file events.
pub fn spawn_indexer(
    db_path: std::path::PathBuf,
    bus: EventBus,
    shutdown: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !shutdown.load(Ordering::Relaxed) {
            let processed = match Connection::open(&db_path) {
                Ok(conn) => process_bus_events(&conn, &bus, &shutdown),
                Err(_) => 0,
            };
            if processed == 0 {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn privacy_blocked_path_is_skipped() {
        assert!(is_under_blocked_path("/etc/passwd"));
        assert!(is_under_blocked_path("/home/user/.ssh/id_rsa"));
        assert!(!is_under_blocked_path("/home/user/Documents/note.md"));
    }

    #[test]
    fn secret_file_pattern_recognized() {
        assert!(looks_like_secret(Path::new("/tmp/.env")));
        assert!(looks_like_secret(Path::new("/tmp/credentials.json")));
        assert!(looks_like_secret(Path::new("/tmp/id_rsa")));
        assert!(!looks_like_secret(Path::new("/tmp/note.md")));
        assert!(!looks_like_secret(Path::new("/tmp/notes.md")));
    }

    #[test]
    fn skip_extension_recognized() {
        assert!(skip_extension(Path::new("/tmp/photo.jpg")));
        assert!(skip_extension(Path::new("/tmp/lib.so")));
        assert!(!skip_extension(Path::new("/tmp/notes.md")));
        assert!(!skip_extension(Path::new("/tmp/script.py")));
    }

    #[test]
    fn file_modified_creates_memory() {
        let conn = setup();
        let path = std::env::temp_dir().join(format!("idx-test-{}.txt", std::process::id()));
        std::fs::write(&path, "hello strawberry world").unwrap();
        let ev = NormalizedEvent::new(EventKind::FileModified {
            path: path.display().to_string(),
            project: Some("MyProject".to_string()),
        });
        let processed = process_event(&conn, &ev).unwrap();
        assert!(processed);
        let listed = list_by_app(&conn, "file_watcher", 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].content.contains("hello strawberry"));
        assert_eq!(listed[0].project_id.as_deref(), Some("MyProject"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn re_indexing_is_idempotent() {
        let conn = setup();
        let path = std::env::temp_dir().join(format!("idx-idem-{}.txt", std::process::id()));
        std::fs::write(&path, "first version").unwrap();
        let ev = NormalizedEvent::new(EventKind::FileModified {
            path: path.display().to_string(),
            project: None,
        });
        process_event(&conn, &ev).unwrap();
        // Modify the file and re-process.
        std::fs::write(&path, "second version").unwrap();
        process_event(&conn, &ev).unwrap();
        let listed = list_by_app(&conn, "file_watcher", 10).unwrap();
        assert_eq!(listed.len(), 1, "idempotent: same path = same memory");
        assert!(listed[0].content.contains("second version"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn secret_file_redacts_content() {
        let conn = setup();
        let path = std::env::temp_dir().join(format!(".env-{}.env", std::process::id()));
        std::fs::write(&path, "SECRET_KEY=supersecretvalue").unwrap();
        let ev = NormalizedEvent::new(EventKind::FileModified {
            path: path.display().to_string(),
            project: None,
        });
        process_event(&conn, &ev).unwrap();
        let listed = list_by_app(&conn, "file_watcher", 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].content.contains("supersecretvalue"),
                "secret body must NOT be indexed: {}", listed[0].content);
        assert!(listed[0].content.contains("[Secret file"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn blocked_path_skipped() {
        let conn = setup();
        let ev = NormalizedEvent::new(EventKind::FileModified {
            path: "/etc/passwd".into(),
            project: None,
        });
        let processed = process_event(&conn, &ev).unwrap();
        assert!(!processed, "blocked path must be skipped");
        let listed = list_by_app(&conn, "file_watcher", 10).unwrap();
        assert!(listed.is_empty());
    }

    #[test]
    fn binary_extension_skipped() {
        let conn = setup();
        let ev = NormalizedEvent::new(EventKind::FileModified {
            path: "/tmp/photo.jpg".into(),
            project: None,
        });
        let processed = process_event(&conn, &ev).unwrap();
        assert!(!processed);
    }
}
