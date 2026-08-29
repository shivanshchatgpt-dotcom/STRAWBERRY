//! 👻 Ghost Tracker — records every meaningful user action.

use rusqlite::Connection;
use crate::ghost::EventType;
use crate::db::now_iso;

/// Record a single ghost event. Returns the new event id, or 0 on failure.
pub fn record(
    conn: &Connection,
    event_type: EventType,
    source_id: Option<&str>,
    source_kind: Option<&str>,
    duration_ms: i64,
    metadata: Option<&str>,
) -> Result<i64, String> {
    let now = now_iso();
    conn.execute(
        "INSERT INTO ghost_events(event_type, source_id, source_kind, duration_ms, metadata, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            event_type.as_str(),
            source_id,
            source_kind,
            duration_ms,
            metadata,
            now
        ],
    ).map_err(|e| format!("ghost event insert failed: {e}"))?;

    Ok(conn.last_insert_rowid())
}

/// Record an event with a custom timestamp (used for backfilling / migrations).
pub fn record_at(
    conn: &Connection,
    event_type: EventType,
    source_id: Option<&str>,
    source_kind: Option<&str>,
    duration_ms: i64,
    metadata: Option<&str>,
    at: &str,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO ghost_events(event_type, source_id, source_kind, duration_ms, metadata, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            event_type.as_str(),
            source_id,
            source_kind,
            duration_ms,
            metadata,
            at
        ],
    ).map_err(|e| format!("ghost event insert failed: {e}"))?;

    Ok(conn.last_insert_rowid())
}

/// Delete all events older than `days`.
pub fn prune_older_than(conn: &Connection, days: i64) -> Result<usize, String> {
    let n = conn.execute(
        "DELETE FROM ghost_events WHERE created_at < datetime('now', ?1)",
        [format!("-{} days", days)],
    ).map_err(|e| format!("ghost prune failed: {e}"))?;
    Ok(n)
}

/// Count all events.
pub fn count_all(conn: &Connection) -> Result<i64, String> {
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM ghost_events", [], |r| r.get(0))
        .map_err(|e| format!("ghost count failed: {e}"))?;
    Ok(n)
}
