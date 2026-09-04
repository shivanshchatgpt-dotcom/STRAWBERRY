-- 📄 DOCX workspace — offline block-document storage.
--
-- Documents are JSON block arrays (Strawberry's own block model, NOT .docx
-- files). plain_text is a derived search projection rebuilt on save.

CREATE TABLE IF NOT EXISTS docx_documents (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    blocks_json TEXT NOT NULL DEFAULT '[]',
    plain_text  TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_docx_updated ON docx_documents(updated_at DESC);

CREATE VIRTUAL TABLE IF NOT EXISTS docx_fts USING fts5(
    title,
    plain_text,
    content='docx_documents',
    content_rowid='rowid'
);
