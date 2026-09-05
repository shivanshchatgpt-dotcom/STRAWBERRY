-- Migration 022: Generic Memory Hardening
-- Extends the unified memory system with:
--   * Interaction timestamps (last_viewed, last_copied, last_used, last_seen)
--   * Generic relationship graph
--   * Generic image memory metadata
--   * Generic credential memory (with secure secret storage)
--   * OCR queue
--   * Asset references for image/original files
--   * Document→memory block index
--
-- All schemas are GENERIC — no specific applications, services, or
-- user-data hard-coded. Names, types, and relationships are user-supplied.

-- ─── 022.1: Extend unified_memories with generic interaction timestamps ───
-- The base table (created in 017) already has created_at_ms, updated_at_ms,
-- occurred_at_ms. We add the interaction-timestamp family.

-- Phase 2: widen the memory_type CHECK to allow generic types like
-- 'credential', 'image', 'document', 'block', 'generic'. The original
-- 017 schema only allowed working/episodic/semantic/project/procedural.
-- We rebuild the table preserving all existing rows (SQLite has no ALTER
-- CHECK, so we must copy/recreate).
CREATE TABLE IF NOT EXISTS unified_memories_new (
    id              TEXT PRIMARY KEY,
    memory_type     TEXT NOT NULL CHECK(memory_type IN (
        'working','episodic','semantic','project','procedural',
        'credential','image','document','block','generic'
    )),
    title           TEXT NOT NULL,
    content         TEXT NOT NULL,
    source          TEXT NOT NULL,
    source_ref      TEXT,
    importance      TEXT NOT NULL DEFAULT 'medium' CHECK(importance IN ('low','medium','high','critical')),
    confidence      REAL NOT NULL DEFAULT 1.0,
    occurred_at_ms  INTEGER NOT NULL,
    created_at_ms   INTEGER NOT NULL,
    updated_at_ms   INTEGER NOT NULL,
    project_id      TEXT,
    session_id      TEXT,
    tags            TEXT,
    verified        INTEGER NOT NULL DEFAULT 0,
    stale           INTEGER NOT NULL DEFAULT 0,
    retention_days  INTEGER
);
INSERT OR IGNORE INTO unified_memories_new
    SELECT id, memory_type, title, content, source, source_ref,
           importance, confidence, occurred_at_ms, created_at_ms, updated_at_ms,
           project_id, session_id, tags, verified, stale, retention_days
    FROM unified_memories;
DROP TABLE IF EXISTS unified_memories;
ALTER TABLE unified_memories_new RENAME TO unified_memories;

-- Recreate the original indexes (022 adds new ones below).
CREATE INDEX IF NOT EXISTS idx_mem_type ON unified_memories(memory_type);
CREATE INDEX IF NOT EXISTS idx_mem_source ON unified_memories(source);
CREATE INDEX IF NOT EXISTS idx_mem_importance ON unified_memories(importance);
CREATE INDEX IF NOT EXISTS idx_mem_project ON unified_memories(project_id);
CREATE INDEX IF NOT EXISTS idx_mem_session ON unified_memories(session_id);
CREATE INDEX IF NOT EXISTS idx_mem_stale ON unified_memories(stale);
CREATE INDEX IF NOT EXISTS idx_mem_occurred ON unified_memories(occurred_at_ms);
CREATE INDEX IF NOT EXISTS idx_mem_tags ON unified_memories(tags);

ALTER TABLE unified_memories ADD COLUMN first_seen_at_ms   INTEGER;
ALTER TABLE unified_memories ADD COLUMN last_seen_at_ms    INTEGER;
ALTER TABLE unified_memories ADD COLUMN last_viewed_at_ms  INTEGER;
ALTER TABLE unified_memories ADD COLUMN last_copied_at_ms  INTEGER;
ALTER TABLE unified_memories ADD COLUMN last_used_at_ms    INTEGER;
ALTER TABLE unified_memories ADD COLUMN view_count        INTEGER NOT NULL DEFAULT 0;
ALTER TABLE unified_memories ADD COLUMN copy_count        INTEGER NOT NULL DEFAULT 0;
ALTER TABLE unified_memories ADD COLUMN use_count         INTEGER NOT NULL DEFAULT 0;
ALTER TABLE unified_memories ADD COLUMN sensitivity       INTEGER NOT NULL DEFAULT 1;
ALTER TABLE unified_memories ADD COLUMN privacy_level     TEXT NOT NULL DEFAULT 'normal'
    CHECK(privacy_level IN ('public','normal','sensitive','private','secret'));
ALTER TABLE unified_memories ADD COLUMN redaction_state   TEXT NOT NULL DEFAULT 'none'
    CHECK(redaction_state IN ('none','redacted','blocked'));
ALTER TABLE unified_memories ADD COLUMN content_hash      TEXT;
ALTER TABLE unified_memories ADD COLUMN app_state         TEXT NOT NULL DEFAULT 'active'
    CHECK(app_state IN ('active','stale','deleted','archived'));
ALTER TABLE unified_memories ADD COLUMN source_application TEXT;
ALTER TABLE unified_memories ADD COLUMN source_window     TEXT;
ALTER TABLE unified_memories ADD COLUMN source_workspace  TEXT;
ALTER TABLE unified_memories ADD COLUMN source_file       TEXT;
ALTER TABLE unified_memories ADD COLUMN source_url        TEXT;
ALTER TABLE unified_memories ADD COLUMN source_session    TEXT;
ALTER TABLE unified_memories ADD COLUMN category          TEXT;
ALTER TABLE unified_memories ADD COLUMN parent_id        TEXT REFERENCES unified_memories(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_mem_last_seen    ON unified_memories(last_seen_at_ms);
CREATE INDEX IF NOT EXISTS idx_mem_last_used    ON unified_memories(last_used_at_ms);
CREATE INDEX IF NOT EXISTS idx_mem_privacy      ON unified_memories(privacy_level);
CREATE INDEX IF NOT EXISTS idx_mem_app          ON unified_memories(source_application);
CREATE INDEX IF NOT EXISTS idx_mem_parent       ON unified_memories(parent_id);
CREATE INDEX IF NOT EXISTS idx_mem_content_hash ON unified_memories(content_hash);

-- ─── 022.2: Generic relationship graph ───
-- Relationships are evidence-backed, between generic memory IDs.
-- type is one of a fixed enum of generic relationship classes.
-- (See memory::relationship module for the canonical list.)
CREATE TABLE IF NOT EXISTS memory_relationships (
    id          TEXT PRIMARY KEY,
    from_id     TEXT NOT NULL REFERENCES unified_memories(id) ON DELETE CASCADE,
    to_id       TEXT NOT NULL REFERENCES unified_memories(id) ON DELETE CASCADE,
    rel_type    TEXT NOT NULL CHECK(rel_type IN (
        'related_to','belongs_to','created_from','copied_from','derived_from',
        'source_for','screenshot_of','captured_during','attached_to',
        'references','part_of','produced_by','used_with','contains',
        'parent_of','child_of','derived_relationship'
    )),
    confidence  REAL NOT NULL DEFAULT 0.5,
    evidence    TEXT,                -- JSON describing WHY the relationship exists
    observed    INTEGER NOT NULL DEFAULT 1,  -- 1 = observed, 0 = inferred/advisory
    created_at_ms INTEGER NOT NULL,
    UNIQUE(from_id, to_id, rel_type)
);

CREATE INDEX IF NOT EXISTS idx_rel_from ON memory_relationships(from_id);
CREATE INDEX IF NOT EXISTS idx_rel_to   ON memory_relationships(to_id);
CREATE INDEX IF NOT EXISTS idx_rel_type ON memory_relationships(rel_type);

-- ─── 022.3: Generic image memory (assets) ───
-- The image BLOB lives in the filesystem (see storage::files).
-- This table holds metadata, OCR text, and search-only fields.
CREATE TABLE IF NOT EXISTS image_assets (
    id              TEXT PRIMARY KEY,
    memory_id       TEXT REFERENCES unified_memories(id) ON DELETE CASCADE,
    original_path   TEXT NOT NULL,
    thumbnail_path  TEXT,
    mime_type       TEXT,
    width           INTEGER,
    height          INTEGER,
    byte_size       INTEGER,
    caption         TEXT,
    source_app      TEXT,
    source_window   TEXT,
    source_project  TEXT,
    ocr_text        TEXT,           -- raw OCR; not FTS-indexed until cleaned
    ocr_status      TEXT NOT NULL DEFAULT 'pending'
        CHECK(ocr_status IN ('pending','queued','running','done','failed','unavailable','skipped')),
    ocr_completed_at_ms INTEGER,
    thumbnail_status TEXT NOT NULL DEFAULT 'pending'
        CHECK(thumbnail_status IN ('pending','queued','running','done','failed','unavailable')),
    thumbnail_completed_at_ms INTEGER,
    privacy_blocked INTEGER NOT NULL DEFAULT 0,
    created_at_ms   INTEGER NOT NULL,
    updated_at_ms   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_img_memory ON image_assets(memory_id);
CREATE INDEX IF NOT EXISTS idx_img_ocr_status ON image_assets(ocr_status);
CREATE INDEX IF NOT EXISTS idx_img_thumb_status ON image_assets(thumbnail_status);
CREATE INDEX IF NOT EXISTS idx_img_source_app ON image_assets(source_app);

-- Image OCR FTS — separate from chat_fts so reindexing is independent.
-- This is a STANDARD FTS5 table (not contentless) so it can be searched
-- even if the source row is updated. The OCR provider is responsible
-- for NOT inserting the raw secret content here.
CREATE VIRTUAL TABLE IF NOT EXISTS image_ocr_fts USING fts5(
    image_id UNINDEXED,
    ocr_text,
    tokenize = 'porter unicode61'
);

-- ─── 022.4: Generic credential memory ───
-- A credential is a memory of type 'credential' that contains a secret.
-- The SECRET itself is stored in a separate table (credential_secrets) which
-- is NEVER FTS-indexed and NEVER appears in normal search results.
-- Searches find the credential METADATA (title, service, account, project)
-- but the secret stays protected until the user explicitly requests reveal/copy.
CREATE TABLE IF NOT EXISTS credentials (
    id              TEXT PRIMARY KEY REFERENCES unified_memories(id) ON DELETE CASCADE,
    service         TEXT NOT NULL,   -- e.g. "ExampleService" (user-supplied)
    account         TEXT,            -- e.g. "ExampleAccount"
    username        TEXT,
    environment     TEXT,            -- e.g. "production", "staging", "dev"
    host            TEXT,            -- e.g. server, device, host
    project         TEXT,            -- e.g. project name
    url             TEXT,            -- optional URL
    notes           TEXT,            -- non-secret notes
    -- "secure" storage: store the secret in a separate, non-FTS, non-search
    -- table. The application layer is responsible for using the strongest
    -- local encryption available; for now we store as opaque ciphertext-like
    -- bytes (the caller is expected to encrypt before insert in production).
    secret_ciphertext BLOB,
    secret_nonce      BLOB,
    secret_set        INTEGER NOT NULL DEFAULT 0,
    last_used_at_ms    INTEGER,
    created_at_ms      INTEGER NOT NULL,
    updated_at_ms      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cred_service ON credentials(service);
CREATE INDEX IF NOT EXISTS idx_cred_account  ON credentials(account);
CREATE INDEX IF NOT EXISTS idx_cred_project  ON credentials(project);

-- Credential metadata FTS — explicitly does NOT include any secret field.
CREATE VIRTUAL TABLE IF NOT EXISTS credential_fts USING fts5(
    credential_id UNINDEXED,
    service,
    account,
    username,
    environment,
    host,
    project,
    notes,
    tokenize = 'porter unicode61'
);

-- ─── 022.5: Document block memory index ───
-- DOCX documents are stored in the docx_documents / docx_blocks tables
-- (created in 021). This table indexes blocks into the unified memory
-- graph so DOCX content participates in unified search.
CREATE TABLE IF NOT EXISTS doc_block_memory (
    id          TEXT PRIMARY KEY,
    block_id    TEXT NOT NULL,
    document_id TEXT NOT NULL,
    memory_id   TEXT REFERENCES unified_memories(id) ON DELETE CASCADE,
    block_type  TEXT,           -- text|table|formula|chart|tree|image|code|...
    created_at_ms INTEGER NOT NULL,
    UNIQUE(block_id, memory_id)
);

CREATE INDEX IF NOT EXISTS idx_doc_block_doc ON doc_block_memory(document_id);
CREATE INDEX IF NOT EXISTS idx_doc_block_mem ON doc_block_memory(memory_id);

-- ─── 022.6: Generic asset storage directory index ───
-- For listing/search of filesystem assets (images, docx, files) by
-- their owning memory, project, or session.
CREATE TABLE IF NOT EXISTS memory_assets (
    id              TEXT PRIMARY KEY,
    memory_id       TEXT REFERENCES unified_memories(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK(kind IN ('image','document','thumbnail','attachment','export','original')),
    path            TEXT NOT NULL,
    mime_type       TEXT,
    byte_size       INTEGER,
    created_at_ms   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_asset_memory ON memory_assets(memory_id);
CREATE INDEX IF NOT EXISTS idx_asset_kind   ON memory_assets(kind);

-- ─── 022.7: DocBlock FTS (searchable text/table/formula content) ───
-- A separate FTS table for document block text — independent of chat_fts
-- and image_ocr_fts so reindexing of one doesn't rebuild the others.
CREATE VIRTUAL TABLE IF NOT EXISTS doc_block_fts USING fts5(
    block_id    UNINDEXED,
    document_id UNINDEXED,
    block_type  UNINDEXED,
    text,
    tokenize = 'porter unicode61'
);
