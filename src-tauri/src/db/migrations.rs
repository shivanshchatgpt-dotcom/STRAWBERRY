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
const MIGRATION_V15: &str = include_str!("../../migrations/015_calendar_v2.sql");
const MIGRATION_V16: &str = include_str!("../../migrations/016_canonical_events.sql");
const MIGRATION_V17: &str = include_str!("../../migrations/017_unified_memory.sql");

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
        .map_err(crate::error::to_string_err(
            "migration 001 failed to record",
        ))?;
    }

    if !applied.contains(&2) {
        tx.execute_batch(MIGRATION_V2)
            .map_err(crate::error::to_string_err("migration 002 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (2, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 002 failed to record",
        ))?;
    }

    if !applied.contains(&3) {
        tx.execute_batch(MIGRATION_V3)
            .map_err(crate::error::to_string_err("migration 003 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (3, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 003 failed to record",
        ))?;
    }

    if !applied.contains(&4) {
        tx.execute_batch(MIGRATION_V4)
            .map_err(crate::error::to_string_err("migration 004 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (4, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 004 failed to record",
        ))?;
    }

    if !applied.contains(&5) {
        tx.execute_batch(MIGRATION_V5)
            .map_err(crate::error::to_string_err("migration 005 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (5, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 005 failed to record",
        ))?;
    }

    if !applied.contains(&6) {
        tx.execute_batch(MIGRATION_V6)
            .map_err(crate::error::to_string_err("migration 006 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (6, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 006 failed to record",
        ))?;
    }

    if !applied.contains(&8) {
        tx.execute_batch(MIGRATION_V8)
            .map_err(crate::error::to_string_err("migration 008 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (8, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 008 failed to record",
        ))?;
    }

    if !applied.contains(&7) {
        tx.execute_batch(MIGRATION_V7)
            .map_err(crate::error::to_string_err("migration 007 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (7, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 007 failed to record",
        ))?;
    }

    if !applied.contains(&9) {
        tx.execute_batch(MIGRATION_V9)
            .map_err(crate::error::to_string_err("migration 009 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (9, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 009 failed to record",
        ))?;
    }

    if !applied.contains(&10) {
        tx.execute_batch(MIGRATION_V10)
            .map_err(crate::error::to_string_err("migration 010 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (10, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 010 failed to record",
        ))?;
    }

    if !applied.contains(&11) {
        tx.execute_batch(MIGRATION_V11)
            .map_err(crate::error::to_string_err("migration 011 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (11, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 011 failed to record",
        ))?;
    }

    if !applied.contains(&12) {
        tx.execute_batch(MIGRATION_V12)
            .map_err(crate::error::to_string_err("migration 012 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (12, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 012 failed to record",
        ))?;
    }

    if !applied.contains(&13) {
        tx.execute_batch(MIGRATION_V13)
            .map_err(crate::error::to_string_err("migration 013 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (13, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 013 failed to record",
        ))?;
    }

    if !applied.contains(&14) {
        tx.execute_batch(MIGRATION_V14)
            .map_err(crate::error::to_string_err("migration 014 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (14, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 014 failed to record",
        ))?;
    }

    if !applied.contains(&15) {
        tx.execute_batch(MIGRATION_V15)
            .map_err(crate::error::to_string_err("migration 015 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (15, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 015 failed to record",
        ))?;
    }

    if !applied.contains(&16) {
        tx.execute_batch(MIGRATION_V16)
            .map_err(crate::error::to_string_err("migration 016 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (16, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 016 failed to record",
        ))?;
    }

    if !applied.contains(&17) {
        tx.execute_batch(MIGRATION_V17)
            .map_err(crate::error::to_string_err("migration 017 failed"))?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (17, ?1)",
            [crate::db::now_iso()],
        )
        .map_err(crate::error::to_string_err(
            "migration 017 failed to record",
        ))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_chat(conn: &Connection) {
        let now = crate::db::now_iso();
        conn.execute(
            "INSERT INTO roots(id,name,color,icon,created_at,updated_at)
             VALUES('r','R',NULL,NULL,?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n','r',NULL,'chat','C',0,?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chats(id,node_id,title,source,raw_path,created_at,updated_at)
             VALUES('c','n','C','manual','/tmp/x',?1,?1)",
            [&now],
        )
        .unwrap();
    }

    /// Migration 003 rebuilds `chat_artifacts` to widen its CHECK constraint.
    /// A rebuild is the one migration shape that can silently lose rows, so
    /// this asserts both halves: old rows survive, new types are accepted.
    #[test]
    fn migration_003_preserves_rows_and_widens_check() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        // Apply only 001 + 002 to simulate an existing pre-003 database.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
        )
        .unwrap();
        conn.execute_batch(MIGRATION_V1).unwrap();
        conn.execute_batch(MIGRATION_V2).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (1,'t'),(2,'t')",
            [],
        )
        .unwrap();
        seed_chat(&conn);

        let now = crate::db::now_iso();
        conn.execute(
            "INSERT INTO chat_artifacts(id,chat_id,artifact_type,content,created_at)
             VALUES('a1','c','decision','keep rusqlite',?1)",
            [&now],
        )
        .unwrap();
        // The pre-003 schema must reject the new type.
        assert!(conn
            .execute(
                "INSERT INTO chat_artifacts(id,chat_id,artifact_type,content,created_at)
                 VALUES('a2','c','rejected','sqlx',?1)",
                [&now],
            )
            .is_err());

        run(&mut conn).expect("migration 003 must apply");

        // Pre-existing row survived the table rebuild.
        let kept: String = conn
            .query_row(
                "SELECT content FROM chat_artifacts WHERE id='a1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, "keep rusqlite");

        // All three new types are now accepted.
        for (id, kind) in [
            ("a2", "rejected"),
            ("a3", "constraint"),
            ("a4", "identifier"),
        ] {
            conn.execute(
                "INSERT INTO chat_artifacts(id,chat_id,artifact_type,content,created_at)
                 VALUES(?1,'c',?2,'x',?3)",
                rusqlite::params![id, kind, now],
            )
            .unwrap_or_else(|e| panic!("type {kind} rejected after migration: {e}"));
        }

        // Unknown types are still refused.
        assert!(conn
            .execute(
                "INSERT INTO chat_artifacts(id,chat_id,artifact_type,content,created_at)
                 VALUES('a9','c','nonsense','x',?1)",
                [&now],
            )
            .is_err());

        // Cascade still works after the rename.
        conn.execute("DELETE FROM chats WHERE id='c'", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM chat_artifacts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "foreign key cascade lost in rebuild");
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();
        let versions: i64 = conn
            .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(versions, 17);
    }

    /// Full migration chain produces all critical tables, indexes, and triggers.
    #[test]
    fn fresh_db_has_critical_tables_and_triggers() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run(&mut conn).unwrap();

        // Critical tables exist
        for table in [
            "roots", "nodes", "chats", "chat_artifacts",
            "todos", "habits", "habit_logs",
            "chat_resume_points", "tabs",
            "screen_frames", "screen_blocklist",
            "work_snapshots", "work_spaces",
            "ambient_events",
            "events", "event_reminders",
            "workspace_sessions", "workspace_items",
            "alpha_candidates",
            "wellness_config", "wellness_state",
            "ghost_events", "ghost_insights",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "table {table} missing after migration");
        }

        // FTS virtual tables are created by ensure_fts() after migration.
        ensure_fts(&conn).expect("ensure_fts must succeed");
        for fts in ["chat_fts"] {
            let exists: bool = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [fts],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "FTS table {fts} missing");
        }

        // Critical triggers exist
        for trigger in [
            "chats_fts_ai", "chats_fts_ad", "chats_fts_au",
        ] {
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

    /// Calendar event round-trip: create → read → update → delete.
    #[test]
    fn calendar_round_trip() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let now = crate::db::now_iso();

        // Create
        conn.execute(
            "INSERT INTO events(id,title,start_at,end_at,category,created_at,updated_at)
             VALUES('ev1','Test Event','2026-09-01T10:00:00Z','2026-09-01T11:00:00Z','work',?1,?1)",
            [&now],
        )
        .unwrap();

        // Read
        let title: String = conn
            .query_row("SELECT title FROM events WHERE id='ev1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "Test Event");

        // Update
        conn.execute(
            "UPDATE events SET title='Updated Event' WHERE id='ev1'",
            [],
        )
        .unwrap();
        let title2: String = conn
            .query_row("SELECT title FROM events WHERE id='ev1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title2, "Updated Event");

        // Reminder (event_reminders has no created_at column)
        conn.execute(
            "INSERT INTO event_reminders(id,event_id,minutes_before,enabled,triggered)
             VALUES('rem1','ev1',15,1,0)",
            [],
        )
        .unwrap();
        let rem: i64 = conn
            .query_row("SELECT minutes_before FROM event_reminders WHERE id='rem1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rem, 15);

        // Delete (cascade)
        conn.execute("DELETE FROM events WHERE id='ev1'", []).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM event_reminders", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "reminder cascade failed");
    }

    /// Task (todo) round-trip: create → toggle → delete.
    #[test]
    fn task_round_trip() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let now = crate::db::now_iso();

        // todos uses INTEGER PK (autoincrement) and 'completed' column
        conn.execute(
            "INSERT INTO todos(title,completed,created_at,updated_at)
             VALUES('Fix bug',0,?1,?1)",
            [&now],
        )
        .unwrap();

        let id: i64 = conn.last_insert_rowid();
        let completed: i64 = conn
            .query_row("SELECT completed FROM todos WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(completed, 0);

        conn.execute("UPDATE todos SET completed=1 WHERE id=?1", [id]).unwrap();
        let completed2: i64 = conn
            .query_row("SELECT completed FROM todos WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(completed2, 1);

        conn.execute("DELETE FROM todos WHERE id=?1", [id]).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM todos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    /// FTS search works after migration and ensure_fts.
    #[test]
    fn fts_search_round_trip() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        ensure_fts(&conn).expect("ensure_fts must succeed");
        let now = crate::db::now_iso();

        // Seed a chat
        conn.execute(
            "INSERT INTO roots(id,name,color,icon,created_at,updated_at)
             VALUES('r','Root',NULL,NULL,?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n','r',NULL,'chat','Chat',0,?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chats(id,node_id,title,source,raw_path,brief_text,created_at,updated_at)
             VALUES('c','n','Rust error handling','manual','/tmp/x','E0308 mismatched types',?1,?1)",
            [&now],
        )
        .unwrap();

        // FTS should find it
        let hit: Option<String> = conn
            .query_row(
                "SELECT ch.id FROM chat_fts f JOIN chats ch ON ch.rowid=f.rowid
                 WHERE chat_fts MATCH 'E0308'",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(hit.as_deref(), Some("c"), "FTS should find the chat");
    }
}
