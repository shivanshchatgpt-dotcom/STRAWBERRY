use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindowBuilder, WebviewUrl};
use crate::wellness::WellnessAgent;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowPopupArgs {
    pub category: String,
    pub title: String,
    pub message: String,
    pub emoji: String,
    pub duration_secs: i64,
}

pub fn show_popup(app: &AppHandle, args: ShowPopupArgs) {
    let label = format!("wellness-popup-{}", uuid::Uuid::new_v4());
    let url = WebviewUrl::App("index.html#/wellness-popup".into());

    let builder = WebviewWindowBuilder::new(app, label.clone(), url)
        .title("🍓 Wellness")
        .inner_size(400.0, 90.0)
        .min_inner_size(360.0, 70.0)
        .max_inner_size(500.0, 120.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(true);

    let _ = builder.build();

    let _ = app.emit("wellness:popup-shown", args);
}
