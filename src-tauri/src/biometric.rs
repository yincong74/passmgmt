//! 生物识别解锁（仅 Android，spec §7）：启用标记管理 + 裸密钥解锁命令。
//! 密钥本体存于 Android Keystore（@impierce/tauri-plugin-keystore，硬件背书，
//! 存/取均强制生物识别），由前端 JS 桥接；Rust 侧只管理启用标记文件 biometric.json，
//! 其中不含任何密钥材料。

use pmgr_core::vault::{hex_decode_key, hex_encode, Vault};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroize;

use crate::commands::vault_path;
use crate::state::AppState;

/// Keystore 别名，前端（src/ui/mobile/bio.ts 的 KEY_ALIAS）必须与之一致。
#[allow(dead_code)] // Rust 侧不直接使用，仅作前端对齐的单一事实源
pub const KEYSTORE_ALIAS: &str = "pmgr_vault_key";
const FLAG_FILE: &str = "biometric.json";

#[derive(Serialize)]
struct Flag {
    enabled: bool,
}

fn flag_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join(FLAG_FILE))
}

/// 写/删启用标记。失效场景（改密、密钥不匹配、新建库）都走 set_flag(app, false)。
pub fn set_flag(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let p = flag_path(app)?;
    if enabled {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string(&Flag { enabled: true }).unwrap();
        std::fs::write(&p, json).map_err(|e| e.to_string())
    } else {
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[tauri::command]
pub fn biometric_enabled(app: AppHandle) -> Result<bool, String> {
    let p = flag_path(&app)?;
    Ok(p.exists())
}

#[tauri::command]
pub fn biometric_set_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_flag(&app, enabled)
}

/// 注册第一步：导出当前 vault 密钥（hex）。前端随后调用 keystore.store(KEY_ALIAS, hex)
/// 存入硬件 Keystore（弹生物识别确认），成功后调 biometric_set_enabled(true)。
/// 密钥字符串在 WebView 层短暂存在（spec §7.5 已明示该约束）。
#[tauri::command]
pub fn biometric_export_key(state: State<'_, AppState>) -> Result<String, String> {
    state.touch();
    let guard = state.vault.lock().unwrap();
    let vault = guard.as_ref().ok_or("未解锁")?;
    let key = vault.export_key();
    Ok(hex_encode(&key[..]))
}

/// 生物识别解锁：前端 retrieve() 拿到 hex 密钥后调用。
/// 密钥与文件不匹配（改密/数据重建）→ 自动失效（spec §7.4），提示改用主密码。
#[tauri::command]
pub fn vault_unlock_with_key(
    app: AppHandle,
    state: State<'_, AppState>,
    mut key_hex: String,
) -> Result<(), String> {
    state.touch();
    let path = vault_path(&app)?;
    let decoded = hex_decode_key(&key_hex);
    key_hex.zeroize();
    let mut key = decoded.ok_or("生物识别密钥已失效，请使用主密码解锁")?;
    let result = Vault::open_with_key(&path, &key)
        .map_err(|_| "生物识别密钥已失效，请使用主密码解锁".to_string());
    key.zeroize();
    if result.is_err() {
        // 自动失效：删标记；Keystore 残留密文由前端 remove() 清理（下次注册同名覆盖）
        let _ = set_flag(&app, false);
        return result.map(|_| ());
    }
    *state.vault.lock().unwrap() = Some(result.unwrap());
    Ok(())
}
