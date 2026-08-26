-- 006_work_snapshots.sql
-- 🧠 Context Recall: one-click snapshots of the whole workspace
-- (open windows, browser tabs, recent web context) plus a generated
-- story so "load previous work" can answer kaha/kya/kyu.

CREATE TABLE IF NOT EXISTS work_snapshots (
    id          TEXT PRIMARY KEY,
    created_at  TEXT NOT NULL,
    active_app  TEXT,
    active_title TEXT,
    story_text  TEXT NOT NULL,
    raw_json    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_work_snapshots_created
    ON work_snapshots(created_at DESC);
