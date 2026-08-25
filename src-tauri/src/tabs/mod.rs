//! 🌐 Tab tracking — visits recorded by the browser-extension-lite
//! (a tiny fetch to the local app), grouped into sessions for
//! "us wali tabs wapas kholo" and dead-link rescue.

use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabGroup {
    pub host: String,
    pub urls: Vec<String>,
    pub titles: Vec<String>,
    pub visit_count: i64,
    pub last_visited: String,
}

#[derive(Debug, Deserialize)]
pub struct TabVisit {
    pub url: String,
    pub title: Option<String>,
}

/// Record a visit (called by the local extension via HTTP or Tauri command).
pub fn record(conn: &rusqlite::Connection, visit: &TabVisit) -> Result<(), String> {
    let host = url_host(&visit.url);
    conn.execute(
        "INSERT INTO tabs(url, title, host) VALUES(?1,?2,?3)",
        params![visit.url, visit.title, host],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Recent tab groups by host — the "wapas kholo" list.
pub fn recent_groups(conn: &rusqlite::Connection, limit: usize) -> Result<Vec<TabGroup>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT host, COUNT(*) AS n, MAX(visited_at),
                    group_concat(url, '\u{1f}'), group_concat(coalesce(title,''), '\u{1f}')
             FROM tabs WHERE host != ''
             GROUP BY host ORDER BY n DESC, MAX(visited_at) DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        let (host, n, last, urls_raw, titles_raw) = row.map_err(|e| e.to_string())?;
        out.push(TabGroup {
            host,
            visit_count: n,
            last_visited: last,
            urls: urls_raw.split('\u{1f}').map(String::from).collect(),
            titles: titles_raw.split('\u{1f}').map(String::from).collect(),
        });
    }
    Ok(out)
}

/// Find tabs matching a topic — dead-link rescue / research reassembly.
pub fn find_for_topic(
    conn: &rusqlite::Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT url, title FROM tabs_fts WHERE tabs_fts MATCH ?1
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let q = format!("{}*", query.replace('"', ""));
    let rows = stmt
        .query_map(params![q, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn url_host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn record_group_find_roundtrip() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::migrations::run(&mut conn).unwrap();

        record(
            &conn,
            &TabVisit {
                url: "https://docs.rs/rusqlite/latest/".into(),
                title: Some("rusqlite docs".into()),
            },
        )
        .unwrap();
        record(
            &conn,
            &TabVisit {
                url: "https://docs.rs/tauri/".into(),
                title: Some("tauri docs".into()),
            },
        )
        .unwrap();

        let groups = recent_groups(&conn, 5).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].host, "docs.rs");
        assert_eq!(groups[0].visit_count, 2);

        let hits = find_for_topic(&conn, "rusqlite", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].0.contains("rusqlite"));
    }

    #[test]
    fn host_extraction() {
        assert_eq!(url_host("https://a.b.com/x?y=1"), "a.b.com");
        assert_eq!(url_host("notaurl"), "notaurl");
    }
}
