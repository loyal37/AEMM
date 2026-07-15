use tauri::State;

use crate::{
    errors::{CommandError, CommandResult},
    models::AppBootstrap,
    services::AppServices,
};

#[tauri::command]
pub async fn get_app_bootstrap(
    app: tauri::AppHandle,
    services: State<'_, AppServices>,
) -> CommandResult<AppBootstrap> {
    services
        .bootstrap(
            app.package_info().name.clone(),
            app.package_info().version.to_string(),
        )
        .await
        .map_err(CommandError::from)
}
