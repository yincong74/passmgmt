//! 剪贴板：复制 + 定时安全清除。
//! 安全要求（Review Focus #4）：清除前必须比较内容，只清除仍等于原文本的情况。
//! 桌面用 arboard；Android 用 tauri-plugin-clipboard-manager（arboard 不支持 Android）。
//! 已知限制（spec §6.1）：Android 进程被冻结时，定时清除延迟到回前台执行。

use std::time::Duration;
use tauri::AppHandle;

#[cfg(desktop)]
pub fn copy_text(_app: &AppHandle, text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("无法访问剪贴板: {e}"))?;
    cb.set_text(text.to_string()).map_err(|e| format!("写入剪贴板失败: {e}"))
}

#[cfg(mobile)]
pub fn copy_text(app: &AppHandle, text: &str) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| format!("写入剪贴板失败: {e}"))
}

/// 到期后若剪贴板内容仍为 text 则清空；若用户已复制别的内容则不动。
pub fn schedule_clear(app: AppHandle, text: String, seconds: u32) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(u64::from(seconds)));
        let _ = clear_if_unchanged(&app, &text);
        // text 随线程结束被释放
    });
}

#[cfg(desktop)]
fn clear_if_unchanged(_app: &AppHandle, text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    if let Ok(current) = cb.get_text() {
        if current == text {
            let _ = cb.set_text(String::new());
        }
    }
    Ok(())
}

#[cfg(mobile)]
fn clear_if_unchanged(app: &AppHandle, text: &str) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let cb = app.clipboard();
    // 注意：2.3.3 版 read_text 返回 Result<String>（不是 Option）；读失败视为不可比较，不清除
    if let Ok(current) = cb.read_text() {
        if current == text {
            let _ = cb.write_text("");
        }
    }
    Ok(())
}
