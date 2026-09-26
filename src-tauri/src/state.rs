use pmgr_core::vault::Vault;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub struct AppState {
    /// None = 锁定状态。Vault 的 Drop 会自动 zeroize 密钥与明文。
    pub vault: Mutex<Option<Vault>>,
    pub last_activity: Mutex<Instant>,
}

impl AppState {
    pub fn new() -> Self {
        Self { vault: Mutex::new(None), last_activity: Mutex::new(Instant::now()) }
    }

    pub fn touch(&self) {
        *self.last_activity.lock().unwrap() = Instant::now();
    }
}

/// 纯函数便于测试：timeout_minutes = 0 表示关闭自动锁定。
pub fn should_auto_lock(elapsed: Duration, timeout_minutes: u32) -> bool {
    timeout_minutes > 0 && elapsed > Duration::from_secs(u64::from(timeout_minutes) * 60)
}

/// Rust 侧兜底自动锁定：每 30 秒检查一次；超时则锁定并通知前端回到解锁屏。
pub fn spawn_auto_lock_thread(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let state = app.state::<AppState>();
        let timeout = state
            .vault
            .lock()
            .unwrap()
            .as_ref()
            .map(|v| v.data.settings.auto_lock_minutes)
            .unwrap_or(0);
        if timeout == 0 {
            continue;
        }
        let elapsed = state.last_activity.lock().unwrap().elapsed();
        if should_auto_lock(elapsed, timeout) {
            *state.vault.lock().unwrap() = None;
            let _ = app.emit("vault-locked", "timeout");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::should_auto_lock;
    use std::time::Duration;

    #[test]
    fn auto_lock_rules() {
        assert!(!should_auto_lock(Duration::from_secs(100), 0)); // 0 = 关闭
        assert!(!should_auto_lock(Duration::from_secs(60), 15)); // 未超时
        assert!(should_auto_lock(Duration::from_secs(16 * 60), 15)); // 超时
    }
}
