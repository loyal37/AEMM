use tauri::State;

use crate::{
    errors::{CommandError, CommandResult},
    models::AppSettings,
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
