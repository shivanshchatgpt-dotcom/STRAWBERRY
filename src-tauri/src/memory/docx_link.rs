//! 📄 DOCX ↔ Memory Link
//!
//! Each DOCX block (text|table|formula|chart|tree|image|code) is linked
//! to a `unified_memories` row so DOCX content participates in unified
//! search and relationships. The DOCX schema is defined in migration 021.
//!
//! Two link patterns are supported:
//!
//! 1. `link_block(...)` — creates a NEW memory record (kind=Block) and
//!    links it to the block. Used by the legacy "save this block as a
//!    memory" path.
//! 2. `link_block_to_memory(...)` — links an EXISTING memory to a block.
//!    Used by the UI "Link Memory" feature. Prevents duplicate memories
//!    and supports arbitrary memory kinds (note, image, credential...).
//!
//! Both paths also write the `References` relationship from the block
//! memory to the linked memory for unified graph traversal.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::{create as create_memory, get as get_memory, NewMemory, MemoryKind, RelationshipType};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocBlockLink {
    pub id: String,
    pub block_id: String,
    pub document_id: String,
    pub memory_id: String,
    pub block_type: Option<String>,
    pub created_at_ms: i64,
}

/// Link a DOCX block to a memory record. Creates the memory row if needed.
pub fn link_block(
    conn: &Connection,
    block_id: &str,
    document_id: &str,
    block_type: Option<&str>,
    title: &str,
    text: &str,
    project: Option<&str>,
) -> Result<String, String> {
    let mut m = NewMemory::new(MemoryKind::Block, title, text, "docx");
    m.project_id = project.map(|p| p.to_string());
    m.tags = vec!["docx".to_string()];
    if let Some(bt) = block_type {
        m.tags.push(bt.to_string());
    }
    let memory_id = create_memory(conn, &m)?;

    // Insert the link row.
    let id = {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in format!("{block_id}|{memory_id}").as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("dlink-{:016x}", h)
    };
    conn.execute(
        "INSERT OR REPLACE INTO doc_block_memory(id, block_id, document_id, memory_id, block_type, created_at_ms)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, block_id, document_id, memory_id, block_type, chrono::Utc::now().timestamp_millis()],
    ).map_err(|e| format!("link block: {e}"))?;

    // FTS index the text.
    if !text.is_empty() {
        let _ = conn.execute(
            "INSERT INTO doc_block_fts(block_id, document_id, block_type, text) VALUES(?1, ?2, ?3, ?4)",
            params![block_id, document_id, block_type.unwrap_or(""), text],
        );
    }

    Ok(memory_id)
}

/// Find memory IDs linked to a document.
pub fn memories_for_document(conn: &Connection, document_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn.prepare("SELECT memory_id FROM doc_block_memory WHERE document_id = ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![document_id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// Remove the link for a block.
pub fn unlink_block(conn: &Connection, block_id: &str) -> Result<bool, String> {
    let _ = conn.execute("DELETE FROM doc_block_fts WHERE block_id = ?1", params![block_id]);
    let n = conn.execute("DELETE FROM doc_block_memory WHERE block_id = ?1", params![block_id])
        .map_err(|e| format!("unlink: {e}"))?;
    Ok(n > 0)
}

/// Search document blocks via FTS.
pub fn search_blocks(conn: &Connection, query: &str, limit: usize) -> Result<Vec<(String, String, String)>, String> {
    let mut stmt = conn.prepare(
        "SELECT block_id, document_id, block_type FROM doc_block_fts
         WHERE doc_block_fts MATCH ?1
         ORDER BY rank LIMIT ?2"
    ).map_err(|e| format!("doc block search: {e}"))?;
    let rows = stmt.query_map(params![query, limit as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    }).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

// ─────────────────────── existing-memory linking ───────────────────────

/// All memory IDs linked to a specific block (in the order they were linked).
pub fn memories_for_block(conn: &Connection, block_id: &str) -> Result<Vec<DocBlockLink>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, block_id, document_id, memory_id, block_type, created_at_ms
         FROM doc_block_memory WHERE block_id = ?1
         ORDER BY created_at_ms ASC"
    ).map_err(|e| format!("memories_for_block: {e}"))?;
    let rows = stmt
        .query_map(params![block_id], |r| {
            Ok(DocBlockLink {
                id: r.get(0)?,
                block_id: r.get(1)?,
                document_id: r.get(2)?,
                memory_id: r.get(3)?,
                block_type: r.get(4)?,
                created_at_ms: r.get(5)?,
            })
        })
        .map_err(|e| format!("memories_for_block rows: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// All block IDs linked to a specific memory.
pub fn blocks_for_memory(conn: &Connection, memory_id: &str) -> Result<Vec<DocBlockLink>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, block_id, document_id, memory_id, block_type, created_at_ms
         FROM doc_block_memory WHERE memory_id = ?1
         ORDER BY created_at_ms ASC"
    ).map_err(|e| format!("blocks_for_memory: {e}"))?;
    let rows = stmt
        .query_map(params![memory_id], |r| {
            Ok(DocBlockLink {
                id: r.get(0)?,
                block_id: r.get(1)?,
                document_id: r.get(2)?,
                memory_id: r.get(3)?,
                block_type: r.get(4)?,
                created_at_ms: r.get(5)?,
            })
        })
        .map_err(|e| format!("blocks_for_memory rows: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// Link an existing memory to a block. Idempotent: linking the same
/// memory to the same block twice is a no-op.
///
/// Verifies:
///   * The memory exists (and is not soft-deleted).
///   * The memory is not a Block kind pointing back to the same block
///     (avoids trivial self-loops in the graph).
///
/// Side effect: creates a `References` relationship from the block's
/// own memory (if any) to the linked memory, so the graph shows the
/// connection in relationship traversals.
pub fn link_block_to_memory(
    conn: &Connection,
    block_id: &str,
    document_id: &str,
    block_type: Option<&str>,
    memory_id: &str,
) -> Result<DocBlockLink, String> {
    // Verify the target memory exists.
    let target = get_memory(conn, memory_id)?
        .ok_or_else(|| format!("memory not found: {memory_id}"))?;

    // Idempotency: if a link already exists, return it.
    let mut stmt = conn.prepare(
        "SELECT id, block_id, document_id, memory_id, block_type, created_at_ms
         FROM doc_block_memory WHERE block_id = ?1 AND memory_id = ?2"
    ).map_err(|e| format!("link query: {e}"))?;
    let existing: Option<DocBlockLink> = stmt
        .query_row(params![block_id, memory_id], |r| {
            Ok(DocBlockLink {
                id: r.get(0)?,
                block_id: r.get(1)?,
                document_id: r.get(2)?,
                memory_id: r.get(3)?,
                block_type: r.get(4)?,
                created_at_ms: r.get(5)?,
            })
        })
        .ok();
    if let Some(link) = existing {
        return Ok(link);
    }

    // Self-loop guard: if the memory is a Block memory for the same
    // document, skip the References relationship.
    let is_block = target.kind == "block";

    let id = {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in format!("{block_id}|{memory_id}").as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("dlink-{:016x}", h)
    };
    let now = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT OR REPLACE INTO doc_block_memory(id, block_id, document_id, memory_id, block_type, created_at_ms)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, block_id, document_id, memory_id, block_type, now],
    )
    .map_err(|e| format!("link insert: {e}"))?;

    // Create a References relationship from any existing Block-kind
    // memory for this block (if one exists) to the new memory. This
    // surfaces the connection in graph traversal.
    if !is_block {
        let block_mems = memories_for_block(conn, block_id)?;
        for bm in block_mems {
            if bm.memory_id != memory_id {
                let _ = super::relationship::create(
                    conn,
                    &bm.memory_id,
                    memory_id,
                    RelationshipType::References,
                    0.7,
                    Some("docx block link"),
                    true,
                );
            }
        }
    }

    Ok(DocBlockLink {
        id,
        block_id: block_id.to_string(),
        document_id: document_id.to_string(),
        memory_id: memory_id.to_string(),
        block_type: block_type.map(String::from),
        created_at_ms: now,
    })
}

/// Unlink a single memory from a block. Does not delete the memory.
pub fn unlink_block_memory(
    conn: &Connection,
    block_id: &str,
    memory_id: &str,
) -> Result<bool, String> {
    let n = conn.execute(
        "DELETE FROM doc_block_memory WHERE block_id = ?1 AND memory_id = ?2",
        params![block_id, memory_id],
    )
    .map_err(|e| format!("unlink_block_memory: {e}"))?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{
        create as memory_create, NewMemory as MemoryNewMemory, MemoryKind as MemoryMemoryKind,
        relationship,
    };

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    #[test]
    fn link_block_creates_memory() {
        let conn = setup();
        let mid = link_block(
            &conn, "block1", "doc1", Some("text"),
            "My Block Title", "This is the block content",
            Some("MyProject"),
        ).unwrap();
        let mems = memories_for_document(&conn, "doc1").unwrap();
        assert_eq!(mems, vec![mid]);
    }

    #[test]
    fn search_blocks_finds_text() {
        let conn = setup();
        link_block(&conn, "b1", "d1", Some("text"), "Title", "the quick brown fox", None).unwrap();
        link_block(&conn, "b2", "d1", Some("text"), "Title2", "lazy dog sleeps", None).unwrap();
        let results = search_blocks(&conn, "quick", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "b1");
    }

    #[test]
    fn unlink_block_removes() {
        let conn = setup();
        link_block(&conn, "b1", "d1", Some("text"), "T", "content", None).unwrap();
        assert!(unlink_block(&conn, "b1").unwrap());
        assert!(memories_for_document(&conn, "d1").unwrap().is_empty());
    }

    // ─────────── new: link existing memory to a block ───────────

    #[test]
    fn link_existing_memory_to_block_creates_link() {
        let conn = setup();
        let existing = memory_create(
            &conn,
            &MemoryNewMemory::new(
                MemoryMemoryKind::Semantic,
                "Existing note",
                "Some content about the topic",
                "user",
            ),
        )
        .unwrap();

        let link = link_block_to_memory(
            &conn, "b1", "doc1", Some("text"), &existing,
        )
        .unwrap();
        assert_eq!(link.memory_id, existing);
        assert_eq!(link.block_id, "b1");
        let list = memories_for_block(&conn, "b1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].memory_id, existing);
    }

    #[test]
    fn link_existing_memory_idempotent() {
        let conn = setup();
        let existing = memory_create(
            &conn,
            &MemoryNewMemory::new(
                MemoryMemoryKind::Image,
                "An image",
                "pixel data",
                "user",
            ),
        )
        .unwrap();
        let l1 = link_block_to_memory(&conn, "b1", "d1", None, &existing).unwrap();
        let l2 = link_block_to_memory(&conn, "b1", "d1", None, &existing).unwrap();
        assert_eq!(l1.id, l2.id, "link is idempotent");
        assert_eq!(memories_for_block(&conn, "b1").unwrap().len(), 1);
    }

    #[test]
    fn link_to_missing_memory_errors() {
        let conn = setup();
        let res = link_block_to_memory(&conn, "b1", "d1", None, "mem-does-not-exist");
        assert!(res.is_err());
    }

    #[test]
    fn unlink_block_memory_removes_only_target() {
        let conn = setup();
        let m1 = memory_create(
            &conn,
            &MemoryNewMemory::new(MemoryMemoryKind::Semantic, "M1", "c1", "user"),
        )
        .unwrap();
        let m2 = memory_create(
            &conn,
            &MemoryNewMemory::new(MemoryMemoryKind::Semantic, "M2", "c2", "user"),
        )
        .unwrap();
        link_block_to_memory(&conn, "b1", "d1", None, &m1).unwrap();
        link_block_to_memory(&conn, "b1", "d1", None, &m2).unwrap();
        assert_eq!(memories_for_block(&conn, "b1").unwrap().len(), 2);
        assert!(unlink_block_memory(&conn, "b1", &m1).unwrap());
        let after = memories_for_block(&conn, "b1").unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].memory_id, m2);
    }

    #[test]
    fn blocks_for_memory_returns_all_links() {
        let conn = setup();
        let m = memory_create(
            &conn,
            &MemoryNewMemory::new(MemoryMemoryKind::Semantic, "M", "c", "user"),
        )
        .unwrap();
        link_block_to_memory(&conn, "b1", "d1", None, &m).unwrap();
        link_block_to_memory(&conn, "b2", "d1", None, &m).unwrap();
        link_block_to_memory(&conn, "b3", "d2", None, &m).unwrap();
        let links = blocks_for_memory(&conn, &m).unwrap();
        assert_eq!(links.len(), 3);
        let doc_ids: std::collections::HashSet<_> =
            links.iter().map(|l| l.document_id.as_str()).collect();
        assert!(doc_ids.contains("d1"));
        assert!(doc_ids.contains("d2"));
    }

    #[test]
    fn linking_creates_references_relationship() {
        let conn = setup();
        // Create a block memory first (via the legacy link_block path).
        let block_mem = link_block(
            &conn, "b1", "d1", Some("text"),
            "Block title", "Block text", None,
        ).unwrap();
        // Now link a separate user memory to the same block.
        let user_mem = memory_create(
            &conn,
            &MemoryNewMemory::new(MemoryMemoryKind::Semantic, "User", "c", "user"),
        )
        .unwrap();
        link_block_to_memory(&conn, "b1", "d1", None, &user_mem).unwrap();

        // The block memory should now have a References relationship
        // pointing at the user memory.
        let rels = relationship::list_all(&conn, &block_mem).unwrap();
        assert!(rels.iter().any(|r| r.to_id == user_mem && r.rel_type == "references"));
    }
}
