//! 🍓 Workspace Resume v0.1 Commands and Persistence Handler.

use std::sync::Arc;
use tauri::State;

use super::Cmd;
use crate::state::AppState;
use crate::workspace::v1::{
    ActionResult, ItemStatus, SessionStatus, WorkspaceAction, WorkspaceItem, WorkspaceRestoreAttempt, WorkspaceSession,
};

fn conn_of(app: &AppState) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    app.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn gen_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{t:x}-{n:x}")
}

pub fn save_session(conn: &rusqlite::Connection, session: &WorkspaceSession) -> Result<(), String> {
    conn.execute(
        "INSERT INTO workspace_sessions(id, name, created_at, frozen_at, resumed_at, status, trigger, metadata_json)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            name=excluded.name,
            frozen_at=excluded.frozen_at,
            resumed_at=excluded.resumed_at,
            status=excluded.status,
            metadata_json=excluded.metadata_json",
        rusqlite::params![
            session.id,
            session.name,
            session.created_at,
            session.frozen_at,
            session.resumed_at,
            session.status.as_str(),
            session.trigger,
            session.metadata_json,
        ],
    )
    .map_err(crate::error::to_string_err("save_session"))?;

    for item in &session.items {
        save_item(conn, item)?;
    }

    Ok(())
}

pub fn save_item(conn: &rusqlite::Connection, item: &WorkspaceItem) -> Result<(), String> {
    conn.execute(
        "INSERT INTO workspace_items(
            id, session_id, item_type, app_name, process_name, window_title, window_geometry,
            workspace, cwd, command, browser_url, browser_title, restore_strategy,
            restore_status, error_message, action_type, action_target, action_payload,
            display_label, last_action_at, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT(id) DO UPDATE SET
            restore_status=excluded.restore_status,
            error_message=excluded.error_message,
            last_action_at=excluded.last_action_at",
        rusqlite::params![
            item.id,
            item.session_id,
            item.item_type,
            item.app_name,
            item.process_name,
            item.window_title,
            item.window_geometry,
            item.workspace,
            item.cwd,
            item.command,
            item.browser_url,
            item.browser_title,
            item.restore_strategy,
            item.restore_status.as_str(),
            item.error_message,
            item.action_type,
            item.action_target,
            item.action_payload,
            item.display_label,
            item.last_action_at,
            item.created_at,
        ],
    )
    .map_err(crate::error::to_string_err("save_item"))?;
    Ok(())
}

pub fn load_session(conn: &rusqlite::Connection, id: &str) -> Result<Option<WorkspaceSession>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, created_at, frozen_at, resumed_at, status, trigger, metadata_json FROM workspace_sessions WHERE id = ?1")
        .map_err(crate::error::to_string_err("prep session load"))?;

    let sess_opt = stmt
        .query_row([id], |r| {
            let status_str: String = r.get(5)?;
            Ok(WorkspaceSession {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                frozen_at: r.get(3)?,
                resumed_at: r.get(4)?,
                status: SessionStatus::from_str(&status_str),
                trigger: r.get(6)?,
                metadata_json: r.get(7)?,
                items: Vec::new(),
            })
        })
        .ok();

    let Some(mut session) = sess_opt else { return Ok(None) };

    let mut istmt = conn
        .prepare("SELECT id, session_id, item_type, app_name, process_name, window_title, window_geometry, workspace, cwd, command, browser_url, browser_title, restore_strategy, restore_status, error_message, action_type, action_target, action_payload, display_label, last_action_at, created_at FROM workspace_items WHERE session_id = ?1 ORDER BY created_at")
        .map_err(crate::error::to_string_err("prep items load"))?;

    let items = istmt
        .query_map([id], |r| {
            let status_str: String = r.get(13)?;
            Ok(WorkspaceItem {
                id: r.get(0)?,
                session_id: r.get(1)?,
                item_type: r.get(2)?,
                app_name: r.get(3)?,
                process_name: r.get(4)?,
                window_title: r.get(5)?,
                window_geometry: r.get(6)?,
                workspace: r.get(7)?,
                cwd: r.get(8)?,
                command: r.get(9)?,
                browser_url: r.get(10)?,
                browser_title: r.get(11)?,
                restore_strategy: r.get(12)?,
                restore_status: ItemStatus::from_str(&status_str),
                error_message: r.get(14)?,
                action_type: r.get(15)?,
                action_target: r.get(16)?,
                action_payload: r.get(17)?,
                display_label: r.get(18)?,
                last_action_at: r.get(19)?,
                created_at: r.get(20)?,
            })
        })
        .map_err(crate::error::to_string_err("items map"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::error::to_string_err("items collect"))?;

    session.items = items;
    Ok(Some(session))
}

#[tauri::command]
pub async fn capture_workspace_snapshot(state: State<'_, Arc<AppState>>) -> Cmd<WorkspaceSession> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let live = crate::workspace::collect();
        let ts = now_ts();
        let session_id = gen_id("ws-session");

        let mut items = Vec::new();

        // 1. Windows (including VS Code)
        for w in &live.windows {
            let item_id = gen_id("ws-item");
            let is_vscode = w.app.eq_ignore_ascii_case("code") || w.app.to_lowercase().contains("vscode");
            let item_type = if is_vscode { "vscode" } else { "window" };

            let (act_type, act_target, act_payload, label) = if is_vscode {
                let path = w.title.split('-').next().unwrap_or("").trim().to_string();
                let action = WorkspaceAction::OpenVsCodeProject { path: path.clone() };
                (
                    Some(action.action_type_str().to_string()),
                    Some(path),
                    serde_json::to_string(&action).ok(),
                    Some(action.display_label()),
                )
            } else {
                (None, None, None, Some(format!("{}: {}", w.app, w.title)))
            };

            items.push(WorkspaceItem {
                id: item_id,
                session_id: session_id.clone(),
                item_type: item_type.into(),
                app_name: Some(w.app.clone()),
                process_name: Some(w.app.clone()),
                window_title: Some(w.title.clone()),
                window_geometry: Some(format!("{},{},{},{}", w.x, w.y, w.w, w.h)),
                workspace: Some(w.desktop.to_string()),
                cwd: None,
                command: w.launch.clone(),
                browser_url: None,
                browser_title: None,
                restore_strategy: "auto".into(),
                restore_status: ItemStatus::Pending,
                error_message: None,
                action_type: act_type,
                action_target: act_target,
                action_payload: act_payload,
                display_label: label,
                last_action_at: None,
                created_at: ts,
            });
        }

        // 2. Browser URLs
        for b in &live.browsers {
            for url in &b.urls {
                let item_id = gen_id("ws-item");
                let action = WorkspaceAction::OpenUrl { url: url.clone() };
                items.push(WorkspaceItem {
                    id: item_id,
                    session_id: session_id.clone(),
                    item_type: "browser".into(),
                    app_name: Some(b.browser.clone()),
                    process_name: Some(b.browser.clone()),
                    window_title: Some(url.clone()),
                    window_geometry: None,
                    workspace: None,
                    cwd: None,
                    command: None,
                    browser_url: Some(url.clone()),
                    browser_title: Some(url.clone()),
                    restore_strategy: "auto".into(),
                    restore_status: ItemStatus::Pending,
                    error_message: None,
                    action_type: Some(action.action_type_str().to_string()),
                    action_target: Some(url.clone()),
                    action_payload: serde_json::to_string(&action).ok(),
                    display_label: Some(action.display_label()),
                    last_action_at: None,
                    created_at: ts,
                });
            }
        }

        // 3. Terminals
        for cwd in &live.terminals {
            let item_id = gen_id("ws-item");
            let action = WorkspaceAction::OpenTerminal { cwd: cwd.clone() };
            items.push(WorkspaceItem {
                id: item_id,
                session_id: session_id.clone(),
                item_type: "terminal".into(),
                app_name: Some("terminal".into()),
                process_name: Some("terminal".into()),
                window_title: Some(format!("Terminal @ {cwd}")),
                window_geometry: None,
                workspace: None,
                cwd: Some(cwd.clone()),
                command: None,
                browser_url: None,
                browser_title: None,
                restore_strategy: "auto".into(),
                restore_status: ItemStatus::Pending,
                error_message: None,
                action_type: Some(action.action_type_str().to_string()),
                action_target: Some(cwd.clone()),
                action_payload: serde_json::to_string(&action).ok(),
                display_label: Some(action.display_label()),
                last_action_at: None,
                created_at: ts,
            });
        }

        let session = WorkspaceSession {
            id: session_id,
            name: live.name,
            created_at: ts,
            frozen_at: None,
            resumed_at: None,
            status: SessionStatus::Capturing,
            trigger: "manual".into(),
            metadata_json: Some(live.story),
            items,
        };

        let conn = conn_of(app)?;
        save_session(&conn, &session)?;
        Ok(session)
    })
    .await
}

#[tauri::command]
pub async fn freeze_workspace(state: State<'_, Arc<AppState>>) -> Cmd<WorkspaceSession> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;

        // Capture live snapshot first
        let mut session = {
            let live = crate::workspace::collect();
            let ts = now_ts();
            let session_id = gen_id("ws-session");
            let mut items = Vec::new();

            for w in &live.windows {
                let item_id = gen_id("ws-item");
                let is_vscode = w.app.eq_ignore_ascii_case("code") || w.app.to_lowercase().contains("vscode");
                let item_type = if is_vscode { "vscode" } else { "window" };

                let (act_type, act_target, act_payload, label) = if is_vscode {
                    let path = w.title.split('-').next().unwrap_or("").trim().to_string();
                    let action = WorkspaceAction::OpenVsCodeProject { path: path.clone() };
                    (
                        Some(action.action_type_str().to_string()),
                        Some(path),
                        serde_json::to_string(&action).ok(),
                        Some(action.display_label()),
                    )
                } else {
                    (None, None, None, Some(format!("{}: {}", w.app, w.title)))
                };

                items.push(WorkspaceItem {
                    id: item_id,
                    session_id: session_id.clone(),
                    item_type: item_type.into(),
                    app_name: Some(w.app.clone()),
                    process_name: Some(w.app.clone()),
                    window_title: Some(w.title.clone()),
                    window_geometry: Some(format!("{},{},{},{}", w.x, w.y, w.w, w.h)),
                    workspace: Some(w.desktop.to_string()),
                    cwd: None,
                    command: w.launch.clone(),
                    browser_url: None,
                    browser_title: None,
                    restore_strategy: "auto".into(),
                    restore_status: ItemStatus::Pending,
                    error_message: None,
                    action_type: act_type,
                    action_target: act_target,
                    action_payload: act_payload,
                    display_label: label,
                    last_action_at: None,
                    created_at: ts,
                });
            }

            for b in &live.browsers {
                for url in &b.urls {
                    let item_id = gen_id("ws-item");
                    let action = WorkspaceAction::OpenUrl { url: url.clone() };
                    items.push(WorkspaceItem {
                        id: item_id,
                        session_id: session_id.clone(),
                        item_type: "browser".into(),
                        app_name: Some(b.browser.clone()),
                        process_name: Some(b.browser.clone()),
                        window_title: Some(url.clone()),
                        window_geometry: None,
                        workspace: None,
                        cwd: None,
                        command: None,
                        browser_url: Some(url.clone()),
                        browser_title: Some(url.clone()),
                        restore_strategy: "auto".into(),
                        restore_status: ItemStatus::Pending,
                        error_message: None,
                        action_type: Some(action.action_type_str().to_string()),
                        action_target: Some(url.clone()),
                        action_payload: serde_json::to_string(&action).ok(),
                        display_label: Some(action.display_label()),
                        last_action_at: None,
                        created_at: ts,
                    });
                }
            }

            for cwd in &live.terminals {
                let item_id = gen_id("ws-item");
                let action = WorkspaceAction::OpenTerminal { cwd: cwd.clone() };
                items.push(WorkspaceItem {
                    id: item_id,
                    session_id: session_id.clone(),
                    item_type: "terminal".into(),
                    app_name: Some("terminal".into()),
                    process_name: Some("terminal".into()),
                    window_title: Some(format!("Terminal @ {cwd}")),
                    window_geometry: None,
                    workspace: None,
                    cwd: Some(cwd.clone()),
                    command: None,
                    browser_url: None,
                    browser_title: None,
                    restore_strategy: "auto".into(),
                    restore_status: ItemStatus::Pending,
                    error_message: None,
                    action_type: Some(action.action_type_str().to_string()),
                    action_target: Some(cwd.clone()),
                    action_payload: serde_json::to_string(&action).ok(),
                    display_label: Some(action.display_label()),
                    last_action_at: None,
                    created_at: ts,
                });
            }

            WorkspaceSession {
                id: session_id,
                name: live.name,
                created_at: ts,
                frozen_at: Some(ts),
                resumed_at: None,
                status: SessionStatus::Frozen,
                trigger: "freeze_now".into(),
                metadata_json: Some(live.story),
                items,
            }
        };

        session.frozen_at = Some(now_ts());
        session.status = SessionStatus::Frozen;
        save_session(&conn, &session)?;
        Ok(session)
    })
    .await
}

#[tauri::command]
pub async fn list_workspace_sessions(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Cmd<Vec<WorkspaceSession>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let max = limit.unwrap_or(20).clamp(1, 100);
        let mut stmt = conn
            .prepare("SELECT id FROM workspace_sessions ORDER BY created_at DESC LIMIT ?1")
            .map_err(crate::error::to_string_err("list_workspace_sessions prep"))?;

        let ids: Vec<String> = stmt
            .query_map([max as i64], |r| r.get(0))
            .map_err(crate::error::to_string_err("list_workspace_sessions query"))?
            .filter_map(|r| r.ok())
            .collect();

        let mut sessions = Vec::new();
        for id in ids {
            if let Some(s) = load_session(&conn, &id)? {
                sessions.push(s);
            }
        }
        Ok(sessions)
    })
    .await
}

#[tauri::command]
pub async fn get_workspace_session(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<Option<WorkspaceSession>> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        load_session(&conn, &id)
    })
    .await
}

#[tauri::command]
pub async fn delete_workspace_session(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<()> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute("DELETE FROM workspace_sessions WHERE id = ?1", [id])
            .map_err(crate::error::to_string_err("delete_workspace_session"))?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn open_workspace_item(
    state: State<'_, Arc<AppState>>,
    item_id: String,
    confirmed: Option<bool>,
) -> Cmd<ActionResult> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let mut stmt = conn
            .prepare("SELECT id, session_id, action_payload FROM workspace_items WHERE id = ?1")
            .map_err(crate::error::to_string_err("open_workspace_item prep"))?;

        let payload: String = stmt
            .query_row([&item_id], |r| r.get(2))
            .map_err(|e| format!("Workspace item '{item_id}' has no stored action: {e}"))?;

        let mut action: WorkspaceAction = serde_json::from_str(&payload)
            .map_err(|e| format!("Failed to parse stored action payload: {e}"))?;

        if let WorkspaceAction::RunTerminalCommand { ref mut confirmed: conf, .. } = action {
            *conf = confirmed.unwrap_or(false);
        }

        let start_time = now_ts();

        let exec_result = action.execute();
        let (success, message) = match exec_result {
            Ok(msg) => (true, msg),
            Err(e) => (false, e),
        };

        let status = if success { ItemStatus::Restored } else { ItemStatus::Failed };

        let _ = conn.execute(
            "UPDATE workspace_items SET restore_status = ?1, error_message = ?2, last_action_at = ?3 WHERE id = ?4",
            rusqlite::params![status.as_str(), if success { None } else { Some(&message) }, start_time, item_id],
        );

        let att_id = gen_id("ws-att");
        let _ = conn.execute(
            "INSERT INTO workspace_restore_attempts(id, session_id, item_id, started_at, finished_at, status, error_message)
             VALUES(?1, (SELECT session_id FROM workspace_items WHERE id = ?2), ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![att_id, item_id, start_time, now_ts(), status.as_str(), if success { None } else { Some(&message) }],
        );

        Ok(ActionResult {
            success,
            item_id,
            message,
        })
    })
    .await
}

#[tauri::command]
pub async fn resume_workspace_session(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<WorkspaceSession> {
    let st = state.inner().clone();
    super::blocking(st, move |app| {
        let conn = conn_of(app)?;
        let Some(mut session) = load_session(&conn, &id)? else {
            return Err(format!("Workspace session '{id}' not found."));
        };

        session.status = SessionStatus::Restoring;
        save_session(&conn, &session)?;

        let mut success_count = 0;
        let mut fail_count = 0;

        for item in &mut session.items {
            if let Some(ref payload) = item.action_payload {
                if let Ok(action) = serde_json::from_str::<WorkspaceAction>(payload) {
                    item.restore_status = ItemStatus::Launching;
                    let res = action.execute();
                    match res {
                        Ok(msg) => {
                            item.restore_status = ItemStatus::Restored;
                            item.error_message = None;
                            success_count += 1;
                        }
                        Err(e) => {
                            item.restore_status = ItemStatus::Failed;
                            item.error_message = Some(e);
                            fail_count += 1;
                        }
                    }
                    save_item(&conn, item)?;
                }
            }
        }

        session.resumed_at = Some(now_ts());
        session.status = if fail_count == 0 {
            SessionStatus::Restored
        } else if success_count > 0 {
            SessionStatus::Partial
        } else {
            SessionStatus::Failed
        };

        save_session(&conn, &session)?;
        Ok(session)
    })
    .await
}

#[tauri::command]
pub async fn retry_workspace_item(
    state: State<'_, Arc<AppState>>,
    item_id: String,
) -> Cmd<ActionResult> {
    open_workspace_item(state, item_id, Some(true)).await
}
