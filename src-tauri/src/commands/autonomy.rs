//! 🤖 Tauri commands exposing the Autonomous Runtime to the frontend.

use std::sync::Arc;

use tauri::State;
use serde::{Deserialize, Serialize};
use crate::autonomous::{
    AutonomyRuntime, EventKind, EventBus, NormalizedEvent, RuntimeMode, RuntimeStats,
    WorldState, CycleResult, CapabilityState, Registry,
};
use crate::state::AppState;
use crate::error;

use super::{blocking, Cmd};

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

// ---------------------------------------------------------------------------
// Capability Registry + Adaptive Scheduler + Decision Ledger (Phase 6)
// ---------------------------------------------------------------------------

/// Append-only audit ledger entry for the explainability requirement.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    pub id: i64,
    pub capability_id: String,
    pub decision: String,
    pub reason: String,
    pub score: Option<f64>,
    pub created_at: String,
}

/// List the capability registry with effective (override-applied) state.
#[tauri::command]
pub async fn list_capabilities(
    state: State<'_, Arc<AppState>>,
) -> Cmd<Vec<CapabilityState>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        Registry::load(&conn)
    })
    .await
}

/// Toggle a capability (persisted to capability_state + logged).
#[tauri::command]
pub async fn set_capability_enabled(
    state: State<'_, Arc<AppState>>,
    capability_id: String,
    enabled: bool,
) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        crate::autonomous::capability::def(&capability_id)
            .ok_or_else(|| format!("unknown capability: {capability_id}"))?;
        let reason = if enabled {
            "enabled by user".to_string()
        } else {
            "disabled by user".to_string()
        };
        Registry::set_enabled(&conn, &capability_id, enabled, &reason)?;
        crate::autonomous::scheduler::Scheduler::log(
            &conn,
            &capability_id,
            if enabled { "resume" } else { "pause" },
            &reason,
            None,
        );
        Ok(())
    })
    .await
}

/// Override a capability's interval (persisted + logged).
#[tauri::command]
pub async fn set_capability_interval(
    state: State<'_, Arc<AppState>>,
    capability_id: String,
    interval_secs: u64,
) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        crate::autonomous::capability::def(&capability_id)
            .ok_or_else(|| format!("unknown capability: {capability_id}"))?;
        let reason = format!("interval set to {interval_secs}s by user");
        Registry::set_interval(&conn, &capability_id, interval_secs, &reason)?;
        crate::autonomous::scheduler::Scheduler::log(
            &conn,
            &capability_id,
            "interval",
            &reason,
            None,
        );
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------------------
// Goal Engine (Phase 7) — deterministic, evidence-backed candidates
// ---------------------------------------------------------------------------

/// Read-only: generate goal candidates from existing storage.
#[tauri::command]
pub async fn get_goal_candidates(
    state: State<'_, Arc<AppState>>,
) -> Cmd<Vec<crate::autonomous::goal::GoalCandidate>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        crate::autonomous::goal::generate(&conn)
    })
    .await
}

/// Read-only: plan every generated goal candidate (Phase 8). Plans are
/// generation-based — no persistence, no execution, pure computation.
#[tauri::command]
pub async fn get_plans(
    state: State<'_, Arc<AppState>>,
) -> Cmd<Vec<serde_json::Value>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let goals = crate::autonomous::goal::generate(&conn)?;
        let out = goals
            .into_iter()
            .map(|g| {
                use crate::autonomous::planner::{plan as plan_goal, Planned};
                match plan_goal(&g) {
                    Planned::Plan(p) => serde_json::json!({ "kind": "plan", "value": p }),
                    Planned::Rejected(r) => serde_json::json!({ "kind": "rejected", "value": r }),
                }
            })
            .collect::<Vec<_>>();
        Ok(out)
    })
    .await
}

/// Read the decision ledger (newest first).
#[tauri::command]
pub async fn get_capability_ledger(
    state: State<'_, Arc<AppState>>,
    limit: Option<u32>,
) -> Cmd<Vec<LedgerEntry>> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let limit = limit.unwrap_or(100).clamp(1, 500) as i64;
        let mut stmt = conn
            .prepare(
                "SELECT id, capability_id, decision, reason, score, created_at
                 FROM autonomy_decisions ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit], |r| {
                Ok(LedgerEntry {
                    id: r.get(0)?,
                    capability_id: r.get(1)?,
                    decision: r.get(2)?,
                    reason: r.get(3)?,
                    score: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
    .await
}

// ─────────────────── Read-only observability for the UI ───────────────────

/// Read-only stats endpoint for the UI.
#[tauri::command]
pub async fn autonomy_get_stats(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    runtime: tauri::State<'_, AutonomyRuntime>,
) -> Cmd<RuntimeSnapshot> {
    let _ = state; // touch state to silence unused warning
    Ok(RuntimeSnapshot {
        mode: runtime.mode(),
        stats: runtime.stats(),
        world_state: runtime.world_state(),
    })
}

#[tauri::command]
pub async fn autonomy_get_ledger(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    limit: Option<usize>,
) -> Cmd<Vec<crate::autonomous::ledger::LedgerRow>> {
    let st = state.inner().clone();
    let limit = limit.unwrap_or(50);
    super::blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, capability_id, decision, reason, score, created_at
             FROM autonomy_decisions ORDER BY id DESC LIMIT ?1"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |r| {
            Ok(crate::autonomous::ledger::LedgerRow {
                id: r.get(0)?,
                capability_id: r.get(1)?,
                decision: r.get(2)?,
                reason: r.get(3)?,
                score: r.get(4)?,
                created_at: r.get(5)?,
            })
        }).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn autonomy_get_goals(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    limit: Option<usize>,
) -> Cmd<Vec<crate::autonomous::goal::GoalCandidate>> {
    let st = state.inner().clone();
    let limit = limit.unwrap_or(20);
    super::blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
        crate::autonomous::goal::generate(&conn).map(|mut g| {
            g.truncate(limit);
            g
        })
    })
    .await
}
