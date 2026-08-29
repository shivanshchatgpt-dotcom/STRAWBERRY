//! 🤖 Tauri commands exposing the Autonomous Runtime to the frontend.

use tauri::State;
use serde::{Deserialize, Serialize};
use crate::autonomous::{
    AutonomyRuntime, EventKind, EventBus, NormalizedEvent, RuntimeMode, RuntimeStats,
    WorldState, CycleResult,
};

use super::Cmd;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishEventArgs {
    pub kind: String,
    pub data: serde_json::Value,
}

#[tauri::command]
pub async fn autonomy_get_state(runtime: State<'_, AutonomyRuntime>) -> Cmd<RuntimeSnapshot> {
    Ok(RuntimeSnapshot {
        mode: runtime.mode(),
        stats: runtime.stats(),
        world_state: runtime.world_state(),
    })
}

#[tauri::command]
pub async fn autonomy_start(runtime: State<'_, AutonomyRuntime>) -> Cmd<RuntimeMode> {
    runtime.start();
    Ok(runtime.mode())
}

#[tauri::command]
pub async fn autonomy_pause(runtime: State<'_, AutonomyRuntime>) -> Cmd<RuntimeMode> {
    runtime.pause();
    Ok(runtime.mode())
}

#[tauri::command]
pub async fn autonomy_resume(runtime: State<'_, AutonomyRuntime>) -> Cmd<RuntimeMode> {
    runtime.resume();
    Ok(runtime.mode())
}

#[tauri::command]
pub async fn autonomy_shutdown(runtime: State<'_, AutonomyRuntime>) -> Cmd<RuntimeMode> {
    runtime.shutdown();
    Ok(runtime.mode())
}

#[tauri::command]
pub async fn autonomy_run_cycle(
    runtime: State<'_, AutonomyRuntime>,
    max_events: Option<usize>,
) -> Cmd<CycleResult> {
    let n = max_events.unwrap_or(32);
    Ok(runtime.run_cycle(n))
}

#[tauri::command]
pub async fn autonomy_publish(
    runtime: State<'_, AutonomyRuntime>,
    args: PublishEventArgs,
) -> Cmd<u64> {
    let ev = event_from_json(&args.kind, args.data)?;
    let id = ev.id.raw();
    runtime.publish(ev);
    Ok(id)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub mode: RuntimeMode,
    pub stats: RuntimeStats,
    pub world_state: WorldState,
}

fn event_from_json(kind: &str, data: serde_json::Value) -> Result<NormalizedEvent, String> {
    let ek = match kind {
        "active_app_changed" => EventKind::ActiveAppChanged {
            from: data.get("from").and_then(|v| v.as_str()).map(|s| s.to_string()),
            to: data.get("to").and_then(|v| v.as_str()).ok_or("missing 'to'")?.to_string(),
        },
        "file_opened" => EventKind::FileOpened {
            path: data.get("path").and_then(|v| v.as_str()).ok_or("missing 'path'")?.to_string(),
            project: data.get("project").and_then(|v| v.as_str()).map(|s| s.to_string()),
        },
        "file_modified" => EventKind::FileModified {
            path: data.get("path").and_then(|v| v.as_str()).ok_or("missing 'path'")?.to_string(),
            project: data.get("project").and_then(|v| v.as_str()).map(|s| s.to_string()),
        },
        "chat_opened" => EventKind::ChatOpened {
            chat_id: data.get("chatId").and_then(|v| v.as_str()).ok_or("missing 'chatId'")?.to_string(),
            title: data.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        },
        "chat_created" => EventKind::ChatCreated {
            chat_id: data.get("chatId").and_then(|v| v.as_str()).ok_or("missing 'chatId'")?.to_string(),
            title: data.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        },
        "folder_opened" => EventKind::FolderOpened {
            folder_id: data.get("folderId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: data.get("name").and_then(|v| v.as_str()).ok_or("missing 'name'")?.to_string(),
        },
        "search_executed" => EventKind::SearchExecuted {
            query: data.get("query").and_then(|v| v.as_str()).ok_or("missing 'query'")?.to_string(),
            result_count: data.get("resultCount").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        },
        "build_state_changed" => EventKind::BuildStateChanged {
            state: data.get("state").and_then(|v| v.as_str()).ok_or("missing 'state'")?.to_string(),
            project: data.get("project").and_then(|v| v.as_str()).map(|s| s.to_string()),
        },
        "todo_toggled" => EventKind::TodoToggled {
            id: data.get("id").and_then(|v| v.as_u64()).unwrap_or(0),
            completed: data.get("completed").and_then(|v| v.as_bool()).unwrap_or(false),
        },
        "focus_session_changed" => EventKind::FocusSessionChanged {
            state: data.get("state").and_then(|v| v.as_str()).ok_or("missing 'state'")?.to_string(),
            minutes: data.get("minutes").and_then(|v| v.as_u64()).unwrap_or(0),
        },
        "tab_visited" => EventKind::TabVisited {
            url: data.get("url").and_then(|v| v.as_str()).ok_or("missing 'url'")?.to_string(),
            title: data.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
        },
        "inbox_added" => EventKind::InboxAdded {
            kind: data.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            preview: data.get("preview").and_then(|v| v.as_str()).ok_or("missing 'preview'")?.to_string(),
        },
        "screen_captured" => EventKind::ScreenCaptured {
            window_title: data.get("windowTitle").and_then(|v| v.as_str()).map(|s| s.to_string()),
            app: data.get("app").and_then(|v| v.as_str()).map(|s| s.to_string()),
        },
        "wellness_break" => EventKind::WellnessBreak {
            category: data.get("category").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        },
        "heartbeat" => EventKind::Heartbeat {
            source: data.get("source").and_then(|v| v.as_str()).unwrap_or("frontend").to_string(),
        },
        "error_observed" => EventKind::ErrorObserved {
            message: data.get("message").and_then(|v| v.as_str()).ok_or("missing 'message'")?.to_string(),
            source: data.get("source").and_then(|v| v.as_str()).ok_or("missing 'source'")?.to_string(),
        },
        _ => return Err(format!("unknown event kind: {kind}")),
    };
    Ok(NormalizedEvent::new(ek))
}
