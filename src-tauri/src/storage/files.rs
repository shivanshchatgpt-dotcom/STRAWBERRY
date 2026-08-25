use std::fs;
use std::path::{Path, PathBuf};

use crate::error;

pub fn files_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("files")
}

pub fn chat_dir(files_root: &Path, root_id: &str, node_id: &str) -> PathBuf {
    files_root.join(root_id).join(node_id)
}

pub fn ensure_dirs(data_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(data_dir)
        .map_err(error::to_string_err("failed to create app data directory"))?;
    let files_root = files_dir(data_dir);
    fs::create_dir_all(&files_root)
        .map_err(error::to_string_err("failed to create files directory"))?;
    Ok(files_root)
}

/// Write the original raw chat file. Called exactly once per chat creation —
/// brief generation must never touch this path.
pub fn write_raw(dir: &Path, chat_id: &str, text: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(error::to_string_err("failed to create chat folder"))?;
    let path = dir.join(format!("{chat_id}.raw.txt"));
    fs::write(&path, text).map_err(error::to_string_err("failed to write original chat file"))?;
    Ok(path)
}

/// Write (or overwrite) the generated brief markdown. Never touches the raw file.
pub fn write_brief(dir: &Path, chat_id: &str, markdown: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(error::to_string_err("failed to create chat folder"))?;
    let path = dir.join(format!("{chat_id}.brief.md"));
    fs::write(&path, markdown).map_err(error::to_string_err("failed to write brief file"))?;
    Ok(path)
}

/// Write the metadata JSON sidecar.
pub fn write_meta_json(dir: &Path, chat_id: &str, meta_json: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(error::to_string_err("failed to create chat folder"))?;
    let path = dir.join(format!("{chat_id}.meta.json"));
    fs::write(&path, meta_json)
        .map_err(error::to_string_err("failed to write metadata file"))?;
    Ok(path)
}

pub fn read_text_file(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }
    fs::read_to_string(path).map_err(error::to_string_err("failed to read file"))
}

/// Remove a whole tree (used when deleting roots).
pub fn remove_dir_tree(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(error::to_string_err("failed to delete stored files"))?;
    }
    Ok(())
}

/// Best-effort removal of empty directories from `start` upward, stopping at
/// the files root (which is never deleted itself).
pub fn prune_empty_dirs(start: PathBuf) {
    let mut cur: Option<PathBuf> = start.parent().map(|p| p.to_path_buf());
    while let Some(dir) = cur {
        // Attempt removal; ignore errors (non-empty or protected).
        if fs::remove_dir(&dir).is_ok() {
            cur = dir.parent().map(|p| p.to_path_buf());
        } else {
            break;
        }
    }
}
