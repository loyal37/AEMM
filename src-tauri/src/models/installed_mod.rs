use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub id: Uuid,
    pub logical_id: String,
    pub repository_path: PathBuf,
    pub content_fingerprint: Option<String>,
    pub size_bytes: u64,
    pub installed_at: i64,
    pub updated_at: i64,
    pub lifecycle_state: ModLifecycleState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModLifecycleState {
    Installing,
    Installed,
    Broken,
    Removing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MetadataSourceKind {
    ModJson,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorModMetadata {
    pub logical_id: String,
    pub name: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub game_version: Option<String>,
    pub website: Option<String>,
    pub preview_path: Option<PathBuf>,
    pub original_document: Option<serde_json::Value>,
    pub source_kind: MetadataSourceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalModMetadata {
    pub display_name_override: Option<String>,
    pub category_override: Option<String>,
    pub description_override: Option<String>,
    pub favorite: bool,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    pub source_path: PathBuf,
    pub deployment_target: Option<PathBuf>,
    pub size_bytes: u64,
    pub content_hash: Option<String>,
    pub file_role: String,
    pub modified_at: i64,
}
