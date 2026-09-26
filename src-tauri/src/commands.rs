use pmgr_core::vault::Vault;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroize;

use crate::state::AppState;

pub fn vault_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("vault.enc"))
}

#[derive(serde::Serialize)]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
}

#[tauri::command]
pub fn vault_status(app: AppHandle, state: State<'_, AppState>) -> Result<VaultStatus, String> {
    state.touch();
    let path = vault_path(&app)?;
    let unlocked = state.vault.lock().unwrap().is_some();
    Ok(VaultStatus { exists: path.exists(), unlocked })
}

#[tauri::command]
pub fn vault_create(
    app: AppHandle,
    state: State<'_, AppState>,
    mut master_password: String,
) -> Result<(), String> {
    state.touch();
    if master_password.len() < 8 {
        master_password.zeroize();
        return Err("主密码至少 8 位".into());
    }
    let path = vault_path(&app)?;
    if path.exists() {
        return Err("数据文件已存在，不能重复创建".into());
    }
    let result = Vault::create(&path, &master_password).map_err(|e| e.to_string());
    master_password.zeroize();
    *state.vault.lock().unwrap() = Some(result?);
    Ok(())
}

#[tauri::command]
pub fn vault_unlock(
    app: AppHandle,
    state: State<'_, AppState>,
    mut master_password: String,
) -> Result<(), String> {
    state.touch();
    let path = vault_path(&app)?;
    // 安全要求：错误统一为笼统文案，不区分密码错/文件损坏
    let result = Vault::open(&path, &master_password)
        .map_err(|_| "主密码错误，或数据文件已损坏".to_string());
    master_password.zeroize();
    *state.vault.lock().unwrap() = Some(result?);
    Ok(())
}

#[tauri::command]
pub fn vault_lock(state: State<'_, AppState>) -> Result<(), String> {
    *state.vault.lock().unwrap() = None; // Vault 的 Drop 自动 zeroize
    Ok(())
}

#[tauri::command]
pub fn activity_ping(state: State<'_, AppState>) -> Result<(), String> {
    state.touch();
    Ok(())
}
