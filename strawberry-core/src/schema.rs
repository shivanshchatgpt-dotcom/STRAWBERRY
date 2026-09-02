//! 🏗️ Shared schema definitions for tables used by both the Tauri app and
//! the clipboard capture daemon.
//!
//! This module is the SINGLE SOURCE OF TRUTH for the subset of the database
//! schema that both processes share. The daemon calls [`ensure_shared_schema`]
//! instead of maintaining its own copy of CREATE TABLE statements.
//!
//! RULES:
//! - This module defines ONLY the shared subset (roots, nodes, chats,
//!   chat_artifacts, FTS). It does NOT replace the app's migration system.
//! - Historical migrations 001–015 in the app are NEVER edited.
//! - Both app and daemon must agree on these table shapes.
//! - If a column is added to a shared table, add it here AND in a new migration.

/// Ensure the shared schema tables exist. Idempotent (CREATE IF NOT EXISTS).
///
/// The daemon calls this instead of its own `ensure_schema`. The app calls
/// its migration system which creates these same tables — so both paths
/// produce identical schemas.
pub fn ensure_shared_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(SHARED_SQL)
        .map_err(|e| format!("Shared schema error: {e}"))
}

/// SQL for the shared tables. Both app and daemon must produce the same
/// result when executing this.
const SHARED_SQL: &str = r#"
-- ═══ Shared domain tables ═══════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS roots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT,
    icon TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    root_id TEXT NOT NULL REFERENCES roots(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES nodes(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK(type IN ('folder','chat')),
    name TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_nodes_root_id ON nodes(root_id);
CREATE INDEX IF NOT EXISTS idx_nodes_parent_id ON nodes(parent_id);

CREATE TABLE IF NOT EXISTS chats (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL UNIQUE REFERENCES nodes(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'manual',
    raw_path TEXT NOT NULL,
    brief_path TEXT,
    first_idea TEXT,
    tags TEXT,
    brief_text TEXT,
    char_count INTEGER,
    word_count INTEGER,
    code_block_count INTEGER,
    error_count INTEGER,
    command_count INTEGER,
    url_count INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chats_node_id ON chats(node_id);

CREATE TABLE IF NOT EXISTS chat_artifacts (
    id TEXT PRIMARY KEY,
    chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL CHECK(artifact_type IN (
        'code','error','command','url','decision','action_item',
        'heading','question','answer',
        'rejected','constraint','identifier')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chat_artifacts_chat_id ON chat_artifacts(chat_id);
CREATE INDEX IF NOT EXISTS idx_chat_artifacts_type ON chat_artifacts(artifact_type);

-- ═══ FTS5 index + sync triggers ═════════════════════════════════════════════

CREATE VIRTUAL TABLE IF NOT EXISTS chat_fts USING fts5(
    title, first_idea, tags, brief_text,
    content='chats', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS chats_fts_ai AFTER INSERT ON chats BEGIN
    INSERT INTO chat_fts(rowid, title, first_idea, tags, brief_text)
    VALUES (
        new.rowid,
        coalesce(new.title, ''),
        coalesce(new.first_idea, ''),
        coalesce(new.tags, ''),
        coalesce(new.brief_text, '')
    );
END;

CREATE TRIGGER IF NOT EXISTS chats_fts_ad AFTER DELETE ON chats BEGIN
    INSERT INTO chat_fts(chat_fts, rowid, title, first_idea, tags, brief_text)
    VALUES (
        'delete', old.rowid,
        coalesce(old.title, ''),
        coalesce(old.first_idea, ''),
        coalesce(old.tags, ''),
        coalesce(old.brief_text, '')
    );
END;

CREATE TRIGGER IF NOT EXISTS chats_fts_au AFTER UPDATE ON chats BEGIN
    INSERT INTO chat_fts(chat_fts, rowid, title, first_idea, tags, brief_text)
    VALUES (
        'delete', old.rowid,
        coalesce(old.title, ''),
        coalesce(old.first_idea, ''),
        coalesce(old.tags, ''),
        coalesce(old.brief_text, '')
    );
    INSERT INTO chat_fts(rowid, title, first_idea, tags, brief_text)
    VALUES (
        new.rowid,
        coalesce(new.title, ''),
        coalesce(new.first_idea, ''),
        coalesce(new.tags, ''),
        coalesce(new.brief_text, '')
    );
END;
"#;

/// Generate a unique ID with a prefix. Used by the daemon for chat/node IDs.
///
/// Format: `{prefix}-{hex_timestamp}-{hex_counter}`
pub fn gen_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{t:x}-{n:x}")
}

/// Current UTC time as ISO 8601 / RFC3339 string. No external crates needed.
pub fn now_iso() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = d.div_euclid(86_400);
    let rem = d.rem_euclid(86_400);
    let (y, m, dd) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{dd:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days-since-epoch → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: u64) -> (i64, u64, u64) {
    let z = z as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u64;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u64;
    ((if m <= 2 { y + 1 } else { y }), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_schema_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_shared_schema(&conn).unwrap();
        ensure_shared_schema(&conn).unwrap(); // second call must not fail
    }

    #[test]
    fn shared_schema_creates_all_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_shared_schema(&conn).unwrap();

        for table in ["roots", "nodes", "chats", "chat_artifacts"] {
            let exists: bool = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "table {table} missing");
        }

        // FTS
        let exists: bool = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='chat_fts'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(exists, "chat_fts missing");

        // Triggers
        for trigger in ["chats_fts_ai", "chats_fts_ad", "chats_fts_au"] {
            let exists: bool = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='trigger' AND name=?1",
                    [trigger],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "trigger {trigger} missing");
        }
    }

    #[test]
    fn gen_id_produces_unique_values() {
        let a = gen_id("chat");
        let b = gen_id("chat");
        assert_ne!(a, b);
        assert!(a.starts_with("chat-"));
    }

    #[test]
    fn now_iso_format() {
        let ts = now_iso();
        // Must be YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[4], b'-');
        assert_eq!(ts.as_bytes()[7], b'-');
        assert_eq!(ts.as_bytes()[10], b'T');
        assert_eq!(ts.as_bytes()[13], b':');
        assert_eq!(ts.as_bytes()[16], b':');
    }

    #[test]
    fn civil_date_known_value() {
        assert_eq!(civil_from_days(20_690), (2026, 8, 25));
    }

    #[test]
    fn shared_schema_allows_full_roundtrip() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        ensure_shared_schema(&conn).unwrap();

        let now = now_iso();

        // Insert root
        conn.execute(
            "INSERT INTO roots(id,name,color,icon,created_at,updated_at)
             VALUES('r1','Test Root',NULL,NULL,?1,?1)",
            [&now],
        )
        .unwrap();

        // Insert node
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n1','r1',NULL,'chat','Test Chat',0,?1,?1)",
            [&now],
        )
        .unwrap();

        // Insert chat
        conn.execute(
            "INSERT INTO chats(id,node_id,title,source,raw_path,first_idea,tags,brief_text,
                               char_count,word_count,created_at,updated_at)
             VALUES('c1','n1','Test Chat','manual','/tmp/x','first idea','rust,tauri',
                    'brief about testing',100,20,?1,?1)",
            [&now],
        )
        .unwrap();

        // FTS should find it
        let hit: Option<String> = conn
            .query_row(
                "SELECT ch.id FROM chat_fts f JOIN chats ch ON ch.rowid=f.rowid
                 WHERE chat_fts MATCH 'testing'",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(hit.as_deref(), Some("c1"));

        // Delete root cascades
        conn.execute("DELETE FROM roots WHERE id='r1'", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM chats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "cascade failed");
    }
}
