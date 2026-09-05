//! 🖋️ Generic OCR Pipeline
//!
//! Real local OCR is platform-specific (Tesseract on Linux, Apple Vision on
//! macOS, Windows OCR on Windows). This module provides a portable
//! abstraction that:
//!   * Queues images for OCR
//!   * Calls the best available local OCR engine
//!   * Records status (pending → queued → running → done/failed/unavailable)
//!   * Stores the extracted text in `image_assets.ocr_text` and the
//!     `image_ocr_fts` index for search
//!
//! When no local OCR engine is available, the pipeline marks images as
//! `ocr_status = 'unavailable'`. The image is still preserved and
//! searchable by metadata (filename, caption, project, tags).
//!
//! NEVER sends image data to a remote service without explicit user opt-in.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection};

use crate::memory::image::OcrStatus;

/// Result of an OCR attempt.
#[derive(Debug, Clone)]
pub struct OcrResult {
    pub status: OcrStatus,
    pub text: Option<String>,
    pub engine: String,
    pub error: Option<String>,
}

/// Detect which local OCR engine is available.
/// Returns Some(name) if a real engine is detected, None if not.
pub fn detect_local_engine() -> Option<String> {
    // Check for Tesseract on Linux/Mac.
    if which_exists("tesseract") {
        return Some("tesseract".to_string());
    }
    // Apple Vision: no CLI; we can't easily detect.
    // Windows OCR: requires WinRT API; not detectable from CLI.
    None
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run OCR on a single image file using the local engine.
/// Returns OcrResult. If no engine is available, returns Unavailable.
///
/// PRIVACY: caller is responsible for the privacy gate. This function
/// runs the OCR engine and returns raw extracted text — the indexer
/// should run `redact_ocr_text` before storing the result in FTS.
pub fn run_local_ocr(image_path: &Path) -> OcrResult {
    let engine = match detect_local_engine() {
        Some(e) => e,
        None => {
            return OcrResult {
                status: OcrStatus::Unavailable,
                text: None,
                engine: "none".to_string(),
                error: Some("no local OCR engine installed".into()),
            };
        }
    };
    if engine == "tesseract" {
        run_tesseract(image_path)
    } else {
        let e = engine.clone();
        OcrResult {
            status: OcrStatus::Unavailable,
            text: None,
            engine,
            error: Some(format!("engine {e} not yet implemented")),
        }
    }
}

/// Redact secret-like content from raw OCR text before it is indexed.
/// Looks for patterns that suggest credentials and replaces them with
/// [REDACTED] markers. This is a deterministic heuristic — false positives
/// are acceptable to prevent secrets leaking into search indices.
pub fn redact_ocr_text(text: &str) -> String {
    let mut out = text.to_string();
    // API keys (long hex / base64-looking tokens)
    let key_re = regex_lite_alnum(32, 256);
    out = key_re.replace_all(&out, "[REDACTED_KEY]").to_string();
    // Bearer tokens
    if let Some(idx) = out.to_lowercase().find("bearer ") {
        let after = &out[idx + 7..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .unwrap_or(after.len());
        let token = &after[..end];
        if token.len() >= 20 {
            out = out.replace(token, "[REDACTED_TOKEN]");
        }
    }
    // Password: / pass: / secret: lines
    for marker in ["password:", "passwd:", "secret:"] {
        if let Some(idx) = out.to_lowercase().find(marker) {
            // Find the value (until end of line)
            let after = &out[idx + marker.len()..];
            let end = after
                .find('\n')
                .unwrap_or(after.len());
            if end > 0 && end < 256 {
                let value = after[..end].trim();
                if !value.is_empty() {
                    out = out.replacen(value, "[REDACTED_VALUE]", 1);
                }
            }
        }
    }
    out
}

/// Minimal regex helper (no external crate). Matches runs of [a-zA-Z0-9+/=]
/// of at least `min` and at most `max` characters.
fn regex_lite_alnum(min: usize, max: usize) -> AlnumRe {
    AlnumRe { min, max }
}

struct AlnumRe {
    min: usize,
    max: usize,
}

impl AlnumRe {
    fn replace_all<'a>(&self, text: &'a str, repl: &str) -> std::borrow::Cow<'a, str> {
        // Walk the text, find runs of allowed chars.
        let allowed = |c: char| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=';
        let bytes = text.as_bytes();
        let mut result = String::with_capacity(text.len());
        let mut i = 0;
        let mut last_end = 0;
        while i < bytes.len() {
            if !allowed(bytes[i] as char) {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && allowed(bytes[i] as char) && (i - start) < self.max {
                i += 1;
            }
            if (i - start) >= self.min {
                if start > last_end {
                    result.push_str(&text[last_end..start]);
                }
                result.push_str(repl);
                last_end = i;
            }
        }
        if last_end < text.len() {
            result.push_str(&text[last_end..]);
        }
        std::borrow::Cow::Owned(result)
    }
}

fn run_tesseract(image_path: &Path) -> OcrResult {
    // tesseract <image> stdout -l eng
    let output = std::process::Command::new("tesseract")
        .arg(image_path)
        .arg("stdout")
        .arg("-l")
        .arg("eng")
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            OcrResult {
                status: OcrStatus::Done,
                text: Some(text),
                engine: "tesseract".to_string(),
                error: None,
            }
        }
        Ok(o) => OcrResult {
            status: OcrStatus::Failed,
            text: None,
            engine: "tesseract".to_string(),
            error: Some(String::from_utf8_lossy(&o.stderr).to_string()),
        },
        Err(e) => OcrResult {
            status: OcrStatus::Failed,
            text: None,
            engine: "tesseract".to_string(),
            error: Some(e.to_string()),
        },
    }
}

/// Bounded OCR worker: process up to N images per tick, then exit.
/// Caller (the scheduler) decides how often to call this.
pub fn process_queue(conn: &Connection, max_per_tick: usize, shutdown: &AtomicBool) -> usize {
    let mut processed = 0;
    while processed < max_per_tick {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        // Mark next image as 'running'.
        let next = conn.query_row(
            "SELECT id, original_path FROM image_assets
             WHERE ocr_status = 'pending' AND privacy_blocked = 0
             ORDER BY created_at_ms ASC LIMIT 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        );
        let (id, path) = match next {
            Ok(t) => t,
            Err(_) => break, // no more pending
        };
        // Move to 'running'.
        let _ = conn.execute(
            "UPDATE image_assets SET ocr_status='running' WHERE id=?1",
            params![id],
        );
        // Run OCR.
        let result = run_local_ocr(Path::new(&path));
        // Persist result.
        match result.status {
            OcrStatus::Done => {
                if let Some(text) = &result.text {
                    // PRIVACY: redact secret-like content before storage + FTS.
                    let safe_text = redact_ocr_text(text);
                    let _ = conn.execute(
                        "UPDATE image_assets
                         SET ocr_text = ?2, ocr_status = 'done',
                             ocr_completed_at_ms = ?3, updated_at_ms = ?3
                         WHERE id = ?1",
                        params![id, safe_text, chrono::Utc::now().timestamp_millis()],
                    );
                    let _ = conn.execute(
                        "INSERT INTO image_ocr_fts(image_id, ocr_text) VALUES(?1, ?2)",
                        params![id, safe_text],
                    );
                }
            }
            OcrStatus::Failed => {
                let _ = conn.execute(
                    "UPDATE image_assets
                     SET ocr_status = 'failed', ocr_completed_at_ms = ?2
                     WHERE id = ?1",
                    params![id, chrono::Utc::now().timestamp_millis()],
                );
            }
            OcrStatus::Unavailable => {
                let _ = conn.execute(
                    "UPDATE image_assets SET ocr_status = 'unavailable' WHERE id = ?1",
                    params![id],
                );
            }
            _ => {}
        }
        processed += 1;
    }
    processed
}

/// Move the OCR status of an image to 'running'.
pub fn set_running(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE image_assets SET ocr_status='running', updated_at_ms=?2 WHERE id=?1",
        params![id, chrono::Utc::now().timestamp_millis()],
    )
    .map_err(|e| format!("ocr set_running: {e}"))?;
    Ok(())
}

/// Mark OCR as done with a (redacted) text.
pub fn set_done(conn: &Connection, id: &str, text: &str) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "UPDATE image_assets
         SET ocr_text = ?2, ocr_status = 'done',
             ocr_completed_at_ms = ?3, updated_at_ms = ?3
         WHERE id = ?1",
        params![id, text, now],
    )
    .map_err(|e| format!("ocr set_done: {e}"))?;
    let _ = conn.execute(
        "INSERT INTO image_ocr_fts(image_id, ocr_text) VALUES(?1, ?2)",
        params![id, text],
    );
    Ok(())
}

/// Mark OCR as failed.
pub fn set_failed(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE image_assets
         SET ocr_status = 'failed', ocr_completed_at_ms = ?2, updated_at_ms = ?2
         WHERE id = ?1",
        params![id, chrono::Utc::now().timestamp_millis()],
    )
    .map_err(|e| format!("ocr set_failed: {e}"))?;
    Ok(())
}

/// Mark OCR as unavailable (no local engine).
pub fn set_unavailable(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE image_assets
         SET ocr_status = 'unavailable', updated_at_ms = ?2
         WHERE id = ?1",
        params![id, chrono::Utc::now().timestamp_millis()],
    )
    .map_err(|e| format!("ocr set_unavailable: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_local_engine_does_not_panic() {
        let _ = detect_local_engine();
    }

    #[test]
    fn run_local_ocr_returns_structured_result() {
        // We don't assert on engine availability, but the call must not panic.
        let result = run_local_ocr(Path::new("/nonexistent/image.png"));
        // Either Unavailable (no engine) or Failed (engine present, missing file)
        // is acceptable. The important thing is it returns a result.
        assert!(matches!(result.status, OcrStatus::Unavailable | OcrStatus::Failed));
    }

    #[test]
    fn process_queue_no_pending() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        let shutdown = AtomicBool::new(false);
        let n = process_queue(&conn, 10, &shutdown);
        assert_eq!(n, 0);
    }

    #[test]
    fn redact_ocr_text_replaces_long_alnum_runs() {
        let text = "Header\nkey=abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234\nBody";
        let red = redact_ocr_text(text);
        assert!(red.contains("[REDACTED_KEY]"));
        assert!(!red.contains("abcd1234abcd1234abcd1234"));
    }

    #[test]
    fn redact_ocr_text_handles_bearer() {
        let text = "Authorization: Bearer abcdefghijklmnopqrstuvwxyz1234567890";
        let red = redact_ocr_text(text);
        // The token is redacted (either as KEY or TOKEN; both are safe).
        assert!(!red.contains("abcdefghijklmnopqrstuvwxyz1234567890"),
                "raw bearer token must not survive: {red}");
        assert!(red.contains("REDACTED"));
    }

    #[test]
    fn redact_ocr_text_handles_password_lines() {
        let text = "user: admin\npassword: hunter2\nrest of content";
        let red = redact_ocr_text(text);
        assert!(red.contains("[REDACTED_VALUE]"));
        assert!(!red.contains("hunter2"));
    }

    #[test]
    fn redact_ocr_text_leaves_normal_text() {
        let text = "This is a normal document about MyProject with notes.";
        let red = redact_ocr_text(text);
        assert_eq!(red, text);
    }
}
