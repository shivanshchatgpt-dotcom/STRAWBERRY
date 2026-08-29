-- Migration 011: The Strawberry Ghost — activity tracking & knowledge graph
-- Records all user activity, builds a knowledge graph, and surfaces insights.

CREATE TABLE IF NOT EXISTS ghost_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type  TEXT NOT NULL CHECK(event_type IN (
        'open_chat', 'open_folder', 'open_root', 'create_chat', 'create_folder',
        'search', 'capture', 'todo_add', 'todo_done', 'habit_done',
        'focus_session', 'tab_visit', 'note_view', 'inbox_add'
    )),
    source_id   TEXT,           -- chat_id, node_id, root_id, etc.
    source_kind TEXT,           -- 'chat', 'folder', 'root', 'search', etc.
    duration_ms INTEGER NOT NULL DEFAULT 0,
    metadata    TEXT,           -- JSON blob for extra context
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ghost_events_created ON ghost_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ghost_events_source  ON ghost_events(source_id);
CREATE INDEX IF NOT EXISTS idx_ghost_events_type    ON ghost_events(event_type);

-- Knowledge graph: nodes = discrete knowledge units (chats, folders, tags)
CREATE TABLE IF NOT EXISTS ghost_graph_nodes (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK(kind IN ('chat', 'folder', 'root', 'tag')),
    label       TEXT NOT NULL,
    weight      REAL NOT NULL DEFAULT 1.0,   -- importance
    color       TEXT,                        -- category color
    position_x  REAL,                        -- for layout (optional)
    position_y  REAL,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_graph_nodes_kind ON ghost_graph_nodes(kind);

-- Knowledge graph: edges = relationships (shared tags, co-access, temporal)
CREATE TABLE IF NOT EXISTS ghost_graph_edges (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id   TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    weight      REAL NOT NULL DEFAULT 1.0,
    edge_type   TEXT NOT NULL CHECK(edge_type IN (
        'shared_tag', 'co_access', 'parent_child', 'temporal', 'semantic'
    )),
    created_at  TEXT NOT NULL,
    UNIQUE(source_id, target_id, edge_type)
);

CREATE INDEX IF NOT EXISTS idx_graph_edges_source ON ghost_graph_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target ON ghost_graph_edges(target_id);

-- Generated insights (serendipities, connections, "I noticed...")
CREATE TABLE IF NOT EXISTS ghost_insights (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL CHECK(kind IN (
        'serendipity', 'pattern', 'resurface', 'cluster', 'warning', 'achievement'
    )),
    title       TEXT NOT NULL,
    body        TEXT NOT NULL,
    source_ids  TEXT,           -- JSON array of related source_ids
    score       REAL NOT NULL DEFAULT 0.5,
    seen        INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ghost_insights_created ON ghost_insights(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ghost_insights_score   ON ghost_insights(score DESC);
