//! Feature 1 — export a chat as an AI-to-AI handoff packet.
//!
//! Reads the original chat from disk, runs the deterministic engine and
//! returns both the paste-ready block and the `.strawberry.json` form plus an
//! auditable budget report. No AI, no network.

use std::sync::Arc;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error;
use crate::state::AppState;
use strawberry_core::handoff;

use super::{blocking, Cmd};

/// Guard rails around the caller-supplied budget. Below ~120 tokens even the
/// goal line plus the source pointer will not fit, and above 8k a "compressed"
/// packet is no longer compressed.
const MIN_BUDGET: usize = 120;
const MAX_BUDGET: usize = 8_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffExport {
    /// Paste-ready block for another AI's chat box.
    pub rendered: String,
    /// `.strawberry.json` interchange form.
    pub json: String,
    /// Structured packet so the UI can show per-slot counts.
    pub packet: handoff::HandoffPacket,
}

fn clamp_budget(requested: Option<usize>) -> usize {
    requested
        .unwrap_or(handoff::DEFAULT_TOKEN_BUDGET)
        .clamp(MIN_BUDGET, MAX_BUDGET)
}

/// Build a handoff packet for a saved chat.
#[tauri::command]
pub async fn export_handoff(
    state: State<'_, Arc<AppState>>,
    chat_id: String,
    token_budget: Option<usize>,
) -> Cmd<HandoffExport> {
    let st = state.inner().clone();
    let budget = clamp_budget(token_budget);
    blocking(st, move |app| {
        let (raw_path, title) = {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            conn.query_row(
                "SELECT ch.raw_path, n.name
                 FROM chats ch JOIN nodes n ON n.id = ch.node_id
                 WHERE ch.id = ?1",
                [&chat_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(error::to_string_err("database failure loading chat"))?
            .ok_or_else(|| error::ERR_MISSING_CHAT.to_string())?
        };

        // Compress the ORIGINAL text, never the brief: the brief has already
        // dropped detail, and compressing a compression loses identifiers.
        let raw = crate::storage::files::read_text_file(std::path::Path::new(&raw_path))?;
        let packet = handoff::build_from_raw(&title, Some(chat_id.clone()), &raw, budget);
        Ok(HandoffExport {
            rendered: handoff::render(&packet),
            json: handoff::to_json(&packet),
            packet,
        })
    })
    .await
}

/// Build a handoff packet from pasted text that was never saved.
///
/// Lets a user compress a chat without first importing it.
#[tauri::command]
pub async fn handoff_from_text(
    title: String,
    text: String,
    token_budget: Option<usize>,
) -> Cmd<HandoffExport> {
    if text.trim().is_empty() {
        return Err(error::ERR_EMPTY_TEXT.to_string());
    }
    let budget = clamp_budget(token_budget);
    let display_title = if title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        title.trim().to_string()
    };
    let packet = handoff::build_from_raw(&display_title, None, &text, budget);
    Ok(HandoffExport {
        rendered: handoff::render(&packet),
        json: handoff::to_json(&packet),
        packet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_is_clamped_to_a_usable_range() {
        assert_eq!(clamp_budget(None), handoff::DEFAULT_TOKEN_BUDGET);
        assert_eq!(clamp_budget(Some(1)), MIN_BUDGET);
        assert_eq!(clamp_budget(Some(999_999)), MAX_BUDGET);
        assert_eq!(clamp_budget(Some(500)), 500);
    }
}
