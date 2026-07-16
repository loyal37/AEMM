use std::path::{Path, PathBuf};

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::{AuthorModMetadata, InstalledMod},
};

mod metadata;
mod repository;
mod scanner;

pub use metadata::FileSystemMetadataManager;
pub use repository::{RepositoryInitializationPolicy, RepositoryRelativePath, RepositoryRoot};
pub use scanner::{
    CachedModFile, FileSystemModScanner, RepositoryScan, ScanCache, ScanIssue, ScannedMod,
};

#[derive(Debug, Clone)]
pub enum ModInstallSource {
    Archive(PathBuf),
    Directory(PathBuf),
}

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub operation_id: Uuid,
    pub source: ModInstallSource,
    pub candidate: ScannedMod,
    pub destination_relative_path: PathBuf,
    pub warnings: Vec<String>,
}

#[async_trait]
pub trait ModScanner: Send + Sync {
    async fn scan_repository(
        &self,
        repository_root: RepositoryRoot,
        cache: ScanCache,
    ) -> Result<RepositoryScan, AppError>;
    async fn scan_candidate(&self, candidate_root: &Path) -> Result<ScannedMod, AppError>;
}

#[async_trait]
pub trait ModMetadataManager: Send + Sync {
    async fn read_author_metadata(&self, mod_root: &Path) -> Result<AuthorModMetadata, AppError>;
}

#[async_trait]
pub trait ModInstaller: Send + Sync {
    async fn prepare(&self, source: ModInstallSource) -> Result<InstallPlan, AppError>;
    async fn commit(&self, plan: InstallPlan) -> Result<InstalledMod, AppError>;
    async fn rollback(&self, operation_id: Uuid) -> Result<(), AppError>;
}

#[async_trait]
pub trait ModManager: Send + Sync {
    async fn list(&self) -> Result<Vec<InstalledMod>, AppError>;
    async fn uninstall(&self, mod_id: Uuid) -> Result<(), AppError>;
}
