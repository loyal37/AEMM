use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub kind: ConflictKind,
    pub severity: ConflictSeverity,
    pub resource_key: String,
    pub mod_ids: Vec<Uuid>,
    pub winning_mod_id: Option<Uuid>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    DeploymentPath,
    EfmiOverride,
    Dependency,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictSeverity {
    Information,
    Warning,
    Error,
}
