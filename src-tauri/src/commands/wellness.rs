use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use serde::{Deserialize, Serialize};
use crate::wellness::WellnessAgent;

pub type Cmd<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WellnessStateDto {
    pub enabled: bool,
    pub next_reminder_in_secs: i64,
    pub last_category: Option<String>,
    pub snoozed_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WellnessConfigDto {
    pub category: String,
    pub enabled: bool,
    pub interval_seconds: i64,
    pub last_reminded_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCategoryArgs {
    pub category: String,
    pub enabled: bool,
    pub interval_seconds: i64,
}

#[tauri::command]
pub async fn wellness_get_state(agent: State<'_, Arc<Mutex<WellnessAgent>>>) -> Cmd<WellnessStateDto> {
    let state = WellnessAgent::get_state(agent.inner());
    Ok(WellnessStateDto {
        enabled: state.enabled,
        next_reminder_in_secs: state.next_reminder_in_secs,
        last_category: state.last_category,
        snoozed_until: state.snoozed_until,
    })
}

#[tauri::command]
pub async fn wellness_set_enabled(agent: State<'_, Arc<Mutex<WellnessAgent>>>, app: AppHandle, enabled: bool) -> Cmd<()> {
    WellnessAgent::set_enabled(agent.inner(), &app, enabled)
}

#[tauri::command]
pub async fn wellness_snooze(agent: State<'_, Arc<Mutex<WellnessAgent>>>, app: AppHandle, minutes: i64) -> Cmd<()> {
    WellnessAgent::snooze(agent.inner(), &app, minutes)
}

#[tauri::command]
pub async fn wellness_get_config(_agent: State<'_, Arc<Mutex<WellnessAgent>>>, app: AppHandle) -> Cmd<Vec<WellnessConfigDto>> {
    let configs = WellnessAgent::get_config(&app)?;
    Ok(configs.into_iter().map(|c| WellnessConfigDto {
        category: c.category,
        enabled: c.enabled,
        interval_seconds: c.interval_seconds,
        last_reminded_at: c.last_reminded_at,
    }).collect())
}

#[tauri::command]
pub async fn wellness_set_category(_agent: State<'_, Arc<Mutex<WellnessAgent>>>, app: AppHandle, args: SetCategoryArgs) -> Cmd<()> {
    WellnessAgent::set_category(&app, args.category, args.enabled, args.interval_seconds)
}

#[tauri::command]
pub async fn wellness_record_activity(_agent: State<'_, Arc<Mutex<WellnessAgent>>>, app: AppHandle, source: String) -> Cmd<()> {
    WellnessAgent::record_activity(&app, &source)
}

#[tauri::command]
pub async fn wellness_dismiss(agent: State<'_, Arc<Mutex<WellnessAgent>>>, app: AppHandle) -> Cmd<()> {
    WellnessAgent::dismiss(agent.inner(), &app);
    Ok(())
}

/// Fire a reminder popup immediately so the user can verify the whole
/// pipeline (Rust → event → frontend overlay) without waiting out an interval.
#[tauri::command]
pub async fn wellness_test_popup(app: AppHandle, category: Option<String>) -> Cmd<()> {
    use tauri::Emitter;
    let cat = category.unwrap_or_else(|| "blink".to_string());
    let (emoji, title, message) = match cat.as_str() {
        "water" => ("💧", "💧 Drink some water", "Test reminder — pipeline working."),
        "stretch" => ("🧍", "🧍 Stand & stretch", "Test reminder — pipeline working."),
        "posture" => ("🪴", "🪴 Posture check", "Test reminder — pipeline working."),
        "eyes" => ("👁️", "👁️ Eye break", "Test reminder — pipeline working."),
        "meal" => ("🍴", "🍴 Meal / snack", "Test reminder — pipeline working."),
        _ => ("👀", "👀 Blink your eyes", "Test reminder — pipeline working."),
    };
    let reminder = crate::wellness::WellnessReminder {
        category: cat,
        title: title.to_string(),
        message: message.to_string(),
        emoji: emoji.to_string(),
        duration_secs: 5,
    };
    let _ = app.emit("wellness:popup", reminder);
    Ok(())
}
