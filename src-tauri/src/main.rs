#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod commands;
mod state;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::new())
        .setup(|app| {
            state::spawn_auto_lock_thread(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::vault_create,
            commands::vault_unlock,
            commands::vault_lock,
            commands::activity_ping,
            commands::entries_list,
            commands::entry_save,
            commands::entry_delete,
            commands::settings_get,
            commands::settings_save,
            commands::password_generate,
            commands::backup_export,
            commands::backup_import,
            commands::master_password_change,
            commands::clipboard_copy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
