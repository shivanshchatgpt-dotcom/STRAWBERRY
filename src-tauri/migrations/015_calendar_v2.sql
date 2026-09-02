-- Migration 015: Advanced Calendar v2.
-- Adds recurrence + color support to the existing events table (010).

ALTER TABLE events ADD COLUMN recurrence TEXT NOT NULL DEFAULT 'none'
    CHECK(recurrence IN ('none','daily','weekly','monthly','yearly'));
ALTER TABLE events ADD COLUMN recurrence_end TEXT;
ALTER TABLE events ADD COLUMN color TEXT;

CREATE INDEX IF NOT EXISTS idx_events_start ON events(start_at);
