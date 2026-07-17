use tauri::State;

use crate::{
    errors::{CommandError, CommandResult},
    models::{AppSettings, StorageSettings},
    services::AppServices,
};

#[tauri::command]
pub async fn update_settings(
    settings: AppSettings,
    services: State<'_, AppServices>,
) -> CommandResult<AppSettings> {
    services
        .update_settings(settings)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_storage_paths(
    storage: StorageSettings,
    services: State<'_, AppServices>,
) -> CommandResult<AppSettings> {
    services
        .configure_storage(storage)
        .await
        .map_err(CommandError::from)
}
