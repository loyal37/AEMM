pub mod commands;
pub mod core;
pub mod database;
pub mod errors;
pub mod models;
pub mod services;
pub mod utils;

use services::AppServices;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let application = tauri::Builder::default()
        .setup(|app| {
            let services = tauri::async_runtime::block_on(AppServices::initialize(app.handle()))?;
            tracing::info!(
                version = app.package_info().version.to_string(),
                "application started"
            );
            app.manage(services);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_commands::get_app_bootstrap,
            commands::settings_commands::update_settings
        ])
        .run(tauri::generate_context!());

    if let Err(error) = application {
        eprintln!("failed to run Endfield Mod Manager: {error}");
        std::process::exit(1);
    }
}
