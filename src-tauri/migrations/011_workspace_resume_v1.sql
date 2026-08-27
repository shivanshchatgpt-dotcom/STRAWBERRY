-- 011_workspace_resume_v1.sql
-- Workspace Resume v0.1: Sessions, Items, Restore Attempts and Actions

CREATE TABLE IF NOT EXISTS workspace_sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    frozen_at INTEGER,
    resumed_at INTEGER,
    status TEXT NOT NULL CHECK(status IN ('capturing','frozen','restoring','restored','partial','failed')),
    trigger TEXT NOT NULL DEFAULT 'manual',
    metadata_json TEXT
);

CREATE TABLE IF NOT EXISTS workspace_items (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES workspace_sessions(id) ON DELETE CASCADE,
    item_type TEXT NOT NULL,
    app_name TEXT,
    process_name TEXT,
    window_title TEXT,
    window_geometry TEXT,
    workspace TEXT,
    cwd TEXT,
    command TEXT,
    browser_url TEXT,
    browser_title TEXT,
    restore_strategy TEXT NOT NULL DEFAULT 'auto',
    restore_status TEXT NOT NULL CHECK(restore_status IN ('pending','launching','restored','skipped','failed')),
    error_message TEXT,
    action_type TEXT,
    action_target TEXT,
    action_payload TEXT,
    display_label TEXT,
    last_action_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_items_session_id ON workspace_items(session_id);
CREATE INDEX IF NOT EXISTS idx_workspace_items_restore_status ON workspace_items(restore_status);

CREATE TABLE IF NOT EXISTS workspace_restore_attempts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES workspace_sessions(id) ON DELETE CASCADE,
    item_id TEXT NOT NULL REFERENCES workspace_items(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    status TEXT NOT NULL,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_workspace_restore_attempts_session_id ON workspace_restore_attempts(session_id);
CREATE INDEX IF NOT EXISTS idx_workspace_restore_attempts_item_id ON workspace_restore_attempts(item_id);
