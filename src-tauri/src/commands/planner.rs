//! 🍓 Planner commands — todos, habits, schedule, focus + the daily briefing.

use serde::{Deserialize, Serialize};
use tauri::State;
use std::sync::Arc;

use super::Cmd;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub priority: String,
    pub completed: bool,
    pub due_date: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Habit {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub target_days: i64,
    /// ISO dates (YYYY-MM-DD) on which this habit was completed.
    pub completed_dates: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleEvent {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub color: Option<String>,
    pub recurring: String,
    pub completed: bool,
}

/// One section of the daily briefing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefingSection {
    pub key: String,
    pub title: String,
    pub lines: Vec<String>,
}

fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())
}

// ---------------------------------------------------------------- todos ----

#[tauri::command]
pub async fn get_todos(state: State<'_, Arc<AppState>>) -> Cmd<Vec<Todo>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let mut stmt = conn
            .prepare("SELECT id,title,description,priority,completed,due_date FROM todos ORDER BY completed, CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, due_date IS NULL, due_date")
            .map_err(crate::error::to_string_err("query todos"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Todo {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    description: r.get(2)?,
                    priority: r.get(3)?,
                    completed: r.get::<_, i64>(4)? != 0,
                    due_date: r.get(5)?,
                })
            })
            .map_err(crate::error::to_string_err("todos map"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn add_todo(
    state: State<'_, Arc<AppState>>,
    title: String,
    priority: String,
    due_date: Option<String>,
) -> Cmd<Todo> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute(
            "INSERT INTO todos(title, priority, due_date) VALUES(?1,?2,?3)",
            rusqlite::params![title.trim(), priority, due_date],
        )
        .map_err(crate::error::to_string_err("insert todo"))?;
        Ok(Todo {
            id: conn.last_insert_rowid(),
            title,
            description: None,
            priority,
            completed: false,
            due_date,
        })
    })
    .await
}

#[tauri::command]
pub async fn toggle_todo(state: State<'_, Arc<AppState>>, todo_id: i64) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute(
            "UPDATE todos SET completed = 1 - completed,
                completed_at = CASE WHEN completed = 0 THEN strftime('%Y-%m-%dT%H:%M:%fZ','now') ELSE NULL END,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE id = ?1",
            [todo_id],
        )
        .map_err(crate::error::to_string_err("toggle todo"))?;
        let done: i64 = conn
            .query_row("SELECT completed FROM todos WHERE id=?1", [todo_id], |r| r.get(0))
            .map_err(crate::error::to_string_err("todo lookup"))?;
        Ok(done != 0)
    })
    .await
}

#[tauri::command]
pub async fn delete_todo(state: State<'_, Arc<AppState>>, todo_id: i64) -> Cmd<()> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        conn_of(app)?
            .execute("DELETE FROM todos WHERE id=?1", [todo_id])
            .map_err(crate::error::to_string_err("delete todo"))?;
        Ok(())
    })
    .await
}

// --------------------------------------------------------------- habits ----

#[tauri::command]
pub async fn get_habits(state: State<'_, Arc<AppState>>) -> Cmd<Vec<Habit>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let mut stmt = conn
            .prepare("SELECT id,name,description,color,icon,target_days FROM habits ORDER BY id")
            .map_err(crate::error::to_string_err("query habits"))?;
        let mut habits = stmt
            .query_map([], |r| {
                Ok(Habit {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    color: r.get(3)?,
                    icon: r.get(4)?,
                    target_days: r.get(5)?,
                    completed_dates: Vec::new(),
                })
            })
            .map_err(crate::error::to_string_err("habits map"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for h in &mut habits {
            let mut lstmt = conn
                .prepare("SELECT completed_date FROM habit_logs WHERE habit_id=?1 ORDER BY completed_date")
                .map_err(crate::error::to_string_err("habit logs"))?;
            h.completed_dates = lstmt
                .query_map([h.id], |r| r.get::<_, String>(0))
                .map_err(crate::error::to_string_err("logs map"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
        }
        Ok(habits)
    })
    .await
}

#[tauri::command]
pub async fn toggle_habit_today(state: State<'_, Arc<AppState>>, habit_id: i64) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let today = crate::db::now_iso()[..10].to_string();
        let exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM habit_logs WHERE habit_id=?1 AND completed_date=?2",
                rusqlite::params![habit_id, today],
                |r| r.get(0),
            )
            .ok();
        if exists.is_some() {
            conn.execute(
                "DELETE FROM habit_logs WHERE habit_id=?1 AND completed_date=?2",
                rusqlite::params![habit_id, today],
            )
            .map_err(crate::error::to_string_err("untick habit"))?;
            Ok(false)
        } else {
            conn.execute(
                "INSERT INTO habit_logs(habit_id, completed_date) VALUES(?1,?2)",
                rusqlite::params![habit_id, today],
            )
            .map_err(crate::error::to_string_err("tick habit"))?;
            Ok(true)
        }
    })
    .await
}

#[tauri::command]
pub async fn add_habit(
    state: State<'_, Arc<AppState>>,
    name: String,
    icon: Option<String>,
    target_days: Option<i64>,
) -> Cmd<Habit> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute(
            "INSERT INTO habits(name, icon, target_days) VALUES(?1,?2,COALESCE(?3,7))",
            rusqlite::params![name.trim(), icon, target_days],
        )
        .map_err(crate::error::to_string_err("insert habit"))?;
        Ok(Habit {
            id: conn.last_insert_rowid(),
            name,
            description: None,
            color: None,
            icon,
            target_days: target_days.unwrap_or(7),
            completed_dates: Vec::new(),
        })
    })
    .await
}

// ------------------------------------------------------------- schedule ----

#[tauri::command]
pub async fn get_schedule(state: State<'_, Arc<AppState>>) -> Cmd<Vec<ScheduleEvent>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let mut stmt = conn
            .prepare("SELECT id,title,description,start_time,end_time,color,recurring,completed FROM schedule ORDER BY start_time")
            .map_err(crate::error::to_string_err("query schedule"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ScheduleEvent {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    description: r.get(2)?,
                    start_time: r.get(3)?,
                    end_time: r.get(4)?,
                    color: r.get(5)?,
                    recurring: r.get(6)?,
                    completed: r.get::<_, i64>(7)? != 0,
                })
            })
            .map_err(crate::error::to_string_err("schedule map"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn add_event(
    state: State<'_, Arc<AppState>>,
    title: String,
    start_time: String,
    end_time: Option<String>,
) -> Cmd<ScheduleEvent> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute(
            "INSERT INTO schedule(title, start_time, end_time) VALUES(?1,?2,?3)",
            rusqlite::params![title.trim(), start_time, end_time],
        )
        .map_err(crate::error::to_string_err("insert event"))?;
        Ok(ScheduleEvent {
            id: conn.last_insert_rowid(),
            title,
            description: None,
            start_time,
            end_time,
            color: None,
            recurring: "none".into(),
            completed: false,
        })
    })
    .await
}

// ------------------------------------------------------------ briefing ----

/// The 🍓 Daily Briefing: today's tasks/habits/events + "on this day" memory.
#[tauri::command]
pub async fn get_daily_briefing(state: State<'_, Arc<AppState>>) -> Cmd<Vec<BriefingSection>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let today_full = crate::db::now_iso(); // 2026-08-26T...
        let today = &today_full[..10];
        let mmdd = format!("-{}", &today[5..]); // "-08-26"

        let mut out: Vec<BriefingSection> = Vec::new();

        // --- Tasks ---
        let open: i64 = conn
            .query_row("SELECT COUNT(*) FROM todos WHERE completed=0", [], |r| r.get(0))
            .unwrap_or(0);
        let overdue: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM todos WHERE completed=0 AND due_date IS NOT NULL AND due_date < ?1",
                [today],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let mut tasks = vec![format!("{open} open tasks")];
        if overdue > 0 {
            tasks.push(format!("⚠️ {overdue} OVERDUE"));
        }
        out.push(BriefingSection {
            key: "tasks".into(),
            title: "📋 Tasks".into(),
            lines: tasks,
        });

        // --- Habits streaks ---
        let mut lines = Vec::new();
        if let Ok(mut stmt) =
            conn.prepare("SELECT id,name FROM habits ORDER BY id")
        {
            let hl = stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                .unwrap_or_default();
            for (hid, name) in hl {
                // current consecutive-day streak ending today or yesterday
                let dates: Vec<String> = {
                    let mut s = conn
                        .prepare("SELECT completed_date FROM habit_logs WHERE habit_id=?1 ORDER BY completed_date DESC")
                        .map_err(crate::error::to_string_err("streak q"))?;
                    let v = s
                        .query_map([hid], |r| r.get::<_, String>(0))
                        .map_err(crate::error::to_string_err("streak m"))?
                        .filter_map(|r| r.ok())
                        .collect::<Vec<_>>();
                    v
                };
                let streak = compute_streak(&dates, today);
                let done_today = dates.first().map(String::as_str) == Some(today);
                lines.push(format!(
                    "{} {} — {} day streak",
                    if done_today { "✅" } else { "⬜" },
                    name,
                    streak
                ));
            }
        }
        if !lines.is_empty() {
            out.push(BriefingSection { key: "habits".into(), title: "🔥 Habits".into(), lines });
        }

        // --- Today's events ---
        let mut sstmt = conn
            .prepare("SELECT title, start_time FROM schedule WHERE substr(start_time,1,10)=?1 AND completed=0 ORDER BY start_time")
            .map_err(crate::error::to_string_err("events"))?;
        let events: Vec<String> = sstmt
            .query_map([today], |r| {
                let t: String = r.get(0)?;
                let s: String = r.get(1)?;
                Ok(format!("{} · {}", &s[11..16], t))
            })
            .map_err(crate::error::to_string_err("events m"))?
            .filter_map(|r| r.ok())
            .collect();
        if !events.is_empty() {
            out.push(BriefingSection { key: "events".into(), title: "📅 Today".into(), lines: events });
        }

        // --- On this day (captures from exactly this date in past years) ---
        let mut ostmt = conn
            .prepare(
                "SELECT title, substr(created_at,1,4) AS y FROM chats
                 WHERE created_at LIKE '%' || ?1 || '%'
                   AND substr(created_at,1,4) < substr(?2,1,4)
                 ORDER BY created_at DESC LIMIT 5",
            )
            .map_err(crate::error::to_string_err("onthisday"))?;
        let memories: Vec<String> = ostmt
            .query_map(rusqlite::params![mmdd, today_full], |r| {
                let title: String = r.get(0)?;
                let year: String = r.get(1)?;
                Ok(format!("{} — “{}”", year, &title.chars().take(60).collect::<String>()))
            })
            .map_err(crate::error::to_string_err("onthisday m"))?
            .filter_map(|r| r.ok())
            .collect();
        if !memories.is_empty() {
            out.push(BriefingSection {
                key: "memories".into(),
                title: "🕰️ On this day".into(),
                lines: memories,
            });
        }

        // --- News (respects app_meta flag; network is opt-in) ---
        let news_on: Option<String> = conn
            .query_row("SELECT value FROM app_meta WHERE key='news_enabled'", [], |r| r.get(0))
            .ok();
        if news_on.as_deref() == Some("1") {
            match crate::commands::news::fetch_top_headlines(3) {
                Ok(headlines) if !headlines.is_empty() => {
                    out.push(BriefingSection {
                        key: "news".into(),
                        title: "📰 Tech news".into(),
                        lines: headlines,
                    });
                }
                _ => {}
            }
        }

        Ok(out)
    })
    .await
}

/// Consecutive-day streak counting back from `today` (or `today-1` if not done today).
fn compute_streak(dates_desc: &[String], today: &str) -> usize {
    use std::collections::HashSet;
    let set: HashSet<String> = dates_desc.iter().map(|s| s.as_str().to_string()).collect();
    let parse = |d: &str| -> Option<chrono::NaiveDate> { chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok() };
    let mut cur = match parse(today) {
        Some(t) => t,
        None => return 0,
    };
    if !set.contains(cur.format("%Y-%m-%d").to_string().as_str()) {
        cur -= chrono::Duration::days(1);
    }
    let mut streak = 0usize;
    while set.contains(&cur.format("%Y-%m-%d").to_string()) {
        streak += 1;
        cur -= chrono::Duration::days(1);
    }
    streak
}
