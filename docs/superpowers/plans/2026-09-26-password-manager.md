# 密码管理器（Tauri 2.x）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个跨平台（Windows/Linux）本地加密密码管理器：主密码解锁，记录应用名/用户名/密码/URL/备注，搜索、生成复杂密码、一键复制（延时自动清除）、加密备份导出/导入，打包为独立可执行文件。

**Architecture:** Tauri 2 桌面应用。Rust workspace 两个 crate：`core/`（`pmgr-core`，纯逻辑：数据模型、Argon2id+AES-256-GCM 加密、原子保存、密码生成，不依赖任何 GUI API）和 `src-tauri/`（`pmgr-gui`，IPC 命令、剪贴板、应用状态）。前端为 Vite + 原生 TypeScript（无框架），逻辑极薄，全部敏感操作经 Tauri IPC 在 Rust 侧完成，主密码与密钥永不进入 WebView。

**Tech Stack:** Rust stable (MSVC)、Tauri 2、argon2 0.5、aes-gcm 0.10、rand 0.8（OsRng）、arboard 3、zeroize、serde/serde_json、thiserror、uuid、tempfile(dev)；Vite 6、TypeScript 5、@tauri-apps/api 2、@tauri-apps/plugin-dialog 2；GitHub Actions + tauri-action。

**Spec:** `docs/superpowers/specs/2026-09-26-password-manager-design.md`

## Global Constraints

以下数值/规则来自 spec，逐字有效，所有任务隐式遵守：

- Argon2id：`m_cost=65536`（KiB，即 64 MiB）、`t_cost=3`、`p_cost=1`，派生 32 字节密钥
- AES-256-GCM：nonce 12 字节每次保存随机生成；GCM tag（16 字节）由 `aes-gcm` crate 附在密文末尾
- vault.enc 布局：magic `"PMGR"`(4B) + 版本 u8=1(1B) + salt(16B) + m/t/p 三个 u32 小端(12B) + nonce(12B) + 密文；头部共 45 字节
- 密码生成：长度 8–64（默认 20）；字符集 小写/大写/数字/符号 默认全选；易混淆字符 `0O1lI|` 默认排除；每个选中字符集至少出现一次
- 设置默认：`auto_lock_minutes=15`、`clipboard_clear_seconds=30`；两者 `0` 表示关闭
- 主密码最少 8 位（创建与修改时校验）
- JSON 字段一律 snake_case：Entry = `id/app_name/username/password/url/notes/created_at/updated_at`；GenOptions = `length/lowercase/uppercase/digits/symbols/exclude_ambiguous`
- Tauri identifier：`com.pmgr.password-manager`（决定数据目录：Windows `%APPDATA%\com.pmgr.password-manager\`，Linux `~/.config/com.pmgr.password-manager/`）
- 版本钉住：Tauri `2`、argon2 `0.5`、aes-gcm `0.10`、rand `0.8`（不要用 0.9，API 不同）、arboard `3`
- 无网络请求、无遥测；错误提示笼统（密码错误与文件损坏不可区分）
- 保存流程：写 `.tmp` → 旧文件复制为 `vault.enc.bak` → 原子 rename
- 环境注意：Windows 本机；rustup 安装后新 shell 才有 PATH，若 `cargo` 不可用先执行 `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`；首次 Rust 编译需 5–10 分钟

## Review Focus

spec 隐含但最可能咬人的输入/失败模式，及钉住它们的测试（已并入对应任务）：

1. **错误主密码 / 被篡改的 vault.enc** → 统一笼统错误、可重试、不 panic、数据不受影响 → Task 4 `decrypt_with_wrong_key_fails`、`tampered_ciphertext_fails`；Task 5 `open_with_wrong_password_fails`、`tampered_file_fails_to_open`；Task 7 命令层错误映射为笼统文案
2. **保存中途崩溃/断电** → `vault.enc` 与 `vault.enc.bak` 至少一个完整可开 → Task 5 `save_creates_bak_with_previous_content` + tmp/rename 原子写实现；Task 15 冒烟第 14 项
3. **中文/emoji/多行长备注等 Unicode 数据** → 存取无损、UI 不注入不破版 → Task 3 `vault_data_json_roundtrip_with_unicode`；Task 13 规定所有用户数据渲染必须经 `esc()`；Task 15 冒烟第 3、7 项
4. **剪贴板自动清除** → 只清除"内容仍等于所复制密码"的情况，绝不误清用户后来复制的其他内容 → Task 9 `schedule_clear` 先比较后清空（代码要求）；Task 15 冒烟第 5 项
5. **生成器极端选项**（长度 8、仅单字符集、排除混淆字符、全不选） → 每字符集至少一个或返回明确错误 → Task 6 `length_is_exact`、`no_charsets_rejected`、`only_selected_sets_appear`、`each_selected_set_represented`、`ambiguous_excluded_when_requested`

---

### Task 1: Rust 工具链 + Cargo workspace + pmgr-core 骨架

**Files:**
- Create: `Cargo.toml`（workspace 根）
- Create: `core/Cargo.toml`
- Create: `core/src/lib.rs`
- Create: `.gitignore`

**Interfaces:**
- Consumes: 无（首个任务）
- Produces: 可运行的 `cargo test -p pmgr-core`；crate 名 `pmgr-core`（后续任务都往 `core/src/` 里加模块）

- [ ] **Step 1: 安装 Rust 工具链**

```powershell
winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
```

若 winget 不可用，回退方案：从 https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe 下载并运行 `rustup-init.exe -y --default-toolchain stable-msvc`。

- [ ] **Step 2: 验证工具链**

新开 shell（或先 `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`）：

```powershell
rustc --version; cargo --version; rustup show active-toolchain
```

预期：版本输出，active-toolchain 为 `stable-x86_64-pc-windows-msvc`。若不是 msvc：`rustup default stable-msvc`。

- [ ] **Step 3: 写 `.gitignore`**

```gitignore
/target
node_modules/
dist/
src-tauri/gen/
*.log
```

- [ ] **Step 4: 写 workspace 根 `Cargo.toml`**

注意：`src-tauri` 目录在 Task 2 才创建，本任务 members 只含 `core`，Task 2 Step 1 再加入 `src-tauri`。

```toml
[workspace]
members = ["core"]
resolver = "2"
```

- [ ] **Step 5: 写 `core/Cargo.toml`**

```toml
[package]
name = "pmgr-core"
version = "0.1.0"
edition = "2021"
description = "密码管理器核心逻辑：数据模型、加密、密码生成（无 GUI 依赖）"

[dependencies]
argon2 = "0.5"
aes-gcm = "0.10"
rand = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
zeroize = { version = "1", features = ["derive"] }
thiserror = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 6: 写 `core/src/lib.rs`（骨架 + 冒烟测试）**

```rust
//! pmgr-core: 密码管理器核心逻辑（数据模型、加密、密码生成），不依赖任何 GUI API。

pub const VERSION: &str = "0.1.0";

#[cfg(test)]
mod smoke_tests {
    #[test]
    fn version_is_set() {
        assert_eq!(super::VERSION, "0.1.0");
    }
}
```

- [ ] **Step 7: 跑冒烟测试（首次编译约 1–3 分钟）**

Run: `cargo test -p pmgr-core`
Expected: `test result: ok. 1 passed`

- [ ] **Step 8: Commit**

```powershell
git add .gitignore Cargo.toml core
git commit -m "chore: Cargo workspace + pmgr-core 骨架"
```

---

### Task 2: Tauri GUI crate + Vite 前端脚手架 + 图标，空应用可运行

**Files:**
- Modify: `Cargo.toml`（workspace members 加入 src-tauri）
- Create: `package.json`、`tsconfig.json`、`vite.config.ts`
- Create: `src/index.html`（占位版，Task 10 替换为完整版）、`src/main.ts`（占位版）
- Create: `src-tauri/Cargo.toml`、`src-tauri/build.rs`、`src-tauri/src/main.rs`（最小版）、`src-tauri/tauri.conf.json`、`src-tauri/capabilities/default.json`
- Create: `scripts/make-icon.js`、`app-icon.png`、`src-tauri/icons/*`（生成）

**Interfaces:**
- Consumes: Task 1 的 workspace（`cargo test -p pmgr-core` 可跑）
- Produces: `npm run build` 产出 `dist/`；`cargo build -p pmgr-gui` 可编译；`npm run tauri dev` 打开空窗口。后续所有 Rust GUI 代码进 `src-tauri/src/`，前端代码进 `src/`。

- [ ] **Step 1: 更新 workspace `Cargo.toml`**

```toml
[workspace]
members = ["core", "src-tauri"]
resolver = "2"
```

- [ ] **Step 2: 写 `package.json`**

```json
{
  "name": "password-manager",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "typescript": "^5.6.0",
    "vite": "^6.0.0"
  }
}
```

- [ ] **Step 3: 写 `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2021",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "lib": ["ES2021", "DOM", "DOM.Iterable"]
  },
  "include": ["src"]
}
```

- [ ] **Step 4: 写 `vite.config.ts`**

```ts
import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});
```

- [ ] **Step 5: 写前端占位文件**

`src/index.html`:

```html
<!doctype html>
<html lang="zh-CN">
<head><meta charset="UTF-8" /><title>密码管理器</title></head>
<body>
  <div id="app">脚手架 OK</div>
  <script type="module" src="/main.ts"></script>
</body>
</html>
```

`src/main.ts`:

```ts
console.log("frontend bootstrap ok");
```

- [ ] **Step 6: 写 `src-tauri/Cargo.toml`**

```toml
[package]
name = "pmgr-gui"
version = "0.1.0"
description = "密码管理器（Tauri GUI）"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
arboard = "3"
zeroize = { version = "1", features = ["derive"] }
pmgr-core = { path = "../core" }
```

- [ ] **Step 7: 写 `src-tauri/build.rs`**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 8: 写 `src-tauri/src/main.rs`（最小版，Task 7 起逐步扩展）**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 9: 写 `src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "password-manager",
  "version": "0.1.0",
  "identifier": "com.pmgr.password-manager",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:5173",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "密码管理器",
        "width": 960,
        "height": 640,
        "minWidth": 720,
        "minHeight": 480
      }
    ],
    "security": {
      "csp": "default-src 'self'"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Step 10: 写 `src-tauri/capabilities/default.json`**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "主窗口权限",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:default"]
}
```

- [ ] **Step 11: 写 `scripts/make-icon.js`（纯 Node 生成 1024×1024 PNG，无第三方依赖）**

```js
// 生成 app-icon.png：透明背景 + 蓝色钥匙图形（供 `tauri icon` 生成全套图标）
const zlib = require("zlib");
const fs = require("fs");

const S = 1024;
const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}

const cx = 360, cy = 360, rOuter = 200, rInner = 110;
function isKey(x, y) {
  const dx = x - cx, dy = y - cy, d2 = dx * dx + dy * dy;
  if (d2 <= rOuter * rOuter && d2 >= rInner * rInner) return true; // 钥匙环
  if (x >= cx + rInner && x <= 900 && y >= cy - 45 && y <= cy + 45) return true; // 杆
  if (x >= 720 && x <= 780 && y >= cy + 45 && y <= cy + 150) return true; // 齿1
  if (x >= 820 && x <= 880 && y >= cy + 45 && y <= cy + 120) return true; // 齿2
  return false;
}

const raw = Buffer.alloc(S * (1 + S * 4));
let o = 0;
for (let y = 0; y < S; y++) {
  raw[o++] = 0; // filter: none
  for (let x = 0; x < S; x++) {
    if (isKey(x, y)) { raw[o++] = 0x4f; raw[o++] = 0x8c; raw[o++] = 0xff; raw[o++] = 0xff; }
    else { raw[o++] = 0; raw[o++] = 0; raw[o++] = 0; raw[o++] = 0; }
  }
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0);
ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type: RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);
fs.writeFileSync("app-icon.png", png);
console.log("app-icon.png written:", png.length, "bytes");
```

- [ ] **Step 12: 安装依赖并生成图标**

```powershell
npm install
node scripts/make-icon.js
npm run tauri icon app-icon.png
```

预期：`src-tauri/icons/` 下生成 `icon.ico`、`icon.png`、`32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.icns` 等。

- [ ] **Step 13: 验证前端构建（生成 dist/，Rust 编译需要它存在）**

Run: `npm run build`
Expected: tsc 无错误，`dist/index.html` 生成

- [ ] **Step 14: 验证 Rust 侧编译（首次约 5–10 分钟）**

Run: `cargo build -p pmgr-gui`
Expected: `Finished` 无错误

- [ ] **Step 15: 冒烟验证空窗口**

后台运行 `npm run tauri dev`，等待编译完成并出现标题为"密码管理器"的窗口、内容显示"脚手架 OK"，确认后关闭进程。

- [ ] **Step 16: Commit**

```powershell
git add -A
git commit -m "chore: Tauri 2 + Vite/TS 脚手架，空应用可运行"
```

---

### Task 3: core 数据模型（Entry / Settings / VaultData）

**Files:**
- Modify: `core/src/lib.rs`
- Create: `core/src/vault.rs`（本任务只含数据模型部分，Task 4/5 追加）

**Interfaces:**
- Consumes: `pmgr-core` crate（Task 1）
- Produces（后续任务依赖的准确签名）:
  - `pub struct Entry { pub id: String, pub app_name: String, pub username: String, pub password: String, pub url: String, pub notes: String, pub created_at: u64, pub updated_at: u64 }`（derive `Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize`）
  - `Entry::new(app_name: String, username: String, password: String, url: String, notes: String, now: u64) -> Self`（自动生成 uuid v4 字符串 id）
  - `pub struct Settings { pub auto_lock_minutes: u32, pub clipboard_clear_seconds: u32 }` + `Default`（15/30）
  - `pub struct VaultData { pub entries: Vec<Entry>, pub settings: Settings }` + `Default`

- [ ] **Step 1: 更新 `core/src/lib.rs` 为**

```rust
//! pmgr-core: 密码管理器核心逻辑（数据模型、加密、密码生成），不依赖任何 GUI API。

pub mod vault;
```

（VERSION 常量与 smoke_tests 移除。）

- [ ] **Step 2: 写失败测试——创建 `core/src/vault.rs`，先只写测试模块**

```rust
#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn entry_new_sets_id_and_timestamps() {
        let e = Entry::new("Gmail".into(), "me@gmail.com".into(), "pw".into(), "https://gmail.com".into(), "".into(), 1758844800);
        assert!(!e.id.is_empty());
        assert_eq!(e.created_at, 1758844800);
        assert_eq!(e.updated_at, 1758844800);
    }

    #[test]
    fn entry_ids_are_unique() {
        let a = Entry::new("A".into(), "".into(), "".into(), "".into(), "".into(), 0);
        let b = Entry::new("B".into(), "".into(), "".into(), "".into(), "".into(), 0);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn vault_data_json_roundtrip_with_unicode() {
        let mut data = VaultData::default();
        data.entries.push(Entry::new(
            "网银".into(),
            "用户🙂".into(),
            "pässwörd—加密".into(),
            "https://例え.jp".into(),
            "备注\n第二行\t制表".into(),
            1,
        ));
        let json = serde_json::to_string(&data).unwrap();
        let back: VaultData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, back);
        // 字段名必须是 snake_case（Global Constraints）
        assert!(json.contains("\"app_name\""));
        assert!(json.contains("\"created_at\""));
    }

    #[test]
    fn settings_defaults() {
        let s = Settings::default();
        assert_eq!(s.auto_lock_minutes, 15);
        assert_eq!(s.clipboard_clear_seconds, 30);
    }
}
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test -p pmgr-core`
Expected: 编译错误 `cannot find struct Entry`（测试先行的"失败"态）

- [ ] **Step 4: 在 `core/src/vault.rs` 顶部（测试模块之前）写实现**

```rust
//! vault: 数据模型、加密与文件存取。

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

/// 一条密码记录。JSON 字段名保持 snake_case（spec §5.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize)]
pub struct Entry {
    pub id: String,
    pub app_name: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Entry {
    pub fn new(
        app_name: String,
        username: String,
        password: String,
        url: String,
        notes: String,
        now: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            app_name,
            username,
            password,
            url,
            notes,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 应用设置。0 表示关闭对应功能（spec §5.2）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize)]
pub struct Settings {
    pub auto_lock_minutes: u32,
    pub clipboard_clear_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_lock_minutes: 15, clipboard_clear_seconds: 30 }
    }
}

/// vault.enc 解密后的完整明文内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Zeroize)]
pub struct VaultData {
    pub entries: Vec<Entry>,
    pub settings: Settings,
}

impl Default for VaultData {
    fn default() -> Self {
        Self { entries: Vec::new(), settings: Settings::default() }
    }
}
```

- [ ] **Step 5: 运行确认通过**

Run: `cargo test -p pmgr-core`
Expected: `4 passed`

- [ ] **Step 6: Commit**

```powershell
git add core
git commit -m "feat(core): Entry/Settings/VaultData 数据模型与序列化"
```

---

### Task 4: core 加密原语（Argon2id 派生 + AES-256-GCM + 文件编解码）

**Files:**
- Modify: `core/src/vault.rs`（追加）

**Interfaces:**
- Consumes: Task 3 的模型（同文件）
- Produces（Task 5、7、8 依赖）:
  - 常量 `MAGIC: &[u8;4] = b"PMGR"`、`FORMAT_VERSION: u8 = 1`、`SALT_LEN=16`、`NONCE_LEN=12`、`KEY_LEN=32`、`M_COST=65536`、`T_COST=3`、`P_COST=1`、`HEADER_LEN=45`
  - `pub enum VaultError { WrongPasswordOrCorrupt, Format(String), Io(#[from] std::io::Error), Crypto(String) }`
  - `derive_key(password: &str, salt: &[u8; SALT_LEN], m_cost: u32, t_cost: u32, p_cost: u32) -> Result<[u8; KEY_LEN], VaultError>`
  - `encrypt_payload(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<([u8; NONCE_LEN], Vec<u8>), VaultError>`
  - `decrypt_payload(key: &[u8; KEY_LEN], nonce: &[u8; NONCE_LEN], ciphertext: &[u8]) -> Result<Vec<u8>, VaultError>`
  - `encode_file(salt: &[u8; SALT_LEN], params: (u32,u32,u32), nonce: &[u8; NONCE_LEN], ciphertext: &[u8]) -> Vec<u8>`
  - `decode_file(bytes: &[u8]) -> Result<FileParts, VaultError>`；`pub struct FileParts { pub salt: [u8; SALT_LEN], pub params: (u32,u32,u32), pub nonce: [u8; NONCE_LEN], pub ciphertext: Vec<u8> }`

- [ ] **Step 1: 在 `core/src/vault.rs` 文件末尾追加失败测试**

```rust
#[cfg(test)]
mod crypto_tests {
    use super::*;

    // 测试用低开销参数加速（生产参数见 Global Constraints）
    const TM: u32 = 4096;
    const TT: u32 = 1;
    const TP: u32 = 1;

    #[test]
    fn derive_key_deterministic_and_salt_sensitive() {
        let salt = [7u8; SALT_LEN];
        let k1 = derive_key("pw", &salt, TM, TT, TP).unwrap();
        let k2 = derive_key("pw", &salt, TM, TT, TP).unwrap();
        let k3 = derive_key("pw2", &salt, TM, TT, TP).unwrap();
        let k4 = derive_key("pw", &[8u8; SALT_LEN], TM, TT, TP).unwrap();
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        assert_ne!(k1, k4);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [3u8; KEY_LEN];
        let (nonce, ct) = encrypt_payload(&key, b"hello vault").unwrap();
        let pt = decrypt_payload(&key, &nonce, &ct).unwrap();
        assert_eq!(pt, b"hello vault");
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key = [3u8; KEY_LEN];
        let wrong = [4u8; KEY_LEN];
        let (nonce, ct) = encrypt_payload(&key, b"secret").unwrap();
        assert!(matches!(
            decrypt_payload(&wrong, &nonce, &ct),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [3u8; KEY_LEN];
        let (nonce, mut ct) = encrypt_payload(&key, b"secret").unwrap();
        ct[0] ^= 0xff;
        assert!(matches!(
            decrypt_payload(&key, &nonce, &ct),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
    }

    #[test]
    fn encode_decode_roundtrip() {
        let salt = [1u8; SALT_LEN];
        let nonce = [2u8; NONCE_LEN];
        let ct = vec![9u8; 64];
        let bytes = encode_file(&salt, (M_COST, T_COST, P_COST), &nonce, &ct);
        assert_eq!(bytes.len(), HEADER_LEN + 64);
        let parts = decode_file(&bytes).unwrap();
        assert_eq!(parts.salt, salt);
        assert_eq!(parts.params, (M_COST, T_COST, P_COST));
        assert_eq!(parts.nonce, nonce);
        assert_eq!(parts.ciphertext, ct);
    }

    #[test]
    fn decode_rejects_bad_magic_and_short_input() {
        assert!(matches!(decode_file(b"PM"), Err(VaultError::Format(_))));
        let mut bad = encode_file(&[1u8; SALT_LEN], (M_COST, T_COST, P_COST), &[2u8; NONCE_LEN], &[9u8; 64]);
        bad[0] = b'X';
        assert!(matches!(decode_file(&bad), Err(VaultError::Format(_))));
        let mut badver = encode_file(&[1u8; SALT_LEN], (M_COST, T_COST, P_COST), &[2u8; NONCE_LEN], &[9u8; 64]);
        badver[4] = 99;
        assert!(matches!(decode_file(&badver), Err(VaultError::Format(_))));
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pmgr-core`
Expected: 编译错误（`derive_key`、`VaultError` 等不存在）

- [ ] **Step 3: 实现——在 `core/src/vault.rs` 的 use 区（`use zeroize::Zeroize;` 之后）追加**

```rust
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use thiserror::Error;
```

并在数据模型代码之后、测试模块之前追加：

```rust
// ---------- 文件格式与加密原语（spec §5.3） ----------

pub const MAGIC: &[u8; 4] = b"PMGR";
pub const FORMAT_VERSION: u8 = 1;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;
pub const M_COST: u32 = 65536; // KiB（64 MiB）
pub const T_COST: u32 = 3;
pub const P_COST: u32 = 1;
pub const HEADER_LEN: usize = 4 + 1 + SALT_LEN + 12 + NONCE_LEN; // 45

#[derive(Debug, Error)]
pub enum VaultError {
    /// 安全要求：密码错误与文件损坏不可区分，统一用此变体
    #[error("主密码错误或数据文件已损坏")]
    WrongPasswordOrCorrupt,
    #[error("数据文件格式无效: {0}")]
    Format(String),
    #[error("文件读写失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("加密操作失败: {0}")]
    Crypto(String),
}

pub fn derive_key(
    password: &str,
    salt: &[u8; SALT_LEN],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<[u8; KEY_LEN], VaultError> {
    let params = Params::new(m_cost, t_cost, p_cost, Some(KEY_LEN))
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    Ok(key)
}

pub fn encrypt_payload(
    key: &[u8; KEY_LEN],
    plaintext: &[u8],
) -> Result<([u8; NONCE_LEN], Vec<u8>), VaultError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = Aes256Gcm::new(key.into());
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    Ok((nonce_bytes, ciphertext))
}

pub fn decrypt_payload(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| VaultError::WrongPasswordOrCorrupt)
}

pub fn encode_file(
    salt: &[u8; SALT_LEN],
    params: (u32, u32, u32),
    nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(FORMAT_VERSION);
    out.extend_from_slice(salt);
    out.extend_from_slice(&params.0.to_le_bytes());
    out.extend_from_slice(&params.1.to_le_bytes());
    out.extend_from_slice(&params.2.to_le_bytes());
    out.extend_from_slice(nonce);
    out.extend_from_slice(ciphertext);
    out
}

pub struct FileParts {
    pub salt: [u8; SALT_LEN],
    pub params: (u32, u32, u32),
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

pub fn decode_file(bytes: &[u8]) -> Result<FileParts, VaultError> {
    if bytes.len() < HEADER_LEN {
        return Err(VaultError::Format("文件过短".into()));
    }
    if &bytes[0..4] != MAGIC {
        return Err(VaultError::Format("不是密码管理器数据文件".into()));
    }
    if bytes[4] != FORMAT_VERSION {
        return Err(VaultError::Format(format!("不支持的版本 {}", bytes[4])));
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[5..5 + SALT_LEN]);
    let o = 5 + SALT_LEN;
    let m = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let t = u32::from_le_bytes(bytes[o + 4..o + 8].try_into().unwrap());
    let p = u32::from_le_bytes(bytes[o + 8..o + 12].try_into().unwrap());
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&bytes[o + 12..o + 12 + NONCE_LEN]);
    let ciphertext = bytes[HEADER_LEN..].to_vec();
    Ok(FileParts { salt, params: (m, t, p), nonce, ciphertext })
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pmgr-core`
Expected: `10 passed`（4 模型 + 6 加密）

- [ ] **Step 5: Commit**

```powershell
git add core
git commit -m "feat(core): Argon2id 密钥派生 + AES-256-GCM 加解密 + vault 文件编解码"
```

---

### Task 5: core Vault 文件操作（create/open/save/.bak/改密/导出/导入）

**Files:**
- Modify: `core/src/vault.rs`（追加）

**Interfaces:**
- Consumes: Task 4 的 `derive_key/encrypt_payload/decrypt_payload/encode_file/decode_file/FileParts/VaultError`
- Produces（Task 7/8 依赖）:
  - `pub struct Vault { pub data: VaultData, /* 私有: path, salt, params, key */ }`
  - `Vault::create(path: impl Into<PathBuf>, password: &str) -> Result<Self, VaultError>`
  - `Vault::open(path: impl Into<PathBuf>, password: &str) -> Result<Self, VaultError>`
  - `vault.save(&mut self) -> Result<(), VaultError>`
  - `vault.change_password(&mut self, old: &str, new: &str) -> Result<(), VaultError>`
  - `vault.import_backup(&mut self, src: &Path, password: &str) -> Result<(), VaultError>`
  - 自由函数 `export_backup(vault_path: &Path, dest: &Path) -> Result<(), VaultError>`、`decrypt_backup(src: &Path, password: &str) -> Result<VaultData, VaultError>`
  - `impl Drop for Vault`：zeroize 密钥与明文数据

- [ ] **Step 1: 在 `core/src/vault.rs` 文件末尾追加失败测试**

注意：使用生产 Argon2id 参数，每次派生约 0.3–1 秒，本任务全部测试约需 15–40 秒，属正常。

```rust
#[cfg(test)]
mod vault_tests {
    use super::*;
    use std::path::Path;

    fn vp(dir: &Path) -> PathBuf {
        dir.join("vault.enc")
    }

    #[test]
    fn create_then_open_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        {
            let mut v = Vault::create(&path, "master-pass-1").unwrap();
            assert!(path.exists());
            v.data.entries.push(Entry::new(
                "Gmail".into(), "me".into(), "pw1".into(),
                "https://gmail.com".into(), "备注".into(), 42,
            ));
            v.data.settings.auto_lock_minutes = 7;
            v.save().unwrap();
        }
        let v2 = Vault::open(&path, "master-pass-1").unwrap();
        assert_eq!(v2.data.entries.len(), 1);
        assert_eq!(v2.data.entries[0].app_name, "Gmail");
        assert_eq!(v2.data.entries[0].notes, "备注");
        assert_eq!(v2.data.settings.auto_lock_minutes, 7);
    }

    #[test]
    fn open_with_wrong_password_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        Vault::create(&path, "right-pass-1").unwrap();
        let err = Vault::open(&path, "wrong-pass-1").unwrap_err();
        assert!(matches!(err, VaultError::WrongPasswordOrCorrupt));
    }

    #[test]
    fn save_creates_bak_with_previous_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        let bak = dir.path().join("vault.enc.bak");
        let mut v = Vault::create(&path, "master-pass-1").unwrap();
        assert!(!bak.exists()); // create 的首次保存时旧文件不存在 → 无 .bak
        v.data.entries.push(Entry::new("A".into(), "".into(), "".into(), "".into(), "".into(), 1));
        v.save().unwrap();
        assert!(bak.exists());
        let old = Vault::open(&bak, "master-pass-1").unwrap();
        assert_eq!(old.data.entries.len(), 0); // .bak 是上一版（空库）
        v.data.entries.push(Entry::new("B".into(), "".into(), "".into(), "".into(), "".into(), 2));
        v.save().unwrap();
        let old2 = Vault::open(&bak, "master-pass-1").unwrap();
        assert_eq!(old2.data.entries.len(), 1); // .bak 滚动为上一版（1 条）
        assert_eq!(old2.data.entries[0].app_name, "A");
    }

    #[test]
    fn tampered_file_fails_to_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        Vault::create(&path, "master-pass-1").unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff; // 篡改 GCM tag 区域
        std::fs::write(&path, &bytes).unwrap();
        let err = Vault::open(&path, "master-pass-1").unwrap_err();
        assert!(matches!(err, VaultError::WrongPasswordOrCorrupt));
    }

    #[test]
    fn change_password_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        let mut v = Vault::create(&path, "old-pass-123").unwrap();
        v.data.entries.push(Entry::new("Bank".into(), "u".into(), "p".into(), "".into(), "".into(), 5));
        v.save().unwrap();
        // 旧密码不对 → 拒绝
        assert!(matches!(
            v.change_password("wrong-old", "new-pass-456"),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
        v.change_password("old-pass-123", "new-pass-456").unwrap();
        drop(v);
        assert!(matches!(Vault::open(&path, "old-pass-123"), Err(VaultError::WrongPasswordOrCorrupt)));
        let v2 = Vault::open(&path, "new-pass-456").unwrap();
        assert_eq!(v2.data.entries.len(), 1);
        assert_eq!(v2.data.entries[0].app_name, "Bank");
    }

    #[test]
    fn export_backup_is_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = vp(dir.path());
        Vault::create(&path, "master-pass-1").unwrap();
        let dest = dir.path().join("backup.enc");
        export_backup(&path, &dest).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), std::fs::read(&dest).unwrap());
    }

    #[test]
    fn import_backup_replaces_data_under_current_password() {
        let dir = tempfile::tempdir().unwrap();
        // 当前库：password-A1，条目 AppA
        let path_a = dir.path().join("a.enc");
        let mut va = Vault::create(&path_a, "password-A1").unwrap();
        va.data.entries.push(Entry::new("AppA".into(), "".into(), "".into(), "".into(), "".into(), 1));
        va.save().unwrap();
        // 备份文件：password-B2，条目 AppB
        let path_b = dir.path().join("b.enc");
        {
            let mut vb = Vault::create(&path_b, "password-B2").unwrap();
            vb.data.entries.push(Entry::new("AppB".into(), "".into(), "".into(), "".into(), "".into(), 2));
            vb.save().unwrap();
        }
        // 错误备份密码 → 拒绝，当前数据不变
        assert!(va.import_backup(&path_b, "wrong-pass").is_err());
        assert_eq!(va.data.entries.len(), 1);
        assert_eq!(va.data.entries[0].app_name, "AppA");
        // 正确备份密码 → 替换数据并用当前主密码重加密落盘
        va.import_backup(&path_b, "password-B2").unwrap();
        assert_eq!(va.data.entries.len(), 1);
        assert_eq!(va.data.entries[0].app_name, "AppB");
        drop(va);
        let reopened = Vault::open(&path_a, "password-A1").unwrap();
        assert_eq!(reopened.data.entries[0].app_name, "AppB");
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pmgr-core`
Expected: 编译错误（`Vault`、`export_backup` 不存在）

- [ ] **Step 3: 实现——use 区追加**

```rust
use std::path::{Path, PathBuf};
```

在加密原语之后、测试模块之前追加：

```rust
// ---------- Vault：解锁后的内存句柄 + 文件操作（spec §5.3、§6.4） ----------

/// 已解锁的 vault。Drop 时 zeroize 密钥与全部明文。
pub struct Vault {
    path: PathBuf,
    salt: [u8; SALT_LEN],
    params: (u32, u32, u32),
    key: [u8; KEY_LEN],
    pub data: VaultData,
}

impl Vault {
    /// 新建 vault 文件并立即落盘（空数据）。
    pub fn create(path: impl Into<PathBuf>, password: &str) -> Result<Self, VaultError> {
        let path = path.into();
        let mut salt = [0u8; SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let params = (M_COST, T_COST, P_COST);
        let key = derive_key(password, &salt, params.0, params.1, params.2)?;
        let mut v = Self { path, salt, params, key, data: VaultData::default() };
        v.save()?;
        Ok(v)
    }

    /// 读取并用主密码解锁。密码错误或文件损坏统一返回 WrongPasswordOrCorrupt。
    pub fn open(path: impl Into<PathBuf>, password: &str) -> Result<Self, VaultError> {
        let path = path.into();
        let bytes = std::fs::read(&path)?;
        let parts = decode_file(&bytes)?;
        let key = derive_key(password, &parts.salt, parts.params.0, parts.params.1, parts.params.2)?;
        let plaintext = decrypt_payload(&key, &parts.nonce, &parts.ciphertext)?;
        let data: VaultData =
            serde_json::from_slice(&plaintext).map_err(|_| VaultError::WrongPasswordOrCorrupt)?;
        Ok(Self { path, salt: parts.salt, params: parts.params, key, data })
    }

    /// 保存：写 .tmp → 旧文件留 .bak → 原子 rename（spec §5.3 保存流程）。
    pub fn save(&mut self) -> Result<(), VaultError> {
        let plaintext =
            serde_json::to_vec(&self.data).map_err(|e| VaultError::Crypto(e.to_string()))?;
        let (nonce, ciphertext) = encrypt_payload(&self.key, &plaintext)?;
        let bytes = encode_file(&self.salt, self.params, &nonce, &ciphertext);
        let tmp = self.path.with_extension("enc.tmp");
        std::fs::write(&tmp, &bytes)?;
        if self.path.exists() {
            std::fs::copy(&self.path, self.path.with_extension("enc.bak"))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// 修改主密码：验证旧密码 → 新 salt 重派生 → 重加密落盘（spec §5.3）。
    pub fn change_password(&mut self, old: &str, new: &str) -> Result<(), VaultError> {
        let check = derive_key(old, &self.salt, self.params.0, self.params.1, self.params.2)?;
        if check != self.key {
            return Err(VaultError::WrongPasswordOrCorrupt);
        }
        let mut salt = [0u8; SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        self.key = derive_key(new, &salt, self.params.0, self.params.1, self.params.2)?;
        self.salt = salt;
        self.save()
    }

    /// 导入备份：用备份自己的主密码验证解密 → 替换数据 → 用当前密钥重加密落盘（spec §6.4）。
    pub fn import_backup(&mut self, src: &Path, password: &str) -> Result<(), VaultError> {
        let imported = decrypt_backup(src, password)?;
        self.data = imported;
        self.save()
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        self.data.zeroize();
        self.key.zeroize();
    }
}

/// 导出备份 = 原样复制加密文件（永不导出明文）。
pub fn export_backup(vault_path: &Path, dest: &Path) -> Result<(), VaultError> {
    std::fs::copy(vault_path, dest)?;
    Ok(())
}

/// 验证并解密一个备份/vault 文件，返回明文数据（不改变任何文件）。
pub fn decrypt_backup(src: &Path, password: &str) -> Result<VaultData, VaultError> {
    let bytes = std::fs::read(src)?;
    let parts = decode_file(&bytes)?;
    let key = derive_key(password, &parts.salt, parts.params.0, parts.params.1, parts.params.2)?;
    let plaintext = decrypt_payload(&key, &parts.nonce, &parts.ciphertext)?;
    serde_json::from_slice(&plaintext).map_err(|_| VaultError::WrongPasswordOrCorrupt)
}
```

- [ ] **Step 4: 运行确认通过（耗时较长属正常）**

Run: `cargo test -p pmgr-core`
Expected: `17 passed`

- [ ] **Step 5: Commit**

```powershell
git add core
git commit -m "feat(core): Vault 创建/解锁/原子保存/.bak/改密/备份导出导入"
```

---

### Task 6: core 密码生成器

**Files:**
- Modify: `core/src/lib.rs`（加 `pub mod generator;`）
- Create: `core/src/generator.rs`

**Interfaces:**
- Consumes: 无（独立模块）
- Produces（Task 8 依赖）:
  - `pub struct GenOptions { pub length: usize, pub lowercase: bool, pub uppercase: bool, pub digits: bool, pub symbols: bool, pub exclude_ambiguous: bool }`（derive `Debug, Clone, Serialize, Deserialize`，`Default` = 20/全选/排除混淆）
  - `generate(opts: &GenOptions) -> Result<String, GenError>`
  - `pub enum GenError { InvalidLength, NoCharSets, EmptySet }`
  - 常量 `LOWER/UPPER/DIGITS/SYMBOLS/AMBIGUOUS/MIN_LENGTH(8)/MAX_LENGTH(64)`

- [ ] **Step 1: `core/src/lib.rs` 追加一行**

```rust
pub mod generator;
```

- [ ] **Step 2: 创建 `core/src/generator.rs`，先只写测试模块（文件末尾）**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_produce_20_chars() {
        let pw = generate(&GenOptions::default()).unwrap();
        assert_eq!(pw.chars().count(), 20);
    }

    #[test]
    fn length_is_exact() {
        for len in [MIN_LENGTH, 16, MAX_LENGTH] {
            let o = GenOptions { length: len, ..Default::default() };
            assert_eq!(generate(&o).unwrap().chars().count(), len);
        }
    }

    #[test]
    fn invalid_length_rejected() {
        let o = GenOptions { length: 7, ..Default::default() };
        assert!(matches!(generate(&o), Err(GenError::InvalidLength)));
        let o = GenOptions { length: 65, ..Default::default() };
        assert!(matches!(generate(&o), Err(GenError::InvalidLength)));
    }

    #[test]
    fn no_charsets_rejected() {
        let o = GenOptions {
            lowercase: false, uppercase: false, digits: false, symbols: false,
            ..Default::default()
        };
        assert!(matches!(generate(&o), Err(GenError::NoCharSets)));
    }

    #[test]
    fn only_selected_sets_appear() {
        let o = GenOptions {
            lowercase: false, uppercase: false, digits: true, symbols: false,
            ..Default::default()
        };
        let pw = generate(&o).unwrap();
        assert!(pw.chars().all(|c| DIGITS.contains(c)));
    }

    #[test]
    fn ambiguous_excluded_when_requested() {
        let o = GenOptions { exclude_ambiguous: true, ..Default::default() };
        for _ in 0..20 {
            let pw = generate(&o).unwrap();
            assert!(!pw.chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn ambiguous_allowed_when_not_excluded() {
        // 仅数字、不排除混淆：大量抽样应见到 '0' 和 '1'
        let o = GenOptions {
            length: 64, lowercase: false, uppercase: false, digits: true,
            symbols: false, exclude_ambiguous: false,
        };
        let (mut seen0, mut seen1) = (false, false);
        for _ in 0..50 {
            let pw = generate(&o).unwrap();
            seen0 |= pw.contains('0');
            seen1 |= pw.contains('1');
        }
        assert!(seen0 && seen1);
    }

    #[test]
    fn each_selected_set_represented() {
        let o = GenOptions::default();
        for _ in 0..50 {
            let pw = generate(&o).unwrap();
            assert!(pw.chars().any(|c| LOWER.contains(c) && !AMBIGUOUS.contains(c)));
            assert!(pw.chars().any(|c| UPPER.contains(c) && !AMBIGUOUS.contains(c)));
            assert!(pw.chars().any(|c| DIGITS.contains(c) && !AMBIGUOUS.contains(c)));
            assert!(pw.chars().any(|c| SYMBOLS.contains(c)));
        }
    }

    #[test]
    fn two_calls_differ() {
        let a = generate(&GenOptions::default()).unwrap();
        let b = generate(&GenOptions::default()).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn options_json_is_snake_case_with_defaults() {
        let json = serde_json::to_string(&GenOptions::default()).unwrap();
        assert!(json.contains("\"length\":20"));
        assert!(json.contains("\"exclude_ambiguous\":true"));
    }
}
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test -p pmgr-core generator`
Expected: 编译错误（`GenOptions`、`generate` 不存在）

- [ ] **Step 4: 实现——在 `core/src/generator.rs` 测试模块之前写**

```rust
//! 密码生成器：系统级 CSPRNG（OsRng），拒绝采样保证每个选中字符集至少一个字符。

use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
pub const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const DIGITS: &str = "0123456789";
pub const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>?";
pub const AMBIGUOUS: &str = "0O1lI|";
pub const MIN_LENGTH: usize = 8;
pub const MAX_LENGTH: usize = 64;

#[derive(Debug, Error)]
pub enum GenError {
    #[error("长度必须在 {MIN_LENGTH}-{MAX_LENGTH} 之间")]
    InvalidLength,
    #[error("至少选择一个字符集")]
    NoCharSets,
    #[error("某字符集过滤后为空")]
    EmptySet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for GenOptions {
    fn default() -> Self {
        Self {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        }
    }
}

fn filter_set(set: &str, exclude_ambiguous: bool) -> Vec<char> {
    set.chars()
        .filter(|c| !exclude_ambiguous || !AMBIGUOUS.contains(*c))
        .collect()
}

pub fn generate(opts: &GenOptions) -> Result<String, GenError> {
    if opts.length < MIN_LENGTH || opts.length > MAX_LENGTH {
        return Err(GenError::InvalidLength);
    }
    let mut sets: Vec<Vec<char>> = Vec::new();
    if opts.lowercase { sets.push(filter_set(LOWER, opts.exclude_ambiguous)); }
    if opts.uppercase { sets.push(filter_set(UPPER, opts.exclude_ambiguous)); }
    if opts.digits { sets.push(filter_set(DIGITS, opts.exclude_ambiguous)); }
    if opts.symbols { sets.push(filter_set(SYMBOLS, opts.exclude_ambiguous)); }
    if sets.is_empty() {
        return Err(GenError::NoCharSets);
    }
    if sets.iter().any(|s| s.is_empty()) {
        return Err(GenError::EmptySet);
    }

    let mut rng = rand::rngs::OsRng;
    let mut chars: Vec<char> = Vec::with_capacity(opts.length);
    // 每个选中字符集先保证一个字符
    for set in &sets {
        chars.push(set[rng.gen_range(0..set.len())]);
    }
    // 其余从合并池抽取
    let pool: Vec<char> = sets.concat();
    while chars.len() < opts.length {
        chars.push(pool[rng.gen_range(0..pool.len())]);
    }
    // Fisher-Yates 洗牌，消除"前几位必来自各字符集"的位置规律
    for i in (1..chars.len()).rev() {
        let j = rng.gen_range(0..=i);
        chars.swap(i, j);
    }
    Ok(chars.into_iter().collect())
}
```

- [ ] **Step 5: 运行确认通过**

Run: `cargo test -p pmgr-core`
Expected: `27 passed`

- [ ] **Step 6: Commit**

```powershell
git add core
git commit -m "feat(core): 密码生成器（OsRng + 拒绝采样 + Fisher-Yates）"
```

---

### Task 7: GUI 状态管理 + vault 生命周期命令（含自动锁定线程）

**Files:**
- Create: `src-tauri/src/state.rs`、`src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`（整体替换）

**Interfaces:**
- Consumes: `pmgr_core::vault::Vault`（Task 5）
- Produces（Task 8/9 追加命令；Task 10 前端调用）:
  - `state::AppState { pub vault: Mutex<Option<Vault>>, pub last_activity: Mutex<Instant> }`、`AppState::new()`、`state.touch()`
  - `state::should_auto_lock(elapsed: Duration, timeout_minutes: u32) -> bool`
  - `state::spawn_auto_lock_thread(app: AppHandle)`（每 30s 检查，超时锁定并 emit 事件 `"vault-locked"`，payload 字符串 `"timeout"`）
  - 命令：`vault_status -> Result<VaultStatus,String>`（`VaultStatus { exists: bool, unlocked: bool }`）、`vault_create(master_password)`、`vault_unlock(master_password)`、`vault_lock()`、`activity_ping()`
  - `commands::vault_path(app: &AppHandle) -> Result<PathBuf, String>`（app_data_dir/vault.enc，自动建目录；Task 8 也用）

- [ ] **Step 1: 写失败测试——创建 `src-tauri/src/state.rs`，先只含纯函数与测试**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `npm run build`（确保 `dist/` 存在，`cargo test -p pmgr-gui` 编译需要它），然后 `cargo test -p pmgr-gui`
Expected: 编译错误（`should_auto_lock` 不存在）

- [ ] **Step 3: 实现 `src-tauri/src/state.rs`（在测试模块之前）**

```rust
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
```

- [ ] **Step 4: 创建 `src-tauri/src/commands.rs`（vault 生命周期部分）**

```rust
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
```

- [ ] **Step 5: 整体替换 `src-tauri/src/main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

fn main() {
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
            commands::activity_ping
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p pmgr-gui`
Expected: `test result: ok. 1 passed`（should_auto_lock）
（若报 `../dist` 不存在：先 `npm run build`。）

- [ ] **Step 7: Commit**

```powershell
git add src-tauri
git commit -m "feat(gui): 应用状态、vault 生命周期命令、自动锁定线程"
```

---

### Task 8: GUI 条目/设置/生成器/备份命令

**Files:**
- Modify: `src-tauri/src/commands.rs`（追加）
- Modify: `src-tauri/src/main.rs`（invoke_handler 扩充）

**Interfaces:**
- Consumes: Task 7 的 `AppState/vault_path`；`pmgr_core::vault::{Entry, Settings, Vault, export_backup}`、`pmgr_core::generator::{GenOptions, generate}`
- Produces（Task 10 前端 api.ts 调用）: 命令 `entries_list() -> Result<Vec<Entry>,String>`、`entry_save(entry: Entry) -> Result<Entry,String>`（id 为空 = 新建）、`entry_delete(id)`、`settings_get() -> Result<Settings,String>`、`settings_save(settings)`、`password_generate(opts: GenOptions) -> Result<String,String>`、`backup_export(dest_path)`、`backup_import(src_path, backup_password)`、`master_password_change(old_password, new_password)`；内部辅助 `with_vault<T>(state, f) -> Result<T,String>`（未解锁返回 `"未解锁"`）、`now_secs() -> u64`

- [ ] **Step 1: 追加实现——`commands.rs` use 区改为（合并原有行）**

```rust
use pmgr_core::generator::{self, GenOptions};
use pmgr_core::vault::{self, Entry, Settings, Vault};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroize;

use crate::state::AppState;
```

文件末尾追加：

```rust
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
```

- [ ] **Step 2: 更新 `main.rs` 的 invoke_handler 为**

```rust
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
            commands::master_password_change
        ])
```

- [ ] **Step 3: 编译验证**

Run: `cargo test -p pmgr-gui`
Expected: 编译通过，`1 passed`（自动锁定测试不受影响）

- [ ] **Step 4: Commit**

```powershell
git add src-tauri
git commit -m "feat(gui): 条目/设置/密码生成/备份/改密 IPC 命令"
```

---

### Task 9: GUI 剪贴板（复制 + 定时安全清除）

**Files:**
- Create: `src-tauri/src/clipboard.rs`
- Modify: `src-tauri/src/commands.rs`（追加 clipboard_copy 命令）
- Modify: `src-tauri/src/main.rs`（mod + handler）

**Interfaces:**
- Consumes: Task 7/8 的 `AppState/with_vault`
- Produces（Task 10 前端调用）: 命令 `clipboard_copy(text: String, secret: bool) -> Result<(), String>`；`clipboard::copy_text(text: &str) -> Result<(), String>`、`clipboard::schedule_clear(text: String, seconds: u32)`
- **Review Focus #4 代码要求**：`schedule_clear` 到期后必须先读取剪贴板，**仅当内容仍等于所复制文本时**才清空，绝不误清用户后来复制的其他内容

- [ ] **Step 1: 创建 `src-tauri/src/clipboard.rs`**

```rust
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
```

- [ ] **Step 2: `commands.rs` 末尾追加**

```rust
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
```

- [ ] **Step 3: `main.rs` 更新**

mod 区改为：

```rust
mod clipboard;
mod commands;
mod state;
```

invoke_handler 列表末尾追加一行：

```rust
            commands::clipboard_copy
```

- [ ] **Step 4: 编译验证**

Run: `cargo test -p pmgr-gui`
Expected: 编译通过，`1 passed`

- [ ] **Step 5: Commit**

```powershell
git add src-tauri
git commit -m "feat(gui): 剪贴板复制与定时安全清除（比较后清除，防误清）"
```

---

### Task 10: 前端基础——api 封装、完整 HTML/CSS、解锁屏

**Files:**
- Modify: `src/index.html`（整体替换为完整版）、`src/main.ts`（v1）
- Create: `src/styles.css`、`src/api.ts`、`src/state.ts`、`src/ui/toast.ts`、`src/ui/screens.ts`、`src/ui/lock.ts`

**Interfaces:**
- Consumes: Task 7–9 的全部 IPC 命令
- Produces（Task 11–14 依赖）:
  - `api.ts`: `Entry/Settings/GenOptions/VaultStatus` 接口 + 函数 `vaultStatus() vaultCreate(masterPassword) vaultUnlock(masterPassword) vaultLock() entriesList() entrySave(entry) entryDelete(id) settingsGet() settingsSave(settings) passwordGenerate(opts) clipboardCopy(text, secret) backupExport(destPath) backupImport(srcPath, backupPassword) masterPasswordChange(oldPassword, newPassword) activityPing()`
  - `state.ts`: `state = { entries: Entry[], selectedId: string|null, query: string, settings: Settings, showPassword: boolean }`
  - `ui/toast.ts`: `toast(msg: string, ms?: number): void`
  - `ui/screens.ts`: `showScreen(name: "lock" | "main"): void`
  - `ui/lock.ts`: `initLock(onUnlocked: () => Promise<void>): Promise<void>`（可重复调用，内部 wired 防重复绑定）

- [ ] **Step 1: 整体替换 `src/index.html`（含后续任务的对话框结构，一次到位）**

```html
<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>密码管理器</title>
<link rel="stylesheet" href="/styles.css" />
</head>
<body>
  <!-- 解锁 / 首次创建主密码 -->
  <section id="screen-lock" class="screen hidden">
    <div class="lock-box">
      <h1>🔐 密码管理器</h1>
      <div id="lock-create-fields" class="hidden">
        <p class="hint">首次使用，请创建主密码（至少 8 位）。主密码无法找回，请务必牢记！</p>
        <input type="password" id="create-pw1" placeholder="主密码" autocomplete="off" />
        <input type="password" id="create-pw2" placeholder="确认主密码" autocomplete="off" />
        <div id="create-strength" class="hint"></div>
        <button id="btn-create" class="primary block" type="button">创建并进入</button>
      </div>
      <div id="lock-unlock-fields" class="hidden">
        <input type="password" id="unlock-pw" placeholder="主密码" autocomplete="off" />
        <button id="btn-unlock" class="primary block" type="button">解锁</button>
        <p class="hint">忘记密码？无法找回。若有加密备份文件且记得其主密码，可重置数据后导入恢复。</p>
      </div>
      <div id="lock-error" class="error hidden"></div>
    </div>
  </section>

  <!-- 主界面 -->
  <section id="screen-main" class="screen hidden">
    <header id="toolbar">
      <input type="search" id="search" placeholder="搜索应用名 / 用户名 / URL / 备注…" />
      <button id="btn-add" class="primary" type="button">＋ 新增条目</button>
      <button id="btn-settings" type="button">⚙ 设置</button>
      <button id="btn-lock" type="button">🔒 锁定</button>
    </header>
    <div id="main-body">
      <aside id="entry-list"></aside>
      <main id="entry-detail"></main>
    </div>
  </section>

  <!-- 条目编辑对话框（含密码生成器面板） -->
  <div id="dialog-editor" class="dialog-backdrop hidden">
    <div class="dialog">
      <h2 id="ed-title">新增条目</h2>
      <label>应用名 *<input id="ed-app" type="text" placeholder="例如：Gmail、网银" /></label>
      <label>用户名<input id="ed-username" type="text" autocomplete="off" /></label>
      <label>密码
        <span class="row">
          <input id="ed-password" type="text" autocomplete="off" />
          <button id="ed-generate" class="small" type="button">生成…</button>
        </span>
      </label>
      <div id="ed-gen" class="gen-panel hidden">
        <label class="inline">长度 <input id="gen-length" type="range" min="8" max="64" value="20" /> <span id="gen-length-val">20</span></label>
        <label class="inline"><input id="gen-lower" type="checkbox" checked /> 小写 a-z</label>
        <label class="inline"><input id="gen-upper" type="checkbox" checked /> 大写 A-Z</label>
        <label class="inline"><input id="gen-digits" type="checkbox" checked /> 数字 0-9</label>
        <label class="inline"><input id="gen-symbols" type="checkbox" checked /> 符号 !@#$…</label>
        <label class="inline"><input id="gen-amb" type="checkbox" checked /> 排除易混淆字符 0O1lI|</label>
        <output id="gen-out" class="gen-out"></output>
        <div class="row">
          <button id="gen-go" class="small" type="button">重新生成</button>
          <button id="gen-use" class="small primary" type="button">使用此密码</button>
        </div>
      </div>
      <label>URL<input id="ed-url" type="text" placeholder="https://…" /></label>
      <label>备注<textarea id="ed-notes" rows="3"></textarea></label>
      <div class="row right">
        <button id="ed-cancel" type="button">取消</button>
        <button id="ed-save" class="primary" type="button">保存</button>
      </div>
    </div>
  </div>

  <!-- 设置对话框 -->
  <div id="dialog-settings" class="dialog-backdrop hidden">
    <div class="dialog">
      <h2>设置</h2>
      <h3>常规</h3>
      <label>自动锁定超时（分钟，0 = 关闭）<input id="set-autolock" type="number" min="0" max="720" /></label>
      <label>剪贴板自动清除（秒，0 = 关闭）<input id="set-clip" type="number" min="0" max="3600" /></label>
      <button id="set-save" class="primary" type="button">保存设置</button>
      <h3>修改主密码</h3>
      <label>当前主密码<input id="set-oldpw" type="password" autocomplete="off" /></label>
      <label>新主密码（至少 8 位）<input id="set-newpw" type="password" autocomplete="off" /></label>
      <label>确认新主密码<input id="set-newpw2" type="password" autocomplete="off" /></label>
      <button id="set-chpw" type="button">修改主密码</button>
      <h3>备份</h3>
      <div class="row">
        <button id="set-export" type="button">导出加密备份…</button>
        <button id="set-import" type="button">导入备份（覆盖当前数据）…</button>
      </div>
      <div class="row right"><button id="set-cancel" type="button">关闭</button></div>
    </div>
  </div>

  <div id="toast" class="hidden"></div>
  <script type="module" src="/main.ts"></script>
</body>
</html>
```

- [ ] **Step 2: 写 `src/styles.css`（完整，一次到位）**

```css
:root {
  --bg: #1e2128; --panel: #262a33; --panel2: #2d323d; --border: #3a404d;
  --text: #e8eaf0; --muted: #9aa1b0; --accent: #4f8cff; --danger: #e5534b;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body { height: 100%; }
body {
  font-family: "Segoe UI", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif;
  background: var(--bg); color: var(--text); font-size: 14px;
}
.hidden { display: none !important; }
.screen { height: 100vh; display: flex; flex-direction: column; }

/* 解锁屏 */
#screen-lock { justify-content: center; align-items: center; }
.lock-box {
  background: var(--panel); border: 1px solid var(--border); border-radius: 12px;
  padding: 36px 40px; width: 400px; display: flex; flex-direction: column; gap: 12px;
}
.lock-box h1 { font-size: 20px; text-align: center; margin-bottom: 8px; }
.hint { color: var(--muted); font-size: 12px; line-height: 1.6; }
.error { color: var(--danger); font-size: 13px; }

/* 表单控件 */
input, textarea {
  background: var(--bg); color: var(--text); border: 1px solid var(--border);
  border-radius: 6px; padding: 8px 10px; font-size: 14px; font-family: inherit; width: 100%;
}
input:focus, textarea:focus { outline: none; border-color: var(--accent); }
button {
  background: var(--panel2); color: var(--text); border: 1px solid var(--border);
  border-radius: 6px; padding: 8px 14px; font-size: 13px; cursor: pointer;
}
button:hover { border-color: var(--accent); }
button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
button.danger { background: var(--danger); border-color: var(--danger); color: #fff; }
button.small { padding: 3px 8px; font-size: 12px; }
button.block { width: 100%; }

/* 工具栏与主区域 */
#toolbar {
  display: flex; gap: 8px; padding: 10px 12px;
  background: var(--panel); border-bottom: 1px solid var(--border);
}
#toolbar #search { flex: 1; }
#main-body { flex: 1; display: flex; min-height: 0; }
#entry-list {
  width: 300px; overflow-y: auto; border-right: 1px solid var(--border); background: var(--panel);
}
.entry-item { padding: 10px 14px; border-bottom: 1px solid var(--border); cursor: pointer; }
.entry-item:hover { background: var(--panel2); }
.entry-item.selected {
  background: var(--panel2); border-left: 3px solid var(--accent); padding-left: 11px;
}
.entry-app { font-weight: 600; word-break: break-all; }
.entry-user { color: var(--muted); font-size: 12px; margin-top: 2px; word-break: break-all; }
#entry-detail { flex: 1; overflow-y: auto; padding: 20px 24px; }
#entry-detail h2 { margin-bottom: 14px; word-break: break-all; }
table.detail { width: 100%; border-collapse: collapse; margin-bottom: 14px; }
table.detail th {
  text-align: left; color: var(--muted); font-weight: normal; width: 70px;
  padding: 7px 10px 7px 0; vertical-align: top;
}
table.detail td { padding: 7px 0; word-break: break-all; }
table.detail code {
  background: var(--panel); padding: 3px 8px; border-radius: 4px;
  margin-right: 6px; display: inline-block; max-width: 100%; word-break: break-all;
}
.notes-cell { white-space: pre-wrap; }
.detail-actions { display: flex; gap: 8px; margin-bottom: 10px; }

/* 对话框 */
.dialog-backdrop {
  position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); z-index: 10;
  display: flex; justify-content: center; align-items: flex-start;
  padding: 40px 0; overflow-y: auto;
}
.dialog {
  background: var(--panel); border: 1px solid var(--border); border-radius: 12px;
  padding: 24px 28px; width: 480px; max-width: 92vw;
  display: flex; flex-direction: column; gap: 10px;
}
.dialog h2 { font-size: 17px; margin-bottom: 4px; }
.dialog h3 {
  font-size: 14px; color: var(--muted); margin: 10px 0 2px;
  border-top: 1px solid var(--border); padding-top: 12px;
}
.dialog label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted); }
.dialog label.inline { flex-direction: row; align-items: center; gap: 6px; color: var(--text); }
.dialog label input[type="checkbox"], .dialog label.inline input[type="range"] { width: auto; }
.gen-panel {
  background: var(--bg); border: 1px solid var(--border); border-radius: 8px;
  padding: 12px; display: flex; flex-direction: column; gap: 8px;
}
.gen-out {
  font-family: Consolas, monospace; background: var(--panel2); padding: 8px;
  border-radius: 6px; word-break: break-all; min-height: 34px;
}
.row { display: flex; gap: 8px; align-items: center; }
.row.right { justify-content: flex-end; }
.row input[type="text"] { flex: 1; }
#ed-password { flex: 1; }

/* Toast */
#toast {
  position: fixed; bottom: 28px; left: 50%; transform: translateX(-50%);
  background: var(--panel2); border: 1px solid var(--accent); color: var(--text);
  padding: 10px 18px; border-radius: 8px; z-index: 99; box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
}
```

- [ ] **Step 3: 写 `src/api.ts`（完整 IPC 封装）**

```ts
import { invoke } from "@tauri-apps/api/core";

export interface Entry {
  id: string;
  app_name: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  created_at: number;
  updated_at: number;
}

export interface Settings {
  auto_lock_minutes: number;
  clipboard_clear_seconds: number;
}

export interface GenOptions {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  exclude_ambiguous: boolean;
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export const vaultStatus = (): Promise<VaultStatus> => invoke("vault_status");
export const vaultCreate = (masterPassword: string): Promise<void> =>
  invoke("vault_create", { masterPassword });
export const vaultUnlock = (masterPassword: string): Promise<void> =>
  invoke("vault_unlock", { masterPassword });
export const vaultLock = (): Promise<void> => invoke("vault_lock");
export const activityPing = (): Promise<void> => invoke("activity_ping");

export const entriesList = (): Promise<Entry[]> => invoke("entries_list");
export const entrySave = (entry: Entry): Promise<Entry> => invoke("entry_save", { entry });
export const entryDelete = (id: string): Promise<void> => invoke("entry_delete", { id });
export const settingsGet = (): Promise<Settings> => invoke("settings_get");
export const settingsSave = (settings: Settings): Promise<void> =>
  invoke("settings_save", { settings });
export const passwordGenerate = (opts: GenOptions): Promise<string> =>
  invoke("password_generate", { opts });
export const clipboardCopy = (text: string, secret: boolean): Promise<void> =>
  invoke("clipboard_copy", { text, secret });
export const backupExport = (destPath: string): Promise<void> =>
  invoke("backup_export", { destPath });
export const backupImport = (srcPath: string, backupPassword: string): Promise<void> =>
  invoke("backup_import", { srcPath, backupPassword });
export const masterPasswordChange = (oldPassword: string, newPassword: string): Promise<void> =>
  invoke("master_password_change", { oldPassword, newPassword });
```

- [ ] **Step 4: 写 `src/state.ts`、`src/ui/toast.ts`、`src/ui/screens.ts`**

`src/state.ts`:

```ts
import type { Entry, Settings } from "./api";

export interface UiState {
  entries: Entry[];
  selectedId: string | null;
  query: string;
  settings: Settings;
  showPassword: boolean;
}

export const state: UiState = {
  entries: [],
  selectedId: null,
  query: "",
  settings: { auto_lock_minutes: 15, clipboard_clear_seconds: 30 },
  showPassword: false,
};
```

`src/ui/toast.ts`:

```ts
let timer: number | undefined;

export function toast(msg: string, ms = 3000): void {
  const el = document.getElementById("toast")!;
  el.textContent = msg;
  el.classList.remove("hidden");
  window.clearTimeout(timer);
  timer = window.setTimeout(() => el.classList.add("hidden"), ms);
}
```

`src/ui/screens.ts`:

```ts
export function showScreen(name: "lock" | "main"): void {
  document.getElementById("screen-lock")!.classList.toggle("hidden", name !== "lock");
  document.getElementById("screen-main")!.classList.toggle("hidden", name !== "main");
}
```

- [ ] **Step 5: 写 `src/ui/lock.ts`**

```ts
import * as api from "../api";
import { showScreen } from "./screens";

let wired = false;

function strength(pw: string): string {
  let score = 0;
  if (pw.length >= 8) score++;
  if (pw.length >= 14) score++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) score++;
  if (/\d/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw)) score++;
  return ["很弱", "弱", "中等", "强", "很强", "很强"][score];
}

/** 显示解锁屏（vault 不存在则显示创建表单）。可重复调用；事件只绑定一次。 */
export async function initLock(onUnlocked: () => Promise<void>): Promise<void> {
  const status = await api.vaultStatus();
  document.getElementById("lock-create-fields")!.classList.toggle("hidden", status.exists);
  document.getElementById("lock-unlock-fields")!.classList.toggle("hidden", !status.exists);
  for (const id of ["create-pw1", "create-pw2", "unlock-pw"]) {
    (document.getElementById(id) as HTMLInputElement).value = "";
  }
  document.getElementById("create-strength")!.textContent = "";
  document.getElementById("lock-error")!.classList.add("hidden");
  showScreen("lock");
  if (wired) return;
  wired = true;

  const errEl = document.getElementById("lock-error")!;
  const showErr = (msg: string): void => {
    errEl.textContent = msg;
    errEl.classList.remove("hidden");
  };

  document.getElementById("btn-create")!.onclick = async () => {
    const pw1 = (document.getElementById("create-pw1") as HTMLInputElement).value;
    const pw2 = (document.getElementById("create-pw2") as HTMLInputElement).value;
    if (pw1.length < 8) { showErr("主密码至少 8 位"); return; }
    if (pw1 !== pw2) { showErr("两次输入的主密码不一致"); return; }
    try {
      await api.vaultCreate(pw1);
      await onUnlocked();
    } catch (e) {
      showErr(String(e));
    }
  };

  const doUnlock = async (): Promise<void> => {
    const pw = (document.getElementById("unlock-pw") as HTMLInputElement).value;
    if (!pw) return;
    try {
      await api.vaultUnlock(pw);
      await onUnlocked();
    } catch {
      showErr("主密码错误，或数据文件已损坏");
    }
  };
  document.getElementById("btn-unlock")!.onclick = () => { void doUnlock(); };
  document.getElementById("unlock-pw")!.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void doUnlock();
  });

  document.getElementById("create-pw1")!.addEventListener("input", (e) => {
    const v = (e.target as HTMLInputElement).value;
    document.getElementById("create-strength")!.textContent = v ? `强度：${strength(v)}` : "";
  });
}
```

- [ ] **Step 6: 写 `src/main.ts`（v1：解锁流程；Task 13/14 会整体替换）**

```ts
import * as api from "./api";
import { initLock } from "./ui/lock";
import { toast } from "./ui/toast";

async function onUnlocked(): Promise<void> {
  toast("解锁成功（主界面在后续任务接入）");
}

async function boot(): Promise<void> {
  const st = await api.vaultStatus();
  if (st.unlocked) {
    await onUnlocked();
    return;
  }
  await initLock(onUnlocked);
}

void boot();
```

- [ ] **Step 7: 类型检查 + 构建验证**

Run: `npm run build`
Expected: tsc 无错误，vite 构建成功

- [ ] **Step 8: 手动验证解锁屏**

Run: `npm run tauri dev`（后台），验证：
1. 首次启动显示"创建主密码"表单
2. 输入 7 位密码点创建 → 提示"主密码至少 8 位"
3. 两次不一致 → 提示"两次输入的主密码不一致"
4. 输入一致的 ≥8 位密码 → toast "解锁成功（主界面在后续任务接入）"
5. 关闭重开 → 显示"解锁"表单；错误密码 → "主密码错误，或数据文件已损坏"；正确密码 → 解锁成功 toast
6. 强度提示随输入变化

验证后删除测试数据：`Remove-Item "$env:APPDATA\com.pmgr.password-manager" -Recurse -Force`，关闭 dev 进程。

- [ ] **Step 9: Commit**

```powershell
git add src package.json tsconfig.json
git commit -m "feat(ui): api 封装、完整界面骨架、解锁/创建主密码屏"
```

---

### Task 11: 前端条目编辑对话框 + 密码生成器面板

**Files:**
- Create: `src/ui/editor.ts`

**Interfaces:**
- Consumes: `api.entrySave/passwordGenerate`、`toast`
- Produces（Task 13 的 list.ts 调用）: `openEditor(entry: api.Entry | null, onSaved: (e: api.Entry) => void | Promise<void>): void`（entry=null 为新增；保存成功后回调已保存的 Entry 并关闭对话框）

- [ ] **Step 1: 写 `src/ui/editor.ts`**

```ts
import * as api from "../api";
import { toast } from "./toast";

function getVal(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}
function setVal(id: string, v: string): void {
  (document.getElementById(id) as HTMLInputElement).value = v;
}
function checked(id: string): boolean {
  return (document.getElementById(id) as HTMLInputElement).checked;
}

async function regenerate(): Promise<void> {
  const opts: api.GenOptions = {
    length: Number(getVal("gen-length")) || 20,
    lowercase: checked("gen-lower"),
    uppercase: checked("gen-upper"),
    digits: checked("gen-digits"),
    symbols: checked("gen-symbols"),
    exclude_ambiguous: checked("gen-amb"),
  };
  const out = document.getElementById("gen-out") as HTMLOutputElement;
  try {
    const pw = await api.passwordGenerate(opts);
    out.textContent = pw;
    out.dataset.pw = pw; // 真正的密码存 dataset；textContent 可能显示错误信息
  } catch (err) {
    out.textContent = String(err);
    delete out.dataset.pw;
  }
}

/** 打开条目编辑对话框。entry=null 表示新增。 */
export function openEditor(
  entry: api.Entry | null,
  onSaved: (e: api.Entry) => void | Promise<void>,
): void {
  const dlg = document.getElementById("dialog-editor")!;
  document.getElementById("ed-title")!.textContent = entry ? "编辑条目" : "新增条目";
  setVal("ed-app", entry?.app_name ?? "");
  setVal("ed-username", entry?.username ?? "");
  setVal("ed-password", entry?.password ?? "");
  setVal("ed-url", entry?.url ?? "");
  setVal("ed-notes", entry?.notes ?? "");
  document.getElementById("ed-gen")!.classList.add("hidden");
  dlg.classList.remove("hidden");

  document.getElementById("ed-cancel")!.onclick = () => dlg.classList.add("hidden");

  document.getElementById("ed-save")!.onclick = async () => {
    const appName = getVal("ed-app").trim();
    if (!appName) { toast("应用名不能为空"); return; }
    const payload: api.Entry = {
      id: entry?.id ?? "",
      app_name: appName,
      username: getVal("ed-username").trim(),
      password: getVal("ed-password"),
      url: getVal("ed-url").trim(),
      notes: getVal("ed-notes"),
      created_at: entry?.created_at ?? 0,
      updated_at: 0,
    };
    try {
      const saved = await api.entrySave(payload);
      dlg.classList.add("hidden");
      await onSaved(saved);
      toast("已保存");
    } catch (err) {
      toast(`保存失败：${String(err)}`);
    }
  };

  document.getElementById("ed-generate")!.onclick = () => {
    document.getElementById("ed-gen")!.classList.remove("hidden");
    void regenerate();
  };
  document.getElementById("gen-go")!.onclick = () => { void regenerate(); };
  document.getElementById("gen-use")!.onclick = () => {
    const out = document.getElementById("gen-out") as HTMLOutputElement;
    const pw = out.dataset.pw ?? "";
    if (!pw) { toast("请先生成密码"); return; }
    setVal("ed-password", pw);
    document.getElementById("ed-gen")!.classList.add("hidden");
  };
  document.getElementById("gen-length")!.addEventListener("input", (e) => {
    document.getElementById("gen-length-val")!.textContent =
      (e.target as HTMLInputElement).value;
  });
}
```

- [ ] **Step 2: 类型检查**

Run: `npm run build`
Expected: tsc 无错误

- [ ] **Step 3: Commit**

```powershell
git add src/ui/editor.ts
git commit -m "feat(ui): 条目编辑对话框与密码生成器面板"
```

---

### Task 12: 前端设置对话框（常规设置/改密/导出/导入）

**Files:**
- Create: `src/ui/settings.ts`

**Interfaces:**
- Consumes: `api.settingsSave/masterPasswordChange/backupExport/backupImport`、`@tauri-apps/plugin-dialog` 的 `save/open/confirm/prompt`、`state.settings`、`toast`
- Produces（Task 13 调用）: `openSettings(): Promise<void>`；导入成功后 `location.reload()`（Rust 侧保持解锁，boot 会直接进入主界面）

- [ ] **Step 1: 写 `src/ui/settings.ts`**

```ts
import {
  save as saveDialog,
  open as openDialog,
  confirm as confirmDialog,
  prompt as promptDialog,
} from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import { state } from "../state";
import { toast } from "./toast";

function numVal(id: string): number {
  const v = Number((document.getElementById(id) as HTMLInputElement).value);
  return Number.isFinite(v) && v >= 0 ? Math.floor(v) : 0;
}
function pwVal(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}
function clearPw(): void {
  for (const id of ["set-oldpw", "set-newpw", "set-newpw2"]) {
    (document.getElementById(id) as HTMLInputElement).value = "";
  }
}

export async function openSettings(): Promise<void> {
  const dlg = document.getElementById("dialog-settings")!;
  (document.getElementById("set-autolock") as HTMLInputElement).value =
    String(state.settings.auto_lock_minutes);
  (document.getElementById("set-clip") as HTMLInputElement).value =
    String(state.settings.clipboard_clear_seconds);
  clearPw();
  dlg.classList.remove("hidden");

  document.getElementById("set-cancel")!.onclick = () => dlg.classList.add("hidden");

  document.getElementById("set-save")!.onclick = async () => {
    const s: api.Settings = {
      auto_lock_minutes: numVal("set-autolock"),
      clipboard_clear_seconds: numVal("set-clip"),
    };
    try {
      await api.settingsSave(s);
      state.settings = s;
      toast("设置已保存");
    } catch (err) {
      toast(`保存失败：${String(err)}`);
    }
  };

  document.getElementById("set-chpw")!.onclick = async () => {
    const oldPw = pwVal("set-oldpw");
    const newPw = pwVal("set-newpw");
    if (newPw.length < 8) { toast("新主密码至少 8 位"); return; }
    if (newPw !== pwVal("set-newpw2")) { toast("两次输入的新密码不一致"); return; }
    try {
      await api.masterPasswordChange(oldPw, newPw);
      clearPw();
      toast("主密码已修改");
    } catch (err) {
      toast(String(err));
    }
  };

  document.getElementById("set-export")!.onclick = async () => {
    const stamp = new Date().toISOString().slice(0, 10).replace(/-/g, "");
    const path = await saveDialog({
      defaultPath: `pmgr-backup-${stamp}.enc`,
      filters: [{ name: "加密备份", extensions: ["enc"] }],
    });
    if (!path) return;
    try {
      await api.backupExport(path);
      toast("备份已导出（加密文件，请妥善保管）");
    } catch (err) {
      toast(`导出失败：${String(err)}`);
    }
  };

  document.getElementById("set-import")!.onclick = async () => {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "加密备份", extensions: ["enc"] }],
    });
    if (!path) return;
    const backupPw = await promptDialog("请输入该备份文件的主密码：", {
      title: "导入备份",
      okLabel: "下一步",
      cancelLabel: "取消",
    });
    if (backupPw === null) return;
    const ok = await confirmDialog(
      "导入将覆盖当前全部数据（当前数据文件会保留 .bak 备份）。确定继续？",
      { title: "确认导入", kind: "warning" },
    );
    if (!ok) return;
    try {
      await api.backupImport(String(path), backupPw);
      dlg.classList.add("hidden");
      toast("导入成功，正在重新载入…");
      window.setTimeout(() => location.reload(), 800);
    } catch (err) {
      toast(`导入失败：${String(err)}`);
    }
  };
}
```

- [ ] **Step 2: 类型检查**

Run: `npm run build`
Expected: tsc 无错误

- [ ] **Step 3: Commit**

```powershell
git add src/ui/settings.ts
git commit -m "feat(ui): 设置对话框（常规/改主密码/导出/导入备份）"
```

---

### Task 13: 前端主界面（列表/搜索/详情/复制）+ 全流程接线

**Files:**
- Create: `src/ui/list.ts`
- Modify: `src/main.ts`（整体替换为 v2）

**Interfaces:**
- Consumes: `state`、`api.entriesList/settingsGet/entryDelete/clipboardCopy/vaultLock`、`openEditor`（Task 11）、`openSettings`（Task 12）、`initLock`、`showScreen`、`toast`、plugin-dialog 的 `confirm`
- Produces（Task 14 依赖）: `enterMain(): Promise<void>`（加载数据、渲染、切到主界面）、`initMain(onUnlocked): void`（一次性绑定工具栏）、`esc(s: string): string`（HTML 转义）、`filteredEntries(): api.Entry[]`、`renderAll(): void`
- **Review Focus #3 代码要求**：所有来自用户数据的字符串插入 innerHTML 前必须经 `esc()`

- [ ] **Step 1: 写 `src/ui/list.ts`**

```ts
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import { state } from "../state";
import { openEditor } from "./editor";
import { initLock } from "./lock";
import { showScreen } from "./screens";
import { openSettings } from "./settings";
import { toast } from "./toast";

/** Review Focus #3：所有用户数据入 innerHTML 前必须经此转义 */
export function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export async function enterMain(): Promise<void> {
  state.entries = await api.entriesList();
  state.settings = await api.settingsGet();
  state.query = "";
  state.showPassword = false;
  (document.getElementById("search") as HTMLInputElement).value = "";
  state.selectedId = state.entries.length > 0 ? state.entries[0].id : null;
  showScreen("main");
  renderAll();
}

export function filteredEntries(): api.Entry[] {
  const q = state.query.trim().toLowerCase();
  if (!q) return state.entries;
  return state.entries.filter((e) =>
    [e.app_name, e.username, e.url, e.notes].some((f) => f.toLowerCase().includes(q)),
  );
}

export function renderAll(): void {
  renderList();
  renderDetail();
}

function fmtTime(secs: number): string {
  return secs === 0 ? "—" : new Date(secs * 1000).toLocaleString();
}

function renderList(): void {
  const el = document.getElementById("entry-list")!;
  const items = filteredEntries();
  el.innerHTML = items.length
    ? items
        .map(
          (e) => `
      <div class="entry-item ${e.id === state.selectedId ? "selected" : ""}" data-id="${e.id}">
        <div class="entry-app">${esc(e.app_name)}</div>
        <div class="entry-user">${esc(e.username)}</div>
      </div>`,
        )
        .join("")
    : `<p class="hint" style="padding:12px">${
        state.query ? "无匹配条目" : "还没有条目，点击右上「＋ 新增条目」"
      }</p>`;
  el.querySelectorAll<HTMLElement>(".entry-item").forEach((item) => {
    item.onclick = () => {
      state.selectedId = item.dataset.id!;
      state.showPassword = false;
      renderAll();
    };
  });
}

function fieldRow(label: string, valueHtml: string, copyText: string): string {
  return `<tr><th>${label}</th><td><code>${valueHtml}</code>` +
    `<button class="small btn-copy" data-copy="${encodeURIComponent(copyText)}">复制</button></td></tr>`;
}

function renderDetail(): void {
  const el = document.getElementById("entry-detail")!;
  const e = state.entries.find((x) => x.id === state.selectedId);
  if (!e) {
    el.innerHTML = `<p class="hint">在左侧选择一个条目，或点击「＋ 新增条目」</p>`;
    return;
  }
  const dots = "•".repeat(Math.max(Math.min(e.password.length, 24), 8));
  const pwShown = state.showPassword ? esc(e.password) : dots;
  el.innerHTML = `
    <h2>${esc(e.app_name)}</h2>
    <table class="detail">
      ${fieldRow("用户名", esc(e.username), e.username)}
      <tr><th>密码</th><td>
        <code>${pwShown}</code>
        <button class="small" id="btn-toggle-pw" type="button">${state.showPassword ? "隐藏" : "显示"}</button>
        <button class="small" id="btn-copy-pw" type="button">复制</button>
      </td></tr>
      ${e.url ? fieldRow("URL", esc(e.url), e.url) : ""}
      ${e.notes ? `<tr><th>备注</th><td class="notes-cell">${esc(e.notes)}</td></tr>` : ""}
    </table>
    <div class="detail-actions">
      <button class="primary" id="btn-edit" type="button">编辑</button>
      <button class="danger" id="btn-delete" type="button">删除</button>
    </div>
    <p class="hint">创建：${fmtTime(e.created_at)} ・ 更新：${fmtTime(e.updated_at)}</p>`;

  el.querySelectorAll<HTMLButtonElement>(".btn-copy").forEach((b) => {
    b.onclick = async () => {
      await api.clipboardCopy(decodeURIComponent(b.dataset.copy!), false);
      toast("已复制");
    };
  });
  el.querySelector<HTMLButtonElement>("#btn-toggle-pw")!.onclick = () => {
    state.showPassword = !state.showPassword;
    renderDetail();
  };
  el.querySelector<HTMLButtonElement>("#btn-copy-pw")!.onclick = async () => {
    await api.clipboardCopy(e.password, true);
    const s = state.settings.clipboard_clear_seconds;
    toast(s > 0 ? `密码已复制，${s} 秒后自动清除剪贴板` : "密码已复制");
  };
  el.querySelector<HTMLButtonElement>("#btn-edit")!.onclick = () =>
    openEditor(e, onEditorSaved);
  el.querySelector<HTMLButtonElement>("#btn-delete")!.onclick = async () => {
    const ok = await confirmDialog(`确定删除「${e.app_name}」的条目？此操作不可恢复。`, {
      title: "确认删除",
      kind: "warning",
    });
    if (!ok) return;
    try {
      await api.entryDelete(e.id);
      state.entries = state.entries.filter((x) => x.id !== e.id);
      state.selectedId = state.entries.length > 0 ? state.entries[0].id : null;
      renderAll();
      toast("已删除");
    } catch (err) {
      toast(`删除失败：${String(err)}`);
    }
  };
}

async function onEditorSaved(saved: api.Entry): Promise<void> {
  const i = state.entries.findIndex((x) => x.id === saved.id);
  if (i >= 0) state.entries[i] = saved;
  else state.entries.unshift(saved);
  state.selectedId = saved.id;
  state.showPassword = false;
  renderAll();
}

let wired = false;

/** 一次性绑定工具栏事件。 */
export function initMain(onUnlocked: () => Promise<void>): void {
  if (wired) return;
  wired = true;
  document.getElementById("search")!.addEventListener("input", (ev) => {
    state.query = (ev.target as HTMLInputElement).value;
    renderList();
  });
  document.getElementById("btn-add")!.onclick = () => openEditor(null, onEditorSaved);
  document.getElementById("btn-settings")!.onclick = () => { void openSettings(); };
  document.getElementById("btn-lock")!.onclick = async () => {
    await api.vaultLock();
    await initLock(onUnlocked);
  };
}
```

- [ ] **Step 2: 整体替换 `src/main.ts`（v2：接入主界面；Task 14 再替换为最终版）**

```ts
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import { initLock } from "./ui/lock";
import { enterMain, initMain } from "./ui/list";

async function onUnlocked(): Promise<void> {
  await enterMain();
}

async function boot(): Promise<void> {
  initMain(onUnlocked);
  const st = await api.vaultStatus();
  if (st.unlocked) await enterMain();
  else await initLock(onUnlocked);
  // Rust 侧自动锁定线程触发时回到解锁屏
  await listen("vault-locked", () => { void initLock(onUnlocked); });
}

void boot();
```

- [ ] **Step 3: 类型检查**

Run: `npm run build`
Expected: tsc 无错误

- [ ] **Step 4: 手动验证全流程**

Run: `npm run tauri dev`（若上一步删过测试数据则重新创建主密码），验证：
1. 新增条目（应用名含中文+emoji、多行备注）→ 保存 → 列表出现
2. 应用名留空保存 → toast "应用名不能为空"
3. 点"生成…" → 面板出现且有密码；"重新生成"结果不同；改长度滑块数字联动；"使用此密码"回填
4. 搜索中文/用户名片段 → 实时过滤；清空恢复
5. 详情：密码打码；"显示/隐藏"切换；复制用户名 → toast"已复制"，粘贴验证；复制密码 → toast 提示 30 秒清除
6. 编辑改用户名 → 保存 → 更新时间变化；删除 → 确认框 → 取消不删、确认删
7. 锁定 → 回解锁屏 → 解锁 → 数据完整
8. 设置：改剪贴板清除为 5 秒保存 → 复制密码 → 5 秒后粘贴为空

验证后关闭 dev 进程（测试数据可保留给 Task 15 冒烟用，也可删除）。

- [ ] **Step 5: Commit**

```powershell
git add src
git commit -m "feat(ui): 主界面（列表/搜索/详情/复制）与全流程接线"
```

---

### Task 14: 前端自动锁定（活动跟踪 + 超时锁定）

**Files:**
- Modify: `src/main.ts`（整体替换为最终版）

**Interfaces:**
- Consumes: `api.activityPing/vaultStatus/vaultLock`、`state.settings.auto_lock_minutes`、`initLock`
- Produces: 完整的前端自动锁定（15s 检查一次；活动事件节流 10s ping 一次 Rust 兜底）

- [ ] **Step 1: 整体替换 `src/main.ts`（最终版）**

```ts
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import { state } from "./state";
import { initLock } from "./ui/lock";
import { enterMain, initMain } from "./ui/list";

async function onUnlocked(): Promise<void> {
  await enterMain();
}

let lastActivity = Date.now();
let lastPing = 0;

function bump(): void {
  lastActivity = Date.now();
  // 节流：最多每 10 秒向 Rust 侧汇报一次活动（Rust 兜底锁定用）
  if (lastActivity - lastPing > 10_000) {
    lastPing = lastActivity;
    api.activityPing().catch(() => {});
  }
}

async function boot(): Promise<void> {
  initMain(onUnlocked);
  const st = await api.vaultStatus();
  if (st.unlocked) await enterMain();
  else await initLock(onUnlocked);
  // Rust 侧自动锁定线程触发时回到解锁屏
  await listen("vault-locked", () => { void initLock(onUnlocked); });

  window.addEventListener("keydown", bump);
  window.addEventListener("mousedown", bump);

  // 前端自动锁定：每 15 秒检查一次无活动时长
  window.setInterval(() => {
    const mins = state.settings.auto_lock_minutes;
    if (mins <= 0 || Date.now() - lastActivity <= mins * 60_000) return;
    void (async () => {
      const s = await api.vaultStatus().catch(() => null);
      if (s?.unlocked) {
        await api.vaultLock().catch(() => {});
        await initLock(onUnlocked);
      }
    })();
  }, 15_000);
}

void boot();
```

- [ ] **Step 2: 类型检查**

Run: `npm run build`
Expected: tsc 无错误

- [ ] **Step 3: 手动验证自动锁定**

Run: `npm run tauri dev`，解锁后：设置自动锁定为 1 分钟并保存 → 完全不碰鼠标键盘约 75 秒 → 界面自动回到解锁屏；重新解锁数据完整。再把设置改回 15 分钟。

- [ ] **Step 4: Commit**

```powershell
git add src/main.ts
git commit -m "feat(ui): 前端自动锁定（活动跟踪 + 节流上报 + 超时锁定）"
```

---

### Task 15: Windows 打包（NSIS）+ 完整冒烟清单

**Files:**
- 无新文件（构建产物在 `target/`，不入库）

**Interfaces:**
- Consumes: 全部前序任务
- Produces: `target/release/bundle/nsis/password-manager_0.1.0_x64-setup.exe`；已验证的冒烟清单

- [ ] **Step 1: 全量测试**

Run: `cargo test`（workspace 全部）
Expected: pmgr-core `27 passed`；pmgr-gui `1 passed`

- [ ] **Step 2: 发布构建（约 5–15 分钟）**

Run: `npm run tauri build`
Expected: 无错误；产物 `target\release\bundle\nsis\password-manager_0.1.0_x64-setup.exe` 存在

- [ ] **Step 3: 安装并执行完整冒烟清单**

运行安装程序（当前用户安装即可），然后逐项验证：

1. 首次启动 → 创建主密码屏；<8 位拒绝；两次不一致报错
2. 创建 `Smoke-Test-123` → 进入主界面（空列表提示）
3. 新增条目：应用名 `网银🙂`、用户名 `user@example.com`、点"生成…"→ 长度默认 20、"重新生成"两次结果不同 → "使用此密码"回填；URL `https://bank.example.com`；多行中文备注 → 保存 → 列表出现
4. 新增时应用名留空 → 拒绝
5. 详情：密码打码；"显示"→明文；复制密码 → toast 提示 30 秒 → 立即粘贴验证内容 → 等 35 秒再粘贴 → 已被清除；再复制密码后马上手动复制一段别的文字 → 等 35 秒 → 别的文字仍在（防误清，Review Focus #4）
6. 搜索 `网银` / `example` → 过滤；清空恢复
7. 编辑改用户名 → 保存 → 详情更新、`更新`时间变化、`创建`时间不变；备注含 `<script>alert(1)</script>` → 详情按纯文本显示不弹窗（Review Focus #3）
8. 删除 → 确认框 → 取消不删；确认删除成功
9. 锁定 → 错误密码 → 笼统错误；正确密码 → 数据完整（Review Focus #1）
10. 设置自动锁定 1 分钟 → 无操作 75 秒 → 自动回锁定屏 → 改回 15
11. 修改主密码：旧密码错误 → "旧主密码不正确"；正确修改 → 锁定后旧密码失败、新密码成功
12. 导出备份到桌面 → 文件生成且与 `%APPDATA%\com.pmgr.password-manager\vault.enc` 字节一致；删除某条目 → 导入备份（输当时的密码）→ 确认 → 重载后条目恢复
13. 关闭应用重开 → 锁定屏（不自动解锁）→ 解锁后数据完整
14. 任务管理器强杀进程（模拟崩溃）→ 重开 → 用主密码能正常解锁（vault.enc 或 .bak 完整，Review Focus #2）；若损坏，将 `.bak` 复制为 `vault.enc` 后可解锁

- [ ] **Step 4: 清理冒烟数据（可选，或保留）**

```powershell
Remove-Item "$env:APPDATA\com.pmgr.password-manager" -Recurse -Force -ErrorAction SilentlyContinue
```

- [ ] **Step 5: Commit（若有修正）**

冒烟发现的任何 bug：修复 → 重跑相关测试 → `git add -A; git commit -m "fix: 冒烟发现的问题 <描述>"`。无问题则跳过。

---

### Task 16: GitHub Actions 双平台发布 workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: 完整仓库（tag 触发）
- Produces: push `v*` tag 时自动构建 Windows（NSIS）+ Linux（AppImage、deb），上传到 GitHub Releases 草稿

- [ ] **Step 1: 写 `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

permissions:
  contents: write

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: windows-latest
            args: "--bundles nsis"
          - platform: ubuntu-22.04
            args: "--bundles appimage,deb"
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Linux dependencies
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
            libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev

      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm

      - uses: dtolnay/rust-toolchain@stable

      - uses: swatinem/rust-cache@v2

      - name: Install frontend dependencies
        run: npm ci

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Password Manager v__VERSION__"
          releaseBody: "Windows: 下载 -setup.exe 安装。Linux: 下载 AppImage（chmod +x 后运行）或 .deb（sudo apt install ./xxx.deb）。"
          releaseDraft: true
          prerelease: false
          args: ${{ matrix.args }}
```

- [ ] **Step 2: 本地校验 YAML 语法**

Run: `node -e "const fs=require('fs');const s=fs.readFileSync('.github/workflows/release.yml','utf8');if(!s.includes('tauri-action'))throw new Error('bad');console.log('yaml ok,',s.split('\n').length,'lines')"`
Expected: `yaml ok`

- [ ] **Step 3: Commit**

```powershell
git add .github
git commit -m "ci: GitHub Actions 双平台发布（Windows NSIS + Linux AppImage/deb）"
```

说明：首次真实验证在用户 push 到 GitHub 并打 tag 时进行（`git tag v0.1.0 && git push --tags`）；仓库尚未关联远端，此步骤留给用户操作，README 中说明。

---

### Task 17: README + 最终校验

**Files:**
- Create: `README.md`

**Interfaces:**
- Consumes: 全部
- Produces: 面向使用者与构建者的完整文档

- [ ] **Step 1: 写 `README.md`**

```markdown
# 密码管理器（Password Manager）

跨平台（Windows / Linux）本地加密密码管理器。Tauri 2 构建：体积小、内存占用低、无需安装任何运行时。

## 功能

- 🔐 主密码解锁（Argon2id 派生密钥 + AES-256-GCM 加密，数据只存本机）
- 📋 记录应用名 / 用户名 / 密码 / URL / 备注，关键字实时搜索
- 🎲 一键生成复杂密码（长度 8–64，字符集可选，可排除易混淆字符，加密安全随机数）
- 📎 一键复制；复制密码后剪贴板默认 30 秒自动清除（只清除未被覆盖的内容）
- ⏱ 无操作自动锁定（默认 15 分钟）；锁定/退出时内存中密钥与明文自动清零
- 💾 加密备份导出/导入（永不导出明文），换机迁移方便
- 每次保存自动保留上一版 `.bak`，损坏时可尝试恢复

## 下载安装

从 GitHub Releases 下载：

- **Windows**：`password-manager_x.y.z_x64-setup.exe`（NSIS 安装包）
- **Linux**：`AppImage`（`chmod +x` 后直接运行）或 `.deb`（`sudo apt install ./xxx.deb`）

## 从源码构建

前置：Rust stable（Windows 用 MSVC 工具链）、Node.js ≥ 20、
Linux 另需 `libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev build-essential`。

```bash
npm install
npm run tauri dev     # 开发调试
npm run tauri build   # 打包（Windows 产 NSIS；Linux 加 --bundles appimage,deb）
cargo test            # 全部单元测试
```

发布：push `v*` tag 触发 GitHub Actions 自动构建双平台产物到 Releases。

## 数据文件位置

- Windows: `%APPDATA%\com.pmgr.password-manager\vault.enc`
- Linux: `~/.config/com.pmgr.password-manager/vault.enc`

损坏时可尝试将同目录 `vault.enc.bak` 复制为 `vault.enc`。

## 安全说明

- 主密码不存储、无法找回——遗忘时只能用备份文件 + 记得的密码恢复
- 应用无任何网络请求与遥测
- 密码错误与文件损坏提示统一，不泄露细节

## 已知限制

- Linux Wayland 原生会话下剪贴板自动清除可能受限（X11/XWayland 正常）
- CLI / AI Agent 接口为预留扩展（核心逻辑在 `core/` crate，未来可复用），当前版本不提供

## 项目结构

- `core/`（pmgr-core）：数据模型、加密、密码生成——纯 Rust，无 GUI 依赖
- `src-tauri/`（pmgr-gui）：Tauri IPC 命令、剪贴板、应用状态
- `src/`：前端（Vite + 原生 TypeScript）
- 设计文档：`docs/superpowers/specs/2026-09-26-password-manager-design.md`
```

- [ ] **Step 2: 最终全量校验**

```powershell
cargo test
npm run build
```

Expected: 全部通过（core 27 + gui 1；tsc/vite 无错误）

- [ ] **Step 3: Commit**

```powershell
git add README.md
git commit -m "docs: README（安装/构建/安全说明）"
```

- [ ] **Step 4: 打 tag 交由用户 push（可选）**

```powershell
git tag v0.1.0
```

（push 到 GitHub 远端由用户执行，触发 CI 发布构建。）
