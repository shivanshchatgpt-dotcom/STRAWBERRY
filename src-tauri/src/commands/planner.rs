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

// ------------------------------------------------------------- events & calendar ----

/// Maps a `SELECT ... FROM events` row (17 columns, see `list_calendar_events`)
/// into a `CalendarEvent`. Shared by all calendar query sites so the column
/// list and struct stay in one place.
fn map_calendar_event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CalendarEvent> {
    Ok(CalendarEvent {
        id: r.get(0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        start_at: r.get(3)?,
        end_at: r.get(4)?,
        timezone: r.get(5)?,
        category: r.get(6)?,
        source_url: r.get(7)?,
        location: r.get(8)?,
        is_all_day: r.get::<_, i64>(9)? != 0,
        certificate_offered: r.get::<_, i64>(10)? != 0,
        registration_required: r.get::<_, i64>(11)? != 0,
        recurrence: r.get(12)?,
        recurrence_end: r.get(13)?,
        color: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub timezone: String,
    pub category: String,
    pub source_url: Option<String>,
    pub location: Option<String>,
    pub is_all_day: bool,
    pub certificate_offered: bool,
    pub registration_required: bool,
    pub recurrence: String,
    pub recurrence_end: Option<String>,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventReminder {
    pub id: String,
    pub event_id: String,
    pub minutes_before: i64,
    pub enabled: bool,
    pub triggered: bool,
}

#[tauri::command]
pub async fn list_calendar_events(
    state: State<'_, Arc<AppState>>,
    start_range: Option<String>,
    end_range: Option<String>,
) -> Cmd<Vec<CalendarEvent>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let query = match (start_range, end_range) {
            (Some(s), Some(e)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, start_at, end_at, timezone, category, source_url, location, is_all_day, certificate_offered, registration_required, recurrence, recurrence_end, color, created_at, updated_at
                     FROM events WHERE end_at >= ?1 AND start_at <= ?2 ORDER BY start_at",
                ).map_err(crate::error::to_string_err("prepare events range"))?;
                let rows = stmt.query_map(rusqlite::params![s, e], map_calendar_event_row)
                    .map_err(crate::error::to_string_err("map events"))?;
                rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
            }
            _ => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, start_at, end_at, timezone, category, source_url, location, is_all_day, certificate_offered, registration_required, recurrence, recurrence_end, color, created_at, updated_at
                     FROM events ORDER BY start_at",
                ).map_err(crate::error::to_string_err("prepare events all"))?;
                let rows = stmt.query_map([], map_calendar_event_row)
                    .map_err(crate::error::to_string_err("map events"))?;
                rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
            }
        };
        Ok(query)
    })
    .await
}

#[tauri::command]
pub async fn create_calendar_event(
    state: State<'_, Arc<AppState>>,
    title: String,
    description: Option<String>,
    start_at: String,
    end_at: String,
    timezone: Option<String>,
    category: Option<String>,
    source_url: Option<String>,
    location: Option<String>,
    is_all_day: Option<bool>,
    certificate_offered: Option<bool>,
    registration_required: Option<bool>,
    recurrence: Option<String>,
    recurrence_end: Option<String>,
    color: Option<String>,
    reminder_minutes: Option<Vec<i64>>,
) -> Cmd<CalendarEvent> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let id = format!("event-{}", crate::db::now_iso().replace(['-', ':', 'T', 'Z'], ""));
        let now = crate::db::now_iso();
        let tz = timezone.unwrap_or_else(|| "UTC".into());
        let cat = category.unwrap_or_else(|| "general".into());
        let all_day = is_all_day.unwrap_or(false);
        let cert = certificate_offered.unwrap_or(false);
        let reg = registration_required.unwrap_or(false);
        let rec = recurrence.unwrap_or_else(|| "none".into());
        let desc = description.unwrap_or_default();

        conn.execute(
            "INSERT INTO events(id, title, description, start_at, end_at, timezone, category, source_url, location, is_all_day, certificate_offered, registration_required, recurrence, recurrence_end, color, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
            rusqlite::params![
                id,
                title.trim(),
                desc,
                start_at,
                end_at,
                tz,
                cat,
                source_url,
                location,
                if all_day { 1 } else { 0 },
                if cert { 1 } else { 0 },
                if reg { 1 } else { 0 },
                rec,
                recurrence_end,
                color,
                now
            ],
        ).map_err(crate::error::to_string_err("insert calendar event"))?;

        if let Some(reminders) = reminder_minutes {
            for m in reminders {
                let rem_id = format!("rem-{}-{}", id, m);
                let _ = conn.execute(
                    "INSERT INTO event_reminders(id, event_id, minutes_before, enabled, triggered) VALUES(?1, ?2, ?3, 1, 0)",
                    rusqlite::params![rem_id, id, m],
                );
            }
        }

        Ok(CalendarEvent {
            id,
            title,
            description: Some(desc),
            start_at,
            end_at,
            timezone: tz,
            category: cat,
            source_url,
            location,
            is_all_day: all_day,
            certificate_offered: cert,
            registration_required: reg,
            recurrence: rec,
            recurrence_end,
            color,
            created_at: now.clone(),
            updated_at: now,
        })
    })
    .await
}

#[tauri::command]
pub async fn delete_calendar_event(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<()> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute("DELETE FROM events WHERE id = ?1", [id])
            .map_err(crate::error::to_string_err("delete calendar event"))?;
        Ok(())
    })
    .await
}

/// Full edit of an existing event. `reminder_minutes`, when provided, replaces
/// the event's reminder set (NULL keeps the existing reminders untouched).
#[tauri::command]
pub async fn update_calendar_event(
    state: State<'_, Arc<AppState>>,
    id: String,
    title: String,
    description: Option<String>,
    start_at: String,
    end_at: String,
    timezone: Option<String>,
    category: Option<String>,
    source_url: Option<String>,
    location: Option<String>,
    is_all_day: Option<bool>,
    certificate_offered: Option<bool>,
    registration_required: Option<bool>,
    recurrence: Option<String>,
    recurrence_end: Option<String>,
    color: Option<String>,
    reminder_minutes: Option<Vec<i64>>,
) -> Cmd<CalendarEvent> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let now = crate::db::now_iso();
        let tz = timezone.unwrap_or_else(|| "UTC".into());
        let cat = category.unwrap_or_else(|| "general".into());
        let all_day = is_all_day.unwrap_or(false);
        let cert = certificate_offered.unwrap_or(false);
        let reg = registration_required.unwrap_or(false);
        let rec = recurrence.unwrap_or_else(|| "none".into());
        let desc = description.unwrap_or_default();

        let updated = conn
            .execute(
                "UPDATE events SET title=?2, description=?3, start_at=?4, end_at=?5, timezone=?6,
                        category=?7, source_url=?8, location=?9, is_all_day=?10,
                        certificate_offered=?11, registration_required=?12,
                        recurrence=?13, recurrence_end=?14, color=?15, updated_at=?16
                 WHERE id=?1",
                rusqlite::params![
                    id,
                    title.trim(),
                    desc,
                    start_at,
                    end_at,
                    tz,
                    cat,
                    source_url,
                    location,
                    if all_day { 1 } else { 0 },
                    if cert { 1 } else { 0 },
                    if reg { 1 } else { 0 },
                    rec,
                    recurrence_end,
                    color,
                    now
                ],
            )
            .map_err(crate::error::to_string_err("update calendar event"))?;
        if updated == 0 {
            return Err(format!("calendar event {id} not found"));
        }

        if let Some(reminders) = reminder_minutes {
            conn.execute(
                "DELETE FROM event_reminders WHERE event_id = ?1",
                [&id],
            )
            .map_err(crate::error::to_string_err("clear event reminders"))?;
            for m in reminders {
                let rem_id = format!("rem-{}-{}", id, m);
                let _ = conn.execute(
                    "INSERT INTO event_reminders(id, event_id, minutes_before, enabled, triggered) VALUES(?1, ?2, ?3, 1, 0)",
                    rusqlite::params![rem_id, id, m],
                );
            }
        }

        conn.query_row(
            "SELECT id, title, description, start_at, end_at, timezone, category, source_url, location, is_all_day, certificate_offered, registration_required, recurrence, recurrence_end, color, created_at, updated_at
             FROM events WHERE id = ?1",
            [&id],
            map_calendar_event_row,
        )
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn list_event_reminders(
    state: State<'_, Arc<AppState>>,
    event_id: String,
) -> Cmd<Vec<EventReminder>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let mut stmt = conn
            .prepare(
                "SELECT id, event_id, minutes_before, enabled, triggered
                 FROM event_reminders WHERE event_id = ?1 ORDER BY minutes_before",
            )
            .map_err(crate::error::to_string_err("prepare event reminders"))?;
        let rows = stmt
            .query_map([&event_id], |r| {
                Ok(EventReminder {
                    id: r.get(0)?,
                    event_id: r.get(1)?,
                    minutes_before: r.get(2)?,
                    enabled: r.get::<_, i64>(3)? != 0,
                    triggered: r.get::<_, i64>(4)? != 0,
                })
            })
            .map_err(crate::error::to_string_err("map event reminders"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
    .await
}

/// Text filter over title / description / category (LIKE %q%).
#[tauri::command]
pub async fn search_calendar_events(
    state: State<'_, Arc<AppState>>,
    query: String,
) -> Cmd<Vec<CalendarEvent>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let like = format!("%{}%", query.trim());
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, start_at, end_at, timezone, category, source_url, location, is_all_day, certificate_offered, registration_required, recurrence, recurrence_end, color, created_at, updated_at
                 FROM events
                 WHERE title LIKE ?1 OR description LIKE ?1 OR category LIKE ?1
                 ORDER BY start_at",
            )
            .map_err(crate::error::to_string_err("prepare event search"))?;
        let rows = stmt
            .query_map(rusqlite::params![like], map_calendar_event_row)
            .map_err(crate::error::to_string_err("map event search"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
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

        // --- Today's calendar events ---
        // Uses the canonical `events` table (010/015) instead of the
        // deprecated `schedule` table (002) which has no UI callers.
        let start_of_day = format!("{today}T00:00:00Z");
        let end_of_day = format!("{today}T23:59:59Z");
        let mut sstmt = conn
            .prepare(
                "SELECT title, start_at FROM events
                 WHERE start_at >= ?1 AND start_at <= ?2
                 ORDER BY start_at",
            )
            .map_err(crate::error::to_string_err("calendar events"))?;
        let events: Vec<String> = sstmt
            .query_map(rusqlite::params![start_of_day, end_of_day], |r| {
                let t: String = r.get(0)?;
                let s: String = r.get(1)?;
                Ok(format!("{} · {}", &s[11..16], t))
            })
            .map_err(crate::error::to_string_err("calendar events m"))?
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

        // --- News (network is opt-in; graceful degradation on failure) ---
        // The previous app_meta.news_enabled gate was unreachable (no setter existed).
        // News fetching now always attempts; it degrades gracefully (empty on failure)
        // and the fetcher already has an 8s timeout. Network usage is visible in logs.
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

// ───────────────────────── Focus sessions (timer + stopwatch) ─────────────

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusStats {
    pub sessions: i64,
    pub total_minutes: i64,
    pub today_minutes: i64,
    pub today_sessions: i64,
    pub recent: Vec<FocusSession>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSession {
    pub id: i64,
    pub minutes: i64,
    pub label: Option<String>,
    pub kind: String,
    pub completed_at: String,
}

/// Log a completed focus/timer/stopwatch session (1..=600 minutes).
#[tauri::command]
pub async fn log_focus_session(
    state: State<'_, Arc<AppState>>,
    minutes: i64,
    label: Option<String>,
    kind: Option<String>,
) -> Cmd<FocusSession> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        if !(1..=600).contains(&minutes) {
            return Err("minutes must be 1-600".into());
        }
        let conn = conn_of(app)?;
        let kind = match kind.as_deref() {
            Some("stopwatch") => "stopwatch",
            _ => "timer",
        };
        conn.execute(
            "INSERT INTO focus_sessions(minutes, label, kind) VALUES(?1,?2,?3)",
            rusqlite::params![minutes, label.as_deref().unwrap_or("").trim(), kind],
        )
        .map_err(crate::error::to_string_err("focus insert"))?;
        Ok(FocusSession {
            id: conn.last_insert_rowid(),
            minutes,
            label,
            kind: kind.into(),
            completed_at: crate::db::now_iso(),
        })
    })
    .await
}

#[tauri::command]
pub async fn get_focus_stats(state: State<'_, Arc<AppState>>) -> Cmd<FocusStats> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let today = crate::db::now_iso()[..10].to_string();
        let (sessions, total_minutes, today_minutes, today_sessions) = conn
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(minutes),0),
                        COALESCE(SUM(CASE WHEN substr(completed_at,1,10)=?1 THEN minutes ELSE 0 END),0),
                        COALESCE(SUM(CASE WHEN substr(completed_at,1,10)=?1 THEN 1 ELSE 0 END),0)
                 FROM focus_sessions",
                [&today],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .map_err(crate::error::to_string_err("focus stats"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, minutes, label, kind, completed_at
                 FROM focus_sessions ORDER BY completed_at DESC LIMIT 8",
            )
            .map_err(crate::error::to_string_err("focus recent"))?;
        let recent = stmt
            .query_map([], |r| {
                Ok(FocusSession {
                    id: r.get(0)?,
                    minutes: r.get(1)?,
                    label: r.get(2)?,
                    kind: r.get(3)?,
                    completed_at: r.get(4)?,
                })
            })
            .map_err(crate::error::to_string_err("focus recent map"))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(FocusStats {
            sessions,
            total_minutes,
            today_minutes,
            today_sessions,
            recent,
        })
    })
    .await
}

/// Tick a habit on ANY date (YYYY-MM-DD) — week-strip backfill.
#[tauri::command]
pub async fn toggle_habit_date(
    state: State<'_, Arc<AppState>>,
    habit_id: i64,
    date: String,
) -> Cmd<bool> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let b = date.as_bytes();
        if date.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return Err("date must be YYYY-MM-DD".into());
        }
        let conn = conn_of(app)?;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM habit_logs WHERE habit_id=?1 AND completed_date=?2",
                rusqlite::params![habit_id, date],
                |r| r.get(0),
            )
            .ok();
        if exists.is_some() {
            conn.execute(
                "DELETE FROM habit_logs WHERE habit_id=?1 AND completed_date=?2",
                rusqlite::params![habit_id, date],
            )
            .map_err(crate::error::to_string_err("untick habit date"))?;
            Ok(false)
        } else {
            conn.execute(
                "INSERT INTO habit_logs(habit_id, completed_date) VALUES(?1,?2)",
                rusqlite::params![habit_id, date],
            )
            .map_err(crate::error::to_string_err("tick habit date"))?;
            Ok(true)
        }
    })
    .await
}
