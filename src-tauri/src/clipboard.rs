//! 剪贴板：复制 + 定时安全清除。
//! 安全要求（Review Focus #4）：清除前必须比较内容，只清除仍等于原文本的情况。

use arboard::Clipboard;
use std::time::Duration;

pub fn copy_text(text: &str) -> Result<(), String> {
    let mut cb = Clipboard::new().map_err(|e| format!("无法访问剪贴板: {e}"))?;
    cb.set_text(text.to_string()).map_err(|e| format!("写入剪贴板失败: {e}"))
}

/// 到期后若剪贴板内容仍为 text 则清空；若用户已复制别的内容则不动。
pub fn schedule_clear(text: String, seconds: u32) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(u64::from(seconds)));
        if let Ok(mut cb) = Clipboard::new() {
            if let Ok(current) = cb.get_text() {
                if current == text {
                    let _ = cb.set_text(String::new());
                }
            }
        }
        // text 随线程结束被释放
    });
}
