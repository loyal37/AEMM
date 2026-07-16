use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use tokio::sync::Mutex;

use crate::{
    core::mods::{
        ExistingModIdentity, FileSystemModScanner, InstallProgressReporter, ModScanner,
        RepositoryInitializationPolicy, RepositoryRelativePath, RepositoryRoot, SafeModInstaller,
        StagingInitializationPolicy, StagingRoot, emit,
    },
    database::{Database, ModStore},
    errors::AppError,
    models::{
        LocalModMetadata, ModDetails, ModImportPlan, ModInstallProgressStage, ModInstallResult,
        ModListItem, ModMutationResult, ModPreview, ModScanResult,
    },
};

use super::SettingsService;

const MAX_OVERRIDE_LENGTH: usize = 512;
const MAX_DESCRIPTION_LENGTH: usize = 32 * 1024;
const MAX_NOTES_LENGTH: usize = 32 * 1024;
const MAX_TAGS: usize = 64;
const MAX_TAG_LENGTH: usize = 64;
const MAX_REPORTED_ISSUES: usize = 200;
const MAX_FAVORITE_BATCH: usize = 10_000;
const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct ModService {
    settings: SettingsService,
    default_repository_path: PathBuf,
    default_staging_path: PathBuf,
    scanner: FileSystemModScanner,
    store: ModStore,
    scan_lock: Arc<Mutex<()>>,
    install_lock: Arc<Mutex<()>>,
}

impl ModService {
    pub fn new(
        settings: SettingsService,
        database: &Database,
        default_repository_path: PathBuf,
        default_staging_path: PathBuf,
    ) -> Self {
        Self {
            settings,
            default_repository_path,
            default_staging_path,
            scanner: FileSystemModScanner::new(),
            store: ModStore::new(database.pool().clone()),
            scan_lock: Arc::new(Mutex::new(())),
            install_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn scan_repository(&self) -> Result<ModScanResult, AppError> {
        let _scan_guard = self.scan_lock.lock().await;
        self.scan_repository_locked().await
    }

    async fn scan_repository_locked(&self) -> Result<ModScanResult, AppError> {
        let started = Instant::now();
        let root = self.repository_root().await?;

        tracing::info!(repository_root = %root.path().display(), "starting mod repository scan");
        let cache = self.store.load_scan_cache().await?;
        let scan = self.scanner.scan_repository(root, cache).await?;
        let sync = self.store.synchronize(&scan).await?;
        let discovered = u64::try_from(scan.mods.len())
            .map_err(|_| AppError::DataIntegrity("mod count overflow".to_owned()))?;
        let mut issues = scan
            .issues
            .iter()
            .chain(scan.mods.iter().flat_map(|item| item.issues.iter()))
            .map(|issue| issue.display_message())
            .take(MAX_REPORTED_ISSUES)
            .collect::<Vec<_>>();
        let total_issue_count = scan.issues.len()
            + scan
                .mods
                .iter()
                .map(|item| item.issues.len())
                .sum::<usize>();
        if total_issue_count > MAX_REPORTED_ISSUES {
            issues.push(format!(
                "另有 {} 条扫描提示未在结果中展开，请查看日志。",
                total_issue_count - MAX_REPORTED_ISSUES
            ));
        }
        for issue in &issues {
            tracing::warn!(scan_issue = issue, "mod repository scan issue");
        }

        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let result = ModScanResult {
            discovered,
            added: sync.added,
            updated: sync.updated,
            unchanged: sync.unchanged,
            broken: sync.broken,
            missing: sync.missing,
            hashed_files: scan.hashed_files,
            reused_hashes: scan.reused_hashes,
            skipped_entries: scan.skipped_entries,
            duration_ms,
            issues,
        };
        tracing::info!(
            discovered = result.discovered,
            added = result.added,
            updated = result.updated,
            unchanged = result.unchanged,
            broken = result.broken,
            missing = result.missing,
            hashed_files = result.hashed_files,
            reused_hashes = result.reused_hashes,
            duration_ms = result.duration_ms,
            filesystem_duration_ms = scan.duration.as_millis(),
            "mod repository scan completed"
        );
        Ok(result)
    }

    pub async fn prepare_import(
        &self,
        source_path: PathBuf,
        progress: InstallProgressReporter,
    ) -> Result<ModImportPlan, AppError> {
        let _install_guard = self.install_lock.lock().await;
        let installer = self.installer().await?;
        let existing = self.existing_install_identities().await?;
        installer.prepare(source_path, existing, progress).await
    }

    pub async fn commit_import(
        &self,
        operation_id: uuid::Uuid,
        progress: InstallProgressReporter,
    ) -> Result<ModInstallResult, AppError> {
        let _install_guard = self.install_lock.lock().await;
        let _scan_guard = self.scan_lock.lock().await;
        let installer = self.installer().await?;
        let existing = self.existing_install_identities().await?;
        let receipt = match installer
            .commit(operation_id, existing, progress.clone())
            .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Err(recovery_error) = installer.recover_operation(operation_id, false).await
                {
                    tracing::error!(operation_id = %operation_id, error = %recovery_error, "failed to clean an unsuccessful installation commit; startup recovery will retry");
                }
                return Err(error);
            }
        };
        let sync_result = self.scan_repository_locked().await;
        if let Err(error) = sync_result {
            if let Err(rollback_error) = installer.rollback_receipt(&receipt, &progress).await {
                tracing::error!(operation_id = %operation_id, error = %rollback_error, "database synchronization and installation rollback both failed");
            }
            return Err(error);
        }
        let item = match self
            .store
            .list_item_by_repository_path(receipt.destination_relative().as_path())
            .await
        {
            Ok(item) => item,
            Err(error) => {
                if let Err(rollback_error) = installer.rollback_receipt(&receipt, &progress).await {
                    tracing::error!(operation_id = %operation_id, error = %rollback_error, "installed record lookup and filesystem rollback both failed");
                }
                if let Err(rescan_error) = self.scan_repository_locked().await {
                    tracing::error!(operation_id = %operation_id, error = %rescan_error, "failed to synchronize database after installation rollback");
                }
                return Err(error);
            }
        };
        if let Err(error) = installer.mark_database_synced(&receipt) {
            tracing::error!(operation_id = %operation_id, error = %error, "database is synchronized but installation journal state could not be persisted; startup recovery will preserve the committed mod");
        }
        let cleanup_installer = installer.clone();
        let cleanup_receipt = receipt.clone();
        if let Err(error) =
            tokio::task::spawn_blocking(move || cleanup_installer.finalize(&cleanup_receipt))
                .await?
        {
            tracing::warn!(operation_id = %operation_id, error = %error, "installation completed but staging cleanup is pending startup recovery");
        }
        emit(
            &progress,
            operation_id,
            ModInstallProgressStage::Completed,
            "模组安装完成。",
            item.file_count,
            Some(item.file_count),
            item.size_bytes,
            Some(item.size_bytes),
        );
        tracing::info!(operation_id = %operation_id, mod_id = %item.id, logical_id = %item.logical_id, "mod installation completed");
        Ok(ModInstallResult {
            operation_id,
            mod_id: item.id,
            name: item.name,
            repository_path: item.repository_path,
        })
    }

    pub async fn cancel_import(&self, operation_id: uuid::Uuid) -> Result<(), AppError> {
        let _install_guard = self.install_lock.lock().await;
        let installer = self.installer().await?;
        tokio::task::spawn_blocking(move || installer.cancel(operation_id)).await?
    }

    pub async fn recover_pending_installations(&self) -> Result<(), AppError> {
        let _install_guard = self.install_lock.lock().await;
        let _scan_guard = self.scan_lock.lock().await;
        let installer = self.installer().await?;
        let pending = installer.pending_installs()?;
        for install in pending {
            let database_has_committed_mod = self
                .store
                .contains_repository_fingerprint(
                    &install.destination_relative,
                    &install.content_fingerprint,
                )
                .await?;
            if let Err(error) = installer
                .recover_operation(install.operation_id, database_has_committed_mod)
                .await
            {
                tracing::error!(operation_id = %install.operation_id, state = ?install.state, error = %error, "failed to recover interrupted mod installation; unsafe files were preserved for manual inspection");
            }
        }
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<ModListItem>, AppError> {
        self.store.list().await
    }

    pub async fn details(&self, mod_id: uuid::Uuid) -> Result<ModDetails, AppError> {
        self.store.details(mod_id).await
    }

    pub async fn set_favorite(
        &self,
        mod_ids: Vec<uuid::Uuid>,
        favorite: bool,
    ) -> Result<ModMutationResult, AppError> {
        if mod_ids.len() > MAX_FAVORITE_BATCH {
            return Err(AppError::ModMetadata(format!(
                "一次最多更新 {MAX_FAVORITE_BATCH} 个模组的收藏状态。"
            )));
        }
        let mod_ids = mod_ids.into_iter().collect::<HashSet<_>>();
        let mod_ids = mod_ids.into_iter().collect::<Vec<_>>();
        let updated = self.store.set_favorite(&mod_ids, favorite).await?;
        tracing::info!(updated, favorite, "mod favorite state updated");
        Ok(ModMutationResult { updated })
    }

    pub async fn mod_directory(&self, mod_id: uuid::Uuid) -> Result<PathBuf, AppError> {
        let reference = self.store.content_reference(mod_id).await?;
        let root = self.repository_root().await?;
        tokio::task::spawn_blocking(move || {
            let relative = RepositoryRelativePath::new(reference.repository_path)?;
            root.resolve_existing_mod_root(&relative)
        })
        .await?
    }

    pub async fn preview(&self, mod_id: uuid::Uuid) -> Result<Option<ModPreview>, AppError> {
        let reference = self.store.content_reference(mod_id).await?;
        let Some(preview_path) = reference.preview_path else {
            return Ok(None);
        };
        let root = self.repository_root().await?;
        tokio::task::spawn_blocking(move || {
            let relative =
                RepositoryRelativePath::new(reference.repository_path.join(preview_path))?;
            let preview_path = root.resolve_existing(&relative)?;
            read_preview(&preview_path)
        })
        .await?
    }

    pub async fn update_local_metadata(
        &self,
        mod_id: uuid::Uuid,
        metadata: LocalModMetadata,
    ) -> Result<ModListItem, AppError> {
        let metadata = validate_local_metadata(metadata)?;
        self.store.update_local_metadata(mod_id, &metadata).await
    }

    async fn repository_root(&self) -> Result<RepositoryRoot, AppError> {
        let settings = self.settings.get().await;
        let configured_path = settings.storage.repository_path;
        let policy =
            if paths_equal_case_insensitive(&configured_path, &self.default_repository_path) {
                RepositoryInitializationPolicy::TrustedAemmDefault
            } else {
                RepositoryInitializationPolicy::EmptyOnly
            };
        tokio::task::spawn_blocking(move || {
            RepositoryRoot::open_or_initialize(&configured_path, policy)
        })
        .await?
    }

    async fn staging_root(&self) -> Result<StagingRoot, AppError> {
        let settings = self.settings.get().await;
        let configured_path = settings.storage.staging_path;
        let policy = if paths_equal_case_insensitive(&configured_path, &self.default_staging_path) {
            StagingInitializationPolicy::TrustedAemmDefault
        } else {
            StagingInitializationPolicy::EmptyOnly
        };
        tokio::task::spawn_blocking(move || {
            StagingRoot::open_or_initialize(&configured_path, policy)
        })
        .await?
    }

    async fn installer(&self) -> Result<SafeModInstaller, AppError> {
        let repository = self.repository_root().await?;
        let staging = self.staging_root().await?;
        Ok(SafeModInstaller::new(repository, staging))
    }

    async fn existing_install_identities(&self) -> Result<Vec<ExistingModIdentity>, AppError> {
        Ok(self
            .store
            .install_identities()
            .await?
            .into_iter()
            .map(|identity| ExistingModIdentity {
                logical_id: identity.logical_id,
                name: identity.name,
                repository_path: identity.repository_path,
                content_fingerprint: identity.content_fingerprint,
                lifecycle_state: identity.lifecycle_state,
            })
            .collect())
    }
}

fn read_preview(path: &Path) -> Result<Option<ModPreview>, AppError> {
    let metadata = fs::metadata(path).map_err(|source| AppError::file_system(path, source))?;
    if !metadata.is_file() || metadata.len() > MAX_PREVIEW_BYTES {
        tracing::warn!(preview = %path.display(), size = metadata.len(), "mod preview skipped because it is not a regular file or exceeds the size limit");
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|source| AppError::file_system(path, source))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_PREVIEW_BYTES {
        tracing::warn!(preview = %path.display(), "mod preview grew beyond the size limit while being read");
        return Ok(None);
    }
    let Some(media_type) = image_media_type(&bytes) else {
        tracing::warn!(preview = %path.display(), "mod preview skipped because its file signature is unsupported");
        return Ok(None);
    };
    Ok(Some(ModPreview {
        data_url: format!("data:{media_type};base64,{}", STANDARD.encode(bytes)),
    }))
}

fn image_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else {
        None
    }
}

fn validate_local_metadata(mut metadata: LocalModMetadata) -> Result<LocalModMetadata, AppError> {
    metadata.display_name_override = normalize_optional(
        metadata.display_name_override,
        "本地显示名称",
        MAX_OVERRIDE_LENGTH,
    )?;
    metadata.category_override =
        normalize_optional(metadata.category_override, "本地分类", MAX_OVERRIDE_LENGTH)?;
    metadata.description_override = normalize_optional(
        metadata.description_override,
        "本地描述",
        MAX_DESCRIPTION_LENGTH,
    )?;
    metadata.notes = normalize_optional(metadata.notes, "本地备注", MAX_NOTES_LENGTH)?;
    if metadata.tags.len() > MAX_TAGS {
        return Err(AppError::ModMetadata(format!(
            "标签数量不能超过 {MAX_TAGS}。"
        )));
    }
    let mut seen = HashSet::new();
    let mut tags = Vec::with_capacity(metadata.tags.len());
    for tag in metadata.tags {
        let tag = tag.trim().to_owned();
        if tag.is_empty() || tag.chars().count() > MAX_TAG_LENGTH || tag.contains('\0') {
            return Err(AppError::ModMetadata(format!(
                "标签不能为空、包含 NUL 或超过 {MAX_TAG_LENGTH} 个字符。"
            )));
        }
        if seen.insert(tag.to_lowercase()) {
            tags.push(tag);
        }
    }
    metadata.tags = tags;
    Ok(metadata)
}

fn normalize_optional(
    value: Option<String>,
    label: &str,
    max_length: usize,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max_length || value.contains('\0') {
        return Err(AppError::ModMetadata(format!(
            "{label}不能包含 NUL 且不能超过 {max_length} 个字符。"
        )));
    }
    Ok(Some(value))
}

fn paths_equal_case_insensitive(left: &Path, right: &Path) -> bool {
    normalized_path_key(left) == normalized_path_key(right)
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use crate::{
        models::LocalModMetadata,
        services::mods::{image_media_type, validate_local_metadata},
    };

    #[test]
    fn normalizes_and_deduplicates_local_tags() -> Result<(), Box<dyn std::error::Error>> {
        let metadata = validate_local_metadata(LocalModMetadata {
            display_name_override: Some("  Local Name  ".to_owned()),
            tags: vec!["Character".to_owned(), "character".to_owned()],
            ..LocalModMetadata::default()
        })?;
        assert_eq!(
            metadata.display_name_override.as_deref(),
            Some("Local Name")
        );
        assert_eq!(metadata.tags, vec!["Character"]);
        Ok(())
    }

    #[test]
    fn accepts_only_supported_preview_signatures() {
        assert_eq!(
            image_media_type(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(image_media_type(b"\xff\xd8\xffrest"), Some("image/jpeg"));
        assert_eq!(image_media_type(b"RIFF0000WEBPrest"), Some("image/webp"));
        assert_eq!(image_media_type(b"GIF89arest"), Some("image/gif"));
        assert_eq!(image_media_type(b"<svg onload='bad'>"), None);
    }
}
