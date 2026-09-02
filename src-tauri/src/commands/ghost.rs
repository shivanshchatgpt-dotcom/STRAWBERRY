//! 👻 Ghost commands — record activity, rebuild graph, get insights/snapshot.

use std::sync::Arc;
use tauri::State;
use serde::{Deserialize, Serialize};
use crate::state::AppState;
use crate::ghost::{
    self, EventType, GhostSnapshot, GhostStats, Graph, GhostInsight,
    AttentionCell, GhostEvent,
};

use super::blocking;

pub type Cmd<T> = Result<T, String>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordEventArgs {
    pub event_type: String,
    pub source_id: Option<String>,
    pub source_kind: Option<String>,
    pub duration_ms: Option<i64>,
    pub metadata: Option<String>,
}

fn parse_event(s: &str) -> Option<EventType> {
    Some(match s {
        "open_chat" => EventType::OpenChat,
        "open_folder" => EventType::OpenFolder,
        "open_root" => EventType::OpenRoot,
        "create_chat" => EventType::CreateChat,
        "create_folder" => EventType::CreateFolder,
        "search" => EventType::Search,
        "capture" => EventType::Capture,
        "todo_add" => EventType::TodoAdd,
        "todo_done" => EventType::TodoDone,
        "habit_done" => EventType::HabitDone,
        "focus_session" => EventType::FocusSession,
        "tab_visit" => EventType::TabVisit,
        "note_view" => EventType::NoteView,
        "inbox_add" => EventType::InboxAdd,
        _ => return None,
    })
}

#[tauri::command]
pub async fn ghost_record_event(
    state: State<'_, Arc<AppState>>,
    args: RecordEventArgs,
) -> Cmd<i64> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;
        let ev = match parse_event(&args.event_type) {
            Some(e) => e,
            None => return Err(format!("Unknown ghost event type: '{}'. Valid types: open_chat, open_folder, open_root, create_chat, create_folder, search, capture, todo_add, todo_done, habit_done, focus_session, tab_visit, note_view, inbox_add", args.event_type)),
        };
        let id = ghost::tracker::record(
            &conn,
            ev,
            args.source_id.as_deref(),
            args.source_kind.as_deref(),
            args.duration_ms.unwrap_or(0),
            args.metadata.as_deref(),
        )?;
        Ok(id)
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildGraphArgs {
    pub rebuild: bool,
}

#[tauri::command]
pub async fn ghost_rebuild_graph(
    state: State<'_, Arc<AppState>>,
) -> Cmd<(usize, usize)> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;
        let (nodes, edges) = ghost::graph::rebuild(&conn)?;
        Ok((nodes.len(), edges.len()))
    })
    .await
}

#[tauri::command]
pub async fn ghost_regenerate_insights(
    state: State<'_, Arc<AppState>>,
) -> Cmd<usize> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;
        let insights = ghost::insights::regenerate(&conn)?;
        Ok(insights.len())
    })
    .await
}

#[tauri::command]
pub async fn ghost_get_snapshot(
    state: State<'_, Arc<AppState>>,
) -> Cmd<GhostSnapshot> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;

        // Stats
        let total_events: i64 = ghost::tracker::count_all(&conn)?;
        let total_chats: i64 = conn.query_row("SELECT COUNT(*) FROM chats", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let total_folders: i64 = conn.query_row("SELECT COUNT(*) FROM nodes WHERE type = 'folder'", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let total_insights: i64 = conn.query_row("SELECT COUNT(*) FROM ghost_insights", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let graph_nodes: i64 = conn.query_row("SELECT COUNT(*) FROM ghost_graph_nodes", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let graph_edges: i64 = conn.query_row("SELECT COUNT(*) FROM ghost_graph_edges", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;

        // Top items
        let most_visited = ghost::attention::top_chats(&conn, 10)?;
        let top_tags = ghost::attention::top_tags(&conn, 10)?;
        let streak = ghost::attention::streak_days(&conn)?;

        // Heatmap
        let heatmap = ghost::attention::heatmap(&conn)?;
        let (peak_hour, peak_day) = ghost::attention::peak(&heatmap);

        let stats = GhostStats {
            total_events,
            total_chats,
            total_folders,
            total_insights,
            graph_nodes,
            graph_edges,
            most_visited,
            top_tags,
            streak_days: streak,
            peak_hour,
            peak_day,
        };

        // Graph
        let mut node_rows = conn.prepare(
            "SELECT id, kind, label, weight, color, position_x, position_y FROM ghost_graph_nodes"
        ).map_err(|e| e.to_string())?;
        let graph_nodes: Vec<crate::ghost::GraphNode> = node_rows.query_map([], |r| {
            Ok(crate::ghost::GraphNode {
                id: r.get(0)?,
                kind: r.get(1)?,
                label: r.get(2)?,
                weight: r.get(3)?,
                color: r.get(4)?,
                position_x: r.get(5)?,
                position_y: r.get(6)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        let mut edge_rows = conn.prepare(
            "SELECT id, source_id, target_id, weight, edge_type FROM ghost_graph_edges"
        ).map_err(|e| e.to_string())?;
        let graph_edges: Vec<crate::ghost::GraphEdge> = edge_rows.query_map([], |r| {
            Ok(crate::ghost::GraphEdge {
                id: r.get(0)?,
                source_id: r.get(1)?,
                target_id: r.get(2)?,
                weight: r.get(3)?,
                edge_type: r.get(4)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        let graph = Graph { nodes: graph_nodes, edges: graph_edges };

        // Insights
        let mut ins_rows = conn.prepare(
            "SELECT id, kind, title, body, source_ids, score, seen, created_at
             FROM ghost_insights WHERE seen = 0 ORDER BY score DESC, created_at DESC LIMIT 30"
        ).map_err(|e| e.to_string())?;
        let insights: Vec<GhostInsight> = ins_rows.query_map([], |r| {
            Ok(GhostInsight {
                id: r.get(0)?,
                kind: r.get(1)?,
                title: r.get(2)?,
                body: r.get(3)?,
                source_ids: r.get(4)?,
                score: r.get(5)?,
                seen: r.get(6)?,
                created_at: r.get(7)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        // Recent events
        let mut ev_rows = conn.prepare(
            "SELECT id, event_type, source_id, source_kind, duration_ms, metadata, created_at
             FROM ghost_events ORDER BY id DESC LIMIT 30"
        ).map_err(|e| e.to_string())?;
        let recent_events: Vec<GhostEvent> = ev_rows.query_map([], |r| {
            Ok(GhostEvent {
                id: r.get(0)?,
                event_type: r.get(1)?,
                source_id: r.get(2)?,
                source_kind: r.get(3)?,
                duration_ms: r.get(4)?,
                metadata: r.get(5)?,
                created_at: r.get(6)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        Ok(GhostSnapshot {
            stats,
            graph,
            insights,
            heatmap,
            recent_events,
        })
    })
    .await
}

#[tauri::command]
pub async fn ghost_mark_seen(
    state: State<'_, Arc<AppState>>,
    insight_id: i64,
) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("UPDATE ghost_insights SET seen = 1 WHERE id = ?1", [insight_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn ghost_purge(
    state: State<'_, Arc<AppState>>,
    days: i64,
) -> Cmd<usize> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|e| e.to_string())?;
        let n = ghost::tracker::prune_older_than(&conn, days)?;
        Ok(n)
    })
    .await
}
