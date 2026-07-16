use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ModFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentContext {
    pub profile_id: Uuid,
    pub mod_id: Uuid,
    pub repository_root: PathBuf,
    pub mod_root: PathBuf,
    pub destination_root: PathBuf,
    pub source_content_fingerprint: String,
    pub files: Vec<ModFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentEntry {
    pub source_relative: PathBuf,
    pub destination_relative: PathBuf,
    pub size_bytes: u64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentPlan {
    pub operation_id: Uuid,
    pub profile_id: Uuid,
    pub mod_id: Uuid,
    pub strategy_id: String,
    pub destination_directory: PathBuf,
    pub source_content_fingerprint: String,
    pub entries: Vec<DeploymentEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentManifest {
    pub schema_version: u32,
    pub id: Uuid,
    pub profile_id: Uuid,
    pub mod_id: Uuid,
    pub strategy_id: String,
    pub destination_root: PathBuf,
    pub destination_directory: PathBuf,
    pub source_content_fingerprint: String,
    pub entries: Vec<DeploymentEntry>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct DeploymentRevokeReceipt {
    pub manifest: DeploymentManifest,
    pub tombstone_directory: PathBuf,
}
