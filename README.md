# 密码管理器（Password Manager）

跨平台（Windows / Linux / Android）本地加密密码管理器。Tauri 2 构建：体积小、内存占用低、无需安装任何运行时。

## 功能

- 🔐 主密码解锁（Argon2id 派生密钥 + AES-256-GCM 加密，数据只存本机）
- 📋 记录应用名 / 用户名 / 密码 / URL / 备注，关键字实时搜索
- 🎲 一键生成复杂密码（长度 8–64，字符集可选，可排除易混淆字符，加密安全随机数）
- 📎 一键复制；复制密码后剪贴板默认 30 秒自动清除（只清除未被覆盖的内容）
- ⏱ 无操作自动锁定（默认 15 分钟）；锁定/退出时内存中密钥与明文自动清零
- 💾 加密备份导出/导入（永不导出明文），换机迁移方便
- 📱 Android 版：功能与桌面对等，可选指纹/人脸解锁（主密码始终兜底）
- 每次保存自动保留上一版 `.bak`，损坏时可尝试恢复

## 下载安装

暂未发布安装包，请从源码构建（见下节）。

发布 `v*` tag 后，GitHub Actions 会自动构建并上传到 GitHub Releases：

- **Windows**：`password-manager_x.y.z_x64-setup.exe`（NSIS 安装包）
- **Linux**：`AppImage`（`chmod +x` 后直接运行）或 `.deb`（`sudo apt install ./xxx.deb`）
- **Android**：arm64 APK（侧载安装，见下文「Android 版」）

## 从源码构建

前置：Rust stable（Windows 用 MSVC 工具链）、Node.js ≥ 20、
Linux 另需 `libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev build-essential`。

```bash
npm install
npm run tauri dev     # 开发调试
npm run tauri build   # 打包（Windows 产 NSIS；Linux 加 --bundles appimage,deb）
cargo test            # 全部单元测试
```

发布：push `v*` tag 触发 GitHub Actions 自动构建三平台产物（Windows / Linux / Android APK）到 Releases。

## Android 版

系统要求：Android 9（API 28）及以上，arm64 设备。

**安装**：发布后从 GitHub Releases 页下载 arm64 APK（arm64：近年绝大多数 Android 手机）侧载安装（需允许「安装未知来源应用」）。覆盖升级须安装同一签名的版本；签名不同需先卸载重装，卸载前请先导出加密备份。

**数据迁移（桌面 ↔ 手机）**：不涉及任何网络，靠手动传输加密备份文件——

1. 桌面端「设置 → 备份 → 导出加密备份」，得到 `.enc` 文件
2. 用任意方式（USB / 网盘 / 聊天工具）把文件传到手机
3. 手机「设置 → 备份 → 导入备份」，选中文件并输入**备份文件的主密码**（全新手机首次打开需先随便创建一个主密码进入应用，再执行导入；导入会覆盖该临时库）

**生物识别解锁（可选）**：「设置 → 生物识别解锁」中开启（需设备已录入指纹/人脸）。解锁密钥存于 Android Keystore（硬件背书），主密码永远是兜底；修改主密码后生物识别自动失效，需重新开启。

**从源码构建**：需 Android Studio（含 Android SDK、NDK）、JDK 17，另装 Rust 目标与构建工具：

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
npm run tauri android init                            # 首次：生成 Android 工程
npm run tauri android build -- --target aarch64 --apk # 打包 APK
npm run tauri android dev                             # 真机/模拟器调试
```

## 数据文件位置

- Windows: `%APPDATA%\com.pmgr.password-manager\vault.enc`
- Linux: `~/.config/com.pmgr.password-manager/vault.enc`
- Android: 应用私有目录（系统沙箱管理），迁移请用应用内「设置 → 备份」导出/导入

损坏时可尝试将同目录 `vault.enc.bak` 复制为 `vault.enc`。

## 安全说明

- 主密码不存储、无法找回——遗忘时只能用备份文件 + 记得的密码恢复
- 应用无任何网络请求与遥测
- 密码错误与文件损坏提示统一，不泄露细节

## 已知限制

- Linux Wayland 原生会话下剪贴板自动清除可能受限（X11/XWayland 正常）
- Android 13+ 复制时系统会弹「已复制」提示；应用在后台被系统冻结时，剪贴板定时清除会延迟到回到前台执行
- CLI / AI Agent 接口为预留扩展（核心逻辑在 `core/` crate，未来可复用），当前版本不提供

## 项目结构

- `core/`（pmgr-core）：数据模型、加密、密码生成——纯 Rust，无 GUI 依赖
- `src-tauri/`（pmgr-gui）：Tauri IPC 命令、剪贴板、应用状态、生物识别（仅 mobile）——`lib.rs` 统一入口，桌面与 Android 共用
- `src/`：前端（Vite + 原生 TypeScript）；`src/ui/mobile/` 为手机专属 UI
- 设计文档：`docs/superpowers/specs/2026-09-26-password-manager-design.md`、`docs/superpowers/specs/2026-10-15-android-app-design.md`（Android 版）
