use std::path::PathBuf;
use std::sync::Arc;

use crate::brief;
use crate::db::{self};
use crate::db::models::{ChatArtifact, ChatDetail, ChatMeta, ChatStats, NodeSummary};
use crate::error;
use crate::state::AppState;
use rusqlite::{params, OptionalExtension};
use tauri::State;

use super::{blocking, Cmd};
use super::folders::validate_move_target;
use super::roots::get_node;

// ---------------------------------------------------------------------------
// JSON import conversion (rule-based, no AI)
// ---------------------------------------------------------------------------

fn display_role(role_raw: &str) -> String {
    match role_raw.to_ascii_lowercase().as_str() {
        "user" | "human" | "me" => "User".to_string(),
        "assistant" | "ai" | "bot" => "Assistant".to_string(),
        "system" => "System".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Message".to_string(),
            }
        }
    }
}

/// Convert a recognized `[{role, content}, ...]` JSON export into readable
/// role-prefixed text. Returns `None` when the shape is not recognizable.
pub(crate) fn convert_json_import(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let items = value.as_array()?;
    if items.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut converted = 0usize;
    for item in items {
        let obj = item.as_object()?;
        let role_raw = obj.get("role").and_then(|r| r.as_str())?;
        let content = obj.get("content").and_then(|c| c.as_str())?;
        out.push_str(&display_role(role_raw));
        out.push_str(":\n");
        out.push_str(content.trim_end());
        out.push_str("\n\n");
        converted += 1;
    }
    if converted == 0 || out.trim().is_empty() {
        None
    } else {
        Some(out.trim_end().to_string())
    }
}

pub(crate) fn title_from_filename(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let stem = base
        .strip_suffix(".json")
        .or_else(|| base.strip_suffix(".md"))
        .or_else(|| base.strip_suffix(".txt"))
        .unwrap_or(base);
    let collapsed = stem.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "Imported Chat".to_string()
    } else {
        collapsed
    }
}

// ---------------------------------------------------------------------------
// Shared creation pipeline
// ---------------------------------------------------------------------------

struct CreateChatArgs {
    root_id: String,
    parent_id: Option<String>,
    title: String,
    text: String,
    source: String,
    tags: Option<String>,
}

fn build_meta_json(
    chat_id: &str,
    node_id: &str,
    root_id: &str,
    title: &str,
    source: &str,
    tags: Option<&String>,
    created_at: &str,
    updated_at: &str,
    stats: &ChatStats,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "chat_id": chat_id,
        "node_id": node_id,
        "root_id": root_id,
        "title": title,
        "source": source,
        "tags": tags.map(|t| t.as_str()),
        "created_at": created_at,
        "updated_at": updated_at,
        "stats": {
            "char_count": stats.char_count,
            "word_count": stats.word_count,
            "code_block_count": stats.code_block_count,
            "error_count": stats.error_count,
            "command_count": stats.command_count,
            "url_count": stats.url_count
        }
    })
}

fn create_chat_impl(app: &AppState, args: CreateChatArgs) -> Result<ChatDetail, String> {
    let title = db::valid_name(&args.title)?;
    let text = args.text;
    if text.trim().is_empty() {
        return Err(error::ERR_EMPTY_TEXT.to_string());
    }

    // Privacy gate (master spec §Privacy: EVERY capture passes policy
    // before SQLite/FTS/raw files). Pasted secrets are blocked or the
    // stored text is the redacted form — never the raw secret.
    let (text, privacy_note): (String, Option<String>) = {
        let policy = strawberry_core::privacy::PrivacyPolicy::default();
        let decision = policy.evaluate(&text);
        match decision.action {
            strawberry_core::privacy::PrivacyAction::Block => {
                return Err(format!(
                    "Blocked by privacy policy: {}",
                    decision.summary()
                ));
            }
            strawberry_core::privacy::PrivacyAction::Redact => {
                (policy.redact(&text), Some(decision.summary()))
            }
            strawberry_core::privacy::PrivacyAction::Allow => (text, None),
        }
    };

    // Validate tree targets before touching the filesystem.
    {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        db::root_exists(&conn, &args.root_id)?;
        db::parent_exists(&conn, &args.root_id, args.parent_id.as_deref())?;
    }

    let node_id = db::new_uuid();
    let chat_id = db::new_uuid();
    let now = db::now_iso();

    let mut generated = brief::generate(&title, &text);
    if let Some(note) = &privacy_note {
        // Explainability: the brief records that (and why) content was
        // redacted before storage — never the redacted material itself.
        generated.markdown = format!("> 🔒 {note}\n\n{}", generated.markdown);
    }
    let chat_stats = ChatStats::from(generated.stats);

    let dir: PathBuf =
        crate::storage::files::chat_dir(&app.files_root(), &args.root_id, &node_id);
    let raw_path = crate::storage::files::write_raw(&dir, &chat_id, &text)?;
    let brief_path = crate::storage::files::write_brief(&dir, &chat_id, &generated.markdown)?;
    let meta_value = build_meta_json(
        &chat_id,
        &node_id,
        &args.root_id,
        &title,
        &args.source,
        args.tags.as_ref(),
        &now,
        &now,
        &chat_stats,
    );
    crate::storage::files::write_meta_json(&dir, &chat_id, &meta_value.to_string())?;

    let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
    let position = db::next_position(&conn, args.parent_id.as_deref())?;
    {
        let tx = conn
            .unchecked_transaction()
            .map_err(error::to_string_err("failed to begin transaction"))?;
        tx.execute(
            "INSERT INTO nodes (id, root_id, parent_id, type, name, position, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'chat', ?4, ?5, ?6, ?6)",
            params![node_id, args.root_id, args.parent_id, title, position, now],
        )
        .map_err(error::to_string_err("failed to create chat node"))?;
        tx.execute(
            "INSERT INTO chats (
                id, node_id, title, source, raw_path, brief_path, first_idea, tags, brief_text,
                char_count, word_count, code_block_count, error_count, command_count, url_count,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
            params![
                chat_id,
                node_id,
                title,
                args.source,
                raw_path.display().to_string(),
                brief_path.display().to_string(),
                generated.first_idea,
                args.tags,
                generated.markdown,
                generated.stats.char_count,
                generated.stats.word_count,
                generated.stats.code_block_count,
                generated.stats.error_count,
                generated.stats.command_count,
                generated.stats.url_count,
                now
            ],
        )
        .map_err(error::to_string_err("failed to save chat"))?;
        db::insert_artifacts(&tx, &chat_id, &generated.artifacts, &now)?;
        tx.commit()
            .map_err(error::to_string_err("failed to save chat"))?;
    }

    let artifacts = generated
        .artifacts
        .iter()
        .map(|(kind, content)| ChatArtifact {
            id: db::new_uuid(),
            chat_id: chat_id.clone(),
            artifact_type: kind.clone(),
            content: content.clone(),
            created_at: now.clone(),
        })
        .collect();

    Ok(ChatDetail {
        meta: ChatMeta {
            chat_id,
            node_id,
            root_id: args.root_id,
            title,
            source: args.source,
            tags: args.tags,
            first_idea: generated.first_idea,
            raw_path: raw_path.display().to_string(),
            brief_path: Some(brief_path.display().to_string()),
            created_at: now.clone(),
            updated_at: now,
            stats: chat_stats,
        },
        brief_markdown: generated.markdown,
        artifacts,
    })
}

// ---------------------------------------------------------------------------
// Detail loading
// ---------------------------------------------------------------------------

pub(crate) fn load_detail(conn: &rusqlite::Connection, chat_id: &str) -> Result<ChatDetail, String> {
    let row = conn
        .query_row(
            "SELECT ch.id, ch.node_id, ch.source, ch.raw_path, ch.brief_path,
                    ch.first_idea, ch.tags, ch.brief_text,
                    coalesce(ch.char_count, 0), coalesce(ch.word_count, 0),
                    coalesce(ch.code_block_count, 0), coalesce(ch.error_count, 0),
                    coalesce(ch.command_count, 0), coalesce(coalesce(ch.url_count, 0), 0),
                    ch.created_at, ch.updated_at, n.name, n.root_id
             FROM chats ch JOIN nodes n ON n.id = ch.node_id
             WHERE ch.id = ?1",
            [chat_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    (
                        r.get::<_, i64>(8)?,
                        r.get::<_, i64>(9)?,
                        r.get::<_, i64>(10)?,
                        r.get::<_, i64>(11)?,
                        r.get::<_, i64>(12)?,
                        r.get::<_, i64>(13)?,
                    ),
                    r.get::<_, String>(14)?,
                    r.get::<_, String>(15)?,
                    r.get::<_, String>(16)?,
                    r.get::<_, String>(17)?,
                ))
            },
        )
        .optional()
        .map_err(error::to_string_err("database failure loading chat"))?
        .ok_or_else(|| error::ERR_MISSING_CHAT.to_string())?;

    let (
        id,
        node_id,
        source,
        raw_path,
        brief_path,
        first_idea,
        tags,
        brief_text,
        (char_count, word_count, code_block_count, error_count, command_count, url_count),
        created_at,
        updated_at,
        title,
        root_id,
    ) = row;

    let markdown = match brief_text {
        Some(text) if !text.trim().is_empty() => text,
        _ => brief_path
            .as_deref()
            .and_then(|p| crate::storage::files::read_text_file(std::path::Path::new(p)).ok())
            .unwrap_or_default(),
    };

    let meta = ChatMeta {
        chat_id: id.clone(),
        node_id,
        root_id,
        title,
        source,
        tags,
        first_idea,
        raw_path,
        brief_path,
        created_at,
        updated_at,
        stats: brief::Stats {
            char_count,
            word_count,
            code_block_count,
            error_count,
            command_count,
            url_count,
        }
        .into(),
    };
    let artifacts = db::artifacts_for_chat(conn, &id)?;
    Ok(ChatDetail {
        meta,
        brief_markdown: markdown,
        artifacts,
    })
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_chat_from_text(
    state: State<'_, Arc<AppState>>,
    root_id: String,
    parent_id: Option<String>,
    title: String,
    text: String,
    tags: Option<String>,
) -> Cmd<ChatDetail> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        create_chat_impl(
            app,
            CreateChatArgs {
                root_id,
                parent_id,
                title,
                text,
                source: "manual".to_string(),
                tags,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn import_chat_file_text(
    state: State<'_, Arc<AppState>>,
    root_id: String,
    parent_id: Option<String>,
    filename: String,
    text: String,
    tags: Option<String>,
) -> Cmd<ChatDetail> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        if text.trim().is_empty() {
            return Err(error::ERR_EMPTY_TEXT.to_string());
        }
        let lower = filename.to_ascii_lowercase();
        let is_json = lower.ends_with(".json");
        let (body, source) = if is_json {
            match convert_json_import(&text) {
                Some(converted) => (converted, "import_json".to_string()),
                None => (text, "custom_import".to_string()),
            }
        } else {
            (text, "import".to_string())
        };
        let title = title_from_filename(&filename);
        create_chat_impl(
            app,
            CreateChatArgs {
                root_id,
                parent_id,
                title,
                text: body,
                source,
                tags,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn get_chat(state: State<'_, Arc<AppState>>, chat_id: String) -> Cmd<ChatDetail> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        load_detail(&conn, &chat_id)
    })
    .await
}

/// Load the original raw text for the Original tab, on demand and separately
/// from `get_chat` so large bodies never load unless requested.
#[tauri::command]
pub async fn get_chat_raw(state: State<'_, Arc<AppState>>, chat_id: String) -> Cmd<String> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let raw_path = {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            conn.query_row(
                "SELECT raw_path FROM chats WHERE id = ?1",
                [&chat_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(error::to_string_err("database failure loading chat"))?
            .ok_or_else(|| error::ERR_MISSING_CHAT.to_string())?
        };
        crate::storage::files::read_text_file(std::path::Path::new(&raw_path))
    })
    .await
}

#[tauri::command]
pub async fn delete_chat(state: State<'_, Arc<AppState>>, chat_id: String) -> Cmd<()> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let (root_id, node_id) = {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            let row: Option<(String, String)> = conn
                .query_row(
                    "SELECT n.root_id, n.id FROM chats ch JOIN nodes n ON n.id = ch.node_id
                     WHERE ch.id = ?1",
                    [&chat_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(error::to_string_err("database failure loading chat"))?;
            row.ok_or_else(|| error::ERR_MISSING_CHAT.to_string())?
        };
        {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            conn.execute("DELETE FROM nodes WHERE id = ?1", [&node_id])
                .map_err(error::to_string_err("failed to delete chat"))?;
        }
        crate::db::remove_chat_files_quiet(&app.files_root(), &root_id, &node_id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn move_chat(
    state: State<'_, Arc<AppState>>,
    chat_id: String,
    new_parent_id: Option<String>,
) -> Cmd<NodeSummary> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        let node_row: Option<(String, String)> = conn
            .query_row(
                "SELECT n.id, n.root_id FROM chats ch JOIN nodes n ON n.id = ch.node_id
                 WHERE ch.id = ?1",
                [&chat_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(error::to_string_err("database failure loading chat"))?;
        let (node_id, root_id) = node_row.ok_or_else(|| error::ERR_MISSING_CHAT.to_string())?;
        validate_move_target(&conn, &node_id, &root_id, new_parent_id.as_deref())?;
        let position = db::next_position(&conn, new_parent_id.as_deref())?;
        conn.execute(
            "UPDATE nodes SET parent_id = ?1, position = ?2, updated_at = ?3 WHERE id = ?4",
            params![new_parent_id, position, db::now_iso(), node_id],
        )
        .map_err(error::to_string_err("failed to move chat"))?;
        get_node(&conn, &node_id)
    })
    .await
}

#[tauri::command]
pub async fn update_chat_metadata(
    state: State<'_, Arc<AppState>>,
    chat_id: String,
    title: Option<String>,
    tags: Option<String>,
) -> Cmd<ChatDetail> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let now = db::now_iso();
        let new_title = match title {
            Some(t) => Some(db::valid_name(&t)?),
            None => None,
        };
        {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT n.id, n.root_id FROM chats ch JOIN nodes n ON n.id = ch.node_id
                     WHERE ch.id = ?1",
                    [&chat_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(error::to_string_err("database failure loading chat"))?;
            let (_node_id, _root_id) =
                existing.ok_or_else(|| error::ERR_MISSING_CHAT.to_string())?;

            let tx = conn
                .unchecked_transaction()
                .map_err(error::to_string_err("failed to begin transaction"))?;

            if let Some(t) = &new_title {
                // Duplicate check among siblings of this chat's node.
                let node: NodeSummary = get_node(&tx, &_node_id)?;
                db::ensure_unique_name(
                    &tx,
                    db::NameScope::Node {
                        root_id: &node.root_id.clone(),
                        parent_id: node.parent_id.as_deref(),
                    },
                    t,
                    Some(&_node_id),
                )?;
                tx.execute(
                    "UPDATE nodes SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![t, now, _node_id],
                )
                .map_err(error::to_string_err("failed to rename chat"))?;
            }

            if tags.is_some() || new_title.is_some() {
                tx.execute(
                    "UPDATE chats SET
                        title = coalesce(?1, title),
                        tags  = coalesce(?2, tags),
                        updated_at = ?3
                     WHERE id = ?4",
                    params![new_title, tags, now, chat_id],
                )
                .map_err(error::to_string_err("failed to update chat metadata"))?;
            } else {
                tx.execute(
                    "UPDATE chats SET updated_at = ?1 WHERE id = ?2",
                    params![now, chat_id],
                )
                .map_err(error::to_string_err("failed to touch chat"))?;
            }
            tx.commit()
                .map_err(error::to_string_err("failed to update chat metadata"))?;
        }

        // Refresh sidecar metadata file (best effort).
        {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            let detail = load_detail(&conn, &chat_id)?;
            let dir = crate::storage::files::chat_dir(
                &app.files_root(),
                &detail.meta.root_id,
                &detail.meta.node_id,
            );
            let meta_value = build_meta_json(
                &detail.meta.chat_id,
                &detail.meta.node_id,
                &detail.meta.root_id,
                &detail.meta.title,
                &detail.meta.source,
                detail.meta.tags.as_ref(),
                &detail.meta.created_at,
                &now,
                &detail.meta.stats,
            );
            let _ = crate::storage::files::write_meta_json(
                &dir,
                &detail.meta.chat_id,
                &meta_value.to_string(),
            );
        }

        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        load_detail(&conn, &chat_id)
    })
    .await
}

#[tauri::command]
pub async fn regenerate_brief(state: State<'_, Arc<AppState>>, chat_id: String) -> Cmd<ChatDetail> {
    let st = state.inner().clone();
    blocking(st, move |app| {
        let (raw_path, node_id, root_id) = {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            let row: Option<(String, String, String)> = conn
                .query_row(
                    "SELECT ch.raw_path, n.id, n.root_id
                     FROM chats ch JOIN nodes n ON n.id = ch.node_id
                     WHERE ch.id = ?1",
                    [&chat_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(error::to_string_err("database failure loading chat"))?;
            row.ok_or_else(|| error::ERR_MISSING_CHAT.to_string())?
        };
        let raw_text = crate::storage::files::read_text_file(std::path::Path::new(&raw_path))?;
        let title = {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            conn.query_row(
                "SELECT n.name FROM chats ch JOIN nodes n ON n.id = ch.node_id WHERE ch.id = ?1",
                [&chat_id],
                |r| r.get::<_, String>(0),
            )
            .map_err(error::to_string_err("failed to load chat title"))?
        };

        let now = db::now_iso();
        let generated = brief::generate(&title, &raw_text);
        let dir = crate::storage::files::chat_dir(&app.files_root(), &root_id, &node_id);
        let brief_path = crate::storage::files::write_brief(&dir, &chat_id, &generated.markdown)?;

        {
            let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
            let tx = conn
                .unchecked_transaction()
                .map_err(error::to_string_err("failed to begin transaction"))?;
            tx.execute(
                "UPDATE chats SET brief_path = ?1, brief_text = ?2, first_idea = ?3,
                        char_count = ?4, word_count = ?5, code_block_count = ?6,
                        error_count = ?7, command_count = ?8, url_count = ?9, updated_at = ?10
                 WHERE id = ?11",
                params![
                    brief_path.display().to_string(),
                    generated.markdown,
                    generated.first_idea,
                    generated.stats.char_count,
                    generated.stats.word_count,
                    generated.stats.code_block_count,
                    generated.stats.error_count,
                    generated.stats.command_count,
                    generated.stats.url_count,
                    now,
                    chat_id
                ],
            )
            .map_err(error::to_string_err("failed to update brief"))?;
            db::delete_artifacts(&tx, &chat_id)?;
            db::insert_artifacts(&tx, &chat_id, &generated.artifacts, &now)?;
            tx.commit()
                .map_err(error::to_string_err("failed to regenerate brief"))?;

            // Refresh sidecar metadata file.
            let detail_preview = load_detail(&conn, &chat_id)?;
            let meta_value = build_meta_json(
                &detail_preview.meta.chat_id,
                &detail_preview.meta.node_id,
                &detail_preview.meta.root_id,
                &detail_preview.meta.title,
                &detail_preview.meta.source,
                detail_preview.meta.tags.as_ref(),
                &detail_preview.meta.created_at,
                &now,
                &detail_preview.meta.stats,
            );
            let _ = crate::storage::files::write_meta_json(
                &dir,
                &chat_id,
                &meta_value.to_string(),
            );
        }

        let conn = app.conn.lock().map_err(|_| error::ERR_DB_LOCK.to_string())?;
        load_detail(&conn, &chat_id)
    })
    .await
}
