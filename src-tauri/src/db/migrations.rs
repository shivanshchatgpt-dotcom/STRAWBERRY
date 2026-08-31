use rusqlite::Connection;

const MIGRATION_V1: &str = include_str!("../../migrations/001_init.sql");
const MIGRATION_V2: &str = include_str!("../../migrations/002_planner.sql");
const MIGRATION_V3: &str = include_str!("../../migrations/003_handoff.sql");
const MIGRATION_V4: &str = include_str!("../../migrations/004_resume_tabs.sql");
const MIGRATION_V5: &str = include_str!("../../migrations/005_screen_memory.sql");
const MIGRATION_V6: &str = include_str!("../../migrations/006_work_snapshots.sql");
const MIGRATION_V7: &str = include_str!("../../migrations/007_planner_merge.sql");
const MIGRATION_V8: &str = include_str!("../../migrations/008_freeze_resume.sql");
const MIGRATION_V9: &str = include_str!("../../migrations/009_ambient_memory.sql");
const MIGRATION_V10: &str = include_str!("../../migrations/010_events_calendar.sql");
const MIGRATION_V11: &str = include_str!("../../migrations/011_workspace_resume_v1.sql");
const MIGRATION_V12: &str = include_str!("../../migrations/012_alpha_hunter.sql");
const MIGRATION_V13: &str = include_str!("../../migrations/013_wellness.sql");
const MIGRATION_V14: &str = include_str!("../../migrations/014_ghost.sql");

fn apply(tx: &rusqlite::Transaction<'_>, version: i64, sql: &str) -> Result<(), String> {
    tx.execute_batch(sql)
        .map_err(crate::error::to_string_err(&format!("migration {version:03} failed")))?;
    tx.execute(
        "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
        rusqlite::params![version, crate::db::now_iso()],
    )
    .map_err(crate::error::to_string_err(&format!("migration {version:03} failed to record")))?;
    Ok(())
}

/// Apply pending schema migrations in strict ascending version order.
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

    let applied: std::collections::HashSet<i64> = {
        let mut stmt = tx
            .prepare("SELECT version FROM schema_migrations")
            .map_err(crate::error::to_string_err("failed to read migrations"))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(crate::error::to_string_err("failed to read migrations"))?;
        rows.collect::<Result<_, _>>()
            .map_err(crate::error::to_string_err("failed to read migrations"))?
    };

    let migrations: &[(i64, &str)] = &[
        (1, MIGRATION_V1),
        (2, MIGRATION_V2),
        (3, MIGRATION_V3),
        (4, MIGRATION_V4),
        (5, MIGRATION_V5),
        (6, MIGRATION_V6),
        (7, MIGRATION_V7),
        (8, MIGRATION_V8),
        (9, MIGRATION_V9),
        (10, MIGRATION_V10),
        (11, MIGRATION_V11),
        (12, MIGRATION_V12),
        (13, MIGRATION_V13),
        (14, MIGRATION_V14),
    ];

    for &(version, sql) in migrations {
        if !applied.contains(&version) {
            apply(&tx, version, sql)?;
        }
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
