-- Migration 009: 🧠 Ambient Memory — continuous OS-independent event fabric & deterministic symbolic graph.

CREATE TABLE IF NOT EXISTS ambient_events (
    id          TEXT PRIMARY KEY,
    event_type  TEXT NOT NULL, -- 'clip', 'screen', 'file_edit', 'system_snapshot', 'symbolic_ast'
    title       TEXT NOT NULL,
    summary     TEXT NOT NULL,
    source_app  TEXT,
    metadata    TEXT, -- JSON metadata payload
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ambient_events_created ON ambient_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ambient_events_type ON ambient_events(event_type);

CREATE TABLE IF NOT EXISTS ambient_relations (
    id          TEXT PRIMARY KEY,
    source_id   TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    relation    TEXT NOT NULL, -- 'imports', 'calls', 'references', 'produced_by'
    weight      REAL NOT NULL DEFAULT 1.0,
    created_at  TEXT NOT NULL,
    FOREIGN KEY(source_id) REFERENCES ambient_events(id) ON DELETE CASCADE,
    FOREIGN KEY(target_id) REFERENCES ambient_events(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ambient_relations_source ON ambient_relations(source_id);
CREATE INDEX IF NOT EXISTS idx_ambient_relations_target ON ambient_relations(target_id);
