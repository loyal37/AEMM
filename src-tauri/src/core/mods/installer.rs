use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{
    errors::AppError,
    models::{MetadataSourceKind, ModImportPlan, ModInstallProgressStage, ModLifecycleState},
    utils::validate_relative_path,
};

use super::{
    ExtractionPolicy, FileSystemModScanner, InstallProgressReporter, ModScanner, RepositoryRoot,
    ScannedMod, StagingOperation, StagingRoot, detect_mod_root, emit, stage_source,
};
use super::{RepositoryRelativePath, repository::path_is_link_or_reparse_point};

const INSTALL_JOURNAL_FILE: &str = ".aemm-install.json";
const INSTALL_JOURNAL_VERSION: u32 = 1;
const COPY_BUFFER_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone)]
pub struct ExistingModIdentity {
    pub logical_id: String,
    pub name: String,
    pub repository_path: PathBuf,
    pub content_fingerprint: Option<String>,
    pub lifecycle_state: ModLifecycleState,
}

#[derive(Debug, Clone)]
pub struct SafeModInstaller {
    repository: RepositoryRoot,
    staging: StagingRoot,
    scanner: FileSystemModScanner,
    extraction_policy: ExtractionPolicy,
}

#[derive(Debug, Clone)]
pub struct CommitReceipt {
    operation_id: Uuid,
    destination_relative: RepositoryRelativePath,
    destination_path: PathBuf,
    candidate_path: PathBuf,
    content_fingerprint: String,
    transfer_kind: TransferKind,
}

impl CommitReceipt {
    pub fn operation_id(&self) -> Uuid {
        self.operation_id
    }

    pub fn destination_relative(&self) -> &RepositoryRelativePath {
        &self.destination_relative
    }
}

#[derive(Debug, Clone)]
pub struct PendingInstall {
    pub operation_id: Uuid,
    pub state: InstallJournalState,
    pub destination_relative: PathBuf,
    pub content_fingerprint: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InstallJournalState {
    Prepared,
    Committing,
    RepositoryCommitted,
    DatabaseSynced,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TransferKind {
    Moved,
    Copied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallJournal {
    schema_version: u32,
    operation_id: Uuid,
    state: InstallJournalState,
    plan: ModImportPlan,
    source_stem: String,
    candidate_relative_path: PathBuf,
    transfer_kind: Option<TransferKind>,
}

impl SafeModInstaller {
    pub fn new(repository: RepositoryRoot, staging: StagingRoot) -> Self {
        Self {
            repository,
            staging,
            scanner: FileSystemModScanner::new(),
            extraction_policy: ExtractionPolicy::default(),
        }
    }

    #[cfg(test)]
    pub fn with_policy(mut self, policy: ExtractionPolicy) -> Self {
        self.extraction_policy = policy;
        self
    }

    pub async fn prepare(
        &self,
        source: PathBuf,
        existing: Vec<ExistingModIdentity>,
        progress: InstallProgressReporter,
    ) -> Result<ModImportPlan, AppError> {
        let operation_id = Uuid::new_v4();
        let staging = self.staging.clone();
        let repository = self.repository.clone();
        let policy = self.extraction_policy;
        let prepare_progress = progress.clone();
        let source_for_worker = source.clone();
        let staged = tokio::task::spawn_blocking(move || {
            validate_source_boundary(&source_for_worker, &repository, &staging)?;
            let operation = staging.create_operation(operation_id)?;
            let result: Result<_, AppError> = (|| {
                let staged = stage_source(
                    &source_for_worker,
                    &operation.payload_path(),
                    operation_id,
                    policy,
                    &prepare_progress,
                )?;
                let detected = detect_mod_root(&staged.staged_root)?;
                Ok((operation, staged, detected))
            })();
            if result.is_err() {
                cleanup_failed_prepare(&staging, operation_id);
            }
            result
        })
        .await??;
        let (operation, staged_source, detected_root) = staged;

        emit(
            &progress,
            operation_id,
            ModInstallProgressStage::Analyzing,
            "正在分析模组结构与元数据…",
            0,
            None,
            0,
            Some(staged_source.total_bytes),
        );
        let candidate_result = self.scanner.scan_candidate(&detected_root.path).await;
        let mut candidate = match candidate_result {
            Ok(candidate) => candidate,
            Err(error) => {
                cleanup_failed_prepare(&self.staging, operation_id);
                return Err(error);
            }
        };
        let source_stem = source_stem(&staged_source.source_name);
        normalize_inferred_metadata(&mut candidate, &source_stem);

        let plan_result = self.build_plan(
            &operation,
            &candidate,
            staged_source,
            detected_root.warnings,
            source_stem.clone(),
            &existing,
        );
        let plan = match plan_result {
            Ok(plan) => plan,
            Err(error) => {
                cleanup_failed_prepare(&self.staging, operation_id);
                return Err(error);
            }
        };
        let candidate_relative_path = detected_root
            .path
            .strip_prefix(operation.path())
            .map_err(|_| AppError::UnsafePath("检测到的模组根目录不属于当前安装操作。".to_owned()))?
            .to_path_buf();
        validate_relative_path(&candidate_relative_path)?;
        let journal = InstallJournal {
            schema_version: INSTALL_JOURNAL_VERSION,
            operation_id,
            state: InstallJournalState::Prepared,
            plan: plan.clone(),
            source_stem,
            candidate_relative_path,
            transfer_kind: None,
        };
        if let Err(error) = operation.write_json(INSTALL_JOURNAL_FILE, &journal) {
            cleanup_failed_prepare(&self.staging, operation_id);
            return Err(error);
        }
        emit(
            &progress,
            operation_id,
            ModInstallProgressStage::Ready,
            if plan.can_install {
                "安全检查完成，等待确认安装。"
            } else {
                "检查完成，但存在阻止安装的问题。"
            },
            plan.file_count,
            Some(plan.file_count),
            plan.size_bytes,
            Some(plan.size_bytes),
        );
        tracing::info!(
            operation_id = %operation_id,
            source_kind = ?plan.source_kind,
            logical_id = %plan.logical_id,
            files = plan.file_count,
            bytes = plan.size_bytes,
            can_install = plan.can_install,
            "mod import plan prepared"
        );
        Ok(plan)
    }

    pub async fn commit(
        &self,
        operation_id: Uuid,
        existing: Vec<ExistingModIdentity>,
        progress: InstallProgressReporter,
    ) -> Result<CommitReceipt, AppError> {
        let operation = self.staging.operation(operation_id)?;
        let mut journal = load_journal(&operation)?;
        validate_journal(&journal, operation_id)?;
        if journal.state != InstallJournalState::Prepared {
            return Err(AppError::ModInstall(
                "安装操作不处于可提交状态，请重新导入。".to_owned(),
            ));
        }
        if !journal.plan.can_install || !journal.plan.blocking_issues.is_empty() {
            return Err(AppError::ModInstall(
                "安装计划包含阻止项，不能提交。".to_owned(),
            ));
        }
        let candidate_path = resolve_operation_candidate(&operation, &journal)?;
        let mut candidate = self.scanner.scan_candidate(&candidate_path).await?;
        normalize_inferred_metadata(&mut candidate, &journal.source_stem);
        verify_candidate_matches_plan(&candidate, &journal.plan)?;
        let duplicate_issues = duplicate_blocking_issues(&candidate, &existing);
        if !duplicate_issues.is_empty() {
            return Err(AppError::ModInstall(format!(
                "提交前发现新的重复模组：{}",
                duplicate_issues.join("；")
            )));
        }

        let destination_relative =
            RepositoryRelativePath::new(journal.plan.destination_relative_path.clone())?;
        let destination_path = self.repository.planned_mod_path(&destination_relative)?;
        if destination_path.exists() {
            return Err(AppError::ModInstall(
                "安装目标已存在；AEMM 不会覆盖任何现有模组。".to_owned(),
            ));
        }
        journal.state = InstallJournalState::Committing;
        operation.write_json(INSTALL_JOURNAL_FILE, &journal)?;
        emit(
            &progress,
            operation_id,
            ModInstallProgressStage::Committing,
            "正在原子提交到模组仓库…",
            0,
            Some(candidate.files.len().try_into().unwrap_or(u64::MAX)),
            0,
            Some(candidate.size_bytes),
        );

        let candidate_for_move = candidate_path.clone();
        let destination_for_move = destination_path.clone();
        let transfer_attempt = tokio::task::spawn_blocking(move || {
            match fs::rename(&candidate_for_move, &destination_for_move) {
                Ok(()) => Ok(TransferAttempt::Moved),
                Err(error) if is_cross_volume_error(&error) => Ok(TransferAttempt::CrossVolume),
                Err(error) => Err(AppError::file_system(destination_for_move, error)),
            }
        })
        .await??;

        let transfer_kind = match transfer_attempt {
            TransferAttempt::Moved => TransferKind::Moved,
            TransferAttempt::CrossVolume => {
                let partial_relative = partial_relative_path(operation_id)?;
                let partial_path = self.repository.planned_mod_path(&partial_relative)?;
                if partial_path.exists() {
                    return Err(AppError::ModInstall(
                        "检测到同一安装操作遗留的仓库临时目录，请重启 AEMM 进行恢复。".to_owned(),
                    ));
                }
                let source = candidate_path.clone();
                let partial = partial_path.clone();
                if let Err(error) =
                    tokio::task::spawn_blocking(move || copy_directory_new(&source, &partial))
                        .await?
                {
                    cleanup_repository_partial(&self.repository, &partial_relative);
                    return Err(error);
                }
                let partial_scan = self.scanner.scan_candidate(&partial_path).await?;
                if partial_scan.content_fingerprint != candidate.content_fingerprint {
                    cleanup_repository_partial(&self.repository, &partial_relative);
                    return Err(AppError::ModInstall(
                        "跨磁盘复制后的内容校验失败，安装已回滚。".to_owned(),
                    ));
                }
                let partial = partial_path.clone();
                let destination = destination_path.clone();
                if let Err(error) = tokio::task::spawn_blocking(move || {
                    fs::rename(&partial, &destination)
                        .map_err(|source| AppError::file_system(&destination, source))
                })
                .await?
                {
                    cleanup_repository_partial(&self.repository, &partial_relative);
                    return Err(error);
                }
                TransferKind::Copied
            }
        };

        let receipt = CommitReceipt {
            operation_id,
            destination_relative,
            destination_path,
            candidate_path,
            content_fingerprint: candidate.content_fingerprint.clone(),
            transfer_kind,
        };
        journal.state = InstallJournalState::RepositoryCommitted;
        journal.transfer_kind = Some(transfer_kind);
        if let Err(error) = operation.write_json(INSTALL_JOURNAL_FILE, &journal) {
            if let Err(rollback_error) = self.rollback_receipt(&receipt, &progress).await {
                tracing::error!(operation_id = %operation_id, error = %rollback_error, "journal write and installation rollback both failed");
            }
            return Err(error);
        }
        emit(
            &progress,
            operation_id,
            ModInstallProgressStage::Synchronizing,
            "文件已提交，正在同步模组数据库…",
            u64::try_from(candidate.files.len()).unwrap_or(u64::MAX),
            Some(u64::try_from(candidate.files.len()).unwrap_or(u64::MAX)),
            candidate.size_bytes,
            Some(candidate.size_bytes),
        );
        Ok(receipt)
    }

    pub fn mark_database_synced(&self, receipt: &CommitReceipt) -> Result<(), AppError> {
        let operation = self.staging.operation(receipt.operation_id)?;
        let mut journal = load_journal(&operation)?;
        validate_journal(&journal, receipt.operation_id)?;
        if journal.state != InstallJournalState::RepositoryCommitted {
            return Err(AppError::DataIntegrity(
                "安装日志未处于仓库已提交状态。".to_owned(),
            ));
        }
        journal.state = InstallJournalState::DatabaseSynced;
        operation.write_json(INSTALL_JOURNAL_FILE, &journal)
    }

    pub fn finalize(&self, receipt: &CommitReceipt) -> Result<(), AppError> {
        self.staging.remove_operation(receipt.operation_id)
    }

    pub async fn rollback_receipt(
        &self,
        receipt: &CommitReceipt,
        progress: &InstallProgressReporter,
    ) -> Result<(), AppError> {
        emit(
            progress,
            receipt.operation_id,
            ModInstallProgressStage::RollingBack,
            "安装未完成，正在安全回滚…",
            0,
            None,
            0,
            None,
        );
        if receipt.destination_path.exists() {
            let destination_scan = self
                .scanner
                .scan_candidate(&receipt.destination_path)
                .await?;
            if destination_scan.content_fingerprint != receipt.content_fingerprint {
                return Err(AppError::ModInstall(
                    "安装目标在回滚前被外部修改，AEMM 已停止删除以保护用户文件。".to_owned(),
                ));
            }
            match receipt.transfer_kind {
                TransferKind::Moved if !receipt.candidate_path.exists() => {
                    let source = receipt.destination_path.clone();
                    let destination = receipt.candidate_path.clone();
                    tokio::task::spawn_blocking(move || {
                        fs::rename(&source, &destination)
                            .map_err(|error| AppError::file_system(&source, error))
                    })
                    .await??;
                }
                TransferKind::Moved | TransferKind::Copied => {
                    let repository = self.repository.clone();
                    let relative = receipt.destination_relative.clone();
                    tokio::task::spawn_blocking(move || repository.remove_mod_root(&relative))
                        .await??;
                }
            }
        }
        let staging = self.staging.clone();
        let operation_id = receipt.operation_id;
        tokio::task::spawn_blocking(move || staging.remove_operation(operation_id)).await??;
        tracing::warn!(operation_id = %receipt.operation_id, "mod installation rolled back");
        Ok(())
    }

    pub fn cancel(&self, operation_id: Uuid) -> Result<(), AppError> {
        let operation = self.staging.operation(operation_id)?;
        let journal = load_journal(&operation)?;
        validate_journal(&journal, operation_id)?;
        if journal.state != InstallJournalState::Prepared {
            return Err(AppError::ModInstall(
                "只能取消尚未提交的安装计划。".to_owned(),
            ));
        }
        self.staging.remove_operation(operation_id)?;
        tracing::info!(operation_id = %operation_id, "prepared mod import cancelled");
        Ok(())
    }

    pub fn pending_installs(&self) -> Result<Vec<PendingInstall>, AppError> {
        let mut pending = Vec::new();
        for operation_id in self.staging.operation_ids()? {
            let operation = self.staging.operation(operation_id)?;
            match load_journal(&operation) {
                Ok(journal) => {
                    validate_journal(&journal, operation_id)?;
                    pending.push(PendingInstall {
                        operation_id,
                        state: journal.state,
                        destination_relative: journal.plan.destination_relative_path,
                        content_fingerprint: journal.plan.content_fingerprint,
                    });
                }
                Err(error) => {
                    tracing::error!(operation_id = %operation_id, error = %error, "invalid installation journal requires manual inspection")
                }
            }
        }
        Ok(pending)
    }

    pub async fn recover_operation(
        &self,
        operation_id: Uuid,
        database_has_committed_mod: bool,
    ) -> Result<(), AppError> {
        let operation = self.staging.operation(operation_id)?;
        let journal = load_journal(&operation)?;
        validate_journal(&journal, operation_id)?;
        if journal.state == InstallJournalState::Prepared {
            let staging = self.staging.clone();
            tokio::task::spawn_blocking(move || staging.remove_operation(operation_id)).await??;
            tracing::info!(operation_id = %operation_id, "removed abandoned prepared import");
            return Ok(());
        }
        if journal.state == InstallJournalState::DatabaseSynced
            || (journal.state == InstallJournalState::RepositoryCommitted
                && database_has_committed_mod)
        {
            let staging = self.staging.clone();
            tokio::task::spawn_blocking(move || staging.remove_operation(operation_id)).await??;
            tracing::info!(operation_id = %operation_id, "completed interrupted installation cleanup");
            return Ok(());
        }
        self.rollback_pending(&operation, &journal).await
    }

    fn build_plan(
        &self,
        operation: &StagingOperation,
        candidate: &ScannedMod,
        staged_source: super::StagedSource,
        mut warnings: Vec<String>,
        _source_stem: String,
        existing: &[ExistingModIdentity],
    ) -> Result<ModImportPlan, AppError> {
        warnings.extend(staged_source.warnings);
        warnings.extend(candidate.issues.iter().map(|issue| issue.display_message()));
        if staged_source.entry_count > u64::try_from(candidate.files.len()).unwrap_or(u64::MAX) {
            warnings.push(format!(
                "包内共检查 {} 个条目，最终模组根目录包含 {} 个文件。",
                staged_source.entry_count,
                candidate.files.len()
            ));
        }
        let mut blocking_issues = duplicate_blocking_issues(candidate, existing);
        if candidate.is_broken() {
            blocking_issues.push("模组扫描发现无法安全读取的文件。".to_owned());
        }
        if existing.iter().any(|identity| {
            identity
                .name
                .eq_ignore_ascii_case(&candidate.author_metadata.name)
                && !identity
                    .logical_id
                    .eq_ignore_ascii_case(&candidate.author_metadata.logical_id)
        }) {
            warnings
                .push("仓库中已有同显示名称但不同 ID 的模组，请确认它们并非同一模组。".to_owned());
        }
        deduplicate_messages(&mut warnings);
        deduplicate_messages(&mut blocking_issues);
        let destination_relative_path = choose_destination_relative(
            &self.repository,
            &candidate.author_metadata.logical_id,
            &candidate.content_fingerprint,
        )?;
        let file_count = u64::try_from(candidate.files.len())
            .map_err(|_| AppError::DataIntegrity("模组文件数量超过支持范围。".to_owned()))?;
        let can_install = blocking_issues.is_empty();
        Ok(ModImportPlan {
            operation_id: operation.operation_id(),
            source_kind: staged_source.source_kind,
            source_name: staged_source.source_name,
            logical_id: candidate.author_metadata.logical_id.clone(),
            name: candidate.author_metadata.name.clone(),
            author: candidate.author_metadata.author.clone(),
            version: candidate.author_metadata.version.clone(),
            description: candidate.author_metadata.description.clone(),
            category: candidate.author_metadata.category.clone(),
            file_count,
            size_bytes: candidate.size_bytes,
            content_fingerprint: candidate.content_fingerprint.clone(),
            destination_relative_path,
            warnings,
            blocking_issues,
            can_install,
        })
    }

    async fn rollback_pending(
        &self,
        operation: &StagingOperation,
        journal: &InstallJournal,
    ) -> Result<(), AppError> {
        let destination_relative =
            RepositoryRelativePath::new(journal.plan.destination_relative_path.clone())?;
        let destination = self.repository.planned_mod_path(&destination_relative)?;
        let candidate = resolve_operation_candidate(operation, journal)?;
        if destination.exists() {
            verify_repository_fingerprint(
                &self.scanner,
                &destination,
                &journal.plan.content_fingerprint,
            )
            .await?;
            if candidate.exists() {
                let repository = self.repository.clone();
                tokio::task::spawn_blocking(move || {
                    repository.remove_mod_root(&destination_relative)
                })
                .await??;
            } else {
                let source = destination.clone();
                let target = candidate.clone();
                tokio::task::spawn_blocking(move || {
                    fs::rename(&source, &target)
                        .map_err(|error| AppError::file_system(&source, error))
                })
                .await??;
            }
        }
        let partial_relative = partial_relative_path(journal.operation_id)?;
        let partial = self.repository.planned_mod_path(&partial_relative)?;
        if partial.exists() {
            verify_repository_fingerprint(
                &self.scanner,
                &partial,
                &journal.plan.content_fingerprint,
            )
            .await?;
            let repository = self.repository.clone();
            tokio::task::spawn_blocking(move || repository.remove_mod_root(&partial_relative))
                .await??;
        }
        let staging = self.staging.clone();
        let operation_id = journal.operation_id;
        tokio::task::spawn_blocking(move || staging.remove_operation(operation_id)).await??;
        tracing::warn!(operation_id = %journal.operation_id, "recovered and rolled back interrupted installation");
        Ok(())
    }
}

enum TransferAttempt {
    Moved,
    CrossVolume,
}

fn validate_source_boundary(
    source: &Path,
    repository: &RepositoryRoot,
    staging: &StagingRoot,
) -> Result<(), AppError> {
    let source = fs::canonicalize(source).map_err(|error| AppError::file_system(source, error))?;
    for (label, managed_root) in [
        ("模组仓库", repository.path()),
        ("安装暂存目录", staging.path()),
    ] {
        if source == managed_root
            || source.starts_with(managed_root)
            || managed_root.starts_with(&source)
        {
            return Err(AppError::UnsafePath(format!(
                "导入源不能与{label}相同、位于其中或包含它。"
            )));
        }
    }
    Ok(())
}

fn normalize_inferred_metadata(candidate: &mut ScannedMod, source_stem: &str) {
    if candidate.author_metadata.source_kind != MetadataSourceKind::Inferred {
        return;
    }
    let fingerprint_prefix = candidate
        .content_fingerprint
        .get(..20)
        .unwrap_or(candidate.content_fingerprint.as_str());
    candidate.author_metadata.logical_id = format!("local.{fingerprint_prefix}");
    candidate.author_metadata.name = source_stem.to_owned();
}

fn source_stem(source_name: &str) -> String {
    let value = Path::new(source_name)
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Imported Mod".to_owned());
    value.trim().chars().take(512).collect()
}

fn duplicate_blocking_issues(
    candidate: &ScannedMod,
    existing: &[ExistingModIdentity],
) -> Vec<String> {
    let mut issues = Vec::new();
    for identity in existing {
        if identity
            .logical_id
            .eq_ignore_ascii_case(&candidate.author_metadata.logical_id)
        {
            issues.push(format!(
                "已存在相同模组 ID：{}（状态：{:?}）。",
                identity.logical_id, identity.lifecycle_state
            ));
        }
        if identity.content_fingerprint.as_deref() == Some(&candidate.content_fingerprint) {
            issues.push(format!("仓库中的 {} 与待安装内容完全相同。", identity.name));
        }
    }
    issues
}

fn choose_destination_relative(
    repository: &RepositoryRoot,
    logical_id: &str,
    fingerprint: &str,
) -> Result<PathBuf, AppError> {
    let base = safe_destination_name(logical_id);
    let occupied = fs::read_dir(repository.path())
        .map_err(|source| AppError::file_system(repository.path(), source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| AppError::file_system(repository.path(), source))?
        .into_iter()
        .map(|entry| entry.file_name().to_string_lossy().to_lowercase())
        .collect::<HashSet<_>>();
    if !occupied.contains(&base.to_lowercase()) {
        return Ok(PathBuf::from(base));
    }
    let fingerprint_prefix = fingerprint.get(..8).unwrap_or(fingerprint);
    let suffixed = format!("{base}-{fingerprint_prefix}");
    if !occupied.contains(&suffixed.to_lowercase()) {
        return Ok(PathBuf::from(suffixed));
    }
    for index in 2..=999_u16 {
        let candidate = format!("{suffixed}-{index}");
        if !occupied.contains(&candidate.to_lowercase()) {
            return Ok(PathBuf::from(candidate));
        }
    }
    Err(AppError::ModInstall(
        "无法为模组生成唯一的仓库目录名称。".to_owned(),
    ))
}

fn safe_destination_name(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    result = result.trim().trim_end_matches(['.', ' ']).to_owned();
    if result.encode_utf16().count() > 96 {
        result = result.chars().take(80).collect();
    }
    if result.is_empty() || validate_relative_path(Path::new(&result)).is_err() {
        let hash = blake3::hash(value.as_bytes()).to_hex();
        result = format!("mod-{}", &hash[..16]);
    }
    result
}

fn load_journal(operation: &StagingOperation) -> Result<InstallJournal, AppError> {
    operation.read_json(INSTALL_JOURNAL_FILE)
}

fn validate_journal(journal: &InstallJournal, operation_id: Uuid) -> Result<(), AppError> {
    if journal.schema_version != INSTALL_JOURNAL_VERSION
        || journal.operation_id != operation_id
        || journal.plan.operation_id != operation_id
    {
        return Err(AppError::DataIntegrity(
            "安装操作日志版本或 ID 不一致。".to_owned(),
        ));
    }
    let destination = RepositoryRelativePath::new(journal.plan.destination_relative_path.clone())?;
    if !destination.is_direct_child() {
        return Err(AppError::UnsafePath(
            "安装日志中的仓库目标不是直属模组目录。".to_owned(),
        ));
    }
    validate_relative_path(&journal.candidate_relative_path)
}

fn resolve_operation_candidate(
    operation: &StagingOperation,
    journal: &InstallJournal,
) -> Result<PathBuf, AppError> {
    validate_relative_path(&journal.candidate_relative_path)?;
    let candidate = operation.path().join(&journal.candidate_relative_path);
    if !candidate.exists() {
        return Ok(candidate);
    }
    if path_is_link_or_reparse_point(&candidate)? {
        return Err(AppError::UnsafePath(
            "安装候选目录不能是链接或重解析点。".to_owned(),
        ));
    }
    let canonical =
        fs::canonicalize(&candidate).map_err(|source| AppError::file_system(&candidate, source))?;
    if canonical == operation.path() || !canonical.starts_with(operation.path()) {
        return Err(AppError::UnsafePath(
            "安装候选目录解析到了当前操作之外。".to_owned(),
        ));
    }
    Ok(canonical)
}

fn verify_candidate_matches_plan(
    candidate: &ScannedMod,
    plan: &ModImportPlan,
) -> Result<(), AppError> {
    let file_count = u64::try_from(candidate.files.len()).unwrap_or(u64::MAX);
    if candidate.content_fingerprint != plan.content_fingerprint
        || !candidate
            .author_metadata
            .logical_id
            .eq_ignore_ascii_case(&plan.logical_id)
        || candidate.size_bytes != plan.size_bytes
        || file_count != plan.file_count
    {
        return Err(AppError::ModInstall(
            "暂存内容在确认前发生变化，已拒绝提交；请重新导入。".to_owned(),
        ));
    }
    Ok(())
}

fn copy_directory_new(source: &Path, destination: &Path) -> Result<(), AppError> {
    if path_is_link_or_reparse_point(source)? {
        return Err(AppError::UnsafePath(
            "跨磁盘复制源不能是链接或重解析点。".to_owned(),
        ));
    }
    fs::create_dir(destination).map_err(|error| AppError::file_system(destination, error))?;
    let result = (|| {
        for entry in WalkDir::new(source).follow_links(false).sort_by_file_name() {
            let entry = entry
                .map_err(|error| AppError::ModInstall(format!("无法遍历跨磁盘复制源：{error}")))?;
            if entry.depth() == 0 {
                continue;
            }
            if path_is_link_or_reparse_point(entry.path())? {
                return Err(AppError::UnsafePath(format!(
                    "跨磁盘复制源包含链接或重解析点：{}。",
                    entry.path().display()
                )));
            }
            let relative = entry.path().strip_prefix(source).map_err(|_| {
                AppError::UnsafePath("跨磁盘复制路径无法转换为相对路径。".to_owned())
            })?;
            validate_relative_path(relative)?;
            let target = destination.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir(&target).map_err(|source| AppError::file_system(&target, source))?;
            } else if entry.file_type().is_file() {
                let metadata = entry
                    .metadata()
                    .map_err(|error| AppError::ModInstall(error.to_string()))?;
                let mut input = File::open(entry.path())
                    .map_err(|source| AppError::file_system(entry.path(), source))?;
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(|source| AppError::file_system(&target, source))?;
                copy_exact(&mut input, &mut output, metadata.len(), &target)?;
            } else {
                return Err(AppError::UnsafePath(
                    "跨磁盘复制源包含非普通文件。".to_owned(),
                ));
            }
        }
        Ok(())
    })();
    if result.is_err() {
        tracing::warn!(partial = %destination.display(), "cross-volume copy failed; partial will be removed by caller");
    }
    result
}

fn copy_exact(
    input: &mut File,
    output: &mut File,
    expected: u64,
    target: &Path,
) -> Result<(), AppError> {
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut copied = 0_u64;
    while copied < expected {
        let limit = usize::try_from(expected - copied)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = input
            .read(&mut buffer[..limit])
            .map_err(|source| AppError::file_system(target, source))?;
        if read == 0 {
            return Err(AppError::ModInstall(
                "复制源文件在操作期间缩短。".to_owned(),
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|source| AppError::file_system(target, source))?;
        copied += u64::try_from(read).unwrap_or(u64::MAX);
    }
    let mut extra = [0_u8; 1];
    if input
        .read(&mut extra)
        .map_err(|source| AppError::file_system(target, source))?
        != 0
    {
        return Err(AppError::ModInstall(
            "复制源文件在操作期间增长。".to_owned(),
        ));
    }
    output
        .sync_all()
        .map_err(|source| AppError::file_system(target, source))
}

fn partial_relative_path(operation_id: Uuid) -> Result<RepositoryRelativePath, AppError> {
    RepositoryRelativePath::new(format!(".aemm-install-{operation_id}.partial"))
}

fn is_cross_volume_error(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::CrossesDevices || error.raw_os_error() == Some(17)
}

fn cleanup_failed_prepare(staging: &StagingRoot, operation_id: Uuid) {
    if let Err(error) = staging.remove_operation(operation_id) {
        tracing::error!(operation_id = %operation_id, error = %error, "failed to clean unsuccessful mod import staging operation");
    }
}

fn cleanup_repository_partial(repository: &RepositoryRoot, relative: &RepositoryRelativePath) {
    if let Ok(path) = repository.planned_mod_path(relative)
        && path.exists()
        && let Err(error) = repository.remove_mod_root(relative)
    {
        tracing::error!(partial = %path.display(), error = %error, "failed to remove repository installation partial");
    }
}

async fn verify_repository_fingerprint(
    scanner: &FileSystemModScanner,
    path: &Path,
    expected: &str,
) -> Result<(), AppError> {
    let scanned = scanner.scan_candidate(path).await?;
    if scanned.content_fingerprint != expected {
        return Err(AppError::ModInstall(
            "恢复前发现安装目录内容已被外部修改，AEMM 已停止删除。".to_owned(),
        ));
    }
    Ok(())
}

fn deduplicate_messages(messages: &mut Vec<String>) {
    let mut seen = HashSet::new();
    messages.retain(|message| seen.insert(message.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use crate::core::mods::{RepositoryInitializationPolicy, StagingInitializationPolicy};

    use super::*;

    fn no_progress() -> InstallProgressReporter {
        Arc::new(|_| {})
    }

    fn installer(root: &Path) -> Result<SafeModInstaller, AppError> {
        let repository_path = root.join("repository");
        let staging_path = root.join("staging");
        fs::create_dir(&repository_path)
            .map_err(|error| AppError::file_system(&repository_path, error))?;
        fs::create_dir(&staging_path)
            .map_err(|error| AppError::file_system(&staging_path, error))?;
        let repository = RepositoryRoot::open_or_initialize(
            &repository_path,
            RepositoryInitializationPolicy::EmptyOnly,
        )?;
        let staging =
            StagingRoot::open_or_initialize(&staging_path, StagingInitializationPolicy::EmptyOnly)?;
        Ok(SafeModInstaller::new(repository, staging))
    }

    #[tokio::test]
    async fn prepares_commits_and_finalizes_folder_install()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let installer = installer(directory.path())?;
        let source = directory.path().join("Example Mod");
        fs::create_dir(&source)?;
        fs::write(
            source.join("mod.json"),
            br#"{"id":"author.example","name":"Example"}"#,
        )?;
        fs::write(source.join("content.ini"), b"content")?;

        let plan = installer.prepare(source, Vec::new(), no_progress()).await?;
        assert!(plan.can_install);
        let receipt = installer
            .commit(plan.operation_id, Vec::new(), no_progress())
            .await?;
        assert!(
            installer
                .repository
                .planned_mod_path(receipt.destination_relative())?
                .is_dir()
        );
        installer.mark_database_synced(&receipt)?;
        installer.finalize(&receipt)?;
        assert!(installer.staging.operation_ids()?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn blocks_duplicate_logical_id_without_overwriting()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let installer = installer(directory.path())?;
        let source = directory.path().join("Duplicate");
        fs::create_dir(&source)?;
        fs::write(
            source.join("mod.json"),
            br#"{"id":"author.example","name":"Example"}"#,
        )?;
        fs::write(source.join("content.ini"), b"new")?;
        let existing = vec![ExistingModIdentity {
            logical_id: "author.example".to_owned(),
            name: "Existing".to_owned(),
            repository_path: PathBuf::from("existing"),
            content_fingerprint: Some("other".to_owned()),
            lifecycle_state: ModLifecycleState::Installed,
        }];

        let plan = installer.prepare(source, existing, no_progress()).await?;
        assert!(!plan.can_install);
        assert!(!plan.blocking_issues.is_empty());
        assert!(
            installer
                .commit(plan.operation_id, Vec::new(), no_progress())
                .await
                .is_err()
        );
        installer.cancel(plan.operation_id)?;
        Ok(())
    }

    #[tokio::test]
    async fn rollback_removes_new_repository_entry_and_keeps_root()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let installer = installer(directory.path())?;
        let source = directory.path().join("Rollback");
        fs::create_dir(&source)?;
        fs::write(source.join("content.ini"), b"content")?;
        let plan = installer.prepare(source, Vec::new(), no_progress()).await?;
        let receipt = installer
            .commit(plan.operation_id, Vec::new(), no_progress())
            .await?;
        installer.rollback_receipt(&receipt, &no_progress()).await?;
        assert!(installer.repository.path().exists());
        assert!(
            !installer
                .repository
                .planned_mod_path(receipt.destination_relative())?
                .exists()
        );
        Ok(())
    }

    #[tokio::test]
    async fn startup_recovery_rolls_back_repository_commit_without_database_record()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let installer = installer(directory.path())?;
        let source = directory.path().join("Interrupted");
        fs::create_dir(&source)?;
        fs::write(source.join("content.ini"), b"content")?;
        let plan = installer.prepare(source, Vec::new(), no_progress()).await?;
        let receipt = installer
            .commit(plan.operation_id, Vec::new(), no_progress())
            .await?;
        let destination = installer
            .repository
            .planned_mod_path(receipt.destination_relative())?;
        assert!(destination.exists());

        installer
            .recover_operation(plan.operation_id, false)
            .await?;
        assert!(!destination.exists());
        assert!(installer.staging.operation_ids()?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn startup_recovery_preserves_database_committed_mod()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let installer = installer(directory.path())?;
        let source = directory.path().join("Committed");
        fs::create_dir(&source)?;
        fs::write(source.join("content.ini"), b"content")?;
        let plan = installer.prepare(source, Vec::new(), no_progress()).await?;
        let receipt = installer
            .commit(plan.operation_id, Vec::new(), no_progress())
            .await?;
        let destination = installer
            .repository
            .planned_mod_path(receipt.destination_relative())?;

        installer.recover_operation(plan.operation_id, true).await?;
        assert!(destination.exists());
        assert!(installer.staging.operation_ids()?.is_empty());
        Ok(())
    }
}
