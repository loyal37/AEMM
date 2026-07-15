use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInstallation {
    pub adapter_id: String,
    pub edition: String,
    pub installation_root: PathBuf,
    pub executable: PathBuf,
    pub loader_root: Option<PathBuf>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameValidation {
    pub valid: bool,
    pub confidence: u8,
    pub evidence: Vec<String>,
    pub installation: Option<GameInstallation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSpec {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
    pub arguments: Vec<String>,
}
