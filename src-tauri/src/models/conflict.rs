use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    DeploymentPath,
    EfmiNamespace,
    EfmiTextureOverride,
    EfmiShaderOverride,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictSeverity {
    Information,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictEvidence {
    pub source_path: PathBuf,
    pub section: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictParticipant {
    pub mod_id: Uuid,
    pub mod_name: String,
    pub load_order: u32,
    pub evidence: Vec<ConflictEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub id: String,
    pub analyzer_id: String,
    pub kind: ConflictKind,
    pub severity: ConflictSeverity,
    pub resource_key: String,
    pub summary: String,
    pub participants: Vec<ConflictParticipant>,
    pub winning_mod_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictReport {
    pub profile_id: Uuid,
    pub generated_at: i64,
    pub enabled_mods: u64,
    pub analyzed_ini_files: u64,
    pub affected_mods: u64,
    pub conflicts: Vec<Conflict>,
    pub load_order_verified: bool,
    pub load_order_note: String,
    pub warnings: Vec<String>,
}
