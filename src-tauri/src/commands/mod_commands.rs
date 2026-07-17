use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::{
    errors::{CommandError, CommandResult},
    models::{
        ModDeploymentMutationResult, ModDetails, ModImportOperation, ModImportPlan,
        ModInstallProgress, ModInstallResult, ModListItem, ModMutationResult, ModPreview,
        ModRemovalResult, ModScanResult, PrepareModImport, RemoveMods, SetModFavorite,
        SetModsEnabled, UpdateLocalModMetadata,
    },
    services::AppServices,
};

const MOD_INSTALL_PROGRESS_EVENT: &str = "mod-install-progress";

#[tauri::command]
pub async fn prepare_mod_import(
    request: PrepareModImport,
    app: AppHandle,
    services: State<'_, AppServices>,
) -> CommandResult<ModImportPlan> {
    services
        .prepare_mod_import(request.source_path, progress_reporter(app))
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn commit_mod_import(
    request: ModImportOperation,
    app: AppHandle,
    services: State<'_, AppServices>,
) -> CommandResult<ModInstallResult> {
    services
        .commit_mod_import(request.operation_id, progress_reporter(app))
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn cancel_mod_import(
    request: ModImportOperation,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    services
        .cancel_mod_import(request.operation_id)
        .await
        .map_err(CommandError::from)
}

fn progress_reporter(app: AppHandle) -> crate::core::mods::InstallProgressReporter {
    Arc::new(move |progress: ModInstallProgress| {
        if let Err(error) = app.emit(MOD_INSTALL_PROGRESS_EVENT, progress) {
            tracing::warn!(error = %error, "failed to emit mod installation progress");
        }
    })
}

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
pub async fn get_mod_details(
    mod_id: uuid::Uuid,
    services: State<'_, AppServices>,
) -> CommandResult<ModDetails> {
    services
        .mod_details(mod_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_mod_favorite(
    request: SetModFavorite,
    services: State<'_, AppServices>,
) -> CommandResult<ModMutationResult> {
    services
        .set_mod_favorite(request.mod_ids, request.favorite)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn uninstall_mods(
    request: RemoveMods,
    services: State<'_, AppServices>,
) -> CommandResult<ModRemovalResult> {
    services
        .uninstall_mods(request.mod_ids)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_mods_enabled(
    request: SetModsEnabled,
    services: State<'_, AppServices>,
) -> CommandResult<ModDeploymentMutationResult> {
    services
        .set_mods_enabled(request.mod_ids, request.enabled)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_mod_preview(
    mod_id: uuid::Uuid,
    services: State<'_, AppServices>,
) -> CommandResult<Option<ModPreview>> {
    services
        .mod_preview(mod_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn open_mod_directory(
    mod_id: uuid::Uuid,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    let path = services
        .mod_directory(mod_id)
        .await
        .map_err(CommandError::from)?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| {
        CommandError::from(crate::errors::AppError::NotAvailable(format!(
            "无法打开模组目录：{error}"
        )))
    })
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
