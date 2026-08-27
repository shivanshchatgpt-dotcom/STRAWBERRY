-- 010_events_calendar.sql
-- Calendar events, reminders, and sources schema for Strawberry Planner v0.2

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    category TEXT NOT NULL DEFAULT 'general',
    source_url TEXT,
    location TEXT,
    is_all_day INTEGER NOT NULL DEFAULT 0,
    certificate_offered INTEGER NOT NULL DEFAULT 0,
    registration_required INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_start_at ON events(start_at);
CREATE INDEX IF NOT EXISTS idx_events_category ON events(category);

CREATE TABLE IF NOT EXISTS event_reminders (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    minutes_before INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    triggered INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_event_reminders_event_id ON event_reminders(event_id);

CREATE TABLE IF NOT EXISTS event_sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    feed_url TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_synced_at TEXT
);
