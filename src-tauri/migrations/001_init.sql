-- Migration 001: initial schema for Chat Memory Tree.
-- FTS5 table and its triggers are created separately at runtime because the
-- bundled SQLite build may or may not expose FTS5 (see db/migrations.rs).

CREATE TABLE IF NOT EXISTS schema_migrations (
    version    INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS roots (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    color      TEXT,
    icon       TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS nodes (
    id         TEXT PRIMARY KEY,
    root_id    TEXT NOT NULL REFERENCES roots(id) ON DELETE CASCADE,
    parent_id  TEXT REFERENCES nodes(id) ON DELETE CASCADE,
    type       TEXT NOT NULL CHECK(type IN ('folder', 'chat')),
    name       TEXT NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_nodes_root_id   ON nodes(root_id);
CREATE INDEX IF NOT EXISTS idx_nodes_parent_id ON nodes(parent_id);

CREATE TABLE IF NOT EXISTS chats (
    id               TEXT PRIMARY KEY,
    node_id          TEXT NOT NULL UNIQUE REFERENCES nodes(id) ON DELETE CASCADE,
    title            TEXT NOT NULL DEFAULT '',
    source           TEXT NOT NULL DEFAULT 'manual',
    raw_path         TEXT NOT NULL,
    brief_path       TEXT,
    first_idea       TEXT,
    tags             TEXT,
    brief_text       TEXT,
    char_count       INTEGER,
    word_count       INTEGER,
    code_block_count INTEGER,
    error_count      INTEGER,
    command_count    INTEGER,
    url_count        INTEGER,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chats_node_id ON chats(node_id);

CREATE TABLE IF NOT EXISTS chat_artifacts (
    id            TEXT PRIMARY KEY,
    chat_id       TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL CHECK(artifact_type IN (
        'code', 'error', 'command', 'url', 'decision',
        'action_item', 'heading', 'question', 'answer'
    )),
    content       TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_artifacts_chat_id ON chat_artifacts(chat_id);

-- Simple key/value store for app-level metadata (e.g. fts_enabled flag).
CREATE TABLE IF NOT EXISTS app_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
