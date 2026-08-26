-- Migration 008: 🧊 Freeze & Resume — whole-workspace session snapshots.

CREATE TABLE IF NOT EXISTS work_spaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    story       TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    restored_at TEXT,
    raw_json    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_work_spaces_created ON work_spaces(created_at DESC);
