-- Migration 004: Context Resume — "kahan chhoda tha, wahin se continue".
-- A resume point is a lightweight bookmark of where the user left off in a
-- chat/task, plus extracted intent so the next session can pick up instantly.

CREATE TABLE IF NOT EXISTS chat_resume_points (
    id            TEXT PRIMARY KEY,
    chat_id       TEXT REFERENCES chats(id) ON DELETE CASCADE,
    -- What the user was doing (extracted goal line / first open question).
    intent        TEXT NOT NULL,
    -- Where exactly they stopped (last meaningful assistant/user exchange).
    last_exchange TEXT,
    -- Open items at pause time (action items not yet done).
    open_items    TEXT,               -- JSON array of strings
    -- Files/identifiers in play when paused.
    context_refs  TEXT,               -- JSON array of strings
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_resume_chat ON chat_resume_points(chat_id);

-- Cross-chat resume: same intent appearing across multiple chats/tasks.
CREATE TABLE IF NOT EXISTS cross_chat_suggestions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    intent_hash TEXT NOT NULL,        -- normalized-intent fingerprint
    chat_id     TEXT REFERENCES chats(id) ON DELETE CASCADE,
    snippet     TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_xchat_hash ON cross_chat_suggestions(intent_hash);

-- Resume points are also searchable via FTS5 (separate virtual table).
CREATE VIRTUAL TABLE IF NOT EXISTS resume_fts USING fts5(
    intent, last_exchange, content='chat_resume_points', content_rowid='rowid'
);
CREATE TRIGGER IF NOT EXISTS resume_fts_ai AFTER INSERT ON chat_resume_points BEGIN
    INSERT INTO resume_fts(rowid, intent, last_exchange)
    VALUES (new.rowid, coalesce(new.intent,''), coalesce(new.last_exchange,''));
END;
CREATE TRIGGER IF NOT EXISTS resume_fts_ad AFTER DELETE ON chat_resume_points BEGIN
    INSERT INTO resume_fts(resume_fts, rowid, intent, last_exchange)
    VALUES ('delete', old.rowid, coalesce(old.intent,''), coalesce(old.last_exchange,''));
END;
CREATE TRIGGER IF NOT EXISTS resume_fts_au AFTER UPDATE ON chat_resume_points BEGIN
    INSERT INTO resume_fts(resume_fts, rowid, intent, last_exchange)
    VALUES ('delete', old.rowid, coalesce(old.intent,''), coalesce(old.last_exchange,''));
    INSERT INTO resume_fts(rowid, intent, last_exchange)
    VALUES (new.rowid, coalesce(new.intent,''), coalesce(new.last_exchange,''));
END;

-- Feature 3 groundwork: browser tabs (extension-lite feeds these later).
CREATE TABLE IF NOT EXISTS tabs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    url        TEXT NOT NULL,
    title      TEXT,
    host       TEXT,
    visited_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX IF NOT EXISTS idx_tabs_url ON tabs(url);
CREATE INDEX IF NOT EXISTS idx_tabs_visited ON tabs(visited_at);

CREATE VIRTUAL TABLE IF NOT EXISTS tabs_fts USING fts5(
    url, title, content='tabs', content_rowid='rowid'
);
CREATE TRIGGER IF NOT EXISTS tabs_fts_ai AFTER INSERT ON tabs BEGIN
    INSERT INTO tabs_fts(rowid, url, title) VALUES (new.rowid, new.url, coalesce(new.title,''));
END;
CREATE TRIGGER IF NOT EXISTS tabs_fts_ad AFTER DELETE ON tabs BEGIN
    INSERT INTO tabs_fts(tabs_fts, rowid, url, title) VALUES ('delete', old.rowid, old.url, coalesce(old.title,''));
END;

-- Screen Memory groundwork kept out for now; lands with its own migration.
