-- Migration 017: Unified Temporal Memory.
-- Provides the durable knowledge layer that unifies memory across all sources.

CREATE TABLE IF NOT EXISTS unified_memories (
    id              TEXT PRIMARY KEY,
    memory_type     TEXT NOT NULL CHECK(memory_type IN ('working','episodic','semantic','project','procedural')),
    title           TEXT NOT NULL,
    content         TEXT NOT NULL,
    source          TEXT NOT NULL,
    source_ref      TEXT,
    importance      TEXT NOT NULL DEFAULT 'medium' CHECK(importance IN ('low','medium','high','critical')),
    confidence      REAL NOT NULL DEFAULT 1.0,
    occurred_at_ms  INTEGER NOT NULL,
    created_at_ms   INTEGER NOT NULL,
    updated_at_ms   INTEGER NOT NULL,
    project_id      TEXT,
    session_id      TEXT,
    tags            TEXT,
    verified        INTEGER NOT NULL DEFAULT 0,
    stale           INTEGER NOT NULL DEFAULT 0,
    retention_days  INTEGER
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_mem_type ON unified_memories(memory_type);
CREATE INDEX IF NOT EXISTS idx_mem_source ON unified_memories(source);
CREATE INDEX IF NOT EXISTS idx_mem_importance ON unified_memories(importance);
CREATE INDEX IF NOT EXISTS idx_mem_project ON unified_memories(project_id);
CREATE INDEX IF NOT EXISTS idx_mem_session ON unified_memories(session_id);
CREATE INDEX IF NOT EXISTS idx_mem_stale ON unified_memories(stale);
CREATE INDEX IF NOT EXISTS idx_mem_occurred ON unified_memories(occurred_at_ms);
CREATE INDEX IF NOT EXISTS idx_mem_tags ON unified_memories(tags);
