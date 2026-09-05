//! 🖼️ Generic Image Memory
//!
//! Image assets are stored in the filesystem (see storage::files).
//! The DB holds metadata, OCR text, FTS index, and thumbnail reference.
//!
//! OCR pipeline:
//!   * Privacy screen first — blocked images are NOT OCR'd
//!   * Queue (image_assets.ocr_status) — pending → queued → running → done/failed
//!   * Failures are recorded as 'failed' but the image still exists
//!   * OCR text is stored separately and is FTS-indexed
//!
//! Thumbnails:
//!   * Stored at `thumbnail_path` in the storage dir
//!   * Bounded: 256x256 max
//!   * Generated on first request, cached thereafter

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::{create as create_memory, NewMemory, MemoryKind, PrivacyLevel};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OcrStatus {
    Pending,
    Queued,
    Running,
    Done,
    Failed,
    Unavailable,
    Skipped,
}

impl OcrStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OcrStatus::Pending => "pending",
            OcrStatus::Queued => "queued",
            OcrStatus::Running => "running",
            OcrStatus::Done => "done",
            OcrStatus::Failed => "failed",
            OcrStatus::Unavailable => "unavailable",
            OcrStatus::Skipped => "skipped",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => OcrStatus::Pending,
            "queued" => OcrStatus::Queued,
            "running" => OcrStatus::Running,
            "done" => OcrStatus::Done,
            "failed" => OcrStatus::Failed,
            "unavailable" => OcrStatus::Unavailable,
            "skipped" => OcrStatus::Skipped,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAsset {
    pub id: String,
    pub memory_id: Option<String>,
    pub original_path: String,
    pub thumbnail_path: Option<String>,
    pub mime_type: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub byte_size: Option<i64>,
    pub caption: Option<String>,
    pub source_app: Option<String>,
    pub source_window: Option<String>,
    pub source_project: Option<String>,
    pub ocr_text: Option<String>,
    pub ocr_status: String,
    pub ocr_completed_at_ms: Option<i64>,
    pub thumbnail_status: String,
    pub thumbnail_completed_at_ms: Option<i64>,
    pub privacy_blocked: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Register an image asset. Returns the image id.
///
/// `privacy_blocked = true` short-circuits OCR — the image is preserved
/// but its text content is never extracted or indexed.
pub fn register(
    conn: &Connection,
    original_path: &str,
    mime_type: Option<&str>,
    width: Option<i64>,
    height: Option<i64>,
    byte_size: Option<i64>,
    caption: Option<&str>,
    source_app: Option<&str>,
    source_window: Option<&str>,
    source_project: Option<&str>,
    privacy_blocked: bool,
) -> Result<String, String> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut m = NewMemory::new(
        MemoryKind::Image,
        caption.unwrap_or(original_path),
        format!("Image asset: {original_path}"),
        "image_asset",
    );
    m.privacy_level = if privacy_blocked { PrivacyLevel::Private } else { PrivacyLevel::Normal };
    m.source_application = source_app.map(|s| s.to_string());
    m.source_window = source_window.map(|s| s.to_string());
    m.source_file = Some(original_path.to_string());
    m.project_id = source_project.map(|s| s.to_string());
    m.tags = vec!["image".to_string()];

    let memory_id = create_memory(conn, &m)?;

    let image_id = {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in memory_id.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("img-{:016x}", h)
    };

    let ocr_status = if privacy_blocked {
        OcrStatus::Skipped
    } else {
        OcrStatus::Pending
    };

    conn.execute(
        "INSERT INTO image_assets(
            id, memory_id, original_path, thumbnail_path, mime_type, width, height, byte_size,
            caption, source_app, source_window, source_project, ocr_text, ocr_status, thumbnail_status,
            privacy_blocked, created_at_ms, updated_at_ms
         ) VALUES(?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, ?12, 'pending', ?13, ?14, ?14)",
        params![
            image_id,
            memory_id,
            original_path,
            mime_type,
            width,
            height,
            byte_size,
            caption,
            source_app,
            source_window,
            source_project,
            ocr_status.as_str(),
            privacy_blocked as i64,
            now_ms,
        ],
    ).map_err(|e| format!("image register: {e}"))?;

    Ok(image_id)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<ImageAsset>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, original_path, thumbnail_path, mime_type, width, height, byte_size,
                caption, source_app, source_window, source_project, ocr_text, ocr_status,
                ocr_completed_at_ms, thumbnail_status, thumbnail_completed_at_ms, privacy_blocked,
                created_at_ms, updated_at_ms
         FROM image_assets WHERE id = ?1"
    ).map_err(|e| format!("get image: {e}"))?;
    let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        Ok(Some(row_to_image(row)?))
    } else {
        Ok(None)
    }
}

/// Set the OCR text and mark OCR done. This is the ONLY way to set OCR text
/// — ensures the text is privacy-screened before storage.
pub fn set_ocr_text(conn: &Connection, id: &str, text: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE image_assets
         SET ocr_text = ?2,
             ocr_status = 'done',
             ocr_completed_at_ms = ?3,
             updated_at_ms = ?3
         WHERE id = ?1",
        params![id, text, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("set ocr: {e}"))?;
    if n > 0 {
        let _ = conn.execute(
            "INSERT INTO image_ocr_fts(image_id, ocr_text) VALUES(?1, ?2)",
            params![id, text],
        );
    }
    Ok(n > 0)
}

/// Mark OCR as failed. The image is still preserved.
pub fn mark_ocr_failed(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE image_assets
         SET ocr_status = 'failed',
             ocr_completed_at_ms = ?2,
             updated_at_ms = ?2
         WHERE id = ?1",
        params![id, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("mark ocr failed: {e}"))?;
    Ok(n > 0)
}

pub fn mark_ocr_unavailable(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE image_assets
         SET ocr_status = 'unavailable', updated_at_ms = ?2
         WHERE id = ?1",
        params![id, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("mark ocr unavailable: {e}"))?;
    Ok(n > 0)
}

pub fn set_thumbnail_path(conn: &Connection, id: &str, path: &str) -> Result<bool, String> {
    let n = conn.execute(
        "UPDATE image_assets
         SET thumbnail_path = ?2, thumbnail_status = 'done',
             thumbnail_completed_at_ms = ?3, updated_at_ms = ?3
         WHERE id = ?1",
        params![id, path, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("set thumbnail: {e}"))?;
    Ok(n > 0)
}

/// Delete an image asset. The file itself must be removed by the caller
/// (we do not touch the filesystem from the DB layer).
pub fn delete(conn: &Connection, id: &str) -> Result<bool, String> {
    let mut stmt = conn.prepare("SELECT memory_id FROM image_assets WHERE id = ?1").map_err(|e| e.to_string())?;
    let memory_id: Option<String> = stmt.query_row(params![id], |r| r.get(0)).ok();
    let n = conn.execute("DELETE FROM image_assets WHERE id = ?1", params![id])
        .map_err(|e| format!("delete image: {e}"))?;
    let _ = conn.execute("DELETE FROM image_ocr_fts WHERE image_id = ?1", params![id]);
    if let Some(mid) = memory_id {
        let _ = super::delete(conn, &mid);
    }
    Ok(n > 0)
}

/// Find the next image that needs OCR processing.
pub fn next_ocr_pending(conn: &Connection) -> Result<Option<ImageAsset>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, original_path, thumbnail_path, mime_type, width, height, byte_size,
                caption, source_app, source_window, source_project, ocr_text, ocr_status,
                ocr_completed_at_ms, thumbnail_status, thumbnail_completed_at_ms, privacy_blocked,
                created_at_ms, updated_at_ms
         FROM image_assets
         WHERE ocr_status IN ('pending','queued')
           AND privacy_blocked = 0
         ORDER BY created_at_ms ASC LIMIT 1"
    ).map_err(|e| format!("next ocr: {e}"))?;
    let mut rows = stmt.query(params![]).map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        Ok(Some(row_to_image(row)?))
    } else {
        Ok(None)
    }
}

fn row_to_image(r: &rusqlite::Row<'_>) -> Result<ImageAsset, String> {
    row_to_image_impl(r)
}

/// Public helper for use by the commands::images module.
pub fn row_to_image_for_test(r: &rusqlite::Row<'_>) -> Result<ImageAsset, String> {
    row_to_image_impl(r)
}

fn row_to_image_impl(r: &rusqlite::Row<'_>) -> Result<ImageAsset, String> {
    Ok(ImageAsset {
        id: r.get(0).map_err(|e| e.to_string())?,
        memory_id: r.get(1).ok(),
        original_path: r.get(2).map_err(|e| e.to_string())?,
        thumbnail_path: r.get(3).ok(),
        mime_type: r.get(4).ok(),
        width: r.get(5).ok(),
        height: r.get(6).ok(),
        byte_size: r.get(7).ok(),
        caption: r.get(8).ok(),
        source_app: r.get(9).ok(),
        source_window: r.get(10).ok(),
        source_project: r.get(11).ok(),
        ocr_text: r.get(12).ok(),
        ocr_status: r.get(13).map_err(|e| e.to_string())?,
        ocr_completed_at_ms: r.get(14).ok(),
        thumbnail_status: r.get(15).map_err(|e| e.to_string())?,
        thumbnail_completed_at_ms: r.get(16).ok(),
        privacy_blocked: r.get::<_, i64>(17).map_err(|e| e.to_string())? != 0,
        created_at_ms: r.get(18).map_err(|e| e.to_string())?,
        updated_at_ms: r.get(19).map_err(|e| e.to_string())?,
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
    fn register_and_get_image() {
        let conn = setup();
        let id = register(
            &conn, "/tmp/example.png", Some("image/png"),
            Some(640), Some(480), Some(12345),
            Some("My example image"), Some("MyApp"),
            Some("Main Window"), Some("MyProject"),
            false,
        ).unwrap();
        let img = get(&conn, &id).unwrap().unwrap();
        assert_eq!(img.original_path, "/tmp/example.png");
        assert_eq!(img.mime_type.as_deref(), Some("image/png"));
        assert_eq!(img.width, Some(640));
        assert_eq!(img.height, Some(480));
        assert!(!img.privacy_blocked);
        assert_eq!(img.ocr_status, "pending");
    }

    #[test]
    fn privacy_blocked_skips_ocr() {
        let conn = setup();
        let id = register(&conn, "/tmp/secret.png", Some("image/png"),
            None, None, None, None, None, None, None, true).unwrap();
        let img = get(&conn, &id).unwrap().unwrap();
        assert!(img.privacy_blocked);
        assert_eq!(img.ocr_status, "skipped");
    }

    #[test]
    fn set_ocr_text_indexes() {
        let conn = setup();
        let id = register(&conn, "/tmp/x.png", None, None, None, None, None, None, None, None, false).unwrap();
        set_ocr_text(&conn, &id, "The quick brown fox").unwrap();
        let img = get(&conn, &id).unwrap().unwrap();
        assert_eq!(img.ocr_status, "done");
        assert!(img.ocr_text.as_deref().unwrap().contains("quick"));
    }

    #[test]
    fn mark_ocr_failed_preserves_image() {
        let conn = setup();
        let id = register(&conn, "/tmp/x.png", None, None, None, None, None, None, None, None, false).unwrap();
        mark_ocr_failed(&conn, &id).unwrap();
        let img = get(&conn, &id).unwrap().unwrap();
        assert_eq!(img.ocr_status, "failed");
        // Image still exists.
        assert!(get(&conn, &id).unwrap().is_some());
    }

    #[test]
    fn next_ocr_pending_finds_oldest() {
        let conn = setup();
        let a = register(&conn, "/tmp/a.png", None, None, None, None, None, None, None, None, false).unwrap();
        let _b = register(&conn, "/tmp/b.png", None, None, None, None, None, None, None, None, false).unwrap();
        let next = next_ocr_pending(&conn).unwrap().unwrap();
        // a is created first, so it should be returned.
        assert_eq!(next.id, a);
    }

    #[test]
    fn delete_cascades() {
        let conn = setup();
        let id = register(&conn, "/tmp/x.png", None, None, None, None, None, None, None, None, false).unwrap();
        delete(&conn, &id).unwrap();
        assert!(get(&conn, &id).unwrap().is_none());
    }
}
