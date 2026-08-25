//! ⏯️ Context Resume — "kahan chhoda tha, wahin se continue".
//!
//! A resume point captures: intent (goal), last exchange, open items and
//! context refs from a chat. `suggest_resume` ranks unfinished work so the
//! Dashboard banner can say: "Kal tu yahan tak pahuncha tha — continue?"

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error;

const MAX_OPEN_ITEMS: usize = 8;
const MAX_REFS: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePoint {
    pub id: String,
    pub chat_id: Option<String>,
    pub chat_title: Option<String>,
    pub intent: String,
    pub last_exchange: Option<String>,
    pub open_items: Vec<String>,
    pub context_refs: Vec<String>,
    pub updated_at: String,
}

fn json_list(s: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
}

/// Build a resume point for a chat by mining its existing artifacts.
/// Deterministic: no LLM. Intent = first question/goal; open items =
/// action items + rejected-free decisions not obviously done.
pub fn build_for_chat(conn: &rusqlite::Connection, chat_id: &str) -> Result<ResumePoint, String> {
    let title: String = conn
        .query_row("SELECT title FROM chats WHERE id=?1", [chat_id], |r| {
            r.get::<_, String>(0)
        })
        .map_err(|_| format!("chat {chat_id} not found"))?;
    let intent_fallback = title.clone();

    let mut intent: Option<String> = None;
    let mut open_items: Vec<String> = Vec::new();
    let mut refs: Vec<String> = Vec::new();

    // Intent: first question artifact, else first answer/key point.
    let mut stmt = conn
        .prepare(
            "SELECT artifact_type, content FROM chat_artifacts
             WHERE chat_id=?1 ORDER BY created_at ASC",
        )
        .map_err(error::to_string_err("artifacts query"))?;
    let rows: Vec<(String, String)> = stmt
        .query_map([chat_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(error::to_string_err("artifacts map"))?
        .filter_map(|r| r.ok())
        .collect();

    for (kind, content) in &rows {
        match kind.as_str() {
            "question" if intent.is_none() => intent = Some(content.clone()),
            "action_item" if open_items.len() < MAX_OPEN_ITEMS => {
                if !open_items.iter().any(|x| x == content) {
                    open_items.push(content.clone());
                }
            }
            "identifier" | "url" | "command" if refs.len() < MAX_REFS => {
                if !refs.iter().any(|x| x == content) {
                    refs.push(content.clone());
                }
            }
            _ => {}
        }
    }

    // Last exchange: most recent answer artifact (clamped).
    let last_exchange: Option<String> = rows
        .iter()
        .rev()
        .find(|(k, _)| k == "answer" || k == "code")
        .map(|(_, c)| c.chars().take(280).collect());

    let now = db::now_iso();
    Ok(ResumePoint {
        id: db::new_uuid(),
        chat_id: Some(chat_id.to_string()),
        chat_title: Some(title),
        intent: intent.unwrap_or(intent_fallback),
        last_exchange,
        open_items,
        context_refs: refs,
        updated_at: now,
    })
}

/// Upsert the single active resume point per chat.
pub fn save_for_chat(conn: &rusqlite::Connection, chat_id: &str) -> Result<ResumePoint, String> {
    let rp = build_for_chat(conn, chat_id)?;
    conn.execute(
        "DELETE FROM chat_resume_points WHERE chat_id=?1",
        [chat_id],
    )
    .map_err(error::to_string_err("clear old resume"))?;
    conn.execute(
        "INSERT INTO chat_resume_points
            (id,chat_id,intent,last_exchange,open_items,context_refs,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            rp.id,
            chat_id,
            rp.intent,
            rp.last_exchange,
            serde_json::to_string(&rp.open_items).unwrap_or_default(),
            serde_json::to_string(&rp.context_refs).unwrap_or_default(),
            rp.updated_at,
            rp.updated_at,
        ],
    )
    .map_err(error::to_string_err("insert resume"))?;
    Ok(rp)
}

/// Ranked suggestions: most-recently-updated first, capped.
pub fn suggestions(conn: &rusqlite::Connection, limit: usize) -> Result<Vec<ResumePoint>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.chat_id, ch.title, r.intent, r.last_exchange,
                    r.open_items, r.context_refs, r.updated_at
             FROM chat_resume_points r LEFT JOIN chats ch ON ch.id = r.chat_id
             ORDER BY r.updated_at DESC LIMIT ?1",
        )
        .map_err(error::to_string_err("resume query"))?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(error::to_string_err("resume map"))?;

    let mut out = Vec::new();
    for row in rows {
        let (id, chat_id, chat_title, intent, last_exchange, open, refs, updated_at) =
            row.map_err(|e| e.to_string())?;
        out.push(ResumePoint {
            id,
            chat_id,
            chat_title,
            intent,
            last_exchange,
            open_items: json_list(&open),
            context_refs: json_list(&refs),
            updated_at,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn build_and_rank_roundtrip() {
        let conn = setup();
        let now = db::now_iso();
        conn.execute(
            "INSERT INTO roots(id,name,created_at,updated_at) VALUES('r','R',?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,type,name,created_at,updated_at)
             VALUES('n','r','chat','Fix login loop',?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chats(id,node_id,title,raw_path,created_at,updated_at)
             VALUES('c1','n','Fix login loop','/tmp/x',?1,?1)",
            [&now],
        )
        .unwrap();
        for (t, c) in [
            ("question", "Why does the login redirect loop happen?"),
            ("answer", "The auth guard re-runs on every render."),
            ("action_item", "Add redirect guard to router config"),
            ("identifier", "AUTH_REDIRECT_URI"),
        ] {
            conn.execute(
                "INSERT INTO chat_artifacts(id,chat_id,artifact_type,content,created_at)
                 VALUES(?1,'c1',?2,?3,?1)",
                params![db::new_uuid(), t, c],
            )
            .unwrap();
        }

        let rp = save_for_chat(&conn, "c1").unwrap();
        assert!(rp.intent.contains("redirect loop"));
        assert_eq!(rp.open_items.len(), 1);
        assert!(rp.context_refs.contains(&"AUTH_REDIRECT_URI".to_string()));

        let list = suggestions(&conn, 5).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].chat_title.as_deref(), Some("Fix login loop"));
    }
}

/// "Resume My Day" — everything needed to re-enter flow in ~10 seconds.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    pub last_chats: Vec<(String, String)>,   // (title, updated_at)
    pub last_captures: Vec<(String, String)>, // (title, created_at)
    pub open_tasks: Vec<String>,
    pub top_intent: Option<String>,
}

pub fn day_summary(conn: &rusqlite::Connection) -> Result<DaySummary, String> {
    let last_chats: Vec<(String, String)> = {
        let mut s = conn
            .prepare("SELECT title, updated_at FROM chats ORDER BY updated_at DESC LIMIT 5")
            .map_err(|e| e.to_string())?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    };

    let last_captures: Vec<(String, String)> = {
        let mut s = conn
            .prepare(
                "SELECT substr(brief_text,1,60), created_at FROM chats
                 WHERE source='capture' ORDER BY created_at DESC LIMIT 5",
            )
            .map_err(|e| e.to_string())?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)));
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    };

    let open_tasks: Vec<String> = {
        let mut s = conn
            .prepare("SELECT title FROM todos WHERE completed=0 ORDER BY priority DESC LIMIT 5")
            .map_err(|e| e.to_string())?;
        let rows = s.query_map([], |r| r.get::<_, String>(0));
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
    };

    let top_intent = suggestions(conn, 1)
        .ok()
        .and_then(|v| v.first().map(|p| p.intent.clone()));

    Ok(DaySummary { last_chats, last_captures, open_tasks, top_intent })
}
