#![allow(dead_code)]

use rusqlite::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Root {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            color: row.get("color")?,
            icon: row.get("icon")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSummary {
    pub id: String,
    pub root_id: String,
    pub parent_id: Option<String>,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Populated only for chat nodes (used by the UI to open/delete).
    #[serde(default)]
    pub chat_id: Option<String>,
}

impl NodeSummary {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            root_id: row.get("root_id")?,
            parent_id: row.get("parent_id")?,
            node_type: row.get("type")?,
            name: row.get("name")?,
            position: row.get("position")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            chat_id: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub id: String,
    pub root_id: String,
    pub parent_id: Option<String>,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
    pub children: Vec<TreeNode>,
}

impl From<NodeSummary> for TreeNode {
    fn from(n: NodeSummary) -> Self {
        Self {
            id: n.id,
            root_id: n.root_id,
            parent_id: n.parent_id,
            node_type: n.node_type,
            name: n.name,
            position: n.position,
            created_at: n.created_at,
            updated_at: n.updated_at,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbItem {
    pub id: String,
    pub label: String,
    /// "root" | "folder"
    pub kind: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStats {
    pub char_count: i64,
    pub word_count: i64,
    pub code_block_count: i64,
    pub error_count: i64,
    pub command_count: i64,
    pub url_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMeta {
    pub chat_id: String,
    pub node_id: String,
    pub root_id: String,
    pub title: String,
    pub source: String,
    pub tags: Option<String>,
    pub first_idea: Option<String>,
    pub raw_path: String,
    pub brief_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub stats: ChatStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatArtifact {
    pub id: String,
    pub chat_id: String,
    pub artifact_type: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDetail {
    pub meta: ChatMeta,
    pub brief_markdown: String,
    pub artifacts: Vec<ChatArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultItem {
    pub chat_id: String,
    pub node_id: String,
    pub title: String,
    pub root_name: String,
    pub folder_path: String,
    pub snippet: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub app_name: String,
    pub version: String,
    pub data_dir: String,
    pub db_path: String,
    pub files_dir: String,
    pub fts_enabled: bool,
    pub sqlite_version: String,
}
