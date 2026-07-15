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
    pub files: Vec<ModFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentEntry {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub content_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentPlan {
    pub strategy_id: String,
    pub entries: Vec<DeploymentEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentManifest {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub mod_id: Uuid,
    pub strategy_id: String,
    pub destination_root: PathBuf,
    pub entries: Vec<DeploymentEntry>,
    pub created_at: i64,
}
