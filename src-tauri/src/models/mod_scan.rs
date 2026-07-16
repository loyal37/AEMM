use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{LocalModMetadata, MetadataSourceKind, ModLifecycleState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModListItem {
    pub id: Uuid,
    pub logical_id: String,
    pub repository_path: PathBuf,
    pub name: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub preview_path: Option<PathBuf>,
    pub favorite: bool,
    pub size_bytes: u64,
    pub file_count: u64,
    pub installed_at: i64,
    pub updated_at: i64,
    pub lifecycle_state: ModLifecycleState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModScanResult {
    pub discovered: u64,
    pub added: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub broken: u64,
    pub missing: u64,
    pub hashed_files: u64,
    pub reused_hashes: u64,
    pub skipped_entries: u64,
    pub duration_ms: u64,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateLocalModMetadata {
    pub mod_id: Uuid,
    pub metadata: LocalModMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetModFavorite {
    pub mod_ids: Vec<Uuid>,
    pub favorite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModMutationResult {
    pub updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFileDetails {
    pub source_path: PathBuf,
    pub size_bytes: u64,
    pub content_hash: Option<String>,
    pub file_role: String,
    pub modified_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetails {
    pub item: ModListItem,
    pub author_name: String,
    pub author_description: Option<String>,
    pub author_category: Option<String>,
    pub game_version: Option<String>,
    pub website: Option<String>,
    pub metadata_source: MetadataSourceKind,
    pub local_metadata: LocalModMetadata,
    pub files: Vec<ModFileDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPreview {
    pub data_url: String,
}
