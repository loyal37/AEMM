use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{LocalModMetadata, ModLifecycleState};

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
