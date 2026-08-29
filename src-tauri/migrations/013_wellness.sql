-- Wellness Agent — owner health nudges
CREATE TABLE IF NOT EXISTS wellness_config (
    category TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1,
    interval_minutes INTEGER NOT NULL DEFAULT 10,
    last_reminded_at TEXT
);

CREATE TABLE IF NOT EXISTS wellness_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    source TEXT NOT NULL
);
