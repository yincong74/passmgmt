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

暂未发布安装包，请从源码构建（见下节）。

发布 `v*` tag 后，GitHub Actions 会自动构建并上传到 GitHub Releases：

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
