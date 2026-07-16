use tauri::State;

use crate::{
    errors::{CommandError, CommandResult},
    models::{ModListItem, ModScanResult, UpdateLocalModMetadata},
    services::AppServices,
};

#[tauri::command]
pub async fn scan_mod_repository(services: State<'_, AppServices>) -> CommandResult<ModScanResult> {
    services
        .scan_mod_repository()
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_installed_mods(
    services: State<'_, AppServices>,
) -> CommandResult<Vec<ModListItem>> {
    services
        .list_installed_mods()
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_local_mod_metadata(
    request: UpdateLocalModMetadata,
    services: State<'_, AppServices>,
) -> CommandResult<ModListItem> {
    services
        .update_local_mod_metadata(request.mod_id, request.metadata)
        .await
        .map_err(CommandError::from)
}
