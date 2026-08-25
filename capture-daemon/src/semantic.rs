//! 🍓 Layer 2 — Semantic search via local embeddings (Ollama nomic-embed-text).
//!
//! Vectors live in the same SQLite DB (`embeddings` table, BLOB float32).
//! Cosine similarity in pure Rust — zero extra services. Local-first:
//! Ollama runs on-device, nothing leaves the machine.

use rusqlite::Connection;
use std::path::PathBuf;

const MODEL: &str = "nomic-embed-text";
const OLLAMA_URL: &str = "http://localhost:11434";

/// Same resolution logic as db.rs — keep in sync.
fn app_data_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|_| PathBuf::from("."))
    };
    base.join("com.local.chatmemorytree")
}

fn open_db() -> Result<Connection, String> {
    let conn = Connection::open(app_data_dir().join("app.db")).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.busy_timeout(std::time::Duration::from_millis(3000)).ok();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS embeddings (
            chat_id TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
            dim     INTEGER NOT NULL,
            vec     BLOB NOT NULL
        );",
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Get an embedding vector from local Ollama.
pub fn embed(text: &str) -> Result<Vec<f32>, String> {
    let body = serde_json::json!({
        "model": MODEL,
        "input": text,
    });
    let url = format!("{OLLAMA_URL}/api/embed");
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(30))
        .send_json(body)
        .map_err(|e| format!("ollama embed failed: {e}"))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let arr = json["embeddings"][0]
        .as_array()
        .ok_or("no embeddings in response")?;
    Ok(arr
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect())
}

/// Store embedding for a chat. Called right after insert_capture succeeds.
pub fn index_chat(chat_id: &str, text: &str) -> Result<(), String> {
    let vec = embed(text)?;
    let dim = vec.len() as i64;
    let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
    let conn = open_db()?;
    conn.execute(
        "INSERT INTO embeddings(chat_id, dim, vec) VALUES(?1,?2,?3)
         ON CONFLICT(chat_id) DO UPDATE SET dim=excluded.dim, vec=excluded.vec",
        rusqlite::params![chat_id, dim, bytes],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Semantic search: query → embedding → cosine over all stored vectors.
/// Returns (title, snippet, similarity) sorted best-first. Fast path: pure
/// in-process math over ≤10k vectors is sub-10ms; FTS remains layer 1.
pub fn search(query: &str, limit: usize) -> Result<Vec<(String, String, f32)>, String> {
    let qvec = embed(query)?;
    let conn = open_db()?;
    let mut stmt = conn
        .prepare(
            "SELECT e.chat_id, ch.title, ch.brief_text, e.vec, e.dim
             FROM embeddings e JOIN chats ch ON ch.id = e.chat_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let title: String = r.get(1)?;
            let body: String = r.get::<_, Option<String>>(2)?.unwrap_or_default();
            let bytes: Vec<u8> = r.get(3)?;
            let dim: i64 = r.get(4)?;
            Ok((title, body, bytes, dim))
        })
        .map_err(|e| e.to_string())?;

    let mut scored: Vec<(String, String, f32)> = Vec::new();
    for row in rows {
        let (title, body, bytes, dim) = row.map_err(|e| e.to_string())?;
        if bytes.len() != dim as usize * 4 {
            continue;
        }
        let vec: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let score = cosine(&qvec, &vec);
        let snippet: String = body.chars().take(120).collect();
        scored.push((title, snippet, score));
    }
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[], &[1.0]), 0.0);
    }

    #[test]
    fn embed_and_search_roundtrip_live_ollama() {
        // Shares the process-wide XDG_DATA_HOME override with the db tests.
        let _guard = crate::db::test_env::lock();

        // Requires ollama with nomic-embed-text — skipped silently if absent.
        let ollama_up = std::process::Command::new("curl")
            .args(["-s", "-o", "/dev/null", "http://localhost:11434"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ollama_up {
            return;
        }
        let dir = std::env::temp_dir().join(format!("sb-l2-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("com.local.chatmemorytree")).unwrap();
        std::env::set_var("XDG_DATA_HOME", &dir);
        super::open_db().unwrap();

        let id1 = crate::db::insert_capture(
            "error",
            "rust borrow checker E0502 cannot borrow x as mutable while immutable borrow active",
            std::path::Path::new("/tmp/a"),
        )
        .unwrap();
        let id2 = crate::db::insert_capture(
            "note",
            "chocolate cake recipe needs two eggs and flour",
            std::path::Path::new("/tmp/b"),
        )
        .unwrap();
        index_chat(&id1, "rust borrow checker E0502 cannot borrow mutable").unwrap();
        index_chat(&id2, "chocolate cake recipe eggs flour").unwrap();

        let results = search("how to fix rust mutable borrow error", 2).unwrap();
        assert_eq!(results.len(), 2);
        // The rust error capture must outrank the cake recipe.
        assert!(results[0].0.contains("borrow"), "top hit was {:?}", results[0].0);

        std::env::remove_var("XDG_DATA_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }
}
