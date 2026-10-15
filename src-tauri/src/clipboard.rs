//! 剪贴板：复制 + 定时安全清除。
//! 安全要求（Review Focus #4）：清除前必须比较内容，只清除仍等于原文本的情况。
//! 桌面用 arboard；Android 用 tauri-plugin-clipboard-manager（arboard 不支持 Android）。
//! 已知限制（spec §6.1）：Android 进程被冻结时，定时清除延迟到回前台执行。
//! 已知限制（Android 10+）：应用不在前台聚焦时读取剪贴板会得到空内容或读取失败，
//! 此时比较必然不相等，定时器会静默跳过清除（fail-safe：宁可不清也绝不误 wipe
//! 用户新复制的数据；代价是该边缘情况下密钥可能残留在剪贴板上）。

use std::time::Duration;
use tauri::AppHandle;
use zeroize::Zeroize;

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

/// 读取当前剪贴板文本；读取失败返回 None（视为不可比较，后续不清除）。
#[cfg(desktop)]
fn read_current(_app: &AppHandle) -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    cb.get_text().ok()
}

/// 读取当前剪贴板文本；读取失败返回 None（视为不可比较，后续不清除）。
#[cfg(mobile)]
fn read_current(app: &AppHandle) -> Option<String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    // 注意：2.3.3 版 read_text 返回 Result<String>（不是 Option）；
    // Android 10+ 后台读取会失败或返回空，均走 None / 比较不等分支，不会误清。
    app.clipboard().read_text().ok()
}

/// 到期后若剪贴板内容仍为 text 则清空；若用户已复制别的内容则不动。
/// 「比较后清除」的安全约定全平台只此一份实现，平台差异收敛在 read_current / copy_text。
fn clear_if_unchanged(app: &AppHandle, text: &str) {
    if read_current(app).as_deref() == Some(text) {
        let _ = copy_text(app, "");
    }
}

/// 定时清除：text 的所有权移入本线程，清除判定结束后主动清零。
pub fn schedule_clear(app: AppHandle, text: String, seconds: u32) {
    std::thread::spawn(move || {
        let mut text = text;
        std::thread::sleep(Duration::from_secs(u64::from(seconds)));
        clear_if_unchanged(&app, &text);
        // text 随线程结束被释放，释放前先清零密钥明文
        text.zeroize();
    });
}
