use rusqlite::Connection;

const MIGRATION_V1: &str = include_str!("../../migrations/001_init.sql");

/// Apply pending schema migrations, tracked in `schema_migrations`.
pub fn run(conn: &mut Connection) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(crate::error::to_string_err("failed to begin migration transaction"))?;

    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(crate::error::to_string_err("failed to create schema_migrations"))?;

    let applied: Vec<i64> = {
        let mut stmt = tx
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .map_err(crate::error::to_string_err("failed to read migrations"))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(crate::error::to_string_err("failed to read migrations"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(crate::error::to_string_err("failed to read migrations"))?
    };

    if !applied.contains(&1) {
        tx.execute_batch(MIGRATION_V1)
            .map_err(crate::error::to_string_err("migration 001 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (1, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err("migration 001 failed to record"))?;
    }

    tx.commit()
        .map_err(crate::error::to_string_err("failed to commit migrations"))
}

/// Try to create the FTS5 index and sync triggers.
///
/// Returns `true` when FTS5 is available and wired up, `false` when the
/// runtime must fall back to LIKE-based search.
pub fn ensure_fts(conn: &Connection) -> Result<bool, String> {
    let fts_sql = "
        CREATE VIRTUAL TABLE IF NOT EXISTS chat_fts USING fts5(
            title,
            first_idea,
            tags,
            brief_text,
            content='chats',
            content_rowid='rowid'
        );
    ";
    if conn.execute_batch(fts_sql).is_err() {
        set_fts_flag(conn, false)?;
        return Ok(false);
    }

    let triggers = "
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
            VALUES ('delete', old.rowid,
                    coalesce(old.title, ''),
                    coalesce(old.first_idea, ''),
                    coalesce(old.tags, ''),
                    coalesce(old.brief_text, ''));
        END;

        CREATE TRIGGER IF NOT EXISTS chats_fts_au AFTER UPDATE ON chats BEGIN
            INSERT INTO chat_fts(chat_fts, rowid, title, first_idea, tags, brief_text)
            VALUES ('delete', old.rowid,
                    coalesce(old.title, ''),
                    coalesce(old.first_idea, ''),
                    coalesce(old.tags, ''),
                    coalesce(old.brief_text, ''));
            INSERT INTO chat_fts(rowid, title, first_idea, tags, brief_text)
            VALUES (
                new.rowid,
                coalesce(new.title, ''),
                coalesce(new.first_idea, ''),
                coalesce(new.tags, ''),
                coalesce(new.brief_text, '')
            );
        END;
    ";
    if conn.execute_batch(triggers).is_err() {
        // FTS table exists but triggers failed; drop FTS to stay consistent.
        let _ = conn.execute_batch("DROP TABLE IF EXISTS chat_fts;");
        set_fts_flag(conn, false)?;
        return Ok(false);
    }

    set_fts_flag(conn, true)?;
    Ok(true)
}

fn set_fts_flag(conn: &Connection, enabled: bool) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_meta(key, value) VALUES ('fts_enabled', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [if enabled { "1" } else { "0" }],
    )
    .map_err(crate::error::to_string_err("failed to store fts flag"))?;
    Ok(())
}
