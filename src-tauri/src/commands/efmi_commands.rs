use std::path::PathBuf;

use tauri::State;

use crate::{
    errors::{CommandError, CommandResult},
    models::GameStatus,
    services::AppServices,
};

#[tauri::command]
pub async fn set_efmi_mods_directory(
    path: PathBuf,
    services: State<'_, AppServices>,
) -> CommandResult<GameStatus> {
    services
        .configure_efmi_loader(Some(&path))
        .await
        .map_err(CommandError::from)
}
