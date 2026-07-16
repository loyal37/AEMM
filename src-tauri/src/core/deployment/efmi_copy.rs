use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{
    core::mods::{FileSystemModScanner, ModScanner, path_is_link_or_reparse_point},
    errors::AppError,
    models::{
        DeploymentContext, DeploymentEntry, DeploymentManifest, DeploymentPlan,
        DeploymentRevokeReceipt,
    },
    utils::validate_relative_path,
};

use super::ModDeploymentStrategy;

pub const EFMI_COPY_STRATEGY_ID: &str = "efmi.copy.v1";
const MANIFEST_SCHEMA_VERSION: u32 = 1;
const DEPLOYMENT_MARKER_FILE: &str = ".aemm-deployment.json";
const DEPLOYMENT_MARKER_KIND: &str = "aemm-efmi-deployment";
const ACTIVE_PREFIX: &str = "AEMM_";
const PENDING_PREFIX: &str = "DISABLED_AEMM_PENDING_";
const REVOKE_PREFIX: &str = "DISABLED_AEMM_REVOKE_";
const MAX_INI_BYTES: u64 = 1024 * 1024;
const MAX_MARKER_BYTES: u64 = 16 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone)]
pub struct EfmiCopyDeploymentStrategy {
    mods_root: PathBuf,
    scanner: FileSystemModScanner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeploymentMarker {
    kind: String,
    schema_version: u32,
    manifest: DeploymentManifest,
}

impl EfmiCopyDeploymentStrategy {
    pub async fn open(efmi_root: PathBuf) -> Result<Self, AppError> {
        let (_, mods_root) =
            tokio::task::spawn_blocking(move || validate_efmi_deployment_root(&efmi_root))
                .await??;
        Ok(Self {
            mods_root,
            scanner: FileSystemModScanner::new(),
        })
    }

    pub fn mods_root(&self) -> &Path {
        &self.mods_root
    }

    pub async fn remove_orphaned_directory(
        &self,
        directory_name: PathBuf,
        expected_manifest: DeploymentManifest,
        allow_partial: bool,
    ) -> Result<(), AppError> {
        let strategy = self.clone();
        tokio::task::spawn_blocking(move || {
            strategy.remove_owned_directory(&directory_name, &expected_manifest, allow_partial)
        })
        .await?
    }

    pub async fn owned_directories(&self) -> Result<Vec<(PathBuf, DeploymentManifest)>, AppError> {
        let strategy = self.clone();
        tokio::task::spawn_blocking(move || strategy.read_owned_directories()).await?
    }

    fn deploy_sync(
        &self,
        context: &DeploymentContext,
        plan: &DeploymentPlan,
    ) -> Result<DeploymentManifest, AppError> {
        self.validate_context_root(context)?;
        validate_plan(plan, context)?;

        let active_directory = direct_child_name(&plan.destination_directory)?;
        let active_path = self.mods_root.join(&active_directory);
        if active_path.exists() {
            return Err(AppError::ModInstall(format!(
                "EFMI 部署目录 {} 已存在；AEMM 不会覆盖现有内容。",
                active_directory.display()
            )));
        }

        let pending_directory = PathBuf::from(format!(
            "{PENDING_PREFIX}{}.partial",
            plan.operation_id.simple()
        ));
        let pending_path = self.mods_root.join(&pending_directory);
        if pending_path.exists() {
            return Err(AppError::DataIntegrity(
                "本次部署的隔离临时目录已存在，请先执行恢复。".to_owned(),
            ));
        }
        fs::create_dir(&pending_path)
            .map_err(|source| AppError::file_system(&pending_path, source))?;

        let manifest = DeploymentManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            profile_id: context.profile_id,
            mod_id: context.mod_id,
            strategy_id: EFMI_COPY_STRATEGY_ID.to_owned(),
            destination_root: self.mods_root.clone(),
            destination_directory: active_directory,
            source_content_fingerprint: plan.source_content_fingerprint.clone(),
            entries: plan.entries.clone(),
            created_at: unix_timestamp_seconds()?,
        };

        let deploy_result = (|| {
            write_marker_new(&pending_path, &manifest)?;
            for entry in &manifest.entries {
                copy_deployment_entry(&context.mod_root, &pending_path, entry)?;
            }
            validate_owned_inventory(&pending_path, &manifest, false)?;
            if active_path.exists() {
                return Err(AppError::ModInstall(
                    "部署提交前目标目录被其他操作占用；已取消且不会覆盖。".to_owned(),
                ));
            }
            fs::rename(&pending_path, &active_path)
                .map_err(|source| AppError::file_system(&active_path, source))?;
            validate_owned_inventory(&active_path, &manifest, false)?;
            Ok(manifest.clone())
        })();

        if deploy_result.is_err() {
            let cleanup_result = if pending_path.exists() {
                self.remove_owned_directory(&pending_directory, &manifest, true)
            } else if active_path.exists() {
                self.quarantine_active(&manifest)
                    .and_then(|receipt| self.finalize_revoke_sync(&receipt))
            } else {
                Ok(())
            };
            if let Err(cleanup_error) = cleanup_result {
                tracing::error!(
                    pending_path = %pending_path.display(),
                    active_path = %active_path.display(),
                    error = %cleanup_error,
                    "failed to clean interrupted EFMI deployment; recovery will retry"
                );
            }
        }
        deploy_result
    }

    fn begin_revoke_sync(
        &self,
        manifest: &DeploymentManifest,
    ) -> Result<DeploymentRevokeReceipt, AppError> {
        self.validate_manifest_root(manifest)?;
        let active_directory = direct_child_name(&manifest.destination_directory)?;
        let active_path = self.mods_root.join(&active_directory);
        validate_owned_inventory(&active_path, manifest, false)?;

        let operation_id = Uuid::new_v4();
        let tombstone_directory = PathBuf::from(format!(
            "{REVOKE_PREFIX}{}_{}.revoke",
            manifest.mod_id.simple(),
            operation_id.simple()
        ));
        let tombstone_path = self.mods_root.join(&tombstone_directory);
        if tombstone_path.exists() {
            return Err(AppError::DataIntegrity(
                "撤销隔离目录发生不可接受的名称碰撞。".to_owned(),
            ));
        }
        fs::rename(&active_path, &tombstone_path)
            .map_err(|source| AppError::file_system(&tombstone_path, source))?;
        if let Err(error) = validate_owned_inventory(&tombstone_path, manifest, false) {
            if !active_path.exists() {
                if let Err(rollback_error) = fs::rename(&tombstone_path, &active_path) {
                    tracing::error!(
                        path = %tombstone_path.display(),
                        error = %rollback_error,
                        "failed to restore deployment after revoke verification failure"
                    );
                }
            }
            return Err(error);
        }
        Ok(DeploymentRevokeReceipt {
            manifest: manifest.clone(),
            tombstone_directory,
        })
    }

    fn finalize_revoke_sync(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError> {
        let tombstone = direct_child_name(&receipt.tombstone_directory)?;
        let tombstone_name = tombstone.to_string_lossy();
        if !tombstone_name.starts_with(REVOKE_PREFIX) {
            return Err(AppError::UnsafePath(
                "拒绝清理不符合 AEMM 撤销命名约定的目录。".to_owned(),
            ));
        }
        self.remove_owned_directory(&tombstone, &receipt.manifest, false)
    }

    fn rollback_revoke_sync(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError> {
        self.validate_manifest_root(&receipt.manifest)?;
        let tombstone = direct_child_name(&receipt.tombstone_directory)?;
        let tombstone_path = self.mods_root.join(&tombstone);
        validate_owned_inventory(&tombstone_path, &receipt.manifest, false)?;
        let active = direct_child_name(&receipt.manifest.destination_directory)?;
        let active_path = self.mods_root.join(active);
        if active_path.exists() {
            return Err(AppError::UnsafePath(
                "无法回滚撤销：原部署目录已被占用，未覆盖任何内容。".to_owned(),
            ));
        }
        fs::rename(&tombstone_path, &active_path)
            .map_err(|source| AppError::file_system(&active_path, source))?;
        validate_owned_inventory(&active_path, &receipt.manifest, false)
    }

    fn rollback_deploy_sync(&self, manifest: &DeploymentManifest) -> Result<(), AppError> {
        self.validate_manifest_root(manifest)?;
        let receipt = self.begin_revoke_sync(manifest)?;
        self.finalize_revoke_sync(&receipt)
    }

    fn verify_sync(&self, manifest: &DeploymentManifest) -> Result<(), AppError> {
        self.validate_manifest_root(manifest)?;
        let directory = direct_child_name(&manifest.destination_directory)?;
        validate_owned_inventory(&self.mods_root.join(directory), manifest, false)
    }

    fn validate_context_root(&self, context: &DeploymentContext) -> Result<(), AppError> {
        let configured = fs::canonicalize(&context.destination_root)
            .map_err(|source| AppError::file_system(&context.destination_root, source))?;
        if !paths_equal(&configured, &self.mods_root) {
            return Err(AppError::UnsafePath(
                "部署计划的 EFMI Mods 根目录与当前已验证目录不一致。".to_owned(),
            ));
        }
        let repository = fs::canonicalize(&context.repository_root)
            .map_err(|source| AppError::file_system(&context.repository_root, source))?;
        let mod_root = fs::canonicalize(&context.mod_root)
            .map_err(|source| AppError::file_system(&context.mod_root, source))?;
        if mod_root.parent() != Some(repository.as_path())
            || path_is_link_or_reparse_point(&mod_root)?
        {
            return Err(AppError::UnsafePath(
                "待部署模组必须是 AEMM 仓库中不含重解析点的直属目录。".to_owned(),
            ));
        }
        Ok(())
    }

    fn validate_manifest_root(&self, manifest: &DeploymentManifest) -> Result<(), AppError> {
        validate_manifest_shape(manifest)?;
        let stored_root = fs::canonicalize(&manifest.destination_root)
            .map_err(|source| AppError::file_system(&manifest.destination_root, source))?;
        if !paths_equal(&stored_root, &self.mods_root) {
            return Err(AppError::UnsafePath(
                "部署清单不属于当前已验证的 EFMI Mods 根目录。".to_owned(),
            ));
        }
        Ok(())
    }

    fn quarantine_active(
        &self,
        manifest: &DeploymentManifest,
    ) -> Result<DeploymentRevokeReceipt, AppError> {
        self.validate_manifest_root(manifest)?;
        let active = direct_child_name(&manifest.destination_directory)?;
        let active_path = self.mods_root.join(active);
        let marker = read_marker(&active_path.join(DEPLOYMENT_MARKER_FILE))?;
        if marker.manifest != *manifest {
            return Err(AppError::UnsafePath(
                "无法隔离失败部署：所有权清单不一致。".to_owned(),
            ));
        }
        let tombstone_directory = PathBuf::from(format!(
            "{REVOKE_PREFIX}{}_{}.revoke",
            manifest.mod_id.simple(),
            Uuid::new_v4().simple()
        ));
        let tombstone_path = self.mods_root.join(&tombstone_directory);
        if tombstone_path.exists() {
            return Err(AppError::DataIntegrity(
                "失败部署隔离目录发生不可接受的名称碰撞。".to_owned(),
            ));
        }
        fs::rename(&active_path, &tombstone_path)
            .map_err(|source| AppError::file_system(&tombstone_path, source))?;
        Ok(DeploymentRevokeReceipt {
            manifest: manifest.clone(),
            tombstone_directory,
        })
    }

    fn remove_owned_directory(
        &self,
        relative: &Path,
        manifest: &DeploymentManifest,
        allow_partial: bool,
    ) -> Result<(), AppError> {
        self.validate_manifest_root(manifest)?;
        let relative = direct_child_name(relative)?;
        let name = relative.to_string_lossy();
        let active = direct_child_name(&manifest.destination_directory)?;
        let recognized_operation_name =
            name.starts_with(PENDING_PREFIX) || name.starts_with(REVOKE_PREFIX);
        if (name.starts_with(ACTIVE_PREFIX) && relative != active)
            || (!name.starts_with(ACTIVE_PREFIX) && !recognized_operation_name)
        {
            return Err(AppError::UnsafePath(
                "部署清理目录名与清单所有权范围不一致。".to_owned(),
            ));
        }
        let root = self.mods_root.join(&relative);
        validate_owned_inventory(&root, manifest, allow_partial)?;
        remove_manifest_tree(&root, manifest, allow_partial)
    }

    fn read_owned_directories(&self) -> Result<Vec<(PathBuf, DeploymentManifest)>, AppError> {
        let mut owned = Vec::new();
        for entry in fs::read_dir(&self.mods_root)
            .map_err(|source| AppError::file_system(&self.mods_root, source))?
        {
            let entry = entry.map_err(|source| AppError::file_system(&self.mods_root, source))?;
            let file_type = entry
                .file_type()
                .map_err(|source| AppError::file_system(entry.path(), source))?;
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let name = PathBuf::from(entry.file_name());
            let name_text = name.to_string_lossy();
            if !(name_text.starts_with(ACTIVE_PREFIX)
                || name_text.starts_with(PENDING_PREFIX)
                || name_text.starts_with(REVOKE_PREFIX))
            {
                continue;
            }
            let marker_path = entry.path().join(DEPLOYMENT_MARKER_FILE);
            let Ok(marker) = read_marker(&marker_path) else {
                tracing::warn!(path = %entry.path().display(), "AEMM-like EFMI directory has no valid ownership marker and was preserved");
                continue;
            };
            owned.push((name, marker.manifest));
        }
        Ok(owned)
    }
}

#[async_trait]
impl ModDeploymentStrategy for EfmiCopyDeploymentStrategy {
    fn strategy_id(&self) -> &'static str {
        EFMI_COPY_STRATEGY_ID
    }

    async fn plan_deploy(&self, context: &DeploymentContext) -> Result<DeploymentPlan, AppError> {
        self.validate_context_root(context)?;
        let scanned = self.scanner.scan_candidate(&context.mod_root).await?;
        if scanned.is_broken() {
            return Err(AppError::ModInstall(
                "模组当前文件快照不完整，不能安全部署。".to_owned(),
            ));
        }
        if scanned.content_fingerprint != context.source_content_fingerprint {
            return Err(AppError::DataIntegrity(
                "模组仓库内容已在扫描后发生变化；请先重新扫描再启用。".to_owned(),
            ));
        }
        if !scanned.files.iter().any(|file| {
            file.source_path
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case("ini"))
        }) {
            return Err(AppError::ModInstall(
                "该目录没有 EFMI 可递归加载的 INI 文件，已拒绝启用。".to_owned(),
            ));
        }

        let mut entries = Vec::with_capacity(scanned.files.len());
        for file in &scanned.files {
            validate_relative_path(&file.source_path)?;
            if file
                .source_path
                .components()
                .any(|component| component.as_os_str() == DEPLOYMENT_MARKER_FILE)
            {
                return Err(AppError::UnsafePath(format!(
                    "模组包含 AEMM 保留文件名 {DEPLOYMENT_MARKER_FILE}，不能部署。"
                )));
            }
            let content_hash = file.content_hash.clone().ok_or_else(|| {
                AppError::DataIntegrity(format!(
                    "模组文件 {} 缺少内容 Hash，请重新扫描。",
                    file.source_path.display()
                ))
            })?;
            entries.push(DeploymentEntry {
                source_relative: file.source_path.clone(),
                destination_relative: file.source_path.clone(),
                size_bytes: file.size_bytes,
                content_hash,
            });
        }
        entries.sort_by(|left, right| {
            normalized_relative_key(&left.destination_relative)
                .cmp(&normalized_relative_key(&right.destination_relative))
        });
        let destination_directory =
            PathBuf::from(format!("{ACTIVE_PREFIX}{}", context.mod_id.simple()));
        if self.mods_root.join(&destination_directory).exists() {
            return Err(AppError::ModInstall(
                "该模组的 EFMI 部署目录已经存在；请先执行状态恢复或禁用。".to_owned(),
            ));
        }
        Ok(DeploymentPlan {
            operation_id: Uuid::new_v4(),
            profile_id: context.profile_id,
            mod_id: context.mod_id,
            strategy_id: EFMI_COPY_STRATEGY_ID.to_owned(),
            destination_directory,
            source_content_fingerprint: context.source_content_fingerprint.clone(),
            entries,
            warnings: vec![
                "若游戏正在运行，请先让被修改角色离开画面，再按 F10 让 EFMI 重新加载。".to_owned(),
            ],
        })
    }

    async fn deploy(
        &self,
        context: &DeploymentContext,
        plan: DeploymentPlan,
    ) -> Result<DeploymentManifest, AppError> {
        let strategy = self.clone();
        let context = context.clone();
        tokio::task::spawn_blocking(move || strategy.deploy_sync(&context, &plan)).await?
    }

    async fn plan_revoke(&self, manifest: &DeploymentManifest) -> Result<DeploymentPlan, AppError> {
        self.verify(manifest).await?;
        Ok(DeploymentPlan {
            operation_id: Uuid::new_v4(),
            profile_id: manifest.profile_id,
            mod_id: manifest.mod_id,
            strategy_id: manifest.strategy_id.clone(),
            destination_directory: manifest.destination_directory.clone(),
            source_content_fingerprint: manifest.source_content_fingerprint.clone(),
            entries: manifest.entries.clone(),
            warnings: vec![
                "撤销会先原子重命名为 EFMI 的 DISABLED* 目录，数据库提交后才清理。".to_owned(),
            ],
        })
    }

    async fn begin_revoke(
        &self,
        manifest: &DeploymentManifest,
    ) -> Result<DeploymentRevokeReceipt, AppError> {
        let strategy = self.clone();
        let manifest = manifest.clone();
        tokio::task::spawn_blocking(move || strategy.begin_revoke_sync(&manifest)).await?
    }

    async fn finalize_revoke(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError> {
        let strategy = self.clone();
        let receipt = receipt.clone();
        tokio::task::spawn_blocking(move || strategy.finalize_revoke_sync(&receipt)).await?
    }

    async fn rollback_revoke(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError> {
        let strategy = self.clone();
        let receipt = receipt.clone();
        tokio::task::spawn_blocking(move || strategy.rollback_revoke_sync(&receipt)).await?
    }

    async fn rollback_deploy(&self, manifest: &DeploymentManifest) -> Result<(), AppError> {
        let strategy = self.clone();
        let manifest = manifest.clone();
        tokio::task::spawn_blocking(move || strategy.rollback_deploy_sync(&manifest)).await?
    }

    async fn verify(&self, manifest: &DeploymentManifest) -> Result<(), AppError> {
        let strategy = self.clone();
        let manifest = manifest.clone();
        tokio::task::spawn_blocking(move || strategy.verify_sync(&manifest)).await?
    }
}

fn validate_efmi_deployment_root(efmi_root: &Path) -> Result<(PathBuf, PathBuf), AppError> {
    if !efmi_root.is_absolute() || efmi_root.as_os_str().is_empty() {
        return Err(AppError::UnsafePath(
            "EFMI 部署根目录必须是非空绝对路径。".to_owned(),
        ));
    }
    if path_is_link_or_reparse_point(efmi_root)? {
        return Err(AppError::UnsafePath(
            "EFMI 根目录不能是链接、目录联接或其他重解析点。".to_owned(),
        ));
    }
    let efmi_root =
        fs::canonicalize(efmi_root).map_err(|source| AppError::file_system(efmi_root, source))?;
    if !efmi_root.is_dir() {
        return Err(AppError::UnsafePath("EFMI 根路径不是目录。".to_owned()));
    }
    let mods_candidate = efmi_root.join("Mods");
    if path_is_link_or_reparse_point(&mods_candidate)? {
        return Err(AppError::UnsafePath(
            "EFMI Mods 目录不能是链接、目录联接或其他重解析点。".to_owned(),
        ));
    }
    let mods_root = fs::canonicalize(&mods_candidate)
        .map_err(|source| AppError::file_system(&mods_candidate, source))?;
    if !mods_root.is_dir() || mods_root.parent() != Some(efmi_root.as_path()) {
        return Err(AppError::UnsafePath(
            "EFMI Mods 必须是加载器根目录中的直属目录。".to_owned(),
        ));
    }
    validate_efmi_include_policy(&efmi_root.join("d3dx.ini"))?;
    Ok((efmi_root, mods_root))
}

fn validate_efmi_include_policy(path: &Path) -> Result<(), AppError> {
    if path_is_link_or_reparse_point(path)? {
        return Err(AppError::UnsafePath(
            "EFMI d3dx.ini 不能是链接或重解析点。".to_owned(),
        ));
    }
    let metadata = fs::metadata(path).map_err(|source| AppError::file_system(path, source))?;
    if !metadata.is_file() || metadata.len() > MAX_INI_BYTES {
        return Err(AppError::LoaderValidation(
            "EFMI d3dx.ini 缺失、类型错误或尺寸异常。".to_owned(),
        ));
    }
    let contents =
        fs::read_to_string(path).map_err(|source| AppError::file_system(path, source))?;
    let mut section = String::new();
    let mut includes_mods = false;
    let mut excludes_disabled = false;
    for line in contents.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            section = name.trim().to_owned();
            continue;
        }
        if !section.eq_ignore_ascii_case("Include") {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        if key.eq_ignore_ascii_case("include_recursive") && value.eq_ignore_ascii_case("Mods") {
            includes_mods = true;
        }
        if key.eq_ignore_ascii_case("exclude_recursive") && value.eq_ignore_ascii_case("DISABLED*")
        {
            excludes_disabled = true;
        }
    }
    if !includes_mods || !excludes_disabled {
        return Err(AppError::LoaderValidation(
            "当前 EFMI 配置未同时声明 include_recursive = Mods 与 exclude_recursive = DISABLED*；无法保证原子启停。"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_plan(plan: &DeploymentPlan, context: &DeploymentContext) -> Result<(), AppError> {
    if plan.strategy_id != EFMI_COPY_STRATEGY_ID
        || plan.profile_id != context.profile_id
        || plan.mod_id != context.mod_id
        || plan.source_content_fingerprint != context.source_content_fingerprint
    {
        return Err(AppError::DataIntegrity(
            "部署计划与当前模组上下文不一致。".to_owned(),
        ));
    }
    direct_child_name(&plan.destination_directory)?;
    if plan.entries.is_empty() {
        return Err(AppError::ModInstall("空模组不能部署。".to_owned()));
    }
    validate_entries(&plan.entries)
}

fn validate_manifest_shape(manifest: &DeploymentManifest) -> Result<(), AppError> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION
        || manifest.strategy_id != EFMI_COPY_STRATEGY_ID
    {
        return Err(AppError::DataIntegrity(
            "部署清单版本或策略不受当前实现支持。".to_owned(),
        ));
    }
    direct_child_name(&manifest.destination_directory)?;
    validate_entries(&manifest.entries)
}

fn validate_entries(entries: &[DeploymentEntry]) -> Result<(), AppError> {
    let mut destinations = HashSet::new();
    for entry in entries {
        validate_relative_path(&entry.source_relative)?;
        validate_relative_path(&entry.destination_relative)?;
        if entry.content_hash.len() != 64
            || !entry
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(AppError::DataIntegrity(format!(
                "部署条目 {} 的 BLAKE3 Hash 无效。",
                entry.destination_relative.display()
            )));
        }
        let key = normalized_relative_key(&entry.destination_relative);
        if !destinations.insert(key) {
            return Err(AppError::DataIntegrity(
                "部署清单包含大小写不敏感的重复目标。".to_owned(),
            ));
        }
    }
    Ok(())
}

fn copy_deployment_entry(
    source_root: &Path,
    destination_root: &Path,
    entry: &DeploymentEntry,
) -> Result<(), AppError> {
    let source = resolve_safe_source(source_root, &entry.source_relative)?;
    let destination = destination_root.join(&entry.destination_relative);
    create_safe_parent_directories(destination_root, &entry.destination_relative)?;
    let mut input =
        File::open(&source).map_err(|source_error| AppError::file_system(&source, source_error))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|source_error| AppError::file_system(&destination, source_error))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut copied = 0_u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|source_error| AppError::file_system(&source, source_error))?;
        if read == 0 {
            break;
        }
        copied =
            copied
                .checked_add(u64::try_from(read).map_err(|_| {
                    AppError::DataIntegrity("部署读取尺寸超出支持范围。".to_owned())
                })?)
                .ok_or_else(|| AppError::DataIntegrity("部署文件尺寸溢出。".to_owned()))?;
        if copied > entry.size_bytes {
            return Err(AppError::DataIntegrity(format!(
                "源文件 {} 在部署期间发生变化。",
                entry.source_relative.display()
            )));
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|source_error| AppError::file_system(&destination, source_error))?;
    }
    output
        .sync_all()
        .map_err(|source_error| AppError::file_system(&destination, source_error))?;
    if copied != entry.size_bytes || hasher.finalize().to_hex().as_str() != entry.content_hash {
        return Err(AppError::DataIntegrity(format!(
            "源文件 {} 的尺寸或 Hash 与部署计划不一致。",
            entry.source_relative.display()
        )));
    }
    Ok(())
}

fn resolve_safe_source(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    validate_relative_path(relative)?;
    let root = fs::canonicalize(root).map_err(|source| AppError::file_system(root, source))?;
    let mut current = root.clone();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(AppError::UnsafePath(
                "部署源路径包含不安全组件。".to_owned(),
            ));
        }
        current.push(component.as_os_str());
        if path_is_link_or_reparse_point(&current)? {
            return Err(AppError::UnsafePath(format!(
                "部署源 {} 包含链接或重解析点。",
                relative.display()
            )));
        }
    }
    let canonical =
        fs::canonicalize(&current).map_err(|source| AppError::file_system(&current, source))?;
    if !canonical.is_file() || !canonical.starts_with(&root) || canonical == root {
        return Err(AppError::UnsafePath(
            "部署源文件解析到了模组根目录之外。".to_owned(),
        ));
    }
    Ok(canonical)
}

fn create_safe_parent_directories(root: &Path, relative_file: &Path) -> Result<(), AppError> {
    let Some(parent) = relative_file.parent() else {
        return Ok(());
    };
    let mut current = root.to_path_buf();
    for component in parent.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(AppError::UnsafePath(
                "部署目标父路径包含不安全组件。".to_owned(),
            ));
        }
        current.push(component.as_os_str());
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(source) if source.kind() == ErrorKind::AlreadyExists => {
                if path_is_link_or_reparse_point(&current)? || !current.is_dir() {
                    return Err(AppError::UnsafePath(format!(
                        "部署目标父路径 {} 不是安全目录。",
                        current.display()
                    )));
                }
            }
            Err(source) => return Err(AppError::file_system(&current, source)),
        }
    }
    Ok(())
}

fn write_marker_new(root: &Path, manifest: &DeploymentManifest) -> Result<(), AppError> {
    let marker_path = root.join(DEPLOYMENT_MARKER_FILE);
    let marker = DeploymentMarker {
        kind: DEPLOYMENT_MARKER_KIND.to_owned(),
        schema_version: MANIFEST_SCHEMA_VERSION,
        manifest: manifest.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&marker).map_err(AppError::ConfigFormat)?;
    bytes.push(b'\n');
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_MARKER_BYTES {
        return Err(AppError::DataIntegrity(
            "部署清单超过安全尺寸限制。".to_owned(),
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker_path)
        .map_err(|source| AppError::file_system(&marker_path, source))?;
    file.write_all(&bytes)
        .map_err(|source| AppError::file_system(&marker_path, source))?;
    file.sync_all()
        .map_err(|source| AppError::file_system(&marker_path, source))
}

fn read_marker(path: &Path) -> Result<DeploymentMarker, AppError> {
    if path_is_link_or_reparse_point(path)? {
        return Err(AppError::UnsafePath(
            "部署所有权清单不能是链接或重解析点。".to_owned(),
        ));
    }
    let metadata = fs::metadata(path).map_err(|source| AppError::file_system(path, source))?;
    if !metadata.is_file() || metadata.len() > MAX_MARKER_BYTES {
        return Err(AppError::UnsafePath(
            "部署所有权清单缺失、类型错误或尺寸异常。".to_owned(),
        ));
    }
    let marker: DeploymentMarker = serde_json::from_reader(
        File::open(path).map_err(|source| AppError::file_system(path, source))?,
    )
    .map_err(AppError::ConfigFormat)?;
    if marker.kind != DEPLOYMENT_MARKER_KIND || marker.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(AppError::UnsafePath(
            "部署所有权清单类型或版本无效。".to_owned(),
        ));
    }
    validate_manifest_shape(&marker.manifest)?;
    Ok(marker)
}

fn validate_owned_inventory(
    root: &Path,
    manifest: &DeploymentManifest,
    allow_partial: bool,
) -> Result<(), AppError> {
    if !root.is_dir() || path_is_link_or_reparse_point(root)? {
        return Err(AppError::UnsafePath(
            "部署目录缺失或已变成链接/重解析点。".to_owned(),
        ));
    }
    let marker = read_marker(&root.join(DEPLOYMENT_MARKER_FILE))?;
    if marker.manifest != *manifest {
        return Err(AppError::UnsafePath(
            "部署目录的所有权清单与数据库记录不一致；拒绝修改。".to_owned(),
        ));
    }

    let expected_files = manifest
        .entries
        .iter()
        .map(|entry| (normalized_relative_key(&entry.destination_relative), entry))
        .collect::<HashMap<_, _>>();
    let mut expected_directories = HashSet::new();
    for entry in &manifest.entries {
        let mut parent = entry.destination_relative.parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() {
                break;
            }
            expected_directories.insert(normalized_relative_key(path));
            parent = path.parent();
        }
    }
    let mut seen = HashSet::new();
    for item in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        let item =
            item.map_err(|error| AppError::UnsafePath(format!("无法安全遍历部署目录：{error}")))?;
        if item.depth() == 0 {
            continue;
        }
        if path_is_link_or_reparse_point(item.path())? {
            return Err(AppError::UnsafePath(format!(
                "部署目录包含链接或重解析点 {}，拒绝修改。",
                item.path().display()
            )));
        }
        let relative = item
            .path()
            .strip_prefix(root)
            .map_err(|_| AppError::UnsafePath("部署条目无法转换为安全相对路径。".to_owned()))?;
        validate_relative_path(relative)?;
        if relative == Path::new(DEPLOYMENT_MARKER_FILE) {
            continue;
        }
        let key = normalized_relative_key(relative);
        if item.file_type().is_dir() {
            if !expected_directories.contains(&key) {
                return Err(AppError::UnsafePath(format!(
                    "部署目录包含清单之外的目录 {}，拒绝删除。",
                    relative.display()
                )));
            }
            continue;
        }
        if !item.file_type().is_file() {
            return Err(AppError::UnsafePath("部署目录包含非普通文件。".to_owned()));
        }
        let Some(expected) = expected_files.get(&key) else {
            return Err(AppError::UnsafePath(format!(
                "部署目录包含清单之外的文件 {}，拒绝删除。",
                relative.display()
            )));
        };
        seen.insert(key);
        if !allow_partial {
            verify_deployed_file(item.path(), expected)?;
        }
    }
    if !allow_partial && seen.len() != expected_files.len() {
        return Err(AppError::DataIntegrity(
            "部署目录缺少清单中的文件。".to_owned(),
        ));
    }
    Ok(())
}

fn verify_deployed_file(path: &Path, entry: &DeploymentEntry) -> Result<(), AppError> {
    let metadata = fs::metadata(path).map_err(|source| AppError::file_system(path, source))?;
    if metadata.len() != entry.size_bytes {
        return Err(AppError::DataIntegrity(format!(
            "部署文件 {} 的尺寸已变化；拒绝自动删除。",
            entry.destination_relative.display()
        )));
    }
    let mut file = File::open(path).map_err(|source| AppError::file_system(path, source))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| AppError::file_system(path, source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if hasher.finalize().to_hex().as_str() != entry.content_hash {
        return Err(AppError::DataIntegrity(format!(
            "部署文件 {} 已被外部修改；拒绝自动删除。",
            entry.destination_relative.display()
        )));
    }
    Ok(())
}

fn remove_manifest_tree(
    root: &Path,
    manifest: &DeploymentManifest,
    allow_partial: bool,
) -> Result<(), AppError> {
    let mut directories = HashSet::new();
    for entry in &manifest.entries {
        let path = root.join(&entry.destination_relative);
        if path.exists() {
            if path_is_link_or_reparse_point(&path)? || !path.is_file() {
                return Err(AppError::UnsafePath(format!(
                    "部署清理目标 {} 已变成非普通文件。",
                    entry.destination_relative.display()
                )));
            }
            if !allow_partial {
                verify_deployed_file(&path, entry)?;
            }
            fs::remove_file(&path).map_err(|source| AppError::file_system(&path, source))?;
        }
        let mut parent = entry.destination_relative.parent();
        while let Some(relative_parent) = parent {
            if relative_parent.as_os_str().is_empty() {
                break;
            }
            directories.insert(relative_parent.to_path_buf());
            parent = relative_parent.parent();
        }
    }

    let marker_path = root.join(DEPLOYMENT_MARKER_FILE);
    let marker = read_marker(&marker_path)?;
    if marker.manifest != *manifest {
        return Err(AppError::UnsafePath(
            "部署清理前所有权清单发生变化；已中止。".to_owned(),
        ));
    }
    fs::remove_file(&marker_path).map_err(|source| AppError::file_system(&marker_path, source))?;

    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for relative in directories {
        let path = root.join(relative);
        if path.exists() {
            if path_is_link_or_reparse_point(&path)? || !path.is_dir() {
                return Err(AppError::UnsafePath(
                    "部署清理期间父目录类型发生变化；已中止。".to_owned(),
                ));
            }
            fs::remove_dir(&path).map_err(|source| AppError::file_system(&path, source))?;
        }
    }
    fs::remove_dir(root).map_err(|source| AppError::file_system(root, source))
}

fn direct_child_name(path: &Path) -> Result<PathBuf, AppError> {
    validate_relative_path(path)?;
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(AppError::UnsafePath(
            "EFMI 部署目录必须是 Mods 中的直属相对目录。".to_owned(),
        ));
    }
    Ok(path.to_path_buf())
}

fn normalized_relative_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(right.to_string_lossy().trim_end_matches(['\\', '/']))
}

fn unix_timestamp_seconds() -> Result<i64, AppError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::DataIntegrity("系统时间早于 Unix Epoch。".to_owned()))?;
    i64::try_from(duration.as_secs())
        .map_err(|_| AppError::DataIntegrity("系统时间超出支持范围。".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use uuid::Uuid;

    use crate::{
        core::{
            deployment::{EfmiCopyDeploymentStrategy, ModDeploymentStrategy},
            mods::{FileSystemModScanner, ModScanner},
        },
        models::DeploymentContext,
    };

    fn create_loader(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(root.join("Mods"))?;
        fs::write(
            root.join("d3dx.ini"),
            "[Include]\ninclude_recursive = Mods\nexclude_recursive = DISABLED*\n",
        )?;
        Ok(())
    }

    async fn context(
        repository: &Path,
        mods_root: &Path,
        mod_id: Uuid,
    ) -> Result<DeploymentContext, Box<dyn std::error::Error>> {
        let mod_root = repository.join("fixture-mod");
        let scanned = FileSystemModScanner::new()
            .scan_candidate(&mod_root)
            .await?;
        Ok(DeploymentContext {
            profile_id: Uuid::new_v4(),
            mod_id,
            repository_root: repository.to_path_buf(),
            mod_root,
            destination_root: mods_root.to_path_buf(),
            source_content_fingerprint: scanned.content_fingerprint,
            files: scanned.files,
        })
    }

    #[tokio::test]
    async fn deploys_verifies_and_transactionally_revokes_efmi_mod()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = tempfile::tempdir()?;
        let repository = workspace.path().join("repository");
        let mod_root = repository.join("fixture-mod");
        fs::create_dir_all(mod_root.join("Textures"))?;
        fs::write(mod_root.join("main.ini"), b"[TextureOverrideFixture]\n")?;
        fs::write(mod_root.join("Textures/body.dds"), b"texture")?;
        let loader = workspace.path().join("EFMI");
        create_loader(&loader)?;
        let strategy = EfmiCopyDeploymentStrategy::open(loader.clone()).await?;
        let mod_id = Uuid::new_v4();
        let context = context(&repository, strategy.mods_root(), mod_id).await?;

        let plan = strategy.plan_deploy(&context).await?;
        assert!(
            plan.destination_directory
                .to_string_lossy()
                .starts_with("AEMM_")
        );
        let manifest = strategy.deploy(&context, plan).await?;
        strategy.verify(&manifest).await?;
        assert!(
            strategy
                .mods_root()
                .join(&manifest.destination_directory)
                .join("main.ini")
                .is_file()
        );

        let receipt = strategy.begin_revoke(&manifest).await?;
        assert!(
            !strategy
                .mods_root()
                .join(&manifest.destination_directory)
                .exists()
        );
        strategy.rollback_revoke(&receipt).await?;
        strategy.verify(&manifest).await?;

        let receipt = strategy.begin_revoke(&manifest).await?;
        strategy.finalize_revoke(&receipt).await?;
        assert!(
            !strategy
                .mods_root()
                .join(&manifest.destination_directory)
                .exists()
        );
        Ok(())
    }

    #[tokio::test]
    async fn refuses_to_revoke_modified_or_unowned_content()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = tempfile::tempdir()?;
        let repository = workspace.path().join("repository");
        let mod_root = repository.join("fixture-mod");
        fs::create_dir_all(&mod_root)?;
        fs::write(mod_root.join("main.ini"), b"[TextureOverrideFixture]\n")?;
        let loader = workspace.path().join("EFMI");
        create_loader(&loader)?;
        let strategy = EfmiCopyDeploymentStrategy::open(loader).await?;
        let context = context(&repository, strategy.mods_root(), Uuid::new_v4()).await?;
        let plan = strategy.plan_deploy(&context).await?;
        let manifest = strategy.deploy(&context, plan).await?;
        let deployed = strategy.mods_root().join(&manifest.destination_directory);

        fs::write(deployed.join("main.ini"), b"user changed this")?;
        assert!(strategy.begin_revoke(&manifest).await.is_err());
        assert!(deployed.exists());

        fs::write(deployed.join("unowned.txt"), b"preserve me")?;
        assert!(strategy.rollback_deploy(&manifest).await.is_err());
        assert!(deployed.join("unowned.txt").exists());
        Ok(())
    }

    #[tokio::test]
    async fn requires_verified_efmi_include_and_disabled_prefix_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let loader = tempfile::tempdir()?;
        fs::create_dir(loader.path().join("Mods"))?;
        fs::write(
            loader.path().join("d3dx.ini"),
            "[Include]\ninclude_recursive = Mods\n",
        )?;
        assert!(
            EfmiCopyDeploymentStrategy::open(loader.path().to_path_buf())
                .await
                .is_err()
        );
        Ok(())
    }
}
