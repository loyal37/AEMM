use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{
    core::mods::path_is_link_or_reparse_point,
    errors::AppError,
    models::{
        DeploymentContext, DeploymentEntry, DeploymentManifest, DeploymentPlan,
        DeploymentRevokeReceipt,
    },
    utils::validate_relative_path,
};

use super::ModDeploymentStrategy;

pub const EFMI_DIRECT_STRATEGY_ID: &str = "efmi.direct-folder.v1";
const DISABLED_PREFIX: &str = "DISABLED_";

#[derive(Debug, Clone)]
pub struct EfmiDirectDeploymentStrategy {
    mods_root: PathBuf,
}

impl EfmiDirectDeploymentStrategy {
    pub async fn open(efmi_root: PathBuf) -> Result<Self, AppError> {
        tokio::task::spawn_blocking(move || {
            let root = canonical_directory(&efmi_root, "EFMI 根目录")?;
            let mods_root = canonical_directory(&root.join("Mods"), "EFMI Mods 目录")?;
            if mods_root.parent() != Some(root.as_path())
                || path_is_link_or_reparse_point(&mods_root)?
            {
                return Err(AppError::UnsafePath(
                    "EFMI Mods 必须是加载器根目录中的非链接直属目录。".to_owned(),
                ));
            }
            Ok(Self { mods_root })
        })
        .await?
    }

    pub fn mods_root(&self) -> &Path {
        &self.mods_root
    }

    fn enabled_destination(&self, source: &Path) -> Result<PathBuf, AppError> {
        let name = direct_child_name(&self.mods_root, source)?;
        let enabled = strip_disabled_prefix(&name);
        if enabled.is_empty() || enabled == name {
            return Err(AppError::Deployment(format!(
                "模组目录 {name} 已经是启用状态。"
            )));
        }
        Ok(self.mods_root.join(enabled))
    }

    fn disabled_destination(&self, active: &Path) -> Result<PathBuf, AppError> {
        let name = direct_child_name(&self.mods_root, active)?;
        if is_disabled_name(&name) {
            return Err(AppError::Deployment(format!(
                "模组目录 {name} 已经是禁用状态。"
            )));
        }
        Ok(self.mods_root.join(format!("{DISABLED_PREFIX}{name}")))
    }
}

#[async_trait]
impl ModDeploymentStrategy for EfmiDirectDeploymentStrategy {
    fn strategy_id(&self) -> &'static str {
        EFMI_DIRECT_STRATEGY_ID
    }

    async fn plan_deploy(&self, context: &DeploymentContext) -> Result<DeploymentPlan, AppError> {
        let strategy = self.clone();
        let context = context.clone();
        tokio::task::spawn_blocking(move || {
            ensure_same_root(&strategy.mods_root, &context.repository_root)?;
            ensure_same_root(&strategy.mods_root, &context.destination_root)?;
            let destination = strategy.enabled_destination(&context.mod_root)?;
            ensure_destination_available(&destination)?;
            let entries = inventory(&context.mod_root)?;
            Ok(DeploymentPlan {
                operation_id: Uuid::new_v4(),
                profile_id: context.profile_id,
                mod_id: context.mod_id,
                strategy_id: EFMI_DIRECT_STRATEGY_ID.to_owned(),
                destination_directory: destination,
                source_content_fingerprint: context.source_content_fingerprint,
                entries,
                warnings: Vec::new(),
            })
        })
        .await?
    }

    async fn deploy(
        &self,
        context: &DeploymentContext,
        plan: DeploymentPlan,
    ) -> Result<DeploymentManifest, AppError> {
        let strategy = self.clone();
        let context = context.clone();
        tokio::task::spawn_blocking(move || {
            validate_plan(&strategy, &context, &plan)?;
            fs::rename(&context.mod_root, &plan.destination_directory)
                .map_err(|source| AppError::file_system(&context.mod_root, source))?;
            Ok(DeploymentManifest {
                schema_version: 1,
                id: plan.operation_id,
                profile_id: plan.profile_id,
                mod_id: plan.mod_id,
                strategy_id: plan.strategy_id,
                destination_root: strategy.mods_root,
                destination_directory: plan.destination_directory,
                source_content_fingerprint: plan.source_content_fingerprint,
                entries: plan.entries,
                created_at: unix_timestamp_seconds()?,
            })
        })
        .await?
    }

    async fn plan_revoke(&self, manifest: &DeploymentManifest) -> Result<DeploymentPlan, AppError> {
        self.verify(manifest).await?;
        Ok(DeploymentPlan {
            operation_id: Uuid::new_v4(),
            profile_id: manifest.profile_id,
            mod_id: manifest.mod_id,
            strategy_id: EFMI_DIRECT_STRATEGY_ID.to_owned(),
            destination_directory: self.disabled_destination(&manifest.destination_directory)?,
            source_content_fingerprint: manifest.source_content_fingerprint.clone(),
            entries: manifest.entries.clone(),
            warnings: Vec::new(),
        })
    }

    async fn begin_revoke(
        &self,
        manifest: &DeploymentManifest,
    ) -> Result<DeploymentRevokeReceipt, AppError> {
        self.verify(manifest).await?;
        let manifest = manifest.clone();
        let source = manifest.destination_directory.clone();
        let tombstone = self.disabled_destination(&source)?;
        ensure_destination_available(&tombstone)?;
        tokio::task::spawn_blocking(move || {
            fs::rename(&source, &tombstone)
                .map_err(|error| AppError::file_system(&source, error))?;
            Ok(DeploymentRevokeReceipt {
                manifest: manifest.clone(),
                tombstone_directory: tombstone,
            })
        })
        .await?
    }

    async fn finalize_revoke(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError> {
        let expected = self.disabled_destination(&receipt.manifest.destination_directory)?;
        if expected != receipt.tombstone_directory || !expected.is_dir() {
            return Err(AppError::DataIntegrity(
                "禁用后的 EFMI 模组目录不符合预期。".to_owned(),
            ));
        }
        Ok(())
    }

    async fn rollback_revoke(&self, receipt: &DeploymentRevokeReceipt) -> Result<(), AppError> {
        let source = receipt.tombstone_directory.clone();
        let destination = receipt.manifest.destination_directory.clone();
        tokio::task::spawn_blocking(move || {
            ensure_destination_available(&destination)?;
            fs::rename(&source, &destination).map_err(|error| AppError::file_system(&source, error))
        })
        .await?
    }

    async fn rollback_deploy(&self, manifest: &DeploymentManifest) -> Result<(), AppError> {
        let source = manifest.destination_directory.clone();
        let destination = self.disabled_destination(&source)?;
        tokio::task::spawn_blocking(move || {
            ensure_destination_available(&destination)?;
            fs::rename(&source, &destination).map_err(|error| AppError::file_system(&source, error))
        })
        .await?
    }

    async fn verify(&self, manifest: &DeploymentManifest) -> Result<(), AppError> {
        if manifest.strategy_id != EFMI_DIRECT_STRATEGY_ID {
            return Err(AppError::Deployment("不支持的模组状态策略。".to_owned()));
        }
        ensure_same_root(&self.mods_root, &manifest.destination_root)?;
        let name = direct_child_name(&self.mods_root, &manifest.destination_directory)?;
        if is_disabled_name(&name)
            || !manifest.destination_directory.is_dir()
            || path_is_link_or_reparse_point(&manifest.destination_directory)?
        {
            return Err(AppError::Deployment(format!(
                "已启用模组目录 {name} 不存在或不安全。"
            )));
        }
        Ok(())
    }
}

fn validate_plan(
    strategy: &EfmiDirectDeploymentStrategy,
    context: &DeploymentContext,
    plan: &DeploymentPlan,
) -> Result<(), AppError> {
    if plan.strategy_id != EFMI_DIRECT_STRATEGY_ID
        || plan.profile_id != context.profile_id
        || plan.mod_id != context.mod_id
        || plan.source_content_fingerprint != context.source_content_fingerprint
        || plan.destination_directory != strategy.enabled_destination(&context.mod_root)?
    {
        return Err(AppError::DataIntegrity(
            "EFMI 原地启用计划已失效。".to_owned(),
        ));
    }
    ensure_destination_available(&plan.destination_directory)
}

fn inventory(root: &Path) -> Result<Vec<DeploymentEntry>, AppError> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(|error| AppError::Deployment(error.to_string()))?;
        if entry.depth() == 0 || entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() || path_is_link_or_reparse_point(entry.path())? {
            return Err(AppError::UnsafePath(format!(
                "模组包含链接或非普通文件：{}",
                entry.path().display()
            )));
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| AppError::UnsafePath("无法解析模组文件的相对路径。".to_owned()))?;
        validate_relative_path(relative)?;
        let bytes =
            fs::read(entry.path()).map_err(|error| AppError::file_system(entry.path(), error))?;
        entries.push(DeploymentEntry {
            source_relative: relative.to_path_buf(),
            destination_relative: relative.to_path_buf(),
            size_bytes: u64::try_from(bytes.len())
                .map_err(|_| AppError::DataIntegrity("模组文件大小超出支持范围。".to_owned()))?,
            content_hash: blake3::hash(&bytes).to_hex().to_string(),
        });
    }
    Ok(entries)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    let canonical = fs::canonicalize(path).map_err(|error| AppError::file_system(path, error))?;
    if !canonical.is_dir() {
        return Err(AppError::NotAvailable(format!("{label}不存在或不是目录。")));
    }
    Ok(canonical)
}

fn ensure_same_root(expected: &Path, actual: &Path) -> Result<(), AppError> {
    let actual = canonical_directory(actual, "模组目录")?;
    if actual != expected {
        return Err(AppError::UnsafePath(
            "模组操作目录不是已验证的 EFMI Mods。".to_owned(),
        ));
    }
    Ok(())
}

fn direct_child_name(root: &Path, path: &Path) -> Result<String, AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::UnsafePath("模组目录缺少父目录。".to_owned()))?;
    if parent != root {
        return Err(AppError::UnsafePath(
            "模组必须是 EFMI Mods 的直属目录。".to_owned(),
        ));
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .map(str::to_owned)
        .ok_or_else(|| AppError::UnsafePath("模组目录名称无效。".to_owned()))
}

fn ensure_destination_available(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        Err(AppError::Deployment(format!(
            "目标目录 {} 已存在，为避免覆盖已取消操作。",
            path.display()
        )))
    } else {
        Ok(())
    }
}

fn is_disabled_name(name: &str) -> bool {
    name.get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("DISABLED"))
}

fn strip_disabled_prefix(name: &str) -> String {
    name.get(8..)
        .unwrap_or_default()
        .trim_start_matches(['_', '-', ' '])
        .to_owned()
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
    use std::fs;

    use uuid::Uuid;

    use crate::{
        core::deployment::{EfmiDirectDeploymentStrategy, ModDeploymentStrategy},
        models::DeploymentContext,
    };

    #[tokio::test]
    async fn enables_and_disables_by_atomic_directory_rename()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = tempfile::tempdir()?;
        let efmi_root = fixture.path().join("EFMI");
        let mods_root = efmi_root.join("Mods");
        let disabled = mods_root.join("DISABLED_example-mod");
        fs::create_dir_all(&disabled)?;
        fs::write(disabled.join("main.ini"), b"[TextureOverrideExample]\n")?;
        let strategy = EfmiDirectDeploymentStrategy::open(efmi_root).await?;
        let context = DeploymentContext {
            profile_id: Uuid::new_v4(),
            mod_id: Uuid::new_v4(),
            repository_root: strategy.mods_root().to_path_buf(),
            mod_root: fs::canonicalize(&disabled)?,
            destination_root: strategy.mods_root().to_path_buf(),
            source_content_fingerprint: "fixture".to_owned(),
            files: Vec::new(),
        };

        let plan = strategy.plan_deploy(&context).await?;
        let manifest = strategy.deploy(&context, plan).await?;
        assert!(mods_root.join("example-mod").is_dir());
        assert!(!disabled.exists());
        strategy.verify(&manifest).await?;

        let receipt = strategy.begin_revoke(&manifest).await?;
        strategy.finalize_revoke(&receipt).await?;
        assert!(disabled.is_dir());
        assert!(!mods_root.join("example-mod").exists());
        Ok(())
    }

    #[tokio::test]
    async fn refuses_to_overwrite_existing_enabled_directory()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = tempfile::tempdir()?;
        let efmi_root = fixture.path().join("EFMI");
        let mods_root = efmi_root.join("Mods");
        let disabled = mods_root.join("DISABLED_collision");
        fs::create_dir_all(&disabled)?;
        fs::create_dir(mods_root.join("collision"))?;
        let strategy = EfmiDirectDeploymentStrategy::open(efmi_root).await?;
        let context = DeploymentContext {
            profile_id: Uuid::new_v4(),
            mod_id: Uuid::new_v4(),
            repository_root: strategy.mods_root().to_path_buf(),
            mod_root: fs::canonicalize(disabled)?,
            destination_root: strategy.mods_root().to_path_buf(),
            source_content_fingerprint: "fixture".to_owned(),
            files: Vec::new(),
        };

        assert!(strategy.plan_deploy(&context).await.is_err());
        assert!(mods_root.join("collision").is_dir());
        Ok(())
    }

    #[test]
    fn disabled_prefix_is_case_insensitive_and_normalized() {
        assert_eq!(super::strip_disabled_prefix("disabled__My Mod"), "My Mod");
        assert!(super::is_disabled_name("DiSaBlEd-My Mod"));
    }
}
