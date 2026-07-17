use tauri::AppHandle;

use crate::{
    core::mods::InstallProgressReporter,
    database::Database,
    errors::AppError,
    models::{
        AppBootstrap, AppSettings, ConflictReport, DetectedGameInstallation, GameLaunchResult,
        GameStatus, LaunchMode, LocalModMetadata, ModDeploymentMutationResult, ModDetails,
        ModImportPlan, ModInstallResult, ModListItem, ModMutationResult, ModPreview, ModScanResult,
        Profile, ProfileSwitchResult,
    },
};

use super::{
    AppPaths, ConflictService, DeploymentService, GameService, ModService, ProfileService,
    SettingsService, logging::initialize_logging,
};

pub struct AppServices {
    paths: AppPaths,
    settings: SettingsService,
    game: GameService,
    mods: ModService,
    deployment: DeploymentService,
    conflicts: ConflictService,
    profiles: ProfileService,
    database: Database,
    _logging_guard: super::logging::LoggingGuard,
}

impl AppServices {
    pub async fn initialize(app: &AppHandle) -> Result<Self, AppError> {
        let paths = AppPaths::resolve(app)?;
        paths.ensure_base_directories().await?;

        let settings = SettingsService::load_or_create(&paths).await?;
        let current_settings = settings.get().await;
        let logging_guard = initialize_logging(&paths.log_directory, current_settings.log_level)?;

        tracing::info!(
            config_path = %paths.config_file.display(),
            database_path = %paths.database_file.display(),
            "initializing application services"
        );

        let database = Database::connect(&paths.database_file).await?;
        tracing::info!("database migrations and health check completed");
        let game = GameService::new(settings.clone());
        let mods = ModService::new(
            settings.clone(),
            &database,
            paths.repository_directory.clone(),
            paths.staging_directory.clone(),
        );
        if let Err(error) = mods.recover_pending_installations().await {
            tracing::error!(error = %error, diagnostic = ?error, "mod installation recovery could not complete during startup");
        }
        let deployment = DeploymentService::new(
            settings.clone(),
            &database,
            paths.repository_directory.clone(),
        );
        let conflicts = ConflictService::new(&database, deployment.operation_lock());
        let profiles = ProfileService::new(&database, deployment.operation_lock());
        match game.validated_efmi_root().await {
            Ok(efmi_root) => {
                if let Err(error) = deployment.recover_pending(efmi_root).await {
                    tracing::error!(error = %error, diagnostic = ?error, "EFMI deployment recovery could not complete during startup");
                }
            }
            Err(AppError::NotAvailable(_)) => {
                tracing::debug!("EFMI deployment recovery skipped because no loader is configured");
            }
            Err(error) => {
                tracing::warn!(error = %error, diagnostic = ?error, "EFMI deployment recovery deferred until loader configuration is valid");
            }
        }

        Ok(Self {
            paths,
            settings,
            game,
            mods,
            deployment,
            conflicts,
            profiles,
            database,
            _logging_guard: logging_guard,
        })
    }

    pub async fn bootstrap(
        &self,
        app_name: String,
        app_version: String,
    ) -> Result<AppBootstrap, AppError> {
        self.database.health_check().await?;
        Ok(AppBootstrap {
            app_name,
            app_version,
            runtime_mode: "desktop",
            database_ready: true,
            config_path: self.paths.config_file.clone(),
            database_path: self.paths.database_file.clone(),
            log_directory: self.paths.log_directory.clone(),
            settings: self.settings.get().await,
        })
    }

    pub async fn update_settings(&self, settings: AppSettings) -> Result<AppSettings, AppError> {
        self.settings.update(settings).await
    }

    pub async fn game_status(&self) -> Result<GameStatus, AppError> {
        self.game.status().await
    }

    pub async fn detect_game_installations(
        &self,
    ) -> Result<Vec<DetectedGameInstallation>, AppError> {
        self.game.detect_installations().await
    }

    pub async fn configure_game_installation(
        &self,
        path: &std::path::Path,
    ) -> Result<GameStatus, AppError> {
        self.game.configure_installation(path).await
    }

    pub async fn configure_efmi_loader(
        &self,
        path: Option<&std::path::Path>,
    ) -> Result<GameStatus, AppError> {
        self.game.configure_loader(path).await
    }

    pub async fn set_game_launch_mode(
        &self,
        launch_mode: LaunchMode,
    ) -> Result<GameStatus, AppError> {
        self.game.set_launch_mode(launch_mode).await
    }

    pub async fn validated_game_directory(&self) -> Result<std::path::PathBuf, AppError> {
        self.game.validated_game_directory().await
    }

    pub async fn launch_game(&self) -> Result<GameLaunchResult, AppError> {
        self.game.launch().await
    }

    pub async fn scan_mod_repository(&self) -> Result<ModScanResult, AppError> {
        self.mods.scan_repository().await
    }

    pub async fn list_installed_mods(&self) -> Result<Vec<ModListItem>, AppError> {
        self.mods.list().await
    }

    pub async fn prepare_mod_import(
        &self,
        source_path: std::path::PathBuf,
        progress: InstallProgressReporter,
    ) -> Result<ModImportPlan, AppError> {
        self.mods.prepare_import(source_path, progress).await
    }

    pub async fn commit_mod_import(
        &self,
        operation_id: uuid::Uuid,
        progress: InstallProgressReporter,
    ) -> Result<ModInstallResult, AppError> {
        self.mods.commit_import(operation_id, progress).await
    }

    pub async fn cancel_mod_import(&self, operation_id: uuid::Uuid) -> Result<(), AppError> {
        self.mods.cancel_import(operation_id).await
    }

    pub async fn mod_details(&self, mod_id: uuid::Uuid) -> Result<ModDetails, AppError> {
        self.mods.details(mod_id).await
    }

    pub async fn set_mod_favorite(
        &self,
        mod_ids: Vec<uuid::Uuid>,
        favorite: bool,
    ) -> Result<ModMutationResult, AppError> {
        self.mods.set_favorite(mod_ids, favorite).await
    }

    pub async fn set_mods_enabled(
        &self,
        mod_ids: Vec<uuid::Uuid>,
        enabled: bool,
    ) -> Result<ModDeploymentMutationResult, AppError> {
        let changed_mod_ids = mod_ids.clone();
        let efmi_root = self.game.validated_efmi_root().await?;
        let mut result = self
            .deployment
            .set_enabled(efmi_root, mod_ids, enabled)
            .await?;
        if enabled && result.updated > 0 {
            match self.conflicts.analyze_active().await {
                Ok(report) => {
                    let new_conflicts = report
                        .conflicts
                        .iter()
                        .filter(|conflict| {
                            conflict
                                .participants
                                .iter()
                                .any(|participant| changed_mod_ids.contains(&participant.mod_id))
                        })
                        .count();
                    if new_conflicts > 0 {
                        result.warnings.push(format!(
                            "启用后检测到 {new_conflicts} 组与本次模组相关的冲突，请查看冲突详情；EFMI 实际胜出顺序尚未验证。"
                        ));
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, diagnostic = ?error, "post-deployment conflict analysis failed");
                    result.warnings.push(
                        "模组已启用，但冲突分析未完成；请查看日志并在模组页面重试。".to_owned(),
                    );
                }
            }
        }
        Ok(result)
    }

    pub async fn active_conflict_report(&self) -> Result<ConflictReport, AppError> {
        self.conflicts.analyze_active().await
    }

    pub async fn list_profiles(&self) -> Result<Vec<Profile>, AppError> {
        self.profiles.list().await
    }

    pub async fn create_profile(&self, name: String) -> Result<Profile, AppError> {
        self.profiles.create(name).await
    }

    pub async fn rename_profile(
        &self,
        profile_id: uuid::Uuid,
        name: String,
    ) -> Result<Profile, AppError> {
        self.profiles.rename(profile_id, name).await
    }

    pub async fn copy_profile(
        &self,
        source_profile_id: uuid::Uuid,
        name: String,
    ) -> Result<Profile, AppError> {
        self.profiles.copy(source_profile_id, name).await
    }

    pub async fn delete_profile(&self, profile_id: uuid::Uuid) -> Result<(), AppError> {
        self.profiles.delete(profile_id).await
    }

    pub async fn switch_profile(
        &self,
        profile_id: uuid::Uuid,
    ) -> Result<ProfileSwitchResult, AppError> {
        let efmi_root = self.game.validated_efmi_root().await;
        self.deployment.switch_profile(efmi_root, profile_id).await
    }

    pub async fn mod_preview(&self, mod_id: uuid::Uuid) -> Result<Option<ModPreview>, AppError> {
        self.mods.preview(mod_id).await
    }

    pub async fn mod_directory(&self, mod_id: uuid::Uuid) -> Result<std::path::PathBuf, AppError> {
        self.mods.mod_directory(mod_id).await
    }

    pub async fn update_local_mod_metadata(
        &self,
        mod_id: uuid::Uuid,
        metadata: LocalModMetadata,
    ) -> Result<ModListItem, AppError> {
        self.mods.update_local_metadata(mod_id, metadata).await
    }
}
