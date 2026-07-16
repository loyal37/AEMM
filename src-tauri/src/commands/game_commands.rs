use std::path::PathBuf;

use tauri::State;

use crate::{
    errors::{AppError, CommandError, CommandResult},
    models::{DetectedGameInstallation, GameLaunchResult, GameStatus, LaunchMode},
    services::AppServices,
};

#[tauri::command]
pub async fn get_game_status(services: State<'_, AppServices>) -> CommandResult<GameStatus> {
    services.game_status().await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn detect_game_installations(
    services: State<'_, AppServices>,
) -> CommandResult<Vec<DetectedGameInstallation>> {
    services
        .detect_game_installations()
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_game_installation(
    path: PathBuf,
    services: State<'_, AppServices>,
) -> CommandResult<GameStatus> {
    services
        .configure_game_installation(&path)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_efmi_loader_root(
    path: Option<PathBuf>,
    services: State<'_, AppServices>,
) -> CommandResult<GameStatus> {
    services
        .configure_efmi_loader(path.as_deref())
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_game_launch_mode(
    launch_mode: LaunchMode,
    services: State<'_, AppServices>,
) -> CommandResult<GameStatus> {
    services
        .set_game_launch_mode(launch_mode)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn open_game_directory(services: State<'_, AppServices>) -> CommandResult<()> {
    let path = services
        .validated_game_directory()
        .await
        .map_err(CommandError::from)?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| {
        CommandError::from(AppError::NotAvailable(format!("无法打开游戏目录：{error}")))
    })
}

#[tauri::command]
pub async fn launch_game(services: State<'_, AppServices>) -> CommandResult<GameLaunchResult> {
    services.launch_game().await.map_err(CommandError::from)
}
