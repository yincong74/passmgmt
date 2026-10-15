//! 应用组合根：Builder、插件与 IPC handler 注册（桌面 bin 与 Android JNI 共用入口）。
mod clipboard;
mod commands;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init());
    #[cfg(mobile)]
    let builder = builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_biometric::init())
        .plugin(tauri_plugin_keystore::init());
    builder
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
