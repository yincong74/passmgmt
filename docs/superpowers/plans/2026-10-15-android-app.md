# Android 版实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为现有 Tauri 2 桌面密码管理器增加功能对等的 Android 版本（含可选生物识别解锁、APK 侧载分发），桌面端行为与外观零变化。

**Architecture:** 单工程 + 平台条件编译（Tauri mobile 标准路线）：`src-tauri` 重构为 `lib.rs` 统一入口，`tauri android init` 生成 `gen/android` 提交进仓库；平台差异用 `#[cfg(desktop)]/#[cfg(mobile)]` 隔离；前端 `main.ts` 按平台分流，桌面 UI 原样保留，手机加载新的 `src/ui/mobile/`（底部 Tab + 页面栈导航），共享 `api.ts`/`state.ts` 与全部 Rust 命令。

**Tech Stack:** Tauri 2 mobile、`pmgr-core`（argon2/aes-gcm 不变）、tauri-plugin-clipboard-manager（Android 剪贴板）、tauri-plugin-os（平台检测）、tauri-plugin-fs + plugin-dialog（Android content URI 备份）、@tauri-apps/plugin-biometric + @impierce/tauri-plugin-keystore（生物识别 + Keystore 密钥托管）、原生 TS（无框架）、GitHub Actions（APK 产物）。

**Spec:** `docs/superpowers/specs/2026-10-15-android-app-design.md`（实施中如对 spec 有偏离，先改 spec 再继续）。

**环境注意:** 开发机为 Windows（PowerShell）。所有命令默认在仓库根 `D:\source_code\password` 执行。提交信息用中文、沿用现有 `feat/fix/docs/ci` 前缀风格。

---

## Task 1: Android 开发环境安装（需用户配合，无代码提交）

**Files:** 无（纯环境）。

- [ ] **Step 1: 安装 Android Studio**

```powershell
winget install -e --id Google.AndroidStudio
```

若 winget 不可用则从 https://developer.android.com/studio 下载安装。安装后**启动一次 Android Studio**，完成 Setup Wizard（默认标准安装即可，会装好 Android SDK 与 platform-tools）。

- [ ] **Step 2: 安装 NDK 与构建工具**

打开 Android Studio → Settings → Languages & Frameworks → Android SDK → SDK Tools 页签，勾选并安装：
- **NDK (Side by side)**
- **Android SDK Build-Tools**（最新稳定版）
- **Android SDK Command-line Tools**

- [ ] **Step 3: 添加 Rust Android target 与 cargo-ndk**

```powershell
rustup target add aarch64-linux-android
cargo install cargo-ndk --locked
```

- [ ] **Step 4: 设置环境变量**

NDK 目录名以实际安装版本为准（`dir $env:LOCALAPPDATA\Android\Sdk\ndk` 查看）：

```powershell
setx ANDROID_HOME "$env:LOCALAPPDATA\Android\Sdk"
setx NDK_HOME "$env:LOCALAPPDATA\Android\Sdk\ndk\<版本号>"
setx JAVA_HOME "C:\Program Files\Android\Android Studio\jbr"
```

设置后**重开终端**（setx 不影响当前会话）。

- [ ] **Step 5: 验证**

```powershell
echo $env:ANDROID_HOME; Test-Path $env:NDK_HOME; & "$env:JAVA_HOME\bin\java" -version
rustup target list --installed | Select-String android
cargo ndk --version
```

预期：路径存在；java 版本 ≥ 17；已安装 `aarch64-linux-android`；cargo-ndk 输出版本号。全部通过才能继续 Task 2+（Task 2–5 不依赖 Android 环境，可先行）。

---

## Task 2: Rust 入口重构（main.rs → lib.rs），桌面回归

**Files:**
- Create: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/main.rs`（整体替换）
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 创建 `src-tauri/src/lib.rs`**（内容 = 原 main.rs 的 Builder 逻辑，入口函数改为 `run()`）

```rust
mod clipboard;
mod commands;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
```

- [ ] **Step 2: 替换 `src-tauri/src/main.rs` 为薄壳**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    pmgr_gui_lib::run();
}
```

- [ ] **Step 3: `src-tauri/Cargo.toml` 增加 `[lib]` 段**（放在 `[package]` 之后、`[build-dependencies]` 之前）

```toml
[lib]
name = "pmgr_gui_lib"
crate-type = ["staticlib", "cdylib", "rlib"]
```

- [ ] **Step 4: 桌面回归验证**

```powershell
cargo test; cargo build -p pmgr-gui; npm run build
```

预期：测试全过、编译成功（生成 lib + exe）、tsc 与 vite 构建无错误。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "refactor(tauri): 入口迁移至 lib.rs（mobile_entry_point 就绪），桌面行为不变"
```

---

## Task 3: core 扩展——`open_with_key` / `export_key`（TDD）

**Files:**
- Modify: `core/src/vault.rs`（`impl Vault` 块内追加两个方法；`mod vault_tests` 内追加测试）

- [ ] **Step 1: 在 `core/src/vault.rs` 的 `mod vault_tests` 末尾追加失败测试**

```rust
    #[test]
    fn open_with_key_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        let mut v = Vault::create(&path, "master-pass-1").unwrap();
        v.data.entries.push(Entry::new("Gmail".into(), "u".into(), "p".into(), "".into(), "".into(), 1));
        v.save().unwrap();
        let key = v.export_key();
        drop(v);
        // 正确密钥 → 解锁成功且数据完整
        let v2 = Vault::open_with_key(&path, &key).unwrap();
        assert_eq!(v2.data.entries.len(), 1);
        assert_eq!(v2.data.entries[0].app_name, "Gmail");
        // 解锁后可正常保存（密钥/salt 一致）
        let mut v2 = v2;
        v2.data.entries.push(Entry::new("B".into(), "".into(), "".into(), "".into(), "".into(), 2));
        v2.save().unwrap();
        let v3 = Vault::open(&path, "master-pass-1").unwrap();
        assert_eq!(v3.data.entries.len(), 2);
    }

    #[test]
    fn open_with_wrong_key_fails_uniformly() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        Vault::create(&path, "master-pass-1").unwrap();
        let wrong = [9u8; KEY_LEN];
        let err = Vault::open_with_key(&path, &wrong).unwrap_err();
        // 安全要求：不泄露失败细节，与密码错误/文件损坏同一错误
        assert!(matches!(err, VaultError::WrongPasswordOrCorrupt));
    }
```

- [ ] **Step 2: 运行确认失败**

```powershell
cargo test -p pmgr-core vault_tests::open_with_key
```

预期：编译错误 `no method named open_with_key / export_key`。

- [ ] **Step 3: 在 `impl Vault` 中（`import_backup` 之后）实现**

```rust
    /// 用裸密钥直接解锁（跳过 Argon2）。仅供生物识别等已持有密钥的可信路径使用；
    /// GCM 校验失败统一返回 WrongPasswordOrCorrupt，不泄露失败原因。
    pub fn open_with_key(path: impl Into<PathBuf>, key: &[u8; KEY_LEN]) -> Result<Self, VaultError> {
        let path = path.into();
        let bytes = std::fs::read(&path)?;
        let parts = decode_file(&bytes)?;
        let plaintext = decrypt_payload(key, &parts.nonce, &parts.ciphertext)?;
        let data: VaultData =
            serde_json::from_slice(&plaintext).map_err(|_| VaultError::WrongPasswordOrCorrupt)?;
        Ok(Self { path, salt: parts.salt, params: parts.params, key: *key, data })
    }

    /// 导出当前 vault 密钥（Zeroizing 保证用后清零）。仅供生物识别注册流程使用。
    pub fn export_key(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(self.key)
    }
```

- [ ] **Step 4: 运行确认通过 + 全量回归**

```powershell
cargo test -p pmgr-core
```

预期：新增 2 个测试通过，原有测试全过。

- [ ] **Step 5: Commit**

```powershell
git add core/src/vault.rs
git commit -m "feat(core): Vault::open_with_key / export_key（生物识别解锁用裸密钥路径，TDD）"
```

---

## Task 4: core 扩展——备份字节约（TDD）

**Files:**
- Modify: `core/src/vault.rs`（`decrypt_backup` 重构 + 新增 `decrypt_backup_bytes`、`Vault::import_backup_bytes`；测试追加）

- [ ] **Step 1: 在 `mod vault_tests` 末尾追加失败测试**

```rust
    #[test]
    fn import_backup_bytes_replaces_data() {
        // 备份文件（password-B2）→ 字节读入 → 导入当前库（password-A1）
        let dir = tempfile::tempdir().unwrap();
        let path_a = dir.path().join("a.enc");
        let mut va = Vault::create(&path_a, "password-A1").unwrap();
        va.data.entries.push(Entry::new("AppA".into(), "".into(), "".into(), "".into(), "".into(), 1));
        va.save().unwrap();
        let path_b = dir.path().join("b.enc");
        {
            let mut vb = Vault::create(&path_b, "password-B2").unwrap();
            vb.data.entries.push(Entry::new("AppB".into(), "".into(), "".into(), "".into(), "".into(), 2));
            vb.save().unwrap();
        }
        let bytes = std::fs::read(&path_b).unwrap();
        // 错误密码 → 拒绝，数据不变
        assert!(va.import_backup_bytes(&bytes, "wrong-pass").is_err());
        assert_eq!(va.data.entries[0].app_name, "AppA");
        // 正确密码 → 替换并用当前密钥重加密落盘
        va.import_backup_bytes(&bytes, "password-B2").unwrap();
        assert_eq!(va.data.entries[0].app_name, "AppB");
        drop(va);
        let reopened = Vault::open(&path_a, "password-A1").unwrap();
        assert_eq!(reopened.data.entries[0].app_name, "AppB");
    }

    #[test]
    fn decrypt_backup_bytes_rejects_garbage() {
        assert!(matches!(
            decrypt_backup_bytes(b"not a vault file at all", "pw"),
            Err(VaultError::Format(_)) | Err(VaultError::WrongPasswordOrCorrupt)
        ));
    }
```

- [ ] **Step 2: 运行确认失败**

```powershell
cargo test -p pmgr-core vault_tests::import_backup_bytes
```

预期：编译错误 `cannot find function decrypt_backup_bytes / no method import_backup_bytes`。

- [ ] **Step 3: 实现——重构 `decrypt_backup` 复用字节版，`impl Vault` 增加 `import_backup_bytes`**

将现有自由函数 `decrypt_backup` 替换为：

```rust
/// 验证并解密一个备份/vault 文件，返回明文数据（不改变任何文件）。
pub fn decrypt_backup(src: &Path, password: &str) -> Result<VaultData, VaultError> {
    let bytes = std::fs::read(src)?;
    decrypt_backup_bytes(&bytes, password)
}

/// 字节版：供 Android 等经 IPC 传入文件内容的路径使用（content URI 无法用 std::fs 读）。
pub fn decrypt_backup_bytes(bytes: &[u8], password: &str) -> Result<VaultData, VaultError> {
    let parts = decode_file(bytes)?;
    let key = derive_key(password, &parts.salt, parts.params.0, parts.params.1, parts.params.2)?;
    let plaintext = decrypt_payload(&key, &parts.nonce, &parts.ciphertext)?;
    serde_json::from_slice(&plaintext).map_err(|_| VaultError::WrongPasswordOrCorrupt)
}
```

在 `impl Vault` 的 `import_backup` 之后追加：

```rust
    /// 字节版导入备份（Android：前端经 fs 插件读 content URI 后传入）。
    pub fn import_backup_bytes(&mut self, bytes: &[u8], password: &str) -> Result<(), VaultError> {
        let imported = decrypt_backup_bytes(bytes, password)?;
        self.data = imported;
        self.save()
    }
```

- [ ] **Step 4: 运行确认通过 + 全量回归**

```powershell
cargo test -p pmgr-core
```

预期：全部通过（含原有 `import_backup_replaces_data_under_current_password`）。

- [ ] **Step 5: Commit**

```powershell
git add core/src/vault.rs
git commit -m "feat(core): decrypt_backup_bytes / import_backup_bytes（Android content URI 备份导入，TDD）"
```

---

## Task 5: state.rs 双钟自动锁定（TDD）

**Files:**
- Modify: `src-tauri/src/state.rs`

- [ ] **Step 1: 在 `mod tests` 追加失败测试**

```rust
    #[test]
    fn dual_clock_takes_max() {
        use super::should_auto_lock_dual;
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
```

- [ ] **Step 2: 运行确认失败**

```powershell
cargo test -p pmgr-gui state::tests::dual_clock
```

预期：编译错误 `no function should_auto_lock_dual`。

- [ ] **Step 3: 实现——替换 `state.rs` 全文如下**（保留原 `should_auto_lock` 与其测试）

```rust
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
```

- [ ] **Step 4: 运行确认通过**

```powershell
cargo test -p pmgr-gui; cargo build -p pmgr-gui
```

预期：`state::tests` 两个测试通过，编译成功（`commands.rs` 只调 `state.touch()`，接口未变）。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/state.rs
git commit -m "feat(state): 自动锁定改双钟判定（单调钟+墙钟取大），修复 Android 深睡漏锁"
```

---

## Task 6: Android 工程初始化 + 依赖 + 配置 + capabilities

**Files:**
- Modify: `.gitignore`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/lib.rs`（插件注册）
- Modify: `src-tauri/capabilities/default.json`
- Create: `src-tauri/capabilities/mobile.json`
- Create: `src-tauri/gen/android/**`（工具生成）
- Modify: `package.json`（npm 依赖）

- [ ] **Step 1: `.gitignore` 放行 gen/android**——把 `src-tauri/gen/` 一行替换为：

```
src-tauri/gen/schemas/
```

- [ ] **Step 2: `src-tauri/tauri.conf.json` 增加 android 配置**——`bundle` 对象内（`"targets"` 之后）加：

```json
    "android": {
      "minSdkVersion": 28
    },
```

- [ ] **Step 3: `src-tauri/Cargo.toml` 依赖调整**——`arboard` 移出通用依赖，改为平台分组；新增插件。改完后 `[dependencies]` 及之后为：

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
tauri-plugin-os = "2"
tauri-plugin-fs = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
zeroize = { version = "1", features = ["derive"] }
pmgr-core = { path = "../core" }

[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]
arboard = "3"

[target.'cfg(any(target_os = "android", target_os = "ios"))'.dependencies]
tauri-plugin-clipboard-manager = "2"
tauri-plugin-biometric = "2"
tauri-plugin-keystore = "2"
```

- [ ] **Step 4: 安装前端依赖**

```powershell
npm install @tauri-apps/plugin-os @tauri-apps/plugin-fs @tauri-apps/plugin-biometric @impierce/tauri-plugin-keystore
```

- [ ] **Step 5: `lib.rs` 注册插件**——`run()` 开头改为：

```rust
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
        // ……（setup / invoke_handler / run 保持原样）
```

注意：若 `tauri_plugin_biometric::init()` 或 `tauri_plugin_keystore::init()` 不存在（以 crates.io 实际版本为准），改用其 README 的 `Builder::new().build()` 写法。

- [ ] **Step 6: capabilities**——`default.json` 的 `permissions` 数组追加 `"os:default"`（platform() 分流在桌面也会调用）。新建 `src-tauri/capabilities/mobile.json`：

```json
{
  "$schema": "../gen/schemas/mobile-schema.json",
  "identifier": "mobile",
  "description": "Android 端专属权限",
  "platforms": ["android"],
  "windows": ["main"],
  "permissions": [
    "biometric:default",
    "keystore:default",
    "fs:default",
    "fs:allow-read-file",
    "fs:allow-write-file",
    {
      "identifier": "fs:scope",
      "allow": [{ "path": "**" }, { "path": "content://**" }]
    }
  ]
}
```

说明：Android 上 fs 的 `**` 范围不构成额外风险——OS 沙箱保证应用只能读自己目录 + 用户经 SAF 明确授权的 URI。若 `keystore:default` 权限标识不存在（构建时报 unknown permission），改为逐条列出 `keystore:allow-store`、`keystore:allow-retrieve`、`keystore:allow-remove`。

- [ ] **Step 7: 初始化 Android 工程**（需 Task 1 环境就绪、新终端已加载环境变量）

```powershell
npm run tauri android init
```

预期：生成 `src-tauri/gen/android/`。验证包名转换：

```powershell
Select-String -Path src-tauri\gen\android\app\build.gradle.kts -Pattern "namespace"
```

预期输出包含 `com.pmgr.password_manager`（连字符自动转下划线）。同时确认 `build.gradle.kts` 中 `minSdkVersion = 28`（来自 Step 2 的配置）。

- [ ] **Step 8: 桌面回归 + Android 编译检查**

```powershell
cargo check; npm run build
cargo ndk --target aarch64-linux-android check -p pmgr-gui
```

预期：桌面 check 通过；android target 的 check 通过（首次会交叉编译依赖，耗时较长）。

- [ ] **Step 9: Commit**

```powershell
git add .gitignore src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/src/lib.rs src-tauri/capabilities package.json package-lock.json src-tauri/gen/android Cargo.lock
git commit -m "feat(android): tauri android init 生成 Android 工程；接入 os/fs/clipboard/biometric/keystore 插件与 mobile capabilities"
```

---

## Task 7: 剪贴板跨平台实现

**Files:**
- Modify: `src-tauri/src/clipboard.rs`（整体替换）
- Modify: `src-tauri/src/commands.rs`（`clipboard_copy` 传入 AppHandle）

- [ ] **Step 1: 替换 `clipboard.rs` 全文**（统一签名：两端都接收 `&AppHandle`，桌面忽略之；「比较后清除」安全逻辑两端一致）

```rust
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
        if let Err(_) = clear_if_unchanged(&app, &text) {
            // 清除失败不重试（与桌面旧行为一致）；text 随线程结束被释放
        }
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
    if let Ok(Some(current)) = cb.read_text() {
        if current == text {
            let _ = cb.write_text("");
        }
    }
    Ok(())
}
```

- [ ] **Step 2: `commands.rs` 的 `clipboard_copy`**——函数签名加 `app: AppHandle`，两处调用改为：

```rust
#[tauri::command]
pub fn clipboard_copy(
    app: AppHandle,
    state: State<'_, AppState>,
    mut text: String,
    secret: bool,
) -> Result<(), String> {
    state.touch();
    let copy_result = crate::clipboard::copy_text(&app, &text);
    // ……（中间逻辑不变）
            // text 移交给清除线程，用于到期比较；线程结束时释放
            crate::clipboard::schedule_clear(app, text, secs);
    // ……
}
```

（仅改这三行：签名、`copy_text(&app, &text)`、`schedule_clear(app, text, secs)`；其余保持原样。）

- [ ] **Step 3: 验证**

```powershell
cargo test; cargo check
cargo ndk --target aarch64-linux-android check -p pmgr-gui
```

预期：桌面与 android target 均编译通过。

- [ ] **Step 4: Commit**

```powershell
git add src-tauri/src/clipboard.rs src-tauri/src/commands.rs
git commit -m "feat(clipboard): Android 走 clipboard-manager 插件，桌面 arboard 不变；比较后清除逻辑两端一致"
```

---

## Task 8: 备份字节约命令（Rust + api.ts）

**Files:**
- Modify: `src-tauri/src/commands.rs`（文件末尾追加两个命令）
- Modify: `src-tauri/src/lib.rs`（handler 注册）
- Modify: `src/api.ts`（末尾追加封装）

- [ ] **Step 1: `commands.rs` 末尾追加**（跨平台注册——纯字节 IO，无 mobile 专属依赖；桌面调用也无害）

```rust
// ---------- Android 备份桥接（content URI 无法用 std::fs 读，改走字节 IPC，spec §6.2） ----------

/// 需解锁；返回 vault.enc 原始字节。出 Rust 的只有密文，永不导出明文。
#[tauri::command]
pub fn backup_export_data(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    with_vault(&state, |_| {
        let path = vault_path(&app)?;
        std::fs::read(&path).map_err(|e| e.to_string())
    })
}

/// 需解锁；字节来自前端经 fs 插件读取的用户所选备份文件。
#[tauri::command]
pub fn backup_import_data(
    state: State<'_, AppState>,
    data: Vec<u8>,
    mut backup_password: String,
) -> Result<(), String> {
    let result = with_vault(&state, |v| {
        v.import_backup_bytes(&data, &backup_password)
            .map_err(|_| "备份文件验证失败：主密码错误或文件已损坏".to_string())
    });
    backup_password.zeroize();
    result
}
```

- [ ] **Step 2: `lib.rs` handler 列表**——`commands::clipboard_copy` 之后追加：

```rust
            commands::backup_export_data,
            commands::backup_import_data,
```

- [ ] **Step 3: `src/api.ts` 末尾追加**

```ts
export const backupExportData = (): Promise<number[]> => invoke("backup_export_data");
export const backupImportData = (data: number[], backupPassword: string): Promise<void> =>
  invoke("backup_import_data", { data, backupPassword });
```

- [ ] **Step 4: 验证**

```powershell
cargo check; npm run build
```

预期：均通过。

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/api.ts
git commit -m "feat(backup): 新增字节级导出/导入命令（Android content URI 桥接，spec §6.2）"
```

---

## Task 9: 生物识别后端（biometric.rs + 命令接线 + api.ts）

**Files:**
- Create: `src-tauri/src/biometric.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands.rs`（`vault_create` / `master_password_change` 失效钩子）
- Modify: `src/api.ts`

- [ ] **Step 1: 创建 `src-tauri/src/biometric.rs`**（整文件仅 mobile 编译）

```rust
//! 生物识别解锁（仅 Android，spec §7）：启用标记管理 + 裸密钥解锁命令。
//! 密钥本体存于 Android Keystore（@impierce/tauri-plugin-keystore，硬件背书，
//! 存/取均强制生物识别），由前端 JS 桥接；Rust 侧只管理启用标记文件 biometric.json，
//! 其中不含任何密钥材料。

use pmgr_core::vault::{Vault, KEY_LEN};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroize;

use crate::commands::vault_path;
use crate::state::AppState;

/// Keystore 别名，前端（src/ui/mobile/bio.ts 的 KEY_ALIAS）必须与之一致。
pub const KEYSTORE_ALIAS: &str = "pmgr_vault_key";
const FLAG_FILE: &str = "biometric.json";

#[derive(Serialize, Deserialize)]
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
        let json = serde_json::to_string(&Flag { enabled: true }).unwrap();
        std::fs::write(&p, json).map_err(|e| e.to_string())
    } else {
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode_key(s: &str) -> Option<[u8; KEY_LEN]> {
    if s.len() != KEY_LEN * 2 {
        return None;
    }
    let mut out = [0u8; KEY_LEN];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
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
    Ok(hex_encode(&key))
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
    let key = hex_decode_key(&key_hex).ok_or("生物识别密钥已失效，请使用主密码解锁")?;
    key_hex.zeroize();
    let result = Vault::open_with_key(&path, &key)
        .map_err(|_| "生物识别密钥已失效，请使用主密码解锁".to_string());
    if result.is_err() {
        // 自动失效：删标记；Keystore 残留密文由前端 remove() 清理（下次注册同名覆盖）
        let _ = set_flag(&app, false);
        return result.map(|_| ());
    }
    *state.vault.lock().unwrap() = Some(result.unwrap());
    Ok(())
}
```

- [ ] **Step 2: `lib.rs` 接线**——模块声明区加：

```rust
#[cfg(mobile)]
mod biometric;
```

handler 列表末尾追加（`generate_handler!` 支持条目级 `#[cfg]`；若宏报错，回退方案：整个 `.invoke_handler(...)` 用 `#[cfg(desktop)]`/`#[cfg(mobile)]` 各写一份，共享列表重复但保证编译）：

```rust
            #[cfg(mobile)]
            biometric::biometric_enabled,
            #[cfg(mobile)]
            biometric::biometric_set_enabled,
            #[cfg(mobile)]
            biometric::biometric_export_key,
            #[cfg(mobile)]
            biometric::vault_unlock_with_key
```

- [ ] **Step 3: `commands.rs` 失效钩子**

`vault_create`：`*state.vault.lock().unwrap() = Some(result?);` 之前加：

```rust
    // 新建库 = 新密钥，旧生物识别注册必然失效（spec §7.4）
    #[cfg(mobile)]
    let _ = crate::biometric::set_flag(&app, false);
```

`master_password_change`：签名加 `app: AppHandle` 参数（放在 `state` 前），成功路径上 `old_password.zeroize();` 之前加：

```rust
    // 改密后 vault 密钥已更换，生物识别注册自动失效（spec §7.4）
    #[cfg(mobile)]
    let _ = crate::biometric::set_flag(&app, false);
```

具体改法：函数开头的校验失败分支不变；`let result = with_vault(...)` 之后改为：

```rust
    if result.is_ok() {
        #[cfg(mobile)]
        let _ = crate::biometric::set_flag(&app, false);
    }
    old_password.zeroize();
    new_password.zeroize();
    result
```

- [ ] **Step 4: `src/api.ts` 末尾追加**

```ts
export const biometricEnabled = (): Promise<boolean> => invoke("biometric_enabled");
export const biometricSetEnabled = (enabled: boolean): Promise<void> =>
  invoke("biometric_set_enabled", { enabled });
export const biometricExportKey = (): Promise<string> => invoke("biometric_export_key");
export const vaultUnlockWithKey = (keyHex: string): Promise<void> =>
  invoke("vault_unlock_with_key", { keyHex });
```

- [ ] **Step 5: 验证**

```powershell
cargo check; npm run build
cargo ndk --target aarch64-linux-android check -p pmgr-gui
```

预期：桌面 check 不含 biometric 代码但通过；android check 编译 biometric.rs 通过。

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/biometric.rs src-tauri/src/lib.rs src-tauri/src/commands.rs src/api.ts
git commit -m "feat(biometric): Android 生物识别后端——标记文件、裸密钥解锁、改密/新建自动失效"
```

---

## Task 10: 前端平台分流（main.ts）

**Files:**
- Modify: `src/main.ts`

- [ ] **Step 1: `main.ts` 修改**——顶部 import 区加一行；`boot()` 开头加分流：

```ts
import { platform } from "@tauri-apps/plugin-os";
```

```ts
async function boot(): Promise<void> {
  // 平台分流：Android → 手机专属 UI（src/ui/mobile/）；桌面 → 现有 UI（零改动）
  let isAndroid = false;
  try {
    isAndroid = (await platform()) === "android";
  } catch {
    isAndroid = false; // 桌面未注册 os 插件权限时 invoke 失败，按桌面处理
  }
  if (isAndroid) {
    const { bootMobile } = await import("./ui/mobile/boot");
    await bootMobile();
    return;
  }
  // ……以下原有桌面 boot 逻辑一字不动……
}
```

- [ ] **Step 2: 占位模块让 tsc 通过**——创建 `src/ui/mobile/boot.ts`（Task 11 会补全，本步先放最小实现）：

```ts
export async function bootMobile(): Promise<void> {
  document.body.innerHTML = "<p style='color:#fff;padding:24px'>手机端 UI 施工中</p>";
}
```

- [ ] **Step 3: 验证**

```powershell
npm run build
```

预期：tsc 与 vite 通过（mobile chunk 动态引入，桌面主 bundle 不含手机代码）。

- [ ] **Step 4: Commit**

```powershell
git add src/main.ts src/ui/mobile/boot.ts
git commit -m "feat(ui): main.ts 平台分流——Android 动态加载手机 UI，桌面路径不变"
```

---

## Task 11: 手机壳层——shell.ts（Tab 栏 + 页面栈路由）+ mobile.css + boot.ts

**Files:**
- Create: `src/ui/mobile/shell.ts`
- Create: `src/ui/mobile/mobile.css`
- Create: `src/ui/mobile/bio.ts`
- Modify: `src/ui/mobile/boot.ts`（替换 Task 10 占位）

> **审查后修正（commit b05f1e8）**：shell.ts 增加模块级 `locked` 标志——`resetStack()` 置 true、`switchTab()` 置 false、popstate 处理器首行 `if (locked) { showTabbar(false); return; }`，堵住「锁定屏上系统返回手势触发 popstate 重渲染数据页」的明文回放漏洞（review C1）；boot.ts 的 `vaultStatus()` 加 `.catch(() => null)` 容错。下方代码块以实际提交为准。

- [ ] **Step 1: 创建 `src/ui/mobile/bio.ts`**（与 Rust `biometric.rs` 的 `KEYSTORE_ALIAS` 保持一致）

```ts
/** Android Keystore 别名，必须与 src-tauri/src/biometric.rs 的 KEYSTORE_ALIAS 一致 */
export const KEY_ALIAS = "pmgr_vault_key";
```

- [ ] **Step 2: 创建 `src/ui/mobile/shell.ts`**

```ts
import "./mobile.css";

export type TabName = "entries" | "generate" | "settings";

type PageRender = (el: HTMLElement) => void | Promise<void>;

const stack: PageRender[] = [];
let currentTab: TabName = "entries";

function pages(): HTMLElement {
  return document.getElementById("m-pages")!;
}

function showTabbar(v: boolean): void {
  document.getElementById("m-tabbar")!.classList.toggle("hidden", !v);
}

export function installShell(): void {
  document.body.innerHTML = `
    <div id="m-root">
      <main id="m-pages"></main>
      <nav id="m-tabbar" class="hidden">
        <button type="button" class="tab" data-tab="entries">🗂️<span>条目</span></button>
        <button type="button" class="tab" data-tab="generate">🎲<span>生成</span></button>
        <button type="button" class="tab" data-tab="settings">⚙️<span>设置</span></button>
      </nav>
    </div>
    <div id="toast" class="hidden"></div>`;
  document.querySelectorAll<HTMLButtonElement>("#m-tabbar .tab").forEach((b) => {
    b.onclick = () => switchTab(b.dataset.tab as TabName);
  });
  // 系统返回手势/返回键 → popstate → 弹页面栈
  window.addEventListener("popstate", (e) => {
    const depth: number = (e.state as { depth?: number } | null)?.depth ?? 0;
    stack.length = Math.min(stack.length, depth);
    if (stack.length === 0) {
      showTabbar(true);
      void renderTab(currentTab);
    } else {
      showTabbar(false);
      void stack[stack.length - 1](pages());
    }
  });
}

export function switchTab(name: TabName): void {
  currentTab = name;
  stack.length = 0;
  history.replaceState({ depth: 0 }, "");
  showTabbar(true);
  document
    .querySelectorAll("#m-tabbar .tab")
    .forEach((b) => b.classList.toggle("active", b.getAttribute("data-tab") === name));
  void renderTab(name);
}

export function pushPage(render: PageRender): void {
  stack.push(render);
  history.pushState({ depth: stack.length }, "");
  showTabbar(false);
  void render(pages());
}

/** 返回上一页；渲染统一由 popstate 处理 */
export function popPage(): void {
  if (stack.length > 0) history.back();
}

/** 锁定屏接管：清空页面栈（不触发 history） */
export function resetStack(): void {
  stack.length = 0;
  history.replaceState({ depth: 0 }, "");
  showTabbar(false);
}

async function renderTab(name: TabName): Promise<void> {
  const el = pages();
  // 动态 import 打破 shell ↔ 页面模块的循环依赖
  if (name === "entries") {
    const { renderEntries } = await import("./entries");
    await renderEntries(el);
  } else if (name === "generate") {
    const { renderGenerate } = await import("./generate");
    renderGenerate(el);
  } else {
    const { renderSettings } = await import("./settings");
    await renderSettings(el);
  }
}
```

- [ ] **Step 3: 创建 `src/ui/mobile/mobile.css`**（复用 styles.css 的 :root 变量；全部选择器限定在 #m-root / .m-* 内，不影响桌面）

```css
/* 手机专属样式（复用 styles.css 的暗色主题变量） */
#m-root { display: flex; flex-direction: column; height: 100vh; height: 100dvh; }
#m-pages { flex: 1; overflow-y: auto; min-height: 0; }
#m-root input, #m-root textarea { font-size: 16px; padding: 12px; }

/* 底部 Tab 栏 */
#m-tabbar {
  display: flex; border-top: 1px solid var(--border); background: var(--panel);
  padding-bottom: env(safe-area-inset-bottom);
}
#m-tabbar .tab {
  flex: 1; background: none; border: none; border-radius: 0;
  display: flex; flex-direction: column; align-items: center; gap: 2px;
  padding: 8px 0 6px; color: var(--muted); font-size: 11px; min-height: 52px;
}
#m-tabbar .tab.active { color: var(--accent); }

/* 列表页 */
.m-toolbar {
  position: sticky; top: 0; z-index: 5; padding: 10px 12px;
  padding-top: calc(10px + env(safe-area-inset-top));
  background: var(--panel); border-bottom: 1px solid var(--border);
}
.m-card { padding: 14px 16px; border-bottom: 1px solid var(--border); min-height: 52px; cursor: pointer; }
.m-card:active { background: var(--panel2); }
.m-card .t { font-weight: 600; word-break: break-all; }
.m-card .s { color: var(--muted); font-size: 12px; margin-top: 2px; word-break: break-all; }
#m-fab {
  position: fixed; right: 20px; bottom: calc(72px + env(safe-area-inset-bottom));
  width: 56px; height: 56px; border-radius: 28px; font-size: 24px;
  background: var(--accent); border: none; color: #fff; z-index: 20;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

/* 通用页面 */
.m-page {
  padding: 16px; padding-top: calc(12px + env(safe-area-inset-top));
  display: flex; flex-direction: column; gap: 12px;
  padding-bottom: calc(16px + env(safe-area-inset-bottom));
}
.m-nav { display: flex; align-items: center; gap: 8px; }
.m-nav h2 { font-size: 17px; flex: 1; word-break: break-all; }
.m-btn-back { font-size: 16px; min-width: 44px; min-height: 44px; }
.m-field { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted); }
.m-inline { display: flex; flex-direction: row; align-items: center; gap: 6px; font-size: 14px; color: var(--text); }
.m-inline input[type="checkbox"], .m-inline input[type="range"] { width: auto; }
.m-row { display: flex; gap: 8px; align-items: center; }
.m-row button { min-height: 44px; }
button.m-big { min-height: 48px; font-size: 15px; }
.m-hint { color: var(--muted); font-size: 12px; line-height: 1.6; }
.m-section { font-size: 14px; color: var(--muted); margin-top: 8px; border-top: 1px solid var(--border); padding-top: 12px; }

/* 解锁屏 */
.m-lock { min-height: 100%; display: flex; align-items: center; justify-content: center; padding: 24px; }
.m-lock-box { width: 100%; max-width: 400px; display: flex; flex-direction: column; gap: 12px; }

/* 详情页 */
.m-detail-row { display: flex; flex-direction: column; gap: 6px; padding: 12px 0; border-bottom: 1px solid var(--border); }
.m-detail-row .k { color: var(--muted); font-size: 12px; }
.m-detail-row .v { word-break: break-all; font-family: Consolas, monospace; }

/* 生成器 */
.m-gen {
  background: var(--panel); border: 1px solid var(--border); border-radius: 8px;
  padding: 12px; display: flex; flex-direction: column; gap: 8px;
}
.m-gen-out {
  font-family: Consolas, monospace; background: var(--panel2); padding: 12px;
  border-radius: 8px; word-break: break-all; min-height: 44px;
}
```

- [ ] **Step 4: 替换 `src/ui/mobile/boot.ts` 全文**

```ts
import { listen } from "@tauri-apps/api/event";
import * as api from "../../api";
import { state } from "../../state";
import { showMobileLock, enterMobileMain } from "./lock";
import { installShell } from "./shell";

let lastActivity = Date.now();
let lastPing = 0;

function bump(): void {
  lastActivity = Date.now();
  // 节流：最多每 10 秒向 Rust 侧汇报一次活动（Rust 兜底锁定用），与桌面 main.ts 同策略
  if (lastActivity - lastPing > 10_000) {
    lastPing = lastActivity;
    api.activityPing().catch(() => {});
  }
}

async function checkAutoLock(): Promise<void> {
  const mins = state.settings.auto_lock_minutes;
  if (mins <= 0 || Date.now() - lastActivity <= mins * 60_000) return;
  const s = await api.vaultStatus().catch(() => null);
  if (s?.unlocked) {
    await api.vaultLock().catch(() => {});
    await showMobileLock();
  }
}

export async function bootMobile(): Promise<void> {
  installShell();
  const st = await api.vaultStatus();
  if (st.unlocked) await enterMobileMain();
  else await showMobileLock();
  // Rust 侧自动锁定线程触发时回到解锁屏
  await listen("vault-locked", () => { void showMobileLock(); });

  window.addEventListener("touchstart", bump, { passive: true });
  window.addEventListener("click", bump);
  window.addEventListener("keydown", bump);

  // 前端自动锁定：15 秒定时 + 回前台立即检查（Android 后台冻结时定时器停走，靠墙钟差值补判）
  window.setInterval(() => { void checkAutoLock(); }, 15_000);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") void checkAutoLock();
  });
}
```

注意：`showMobileLock`/`enterMobileMain` 在 Task 12 创建，本任务结束时 `npm run build` 会因缺模块失败——**先创建 `src/ui/mobile/lock.ts` 空导出占位**：

```ts
export async function showMobileLock(): Promise<void> {}
export async function enterMobileMain(): Promise<void> {}
```

（Task 12 会替换占位内容。）

- [ ] **Step 5: 验证**

```powershell
npm run build
```

预期：tsc + vite 通过。

- [ ] **Step 6: Commit**

```powershell
git add src/ui/mobile
git commit -m "feat(ui-mobile): 壳层——底部 Tab + 页面栈路由 + 移动样式 + boot（活动跟踪/双触发自动锁定）"
```

---

## Task 12: 手机解锁屏（含生物识别自动弹出）

**Files:**
- Modify: `src/ui/mobile/lock.ts`（替换占位全文）

- [ ] **Step 1: `lock.ts` 全文**

```ts
import { remove, retrieve } from "@impierce/tauri-plugin-keystore";
import * as api from "../../api";
import { state } from "../../state";
import { toast } from "../toast";
import { KEY_ALIAS } from "./bio";
import { resetStack, switchTab } from "./shell";

export async function enterMobileMain(): Promise<void> {
  onLock = false;
  state.entries = await api.entriesList();
  state.settings = await api.settingsGet();
  state.query = "";
  state.showPassword = false;
  switchTab("entries");
}

/** 生物识别解锁（spec §7.3/§7.4）。成功返回 true；任何失败回落主密码输入。 */
async function tryBiometricUnlock(): Promise<boolean> {
  let keyHex: string | null = null;
  try {
    keyHex = await retrieve(KEY_ALIAS, {
      title: "解锁密码管理器",
      subtitle: "验证身份以解锁",
      cancelLabel: "改用主密码",
    });
  } catch {
    return false; // 用户取消或生物识别失败 → 回落主密码，不失效注册
  }
  if (!keyHex) return false;
  try {
    await api.vaultUnlockWithKey(keyHex);
    return true;
  } catch {
    // 密钥与文件不匹配（改密/数据重建）→ 自动失效并清理
    await api.biometricSetEnabled(false).catch(() => {});
    await remove(KEY_ALIAS).catch(() => {});
    toast("生物识别密钥已失效，请使用主密码解锁");
    return false;
  }
}

function showErr(msg: string): void {
  const el = document.getElementById("m-lock-error")!;
  el.textContent = msg;
  el.classList.remove("hidden");
}

const UNLOCK_FORM = `
  <input type="password" id="m-unlock-pw" placeholder="主密码" autocomplete="off" />
  <button id="m-btn-unlock" class="primary m-big" type="button">解锁</button>
  <p class="m-hint">忘记密码？无法找回。若有加密备份文件且记得其主密码，可重置数据后导入恢复。</p>`;

const CREATE_FORM = `
  <p class="m-hint">首次使用，请创建主密码（至少 8 位）。主密码无法找回，请务必牢记！</p>
  <input type="password" id="m-create-pw1" placeholder="主密码" autocomplete="off" />
  <input type="password" id="m-create-pw2" placeholder="确认主密码" autocomplete="off" />
  <button id="m-btn-create" class="primary m-big" type="button">创建并进入</button>`;

/** 显示解锁屏（同步幂等标志防 vault-locked 事件与定时检查并发双弹指纹，review M4）。 */
let onLock = false;

export async function showMobileLock(): Promise<void> {
  if (onLock) return;
  onLock = true;
  resetStack();
  // 纵深防御（review I1，AGENTS.md 清零约定）：锁定即清空前端明文缓存——
  // Rust 侧已 zeroize，JS 侧不得残留条目明文
  state.entries = [];
  state.selectedId = null;
  state.query = "";
  state.showPassword = false;
  const status = await api.vaultStatus();
  const el = document.getElementById("m-pages")!;
  el.innerHTML = `<div class="m-lock"><div class="m-lock-box">
      <h1>🔐 密码管理器</h1>
      ${status.exists ? UNLOCK_FORM : CREATE_FORM}
      <div id="m-lock-error" class="error hidden"></div>
    </div></div>`;

  if (status.exists) {
    document.getElementById("m-btn-unlock")!.onclick = async () => {
      const pw = (document.getElementById("m-unlock-pw") as HTMLInputElement).value;
      if (!pw) return;
      try {
        await api.vaultUnlock(pw);
        await enterMobileMain();
      } catch {
        // 安全要求：统一笼统文案
        showErr("主密码错误，或数据文件已损坏");
      }
    };
    // 已开启生物识别 → 自动弹指纹（表单已渲染，取消即回落）
    if (await api.biometricEnabled().catch(() => false)) {
      if (await tryBiometricUnlock()) await enterMobileMain();
    }
  } else {
    document.getElementById("m-btn-create")!.onclick = async () => {
      const pw1 = (document.getElementById("m-create-pw1") as HTMLInputElement).value;
      const pw2 = (document.getElementById("m-create-pw2") as HTMLInputElement).value;
      if (pw1.length < 8) { showErr("主密码至少 8 位"); return; }
      if (pw1 !== pw2) { showErr("两次输入的主密码不一致"); return; }
      try {
        await api.vaultCreate(pw1);
        await enterMobileMain();
      } catch (e) {
        showErr(String(e));
      }
    };
  }
}
```

- [ ] **Step 2: 验证**

```powershell
npm run build
```

预期：通过。

- [ ] **Step 3: Commit**

```powershell
git add src/ui/mobile/lock.ts
git commit -m "feat(ui-mobile): 解锁屏——创建/解锁表单 + 生物识别自动弹出与失效回落"
```

---

## Task 13: 手机条目列表 + 详情页

**Files:**
- Create: `src/ui/mobile/entries.ts`
- Create: `src/ui/mobile/detail.ts`

- [ ] **Step 1: 创建 `src/ui/mobile/entries.ts`**（复用桌面 `list.ts` 的 `esc`/`filteredEntries`，避免重复实现）

```ts
import * as api from "../../api";
import { state } from "../../state";
import { esc, filteredEntries } from "../list";
import { renderDetail } from "./detail";
import { renderEditor } from "./editor";
import { pushPage } from "./shell";

export async function renderEntries(el: HTMLElement): Promise<void> {
  el.innerHTML = `
    <div class="m-toolbar">
      <input type="search" id="m-search" placeholder="搜索应用名 / 用户名 / URL / 备注…"
        value="${esc(state.query)}" />
    </div>
    <div id="m-list"></div>
    <button id="m-fab" type="button">＋</button>`;
  document.getElementById("m-search")!.addEventListener("input", (ev) => {
    state.query = (ev.target as HTMLInputElement).value;
    renderMList();
  });
  document.getElementById("m-fab")!.onclick = () => pushPage((p) => renderEditor(p, null));
  renderMList();
}

function renderMList(): void {
  const el = document.getElementById("m-list")!;
  const items = filteredEntries();
  el.innerHTML = items.length
    ? items
        .map(
          (e) => `
      <div class="m-card" data-id="${e.id}">
        <div class="t">${esc(e.app_name)}</div>
        <div class="s">${esc(e.username)}</div>
      </div>`,
        )
        .join("")
    : `<p class="m-hint m-empty">${
        state.query ? "无匹配条目" : "还没有条目，点击右下「＋」新建"
      }</p>`;
  el.querySelectorAll<HTMLElement>(".m-card").forEach((c) => {
    c.onclick = () => {
      // 每次进入详情都要求重新显式揭示密码（镜像桌面 list.ts 选中重置，review Task13#1）
      state.showPassword = false;
      pushPage((p) => renderDetail(p, c.dataset.id!));
    };
  });
}
```

- [ ] **Step 2: 创建 `src/ui/mobile/detail.ts`**

```ts
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../../api";
import { state } from "../../state";
import { esc } from "../list";
import { toast } from "../toast";
import { renderEditor } from "./editor";
import { popPage, pushPage } from "./shell";

function fmtTime(secs: number): string {
  return secs === 0 ? "—" : new Date(secs * 1000).toLocaleString();
}

export async function renderDetail(el: HTMLElement, id: string): Promise<void> {
  const e = state.entries.find((x) => x.id === id);
  if (!e) { popPage(); return; }
  const dots = "•".repeat(Math.max(Math.min(e.password.length, 24), 8));
  el.innerHTML = `
    <div class="m-page">
      <div class="m-nav">
        <button class="m-btn-back" id="d-back" type="button">←</button>
        <h2>${esc(e.app_name)}</h2>
      </div>
      <div class="m-detail-row">
        <span class="k">用户名</span>
        <span class="v">${esc(e.username) || "—"}</span>
        <div class="m-row"><button class="small" id="d-copy-user" type="button">复制用户名</button></div>
      </div>
      <div class="m-detail-row">
        <span class="k">密码</span>
        <span class="v">${state.showPassword ? esc(e.password) : dots}</span>
        <div class="m-row">
          <button class="small" id="d-toggle" type="button">${state.showPassword ? "隐藏" : "显示"}</button>
          <button class="small" id="d-copy-pw" type="button">复制密码</button>
        </div>
      </div>
      ${e.url ? `<div class="m-detail-row"><span class="k">URL</span><span class="v">${esc(e.url)}</span></div>` : ""}
      ${e.notes ? `<div class="m-detail-row"><span class="k">备注</span><span class="v m-pre-wrap">${esc(e.notes)}</span></div>` : ""}
      <p class="m-hint">创建：${fmtTime(e.created_at)} ・ 更新：${fmtTime(e.updated_at)}</p>
      <div class="m-row">
        <button class="primary m-big m-flex1" id="d-edit" type="button">编辑</button>
        <button class="danger m-big m-flex1" id="d-del" type="button">删除</button>
      </div>
    </div>`;

  document.getElementById("d-back")!.onclick = () => popPage();
  document.getElementById("d-copy-user")!.onclick = async () => {
    await api.clipboardCopy(e.username, false);
    toast("已复制");
  };
  document.getElementById("d-toggle")!.onclick = () => {
    state.showPassword = !state.showPassword;
    void renderDetail(el, id);
  };
  document.getElementById("d-copy-pw")!.onclick = async () => {
    await api.clipboardCopy(e.password, true);
    const s = state.settings.clipboard_clear_seconds;
    toast(s > 0 ? `密码已复制，${s} 秒后自动清除剪贴板` : "密码已复制");
  };
  document.getElementById("d-edit")!.onclick = () => pushPage((p) => renderEditor(p, e));
  document.getElementById("d-del")!.onclick = async () => {
    const ok = await confirmDialog(`确定删除「${e.app_name}」的条目？此操作不可恢复。`, {
      title: "确认删除",
      kind: "warning",
    });
    if (!ok) return;
    try {
      await api.entryDelete(e.id);
      state.entries = state.entries.filter((x) => x.id !== e.id);
      toast("已删除");
      popPage();
    } catch (err) {
      toast(`删除失败：${String(err)}`);
    }
  };
}
```

- [ ] **Step 3: 创建 `src/ui/mobile/editor.ts` 占位**（Task 14 补全，保证本任务可编译）

```ts
import type * as api from "../../api";

export function renderEditor(_el: HTMLElement, _entry: api.Entry | null): void {
  // Task 14 实现
}
```

- [ ] **Step 4: 验证**

```powershell
npm run build
```

预期：通过（editor 占位无未使用参数告警——若有 noUnusedParameters 报错，将参数改名为注释形式或在 tsconfig 允许范围内处理；本项目 tsconfig 以实际为准）。

- [ ] **Step 5: Commit**

```powershell
git add src/ui/mobile/entries.ts src/ui/mobile/detail.ts src/ui/mobile/editor.ts
git commit -m "feat(ui-mobile): 条目 Tab——搜索/卡片列表/详情页（复制、显隐、删除）+ FAB 入口"
```

---

## Task 14: 手机编辑页 + 生成器 Tab

**Files:**
- Modify: `src/ui/mobile/editor.ts`（替换占位全文）
- Create: `src/ui/mobile/generate.ts`

- [ ] **Step 1: `editor.ts` 全文**

```ts
import * as api from "../../api";
import { state } from "../../state";
import { esc } from "../list";
import { toast } from "../toast";
import { popPage } from "./shell";

function val(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}
function chk(id: string): boolean {
  return (document.getElementById(id) as HTMLInputElement).checked;
}

async function regenerate(): Promise<void> {
  const opts: api.GenOptions = {
    length: Number(val("m-gen-length")) || 20,
    lowercase: chk("m-gen-lower"),
    uppercase: chk("m-gen-upper"),
    digits: chk("m-gen-digits"),
    symbols: chk("m-gen-symbols"),
    exclude_ambiguous: chk("m-gen-amb"),
  };
  const out = document.getElementById("m-gen-out") as HTMLOutputElement;
  try {
    const pw = await api.passwordGenerate(opts);
    out.textContent = pw;
    out.dataset.pw = pw; // 真正的密码存 dataset；textContent 可能显示错误信息
  } catch (err) {
    out.textContent = String(err);
    delete out.dataset.pw;
  }
}

/** entry=null 表示新增 */
export function renderEditor(el: HTMLElement, entry: api.Entry | null): void {
  el.innerHTML = `
    <div class="m-page">
      <div class="m-nav">
        <button class="m-btn-back" id="m-ed-cancel" type="button">←</button>
        <h2>${entry ? "编辑条目" : "新增条目"}</h2>
      </div>
      <label class="m-field">应用名 *
        <input id="m-ed-app" type="text" value="${esc(entry?.app_name ?? "")}" placeholder="例如：Gmail、网银" /></label>
      <label class="m-field">用户名
        <input id="m-ed-user" type="text" autocomplete="off" value="${esc(entry?.username ?? "")}" /></label>
      <label class="m-field">密码
        <input id="m-ed-pw" type="text" autocomplete="off" value="${esc(entry?.password ?? "")}" /></label>
      <div class="m-gen">
        <label class="m-inline">长度
          <input id="m-gen-length" type="range" min="8" max="64" value="20" />
          <span id="m-gen-len-val">20</span></label>
        <label class="m-inline"><input id="m-gen-lower" type="checkbox" checked /> 小写 a-z</label>
        <label class="m-inline"><input id="m-gen-upper" type="checkbox" checked /> 大写 A-Z</label>
        <label class="m-inline"><input id="m-gen-digits" type="checkbox" checked /> 数字 0-9</label>
        <label class="m-inline"><input id="m-gen-symbols" type="checkbox" checked /> 符号 !@#$…</label>
        <label class="m-inline"><input id="m-gen-amb" type="checkbox" checked /> 排除易混淆字符 0O1lI|</label>
        <output id="m-gen-out" class="m-gen-out"></output>
        <div class="m-row">
          <button class="small" id="m-gen-go" type="button">随机生成</button>
          <button class="small primary" id="m-gen-use" type="button">填入密码</button>
        </div>
      </div>
      <label class="m-field">URL
        <input id="m-ed-url" type="text" value="${esc(entry?.url ?? "")}" placeholder="https://…" /></label>
      <label class="m-field">备注
        <textarea id="m-ed-notes" rows="3">${esc(entry?.notes ?? "")}</textarea></label>
      <button class="primary m-big" id="m-ed-save" type="button">保存</button>
    </div>`;

  document.getElementById("m-ed-cancel")!.onclick = () => popPage();
  document.getElementById("m-gen-go")!.onclick = () => { void regenerate(); };
  document.getElementById("m-gen-use")!.onclick = () => {
    const out = document.getElementById("m-gen-out") as HTMLOutputElement;
    const pw = out.dataset.pw ?? "";
    if (!pw) { toast("请先生成密码"); return; }
    (document.getElementById("m-ed-pw") as HTMLInputElement).value = pw;
  };
  document.getElementById("m-gen-length")!.addEventListener("input", (ev) => {
    document.getElementById("m-gen-len-val")!.textContent = (ev.target as HTMLInputElement).value;
  });
  document.getElementById("m-ed-save")!.onclick = async () => {
    const appName = val("m-ed-app").trim();
    if (!appName) { toast("应用名不能为空"); return; }
    // 防双击重复保存（review Task14 I1）：请求期间禁用按钮，失败再恢复
    const saveBtn = document.getElementById("m-ed-save") as HTMLButtonElement;
    saveBtn.disabled = true;
    const payload: api.Entry = {
      id: entry?.id ?? "",
      app_name: appName,
      username: val("m-ed-user").trim(),
      password: val("m-ed-pw"),
      url: val("m-ed-url").trim(),
      notes: val("m-ed-notes"),
      created_at: entry?.created_at ?? 0,
      updated_at: 0,
    };
    try {
      const saved = await api.entrySave(payload);
      const i = state.entries.findIndex((x) => x.id === saved.id);
      if (i >= 0) state.entries[i] = saved;
      else state.entries.unshift(saved);
      state.showPassword = false;
      toast("已保存");
      popPage();
    } catch (err) {
      saveBtn.disabled = false;
      toast(`保存失败：${String(err)}`);
    }
  };
}
```

- [ ] **Step 2: 创建 `src/ui/mobile/generate.ts`**

```ts
import * as api from "../../api";
import { state } from "../../state";
import { toast } from "../toast";

function val(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}
function chk(id: string): boolean {
  return (document.getElementById(id) as HTMLInputElement).checked;
}

export function renderGenerate(el: HTMLElement): void {
  el.innerHTML = `
    <div class="m-page">
      <div class="m-nav"><h2>密码生成器</h2></div>
      <label class="m-inline">长度
        <input id="g-length" type="range" min="8" max="64" value="20" />
        <span id="g-len-val">20</span></label>
      <label class="m-inline"><input id="g-lower" type="checkbox" checked /> 小写 a-z</label>
      <label class="m-inline"><input id="g-upper" type="checkbox" checked /> 大写 A-Z</label>
      <label class="m-inline"><input id="g-digits" type="checkbox" checked /> 数字 0-9</label>
      <label class="m-inline"><input id="g-symbols" type="checkbox" checked /> 符号 !@#$…</label>
      <label class="m-inline"><input id="g-amb" type="checkbox" checked /> 排除易混淆字符 0O1lI|</label>
      <output id="g-out" class="m-gen-out"></output>
      <div class="m-row">
        <button class="primary m-big m-flex1" id="g-go" type="button">生成</button>
        <button class="m-big m-flex1" id="g-copy" type="button">复制</button>
      </div>
    </div>`;

  const regen = async (): Promise<void> => {
    const out = document.getElementById("g-out") as HTMLOutputElement;
    try {
      const pw = await api.passwordGenerate({
        length: Number(val("g-length")) || 20,
        lowercase: chk("g-lower"),
        uppercase: chk("g-upper"),
        digits: chk("g-digits"),
        symbols: chk("g-symbols"),
        exclude_ambiguous: chk("g-amb"),
      });
      out.textContent = pw;
      out.dataset.pw = pw;
    } catch (err) {
      out.textContent = String(err);
      delete out.dataset.pw;
    }
  };
  document.getElementById("g-length")!.addEventListener("input", (ev) => {
    document.getElementById("g-len-val")!.textContent = (ev.target as HTMLInputElement).value;
  });
  document.getElementById("g-go")!.onclick = () => { void regen(); };
  document.getElementById("g-copy")!.onclick = async () => {
    const out = document.getElementById("g-out") as HTMLOutputElement;
    const pw = out.dataset.pw ?? "";
    if (!pw) { toast("请先生成密码"); return; }
    await api.clipboardCopy(pw, true);
    const s = state.settings.clipboard_clear_seconds;
    toast(s > 0 ? `已复制，${s} 秒后自动清除剪贴板` : "已复制");
  };
  void regen();
}
```

- [ ] **Step 3: 验证**

```powershell
npm run build
```

预期：通过。

- [ ] **Step 4: Commit**

```powershell
git add src/ui/mobile/editor.ts src/ui/mobile/generate.ts
git commit -m "feat(ui-mobile): 编辑页（内嵌生成器）与生成器 Tab"
```

---

## Task 15: 手机设置页（生物识别开关 + SAF 备份 + 改密 + 锁定）

**Files:**
- Create: `src/ui/mobile/settings.ts`

- [ ] **Step 1: `settings.ts` 全文**

```ts
import {
  save as saveDialog,
  open as openDialog,
  confirm as confirmDialog,
} from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";
import { checkStatus, BiometryType } from "@tauri-apps/plugin-biometric";
import { remove, store } from "@impierce/tauri-plugin-keystore";
import * as api from "../../api";
import { state } from "../../state";
import { toast } from "../toast";
import { KEY_ALIAS } from "./bio";
import { showMobileLock } from "./lock";

function numVal(id: string): number {
  const v = Number((document.getElementById(id) as HTMLInputElement).value);
  return Number.isFinite(v) && v >= 0 ? Math.floor(v) : 0;
}
function pwVal(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}

async function enableBiometric(): Promise<void> {
  const st = await checkStatus();
  if (!st.isAvailable || st.biometryType === BiometryType.None) {
    toast("设备未录入指纹/人脸，请先在系统设置中添加");
    return;
  }
  try {
    const keyHex = await api.biometricExportKey();
    await store(KEY_ALIAS, keyHex, {
      title: "开启生物识别解锁",
      subtitle: "验证身份以保存解锁密钥",
      cancelLabel: "取消",
    });
    await api.biometricSetEnabled(true);
    toast("生物识别解锁已开启");
  } catch (e) {
    toast(`开启失败：${String(e)}`);
  }
}

async function disableBiometric(): Promise<void> {
  await remove(KEY_ALIAS).catch(() => {});
  await api.biometricSetEnabled(false);
  toast("生物识别解锁已关闭");
}

async function exportBackup(): Promise<void> {
  const stamp = new Date().toISOString().slice(0, 10).replace(/-/g, "");
  const uri = await saveDialog({ defaultPath: `pmgr-backup-${stamp}.enc` });
  if (!uri) return;
  try {
    const data = await api.backupExportData();
    await writeFile(String(uri), new Uint8Array(data));
    toast("备份已导出（加密文件，请妥善保管）");
  } catch (err) {
    toast(`导出失败：${String(err)}`);
  }
}

async function importBackup(): Promise<void> {
  const backupPw = pwVal("m-set-importpw");
  if (!backupPw) { toast("请先填写「备份文件的主密码」"); return; }
  const uri = await openDialog({ multiple: false });
  if (!uri) return;
  const ok = await confirmDialog(
    "导入将覆盖当前全部数据（当前数据文件会保留 .bak 备份）。确定继续？",
    { title: "确认导入", kind: "warning" },
  );
  if (!ok) return;
  try {
    const bytes = await readFile(String(uri));
    await api.backupImportData(Array.from(bytes), backupPw);
    (document.getElementById("m-set-importpw") as HTMLInputElement).value = "";
    toast("导入成功，正在重新载入…");
    window.setTimeout(() => location.reload(), 800);
  } catch (err) {
    toast(`导入失败：${String(err)}`);
  }
}

export async function renderSettings(el: HTMLElement): Promise<void> {
  const bioOn = await api.biometricEnabled().catch(() => false);
  el.innerHTML = `
    <div class="m-page">
      <div class="m-nav"><h2>设置</h2></div>

      <h3 class="m-section">常规</h3>
      <label class="m-field">自动锁定超时（分钟，0 = 关闭）
        <input id="m-set-autolock" type="number" min="0" max="720" value="${state.settings.auto_lock_minutes}" /></label>
      <label class="m-field">剪贴板自动清除（秒，0 = 关闭）
        <input id="m-set-clip" type="number" min="0" max="3600" value="${state.settings.clipboard_clear_seconds}" /></label>
      <button class="primary m-big" id="m-set-save" type="button">保存设置</button>

      <h3 class="m-section">生物识别解锁</h3>
      <p class="m-hint">开启后日常解锁可用指纹/人脸；主密码始终可用作兜底。修改主密码后需重新开启。</p>
      <button class="m-big" id="m-set-bio" type="button">${bioOn ? "关闭生物识别解锁" : "开启生物识别解锁"}</button>

      <h3 class="m-section">修改主密码</h3>
      <label class="m-field">当前主密码<input id="m-set-oldpw" type="password" autocomplete="off" /></label>
      <label class="m-field">新主密码（至少 8 位）<input id="m-set-newpw" type="password" autocomplete="off" /></label>
      <label class="m-field">确认新主密码<input id="m-set-newpw2" type="password" autocomplete="off" /></label>
      <button class="m-big" id="m-set-chpw" type="button">修改主密码</button>

      <h3 class="m-section">备份</h3>
      <label class="m-field">导入用：备份文件的主密码
        <input id="m-set-importpw" type="password" autocomplete="off" placeholder="仅导入备份时需要" /></label>
      <button class="m-big" id="m-set-export" type="button">导出加密备份…</button>
      <button class="m-big" id="m-set-import" type="button">导入备份（覆盖当前数据）…</button>

      <h3 class="m-section"></h3>
      <button class="danger m-big" id="m-set-lock" type="button">🔒 立即锁定</button>
    </div>`;

  document.getElementById("m-set-save")!.onclick = async () => {
    const s: api.Settings = {
      auto_lock_minutes: numVal("m-set-autolock"),
      clipboard_clear_seconds: numVal("m-set-clip"),
    };
    try {
      await api.settingsSave(s);
      state.settings = s;
      toast("设置已保存");
    } catch (err) {
      toast(`保存失败：${String(err)}`);
    }
  };

  document.getElementById("m-set-bio")!.onclick = async () => {
    if (bioOn) await disableBiometric();
    else await enableBiometric();
    await renderSettings(el);
  };

  document.getElementById("m-set-chpw")!.onclick = async () => {
    const oldPw = pwVal("m-set-oldpw");
    const newPw = pwVal("m-set-newpw");
    if (newPw.length < 8) { toast("新主密码至少 8 位"); return; }
    if (newPw !== pwVal("m-set-newpw2")) { toast("两次输入的新密码不一致"); return; }
    try {
      await api.masterPasswordChange(oldPw, newPw);
      // Rust 侧已自动失效标记；这里清理 Keystore 残留密文（同名注册会覆盖，失败可忽略）
      await remove(KEY_ALIAS).catch(() => {});
      for (const id of ["m-set-oldpw", "m-set-newpw", "m-set-newpw2"]) {
        (document.getElementById(id) as HTMLInputElement).value = "";
      }
      toast(bioOn ? "主密码已修改，生物识别解锁已关闭，如需请重新开启" : "主密码已修改");
      await renderSettings(el);
    } catch (err) {
      toast(String(err));
    }
  };

  document.getElementById("m-set-export")!.onclick = () => { void exportBackup(); };
  document.getElementById("m-set-import")!.onclick = () => { void importBackup(); };
  document.getElementById("m-set-lock")!.onclick = async () => {
    await api.vaultLock();
    await showMobileLock();
  };
}
```

注意：`renderSettings` 顶部按钮 `m-set-bio` 的回调里引用了渲染时的 `bioOn` 常量，操作后重新 `renderSettings(el)` 刷新状态，逻辑自洽。

- [ ] **Step 2: 验证**

```powershell
npm run build
```

预期：tsc + vite 通过。若 `BiometryType` / `checkStatus` 导入名与实际包版本不符，以 `node_modules/@tauri-apps/plugin-biometric/dist-js/index.d.ts` 的导出为准修正。

- [ ] **Step 3: Commit**

```powershell
git add src/ui/mobile/settings.ts
git commit -m "feat(ui-mobile): 设置 Tab——常规/生物识别开关/改密/SAF 备份导入导出/立即锁定"
```

---

## Task 16: 首次真机运行 + 签名配置 + release APK

**Files:**
- Modify: `src-tauri/gen/android/app/build.gradle.kts`（签名配置）

- [ ] **Step 1: 开发模式跑通（USB 真机开调试，或启动模拟器）**

```powershell
npm run tauri android dev
```

预期：应用装上设备并启动，显示手机解锁屏/创建屏。首次构建较慢（交叉编译全部依赖）。跑通冒烟：创建库 → 新增条目 → 复制密码 → 生成器 → 设置保存。**遇到问题按 systematic-debugging 流程处理后再继续。**

- [ ] **Step 2: 生成发布签名 keystore（用户交互，一次性；文件在仓库外）**

```powershell
& "$env:JAVA_HOME\bin\keytool" -genkeypair -v -keystore "$env:USERPROFILE\.android\pmgr-release.keystore" -alias pmgr -keyalg RSA -keysize 4096 -validity 10000
```

按提示设置 store/key 口令（可相同），**口令与文件务必妥善保管**——丢失后用户无法覆盖安装升级。

- [ ] **Step 3: `gen/android/app/build.gradle.kts` 增加签名配置**

在 `android { ... }` 块内（`buildTypes` 之前）加入 `signingConfigs`；`release` buildType 引用之。以生成文件的实际结构为准，模式如下：

```kotlin
    signingConfigs {
        create("release") {
            // 环境变量注入，keystore 不进 git；未配置时回落 debug 签名
            System.getenv("PMGR_KEYSTORE_PATH")?.let {
                storeFile = file(it)
                storePassword = System.getenv("PMGR_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("PMGR_KEY_ALIAS")
                keyPassword = System.getenv("PMGR_KEY_PASSWORD")
            }
        }
    }
```

并把 `getByName("release")` 的签名改为：

```kotlin
            signingConfig = if (System.getenv("PMGR_KEYSTORE_PATH") != null)
                signingConfigs.getByName("release") else signingConfigs.getByName("debug")
```

- [ ] **Step 4: 构建 release APK（当前会话设置签名环境变量）**

```powershell
$env:PMGR_KEYSTORE_PATH = "$env:USERPROFILE\.android\pmgr-release.keystore"
$env:PMGR_KEYSTORE_PASSWORD = "<口令>"
$env:PMGR_KEY_ALIAS = "pmgr"
$env:PMGR_KEY_PASSWORD = "<口令>"
npm run tauri android build -- --target aarch64 --apk
```

预期产物：`src-tauri\gen\android\app\build\outputs\apk\` 下 arm64-v8a 的 release APK（用 `Get-ChildItem -Recurse -Filter *.apk` 定位）。

- [ ] **Step 5: 安装验证签名版**

```powershell
& "$env:ANDROID_HOME\platform-tools\adb" install -r <apk路径>
```

手机上启动，重复 Step 1 冒烟 + 验证生物识别：设置 → 开启生物识别（弹指纹）→ 杀进程重开（自动弹指纹解锁）→ 修改主密码（提示失效）→ 重新开启。

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/gen/android/app/build.gradle.kts
git commit -m "build(android): release 签名配置（环境变量注入，keystore 不进仓库）"
```

---

## Task 17: CI——release.yml 增加 android job

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: 配置 GitHub Secrets（用户在仓库 Settings → Secrets and variables → Actions 添加）**

- `ANDROID_KEYSTORE_B64`：`[Convert]::ToBase64String([IO.File]::ReadAllBytes("$env:USERPROFILE\.android\pmgr-release.keystore"))` 的输出
- `ANDROID_KEYSTORE_PASSWORD`、`ANDROID_KEY_ALIAS`（=pmgr）、`ANDROID_KEY_PASSWORD`

- [ ] **Step 2: `release.yml` 的 `jobs:` 下追加独立 job（与现有 `build` 并列），并更新桌面 job 的 releaseBody**

```yaml
  android:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm

      - uses: actions/setup-java@v4
        with:
          distribution: temurin
          java-version: 17
          cache: gradle

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-linux-android

      - uses: swatinem/rust-cache@v2

      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-ndk

      - name: Install frontend dependencies
        run: npm ci

      - name: Decode release keystore
        env:
          KEYSTORE_B64: ${{ secrets.ANDROID_KEYSTORE_B64 }}
        run: |
          printf '%s' "$KEYSTORE_B64" | base64 -d > /tmp/pmgr-release.keystore
          [ -s /tmp/pmgr-release.keystore ] || { echo "错误：ANDROID_KEYSTORE_B64 secret 未配置或为空"; exit 1; }

      - name: Point NDK_HOME at runner NDK
        run: echo "NDK_HOME=$ANDROID_NDK_LATEST_HOME" >> "$GITHUB_ENV"

      - uses: tauri-apps/tauri-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          PMGR_KEYSTORE_PATH: /tmp/pmgr-release.keystore
          PMGR_KEYSTORE_PASSWORD: ${{ secrets.ANDROID_KEYSTORE_PASSWORD }}
          PMGR_KEY_ALIAS: ${{ secrets.ANDROID_KEY_ALIAS }}
          PMGR_KEY_PASSWORD: ${{ secrets.ANDROID_KEY_PASSWORD }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Password Manager v__VERSION__"
          releaseBody: "Windows: 下载 -setup.exe 安装。Linux: 下载 AppImage（chmod +x 后运行）或 .deb（sudo apt install ./xxx.deb）。Android: 下载 arm64 APK 侧载安装（需允许未知来源；覆盖升级须安装同一签名构建的版本）。"
          releaseDraft: true
          prerelease: false
          mobile: android
          args: --target aarch64 --apk
```

两个 job 使用相同 `tagName`，tauri-action 会把产物附到同一个 draft Release。**审查修正（Task 17 C1/C2/I1/M1/M4，commit 5a64dfb）**：必须用 `tauri-action@v1` + `mobile: android`（v0 无 mobile 输入，会把 args 拼成非法命令）；rustup 需显式 `targets: aarch64-linux-android`；NDK_HOME 兜底导出；keystore 解码走 env 且校验非空；setup-java 开 gradle 缓存。

- [ ] **Step 3: 验证 YAML 语法**

```powershell
npx yaml-lint .github/workflows/release.yml
```

（若无 yaml-lint 也可用 `node -e "require('js-yaml')"` 类工具或跳过——最终以打 tag 后的 Actions 运行为准。）

- [ ] **Step 4: Commit**

```powershell
git add .github/workflows/release.yml
git commit -m "ci: Android APK 构建 job（arm64、release 签名、附到 draft Release）"
```

真实验证在发版时：`npm version patch`（同步四处版本号，见 AGENTS.md）→ push `v*` tag → 观察 Actions 两个 job 均绿、draft Release 含 APK。

---

## Task 18: 文档同步 + 全量回归

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`

- [ ] **Step 1: `AGENTS.md` 更新**（对照 spec §10 逐条落实）

- 「项目结构」：`src-tauri/` 描述补充 `lib.rs`（统一入口，`mobile_entry_point`）、`biometric.rs`（仅 mobile）；`src/` 补充 `src/ui/mobile/`（手机专属 UI，底部 Tab）；`src-tauri/gen/android` 为提交进仓库的 Android 工程（可改签名/Manifest，`gen/schemas` 仍 gitignore）
- 「常用命令」：追加

```bash
npm run tauri android dev                       # Android 真机/模拟器调试
npm run tauri android build -- --target aarch64 --apk   # release APK（需签名环境变量）
```

- 「新增 IPC 命令的流程」：追加第 5 步——mobile 专属命令放 `#[cfg(mobile)]` 模块并在 `generate_handler!` 条目上加 `#[cfg(mobile)]`；前端若直接调插件（fs/biometric/keystore），需在 `src-tauri/capabilities/mobile.json` 加权限
- 自动锁定段落：补充「Rust 侧为单调钟+墙钟双判定（Android 深睡单调钟停走），改 `state.rs` 时保持 `should_auto_lock_dual` 纯函数可测」
- 安全约定：追加两条——「Android 剪贴板走 clipboard-manager 插件，『比较后清除』逻辑必须保留」「生物识别密钥仅存 Android Keystore；`biometric.json` 只是启用标记，不得写入任何密钥材料；密钥 hex 经 WebView 过桥为已明示约束（spec §7.5），不得扩大使用范围」
- 「数据与发布」：补充 Android 产物（arm64 APK）、keystore 管理（`%USERPROFILE%\.android\pmgr-release.keystore`，丢失无法覆盖升级）、版本号由 tauri.conf.json 自动派生 versionName/versionCode

- [ ] **Step 2: `README.md` 增加「Android 版」章节**：环境要求（Android 9+）、APK 下载安装（允许未知来源）、桌面↔手机数据迁移步骤（桌面导出 .enc → 传输 → 手机设置导入）、生物识别开启说明与失效规则、从源码构建（指向 AGENTS.md 命令）。

- [ ] **Step 3: 全量回归**

```powershell
cargo test; npm run build
cargo ndk --target aarch64-linux-android check -p pmgr-gui
```

预期：全部通过。

- [ ] **Step 4: Commit**

```powershell
git add AGENTS.md README.md
git commit -m "docs: AGENTS.md/README 同步 Android 版结构、命令、安全约定与发布说明"
```

---

## Task 19: 真机手测清单（用户执行，问题回修）

- [ ] **Step 1: 按清单逐项验证**（spec §9 Android 手测清单）

1. 全新安装 → 创建主密码 → 进入条目页
2. 新增/编辑/删除条目；搜索过滤
3. 复制用户名（不清除）与密码（N 秒后清除；期间复制别的内容不被误清）
4. 生成器 Tab：参数生效、复制走定时清除
5. 设置：自动锁定改 1 分钟 → 退后台 2 分钟回前台 → 已锁定回解锁屏
6. 指纹注册 → 杀进程重开 → 自动弹指纹 → 解锁成功
7. 指纹弹窗选「改用主密码」→ 密码解锁成功，注册不失效
8. 修改主密码 → 提示生物识别已关闭 → 重开不再弹指纹 → 可重新注册
9. 导出备份 → SAF 选 Download → 文件管理器可见 .enc
10. 卸载重装 → 导入该备份（输桌面/原主密码）→ 数据完整
11. 桌面端导出 .enc → 传到手机 → 手机导入 → 条目一致（跨端互通）
12. 桌面端全功能冒烟（回归）：创建/解锁/CRUD/复制/备份导出导入/改密/自动锁定

- [ ] **Step 2: 发现的问题逐个走 systematic-debugging 修复并补提交**；涉及设计变更的先更新 spec 再改码。

### 验收记录（2026-11-16，模拟器）

真机不在场期间，用 Android 模拟器（AVD pixel_6 / API 35 google_apis x86_64 / WHPX）完成本清单的自动化验收；**arm64 release APK 直装**（镜像含 arm64 转译层），全流程 adb 驱动（input/uiautomator/screencap），指纹用 `adb emu finger touch`。结果：

- 1–4、6、8–10 全部通过：创建/CRUD/搜索/复制三行为（写入、比较后清除、定时清除，用 IME 剪贴板条作 oracle）/生成器/自动锁定 1 分钟/指纹注册→冷启动自动弹→解锁/改密自动失效→可重注册/SAF 导出（Download 落盘、magic `PMGR\x01`）/卸载重装导入恢复/删除 confirm 双路径。
- 5 变体通过：前台空闲超时锁定 + 退后台回前台状态保留；深睡墙钟路径由 `should_auto_lock_dual` 单元测试覆盖。
- 7 补测通过：指纹弹窗「改用主密码」→ 密码解锁成功且注册不失效（再锁仍自动弹）。
- 11 跨端实测闭环（2026-11-16 补测，双向）：方向 B＝安卓 UI 导出 .enc → 桌面命令实体（`Vault::import_backup`）导入桌面式新库，条目逐字段一致且本地密码可重开；方向 A＝桌面命令实体（`Vault::create`+`export_backup`，即桌面 vault_create/backup_export 所调函数）生成 .enc → push 进模拟器 → 安卓 UI 走 SAF 导入，Desktop-Origin-1/2 完整替换安卓条目，详情密码/URL/中文备注/原始时间戳逐字段无损。辅助工具为临时 cargo 工程（core 路径依赖），未入仓库。
- 12 桌面冒烟：本会话未改桌面代码（仅此前 viewport 一行，已真机验证）；桌面回归由 cargo test + tsc 保证。
- 稳定性：横竖屏旋转与后台切换不丢 WebView 状态；logcat 本进程无 panic/JS error/FATAL（仅 x86 转译与 HWUI 平台噪音）。
- 方法学备注：ESC 不收此 WebView 的 IME（须 BACK）；FLAG_SECURE 表面（BiometricPrompt/PIN）截屏全黑，用 uiautomator 控件树作 oracle；模拟器 SurfaceFlinger 安全弹窗后偶发截屏全黑，POWER 循环恢复。
- 结论：**未发现新缺陷**；Task 19 真机项中除 OEM 特有行为与跨端 .enc 外均已在模拟器闭环。发布前真机复核仍建议但非阻塞。

---

## 风险与回退备忘（实施者须知）

- **`generate_handler!` 条目级 `#[cfg(mobile)]`**：Tauri 2 宏支持；若报错，回退为两份 handler 列表（Task 9 Step 2 有说明）。
- **`keystore:default` 权限名**：若 unknown permission，换 `keystore:allow-store/allow-retrieve/allow-remove` 三条（Task 6 Step 6）。
- **fs 插件对 content URI 的 scope 行为**：已按官方文档「fs 插件开箱支持任意路径格式」配置；若真机报 forbidden path，在 mobile.json 的 `fs:scope` 中确认 `content://**` 条目生效，必要时升级 `tauri-plugin-fs` 版本。
- **`tauri android init` 包名转换**：预期 `com.pmgr.password_manager`；若 CLI 行为有变（生成异常包名），用 `src-tauri/tauri.android.conf.json` 覆盖 identifier（仅影响 Android，不动桌面数据目录）。
- **biometric/keystore 插件 API 签名漂移**：以 `node_modules/@tauri-apps/plugin-biometric/dist-js/index.d.ts`、`node_modules/@impierce/tauri-plugin-keystore/dist-js/index.d.ts` 实际导出为准，前端调用处小改。
- **交互动画/样式打磨**：本计划交付功能完备的移动端 UI；视觉细节打磨可在手测后按用户反馈迭代，不阻塞主线。
