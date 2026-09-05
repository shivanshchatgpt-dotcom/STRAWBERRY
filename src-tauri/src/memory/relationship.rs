//! 🔗 Generic Relationship Graph
//!
//! Relationships are between arbitrary memory IDs. The 17 relationship
//! types are a fixed enum — no arbitrary strings. Relationships are
//! evidence-backed: the `evidence` field describes WHY the relationship
//! exists (project, session, source, etc.).
//!
//! Observed vs derived: `observed=1` means a real event linked the two
//! (e.g. user attached a memory to a task); `observed=0` means an
//! inference (e.g. a model suggested they are related).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use super::{RelationshipType};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub rel_type: String,
    pub confidence: f32,
    pub evidence: Option<String>,
    pub observed: bool,
    pub created_at_ms: i64,
}

pub fn create(
    conn: &Connection,
    from_id: &str,
    to_id: &str,
    rel: RelationshipType,
    confidence: f32,
    evidence: Option<&str>,
    observed: bool,
) -> Result<String, String> {
    if from_id == to_id {
        return Err("cannot relate a memory to itself".into());
    }
    let now_ms = chrono::Utc::now().timestamp_millis();
    // Content-derived stable id.
    let id = {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in format!("{from_id}|{to_id}|{}", rel.as_str()).as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("rel-{:016x}", h)
    };
    conn.execute(
        "INSERT INTO memory_relationships(id, from_id, to_id, rel_type, confidence, evidence, observed, created_at_ms)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(from_id, to_id, rel_type) DO UPDATE SET
            confidence = MAX(confidence, excluded.confidence),
            evidence = COALESCE(excluded.evidence, evidence)",
        params![id, from_id, to_id, rel.as_str(), confidence as f64, evidence, observed as i64, now_ms],
    ).map_err(|e| format!("create relationship: {e}"))?;
    Ok(id)
}

/// List relationships originating from a memory.
pub fn list_from(conn: &Connection, id: &str) -> Result<Vec<Relationship>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, from_id, to_id, rel_type, confidence, evidence, observed, created_at_ms
         FROM memory_relationships WHERE from_id = ?1
         ORDER BY confidence DESC, created_at_ms DESC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![id], row_to_rel_err).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// List relationships pointing to a memory.
pub fn list_to(conn: &Connection, id: &str) -> Result<Vec<Relationship>, String> {
    let mut stmt = conn.prepare(
        "SELECT id, from_id, to_id, rel_type, confidence, evidence, observed, created_at_ms
         FROM memory_relationships WHERE to_id = ?1
         ORDER BY confidence DESC, created_at_ms DESC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![id], row_to_rel_err).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// List all relationships for a memory (both directions).
pub fn list_all(conn: &Connection, id: &str) -> Result<Vec<Relationship>, String> {
    let mut out = list_from(conn, id)?;
    out.extend(list_to(conn, id)?);
    Ok(out)
}

/// Delete a relationship by id.
pub fn delete(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn.execute("DELETE FROM memory_relationships WHERE id = ?1", params![id])
        .map_err(|e| format!("delete relationship: {e}"))?;
    Ok(n > 0)
}

/// Walk the graph up to `max_depth` from `start_id`, returning
/// (memory_id, depth) pairs. BFS to keep result deterministic.
pub fn neighbors(conn: &Connection, start_id: &str, max_depth: usize) -> Result<Vec<(String, usize)>, String> {
    use std::collections::VecDeque;
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut out: Vec<(String, usize)> = Vec::new();
    queue.push_back((start_id.to_string(), 0));
    visited.insert(start_id.to_string());
    while let Some((current, depth)) = queue.pop_front() {
        if depth > 0 {
            out.push((current.clone(), depth));
        }
        if depth >= max_depth {
            continue;
        }
        let rels = list_all(conn, &current)?;
        for r in rels {
            let next = if r.from_id == current { r.to_id } else { r.from_id };
            if !visited.contains(&next) {
                visited.insert(next.clone());
                queue.push_back((next, depth + 1));
            }
        }
    }
    Ok(out)
}

fn row_to_rel_err(r: &rusqlite::Row<'_>) -> Result<Relationship, rusqlite::Error> {
    row_to_rel(r).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn row_to_rel(r: &rusqlite::Row<'_>) -> Result<Relationship, String> {
    let id: String = r.get(0).map_err(|e| e.to_string())?;
    let from_id: String = r.get(1).map_err(|e| e.to_string())?;
    let to_id: String = r.get(2).map_err(|e| e.to_string())?;
    let rel_type: String = r.get(3).map_err(|e| e.to_string())?;
    let confidence: f32 = r.get::<_, f64>(4).map_err(|e| e.to_string())? as f32;
    let evidence: Option<String> = r.get(5).ok();
    let observed: bool = r.get::<_, i64>(6).map_err(|e| e.to_string())? != 0;
    let created_at_ms: i64 = r.get(7).map_err(|e| e.to_string())?;
    Ok(Relationship { id, from_id, to_id, rel_type, confidence, evidence, observed, created_at_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{create as create_memory, NewMemory, MemoryKind};

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        crate::db::migrations::ensure_fts(&conn).unwrap();
        conn
    }

    fn make_memory(conn: &Connection, title: &str) -> String {
        let m = NewMemory::new(MemoryKind::Semantic, title, "content", "src");
        create_memory(conn, &m).unwrap()
    }

    #[test]
    fn create_and_list_relationship() {
        let conn = setup();
        let a = make_memory(&conn, "A");
        let b = make_memory(&conn, "B");
        let id = create(&conn, &a, &b, RelationshipType::RelatedTo, 0.8, Some("test evidence"), true).unwrap();
        let rels = list_from(&conn, &a).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].id, id);
        assert_eq!(rels[0].rel_type, "related_to");
        assert!(rels[0].observed);
    }

    #[test]
    fn self_relationship_rejected() {
        let conn = setup();
        let a = make_memory(&conn, "A");
        let result = create(&conn, &a, &a, RelationshipType::RelatedTo, 0.5, None, true);
        assert!(result.is_err());
    }

    #[test]
    fn bfs_neighbors_finds_connected_memories() {
        let conn = setup();
        let a = make_memory(&conn, "A");
        let b = make_memory(&conn, "B");
        let c = make_memory(&conn, "C");
        let d = make_memory(&conn, "D");
        create(&conn, &a, &b, RelationshipType::RelatedTo, 0.9, None, true).unwrap();
        create(&conn, &b, &c, RelationshipType::RelatedTo, 0.7, None, true).unwrap();
        create(&conn, &c, &d, RelationshipType::RelatedTo, 0.6, None, true).unwrap();
        let n = neighbors(&conn, &a, 2).unwrap();
        // Should find B (depth 1) and C (depth 2), but not D (depth 3).
        let ids: Vec<&str> = n.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&b.as_str()));
        assert!(ids.contains(&c.as_str()));
        assert!(!ids.contains(&d.as_str()), "D is at depth 3, beyond max_depth=2");
    }

    #[test]
    fn unique_constraint_per_pair_and_type() {
        let conn = setup();
        let a = make_memory(&conn, "A");
        let b = make_memory(&conn, "B");
        create(&conn, &a, &b, RelationshipType::RelatedTo, 0.5, None, true).unwrap();
        // Second insert with same (from, to, type) should update, not duplicate.
        create(&conn, &a, &b, RelationshipType::RelatedTo, 0.9, Some("stronger"), true).unwrap();
        let rels = list_from(&conn, &a).unwrap();
        assert_eq!(rels.len(), 1);
        assert!(rels[0].confidence >= 0.89, "confidence should be max, got {}", rels[0].confidence);
    }
}
