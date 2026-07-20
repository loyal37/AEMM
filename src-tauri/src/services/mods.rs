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
        PendingRemovalRecovery, REMOVAL_TOMBSTONE_PREFIX, RepositoryInitializationPolicy,
        RepositoryRelativePath, RepositoryRemoval, RepositoryRoot, SafeModInstaller,
        StagingInitializationPolicy, StagingRoot, emit,
    },
    database::{Database, ModStore, StoredModRemoval},
    errors::AppError,
    models::{
        AppSettings, LocalModMetadata, ModDetails, ModImportPlan, ModInstallProgressStage,
        ModInstallResult, ModListItem, ModMutationResult, ModPreview, ModRemovalResult,
        ModScanResult, StorageSettings,
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
const MAX_REMOVAL_BATCH: usize = 256;
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
    deployment_lock: Arc<Mutex<()>>,
}

impl ModService {
    pub fn new(
        settings: SettingsService,
        database: &Database,
        default_repository_path: PathBuf,
        default_staging_path: PathBuf,
        deployment_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            settings,
            default_repository_path,
            default_staging_path,
            scanner: FileSystemModScanner::new(),
            store: ModStore::new(database.pool().clone()),
            scan_lock: Arc::new(Mutex::new(())),
            install_lock: Arc::new(Mutex::new(())),
            deployment_lock,
        }
    }

    pub async fn scan_repository(&self) -> Result<ModScanResult, AppError> {
        let _deployment_guard = self.deployment_lock.lock().await;
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
        let _deployment_guard = self.deployment_lock.lock().await;
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
        let _deployment_guard = self.deployment_lock.lock().await;
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

    pub async fn uninstall(&self, mod_ids: Vec<uuid::Uuid>) -> Result<ModRemovalResult, AppError> {
        if mod_ids.len() > MAX_REMOVAL_BATCH {
            return Err(AppError::ModInstall(format!(
                "一次最多卸载 {MAX_REMOVAL_BATCH} 个模组。"
            )));
        }
        let mut seen = HashSet::new();
        let mod_ids = mod_ids
            .into_iter()
            .filter(|mod_id| seen.insert(*mod_id))
            .collect::<Vec<_>>();
        if mod_ids.is_empty() {
            return Ok(ModRemovalResult {
                removed: 0,
                warnings: Vec::new(),
            });
        }

        let _deployment_guard = self.deployment_lock.lock().await;
        let _install_guard = self.install_lock.lock().await;
        let _scan_guard = self.scan_lock.lock().await;
        let root = self.repository_root().await?;
        let requests = mod_ids
            .iter()
            .map(|mod_id| {
                (
                    *mod_id,
                    PathBuf::from(format!("{REMOVAL_TOMBSTONE_PREFIX}{mod_id}")),
                )
            })
            .collect::<Vec<_>>();
        let records = self.store.prepare_removals(&requests).await?;
        let quarantine_root = root.clone();
        let quarantine_records = records.clone();
        let quarantined = match tokio::task::spawn_blocking(move || {
            quarantine_removals(&quarantine_root, &quarantine_records)
        })
        .await?
        {
            Ok(removals) => removals,
            Err(failure) => {
                if failure.rollback_failed {
                    tracing::error!(error = %failure.error, "mod uninstall failed and filesystem rollback is incomplete; startup recovery will retry");
                    return Err(failure.error);
                }
                self.store.rollback_removals(&records).await?;
                return Err(failure.error);
            }
        };

        if let Err(commit_error) = self.store.commit_removals(&records).await {
            let rollback_root = root.clone();
            let rollback_removals = quarantined.clone();
            if let Err(rollback_error) = tokio::task::spawn_blocking(move || {
                restore_removals(&rollback_root, &rollback_removals)
            })
            .await?
            {
                tracing::error!(error = %commit_error, rollback_error = %rollback_error, "mod uninstall database commit and filesystem rollback both failed; startup recovery will retry");
                return Err(rollback_error);
            }
            self.store.rollback_removals(&records).await?;
            return Err(commit_error);
        }

        let mut warnings = Vec::new();
        for removal in quarantined {
            let cleanup_root = root.clone();
            let cleanup_removal = removal.clone();
            if let Err(error) = tokio::task::spawn_blocking(move || {
                cleanup_root.finalize_quarantined_mod(&cleanup_removal)
            })
            .await?
            {
                tracing::warn!(mod_id = %removal.mod_id(), error = %error, "mod was uninstalled but repository tombstone cleanup is pending startup recovery");
                warnings.push(format!(
                    "模组 {} 已从数据库移除，但文件清理将在下次启动时重试。",
                    removal.mod_id()
                ));
            }
        }
        let removed = u64::try_from(records.len())
            .map_err(|_| AppError::DataIntegrity("卸载模组数量超出支持范围。".to_owned()))?;
        tracing::info!(removed, "mod uninstall transaction completed");
        Ok(ModRemovalResult { removed, warnings })
    }

    pub async fn recover_pending_removals(&self) -> Result<(), AppError> {
        let _deployment_guard = self.deployment_lock.lock().await;
        let _install_guard = self.install_lock.lock().await;
        let _scan_guard = self.scan_lock.lock().await;
        let root = self.repository_root().await?;
        for record in self.store.pending_removals().await? {
            let removal = repository_removal(&record)?;
            let recovery_root = root.clone();
            let recovery_removal = removal.clone();
            match tokio::task::spawn_blocking(move || {
                recovery_root.recover_pending_removal(&recovery_removal)
            })
            .await?
            {
                Ok(outcome) => {
                    let mut rollback_record = record.clone();
                    if outcome == PendingRemovalRecovery::Missing {
                        rollback_record.previous_lifecycle_state =
                            crate::models::ModLifecycleState::Broken;
                    }
                    self.store.rollback_removals(&[rollback_record]).await?;
                    tracing::warn!(mod_id = %record.mod_id, ?outcome, "recovered interrupted mod uninstall by restoring database ownership");
                }
                Err(error) => {
                    tracing::error!(mod_id = %record.mod_id, error = %error, "interrupted mod uninstall could not be restored safely; files and journal were preserved");
                }
            }
        }

        let tombstones = {
            let list_root = root.clone();
            tokio::task::spawn_blocking(move || list_root.removal_tombstones()).await??
        };
        for tombstone in tombstones {
            let inspect_root = root.clone();
            let inspect_tombstone = tombstone.clone();
            let removal = match tokio::task::spawn_blocking(move || {
                inspect_root.inspect_removal_tombstone(&inspect_tombstone)
            })
            .await?
            {
                Ok(removal) => removal,
                Err(error) => {
                    tracing::error!(path = %tombstone.storage_key(), error = %error, "invalid removal tombstone was preserved for manual inspection");
                    continue;
                }
            };
            if self.store.contains_mod(removal.mod_id()).await? {
                tracing::error!(mod_id = %removal.mod_id(), "orphan removal tombstone still has a mod database record; preserved for manual inspection");
                continue;
            }
            let cleanup_root = root.clone();
            let cleanup_removal = removal.clone();
            match tokio::task::spawn_blocking(move || {
                cleanup_root.finalize_quarantined_mod(&cleanup_removal)
            })
            .await?
            {
                Ok(()) => {
                    tracing::info!(mod_id = %removal.mod_id(), "finalized committed mod uninstall during startup recovery")
                }
                Err(error) => {
                    tracing::error!(mod_id = %removal.mod_id(), error = %error, "committed mod uninstall cleanup remains pending")
                }
            }
        }
        Ok(())
    }

    pub async fn configure_storage(
        &self,
        requested: StorageSettings,
    ) -> Result<AppSettings, AppError> {
        let _deployment_guard = self.deployment_lock.lock().await;
        let _install_guard = self.install_lock.lock().await;
        let _scan_guard = self.scan_lock.lock().await;
        let mut settings = self.settings.get().await;
        let repository_changed = !paths_equal_case_insensitive(
            &settings.storage.repository_path,
            &requested.repository_path,
        );
        let staging_changed =
            !paths_equal_case_insensitive(&settings.storage.staging_path, &requested.staging_path);
        if !repository_changed && !staging_changed {
            return Ok(settings);
        }

        let mut candidate = settings.clone();
        candidate.storage = requested.clone();
        SettingsService::validate_candidate(&candidate)?;

        if repository_changed {
            let current_repository = self.repository_root().await?;
            let current_has_content = {
                let root = current_repository.clone();
                tokio::task::spawn_blocking(move || root.has_repository_content()).await??
            };
            if self.store.count().await? != 0 || current_has_content {
                return Err(AppError::ConfigValidation(
                    "更改模组仓库前必须先卸载全部模组并清空旧仓库；AEMM 不会隐式搬运或遗留数据库引用。"
                        .to_owned(),
                ));
            }
        }
        if staging_changed {
            let current_staging = self.staging_root().await?;
            let pending =
                tokio::task::spawn_blocking(move || current_staging.operation_ids()).await??;
            if !pending.is_empty() {
                return Err(AppError::ConfigValidation(
                    "存在未完成的安装事务，暂时不能更改临时目录。".to_owned(),
                ));
            }
        }

        let repository_policy =
            repository_policy(&requested.repository_path, &self.default_repository_path);
        let requested_repository = requested.repository_path.clone();
        let repository = tokio::task::spawn_blocking(move || {
            RepositoryRoot::open_or_initialize(&requested_repository, repository_policy)
        })
        .await??;
        let repository_check = repository.clone();
        let removal_tombstones =
            tokio::task::spawn_blocking(move || repository_check.removal_tombstones()).await??;
        if !removal_tombstones.is_empty() {
            return Err(AppError::ConfigValidation(
                "新仓库包含未完成的模组卸载暂存目录，必须先使用原数据库恢复。".to_owned(),
            ));
        }

        let staging_policy =
            if paths_equal_case_insensitive(&requested.staging_path, &self.default_staging_path) {
                StagingInitializationPolicy::TrustedAemmDefault
            } else {
                StagingInitializationPolicy::EmptyOnly
            };
        let requested_staging = requested.staging_path.clone();
        let staging = tokio::task::spawn_blocking(move || {
            StagingRoot::open_or_initialize(&requested_staging, staging_policy)
        })
        .await??;
        let staging_check = staging.clone();
        let pending_operations =
            tokio::task::spawn_blocking(move || staging_check.operation_ids()).await??;
        if !pending_operations.is_empty() {
            return Err(AppError::ConfigValidation(
                "新临时目录包含其他未完成的安装事务，已拒绝接管。".to_owned(),
            ));
        }

        settings.storage = StorageSettings {
            repository_path: repository.path().to_path_buf(),
            staging_path: staging.path().to_path_buf(),
        };
        self.settings.update(settings).await
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
        let policy = repository_policy(&configured_path, &self.default_repository_path);
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

fn repository_policy(path: &Path, default_path: &Path) -> RepositoryInitializationPolicy {
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Mods"))
    {
        RepositoryInitializationPolicy::ExternalEfmiMods
    } else if paths_equal_case_insensitive(path, default_path) {
        RepositoryInitializationPolicy::TrustedAemmDefault
    } else {
        RepositoryInitializationPolicy::EmptyOnly
    }
}

struct RemovalFailure {
    error: AppError,
    rollback_failed: bool,
}

fn repository_removal(record: &StoredModRemoval) -> Result<RepositoryRemoval, AppError> {
    RepositoryRemoval::new(
        record.mod_id,
        RepositoryRelativePath::new(record.original_repository_path.clone())?,
        RepositoryRelativePath::new(record.tombstone_repository_path.clone())?,
    )
}

fn quarantine_removals(
    root: &RepositoryRoot,
    records: &[StoredModRemoval],
) -> Result<Vec<RepositoryRemoval>, RemovalFailure> {
    let mut quarantined = Vec::new();
    for record in records {
        let removal = repository_removal(record).map_err(|error| RemovalFailure {
            error,
            rollback_failed: false,
        })?;
        match root.quarantine_mod_root(removal.original_relative(), removal.mod_id()) {
            Ok(Some(actual)) if actual == removal => quarantined.push(actual),
            Ok(Some(_)) => {
                let error =
                    AppError::DataIntegrity("仓库返回了不一致的模组卸载暂存记录。".to_owned());
                let rollback_failed = restore_removals(root, &quarantined).is_err();
                return Err(RemovalFailure {
                    error,
                    rollback_failed,
                });
            }
            Ok(None) => {}
            Err(error) => {
                let rollback_failed = restore_removals(root, &quarantined).is_err();
                return Err(RemovalFailure {
                    error,
                    rollback_failed,
                });
            }
        }
    }
    Ok(quarantined)
}

fn restore_removals(root: &RepositoryRoot, removals: &[RepositoryRemoval]) -> Result<(), AppError> {
    for removal in removals.iter().rev() {
        root.restore_quarantined_mod(removal, false)?;
    }
    Ok(())
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
    use std::{fs, path::Path, sync::Arc};

    use tokio::sync::Mutex;

    use crate::{
        core::mods::{REMOVAL_TOMBSTONE_PREFIX, RepositoryRelativePath},
        database::Database,
        models::{LocalModMetadata, StorageSettings},
        services::{
            AppPaths, SettingsService,
            mods::{ModService, image_media_type, validate_local_metadata},
        },
    };

    async fn removal_fixture(
        root: &Path,
    ) -> Result<(ModService, Database, AppPaths, uuid::Uuid), Box<dyn std::error::Error>> {
        let paths = AppPaths::for_test(root);
        paths.ensure_base_directories().await?;
        let settings = SettingsService::load_or_create(&paths).await?;
        let database = Database::connect(&paths.database_file).await?;
        let mod_root = paths.repository_directory.join("DISABLED_removal-fixture");
        fs::create_dir(&mod_root)?;
        fs::write(
            mod_root.join("mod.json"),
            br#"{"id":"fixture.removal","name":"Removal Fixture"}"#,
        )?;
        fs::write(
            mod_root.join("content.ini"),
            b"[Constants]\nglobal persist $x = 1",
        )?;
        let service = ModService::new(
            settings,
            &database,
            paths.repository_directory.clone(),
            paths.staging_directory.clone(),
            Arc::new(Mutex::new(())),
        );
        service.scan_repository().await?;
        let mod_id = service
            .list()
            .await?
            .first()
            .ok_or("fixture mod missing after scan")?
            .id;
        Ok((service, database, paths, mod_id))
    }

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

    #[tokio::test]
    async fn uninstalls_disabled_mod_and_cascades_profile_membership()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let (service, database, paths, mod_id) = removal_fixture(directory.path()).await?;
        sqlx::query(
            "UPDATE profile_mods SET enabled = 0
             WHERE profile_id = '00000000-0000-0000-0000-000000000001' AND mod_id = ?",
        )
        .bind(mod_id.to_string())
        .execute(database.pool())
        .await?;

        let result = service.uninstall(vec![mod_id]).await?;

        assert_eq!(result.removed, 1);
        assert!(result.warnings.is_empty());
        assert!(
            !paths
                .repository_directory
                .join("DISABLED_removal-fixture")
                .exists()
        );
        let mods: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mods")
            .fetch_one(database.pool())
            .await?;
        let memberships: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM profile_mods")
            .fetch_one(database.pool())
            .await?;
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_mod_removals")
            .fetch_one(database.pool())
            .await?;
        assert_eq!((mods, memberships, pending), (0, 0, 0));
        Ok(())
    }

    #[tokio::test]
    async fn refuses_to_uninstall_active_enabled_mod() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let (service, database, paths, mod_id) = removal_fixture(directory.path()).await?;
        sqlx::query(
            "UPDATE profile_mods SET enabled = 1
             WHERE profile_id = '00000000-0000-0000-0000-000000000001' AND mod_id = ?",
        )
        .bind(mod_id.to_string())
        .execute(database.pool())
        .await?;

        assert!(service.uninstall(vec![mod_id]).await.is_err());
        assert!(
            paths
                .repository_directory
                .join("DISABLED_removal-fixture")
                .is_dir()
        );
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_mod_removals")
            .fetch_one(database.pool())
            .await?;
        assert_eq!(pending, 0);
        Ok(())
    }

    #[tokio::test]
    async fn recovers_precommit_and_postcommit_uninstall_interruptions()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let (service, _database, paths, mod_id) = removal_fixture(directory.path()).await?;
        let tombstone = Path::new(&format!("{REMOVAL_TOMBSTONE_PREFIX}{mod_id}")).to_path_buf();
        let _records = service
            .store
            .prepare_removals(&[(mod_id, tombstone.clone())])
            .await?;
        let root = service.repository_root().await?;
        root.quarantine_mod_root(
            &RepositoryRelativePath::new("DISABLED_removal-fixture")?,
            mod_id,
        )?;

        service.recover_pending_removals().await?;

        assert!(
            paths
                .repository_directory
                .join("DISABLED_removal-fixture")
                .is_dir()
        );
        assert!(!paths.repository_directory.join(&tombstone).exists());

        let records = service
            .store
            .prepare_removals(&[(mod_id, tombstone.clone())])
            .await?;
        root.quarantine_mod_root(
            &RepositoryRelativePath::new("DISABLED_removal-fixture")?,
            mod_id,
        )?;
        service.store.commit_removals(&records).await?;

        service.recover_pending_removals().await?;

        assert!(!paths.repository_directory.join(&tombstone).exists());
        assert!(!service.store.contains_mod(mod_id).await?);
        assert_eq!(records.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn refuses_repository_change_while_mods_are_installed()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let (service, _database, _paths, _mod_id) = removal_fixture(directory.path()).await?;
        let original = service.settings.get().await.storage;
        let requested = StorageSettings {
            repository_path: directory.path().join("new-repository"),
            staging_path: original.staging_path.clone(),
        };

        assert!(service.configure_storage(requested).await.is_err());
        assert_eq!(service.settings.get().await.storage, original);
        Ok(())
    }

    #[tokio::test]
    async fn switches_to_new_empty_storage_roots() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let paths = AppPaths::for_test(directory.path());
        paths.ensure_base_directories().await?;
        let settings = SettingsService::load_or_create(&paths).await?;
        let database = Database::connect(&paths.database_file).await?;
        let service = ModService::new(
            settings,
            &database,
            paths.repository_directory.clone(),
            paths.staging_directory.clone(),
            Arc::new(Mutex::new(())),
        );
        let requested = StorageSettings {
            repository_path: directory.path().join("new-repository"),
            staging_path: directory.path().join("new-staging"),
        };

        let updated = service.configure_storage(requested.clone()).await?;

        assert_eq!(
            updated.storage.repository_path,
            fs::canonicalize(&requested.repository_path)?
        );
        assert_eq!(
            updated.storage.staging_path,
            fs::canonicalize(&requested.staging_path)?
        );
        assert_eq!(service.settings.get().await.storage, updated.storage);
        Ok(())
    }
}
