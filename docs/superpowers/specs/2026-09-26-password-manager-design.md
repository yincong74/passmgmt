# 密码管理器（Password Manager）设计文档

- 日期：2026-09-26
- 状态：已与用户逐节确认
- 技术路线：Tauri 2.x（Rust 后端 + WebView 前端）

## 1. 背景与目标

为个人用户（单用户）提供一个跨平台（Windows / Linux）的桌面密码管理工具：

- 记录各应用程序的账号信息（应用名、用户名、密码、URL、备注）
- 自动生成复杂的随机密码
- 一键复制用户名/密码到剪贴板，并在延时后自动清除
- 数据以主密码加密后存储在本地，支持导出/导入加密备份
- 打包为独立可执行文件，最终用户无需安装任何运行时

## 2. 非目标（YAGNI）

- 云同步、多设备自动同步、服务器端
- 多用户 / 共享凭据
- 浏览器扩展、自动填充
- TOTP/2FA 码生成
- 分组 / 标签（用户已明确不需要）
- 移动端

## 3. 技术选型

| 层 | 选择 | 理由 |
|---|---|---|
| 桌面框架 | Tauri 2.x | 用户选定；体积小、内存占用低、Rust 安全性 |
| 后端语言 | Rust (stable, MSVC 工具链) | Tauri 原生；加密生态成熟 |
| 前端 | Vite + 原生 TypeScript（无框架） | 应用规模小，避免框架开销 |
| 加密 | `argon2` + `aes-gcm` crate | Argon2id 密钥派生 + AES-256-GCM 认证加密 |
| 随机数 | `rand` crate（OsRng，CSPRNG） | 密码生成必须用系统级加密安全随机数 |
| 剪贴板 | `arboard` crate | Rust 侧完全控制（含定时清除） |
| 文件对话框 | `tauri-plugin-dialog` | 导出/导入选择路径 |
| 内存清理 | `zeroize` crate | 锁定/退出时清零密钥与明文 |
| 其他 | `serde`/`serde_json`、`uuid` | 序列化与条目 ID |
| CI 打包 | GitHub Actions + `tauri-apps/tauri-action` | 双平台自动构建发布 |

本机环境已确认：VS 2022 Community（含 C++ 工具链）、WebView2 运行时、Node 24 + npm。
Rust 未安装，实现阶段第一步用 rustup 安装（一次性前置工作）。

## 4. 架构

```
┌────────────────────────── Tauri 应用 ──────────────────────────┐
│  前端 (WebView)                                                │
│  解锁屏 / 主界面 / 设置对话框（Vite + TS，仅展示与交互）        │
│        │ Tauri IPC（invoke command）                           │
│  Rust 后端                                                     │
│  commands.rs  — IPC 命令入口、鉴权（未解锁一律拒绝）            │
│  vault.rs     — 加密/解密、数据模型、原子保存、.bak             │
│  generator.rs — 密码生成器                                     │
│  clipboard.rs — 复制 + 定时清除线程                            │
│  state.rs     — 应用状态（内存中的 vault、密钥、设置、计时器）   │
└────────────────────────────────────────────────────────────────┘
         │
         ▼
  vault.enc（应用数据目录）        GitHub Actions（发布构建）
  Windows: %APPDATA%\password-manager\
  Linux:   ~/.config/password-manager/
```

原则：

- **主密码与派生密钥永远不离开 Rust 侧**，前端只通过 IPC 收发明文条目数据（解锁状态下展示所必需）。
- 核心逻辑（vault、generator）不依赖 Tauri API，可独立单元测试。
- 前端逻辑保持极薄：状态渲染、过滤、事件转发。

## 5. 数据模型与文件格式

### 5.1 条目（Entry）

```json
{
  "id": "uuid-v4",
  "app_name": "应用名（必填）",
  "username": "用户名",
  "password": "密码",
  "url": "网址（可选）",
  "notes": "备注（可选）",
  "created_at": 1758844800,
  "updated_at": 1758844800
}
```

时间戳为 Unix 秒（u64），显示格式化由前端完成。

### 5.2 设置（Settings）

```json
{
  "auto_lock_minutes": 15,
  "clipboard_clear_seconds": 30
}
```

两者均支持 `0` 表示关闭。默认：自动锁定 15 分钟、剪贴板清除 30 秒。

### 5.3 vault.enc 文件布局

```
偏移  内容
0     magic: "PMGR"（4 字节）
4     版本: u8（当前 = 1）
5     Argon2id salt（16 字节，随机）
21    Argon2id 参数: m_cost(u32, KiB=65536), t_cost(u32=3), p_cost(u32=1)
33    AES-256-GCM nonce（12 字节，每次保存随机生成）
45    密文 + GCM 认证标签（16 字节 tag 附在密文末尾）
```

密文明文为 JSON：`{ "entries": [...], "settings": {...} }`。

- **密钥派生**：`Argon2id(master_password, salt, m=64MiB, t=3, p=1) → 32 字节密钥`。
- **主密码校验**：不存独立哈希；解密时 GCM 认证标签校验失败即"主密码错误或文件损坏"。
- **保存流程**：写临时文件 → 将现有 vault.enc 复制为 vault.enc.bak → 原子重命名临时文件为 vault.enc。
- **修改主密码**：用旧密码派生密钥解密全部数据 → 生成新 salt → 用新密码派生密钥重加密保存。

## 6. 安全设计

1. **密码生成器**：OsRng；长度 8–64（默认 20）；字符集开关：小写、大写、数字、符号（默认全选）；可选排除易混淆字符 `0O1lI|`（默认排除）；采用拒绝采样保证每个选中字符集至少出现一个字符。
2. **剪贴板自动清除**：复制密码类内容后启动 Rust 侧定时线程，到期后若剪贴板内容仍等于所复制内容则清空（避免误清用户后续复制的其他内容）；界面显示"已复制，N 秒后自动清除"提示。
3. **自动锁定**：前端记录用户活动（键鼠事件），超过 `auto_lock_minutes` 无活动调用 `vault_lock`；Rust 侧同时以最后一次 IPC 调用时间做兜底。锁定与退出时 `zeroize` 清零内存中密钥与条目明文。
4. **导出/导入备份**：
   - 导出 = 将当前加密 vault 文件原样复制一份到用户选择的路径（永不导出明文）。
   - 导入 = 用户提供备份路径 + 该备份的主密码；Rust 侧验证解密成功后，用**当前**主密码重新加密并替换现有 vault（替换前现有文件留 .bak）；导入需界面二次确认（提示将覆盖当前数据）。
5. **锁定状态鉴权**：所有条目/设置读写命令在 Rust 侧检查解锁状态，未解锁一律返回错误。
6. **无遥测、无网络请求**：应用不联网（CI 构建除外）。
7. 错误提示不泄露细节（错误密码与文件损坏返回同一类笼统消息）。

## 7. 界面设计

### 7.1 解锁屏

- vault 不存在（首次启动）：显示"创建主密码"表单——主密码 + 确认输入 + 简单强度提示（长度/字符种类），两次不一致或过短（< 8 位）拒绝。
- vault 已存在：主密码输入 + 解锁按钮；失败显示笼统错误。
- "忘记密码？"帮助文字：无法找回；若有备份文件且记得密码，可在重置后导入。

### 7.2 主界面

- **顶部工具栏**：搜索框（实时过滤）、"新增条目"按钮、"锁定"按钮、"设置"按钮。
- **左侧列表**：每行显示应用名（主）+ 用户名（次）；点击选中；搜索匹配应用名/用户名/URL/备注，不区分大小写，由前端对已加载条目过滤。
- **右侧详情面板**：逐字段展示；密码默认打码（••••），带显示/隐藏切换；每字段旁复制按钮；"编辑"、"删除"按钮（删除需确认对话框）。
- **条目编辑对话框**：应用名（必填，空则拒绝保存）、用户名、密码（旁有"生成"按钮打开生成器面板：长度滑块 + 字符集勾选 + 排除混淆字符 + 重新生成/使用）、URL、备注（多行）。

### 7.3 设置对话框

- 自动锁定超时（分钟，0=关闭）
- 剪贴板清除时间（秒，0=关闭）
- 修改主密码（旧密码 + 新密码 + 确认）
- 导出备份（文件对话框选保存路径）/ 导入备份（选文件 + 输入备份主密码 + 二次确认）
- 所有设置修改即时保存并提示成功。

## 8. IPC 命令清单

| 命令 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `vault_status` | — | `{ exists, unlocked }` | 前端启动时决定显示哪个屏 |
| `vault_create` | `master_password` | `ok / err` | 首次创建 |
| `vault_unlock` | `master_password` | `ok / err` | 解密载入内存 |
| `vault_lock` | — | `ok` | 清零内存 |
| `entries_list` | — | `[Entry]` | 需解锁 |
| `entry_save` | `Entry`（无 id 则新建） | `Entry` | 新增/更新并落盘 |
| `entry_delete` | `id` | `ok` | 删除并落盘 |
| `settings_get` | — | `Settings` | 需解锁 |
| `settings_save` | `Settings` | `ok` | 保存并落盘 |
| `password_generate` | `GenOptions` | `string` | 无需解锁也可用（生成器独立） |
| `clipboard_copy` | `text, secret: bool` | `ok` | secret=true 时启动定时清除 |
| `backup_export` | `dest_path` | `ok / err` | 需解锁；复制加密文件 |
| `backup_import` | `src_path, backup_password` | `ok / err` | 验证后重加密替换 |
| `master_password_change` | `old, new` | `ok / err` | 需解锁 |

## 9. 打包与分发

- **Windows**（本机）：`tauri build` → NSIS 安装包 `.exe`；发布附便携运行说明。
- **Linux**：GitHub Actions（`tauri-action`，ubuntu-22.04）→ **AppImage + .deb**。
- **发布流程**：push git tag（如 `v0.1.0`）触发 workflow，双平台产物自动上传到 GitHub Releases。
- 已知限制（如实告知）：Wayland 原生会话下 `arboard` 剪贴板可能受限；X11/XWayland 正常。文档中注明。
- 前置步骤：rustup 安装 Rust stable MSVC（本机 VS C++ 组件已就绪）。

## 10. 错误处理

| 场景 | 行为 |
|---|---|
| 主密码错误 / 文件损坏 | 统一笼统错误提示，不区分细节 |
| vault.enc 损坏 | 报错并提示存在 vault.enc.bak 可尝试恢复（提供"用备份替换"操作指引） |
| 导入备份验证失败 | 拒绝导入并提示，当前数据不受影响 |
| 磁盘写入失败（满/权限） | 界面明确报错，绝不静默 |
| IPC 未解锁调用受保护命令 | 返回 `Locked` 错误，前端跳回解锁屏 |
| 应用名留空保存 | 前端拒绝 + 后端二次校验 |

## 11. 测试策略

**Rust 单元测试（`cargo test`，核心逻辑与 Tauri 解耦）：**

- vault：创建→解锁→增删改条目→保存→重新解锁数据一致（往返）
- 错误主密码解锁必须失败；篡改密文一个字节必须失败（GCM 认证）
- 保存后 .bak 存在且内容为上一版
- 修改主密码往返：旧密码失效、新密码可解锁、数据完整
- 导入备份：验证成功重加密、验证失败拒绝
- generator：长度精确；仅含选中字符集；排除混淆字符生效；每个选中字符集至少一个；两次生成结果不同
- 设置默认值与 0 值语义

**手动冒烟清单（Windows 本机执行）：**
创建主密码 → 新增/编辑/删除条目 → 搜索过滤 → 生成密码 → 复制并等待自动清除 → 锁定/解锁 → 自动锁定超时 → 修改主密码 → 导出备份 → 清空数据后导入恢复 → `tauri build` 产物可安装运行。

**Linux 侧**：CI 构建通过即验证可编译打包；AppImage 在目标机手动冒烟（由用户在 Linux 机器执行，清单同上）。

## 12. 仓库结构

```
password/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs        # Tauri 入口、命令注册
│   │   ├── commands.rs    # IPC 命令
│   │   ├── vault.rs       # 加密、数据模型、原子保存
│   │   ├── generator.rs   # 密码生成
│   │   ├── clipboard.rs   # 复制与定时清除
│   │   └── state.rs       # 应用状态管理
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                   # 前端（Vite + TS）
│   ├── index.html
│   ├── main.ts
│   ├── api.ts             # IPC 封装
│   ├── ui/                # 各屏幕与对话框渲染逻辑
│   └── styles.css
├── package.json
├── .github/workflows/release.yml
└── docs/superpowers/specs/
```
