use pmgr_core::vault::Vault;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

/// 最后活动时间：单调钟 + 墙钟双记录。
/// Android 深睡时单调钟停走，墙钟补偿漏锁；墙钟被回拨时单调钟兜底。
#[derive(Clone, Copy)]
pub struct Activity {
    pub monotonic: Instant,
    pub wall: SystemTime,
}

impl Activity {
    fn now() -> Self {
        Self { monotonic: Instant::now(), wall: SystemTime::now() }
    }

    /// 两个钟各自的流逝时长（墙钟回拨时为 0）。
    pub fn elapsed(&self) -> (Duration, Duration) {
        let mono = self.monotonic.elapsed();
        let wall = SystemTime::now().duration_since(self.wall).unwrap_or(Duration::ZERO);
        (mono, wall)
    }
}

pub struct AppState {
    /// None = 锁定状态。Vault 的 Drop 会自动 zeroize 密钥与明文。
    pub vault: Mutex<Option<Vault>>,
    pub last_activity: Mutex<Activity>,
}

impl AppState {
    pub fn new() -> Self {
        Self { vault: Mutex::new(None), last_activity: Mutex::new(Activity::now()) }
    }

    pub fn touch(&self) {
        *self.last_activity.lock().unwrap() = Activity::now();
    }
}

/// 纯函数便于测试：timeout_minutes = 0 表示关闭自动锁定。
pub fn should_auto_lock(elapsed: Duration, timeout_minutes: u32) -> bool {
    timeout_minutes > 0 && elapsed > Duration::from_secs(u64::from(timeout_minutes) * 60)
}

/// 双钟版：取单调钟与墙钟流逝的较大者（安全方向：宁可早锁）。
pub fn should_auto_lock_dual(
    monotonic_elapsed: Duration,
    wall_elapsed: Duration,
    timeout_minutes: u32,
) -> bool {
    should_auto_lock(monotonic_elapsed.max(wall_elapsed), timeout_minutes)
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
        let (mono, wall) = state.last_activity.lock().unwrap().elapsed();
        if should_auto_lock_dual(mono, wall, timeout) {
            *state.vault.lock().unwrap() = None;
            let _ = app.emit("vault-locked", "timeout");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{should_auto_lock, should_auto_lock_dual};
    use std::time::Duration;

    #[test]
    fn auto_lock_rules() {
        assert!(!should_auto_lock(Duration::from_secs(100), 0)); // 0 = 关闭
        assert!(!should_auto_lock(Duration::from_secs(60), 15)); // 未超时
        assert!(should_auto_lock(Duration::from_secs(16 * 60), 15)); // 超时
    }

    #[test]
    fn dual_clock_takes_max() {
        // 深睡场景：单调钟只走了 1 分钟、墙钟走了 20 分钟 → 应锁定
        assert!(should_auto_lock_dual(Duration::from_secs(60), Duration::from_secs(20 * 60), 15));
        // 两者都未超时 → 不锁
        assert!(!should_auto_lock_dual(Duration::from_secs(60), Duration::from_secs(60), 15));
        // 墙钟被回拨（负值按 0 处理）→ 以单调钟为准
        assert!(!should_auto_lock_dual(Duration::from_secs(60), Duration::ZERO, 15));
        assert!(should_auto_lock_dual(Duration::from_secs(16 * 60), Duration::ZERO, 15));
        // 0 = 关闭，两个钟都超时也不锁
        assert!(!should_auto_lock_dual(Duration::from_secs(99 * 60), Duration::from_secs(99 * 60), 0));
    }
}
