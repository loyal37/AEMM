use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::errors::AppError;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub database_file: PathBuf,
    pub log_directory: PathBuf,
    pub repository_directory: PathBuf,
    pub staging_directory: PathBuf,
}

impl AppPaths {
    pub fn resolve(app: &tauri::AppHandle) -> Result<Self, AppError> {
        let path_resolver = app.path();
        let config_directory = path_resolver
            .app_config_dir()
            .map_err(|error| AppError::PathResolution(error.to_string()))?;
        let data_directory = path_resolver
            .app_local_data_dir()
            .map_err(|error| AppError::PathResolution(error.to_string()))?;
        let cache_directory = path_resolver
            .app_cache_dir()
            .map_err(|error| AppError::PathResolution(error.to_string()))?;
        let log_directory = path_resolver
            .app_log_dir()
            .map_err(|error| AppError::PathResolution(error.to_string()))?;

        Ok(Self {
            config_file: config_directory.join("config.json"),
            database_file: data_directory.join("mods.db"),
            log_directory,
            repository_directory: data_directory.join("repository"),
            staging_directory: cache_directory.join("staging"),
        })
    }

    pub async fn ensure_base_directories(&self) -> Result<(), AppError> {
        let required = [
            parent_of(&self.config_file)?,
            parent_of(&self.database_file)?,
            self.log_directory.as_path(),
            self.repository_directory.as_path(),
            self.staging_directory.as_path(),
        ];

        for directory in required {
            tokio::fs::create_dir_all(directory)
                .await
                .map_err(|source| AppError::file_system(directory, source))?;
        }

        Ok(())
    }

    #[cfg(test)]
    pub fn for_test(root: &Path) -> Self {
        Self {
            config_file: root.join("config/config.json"),
            database_file: root.join("data/mods.db"),
            log_directory: root.join("logs"),
            repository_directory: root.join("data/repository"),
            staging_directory: root.join("cache/staging"),
        }
    }
}

fn parent_of(path: &Path) -> Result<&Path, AppError> {
    path.parent().ok_or_else(|| {
        AppError::PathResolution(format!("{} has no parent directory", path.display()))
    })
}
