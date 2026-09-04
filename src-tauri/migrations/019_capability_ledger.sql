-- 🤖 Phase 6 — Capability Registry + Action Ledger.
--
-- capability_state: per-capability user overrides on top of the static
-- manifest in src/autonomous/capability.rs. The manifest is code; this table
-- only stores what the user (or the adaptive engine, with a logged reason)
-- changed. Empty table == pure manifest defaults.
--
-- autonomy_decisions: the Action Ledger. Every run/skip/defer/pause decision
-- and every risk-gated action is appended here — WHO/WHAT/WHY/WHEN/RESULT —
-- so the agent stays auditable ("explainability" requirement).

CREATE TABLE IF NOT EXISTS capability_state (
    capability_id   TEXT PRIMARY KEY,           -- matches manifest id
    enabled         INTEGER NOT NULL DEFAULT 1, -- user/adaptive override
    interval_secs   INTEGER,                    -- NULL => manifest default
    changed_reason  TEXT,                       -- why the override happened
    changed_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_capability_state_id ON capability_state(capability_id);

-- Append-only decision ledger.
CREATE TABLE IF NOT EXISTS autonomy_decisions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    capability_id   TEXT NOT NULL,              -- '' for global decisions
    decision        TEXT NOT NULL,               -- run|skip|defer|pause|resume|block|ask|fail
    reason          TEXT NOT NULL,              -- human-readable, no secrets
    score           REAL,                       -- run_score when applicable
    details         TEXT,                       -- optional JSON context
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_decisions_created ON autonomy_decisions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_decisions_capability ON autonomy_decisions(capability_id);
