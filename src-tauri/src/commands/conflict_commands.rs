use tauri::State;

use crate::{
    errors::{CommandError, CommandResult},
    models::ConflictReport,
    services::AppServices,
};

#[tauri::command]
pub async fn get_active_conflict_report(
    services: State<'_, AppServices>,
) -> CommandResult<ConflictReport> {
    services
        .active_conflict_report()
        .await
        .map_err(CommandError::from)
}
