-- Migration 016: Canonical Event Foundation.
-- Provides the persistent event spine that unifies all event vocabularies.

CREATE TABLE IF NOT EXISTS canonical_events (
    id              TEXT PRIMARY KEY,
    role            TEXT NOT NULL CHECK(role IN ('raw','observation','memory','insight','task','action')),
    event_type      TEXT NOT NULL,
    source          TEXT NOT NULL,
    source_version  TEXT NOT NULL DEFAULT '1.0',
    reference_id    TEXT,
    actor           TEXT NOT NULL DEFAULT 'user',
    occurred_at_ms  INTEGER NOT NULL,
    created_at_ms   INTEGER NOT NULL,
    project_id      TEXT,
    session_id      TEXT,
    payload_json    TEXT,
    privacy_level   TEXT NOT NULL DEFAULT 'public' CHECK(privacy_level IN ('public','sensitive','secret')),
    dedupe_key      TEXT,
    retention       TEXT NOT NULL DEFAULT 'medium' CHECK(retention IN ('short','medium','permanent','user_managed')),
    confidence      REAL NOT NULL DEFAULT 1.0,
    provenance      TEXT
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_ce_type ON canonical_events(event_type);
CREATE INDEX IF NOT EXISTS idx_ce_source ON canonical_events(source);
CREATE INDEX IF NOT EXISTS idx_ce_role ON canonical_events(role);
CREATE INDEX IF NOT EXISTS idx_ce_occurred ON canonical_events(occurred_at_ms);
CREATE INDEX IF NOT EXISTS idx_ce_project ON canonical_events(project_id);
CREATE INDEX IF NOT EXISTS idx_ce_session ON canonical_events(session_id);
CREATE INDEX IF NOT EXISTS idx_ce_dedupe ON canonical_events(dedupe_key);
CREATE INDEX IF NOT EXISTS idx_ce_privacy ON canonical_events(privacy_level);
CREATE INDEX IF NOT EXISTS idx_ce_retention ON canonical_events(retention);
