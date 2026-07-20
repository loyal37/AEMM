use std::{path::PathBuf, sync::Arc};

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    core::{
        deployment::{EfmiDirectDeploymentStrategy, ModDeploymentStrategy},
        mods::{RepositoryInitializationPolicy, RepositoryRelativePath, RepositoryRoot},
    },
    database::{Database, DeploymentStore, ProfileStore},
    errors::AppError,
    models::{
        DeploymentContext, DeploymentManifest, DeploymentRevokeReceipt,
        ModDeploymentMutationResult, ModLifecycleState, ProfileSwitchResult,
    },
};

use super::SettingsService;

const MAX_BATCH_DEPLOYMENTS: usize = 256;
const LOADER_REFRESH_GUIDANCE: &str =
    "如果游戏正在运行：先让被修改角色离开画面，再按 F10 让 EFMI 重新加载。";

#[derive(Debug)]
pub struct DeploymentService {
    settings: SettingsService,
    store: DeploymentStore,
    profiles: ProfileStore,
    operation_lock: Arc<Mutex<()>>,
}

impl DeploymentService {
    pub fn new(
        settings: SettingsService,
        database: &Database,
        _default_repository_path: PathBuf,
    ) -> Self {
        Self {
            settings,
            store: DeploymentStore::new(database.pool().clone()),
            profiles: ProfileStore::new(database.pool().clone()),
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
        let strategy = EfmiDirectDeploymentStrategy::open(efmi_root).await?;
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

    pub async fn switch_profile(
        &self,
        efmi_root: Result<PathBuf, AppError>,
        target_profile_id: Uuid,
    ) -> Result<ProfileSwitchResult, AppError> {
        let _guard = self.operation_lock.lock().await;
        let plan = self.profiles.prepare_switch(target_profile_id).await?;
        if plan.source_profile_id == plan.target_profile_id {
            return Ok(ProfileSwitchResult {
                profile: self.profiles.get(target_profile_id).await?,
                disabled_mods: 0,
                enabled_mods: 0,
                guidance: None,
                warnings: plan.warnings,
            });
        }

        if plan.source_manifests.is_empty() && plan.target_mod_ids.is_empty() {
            self.profiles.commit_switch(&plan, &[]).await?;
            tracing::info!(
                source_profile_id = %plan.source_profile_id,
                target_profile_id = %plan.target_profile_id,
                "empty profile switched without a deployment target"
            );
            return Ok(ProfileSwitchResult {
                profile: self.profiles.get(target_profile_id).await?,
                disabled_mods: 0,
                enabled_mods: 0,
                guidance: None,
                warnings: plan.warnings,
            });
        }

        let strategy = EfmiDirectDeploymentStrategy::open(efmi_root?).await?;
        let repository = self.repository_root().await?;
        let mut warnings = plan.warnings.clone();
        let mut source_receipts = Vec::with_capacity(plan.source_manifests.len());
        for manifest in &plan.source_manifests {
            let revoke_plan = match strategy.plan_revoke(manifest).await {
                Ok(revoke_plan) => revoke_plan,
                Err(error) => {
                    rollback_revokes(&strategy, &source_receipts).await;
                    return Err(error);
                }
            };
            warnings.extend(revoke_plan.warnings);
            match strategy.begin_revoke(manifest).await {
                Ok(receipt) => source_receipts.push(receipt),
                Err(error) => {
                    rollback_revokes(&strategy, &source_receipts).await;
                    return Err(error);
                }
            }
        }

        let mut target_manifests = Vec::with_capacity(plan.target_mod_ids.len());
        for mod_id in &plan.target_mod_ids {
            let context = match self
                .deployment_context(&repository, plan.target_profile_id, &strategy, *mod_id)
                .await
            {
                Ok(context) => context,
                Err(error) => {
                    rollback_profile_switch(&strategy, &target_manifests, &source_receipts).await;
                    return Err(error);
                }
            };
            let deployment_plan = match strategy.plan_deploy(&context).await {
                Ok(deployment_plan) => deployment_plan,
                Err(error) => {
                    rollback_profile_switch(&strategy, &target_manifests, &source_receipts).await;
                    return Err(error);
                }
            };
            warnings.extend(deployment_plan.warnings.iter().cloned());
            match strategy.deploy(&context, deployment_plan).await {
                Ok(manifest) => target_manifests.push(manifest),
                Err(error) => {
                    rollback_profile_switch(&strategy, &target_manifests, &source_receipts).await;
                    return Err(error);
                }
            }
        }

        if let Err(error) = self.profiles.commit_switch(&plan, &target_manifests).await {
            rollback_profile_switch(&strategy, &target_manifests, &source_receipts).await;
            return Err(error);
        }

        for receipt in &source_receipts {
            if let Err(error) = strategy.finalize_revoke(receipt).await {
                warnings.push(format!(
                    "源 Profile 的模组 {} 已撤销，但隔离目录清理将在下次启动重试。",
                    receipt.manifest.mod_id
                ));
                tracing::warn!(deployment_id = %receipt.manifest.id, error = %error, "profile switch committed but source cleanup is pending recovery");
            }
        }
        warnings = deduplicate_warnings(warnings);
        tracing::info!(
            source_profile_id = %plan.source_profile_id,
            target_profile_id = %plan.target_profile_id,
            disabled = source_receipts.len(),
            enabled = target_manifests.len(),
            "profile switch committed"
        );
        Ok(ProfileSwitchResult {
            profile: self.profiles.get(target_profile_id).await?,
            disabled_mods: u64::try_from(source_receipts.len()).map_err(|_| {
                AppError::DataIntegrity("Profile 撤销数量超出支持范围。".to_owned())
            })?,
            enabled_mods: u64::try_from(target_manifests.len()).map_err(|_| {
                AppError::DataIntegrity("Profile 启用数量超出支持范围。".to_owned())
            })?,
            guidance: (!source_receipts.is_empty() || !target_manifests.is_empty())
                .then(|| LOADER_REFRESH_GUIDANCE.to_owned()),
            warnings,
        })
    }

    pub async fn recover_pending(&self, efmi_root: PathBuf) -> Result<(), AppError> {
        let _guard = self.operation_lock.lock().await;
        let strategy = EfmiDirectDeploymentStrategy::open(efmi_root).await?;
        for manifest in self.store.all_manifests().await? {
            if let Err(error) = strategy.verify(&manifest).await {
                tracing::warn!(deployment_id = %manifest.id, mod_id = %manifest.mod_id, error = %error, "physical EFMI mod state will be reconciled by the next scan");
            }
        }
        Ok(())
    }

    async fn enable(
        &self,
        profile_id: Uuid,
        strategy: &EfmiDirectDeploymentStrategy,
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
            let context = match self
                .deployment_context(&repository, profile_id, strategy, *mod_id)
                .await
            {
                Ok(context) => context,
                Err(error) => {
                    rollback_deployments(strategy, &manifests).await;
                    return Err(error);
                }
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
        strategy: &EfmiDirectDeploymentStrategy,
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

    async fn deployment_context(
        &self,
        repository: &RepositoryRoot,
        profile_id: Uuid,
        strategy: &EfmiDirectDeploymentStrategy,
        mod_id: Uuid,
    ) -> Result<DeploymentContext, AppError> {
        let source = self.store.deployment_source(mod_id).await?;
        if source.lifecycle_state != ModLifecycleState::Installed {
            return Err(AppError::Deployment(format!(
                "模组 {mod_id} 当前状态不是“已安装”，不能启用。"
            )));
        }
        let repository_relative = RepositoryRelativePath::new(source.repository_path)?;
        let mod_root = repository.resolve_existing_mod_root(&repository_relative)?;
        Ok(DeploymentContext {
            profile_id,
            mod_id: source.mod_id,
            repository_root: repository.path().to_path_buf(),
            mod_root,
            destination_root: strategy.mods_root().to_path_buf(),
            source_content_fingerprint: source.content_fingerprint,
            files: Vec::new(),
        })
    }

    async fn repository_root(&self) -> Result<RepositoryRoot, AppError> {
        let settings = self.settings.get().await;
        let configured = settings.storage.repository_path;
        let policy = RepositoryInitializationPolicy::ExternalEfmiMods;
        tokio::task::spawn_blocking(move || RepositoryRoot::open_or_initialize(&configured, policy))
            .await?
    }
}

async fn rollback_deployments(
    strategy: &EfmiDirectDeploymentStrategy,
    manifests: &[DeploymentManifest],
) {
    for manifest in manifests.iter().rev() {
        if let Err(error) = strategy.rollback_deploy(manifest).await {
            tracing::error!(deployment_id = %manifest.id, error = %error, "failed to roll back EFMI deployment; startup recovery will retry");
        }
    }
}

async fn rollback_revokes(
    strategy: &EfmiDirectDeploymentStrategy,
    receipts: &[DeploymentRevokeReceipt],
) {
    for receipt in receipts.iter().rev() {
        if let Err(error) = strategy.rollback_revoke(receipt).await {
            tracing::error!(deployment_id = %receipt.manifest.id, error = %error, "failed to roll back EFMI revoke; startup recovery will retry");
        }
    }
}

async fn rollback_profile_switch(
    strategy: &EfmiDirectDeploymentStrategy,
    target_manifests: &[DeploymentManifest],
    source_receipts: &[DeploymentRevokeReceipt],
) {
    rollback_deployments(strategy, target_manifests).await;
    rollback_revokes(strategy, source_receipts).await;
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, path::Path};

    use uuid::Uuid;

    use crate::{
        core::mods::{
            FileSystemModScanner, ModScanner, RepositoryInitializationPolicy, RepositoryRoot,
        },
        database::{Database, DeploymentStore, ModStore, ProfileStore},
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
    #[ignore = "legacy copy-deployment behavior superseded by direct EFMI Mods state"]
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
    #[ignore = "legacy copy-deployment behavior superseded by direct EFMI Mods state"]
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
    #[ignore = "legacy copy-deployment recovery superseded by direct EFMI Mods state"]
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

    #[tokio::test]
    #[ignore = "legacy copy-deployment profile fixture superseded by direct EFMI Mods state"]
    async fn switches_profiles_and_preserves_each_desired_mod_set()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let (service, database, paths, loader) = service_fixture(root.path()).await?;
        let first_root = paths.repository_directory.join("first-mod");
        fs::create_dir(&first_root)?;
        fs::write(first_root.join("main.ini"), b"[TextureOverrideFirst]\n")?;
        let second_root = paths.repository_directory.join("second-mod");
        fs::create_dir(&second_root)?;
        fs::write(second_root.join("main.ini"), b"[TextureOverrideSecond]\n")?;
        let mods = scan_repository(&database, &paths.repository_directory).await?;
        let first_id = mods
            .iter()
            .find(|item| item.repository_path == Path::new("first-mod"))
            .ok_or_else(|| std::io::Error::other("first fixture missing"))?
            .id;
        let second_id = mods
            .iter()
            .find(|item| item.repository_path == Path::new("second-mod"))
            .ok_or_else(|| std::io::Error::other("second fixture missing"))?
            .id;
        service
            .set_enabled(loader.clone(), vec![first_id], true)
            .await?;

        let profiles = ProfileStore::new(database.pool().clone());
        let source_profile_id = DeploymentStore::new(database.pool().clone())
            .active_profile_id()
            .await?;
        let target_profile_id = Uuid::new_v4();
        profiles.create(target_profile_id, "Second profile").await?;
        sqlx::query(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, load_order)
             VALUES (?, ?, 1, 0)",
        )
        .bind(target_profile_id.to_string())
        .bind(second_id.to_string())
        .execute(database.pool())
        .await?;

        let switched = service
            .switch_profile(Ok(loader.clone()), target_profile_id)
            .await?;
        assert_eq!(switched.disabled_mods, 1);
        assert_eq!(switched.enabled_mods, 1);
        assert!(switched.profile.is_active);
        assert!(
            !loader
                .join("Mods")
                .join(format!("AEMM_{}", first_id.simple()))
                .exists()
        );
        assert!(
            loader
                .join("Mods")
                .join(format!("AEMM_{}", second_id.simple()))
                .is_dir()
        );
        let source_desired: i64 = sqlx::query_scalar(
            "SELECT enabled FROM profile_mods WHERE profile_id = ? AND mod_id = ?",
        )
        .bind(source_profile_id.to_string())
        .bind(first_id.to_string())
        .fetch_one(database.pool())
        .await?;
        assert_eq!(source_desired, 1);

        service
            .switch_profile(Ok(loader.clone()), source_profile_id)
            .await?;
        assert!(
            loader
                .join("Mods")
                .join(format!("AEMM_{}", first_id.simple()))
                .is_dir()
        );
        assert!(
            !loader
                .join("Mods")
                .join(format!("AEMM_{}", second_id.simple()))
                .exists()
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "legacy copy-deployment profile fixture superseded by direct EFMI Mods state"]
    async fn failed_profile_deployment_restores_source_profile()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let (service, database, paths, loader) = service_fixture(root.path()).await?;
        let valid_root = paths.repository_directory.join("valid-mod");
        fs::create_dir(&valid_root)?;
        fs::write(valid_root.join("main.ini"), b"[TextureOverrideValid]\n")?;
        let invalid_root = paths.repository_directory.join("invalid-mod");
        fs::create_dir(&invalid_root)?;
        fs::write(invalid_root.join("readme.txt"), b"not an EFMI mod")?;
        let mods = scan_repository(&database, &paths.repository_directory).await?;
        let valid_id = mods
            .iter()
            .find(|item| item.repository_path == Path::new("valid-mod"))
            .ok_or_else(|| std::io::Error::other("valid fixture missing"))?
            .id;
        let invalid_id = mods
            .iter()
            .find(|item| item.repository_path == Path::new("invalid-mod"))
            .ok_or_else(|| std::io::Error::other("invalid fixture missing"))?
            .id;
        service
            .set_enabled(loader.clone(), vec![valid_id], true)
            .await?;
        let deployment_store = DeploymentStore::new(database.pool().clone());
        let source_profile_id = deployment_store.active_profile_id().await?;
        let target_profile_id = Uuid::new_v4();
        ProfileStore::new(database.pool().clone())
            .create(target_profile_id, "Broken target")
            .await?;
        sqlx::query(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, load_order)
             VALUES (?, ?, 1, 0)",
        )
        .bind(target_profile_id.to_string())
        .bind(invalid_id.to_string())
        .execute(database.pool())
        .await?;

        assert!(
            service
                .switch_profile(Ok(loader.clone()), target_profile_id)
                .await
                .is_err()
        );
        assert_eq!(
            deployment_store.active_profile_id().await?,
            source_profile_id
        );
        assert!(
            deployment_store
                .manifest(source_profile_id, valid_id)
                .await?
                .is_some()
        );
        assert!(
            deployment_store
                .manifest(target_profile_id, invalid_id)
                .await?
                .is_none()
        );
        assert!(
            loader
                .join("Mods")
                .join(format!("AEMM_{}", valid_id.simple()))
                .is_dir()
        );
        Ok(())
    }

    #[tokio::test]
    async fn directly_toggles_efmi_mods_and_persists_physical_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = tempfile::tempdir()?;
        let paths = AppPaths::for_test(&fixture.path().join("app"));
        paths.ensure_base_directories().await?;
        let loader = fixture.path().join("EFMI");
        create_loader(&loader)?;
        let disabled = loader.join("Mods/DISABLED_fixture-mod");
        fs::create_dir(&disabled)?;
        fs::write(disabled.join("main.ini"), b"[TextureOverrideFixture]\n")?;

        let settings = SettingsService::load_or_create(&paths).await?;
        let mut configured = settings.get().await;
        configured.storage.repository_path = loader.join("Mods");
        settings.update(configured).await?;
        let database = Database::connect(&paths.database_file).await?;
        let repository = RepositoryRoot::open_or_initialize(
            &loader.join("Mods"),
            RepositoryInitializationPolicy::ExternalEfmiMods,
        )?;
        let scan = FileSystemModScanner::new()
            .scan_repository(repository, HashMap::new())
            .await?;
        let store = ModStore::new(database.pool().clone());
        store.synchronize(&scan).await?;
        let mod_id = store.list().await?.first().ok_or("missing mod")?.id;
        let service = DeploymentService::new(settings, &database, paths.repository_directory);

        let enabled = service
            .set_enabled(loader.clone(), vec![mod_id], true)
            .await?;
        assert_eq!(enabled.updated, 1);
        assert!(loader.join("Mods/fixture-mod").is_dir());
        assert_eq!(
            store.list().await?.first().map(|item| item.enabled),
            Some(true)
        );

        let disabled_result = service
            .set_enabled(loader.clone(), vec![mod_id], false)
            .await?;
        assert_eq!(disabled_result.updated, 1);
        assert!(loader.join("Mods/DISABLED_fixture-mod").is_dir());
        assert_eq!(
            store.list().await?.first().map(|item| item.enabled),
            Some(false)
        );
        Ok(())
    }

    #[tokio::test]
    async fn switches_between_empty_profiles_without_a_loader()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let (service, database, _, _) = service_fixture(root.path()).await?;
        let target_profile_id = Uuid::new_v4();
        ProfileStore::new(database.pool().clone())
            .create(target_profile_id, "Empty target")
            .await?;

        let result = service
            .switch_profile(
                Err(crate::errors::AppError::NotAvailable(
                    "loader is not configured".to_owned(),
                )),
                target_profile_id,
            )
            .await?;

        assert!(result.profile.is_active);
        assert_eq!(result.disabled_mods, 0);
        assert_eq!(result.enabled_mods, 0);
        Ok(())
    }
}
