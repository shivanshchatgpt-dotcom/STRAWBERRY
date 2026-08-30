use std::sync::Arc;
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
    pub interval_minutes: i64,
    pub last_reminded_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCategoryArgs {
    pub category: String,
    pub enabled: bool,
    pub interval_minutes: i64,
}

#[tauri::command]
pub async fn wellness_get_state(agent: State<'_, Arc<WellnessAgent>>) -> Cmd<WellnessStateDto> {
    let state = WellnessAgent::get_state(agent.inner());
    Ok(WellnessStateDto {
        enabled: state.enabled,
        next_reminder_in_secs: state.next_reminder_in_secs,
        last_category: state.last_category,
        snoozed_until: state.snoozed_until,
    })
}

#[tauri::command]
pub async fn wellness_set_enabled(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, enabled: bool) -> Cmd<()> {
    WellnessAgent::set_enabled(agent.inner(), &app, enabled)
}

#[tauri::command]
pub async fn wellness_snooze(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, minutes: i64) -> Cmd<()> {
    WellnessAgent::snooze(agent.inner(), &app, minutes)
}

#[tauri::command]
pub async fn wellness_get_config(_agent: State<'_, Arc<WellnessAgent>>, app: AppHandle) -> Cmd<Vec<WellnessConfigDto>> {
    let configs = WellnessAgent::get_config(&app)?;
    Ok(configs.into_iter().map(|c| WellnessConfigDto {
        category: c.category,
        enabled: c.enabled,
        interval_minutes: c.interval_minutes,
        last_reminded_at: c.last_reminded_at,
    }).collect())
}

#[tauri::command]
pub async fn wellness_set_category(_agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, args: SetCategoryArgs) -> Cmd<()> {
    WellnessAgent::set_category(&app, args.category, args.enabled, args.interval_minutes)
}

#[tauri::command]
pub async fn wellness_record_activity(_agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, source: String) -> Cmd<()> {
    WellnessAgent::record_activity(&app, &source)
}

#[tauri::command]
pub async fn wellness_dismiss(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle) -> Cmd<()> {
    WellnessAgent::dismiss(agent.inner(), &app);
    Ok(())
}
