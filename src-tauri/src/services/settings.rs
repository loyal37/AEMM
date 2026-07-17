use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use tempfile::NamedTempFile;
use tokio::sync::{Mutex, RwLock};

use crate::{
    errors::AppError,
    models::{AppSettings, CONFIG_SCHEMA_VERSION, GameSettings},
};

use super::AppPaths;

#[derive(Debug, Clone)]
pub struct SettingsService {
    config_file: PathBuf,
    settings: Arc<RwLock<AppSettings>>,
    write_lock: Arc<Mutex<()>>,
}

impl SettingsService {
    pub async fn load_or_create(paths: &AppPaths) -> Result<Self, AppError> {
        recover_interrupted_write(&paths.config_file).await?;

        let settings = if tokio::fs::try_exists(&paths.config_file)
            .await
            .map_err(|source| AppError::file_system(&paths.config_file, source))?
        {
            let contents = tokio::fs::read(&paths.config_file)
                .await
                .map_err(|source| AppError::file_system(&paths.config_file, source))?;
            serde_json::from_slice::<AppSettings>(&contents).map_err(AppError::ConfigFormat)?
        } else {
            AppSettings::defaults(
                paths.repository_directory.clone(),
                paths.staging_directory.clone(),
            )
        };

        validate_settings(&settings)?;
        prepare_storage_directories(&settings).await?;

        let service = Self {
            config_file: paths.config_file.clone(),
            settings: Arc::new(RwLock::new(settings)),
            write_lock: Arc::new(Mutex::new(())),
        };

        if !tokio::fs::try_exists(&service.config_file)
            .await
            .map_err(|source| AppError::file_system(&service.config_file, source))?
        {
            let initial = service.get().await;
            service.persist(&initial).await?;
        }

        Ok(service)
    }

    pub async fn get(&self) -> AppSettings {
        self.settings.read().await.clone()
    }

    pub async fn update(&self, settings: AppSettings) -> Result<AppSettings, AppError> {
        validate_settings(&settings)?;
        prepare_storage_directories(&settings).await?;

        let _write_guard = self.write_lock.lock().await;
        self.persist(&settings).await?;
        *self.settings.write().await = settings.clone();
        tracing::info!("application settings updated");
        Ok(settings)
    }

    pub async fn update_game(&self, game: GameSettings) -> Result<AppSettings, AppError> {
        let _write_guard = self.write_lock.lock().await;
        let mut settings = self.settings.read().await.clone();
        settings.game = game;
        validate_settings(&settings)?;
        prepare_storage_directories(&settings).await?;
        self.persist(&settings).await?;
        *self.settings.write().await = settings.clone();
        tracing::info!("game settings updated");
        Ok(settings)
    }

    async fn persist(&self, settings: &AppSettings) -> Result<(), AppError> {
        let mut json = serde_json::to_vec_pretty(settings).map_err(AppError::ConfigFormat)?;
        json.push(b'\n');
        let path = self.config_file.clone();

        tokio::task::spawn_blocking(move || atomic_write(&path, &json)).await??;
        Ok(())
    }
}

fn validate_settings(settings: &AppSettings) -> Result<(), AppError> {
    if settings.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(AppError::ConfigValidation(format!(
            "不支持的配置版本 {}，当前版本为 {}。",
            settings.schema_version, CONFIG_SCHEMA_VERSION
        )));
    }

    if !matches!(settings.language.as_str(), "zh-CN" | "en-US") {
        return Err(AppError::ConfigValidation(
            "当前仅支持 zh-CN 和 en-US 语言代码。".to_owned(),
        ));
    }

    if settings.game.adapter_id.trim().is_empty() || settings.game.adapter_id.len() > 128 {
        return Err(AppError::ConfigValidation(
            "游戏适配器 ID 无效。".to_owned(),
        ));
    }

    for (label, path) in [
        ("模组仓库", settings.storage.repository_path.as_path()),
        ("临时目录", settings.storage.staging_path.as_path()),
    ] {
        validate_absolute_path(label, path)?;
    }

    for (label, path) in [
        ("游戏目录", settings.game.installation_path.as_deref()),
        ("加载器目录", settings.game.loader_root.as_deref()),
    ] {
        if let Some(path) = path {
            validate_absolute_path(label, path)?;
        }
    }

    if paths_overlap_case_insensitive(
        &settings.storage.repository_path,
        &settings.storage.staging_path,
    ) {
        return Err(AppError::ConfigValidation(
            "模组仓库和临时目录不能相同或互相包含。".to_owned(),
        ));
    }

    Ok(())
}

fn validate_absolute_path(label: &str, path: &Path) -> Result<(), AppError> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(AppError::ConfigValidation(format!(
            "{label}必须是绝对路径。"
        )));
    }

    if path.to_string_lossy().contains('\0') {
        return Err(AppError::ConfigValidation(format!("{label}包含非法字符。")));
    }

    Ok(())
}

fn paths_overlap_case_insensitive(left: &Path, right: &Path) -> bool {
    let left = normalized_path_key(left);
    let right = normalized_path_key(right);

    left == right
        || left
            .strip_prefix(&right)
            .is_some_and(|suffix| suffix.starts_with('\\'))
        || right
            .strip_prefix(&left)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

async fn prepare_storage_directories(settings: &AppSettings) -> Result<(), AppError> {
    for path in [
        settings.storage.repository_path.as_path(),
        settings.storage.staging_path.as_path(),
    ] {
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|source| AppError::file_system(path, source))?;
    }

    let repository = tokio::fs::canonicalize(&settings.storage.repository_path)
        .await
        .map_err(|source| AppError::file_system(&settings.storage.repository_path, source))?;
    let staging = tokio::fs::canonicalize(&settings.storage.staging_path)
        .await
        .map_err(|source| AppError::file_system(&settings.storage.staging_path, source))?;

    if repository == staging || repository.starts_with(&staging) || staging.starts_with(&repository)
    {
        return Err(AppError::ConfigValidation(
            "模组仓库和临时目录解析后不能相同或互相包含。".to_owned(),
        ));
    }

    Ok(())
}

async fn recover_interrupted_write(config_file: &Path) -> Result<(), AppError> {
    let backup = backup_path(config_file);
    let config_exists = tokio::fs::try_exists(config_file)
        .await
        .map_err(|source| AppError::file_system(config_file, source))?;
    let backup_exists = tokio::fs::try_exists(&backup)
        .await
        .map_err(|source| AppError::file_system(&backup, source))?;

    if !config_exists && backup_exists {
        tokio::fs::rename(&backup, config_file)
            .await
            .map_err(|source| AppError::file_system(config_file, source))?;
    }

    Ok(())
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::PathResolution(format!("{} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|source| AppError::file_system(parent, source))?;

    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|source| AppError::file_system(parent, source))?;
    temporary
        .write_all(contents)
        .map_err(|source| AppError::file_system(path, source))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| AppError::file_system(path, source))?;

    let backup = backup_path(path);
    let had_existing_file = path.exists();

    if backup.exists() {
        fs::remove_file(&backup).map_err(|source| AppError::file_system(&backup, source))?;
    }

    if had_existing_file {
        fs::rename(path, &backup).map_err(|source| AppError::file_system(path, source))?;
    }

    if let Err(persist_error) = temporary.persist(path) {
        if had_existing_file && backup.exists() {
            if let Err(restore_error) = fs::rename(&backup, path) {
                tracing::error!(
                    write_error = %persist_error.error,
                    restore_error = %restore_error,
                    config_path = %path.display(),
                    "configuration write and rollback both failed"
                );
                return Err(AppError::file_system(path, restore_error));
            }
        }
        return Err(AppError::file_system(path, persist_error.error));
    }

    if had_existing_file && backup.exists() {
        fs::remove_file(&backup).map_err(|source| AppError::file_system(&backup, source))?;
    }

    Ok(())
}

fn backup_path(config_file: &Path) -> PathBuf {
    config_file.with_extension("json.bak")
}

#[cfg(test)]
mod tests {
    use crate::{models::ThemePreference, services::AppPaths};

    use super::SettingsService;

    #[tokio::test]
    async fn creates_and_updates_versioned_config() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let paths = AppPaths::for_test(directory.path());
        paths.ensure_base_directories().await?;

        let service = SettingsService::load_or_create(&paths).await?;
        let mut settings = service.get().await;
        assert_eq!(settings.schema_version, 1);

        settings.theme = ThemePreference::System;
        service.update(settings).await?;

        let reloaded = SettingsService::load_or_create(&paths).await?.get().await;
        assert_eq!(reloaded.theme, ThemePreference::System);
        Ok(())
    }

    #[tokio::test]
    async fn rejects_repository_and_staging_path_collision()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let paths = AppPaths::for_test(directory.path());
        paths.ensure_base_directories().await?;
        let service = SettingsService::load_or_create(&paths).await?;
        let mut settings = service.get().await;
        settings.storage.staging_path = settings.storage.repository_path.clone();

        let result = service.update(settings).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_nested_repository_and_staging_paths() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let paths = AppPaths::for_test(directory.path());
        paths.ensure_base_directories().await?;
        let service = SettingsService::load_or_create(&paths).await?;
        let mut settings = service.get().await;
        settings.storage.staging_path = settings.storage.repository_path.join("staging");

        let result = service.update(settings).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_unsupported_interface_language() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let paths = AppPaths::for_test(directory.path());
        paths.ensure_base_directories().await?;
        let service = SettingsService::load_or_create(&paths).await?;
        let mut settings = service.get().await;
        settings.language = "not-a-supported-locale".to_owned();

        let result = service.update(settings).await;

        assert!(result.is_err());
        Ok(())
    }
}
