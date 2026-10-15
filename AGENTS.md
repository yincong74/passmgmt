# AGENTS.md

本地加密密码管理器（Windows / Linux / Android）：Tauri 2 + Rust workspace + 原生 TypeScript（Vite，无前端框架）。

## 项目结构

- `core/`（crate `pmgr-core`）：数据模型、Argon2id + AES-256-GCM 加密、密码生成。纯 Rust、无 GUI 依赖，单元测试主要在这里。
- `src-tauri/`（crate `pmgr-gui`）：`lib.rs` 是统一入口（`run()` 带 `#[cfg_attr(mobile, tauri::mobile_entry_point)]`，桌面 bin 与 Android JNI 共用；`main.rs` 只是薄壳）、Tauri IPC 命令（`commands.rs`）、剪贴板（`clipboard.rs`，cfg 分派：桌面 arboard / Android clipboard-manager 插件）、应用状态与自动锁定线程（`state.rs`）、生物识别解锁（`biometric.rs`，仅 mobile 编译）。
- `src/`：前端。所有 `invoke` 调用统一封装在 `src/api.ts`，桌面 UI 按屏幕拆分在 `src/ui/`；手机专属 UI 在 `src/ui/mobile/`（底部 Tab + history 页面栈），`main.ts` 启动时按 `platform()`（plugin-os）分流。
- 设计文档：`docs/superpowers/specs/2026-09-26-password-manager-design.md`；Android 版：`docs/superpowers/specs/2026-10-15-android-app-design.md`（下文简称 Android spec）。

## 常用命令

```bash
npm run tauri dev      # 开发调试（Vite :5173 + Rust）
npm run tauri build    # 打包；Linux 需加 --bundles appimage,deb
npm run tauri android dev                            # Android 真机/模拟器调试
npm run tauri android build -- --target aarch64 --apk  # Android arm64 APK
npm run build          # 仅前端：tsc --noEmit && vite build
cargo test             # 全部 Rust 测试
cargo test -p pmgr-core vault::tests::<测试名>   # 跑单个测试
```

- 没有 ESLint/Prettier/rustfmt 配置；TS 类型检查只靠 `tsc --noEmit`（strict），构建前会自动跑。
- 前端没有测试框架，全部测试在 Rust 侧——可测试的逻辑应下沉到 `core/`。
- 环境：Rust stable（Windows 用 MSVC 工具链）、Node ≥ 20；Linux 另需 webkit2gtk 等系统依赖（见 README）；android 命令需 Android SDK/NDK + JDK 17 + `rustup target add aarch64-linux-android` + cargo-ndk（Android spec §8.1），且首次需 `npm run tauri android init` 生成 `src-tauri/gen/android`。

## 新增 IPC 命令的流程（缺一不可）

1. `src-tauri/src/commands.rs`：`#[tauri::command]` 函数，返回 `Result<T, String>`——错误字符串是直接展示给用户的中文文案。
2. `src-tauri/src/lib.rs`：注册进 `generate_handler![]`。
3. `src/api.ts`：加对应 `invoke` 封装。
4. 参数命名：前端传 camelCase（如 `masterPassword`），Rust 参数是 snake_case（`master_password`），Tauri 自动转换，两边不要写成一致风格。
5. mobile 专属命令放 `#[cfg(mobile)]` 模块（见 `biometric.rs`），`generate_handler![]` 条目同样加 `#[cfg(mobile)]`；前端直接调用的插件 API（fs/biometric/keystore 等）要在 `src-tauri/capabilities/` 登记权限——Android 专属放 `mobile.json`（`platforms: ["android"]`），跨平台的（dialog/os）放 `default.json`。

Rust → 前端只有一个事件：`vault-locked`（自动锁定线程 emit，桌面 `src/main.ts` / 手机 `src/ui/mobile/boot.ts` 监听后回解锁屏）。自动锁定是双实现：Rust 线程兜底（每 30s）+ 前端定时器（每 15s，手机端另在 `visibilitychange` 回前台时立即补查一次）。Rust 侧超时判定是双钟：`Activity{monotonic, wall}`，`should_auto_lock_dual` 取单调钟与墙钟流逝的较大者（Android 深睡时单调钟停走，靠墙钟补漏锁；墙钟被回拨时单调钟兜底）。改锁定逻辑要同时看 `state.rs`、`main.ts` 和 `src/ui/mobile/boot.ts`。

## 安全约定（改代码必须遵守）

- 明文密码/密钥必须清零：`Vault` Drop 时自动 zeroize；命令里用完 `master_password` 要手动 `.zeroize()`。
- 解锁失败的错误必须统一笼统（“主密码错误，或数据文件已损坏”），不得区分密码错误与文件损坏。
- 剪贴板定时清除前必须比较内容，只清除未被用户覆盖的文本。「比较后清除」全平台只有 `clipboard.rs::clear_if_unchanged` 一份实现（桌面 arboard、Android clipboard-manager 插件的平台差异收敛在 `copy_text`/`read_current`），不得另写第二份。
- 生物识别解锁（仅 Android）：vault 密钥只存 Android Keystore（alias `pmgr_vault_key`）；`biometric.json` 只是启用标记，不得写入任何密钥材料。新建库、修改主密码成功后必须失效生物识别（`commands.rs::invalidate_biometric`），解锁时密钥与文件不匹配也要自动失效（`biometric.rs::vault_unlock_with_key`）。
- vault 密钥以 hex 字符串经 WebView JS 过桥是 Android spec §7.5 明示接受的约束，仅限生物识别注册/解锁两条路径，不得扩大使用范围；Rust 侧全程 `Zeroizing`/手动 `.zeroize()`。
- 锁定（含自动锁定与手机锁屏接管）时前端清空 `state.entries` 明文缓存（见 `src/ui/mobile/lock.ts::showMobileLock`），JS 侧不得残留条目明文；新增锁定路径必须照做。
- 应用零网络请求、零遥测，不得引入网络依赖。
- 用户可见文案（UI、错误信息）一律中文。

## 数据与发布

- 运行时数据文件：`app_data_dir/vault.enc`（Windows 在 `%APPDATA%\com.pmgr.password-manager\`；Android 自动映射到应用私有目录），每次保存自动留上一版 `.bak`。
- 发布：push `v*` tag 触发 `.github/workflows/release.yml`，三平台构建并创建 draft Release：Windows NSIS、Linux AppImage/deb、Android arm64 release APK（android job 用 tauri-action@v1 的 `mobile: android` 模式）；没有针对 PR 的 CI，提交前本地跑 `cargo test` 和 `npm run build`。
- Android 签名：持久自签 keystore，本地约定 `%USERPROFILE%\.android\pmgr-release.keystore`（经 `PMGR_*` 环境变量注入，不进 git），CI 从 GitHub Secrets（`ANDROID_KEYSTORE_B64` 等）解码；keystore 丢失则用户无法覆盖升级安装，务必备份。
- 版本号需同步改：`package.json`、`src-tauri/tauri.conf.json`（Release 名里的 `__VERSION__` 取自 tauri.conf.json）、两个 crate 的 `Cargo.toml`；`gen/android` 的 versionName/versionCode 由 tauri.conf.json 自动派生，无需手动改。
- 应用图标：改 `scripts/make-icon.cjs` 重新生成 `app-icon.png`，再用 `npm run tauri icon` 生成全套图标。

## 构建产物

- 勿手改（已 gitignore）：`dist/`、`target/`、`src-tauri/gen/schemas/`（Tauri 生成的 capability schema）。
- `src-tauri/gen/android` 例外：它是 `tauri android init` 生成的 Android 工程，**提交进仓库**，生成后归项目所有（可改签名配置与 Manifest）。当前分支尚未生成，待 Android SDK 环境就绪后补（Android spec §8.1）。
