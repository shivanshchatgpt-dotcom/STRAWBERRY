-- Migration 003: negative knowledge + verbatim identifiers.
--
-- Adds three artifact types used by the AI-to-AI handoff packet:
--   rejected   — an approach that was tried and abandoned, with its reason
--   constraint — a hard rule the receiving AI must not violate
--   identifier — env var / port / table / fn / version, kept verbatim
--
-- SQLite cannot alter a CHECK constraint in place, so the table is rebuilt.
-- Rows are copied first and the FTS index is untouched (it indexes `chats`,
-- not `chat_artifacts`), so this migration is data-preserving.

CREATE TABLE chat_artifacts_new (
    id            TEXT PRIMARY KEY,
    chat_id       TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL CHECK(artifact_type IN (
        'code', 'error', 'command', 'url', 'decision',
        'action_item', 'heading', 'question', 'answer',
        'rejected', 'constraint', 'identifier'
    )),
    content       TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

INSERT INTO chat_artifacts_new (id, chat_id, artifact_type, content, created_at)
SELECT id, chat_id, artifact_type, content, created_at FROM chat_artifacts;

DROP TABLE chat_artifacts;

ALTER TABLE chat_artifacts_new RENAME TO chat_artifacts;

CREATE INDEX IF NOT EXISTS idx_chat_artifacts_chat_id ON chat_artifacts(chat_id);
CREATE INDEX IF NOT EXISTS idx_chat_artifacts_type ON chat_artifacts(artifact_type);
