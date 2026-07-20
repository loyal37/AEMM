use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::errors::AppError;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_directory: PathBuf,
    pub config_file: PathBuf,
    pub database_file: PathBuf,
    pub log_directory: PathBuf,
    pub repository_directory: PathBuf,
    pub staging_directory: PathBuf,
}

impl AppPaths {
    pub fn resolve(_app: &tauri::AppHandle) -> Result<Self, AppError> {
        let executable =
            std::env::current_exe().map_err(|error| AppError::PathResolution(error.to_string()))?;
        let executable_directory = parent_of(&executable)?;
        let software_directory = find_development_root(executable_directory)
            .unwrap_or_else(|| executable_directory.to_path_buf());
        let data_directory = software_directory.join("data");

        Ok(Self {
            data_directory: data_directory.clone(),
            config_file: data_directory.join("config.json"),
            database_file: data_directory.join("mods.db"),
            log_directory: data_directory.join("logs"),
            repository_directory: data_directory.join("unconfigured-mods"),
            staging_directory: data_directory.join("staging"),
        })
    }

    pub async fn ensure_base_directories(&self) -> Result<(), AppError> {
        let required = [
            parent_of(&self.config_file)?,
            parent_of(&self.database_file)?,
            self.log_directory.as_path(),
            self.staging_directory.as_path(),
        ];

        for directory in required {
            tokio::fs::create_dir_all(directory)
                .await
                .map_err(|source| AppError::file_system(directory, source))?;
        }

        Ok(())
    }

    pub async fn migrate_legacy_app_data(&self, app: &tauri::AppHandle) -> Result<(), AppError> {
        let resolver = app.path();
        let legacy_config = resolver
            .app_config_dir()
            .map_err(|error| AppError::PathResolution(error.to_string()))?
            .join("config.json");
        let legacy_data = resolver
            .app_local_data_dir()
            .map_err(|error| AppError::PathResolution(error.to_string()))?;
        let legacy_logs = resolver
            .app_log_dir()
            .map_err(|error| AppError::PathResolution(error.to_string()))?;
        let paths = self.clone();
        tokio::task::spawn_blocking(move || {
            copy_if_missing(&legacy_config, &paths.config_file)?;
            for suffix in ["", "-wal", "-shm"] {
                copy_if_missing(
                    &legacy_data.join(format!("mods.db{suffix}")),
                    &PathBuf::from(format!("{}{suffix}", paths.database_file.display())),
                )?;
            }
            if legacy_logs.is_dir() {
                for entry in std::fs::read_dir(&legacy_logs)
                    .map_err(|error| AppError::file_system(&legacy_logs, error))?
                {
                    let entry =
                        entry.map_err(|error| AppError::file_system(&legacy_logs, error))?;
                    if entry
                        .file_type()
                        .map_err(|error| AppError::file_system(entry.path(), error))?
                        .is_file()
                    {
                        copy_if_missing(
                            &entry.path(),
                            &paths.log_directory.join(entry.file_name()),
                        )?;
                    }
                }
            }
            Ok(())
        })
        .await?
    }

    #[cfg(test)]
    pub fn for_test(root: &Path) -> Self {
        Self {
            data_directory: root.join("data"),
            config_file: root.join("config/config.json"),
            database_file: root.join("data/mods.db"),
            log_directory: root.join("logs"),
            repository_directory: root.join("data/repository"),
            staging_directory: root.join("cache/staging"),
        }
    }
}

fn find_development_root(executable_directory: &Path) -> Option<PathBuf> {
    executable_directory.ancestors().find_map(|ancestor| {
        let tauri_directory = ancestor.join("src-tauri");
        (tauri_directory.join("tauri.conf.json").is_file()
            && ancestor.join("package.json").is_file())
        .then(|| ancestor.to_path_buf())
    })
}

fn parent_of(path: &Path) -> Result<&Path, AppError> {
    path.parent().ok_or_else(|| {
        AppError::PathResolution(format!("{} has no parent directory", path.display()))
    })
}

fn copy_if_missing(source: &Path, destination: &Path) -> Result<(), AppError> {
    if destination.exists() || !source.is_file() {
        return Ok(());
    }
    let parent = parent_of(destination)?;
    std::fs::create_dir_all(parent).map_err(|error| AppError::file_system(parent, error))?;
    std::fs::copy(source, destination).map_err(|error| AppError::file_system(source, error))?;
    Ok(())
}
