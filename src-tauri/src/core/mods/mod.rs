use std::path::Path;

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    errors::AppError,
    models::{AuthorModMetadata, InstalledMod},
};

mod archive;
mod installer;
mod metadata;
mod repository;
mod root_detector;
mod scanner;
mod staging;

pub use archive::{ExtractionPolicy, InstallProgressReporter, StagedSource, emit, stage_source};
pub use installer::{
    CommitReceipt, ExistingModIdentity, InstallJournalState, PendingInstall, SafeModInstaller,
};
pub use metadata::FileSystemMetadataManager;
pub(crate) use repository::path_is_link_or_reparse_point;
pub use repository::{RepositoryInitializationPolicy, RepositoryRelativePath, RepositoryRoot};
pub use root_detector::{DetectedModRoot, detect_mod_root};
pub use scanner::{
    CachedModFile, FileSystemModScanner, RepositoryScan, ScanCache, ScanIssue, ScannedMod,
};
pub use staging::{
    OPERATION_MARKER_FILE, STAGING_MARKER_FILE, StagingInitializationPolicy, StagingOperation,
    StagingRoot,
};

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
pub trait ModManager: Send + Sync {
    async fn list(&self) -> Result<Vec<InstalledMod>, AppError>;
    async fn uninstall(&self, mod_id: Uuid) -> Result<(), AppError>;
}
