use serde::{Deserialize, Serialize};
use strawberry_core::{analyze_source, SymbolicAnalysis};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbientEvent {
    pub id: String,
    pub event_type: String,
    pub title: String,
    pub summary: String,
    pub source_app: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbientStats {
    pub total_events: i64,
    pub clip_events: i64,
    pub screen_events: i64,
    pub ast_events: i64,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterministicReport {
    pub timestamp: String,
    pub platform: String,
    pub total_events_analyzed: usize,
    pub active_languages: Vec<String>,
    pub extracted_symbols: usize,
    pub summary_markdown: String,
}

#[tauri::command]
pub fn record_ambient_event(
    state: State<'_, std::sync::Arc<AppState>>,
    event_type: String,
    title: String,
    summary: String,
    source_app: Option<String>,
    metadata: Option<String>,
) -> Result<AmbientEvent, String> {
    let conn = state.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;

    let id = format!("amb_{}", uuid::Uuid::new_v4().simple());
    let now = crate::db::now_iso();

    conn.execute(
        "INSERT INTO ambient_events(id, event_type, title, summary, source_app, metadata, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, event_type, title, summary, source_app, metadata, now],
    )
    .map_err(crate::error::to_string_err("failed to insert ambient event"))?;

    Ok(AmbientEvent {
        id,
        event_type,
        title,
        summary,
        source_app,
        metadata,
        created_at: now,
    })
}

#[tauri::command]
pub fn get_ambient_events(
    state: State<'_, std::sync::Arc<AppState>>,
    limit: Option<usize>,
) -> Result<Vec<AmbientEvent>, String> {
    let conn = state.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;
    let limit_num = limit.unwrap_or(50) as i64;

    let mut stmt = conn
        .prepare(
            "SELECT id, event_type, title, summary, source_app, metadata, created_at
             FROM ambient_events
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .map_err(crate::error::to_string_err("failed to prepare query"))?;

    let rows = stmt
        .query_map([limit_num], |row| {
            Ok(AmbientEvent {
                id: row.get(0)?,
                event_type: row.get(1)?,
                title: row.get(2)?,
                summary: row.get(3)?,
                source_app: row.get(4)?,
                metadata: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(crate::error::to_string_err("failed to execute query"))?;

    let mut events = Vec::new();
    for r in rows {
        events.push(r.map_err(crate::error::to_string_err("failed to read ambient event"))?);
    }
    Ok(events)
}

#[tauri::command]
pub fn analyze_code_ast(
    lang_or_ext: String,
    source: String,
) -> Result<serde_json::Value, String> {
    let a = analyze_source(&lang_or_ext, &source);
    let kind_str = |k: &strawberry_core::SymbolKind| -> &'static str {
        use strawberry_core::SymbolKind::*;
        match k {
            Function => "function",
            ClassOrStruct => "class_or_struct",
            InterfaceOrTrait => "interface_or_trait",
            Import => "import",
            ErrorOrThrow => "error_or_throw",
        }
    };
    let sym_to_json = |s: &strawberry_core::SymbolItem| serde_json::json!({
        "kind": kind_str(&s.kind),
        "name": s.name,
        "signature": s.signature,
        "line": s.line,
    });
    Ok(serde_json::json!({
        "language": a.language,
        "total_lines": a.total_lines,
        "imports": a.imports,
        "functions": a.functions.iter().map(sym_to_json).collect::<Vec<_>>(),
        "types_or_classes": a.types_or_classes.iter().map(sym_to_json).collect::<Vec<_>>(),
        "error_points": a.error_points.iter().map(sym_to_json).collect::<Vec<_>>(),
    }))
}

#[tauri::command]
pub fn get_ambient_stats(state: State<'_, std::sync::Arc<AppState>>) -> Result<AmbientStats, String> {
    let conn = state.conn.lock().map_err(|_| crate::error::ERR_DB_LOCK.to_string())?;

    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM ambient_events", [], |r| r.get(0))
        .unwrap_or(0);
    let clip: i64 = conn
        .query_row("SELECT COUNT(*) FROM ambient_events WHERE event_type='clip'", [], |r| r.get(0))
        .unwrap_or(0);
    let screen: i64 = conn
        .query_row("SELECT COUNT(*) FROM ambient_events WHERE event_type='screen'", [], |r| r.get(0))
        .unwrap_or(0);
    let ast: i64 = conn
        .query_row("SELECT COUNT(*) FROM ambient_events WHERE event_type='symbolic_ast'", [], |r| r.get(0))
        .unwrap_or(0);

    let platform = std::env::consts::OS.to_string();

    Ok(AmbientStats {
        total_events: total,
        clip_events: clip,
        screen_events: screen,
        ast_events: ast,
        platform,
    })
}

#[tauri::command]
pub fn generate_deterministic_report(
    state: State<'_, std::sync::Arc<AppState>>,
) -> Result<DeterministicReport, String> {
    let events = get_ambient_events(state, Some(100))?;
    let platform = std::env::consts::OS.to_string();
    let now = crate::db::now_iso();

    let mut languages = std::collections::HashSet::new();
    let mut symbol_count = 0usize;
    let mut ast_events_count = 0usize;
    let mut error_events_count = 0usize;

    for ev in &events {
        if ev.event_type == "symbolic_ast" {
            ast_events_count += 1;
            if let Some(ref meta) = ev.metadata {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(meta) {
                    if let Some(lang) = val.get("language").and_then(|v| v.as_str()) {
                        languages.insert(lang.to_string());
                    }
                    if let Some(fns) = val.get("functions").and_then(|v| v.as_array()) {
                        symbol_count += fns.len();
                    }
                    if let Some(types) = val.get("typesOrClasses").and_then(|v| v.as_array()) {
                        symbol_count += types.len();
                    }
                }
            }
        }
        if ev.summary.contains("error") || ev.summary.contains("panic") {
            error_events_count += 1;
        }
    }

    let langs_vec: Vec<String> = languages.into_iter().collect();

    let mut markdown = format!(
        "# 🧠 Ambient Memory & Symbolic Synthesis Report\n\n\
         - **Generated:** `{}`\n\
         - **Platform:** `{}` (OS Independent Engine)\n\
         - **Total Ambient Events Analyzed:** {}\n\
         - **Symbolic AST Extractions:** {}\n\
         - **Active Languages Detected:** {}\n\
         - **Total Structural Symbols Extracted:** {}\n\
         - **Errors / Panics Logged:** {}\n\n\
         ## ⚡ Deterministic Synthesis Recommendations\n\n",
        now,
        platform,
        events.len(),
        ast_events_count,
        if langs_vec.is_empty() { "None".to_string() } else { langs_vec.join(", ") },
        symbol_count,
        error_events_count
    );

    if error_events_count > 0 {
        markdown.push_str("- ⚠️ **Error Pattern Detected:** Recent captured events contain error traces. Inspect the AST error points below.\n");
    } else {
        markdown.push_str("- ✅ **Clean Execution State:** No unhandled panic or syntax error patterns recorded in recent events.\n");
    }

    if !langs_vec.is_empty() {
        markdown.push_str(&format!(
            "- 🌳 **Multi-Language Graph:** Multi-language symbolic tree actively tracking `{}` structures.\n",
            langs_vec.join(", ")
        ));
    }

    markdown.push_str("\n## 📋 Recent Ambient Timeline Events\n\n");
    if events.is_empty() {
        markdown.push_str("_No ambient memory events recorded yet. Perform actions or run AST analysis to populate memory._\n");
    } else {
        for (idx, ev) in events.iter().take(10).enumerate() {
            markdown.push_str(&format!(
                "{}. **[{}]** `{}` — {}\n",
                idx + 1,
                ev.event_type.to_uppercase(),
                ev.title,
                ev.summary
            ));
        }
    }

    Ok(DeterministicReport {
        timestamp: now,
        platform,
        total_events_analyzed: events.len(),
        active_languages: langs_vec,
        extracted_symbols: symbol_count,
        summary_markdown: markdown,
    })
}
