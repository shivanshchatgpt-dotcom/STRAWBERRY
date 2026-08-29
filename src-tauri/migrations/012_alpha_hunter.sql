-- 009_alpha_hunter.sql
-- 🎯 Alpha Hunter: candidates for free AI models / free tiers detected
-- from legitimate public sources (HackerNews, Reddit, RSS, OpenRouter).
-- Deterministic keyword detection, no LLM. Verification happens on-demand
-- via a real OpenAI-compatible test call. Network use is opt-in behind
-- app_meta key 'alpha_hunter_enabled'.

CREATE TABLE IF NOT EXISTS alpha_candidates (
    id          TEXT PRIMARY KEY,
    source      TEXT NOT NULL,               -- hackernews | reddit | rss | openrouter
    title       TEXT NOT NULL,
    url         TEXT,
    provider    TEXT,                        -- detected provider hint (e.g. 'tokenrouter')
    model_id    TEXT,                        -- detected model hint (e.g. 'qwen/qwen3.8-max-free')
    base_url    TEXT,                        -- detected endpoint hint (e.g. 'https://api.x.com/v1')
    status      TEXT NOT NULL DEFAULT 'new'
                CHECK(status IN ('new','verified','failed','dismissed')),
    score       INTEGER NOT NULL DEFAULT 0,  -- detector confidence
    detected_at TEXT NOT NULL,
    verified_at TEXT,
    notes       TEXT                         -- verify result / error details
);

CREATE INDEX IF NOT EXISTS idx_alpha_candidates_status
    ON alpha_candidates(status, score DESC, detected_at DESC);
