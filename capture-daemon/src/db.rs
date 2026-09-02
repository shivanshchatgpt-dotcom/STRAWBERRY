//! 🍓 Direct SQLite (FTS5) writer for captures.
//! Writes straight into the STRAWBERRY app database so every popup-click
//! is instantly searchable in the app. WAL mode allows concurrent access
//! between the daemon and the Tauri app.

use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Resolve the STRAWBERRY app-data dir per OS (same paths the Tauri app uses).
pub fn app_data_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        // Linux/BSD: XDG_DATA_HOME or ~/.local/share
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|_| PathBuf::from("."))
    };
    base.join("com.local.chatmemorytree")
}

pub fn db_path() -> PathBuf {
    app_data_dir().join("app.db")
}

/// Open the shared DB with WAL + busy timeout so the app and daemon can run together.
pub fn open() -> Result<Connection, String> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(3000))
        .map_err(|e| e.to_string())?;
    ensure_schema(&conn)?;
    Ok(conn)
}/// Create the exact same schema the Tauri app uses (idempotent).
/// Delegates to `strawberry_core::schema::ensure_shared_schema` — the
/// SINGLE SOURCE OF TRUTH for tables shared between app and daemon.
fn ensure_schema(conn: &Connection) -> Result<(), String> {
    strawberry_core::schema::ensure_shared_schema(conn)
}

fn now_iso() -> String {
    strawberry_core::schema::now_iso()
}

fn gen_id(prefix: &str) -> String {
    strawberry_core::schema::gen_id(prefix)
}

/// Insert a capture as a full first-class chat inside the "🍓 Captures" root.
///
/// Returns the chat id on success. The raw text also lands on disk so the
/// app's Original tab keeps working unchanged.
pub fn insert_capture(kind: &str, text: &str, raw_file: &Path) -> Result<String, String> {
    let conn = open()?;
    let ts = now_iso();

    // Ensure the Captures root exists (stable id, created once).
    const ROOT_ID: &str = "root-captures";
    conn.execute(
        "INSERT OR IGNORE INTO roots(id,name,color,icon,created_at,updated_at)
         VALUES(?1,'🍓 Captures','#e74c3c','🍓',?2,?2)",
        rusqlite::params![ROOT_ID, ts],
    )
    .map_err(|e| e.to_string())?;

    let chat_id = gen_id("chat");
    let node_id = gen_id("node");

    let title: String = text
        .chars()
        .take(70)
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect();
    let first_idea: String = text.lines().next().unwrap_or("").chars().take(240).collect();
    let words = text.split_whitespace().count() as i64;
    let chars = text.chars().count() as i64;

    conn.execute(
        "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
         VALUES(?1,?2,NULL,'chat',?3,
                (SELECT COALESCE(MAX(position),0)+1 FROM nodes WHERE root_id=?2),
                ?4,?4)",
        rusqlite::params![node_id, ROOT_ID, title, ts],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO chats(id,node_id,title,source,raw_path,first_idea,tags,brief_text,
                           char_count,word_count,created_at,updated_at)
         VALUES(?1,?2,?3,'capture',?4,?5,?6,?7,?8,?9,?10,?10)",
        rusqlite::params![
            chat_id,
            node_id,
            title,
            raw_file.display().to_string(),
            first_idea,
            kind,      // tags column → searchable + filterable by type
            text,      // brief_text = full text → FTS5 finds EVERYTHING
            chars,
            words,
            ts
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(chat_id)
}

#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Serializes tests that override the process-wide `XDG_DATA_HOME`.
    ///
    /// `set_var` is global to the process, so two tests pointing it at
    /// different temp dirs in parallel will read each other's database. This
    /// lock makes the override effectively single-threaded.
    pub fn lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_search_roundtrip() {
        let _guard = test_env::lock();

        // Use a temp override via XDG_DATA_HOME.
        let dir = std::env::temp_dir().join(format!("sb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_DATA_HOME", &dir);

        let raw = dir.join("raw.txt");
        std::fs::write(&raw, "test cargo fix error E0308").unwrap();

        let id = insert_capture("error", "test cargo fix error E0308", &raw).unwrap();

        let conn = open().unwrap();
        let hit: Option<String> = conn
            .query_row(
                "SELECT ch.id FROM chat_fts f JOIN chats ch ON ch.rowid=f.rowid
                 WHERE chat_fts MATCH 'E0308'",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(hit.as_deref(), Some(id.as_str()));

        // Second insert doesn't collide.
        let id2 = insert_capture("note", "second unique note alpha", &raw).unwrap();
        assert_ne!(id, id2);

        // The widened CHECK constraint must accept the handoff artifact types.
        let now = now_iso();
        for (i, kind) in ["rejected", "constraint", "identifier"].iter().enumerate() {
            conn.execute(
                "INSERT INTO chat_artifacts(id,chat_id,artifact_type,content,created_at)
                 VALUES(?1,?2,?3,'x',?4)",
                rusqlite::params![format!("art-{i}"), id, kind, now],
            )
            .unwrap_or_else(|e| panic!("daemon schema rejected {kind}: {e}"));
        }

        drop(conn);
        std::env::remove_var("XDG_DATA_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }

    // civil_from_days and now_iso tests live in strawberry_core::schema::tests
}
