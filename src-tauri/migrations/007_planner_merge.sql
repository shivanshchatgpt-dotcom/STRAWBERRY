-- Migration 007: Planner merge round 2 (anime-planner features).
-- focus_sessions already exists (002); add the stopwatch/timer kind column
-- and an optional habit link for tracked focus sessions.

ALTER TABLE focus_sessions ADD COLUMN kind TEXT NOT NULL DEFAULT 'timer';
ALTER TABLE focus_sessions ADD COLUMN habit_id INTEGER REFERENCES habits(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_focus_completed ON focus_sessions(completed_at DESC);
