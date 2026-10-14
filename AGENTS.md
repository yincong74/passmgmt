# AGENTS.md

本地加密密码管理器：Tauri 2 + Rust workspace + 原生 TypeScript（Vite，无前端框架）。

## 项目结构

- `core/`（crate `pmgr-core`）：数据模型、Argon2id + AES-256-GCM 加密、密码生成。纯 Rust、无 GUI 依赖，单元测试主要在这里。
- `src-tauri/`（crate `pmgr-gui`）：Tauri IPC 命令（`commands.rs`）、剪贴板（`clipboard.rs`）、应用状态与自动锁定线程（`state.rs`）。
- `src/`：前端。所有 `invoke` 调用统一封装在 `src/api.ts`，UI 按屏幕拆分在 `src/ui/`。
- 设计文档：`docs/superpowers/specs/2026-09-26-password-manager-design.md`。

## 常用命令

```bash
npm run tauri dev      # 开发调试（Vite :5173 + Rust）
npm run tauri build    # 打包；Linux 需加 --bundles appimage,deb
npm run build          # 仅前端：tsc --noEmit && vite build
cargo test             # 全部 Rust 测试
cargo test -p pmgr-core vault::tests::<测试名>   # 跑单个测试
```

- 没有 ESLint/Prettier/rustfmt 配置；TS 类型检查只靠 `tsc --noEmit`（strict），构建前会自动跑。
- 前端没有测试框架，全部测试在 Rust 侧——可测试的逻辑应下沉到 `core/`。
- 环境：Rust stable（Windows 用 MSVC 工具链）、Node ≥ 20；Linux 另需 webkit2gtk 等系统依赖（见 README）。

## 新增 IPC 命令的流程（缺一不可）

1. `src-tauri/src/commands.rs`：`#[tauri::command]` 函数，返回 `Result<T, String>`——错误字符串是直接展示给用户的中文文案。
2. `src-tauri/src/main.rs`：注册进 `generate_handler![]`。
3. `src/api.ts`：加对应 `invoke` 封装。
4. 参数命名：前端传 camelCase（如 `masterPassword`），Rust 参数是 snake_case（`master_password`），Tauri 自动转换，两边不要写成一致风格。

Rust → 前端只有一个事件：`vault-locked`（自动锁定线程 emit，`src/main.ts` 监听后回解锁屏）。自动锁定是双实现：Rust 线程兜底（每 30s）+ 前端定时器（每 15s），改锁定逻辑要同时看 `state.rs` 和 `main.ts`。

## 安全约定（改代码必须遵守）

- 明文密码/密钥必须清零：`Vault` Drop 时自动 zeroize；命令里用完 `master_password` 要手动 `.zeroize()`。
- 解锁失败的错误必须统一笼统（“主密码错误，或数据文件已损坏”），不得区分密码错误与文件损坏。
- 剪贴板定时清除前必须比较内容，只清除未被用户覆盖的文本（见 `clipboard.rs`）。
- 应用零网络请求、零遥测，不得引入网络依赖。
- 用户可见文案（UI、错误信息）一律中文。

## 数据与发布

- 运行时数据文件：`app_data_dir/vault.enc`（Windows 在 `%APPDATA%\com.pmgr.password-manager\`），每次保存自动留上一版 `.bak`。
- 发布：push `v*` tag 触发 `.github/workflows/release.yml`，双平台构建并创建 draft Release；没有针对 PR 的 CI，提交前本地跑 `cargo test` 和 `npm run build`。
- 版本号需同步改：`package.json`、`src-tauri/tauri.conf.json`（Release 名里的 `__VERSION__` 取自 tauri.conf.json）、两个 crate 的 `Cargo.toml`。
- 应用图标：改 `scripts/make-icon.cjs` 重新生成 `app-icon.png`，再用 `npm run tauri icon` 生成全套图标。

## 构建产物（勿手改，均已 gitignore）

`dist/`、`target/`、`src-tauri/gen/`（Tauri 生成的 schema）。
