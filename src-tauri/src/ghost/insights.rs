//! 👻 Ghost Insights — surfaces serendipities, patterns, and resurfaces.

use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use crate::ghost::GhostInsight;
use crate::db::now_iso;

/// Generate fresh insights based on current data. Wipes old unseen insights.
pub fn regenerate(conn: &Connection) -> Result<Vec<GhostInsight>, String> {
    // Wipe old (seen=0) insights.
    conn.execute("DELETE FROM ghost_insights WHERE seen = 0", [])
        .map_err(|e| format!("insights wipe: {e}"))?;

    let now = now_iso();
    let mut generated: Vec<GhostInsight> = Vec::new();

    // 1) Serendipity: chats with shared tags that haven't been opened in a long time.
    generated.extend(serendipity_old_chats_with_shared_tags(conn, &now)?);

    // 2) Resurface: chats you saved but never opened again in 7+ days.
    generated.extend(resurface_unread(conn, &now)?);

    // 3) Pattern: your busiest hour / day.
    if let Some(ins) = busiest_pattern(conn, &now)? {
        generated.push(ins);
    }

    // 4) Achievement: streaks and milestones.
    generated.extend(achievements(conn, &now)?);

    // 5) Cluster: find tight clusters in the graph (tags with many chats).
    generated.extend(tag_clusters(conn, &now)?);

    // 6) Warning: dangling chats (no parent folder).
    if let Some(ins) = dangling_chats_warning(conn, &now)? {
        generated.push(ins);
    }

    // 7) Cross-pollination: chats in different roots that share tags.
    generated.extend(cross_root_serendipity(conn, &now)?);

    // 8) Quiet roots: roots you haven't touched in 14+ days.
    generated.extend(quiet_roots(conn, &now)?);

    // 9) Velocity: how many new chats in the last week vs prior week.
    if let Some(ins) = velocity_insight(conn, &now)? {
        generated.push(ins);
    }

    // 10) Tag orphans: tags that exist on only one chat (maybe consolidate).
    generated.extend(orphan_tags(conn, &now)?);

    Ok(generated)
}

/// 1) Serendipity: old chats sharing tags with recent chats.
fn serendipity_old_chats_with_shared_tags(
    conn: &Connection,
    now: &str,
) -> Result<Vec<GhostInsight>, String> {
    let mut out = Vec::new();
    // Find tags that appear on at least one old chat (created >14d ago) and at least one recent chat.
    let mut stmt = conn.prepare(
        "SELECT DISTINCT tags FROM chats
         WHERE tags IS NOT NULL AND tags != ''"
    ).map_err(|e| format!("serendipity prep: {e}"))?;
    let tag_rows: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("serendipity rows: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    for tags_csv in tag_rows {
        for tag in tags_csv.split(',') {
            let t = tag.trim();
            if t.is_empty() { continue; }

            // Old chat
            let old: Option<(String, String, String)> = conn.query_row(
                "SELECT ch.id, ch.title, ch.created_at FROM chats ch
                 WHERE ?1 IN (SELECT value FROM json_each('[' || REPLACE(ch.tags, ',', ',') || ']'))
                 AND ch.created_at < datetime('now', '-14 days')
                 ORDER BY ch.created_at DESC LIMIT 1",
                [t],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).ok();
            // Recent chat
            let recent: Option<(String, String, String)> = conn.query_row(
                "SELECT ch.id, ch.title, ch.created_at FROM chats ch
                 WHERE ?1 IN (SELECT value FROM json_each('[' || REPLACE(ch.tags, ',', ',') || ']'))
                 AND ch.created_at >= datetime('now', '-14 days')
                 ORDER BY ch.created_at DESC LIMIT 1",
                [t],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).ok();

            if let (Some((oid, otitle, _)), Some((rid, rtitle, _))) = (old, recent) {
                if oid != rid {
                    let title = format!("🌀 Serendipity: tag `{}`", t);
                    let body = format!(
                        "You saved \"{}\" a while back, and recently saved \"{}\". They both touch `{}` — might be worth a cross-check.",
                        truncate(&otitle, 60), truncate(&rtitle, 60), t
                    );
                    let sources = serde_json::json!([oid, rid]).to_string();
                    conn.execute(
                        "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
                         VALUES ('serendipity', ?1, ?2, ?3, 0.7, 0, ?4)",
                        rusqlite::params![&title, &body, &sources, now],
                    ).map_err(|e| format!("insight insert: {e}"))?;
                    let id = conn.last_insert_rowid();
                    out.push(GhostInsight {
                        id, kind: "serendipity".into(), title, body, source_ids: Some(sources),
                        score: 0.7, seen: 0, created_at: now.into(),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// 2) Resurface: chats saved but never re-opened.
fn resurface_unread(conn: &Connection, now: &str) -> Result<Vec<GhostInsight>, String> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT ch.id, ch.title, ch.created_at, COALESCE(opens, 0) FROM chats ch
         LEFT JOIN (SELECT source_id, COUNT(*) AS opens FROM ghost_events
                    WHERE event_type = 'open_chat' GROUP BY source_id) g
              ON g.source_id = ch.id
         WHERE ch.created_at < datetime('now', '-7 days')
           AND COALESCE(opens, 0) = 0
         ORDER BY ch.created_at DESC LIMIT 5"
    ).map_err(|e| format!("resurface prep: {e}"))?;

    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
    }).map_err(|e| format!("resurface rows: {e}"))?;

    for row in rows {
        let (cid, title, _created, _opens) = row.map_err(|e| format!("resurface row: {e}"))?;
        let title_str = "💤 Sleeping note".to_string();
        let body = format!("You saved \"{}\" but haven't opened it again. Maybe it's time to revisit?", truncate(&title, 70));
        let sources = serde_json::json!([cid]).to_string();
        conn.execute(
            "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
             VALUES ('resurface', ?1, ?2, ?3, 0.5, 0, ?4)",
            rusqlite::params![&title_str, &body, &sources, now],
        ).map_err(|e| format!("resurface insert: {e}"))?;
        let id = conn.last_insert_rowid();
        out.push(GhostInsight {
            id, kind: "resurface".into(), title: title_str, body, source_ids: Some(sources),
            score: 0.5, seen: 0, created_at: now.into(),
        });
    }
    Ok(out)
}

/// 3) Pattern: busiest hour / day.
fn busiest_pattern(conn: &Connection, now: &str) -> Result<Option<GhostInsight>, String> {
    let peak: Option<(String, i64)> = conn.query_row(
        "SELECT strftime('%H', created_at) AS h, COUNT(*) AS c FROM ghost_events
         GROUP BY h ORDER BY c DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).ok();

    if let Some((hour, count)) = peak {
        let h: i64 = hour.parse().unwrap_or(0);
        let title = "⏰ Your peak hour".to_string();
        let body = format!(
            "You're most active around {}:00 IST ({} events). Plan deep work there.",
            h, count
        );
        let sources = "[]".to_string();
        conn.execute(
            "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
             VALUES ('pattern', ?1, ?2, ?3, 0.6, 0, ?4)",
            rusqlite::params![&title, &body, &sources, now],
        ).map_err(|e| format!("peak insert: {e}"))?;
        let id = conn.last_insert_rowid();
        return Ok(Some(GhostInsight {
            id, kind: "pattern".into(), title, body, source_ids: Some(sources),
            score: 0.6, seen: 0, created_at: now.into(),
        }));
    }
    Ok(None)
}

/// 4) Achievements: streak days, total milestones.
fn achievements(conn: &Connection, now: &str) -> Result<Vec<GhostInsight>, String> {
    let mut out = Vec::new();
    let total_chats: i64 = conn.query_row("SELECT COUNT(*) FROM chats", [], |r| r.get(0)).unwrap_or(0);

    if total_chats > 0 && total_chats % 10 == 0 {
        let title = format!("🎯 {} chats milestone", total_chats);
        let body = format!("You've saved {} chats. The Ghost is watching. 🫡", total_chats);
        conn.execute(
            "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
             VALUES ('achievement', ?1, ?2, '[]', 0.9, 0, ?3)",
            rusqlite::params![&title, &body, now],
        ).map_err(|e| format!("achievement insert: {e}"))?;
        out.push(GhostInsight {
            id: conn.last_insert_rowid(),
            kind: "achievement".into(),
            title, body, source_ids: Some("[]".into()),
            score: 0.9, seen: 0, created_at: now.into(),
        });
    }

    Ok(out)
}

/// 5) Tag clusters: tags with many chats.
fn tag_clusters(conn: &Connection, now: &str) -> Result<Vec<GhostInsight>, String> {
    let mut out = Vec::new();
    let mut tag_count: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT tags FROM chats WHERE tags IS NOT NULL")
            .map_err(|e| format!("tag cluster prep: {e}"))?;
        let rows = stmt.query_map([], |r| r.get::<_, Option<String>>(0))
            .map_err(|e| format!("tag cluster rows: {e}"))?;
        for row in rows {
            if let Some(Some(tags)) = row.ok() {
                for tag in tags.split(',') {
                    let t = tag.trim().to_string();
                    if !t.is_empty() {
                        *tag_count.entry(t).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    for (tag, count) in tag_count {
        if count >= 5 {
            let title = format!("🧠 Tag cluster: `{}`", tag);
            let body = format!("{} chats share the tag `{}` — that's a real topic in your knowledge tree.", count, tag);
            conn.execute(
                "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
                 VALUES ('cluster', ?1, ?2, '[]', 0.6, 0, ?3)",
                rusqlite::params![&title, &body, now],
            ).map_err(|e| format!("cluster insert: {e}"))?;
            out.push(GhostInsight {
                id: conn.last_insert_rowid(),
                kind: "cluster".into(),
                title, body, source_ids: Some("[]".into()),
                score: 0.6, seen: 0, created_at: now.into(),
            });
        }
    }
    Ok(out)
}

/// 6) Dangling chats: no parent folder.
fn dangling_chats_warning(conn: &Connection, now: &str) -> Result<Option<GhostInsight>, String> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE type = 'chat' AND parent_id IS NULL",
        [], |r| r.get(0)
    ).unwrap_or(0);
    if n == 0 { return Ok(None); }
    let title = "⚠️ Dangling chats".to_string();
    let body = format!("{} chat{} sit at the top level. Consider moving them into a folder.", n, if n == 1 { "" } else { "s" });
    conn.execute(
        "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
         VALUES ('warning', ?1, ?2, '[]', 0.4, 0, ?3)",
        rusqlite::params![&title, &body, now],
    ).map_err(|e| format!("dangling insert: {e}"))?;
    Ok(Some(GhostInsight {
        id: conn.last_insert_rowid(),
        kind: "warning".into(),
        title, body, source_ids: Some("[]".into()),
        score: 0.4, seen: 0, created_at: now.into(),
    }))
}

/// 7) Cross-root serendipity: same tag in different roots.
fn cross_root_serendipity(conn: &Connection, now: &str) -> Result<Vec<GhostInsight>, String> {
    let mut out = Vec::new();
    let mut tag_roots: HashMap<String, HashSet<String>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT n.root_id, ch.tags FROM chats ch JOIN nodes n ON n.id = ch.node_id
             WHERE ch.tags IS NOT NULL AND ch.tags != ''"
        ).map_err(|e| format!("crossroot prep: {e}"))?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }).map_err(|e| format!("crossroot rows: {e}"))?;
        for row in rows {
            let (root, tags) = row.map_err(|e| format!("crossroot row: {e}"))?;
            for tag in tags.split(',') {
                let t = tag.trim().to_string();
                if !t.is_empty() {
                    tag_roots.entry(t).or_default().insert(root.clone());
                }
            }
        }
    }
    for (tag, roots) in tag_roots {
        if roots.len() >= 2 {
            let mut roots_vec: Vec<String> = roots.into_iter().collect();
            roots_vec.sort();
            let title = format!("🌐 Cross-root tag: `{}`", tag);
            let body = format!("The tag `{}` shows up across {} roots: {}. They might be related.", tag, roots_vec.len(), roots_vec.join(", "));
            conn.execute(
                "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
                 VALUES ('serendipity', ?1, ?2, '[]', 0.55, 0, ?3)",
                rusqlite::params![&title, &body, now],
            ).map_err(|e| format!("crossroot insert: {e}"))?;
            out.push(GhostInsight {
                id: conn.last_insert_rowid(),
                kind: "serendipity".into(),
                title, body, source_ids: Some("[]".into()),
                score: 0.55, seen: 0, created_at: now.into(),
            });
        }
    }
    Ok(out)
}

/// 8) Quiet roots: untouched for 14+ days.
fn quiet_roots(conn: &Connection, now: &str) -> Result<Vec<GhostInsight>, String> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT r.id, r.name, MAX(ch.created_at) AS last_chat
         FROM roots r
         LEFT JOIN nodes n ON n.root_id = r.id
         LEFT JOIN chats ch ON ch.node_id = n.id
         GROUP BY r.id, r.name"
    ).map_err(|e| format!("quiet prep: {e}"))?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
    }).map_err(|e| format!("quiet rows: {e}"))?;
    for row in rows {
        let (rid, name, last) = row.map_err(|e| format!("quiet row: {e}"))?;
        let is_quiet = match last {
            None => true,
            Some(t) => {
                let dt = chrono::DateTime::parse_from_rfc3339(&t).ok();
                match dt {
                    Some(d) => (chrono::Utc::now() - d.with_timezone(&chrono::Utc)).num_days() >= 14,
                    None => true,
                }
            }
        };
        if is_quiet {
            let title = format!("🤫 Quiet root: {}", name);
            let body = format!("`{}` hasn't had a new chat in 14+ days. Forgotten topic, or just resting?", name);
            let sources = serde_json::json!([format!("root:{}", rid)]).to_string();
            conn.execute(
                "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
                 VALUES ('pattern', ?1, ?2, ?3, 0.4, 0, ?4)",
                rusqlite::params![&title, &body, &sources, now],
            ).map_err(|e| format!("quiet insert: {e}"))?;
            out.push(GhostInsight {
                id: conn.last_insert_rowid(),
                kind: "pattern".into(),
                title, body, source_ids: Some(sources),
                score: 0.4, seen: 0, created_at: now.into(),
            });
        }
    }
    Ok(out)
}

/// 9) Velocity: new chats this week vs prior week.
fn velocity_insight(conn: &Connection, now: &str) -> Result<Option<GhostInsight>, String> {
    let this_week: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chats WHERE created_at >= datetime('now', '-7 days')",
        [], |r| r.get(0)
    ).unwrap_or(0);
    let last_week: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chats
         WHERE created_at >= datetime('now', '-14 days') AND created_at < datetime('now', '-7 days')",
        [], |r| r.get(0)
    ).unwrap_or(0);
    if this_week == 0 && last_week == 0 { return Ok(None); }
    let (emoji, msg) = if this_week > last_week {
        ("📈", format!("You saved {} chats this week vs {} last week — momentum up.", this_week, last_week))
    } else if this_week < last_week {
        ("📉", format!("You saved {} chats this week vs {} last week — slow week.", this_week, last_week))
    } else if this_week > 0 {
        ("➡️", format!("You saved {} chats both this and last week — steady pace.", this_week))
    } else {
        return Ok(None);
    };
    let title = format!("{} Velocity", emoji);
    let body = msg;
    conn.execute(
        "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
         VALUES ('pattern', ?1, ?2, '[]', 0.5, 0, ?3)",
        rusqlite::params![&title, &body, now],
    ).map_err(|e| format!("velocity insert: {e}"))?;
    Ok(Some(GhostInsight {
        id: conn.last_insert_rowid(),
        kind: "pattern".into(),
        title, body, source_ids: Some("[]".into()),
        score: 0.5, seen: 0, created_at: now.into(),
    }))
}

/// 10) Orphan tags: only on one chat.
fn orphan_tags(conn: &Connection, now: &str) -> Result<Vec<GhostInsight>, String> {
    let mut out = Vec::new();
    let mut tag_count: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT tags FROM chats WHERE tags IS NOT NULL")
            .map_err(|e| format!("orphan prep: {e}"))?;
        let rows = stmt.query_map([], |r| r.get::<_, Option<String>>(0))
            .map_err(|e| format!("orphan rows: {e}"))?;
        for row in rows {
            if let Some(Some(tags)) = row.ok() {
                for tag in tags.split(',') {
                    let t = tag.trim().to_string();
                    if !t.is_empty() {
                        *tag_count.entry(t).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    let orphans: Vec<(String, i64)> = tag_count.into_iter()
        .filter(|(_, c)| *c == 1)
        .take(3)
        .collect();
    if !orphans.is_empty() {
        let names: Vec<String> = orphans.iter().map(|(t, _)| format!("`{}`", t)).collect();
        let title = "🏷️ Orphan tags".to_string();
        let body = format!("These tags only appear on one chat each: {}. Maybe consolidate?", names.join(", "));
        conn.execute(
            "INSERT INTO ghost_insights(kind, title, body, source_ids, score, seen, created_at)
             VALUES ('pattern', ?1, ?2, '[]', 0.3, 0, ?3)",
            rusqlite::params![&title, &body, now],
        ).map_err(|e| format!("orphan insert: {e}"))?;
        out.push(GhostInsight {
            id: conn.last_insert_rowid(),
            kind: "pattern".into(),
            title, body, source_ids: Some("[]".into()),
            score: 0.3, seen: 0, created_at: now.into(),
        });
    }
    Ok(out)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut)
    }
}
