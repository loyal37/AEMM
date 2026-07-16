use std::{
    cmp::Reverse,
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    core::game::{EfmiAdapter, EndfieldAdapter, GameAdapter},
    errors::AppError,
    models::{
        DetectedGameInstallation, EfmiValidation, GameDiscoverySource, GameLaunchResult,
        GameStatus, GameValidation, LaunchMode, LaunchSpec,
    },
};

use super::SettingsService;

#[derive(Debug)]
pub struct GameService {
    settings: SettingsService,
    game_adapter: EndfieldAdapter,
    efmi_adapter: EfmiAdapter,
}

impl GameService {
    pub fn new(settings: SettingsService) -> Self {
        Self {
            settings,
            game_adapter: EndfieldAdapter::new(),
            efmi_adapter: EfmiAdapter::new(),
        }
    }

    pub async fn detect_installations(&self) -> Result<Vec<DetectedGameInstallation>, AppError> {
        tracing::info!("starting Endfield installation discovery");
        let settings = self.settings.get().await;
        let mut detected = self.game_adapter.discover().await?;

        if let Some(configured_path) = settings.game.installation_path {
            let validation = self.game_adapter.validate(&configured_path).await?;
            if validation.valid {
                detected.push(DetectedGameInstallation {
                    source: GameDiscoverySource::ConfiguredPath,
                    validation,
                });
            }
        }

        let mut seen = HashSet::new();
        detected.retain(|candidate| {
            candidate
                .validation
                .installation
                .as_ref()
                .is_some_and(|installation| {
                    seen.insert(normalized_path_key(&installation.installation_root))
                })
        });
        detected.sort_by_key(|candidate| Reverse(candidate.validation.confidence));
        tracing::info!(
            count = detected.len(),
            "game installation discovery completed"
        );
        Ok(detected)
    }

    pub async fn validate_installation(&self, path: &Path) -> Result<GameValidation, AppError> {
        self.game_adapter.validate(path).await
    }

    pub async fn configure_installation(&self, path: &Path) -> Result<GameStatus, AppError> {
        let validation = self.game_adapter.validate(path).await?;
        let installation = validation.installation.as_ref().ok_or_else(|| {
            AppError::GameValidation(first_issue(
                &validation.issues,
                "所选目录不是可识别的终末地游戏目录。",
            ))
        })?;

        let mut settings = self.settings.get().await;
        settings.game.adapter_id = self.game_adapter.adapter_id().to_owned();
        settings.game.edition = Some(edition_config_value(installation.edition).to_owned());
        settings.game.installation_path = Some(installation.installation_root.clone());
        self.settings.update_game(settings.game).await?;
        tracing::info!(
            game_root = %installation.installation_root.display(),
            confidence = validation.confidence,
            "game installation configured"
        );
        self.status().await
    }

    pub async fn configure_loader(&self, path: Option<&Path>) -> Result<GameStatus, AppError> {
        let mut settings = self.settings.get().await;
        if let Some(path) = path {
            let installation = self.require_valid_installation(&settings).await?;
            let validation = self
                .efmi_adapter
                .validate(path, &installation.executable)
                .await?;
            let root = validation.root.as_ref().ok_or_else(|| {
                AppError::LoaderValidation(first_issue(
                    &validation.issues,
                    "所选目录不是可识别的 EFMI 加载器目录。",
                ))
            })?;
            settings.game.loader_root = Some(root.clone());
            tracing::info!(loader_root = %root.display(), "EFMI loader configured");
        } else {
            settings.game.loader_root = None;
            tracing::info!("EFMI loader configuration cleared");
        }
        self.settings.update_game(settings.game).await?;
        self.status().await
    }

    pub async fn set_launch_mode(&self, launch_mode: LaunchMode) -> Result<GameStatus, AppError> {
        let mut settings = self.settings.get().await;
        settings.game.launch_mode = launch_mode;
        self.settings.update_game(settings.game).await?;
        tracing::info!(?launch_mode, "game launch mode updated");
        self.status().await
    }

    pub async fn status(&self) -> Result<GameStatus, AppError> {
        let settings = self.settings.get().await;
        let installation = match settings.game.installation_path.as_deref() {
            Some(path) => Some(self.game_adapter.validate(path).await?),
            None => None,
        };

        let loader = match (
            settings.game.loader_root.as_deref(),
            installation
                .as_ref()
                .and_then(|validation| validation.installation.as_ref()),
        ) {
            (Some(loader_root), Some(installation)) => Some(
                self.efmi_adapter
                    .validate(loader_root, &installation.executable)
                    .await?,
            ),
            _ => None,
        };

        let (can_launch, launch_block_reason) = launch_availability(
            settings.game.launch_mode,
            installation.as_ref(),
            loader.as_ref(),
        );

        Ok(GameStatus {
            configured: installation.as_ref().is_some_and(|value| value.valid),
            installation,
            loader,
            launch_mode: settings.game.launch_mode,
            can_launch,
            launch_block_reason,
        })
    }

    pub async fn validated_game_directory(&self) -> Result<PathBuf, AppError> {
        let settings = self.settings.get().await;
        Ok(self
            .require_valid_installation(&settings)
            .await?
            .installation_root)
    }

    pub async fn resolve_launch_spec(&self) -> Result<LaunchSpec, AppError> {
        let settings = self.settings.get().await;
        self.resolve_launch_spec_for(&settings).await
    }

    async fn resolve_launch_spec_for(
        &self,
        settings: &crate::models::AppSettings,
    ) -> Result<LaunchSpec, AppError> {
        let installation = self.require_valid_installation(settings).await?;

        match settings.game.launch_mode {
            LaunchMode::Game => self.game_adapter.launch_spec(&installation).await,
            LaunchMode::EfmiLoader => {
                let loader_root = settings.game.loader_root.as_deref().ok_or_else(|| {
                    AppError::NotAvailable("请先配置 EFMI 加载器目录。".to_owned())
                })?;
                let loader = self
                    .efmi_adapter
                    .validate(loader_root, &installation.executable)
                    .await?;
                self.efmi_adapter.launch_spec(&loader)
            }
            LaunchMode::ExternalLauncher => Err(AppError::NotAvailable(
                "外部启动器模式尚未确认安全的调用协议，请改用直接启动或 EFMI。".to_owned(),
            )),
        }
    }

    pub async fn launch(&self) -> Result<GameLaunchResult, AppError> {
        let settings = self.settings.get().await;
        let mode = settings.game.launch_mode;
        let spec = self.resolve_launch_spec_for(&settings).await?;
        let spec = tokio::task::spawn_blocking(move || validate_launch_spec(spec)).await??;

        tracing::info!(
            mode = ?mode,
            executable = %spec.executable.display(),
            working_directory = %spec.working_directory.display(),
            "starting configured game process"
        );
        let child = Command::new(&spec.executable)
            .current_dir(&spec.working_directory)
            .args(&spec.arguments)
            .spawn()
            .map_err(|source| AppError::ProcessLaunch {
                path: spec.executable,
                source,
            })?;

        Ok(GameLaunchResult {
            process_id: child.id(),
            mode,
        })
    }

    async fn require_valid_installation(
        &self,
        settings: &crate::models::AppSettings,
    ) -> Result<crate::models::GameInstallation, AppError> {
        let path = settings
            .game
            .installation_path
            .as_deref()
            .ok_or_else(|| AppError::NotAvailable("请先配置终末地游戏目录。".to_owned()))?;
        let validation = self.game_adapter.validate(path).await?;
        validation.installation.ok_or_else(|| {
            AppError::GameValidation(first_issue(
                &validation.issues,
                "已保存的游戏目录不再有效，请重新选择。",
            ))
        })
    }
}

fn launch_availability(
    mode: LaunchMode,
    installation: Option<&GameValidation>,
    loader: Option<&EfmiValidation>,
) -> (bool, Option<String>) {
    if !installation.is_some_and(|value| value.valid) {
        return (false, Some("请先配置有效的游戏目录。".to_owned()));
    }

    match mode {
        LaunchMode::Game => (true, None),
        LaunchMode::EfmiLoader if loader.is_some_and(|value| value.valid && value.launch_ready) => {
            (true, None)
        }
        LaunchMode::EfmiLoader => (
            false,
            Some("请配置 EFMI，并确保 d3dx.ini 的 launch 路径指向当前游戏。".to_owned()),
        ),
        LaunchMode::ExternalLauncher => (false, Some("外部启动器协议尚未完成适配。".to_owned())),
    }
}

fn validate_launch_spec(spec: LaunchSpec) -> Result<LaunchSpec, AppError> {
    let working_directory = std::fs::canonicalize(&spec.working_directory)
        .map_err(|source| AppError::file_system(&spec.working_directory, source))?;
    let executable = std::fs::canonicalize(&spec.executable)
        .map_err(|source| AppError::file_system(&spec.executable, source))?;
    if !working_directory.is_dir()
        || !executable.is_file()
        || executable.parent() != Some(working_directory.as_path())
    {
        return Err(AppError::UnsafePath(
            "启动程序必须是已验证工作目录中的直属文件。".to_owned(),
        ));
    }

    Ok(LaunchSpec {
        executable,
        working_directory,
        arguments: spec.arguments,
    })
}

fn first_issue(issues: &[String], fallback: &str) -> String {
    issues
        .first()
        .cloned()
        .unwrap_or_else(|| fallback.to_owned())
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

const fn edition_config_value(edition: crate::models::GameEdition) -> &'static str {
    match edition {
        crate::models::GameEdition::China => "china",
        crate::models::GameEdition::International => "international",
        crate::models::GameEdition::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::{
        models::LaunchMode,
        services::{AppPaths, GameService, SettingsService},
    };

    fn create_game_fixture(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(root.join("Endfield_Data"))?;
        fs::write(root.join("Endfield.exe"), b"fixture")?;
        fs::write(
            root.join("Endfield_Data/app.info"),
            b"Hypergryph\nEndfield\n",
        )?;
        fs::write(root.join("UnityPlayer.dll"), b"fixture")?;
        fs::write(root.join("GameAssembly.dll"), b"fixture")?;
        Ok(())
    }

    async fn service(root: &Path) -> Result<GameService, Box<dyn std::error::Error>> {
        let paths = AppPaths::for_test(root);
        paths.ensure_base_directories().await?;
        let settings = SettingsService::load_or_create(&paths).await?;
        Ok(GameService::new(settings))
    }

    #[tokio::test]
    async fn persists_only_validated_game_root() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let game = root.path().join("game");
        create_game_fixture(&game)?;
        let service = service(&root.path().join("app")).await?;

        let status = service.configure_installation(&game).await?;
        assert!(status.configured);
        assert!(status.installation.is_some_and(|value| value.valid));
        Ok(())
    }

    #[tokio::test]
    async fn direct_launch_spec_is_contained_without_starting_process()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let game = root.path().join("game");
        create_game_fixture(&game)?;
        let service = service(&root.path().join("app")).await?;
        service.configure_installation(&game).await?;
        service.set_launch_mode(LaunchMode::Game).await?;

        let spec = service.resolve_launch_spec().await?;
        assert_eq!(
            spec.executable.parent(),
            Some(spec.working_directory.as_path())
        );
        assert!(spec.arguments.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_invalid_game_root_without_persisting_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let invalid = root.path().join("invalid");
        fs::create_dir_all(&invalid)?;
        fs::write(invalid.join("Endfield.exe"), b"fixture")?;
        let service = service(&root.path().join("app")).await?;

        assert!(service.configure_installation(&invalid).await.is_err());
        assert!(!service.status().await?.configured);
        Ok(())
    }
}
