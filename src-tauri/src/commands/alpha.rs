//! 🎯 Alpha Hunter commands — scan, list, verify, dismiss, config.
//! All network access is gated behind `app_meta.alpha_hunter_enabled = '1'`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::alpha;
use crate::state::AppState;

use super::{blocking, conn_of, Cmd};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlphaCandidate {
    pub id: String,
    pub source: String,
    pub title: String,
    pub url: Option<String>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub base_url: Option<String>,
    pub status: String,
    pub score: i64,
    pub detected_at: String,
    pub verified_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned_sources: u32,
    pub items_checked: u64,
    pub new_candidates: u64,
    pub enabled: bool,
}

fn alpha_enabled(conn: &rusqlite::Connection) -> bool {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key='alpha_hunter_enabled'",
            [],
            |r| r.get(0),
        )
        .ok();
    v.as_deref() == Some("1")
}

/// Read the opt-in flag.
#[tauri::command]
pub async fn get_alpha_enabled(state: tauri::State<'_, Arc<AppState>>) -> Cmd<bool> {
    let st = state.inner().clone();
    blocking(st, |app| {
        let conn = conn_of(app)?;
        Ok(alpha_enabled(&conn))
    })
    .await
}

/// Toggle the opt-in flag (network access for Alpha Hunter).
#[tauri::command]
pub async fn set_alpha_enabled(
    state: tauri::State<'_, Arc<AppState>>,
    enabled: bool,
) -> Cmd<bool> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute(
            "INSERT INTO app_meta(key, value) VALUES ('alpha_hunter_enabled', ?1)
             ON CONFLICT(key) DO UPDATE SET value=?1",
            [if enabled { "1" } else { "0" }],
        )
        .map_err(crate::error::to_string_err("failed to set alpha_hunter_enabled"))?;
        Ok(enabled)
    })
    .await
}

fn row_to_candidate(r: &rusqlite::Row<'_>) -> rusqlite::Result<AlphaCandidate> {
    Ok(AlphaCandidate {
        id: r.get(0)?,
        source: r.get(1)?,
        title: r.get(2)?,
        url: r.get(3)?,
        provider: r.get(4)?,
        model_id: r.get(5)?,
        base_url: r.get(6)?,
        status: r.get(7)?,
        score: r.get(8)?,
        detected_at: r.get(9)?,
        verified_at: r.get(10)?,
        notes: r.get(11)?,
    })
}

const SELECT_COLS: &str = "id, source, title, url, provider, model_id, base_url, status, score, detected_at, verified_at, notes";

/// Scan all sources, detect candidates, insert new ones (dedup by title+source).
#[tauri::command]
pub async fn scan_alpha(state: tauri::State<'_, Arc<AppState>>) -> Cmd<ScanReport> {
    let st = state.inner().clone();
    blocking(st, |app| {
        let conn = conn_of(app)?;
        if !alpha_enabled(&conn) {
            return Ok(ScanReport {
                scanned_sources: 0,
                items_checked: 0,
                new_candidates: 0,
                enabled: false,
            });
        }
        drop(conn);

        // Network fetches happen outside the DB lock.
        let mut items: Vec<alpha::RawItem> = Vec::new();
        items.extend(alpha::fetch_hn());
        items.extend(alpha::fetch_reddit());
        items.extend(alpha::fetch_openrouter());
        items.extend(alpha::fetch_github());
        items.extend(alpha::fetch_huggingface());
        items.extend(alpha::fetch_producthunt());
        let items_checked = items.len() as u64;

        let conn = conn_of(app)?;
        let now = crate::db::now_iso();
        let mut inserted = 0u64;
        for item in &items {
            if let Some(c) = alpha::detect(item) {
                // Dedup: same source + title already present → skip.
                let exists: bool = conn
                    .query_row(
                        "SELECT 1 FROM alpha_candidates WHERE source=?1 AND title=?2",
                        rusqlite::params![c.source, c.title],
                        |_| Ok(true),
                    )
                    .unwrap_or(false);
                if exists {
                    continue;
                }
                let id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO alpha_candidates
                        (id, source, title, url, provider, model_id, base_url, status, score, detected_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'new', ?8, ?9)",
                    rusqlite::params![
                        id,
                        c.source,
                        c.title,
                        c.url,
                        c.provider,
                        c.model_id,
                        c.base_url,
                        c.score,
                        now,
                    ],
                )
                .map_err(crate::error::to_string_err("alpha insert failed"))?;
                inserted += 1;
            }
        }
        Ok(ScanReport {
            scanned_sources: 6,
            items_checked,
            new_candidates: inserted,
            enabled: true,
        })
    })
    .await
}

/// List candidates, best score first, newest first within a score.
#[tauri::command]
pub async fn list_alpha_candidates(
    state: tauri::State<'_, Arc<AppState>>,
) -> Cmd<Vec<AlphaCandidate>> {
    let st = state.inner().clone();
    blocking(st, |app| {
        let conn = conn_of(app)?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM alpha_candidates
                 WHERE status != 'dismissed'
                 ORDER BY CASE status WHEN 'new' THEN 0 WHEN 'verified' THEN 1 ELSE 2 END,
                          score DESC, detected_at DESC
                 LIMIT 100"
            ))
            .map_err(crate::error::to_string_err("alpha list prepare failed"))?;
        let rows = stmt
            .query_map([], row_to_candidate)
            .map_err(crate::error::to_string_err("alpha list query failed"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(crate::error::to_string_err("alpha list read failed"))
    })
    .await
}

/// Verify one candidate with a live OpenAI-compatible call.
#[tauri::command]
pub async fn verify_alpha_candidate(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    api_key: String,
) -> Cmd<AlphaCandidate> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        // Read candidate first.
        let (base_url, model_id) = {
            let conn = conn_of(app)?;
            let row: (Option<String>, Option<String>) = conn
                .query_row(
                    "SELECT base_url, model_id FROM alpha_candidates WHERE id=?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|_| "Candidate not found.".to_string())?;
            (row.0, row.1)
        };
        let base_url = base_url.ok_or_else(|| {
            "No base URL detected for this candidate — cannot verify automatically.".to_string()
        })?;
        let model_id = model_id.ok_or_else(|| {
            "No model ID detected for this candidate — cannot verify automatically.".to_string()
        })?;

        // Live call outside the DB lock.
        let result = alpha::verify(&base_url, &model_id, &api_key);

        let conn = conn_of(app)?;
        let now = crate::db::now_iso();
        let status = if result.ok { "verified" } else { "failed" };
        conn.execute(
            "UPDATE alpha_candidates SET status=?2, verified_at=?3, notes=?4 WHERE id=?1",
            rusqlite::params![id, status, now, result.detail],
        )
        .map_err(crate::error::to_string_err("alpha verify update failed"))?;

        conn.query_row(
            &format!("SELECT {SELECT_COLS} FROM alpha_candidates WHERE id=?1"),
            [&id],
            row_to_candidate,
        )
        .map_err(crate::error::to_string_err("alpha verify read failed"))
    })
    .await
}

/// Dismiss a candidate (soft delete).
#[tauri::command]
pub async fn dismiss_alpha_candidate(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = conn_of(app)?;
        conn.execute(
            "UPDATE alpha_candidates SET status='dismissed' WHERE id=?1",
            [&id],
        )
        .map_err(crate::error::to_string_err("alpha dismiss failed"))?;
        Ok(())
    })
    .await
}

/// Build a copy-paste config snippet for a candidate.
#[tauri::command]
pub async fn get_alpha_config(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Cmd<String> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = conn_of(app)?;
        let (provider, model_id, base_url): (Option<String>, Option<String>, Option<String>) =
            conn.query_row(
                "SELECT provider, model_id, base_url FROM alpha_candidates WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|_| "Candidate not found.".to_string())?;
        Ok(alpha::build_config_snippet(
            provider.as_deref(),
            model_id.as_deref(),
            base_url.as_deref(),
        ))
    })
    .await
}
