use tauri::State;

use crate::{
    errors::{CommandError, CommandResult},
    models::{
        CopyProfile, CreateProfile, Profile, ProfileOperation, ProfileSwitchResult, RenameProfile,
    },
    services::AppServices,
};

#[tauri::command]
pub async fn list_profiles(services: State<'_, AppServices>) -> CommandResult<Vec<Profile>> {
    services.list_profiles().await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_profile(
    request: CreateProfile,
    services: State<'_, AppServices>,
) -> CommandResult<Profile> {
    services
        .create_profile(request.name)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn rename_profile(
    request: RenameProfile,
    services: State<'_, AppServices>,
) -> CommandResult<Profile> {
    services
        .rename_profile(request.profile_id, request.name)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn copy_profile(
    request: CopyProfile,
    services: State<'_, AppServices>,
) -> CommandResult<Profile> {
    services
        .copy_profile(request.source_profile_id, request.name)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn delete_profile(
    request: ProfileOperation,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    services
        .delete_profile(request.profile_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn switch_profile(
    request: ProfileOperation,
    services: State<'_, AppServices>,
) -> CommandResult<ProfileSwitchResult> {
    services
        .switch_profile(request.profile_id)
        .await
        .map_err(CommandError::from)
}
