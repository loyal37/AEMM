use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModImportSourceKind {
    Zip,
    SevenZip,
    Rar,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModImportPlan {
    pub operation_id: Uuid,
    pub source_kind: ModImportSourceKind,
    pub source_name: String,
    pub logical_id: String,
    pub name: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub file_count: u64,
    pub size_bytes: u64,
    pub content_fingerprint: String,
    pub destination_relative_path: PathBuf,
    pub warnings: Vec<String>,
    pub blocking_issues: Vec<String>,
    pub can_install: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareModImport {
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModImportOperation {
    pub operation_id: Uuid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModInstallProgressStage {
    Inspecting,
    Extracting,
    Analyzing,
    Ready,
    Committing,
    Synchronizing,
    RollingBack,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallProgress {
    pub operation_id: Uuid,
    pub stage: ModInstallProgressStage,
    pub message: String,
    pub processed_items: u64,
    pub total_items: Option<u64>,
    pub processed_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallResult {
    pub operation_id: Uuid,
    pub mod_id: Uuid,
    pub name: String,
    pub repository_path: PathBuf,
}
