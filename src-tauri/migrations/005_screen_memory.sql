-- Screen Memory: screenshot capture with privacy & search
-- 003: Screen Memory tables

-- Screen frames: captured screenshots with metadata
CREATE TABLE IF NOT EXISTS screen_frames (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,                    -- capture timestamp (unix ms)
    app_name TEXT,                          -- active app name
    window_title TEXT,                      -- active window title
    file_path TEXT NOT NULL,                -- relative to strawberry_data_dir/screens/
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    byte_size INTEGER NOT NULL,
    perceptual_hash TEXT,                   -- 64-bit hex for near-dup detection
    ocr_text TEXT,                          -- extracted text via OCR
    embedding BLOB,                         -- 384-dim float32 vector (1536 bytes)
    is_blurred INTEGER DEFAULT 0,           -- privacy blur applied
    thumbnail_path TEXT,                    -- small preview for UI
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_screen_ts ON screen_frames(ts);
CREATE INDEX IF NOT EXISTS idx_screen_app ON screen_frames(app_name);
CREATE INDEX IF NOT EXISTS idx_screen_hash ON screen_frames(perceptual_hash);

-- Blocklist: apps / window title patterns to NEVER capture
CREATE TABLE IF NOT EXISTS screen_blocklist (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern TEXT NOT NULL,                  -- app name substring or window title regex
    added_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    reason TEXT                             -- 'banking' | 'passwords' | 'manual'
);

-- FTS5 for screen content search
CREATE VIRTUAL TABLE IF NOT EXISTS screen_fts USING fts5(
    ocr_text,
    app_name,
    window_title,
    content='screen_frames',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS screen_fts_ai AFTER INSERT ON screen_frames BEGIN
    INSERT INTO screen_fts(rowid, ocr_text, app_name, window_title)
    VALUES (new.rowid, coalesce(new.ocr_text,''), coalesce(new.app_name,''), coalesce(new.window_title,''));
END;

CREATE TRIGGER IF NOT EXISTS screen_fts_ad AFTER DELETE ON screen_frames BEGIN
    INSERT INTO screen_fts(screen_fts, rowid, ocr_text, app_name, window_title)
    VALUES ('delete', old.rowid, coalesce(old.ocr_text,''), coalesce(old.app_name,''), coalesce(old.window_title,''));
END;

CREATE TRIGGER IF NOT EXISTS screen_fts_au AFTER UPDATE ON screen_frames BEGIN
    INSERT INTO screen_fts(screen_fts, rowid, ocr_text, app_name, window_title)
    VALUES ('delete', old.rowid, coalesce(old.ocr_text,''), coalesce(old.app_name,''), coalesce(old.window_title,''));
    INSERT INTO screen_fts(rowid, ocr_text, app_name, window_title)
    VALUES (new.rowid, coalesce(new.ocr_text,''), coalesce(new.app_name,''), coalesce(new.window_title,''));
END;