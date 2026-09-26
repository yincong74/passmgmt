use pmgr_core::generator::{self, GenOptions};
use pmgr_core::vault::{self, Entry, Settings, Vault};
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

// ---------- 受保护命令（需解锁） ----------

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn with_vault<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Vault) -> Result<T, String>,
) -> Result<T, String> {
    state.touch();
    let mut guard = state.vault.lock().unwrap();
    let vault = guard.as_mut().ok_or("未解锁")?;
    f(vault)
}

#[tauri::command]
pub fn entries_list(state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    with_vault(&state, |v| Ok(v.data.entries.clone()))
}

#[tauri::command]
pub fn entry_save(state: State<'_, AppState>, entry: Entry) -> Result<Entry, String> {
    with_vault(&state, |v| {
        if entry.app_name.trim().is_empty() {
            return Err("应用名不能为空".into());
        }
        let now = now_secs();
        if entry.id.is_empty() {
            let e = Entry::new(
                entry.app_name, entry.username, entry.password,
                entry.url, entry.notes, now,
            );
            v.data.entries.push(e.clone());
            v.save().map_err(|err| err.to_string())?;
            Ok(e)
        } else {
            let existing = v
                .data
                .entries
                .iter_mut()
                .find(|x| x.id == entry.id)
                .ok_or("条目不存在")?;
            existing.app_name = entry.app_name;
            existing.username = entry.username;
            existing.password = entry.password;
            existing.url = entry.url;
            existing.notes = entry.notes;
            existing.updated_at = now;
            let saved = existing.clone();
            v.save().map_err(|err| err.to_string())?;
            Ok(saved)
        }
    })
}

#[tauri::command]
pub fn entry_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_vault(&state, |v| {
        let before = v.data.entries.len();
        v.data.entries.retain(|e| e.id != id);
        if v.data.entries.len() == before {
            return Err("条目不存在".into());
        }
        v.save().map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<Settings, String> {
    with_vault(&state, |v| Ok(v.data.settings.clone()))
}

#[tauri::command]
pub fn settings_save(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    with_vault(&state, |v| {
        v.data.settings = settings;
        v.save().map_err(|err| err.to_string())
    })
}

#[tauri::command]
pub fn password_generate(opts: GenOptions) -> Result<String, String> {
    generator::generate(&opts).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn backup_export(
    app: AppHandle,
    state: State<'_, AppState>,
    dest_path: String,
) -> Result<(), String> {
    with_vault(&state, |_| {
        let src = vault_path(&app)?;
        vault::export_backup(&src, std::path::Path::new(&dest_path)).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn backup_import(
    state: State<'_, AppState>,
    src_path: String,
    mut backup_password: String,
) -> Result<(), String> {
    let result = with_vault(&state, |v| {
        v.import_backup(std::path::Path::new(&src_path), &backup_password)
            .map_err(|_| "备份文件验证失败：主密码错误或文件已损坏".to_string())
    });
    backup_password.zeroize();
    result
}

#[tauri::command]
pub fn master_password_change(
    state: State<'_, AppState>,
    mut old_password: String,
    mut new_password: String,
) -> Result<(), String> {
    if new_password.len() < 8 {
        return Err("新主密码至少 8 位".into());
    }
    let result = with_vault(&state, |v| {
        v.change_password(&old_password, &new_password).map_err(|e| match e {
            vault::VaultError::WrongPasswordOrCorrupt => "旧主密码不正确".to_string(),
            other => other.to_string(),
        })
    });
    old_password.zeroize();
    new_password.zeroize();
    result
}

#[tauri::command]
pub fn clipboard_copy(
    state: State<'_, AppState>,
    text: String,
    secret: bool,
) -> Result<(), String> {
    state.touch();
    crate::clipboard::copy_text(&text)?;
    if secret {
        let secs = state
            .vault
            .lock()
            .unwrap()
            .as_ref()
            .map(|v| v.data.settings.clipboard_clear_seconds)
            .unwrap_or(0);
        if secs > 0 {
            crate::clipboard::schedule_clear(text, secs);
        }
    }
    Ok(())
}
