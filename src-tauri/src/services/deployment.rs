use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    core::{
        deployment::{EFMI_COPY_STRATEGY_ID, EfmiCopyDeploymentStrategy, ModDeploymentStrategy},
        mods::{RepositoryInitializationPolicy, RepositoryRelativePath, RepositoryRoot},
    },
    database::{Database, DeploymentStore},
    errors::AppError,
    models::{
        DeploymentContext, DeploymentManifest, DeploymentRevokeReceipt,
        ModDeploymentMutationResult, ModLifecycleState,
    },
};

use super::SettingsService;

const MAX_BATCH_DEPLOYMENTS: usize = 256;
const LOADER_REFRESH_GUIDANCE: &str =
    "如果游戏正在运行：先让被修改角色离开画面，再按 F10 让 EFMI 重新加载。";

#[derive(Debug)]
pub struct DeploymentService {
    settings: SettingsService,
    default_repository_path: PathBuf,
    store: DeploymentStore,
    operation_lock: Arc<Mutex<()>>,
}

impl DeploymentService {
    pub fn new(
        settings: SettingsService,
        database: &Database,
        default_repository_path: PathBuf,
    ) -> Self {
        Self {
            settings,
            default_repository_path,
            store: DeploymentStore::new(database.pool().clone()),
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn operation_lock(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.operation_lock)
    }

    pub async fn set_enabled(
        &self,
        efmi_root: PathBuf,
        mod_ids: Vec<Uuid>,
        enabled: bool,
    ) -> Result<ModDeploymentMutationResult, AppError> {
        let mod_ids = validate_batch(mod_ids)?;
        let _guard = self.operation_lock.lock().await;
        let strategy = EfmiCopyDeploymentStrategy::open(efmi_root).await?;
        let profile_id = self.store.active_profile_id().await?;
        let (updated, warnings) = if enabled {
            self.enable(profile_id, &strategy, &mod_ids).await?
        } else {
            self.disable(profile_id, &strategy, &mod_ids).await?
        };

        tracing::info!(
            profile_id = %profile_id,
            enabled,
            requested = mod_ids.len(),
            updated,
            strategy = strategy.strategy_id(),
            "mod deployment state updated"
        );
        Ok(ModDeploymentMutationResult {
            updated: u64::try_from(updated)
                .map_err(|_| AppError::DataIntegrity("部署更新数量超出支持范围。".to_owned()))?,
            enabled,
            profile_id,
            guidance: (updated > 0).then(|| LOADER_REFRESH_GUIDANCE.to_owned()),
            warnings,
        })
    }

    pub async fn recover_pending(&self, efmi_root: PathBuf) -> Result<(), AppError> {
        let _guard = self.operation_lock.lock().await;
        let strategy = EfmiCopyDeploymentStrategy::open(efmi_root).await?;
        let stored = self.store.all_manifests().await?;
        let known = stored
            .iter()
            .map(|manifest| (manifest.id, manifest.clone()))
            .collect::<HashMap<_, _>>();
        let owned = strategy.owned_directories().await?;

        for (directory, manifest) in owned {
            if manifest.strategy_id != EFMI_COPY_STRATEGY_ID {
                tracing::warn!(path = %directory.display(), strategy = %manifest.strategy_id, "preserving deployment owned by an unsupported strategy");
                continue;
            }
            let name = directory.to_string_lossy();
            let result = if name.starts_with("DISABLED_AEMM_PENDING_") {
                strategy
                    .remove_orphaned_directory(directory.clone(), manifest.clone(), true)
                    .await
            } else if name.starts_with("DISABLED_AEMM_REVOKE_") {
                let receipt = DeploymentRevokeReceipt {
                    manifest: manifest.clone(),
                    tombstone_directory: directory.clone(),
                };
                if known.get(&manifest.id) == Some(&manifest) {
                    strategy.rollback_revoke(&receipt).await
                } else {
                    strategy.finalize_revoke(&receipt).await
                }
            } else if name.starts_with("AEMM_") {
                if known.get(&manifest.id) == Some(&manifest) {
                    strategy.verify(&manifest).await
                } else {
                    strategy
                        .remove_orphaned_directory(directory.clone(), manifest.clone(), false)
                        .await
                }
            } else {
                Ok(())
            };
            if let Err(error) = result {
                tracing::error!(path = %directory.display(), deployment_id = %manifest.id, error = %error, "EFMI deployment recovery preserved an inconsistent directory for manual inspection");
            }
        }

        for manifest in &stored {
            if manifest.strategy_id == EFMI_COPY_STRATEGY_ID {
                if let Err(error) = strategy.verify(manifest).await {
                    tracing::error!(deployment_id = %manifest.id, mod_id = %manifest.mod_id, error = %error, "database says mod is enabled but its EFMI deployment is not valid");
                }
            }
        }
        Ok(())
    }

    async fn enable(
        &self,
        profile_id: Uuid,
        strategy: &EfmiCopyDeploymentStrategy,
        mod_ids: &[Uuid],
    ) -> Result<(usize, Vec<String>), AppError> {
        let repository = self.repository_root().await?;
        let mut manifests = Vec::new();
        let mut warnings = Vec::new();

        for mod_id in mod_ids {
            if let Some(manifest) = self.store.manifest(profile_id, *mod_id).await? {
                strategy.verify(&manifest).await?;
                continue;
            }
            let source = self.store.deployment_source(*mod_id).await?;
            if source.lifecycle_state != ModLifecycleState::Installed {
                rollback_deployments(strategy, &manifests).await;
                return Err(AppError::Deployment(format!(
                    "模组 {mod_id} 当前状态不是“已安装”，不能启用。"
                )));
            }
            let repository_relative = RepositoryRelativePath::new(source.repository_path)?;
            let mod_root = match repository.resolve_existing_mod_root(&repository_relative) {
                Ok(path) => path,
                Err(error) => {
                    rollback_deployments(strategy, &manifests).await;
                    return Err(error);
                }
            };
            let context = DeploymentContext {
                profile_id,
                mod_id: source.mod_id,
                repository_root: repository.path().to_path_buf(),
                mod_root,
                destination_root: strategy.mods_root().to_path_buf(),
                source_content_fingerprint: source.content_fingerprint,
                files: Vec::new(),
            };
            let plan = match strategy.plan_deploy(&context).await {
                Ok(plan) => plan,
                Err(error) => {
                    rollback_deployments(strategy, &manifests).await;
                    return Err(error);
                }
            };
            warnings.extend(plan.warnings.iter().cloned());
            match strategy.deploy(&context, plan).await {
                Ok(manifest) => manifests.push(manifest),
                Err(error) => {
                    rollback_deployments(strategy, &manifests).await;
                    return Err(error);
                }
            }
        }

        if let Err(error) = self.store.save_enabled(profile_id, &manifests).await {
            rollback_deployments(strategy, &manifests).await;
            return Err(error);
        }
        Ok((manifests.len(), deduplicate_warnings(warnings)))
    }

    async fn disable(
        &self,
        profile_id: Uuid,
        strategy: &EfmiCopyDeploymentStrategy,
        mod_ids: &[Uuid],
    ) -> Result<(usize, Vec<String>), AppError> {
        let mut receipts = Vec::new();
        let mut warnings = Vec::new();
        for mod_id in mod_ids {
            let Some(manifest) = self.store.manifest(profile_id, *mod_id).await? else {
                continue;
            };
            let plan = match strategy.plan_revoke(&manifest).await {
                Ok(plan) => plan,
                Err(error) => {
                    rollback_revokes(strategy, &receipts).await;
                    return Err(error);
                }
            };
            warnings.extend(plan.warnings);
            match strategy.begin_revoke(&manifest).await {
                Ok(receipt) => receipts.push(receipt),
                Err(error) => {
                    rollback_revokes(strategy, &receipts).await;
                    return Err(error);
                }
            }
        }
        let manifests = receipts
            .iter()
            .map(|receipt| receipt.manifest.clone())
            .collect::<Vec<_>>();
        if let Err(error) = self.store.save_disabled(profile_id, &manifests).await {
            rollback_revokes(strategy, &receipts).await;
            return Err(error);
        }

        for receipt in &receipts {
            if let Err(error) = strategy.finalize_revoke(receipt).await {
                warnings.push(format!(
                    "模组 {} 已禁用，但隔离目录清理将在下次启动重试。",
                    receipt.manifest.mod_id
                ));
                tracing::warn!(deployment_id = %receipt.manifest.id, error = %error, "deployment revoke committed but cleanup is pending recovery");
            }
        }
        Ok((receipts.len(), deduplicate_warnings(warnings)))
    }

    async fn repository_root(&self) -> Result<RepositoryRoot, AppError> {
        let settings = self.settings.get().await;
        let configured = settings.storage.repository_path;
        let policy = if paths_equal(&configured, &self.default_repository_path) {
            RepositoryInitializationPolicy::TrustedAemmDefault
        } else {
            RepositoryInitializationPolicy::EmptyOnly
        };
        tokio::task::spawn_blocking(move || RepositoryRoot::open_or_initialize(&configured, policy))
            .await?
    }
}

async fn rollback_deployments(
    strategy: &EfmiCopyDeploymentStrategy,
    manifests: &[DeploymentManifest],
) {
    for manifest in manifests.iter().rev() {
        if let Err(error) = strategy.rollback_deploy(manifest).await {
            tracing::error!(deployment_id = %manifest.id, error = %error, "failed to roll back EFMI deployment; startup recovery will retry");
        }
    }
}

async fn rollback_revokes(
    strategy: &EfmiCopyDeploymentStrategy,
    receipts: &[DeploymentRevokeReceipt],
) {
    for receipt in receipts.iter().rev() {
        if let Err(error) = strategy.rollback_revoke(receipt).await {
            tracing::error!(deployment_id = %receipt.manifest.id, error = %error, "failed to roll back EFMI revoke; startup recovery will retry");
        }
    }
}

fn validate_batch(mod_ids: Vec<Uuid>) -> Result<Vec<Uuid>, AppError> {
    if mod_ids.is_empty() {
        return Err(AppError::Deployment("请至少选择一个模组。".to_owned()));
    }
    if mod_ids.len() > MAX_BATCH_DEPLOYMENTS {
        return Err(AppError::Deployment(format!(
            "单次最多启停 {MAX_BATCH_DEPLOYMENTS} 个模组。"
        )));
    }
    let mut unique = Vec::with_capacity(mod_ids.len());
    for mod_id in mod_ids {
        if !unique.contains(&mod_id) {
            unique.push(mod_id);
        }
    }
    Ok(unique)
}

fn deduplicate_warnings(warnings: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for warning in warnings {
        if !unique.contains(&warning) {
            unique.push(warning);
        }
    }
    unique
}

fn paths_equal(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(right.to_string_lossy().trim_end_matches(['\\', '/']))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, path::Path};

    use crate::{
        core::mods::{
            FileSystemModScanner, ModScanner, RepositoryInitializationPolicy, RepositoryRoot,
        },
        database::{Database, DeploymentStore, ModStore},
        services::{AppPaths, DeploymentService, SettingsService},
    };

    fn create_loader(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(root.join("Mods"))?;
        fs::write(
            root.join("d3dx.ini"),
            "[Include]\ninclude_recursive = Mods\nexclude_recursive = DISABLED*\n",
        )?;
        Ok(())
    }

    async fn service_fixture(
        root: &Path,
    ) -> Result<
        (DeploymentService, Database, AppPaths, std::path::PathBuf),
        Box<dyn std::error::Error>,
    > {
        let paths = AppPaths::for_test(&root.join("app"));
        paths.ensure_base_directories().await?;
        let settings = SettingsService::load_or_create(&paths).await?;
        let database = Database::connect(&paths.database_file).await?;
        RepositoryRoot::open_or_initialize(
            &paths.repository_directory,
            RepositoryInitializationPolicy::TrustedAemmDefault,
        )?;
        let loader = root.join("EFMI");
        create_loader(&loader)?;
        Ok((
            DeploymentService::new(settings, &database, paths.repository_directory.clone()),
            database,
            paths,
            loader,
        ))
    }

    async fn scan_repository(
        database: &Database,
        repository_path: &Path,
    ) -> Result<Vec<crate::models::ModListItem>, Box<dyn std::error::Error>> {
        let repository = RepositoryRoot::open_or_initialize(
            repository_path,
            RepositoryInitializationPolicy::TrustedAemmDefault,
        )?;
        let scan = FileSystemModScanner::new()
            .scan_repository(repository, HashMap::new())
            .await?;
        let store = ModStore::new(database.pool().clone());
        store.synchronize(&scan).await?;
        Ok(store.list().await?)
    }

    #[tokio::test]
    async fn enables_and_disables_through_manifest_backed_service()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let (service, database, paths, loader) = service_fixture(root.path()).await?;
        let mod_root = paths.repository_directory.join("fixture-mod");
        fs::create_dir(&mod_root)?;
        fs::write(mod_root.join("main.ini"), b"[TextureOverrideFixture]\n")?;
        let mods = scan_repository(&database, &paths.repository_directory).await?;
        let mod_id = mods
            .first()
            .ok_or_else(|| std::io::Error::other("fixture mod missing"))?
            .id;

        let enabled = service
            .set_enabled(loader.clone(), vec![mod_id], true)
            .await?;
        assert_eq!(enabled.updated, 1);
        assert!(
            loader
                .join("Mods")
                .join(format!("AEMM_{}", mod_id.simple()))
                .is_dir()
        );
        assert!(
            ModStore::new(database.pool().clone())
                .list()
                .await?
                .first()
                .is_some_and(|item| item.enabled)
        );

        let disabled = service.set_enabled(loader, vec![mod_id], false).await?;
        assert_eq!(disabled.updated, 1);
        assert!(
            ModStore::new(database.pool().clone())
                .list()
                .await?
                .first()
                .is_some_and(|item| !item.enabled)
        );
        Ok(())
    }

    #[tokio::test]
    async fn batch_failure_rolls_back_prior_filesystem_deployments()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let (service, database, paths, loader) = service_fixture(root.path()).await?;
        let valid = paths.repository_directory.join("a-valid");
        fs::create_dir(&valid)?;
        fs::write(valid.join("main.ini"), b"[TextureOverrideFixture]\n")?;
        let invalid = paths.repository_directory.join("z-invalid");
        fs::create_dir(&invalid)?;
        fs::write(invalid.join("readme.txt"), b"not an EFMI mod")?;
        let mods = scan_repository(&database, &paths.repository_directory).await?;
        let valid_id = mods
            .iter()
            .find(|item| item.repository_path == Path::new("a-valid"))
            .ok_or_else(|| std::io::Error::other("valid fixture missing"))?
            .id;
        let invalid_id = mods
            .iter()
            .find(|item| item.repository_path == Path::new("z-invalid"))
            .ok_or_else(|| std::io::Error::other("invalid fixture missing"))?
            .id;

        assert!(
            service
                .set_enabled(loader.clone(), vec![valid_id, invalid_id], true)
                .await
                .is_err()
        );
        assert!(
            !loader
                .join("Mods")
                .join(format!("AEMM_{}", valid_id.simple()))
                .exists()
        );
        assert!(
            DeploymentStore::new(database.pool().clone())
                .all_manifests()
                .await?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn startup_recovery_reconciles_revoke_with_database_commit_state()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::core::deployment::{EfmiCopyDeploymentStrategy, ModDeploymentStrategy};

        let root = tempfile::tempdir()?;
        let (service, database, paths, loader) = service_fixture(root.path()).await?;
        let mod_root = paths.repository_directory.join("fixture-mod");
        fs::create_dir(&mod_root)?;
        fs::write(mod_root.join("main.ini"), b"[TextureOverrideFixture]\n")?;
        let mods = scan_repository(&database, &paths.repository_directory).await?;
        let mod_id = mods
            .first()
            .ok_or_else(|| std::io::Error::other("fixture mod missing"))?
            .id;
        service
            .set_enabled(loader.clone(), vec![mod_id], true)
            .await?;
        let store = DeploymentStore::new(database.pool().clone());
        let profile_id = store.active_profile_id().await?;
        let manifest = store
            .manifest(profile_id, mod_id)
            .await?
            .ok_or_else(|| std::io::Error::other("deployment manifest missing"))?;
        let strategy = EfmiCopyDeploymentStrategy::open(loader.clone()).await?;

        let receipt = strategy.begin_revoke(&manifest).await?;
        assert!(
            strategy
                .mods_root()
                .join(&receipt.tombstone_directory)
                .is_dir()
        );
        service.recover_pending(loader.clone()).await?;
        strategy.verify(&manifest).await?;

        let receipt = strategy.begin_revoke(&manifest).await?;
        store
            .save_disabled(profile_id, std::slice::from_ref(&manifest))
            .await?;
        service.recover_pending(loader).await?;
        assert!(
            !strategy
                .mods_root()
                .join(&receipt.tombstone_directory)
                .exists()
        );
        assert!(
            !strategy
                .mods_root()
                .join(&manifest.destination_directory)
                .exists()
        );
        Ok(())
    }
}
