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
pub async fn wellness_get_state(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle) -> Cmd<WellnessStateDto> {
    let state = agent.get_state(&app)?;
    Ok(WellnessStateDto {
        enabled: state.enabled,
        next_reminder_in_secs: state.next_reminder_in_secs,
        last_category: state.last_category,
        snoozed_until: state.snoozed_until,
    })
}

#[tauri::command]
pub async fn wellness_set_enabled(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, enabled: bool) -> Cmd<()> {
    agent.set_enabled(&app, enabled)
}

#[tauri::command]
pub async fn wellness_snooze(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, minutes: i64) -> Cmd<()> {
    agent.snooze(&app, minutes)
}

#[tauri::command]
pub async fn wellness_get_config(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle) -> Cmd<Vec<WellnessConfigDto>> {
    let configs = agent.get_config(&app)?;
    Ok(configs.into_iter().map(|c| WellnessConfigDto {
        category: c.category,
        enabled: c.enabled,
        interval_minutes: c.interval_minutes,
        last_reminded_at: c.last_reminded_at,
    }).collect())
}

#[tauri::command]
pub async fn wellness_set_category(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, args: SetCategoryArgs) -> Cmd<()> {
    agent.set_category(&app, args.category, args.enabled, args.interval_minutes)
}

#[tauri::command]
pub async fn wellness_record_activity(agent: State<'_, Arc<WellnessAgent>>, app: AppHandle, source: String) -> Cmd<()> {
    agent.record_activity(&app, &source)
}

#[tauri::command]
pub async fn wellness_dismiss(agent: State<'_, Arc<WellnessAgent>>) -> Cmd<()> {
    agent.dismiss();
    Ok(())
}
