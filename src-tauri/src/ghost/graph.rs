//! 👻 Ghost Graph — builds a knowledge graph from your chats, folders, roots, and tags.
//!
//! Nodes:
//! - chats: each chat is a node
//! - folders: each folder is a node
//! - roots: each root is a node
//! - tags: each unique tag is a node
//!
//! Edges:
//! - parent_child: folder → chat, root → folder, etc.
//! - shared_tag: chat ↔ tag, chat ↔ chat (via shared tags)
//! - co_access: chat ↔ chat (if opened in same session)
//! - temporal: chat ↔ chat (if created within 1h of each other)

use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use crate::ghost::{GraphNode, GraphEdge};

/// Rebuild the entire knowledge graph from scratch.
pub fn rebuild(conn: &Connection) -> Result<(Vec<GraphNode>, Vec<GraphEdge>), String> {
    // Wipe existing graph data.
    conn.execute("DELETE FROM ghost_graph_edges", [])
        .map_err(|e| format!("graph clear edges: {e}"))?;
    conn.execute("DELETE FROM ghost_graph_nodes", [])
        .map_err(|e| format!("graph clear nodes: {e}"))?;

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    // 1) Collect all chats with their node ancestry + tags.
    let mut chats: Vec<(String, String, String, Option<String>, Option<String>, String)> = Vec::new();
    // (chat_id, title, node_id, tags_csv, parent_node_id, created_at)
    {
        let mut stmt = conn.prepare(
            "SELECT ch.id, COALESCE(ch.title,''), ch.node_id, n.parent_id, ch.tags, ch.created_at
             FROM chats ch
             JOIN nodes n ON n.id = ch.node_id"
        ).map_err(|e| format!("graph prepare chats: {e}"))?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
            ))
        }).map_err(|e| format!("graph query chats: {e}"))?;
        for row in rows {
            chats.push(row.map_err(|e| format!("graph chat row: {e}"))?);
        }
    }

    // 2) Collect all folders.
    let mut folders: HashMap<String, (String, Option<String>, String)> = HashMap::new();
    // (folder_id, name, parent_id, root_id)
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, root_id FROM nodes WHERE type = 'folder'"
        ).map_err(|e| format!("graph prepare folders: {e}"))?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        }).map_err(|e| format!("graph query folders: {e}"))?;
        for row in rows {
            let (id, name, parent, root) = row.map_err(|e| format!("graph folder row: {e}"))?;
            folders.insert(id, (name, parent, root));
        }
    }

    // 3) Collect all roots.
    let mut roots: HashMap<String, String> = HashMap::new(); // root_id → name
    {
        let mut stmt = conn.prepare("SELECT id, name FROM roots")
            .map_err(|e| format!("graph prepare roots: {e}"))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("graph query roots: {e}"))?;
        for row in rows {
            let (id, name) = row.map_err(|e| format!("graph root row: {e}"))?;
            roots.insert(id, name);
        }
    }

    // 4) Insert root nodes.
    let now = crate::db::now_iso();
    for (rid, name) in &roots {
        conn.execute(
            "INSERT INTO ghost_graph_nodes(id, kind, label, weight, color, created_at) VALUES (?1, 'root', ?2, 1.0, '#fb7185', ?3)",
            rusqlite::params![format!("root:{}", rid), name, &now],
        ).map_err(|e| format!("graph insert root: {e}"))?;
        nodes.push(GraphNode {
            id: format!("root:{}", rid),
            kind: "root".into(),
            label: name.clone(),
            weight: 1.0,
            color: Some("#fb7185".into()),
            position_x: None,
            position_y: None,
        });
    }

    // 5) Insert folder nodes.
    for (fid, (name, parent, root)) in &folders {
        conn.execute(
            "INSERT INTO ghost_graph_nodes(id, kind, label, weight, color, created_at) VALUES (?1, 'folder', ?2, 1.0, '#60a5fa', ?3)",
            rusqlite::params![format!("folder:{}", fid), name, &now],
        ).map_err(|e| format!("graph insert folder: {e}"))?;
        nodes.push(GraphNode {
            id: format!("folder:{}", fid),
            kind: "folder".into(),
            label: name.clone(),
            weight: 1.0,
            color: Some("#60a5fa".into()),
            position_x: None,
            position_y: None,
        });

        // Edge: parent → folder
        let parent_node_id = if let Some(pid) = parent {
            format!("folder:{}", pid)
        } else {
            format!("root:{}", root)
        };
        insert_edge(conn, &parent_node_id, &format!("folder:{}", fid), 1.0, "parent_child", &now)?;
        edges.push(GraphEdge {
            id: edges.len() as i64 + 1,
            source_id: parent_node_id,
            target_id: format!("folder:{}", fid),
            weight: 1.0,
            edge_type: "parent_child".into(),
        });
    }

    // 6) Insert chat nodes + parent edges.
    for (cid, title, _node_id, tags_csv, parent_node_id, _created) in &chats {
        let weight = compute_chat_weight(conn, cid)?;
        conn.execute(
            "INSERT INTO ghost_graph_nodes(id, kind, label, weight, color, created_at) VALUES (?1, 'chat', ?2, ?3, '#34d399', ?4)",
            rusqlite::params![format!("chat:{}", cid), title, weight, &now],
        ).map_err(|e| format!("graph insert chat: {e}"))?;
        nodes.push(GraphNode {
            id: format!("chat:{}", cid),
            kind: "chat".into(),
            label: title.clone(),
            weight,
            color: Some("#34d399".into()),
            position_x: None,
            position_y: None,
        });

        // Edge: parent folder/root → chat
        if let Some(pid) = parent_node_id {
            let parent_id = format!("folder:{}", pid);
            insert_edge(conn, &parent_id, &format!("chat:{}", cid), 1.0, "parent_child", &now)?;
            edges.push(GraphEdge {
                id: edges.len() as i64 + 1,
                source_id: parent_id,
                target_id: format!("chat:{}", cid),
                weight: 1.0,
                edge_type: "parent_child".into(),
            });
        }
    }

    // 7) Tag nodes + shared_tag edges.
    let mut tag_count: HashMap<String, Vec<String>> = HashMap::new(); // tag → [chat_ids]
    for (cid, _t, _n, tags_csv, _p, _c) in &chats {
        if let Some(tags_str) = tags_csv {
            for tag in tags_str.split(',') {
                let t = tag.trim().to_lowercase();
                if t.is_empty() { continue; }
                tag_count.entry(t).or_default().push(cid.clone());
            }
        }
    }
    for (tag, chat_ids) in &tag_count {
        let tag_id = format!("tag:{}", tag);
        let weight = (chat_ids.len() as f64).max(1.0);
        conn.execute(
            "INSERT INTO ghost_graph_nodes(id, kind, label, weight, color, created_at) VALUES (?1, 'tag', ?2, ?3, '#fbbf24', ?4)",
            rusqlite::params![&tag_id, tag, weight, &now],
        ).map_err(|e| format!("graph insert tag: {e}"))?;
        nodes.push(GraphNode {
            id: tag_id.clone(),
            kind: "tag".into(),
            label: tag.clone(),
            weight,
            color: Some("#fbbf24".into()),
            position_x: None,
            position_y: None,
        });

        for cid in chat_ids {
            insert_edge(conn, &tag_id, &format!("chat:{}", cid), 0.5, "shared_tag", &now)?;
            edges.push(GraphEdge {
                id: edges.len() as i64 + 1,
                source_id: tag_id.clone(),
                target_id: format!("chat:{}", cid),
                weight: 0.5,
                edge_type: "shared_tag".into(),
            });
        }

        // chat ↔ chat via shared tag (only for tags shared by 2-5 chats to avoid clutter)
        if chat_ids.len() >= 2 && chat_ids.len() <= 5 {
            for i in 0..chat_ids.len() {
                for j in (i+1)..chat_ids.len() {
                    let a = format!("chat:{}", chat_ids[i]);
                    let b = format!("chat:{}", chat_ids[j]);
                    insert_edge(conn, &a, &b, 0.3, "shared_tag", &now)?;
                    edges.push(GraphEdge {
                        id: edges.len() as i64 + 1,
                        source_id: a,
                        target_id: b,
                        weight: 0.3,
                        edge_type: "shared_tag".into(),
                    });
                }
            }
        }
    }

    // 8) Temporal edges: chats created within 1h of each other.
    let mut sorted_chats: Vec<(&String, &String)> = chats.iter()
        .map(|(cid, _t, _n, _tags, _p, created)| (cid, created))
        .collect();
    sorted_chats.sort_by(|a, b| a.1.cmp(b.1));
    for i in 0..sorted_chats.len().saturating_sub(1) {
        let (cid_a, t_a) = sorted_chats[i];
        let (cid_b, t_b) = sorted_chats[i+1];
        if is_within_hours(t_a, t_b, 1) {
            let a = format!("chat:{}", cid_a);
            let b = format!("chat:{}", cid_b);
            insert_edge(conn, &a, &b, 0.2, "temporal", &now)?;
            edges.push(GraphEdge {
                id: edges.len() as i64 + 1,
                source_id: a,
                target_id: b,
                weight: 0.2,
                edge_type: "temporal".into(),
            });
        }
    }

    // 9) Co-access edges from ghost_events (chats opened in same 10-min window).
    let mut access: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT source_id, created_at FROM ghost_events
             WHERE event_type = 'open_chat' AND source_id IS NOT NULL
             ORDER BY created_at DESC LIMIT 1000"
        ).map_err(|e| format!("graph coaccess: {e}"))?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }).map_err(|e| format!("graph coaccess rows: {e}"))?;
        // Group into 10-minute buckets
        let mut buckets: HashMap<String, HashSet<String>> = HashMap::new();
        for row in rows {
            let (sid, ts) = row.map_err(|e| format!("graph coaccess row: {e}"))?;
            // Truncate to 10-min bucket
            let bucket = truncate_to_10min(&ts);
            buckets.entry(bucket).or_default().insert(sid);
        }
        let _ = access; // (not used directly; edges are derived from buckets)
        for (_bucket, ids) in &buckets {
            let ids: Vec<String> = ids.iter().cloned().collect();
            for i in 0..ids.len() {
                for j in (i+1)..ids.len() {
                    let a = format!("chat:{}", ids[i]);
                    let b = format!("chat:{}", ids[j]);
                    insert_edge(conn, &a, &b, 0.4, "co_access", &now)?;
                    edges.push(GraphEdge {
                        id: edges.len() as i64 + 1,
                        source_id: a,
                        target_id: b,
                        weight: 0.4,
                        edge_type: "co_access".into(),
                    });
                }
            }
        }
    }

    Ok((nodes, edges))
}

/// Insert an edge if it doesn't exist already.
fn insert_edge(
    conn: &Connection,
    source: &str,
    target: &str,
    weight: f64,
    edge_type: &str,
    now: &str,
) -> Result<(), String> {
    let (a, b) = if source < target { (source, target) } else { (target, source) };
    let _ = conn.execute(
        "INSERT OR IGNORE INTO ghost_graph_edges(source_id, target_id, weight, edge_type, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![a, b, weight, edge_type, now],
    );
    Ok(())
}

/// Compute a chat's importance weight based on its length, artifacts, and recency.
fn compute_chat_weight(conn: &Connection, chat_id: &str) -> Result<f64, String> {
    let len: i64 = conn
        .query_row(
            "SELECT COALESCE(char_count, 0) FROM chats WHERE id = ?1",
            [chat_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let artifacts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chat_artifacts WHERE chat_id = ?1",
            [chat_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let accesses: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ghost_events WHERE event_type = 'open_chat' AND source_id = ?1",
            [chat_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let w = (len as f64 / 200.0) + (artifacts as f64 * 2.0) + (accesses as f64 * 0.5) + 1.0;
    Ok(w)
}

/// Are two ISO timestamps within N hours of each other?
fn is_within_hours(a: &str, b: &str, hours: i64) -> bool {
    let pa = chrono::DateTime::parse_from_rfc3339(a).ok();
    let pb = chrono::DateTime::parse_from_rfc3339(b).ok();
    match (pa, pb) {
        (Some(x), Some(y)) => {
            let diff = (x - y).num_seconds().abs();
            diff <= hours * 3600
        }
        _ => false,
    }
}

/// Truncate an ISO timestamp to a 10-minute bucket key.
fn truncate_to_10min(ts: &str) -> String {
    if ts.len() < 16 { return ts.to_string(); }
    let min: i64 = ts[14..16].parse().unwrap_or(0);
    let bucket_min = (min / 10) * 10;
    format!("{}{:02}", &ts[..14], bucket_min)
}
