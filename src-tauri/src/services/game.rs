use std::path::{Path, PathBuf};

use crate::{core::game::EfmiAdapter, errors::AppError, models::GameStatus};

use super::SettingsService;

/// Transitional EFMI configuration service.
///
/// The public product no longer manages or launches the game. The legacy
/// `GameStatus` transport shape is kept for config compatibility until the
/// next settings-schema migration.
#[derive(Debug)]
pub struct GameService {
    settings: SettingsService,
    efmi_adapter: EfmiAdapter,
}

impl GameService {
    pub fn new(settings: SettingsService) -> Self {
        Self {
            settings,
            efmi_adapter: EfmiAdapter::new(),
        }
    }

    pub async fn configure_loader(&self, path: Option<&Path>) -> Result<GameStatus, AppError> {
        let mut settings = self.settings.get().await;
        if let Some(path) = path {
            let candidate = if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Mods"))
            {
                path.parent().ok_or_else(|| {
                    AppError::LoaderValidation("所选 Mods 目录缺少 EFMI 父目录。".to_owned())
                })?
            } else {
                path
            };
            let validation = self.efmi_adapter.validate(candidate).await?;
            let root = validation.root.as_ref().ok_or_else(|| {
                AppError::LoaderValidation(first_issue(
                    &validation.issues,
                    "所选目录不是可识别的 EFMI 根目录或 Mods 目录。",
                ))
            })?;
            settings.game.loader_root = Some(root.clone());
            tracing::info!(efmi_root = %root.display(), mods_root = %root.join("Mods").display(), "EFMI Mods configured");
        } else {
            settings.game.loader_root = None;
        }
        self.settings.update_game(settings.game).await?;
        self.status().await
    }

    pub async fn status(&self) -> Result<GameStatus, AppError> {
        let settings = self.settings.get().await;
        let loader = match settings.game.loader_root.as_deref() {
            Some(root) => Some(self.efmi_adapter.validate(root).await?),
            None => None,
        };
        Ok(GameStatus {
            configured: loader.as_ref().is_some_and(|validation| validation.valid),
            installation: None,
            loader,
            launch_mode: settings.game.launch_mode,
            can_launch: false,
            launch_block_reason: Some("AEMM 仅管理 EFMI Mods，不提供游戏启动功能。".to_owned()),
        })
    }

    pub async fn validated_efmi_root(&self) -> Result<PathBuf, AppError> {
        let settings = self.settings.get().await;
        let root = settings.game.loader_root.as_deref().ok_or_else(|| {
            AppError::NotAvailable("请先在设置中选择 EFMI Mods 目录。".to_owned())
        })?;
        let validation = self.efmi_adapter.validate(root).await?;
        if !validation.valid {
            return Err(AppError::LoaderValidation(first_issue(
                &validation.issues,
                "已保存的 EFMI Mods 目录不再有效，请重新配置。",
            )));
        }
        validation.root.ok_or_else(|| {
            AppError::DataIntegrity("EFMI 校验成功但未返回规范化根目录。".to_owned())
        })
    }
}

fn first_issue(issues: &[String], fallback: &str) -> String {
    issues
        .first()
        .cloned()
        .unwrap_or_else(|| fallback.to_owned())
}
