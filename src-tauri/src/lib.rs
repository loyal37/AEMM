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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
            commands::settings_commands::update_settings,
            commands::game_commands::get_game_status,
            commands::game_commands::detect_game_installations,
            commands::game_commands::set_game_installation,
            commands::game_commands::set_efmi_loader_root,
            commands::game_commands::set_game_launch_mode,
            commands::game_commands::open_game_directory,
            commands::game_commands::launch_game,
            commands::mod_commands::scan_mod_repository,
            commands::mod_commands::prepare_mod_import,
            commands::mod_commands::commit_mod_import,
            commands::mod_commands::cancel_mod_import,
            commands::mod_commands::list_installed_mods,
            commands::mod_commands::get_mod_details,
            commands::mod_commands::set_mod_favorite,
            commands::mod_commands::set_mods_enabled,
            commands::mod_commands::get_mod_preview,
            commands::mod_commands::open_mod_directory,
            commands::mod_commands::update_local_mod_metadata,
            commands::conflict_commands::get_active_conflict_report
        ])
        .run(tauri::generate_context!());

    if let Err(error) = application {
        eprintln!("failed to run Endfield Mod Manager: {error}");
        std::process::exit(1);
    }
}
