use tauri::AppHandle;

use crate::{
    database::Database,
    errors::AppError,
    models::{AppBootstrap, AppSettings},
};

use super::{AppPaths, SettingsService, logging::initialize_logging};

pub struct AppServices {
    paths: AppPaths,
    settings: SettingsService,
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

        Ok(Self {
            paths,
            settings,
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
}
