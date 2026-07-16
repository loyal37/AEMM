use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GameEdition {
    China,
    International,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GameDiscoverySource {
    ConfiguredPath,
    LauncherRegistry,
    KnownInstallRoot,
    ManualSelection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameVersionInfo {
    pub value: Option<String>,
    pub source: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInstallation {
    pub adapter_id: String,
    pub edition: GameEdition,
    pub installation_root: PathBuf,
    pub executable: PathBuf,
    pub loader_root: Option<PathBuf>,
    pub version: GameVersionInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameValidation {
    pub valid: bool,
    pub confidence: u8,
    pub evidence: Vec<String>,
    pub issues: Vec<String>,
    pub installation: Option<GameInstallation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedGameInstallation {
    pub source: GameDiscoverySource,
    pub validation: GameValidation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EfmiValidation {
    pub valid: bool,
    pub launch_ready: bool,
    pub root: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub configured_game_executable: Option<PathBuf>,
    pub evidence: Vec<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStatus {
    pub configured: bool,
    pub installation: Option<GameValidation>,
    pub loader: Option<EfmiValidation>,
    pub launch_mode: super::LaunchMode,
    pub can_launch: bool,
    pub launch_block_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSpec {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunchResult {
    pub process_id: u32,
    pub mode: super::LaunchMode,
}
