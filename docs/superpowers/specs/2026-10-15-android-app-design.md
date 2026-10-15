# 密码管理器 Android 版设计文档

- 日期：2026-10-15
- 状态：已与用户逐节确认
- 基础：在《2026-09-26 密码管理器设计》之上扩展；原设计 §2「非目标：移动端」由本文档解除
- 技术路线：Tauri 2 mobile（官方 Android 支持），单工程 + 平台条件编译

## 1. 背景与目标

为现有桌面密码管理器（Windows / Linux）增加 Android 手机版本：

- 功能与桌面**对等**：保险库增删改查、密码生成、剪贴板复制 + 定时清除、自动锁定、加密备份导出/导入、修改主密码
- 增加**可选的生物识别解锁**（指纹/人脸），安全定位为「便捷解锁」：主密码永远是兜底
- 桌面与手机之间通过**手动传输加密备份文件**交换数据（USB / 网盘 / 聊天工具），维持零网络约定
- 分发方式：**APK 侧载自装**（GitHub Release 附件），不上架 Google Play
- 桌面端行为与外观**完全不变**

## 2. 非目标（YAGNI）

- Google Play 上架（AAB、商店元数据、隐私政策页等）
- Android 系统自动填充（Autofill Service / 无障碍服务）
- 任何形式的云同步、局域网同步（零网络约定不变）
- iOS 版本（架构上不排斥，但本期不做、不验证）
- 平板/折叠屏专属布局（手机竖屏优先）

## 3. 技术选型

| 层 | 选择 | 理由 |
|---|---|---|
| 移动框架 | Tauri 2 mobile | 与桌面同工程同后端，复用 `pmgr-core` 与全部 IPC 命令 |
| Android 工程 | `tauri android init` 生成 `src-tauri/gen/android`，**提交进 git** | 生成后归项目所有，可改签名配置与 Manifest |
| 剪贴板（Android） | `tauri-plugin-clipboard-manager` | `arboard` 不支持 Android；插件提供 Rust 侧 `ClipboardExt` API，桌面继续用 arboard |
| 生物识别提示 | 官方 `@tauri-apps/plugin-biometric` 的 `checkStatus()` | 检测设备是否录入指纹/人脸，决定设置项可用性 |
| 密钥托管 | `@impierce/tauri-plugin-keystore` | Android Keystore 硬件背书；存/取都强制生物识别验证；要求 Android 9+（API 28） |
| Android 文件读写 | 官方 `@tauri-apps/plugin-fs` | dialog 在 Android 返回 `content://` URI，`std::fs` 读不了；fs 插件原生支持 content URI |
| 平台检测 | `@tauri-apps/plugin-os` 的 `platform()` | 前端启动时分流桌面/手机 UI |
| 签名 | 持久自签 keystore | 侧载升级必须同签名才能覆盖安装；keystore 不进 git |
| CI | `release.yml` 增加 android job（ubuntu-latest） | runner 预装 SDK/NDK；产物 APK 附到 draft Release |
| 最低系统 | minSdk 28（Android 9） | keystore 插件要求；2026 年覆盖率 >97% |
| ABI | 仅 `arm64-v8a` | 覆盖所有近代手机，APK 体积最小 |

## 4. 架构

```
┌──────────────────── Tauri 应用（同一工程） ────────────────────┐
│  前端 (WebView)                                                │
│  main.ts ──platform()──┬─ desktop → src/ui/*（现有，零改动）    │
│                        └─ android → src/ui/mobile/*（新增）     │
│  两端共享：api.ts、state.ts、样式变量                           │
│        │ Tauri IPC                                             │
│  Rust 后端（src-tauri，lib.rs 统一入口）                        │
│  ┌ pmgr-core（不变 + 两个新方法）─────────────────────────────┐ │
│  │ vault.rs / generator.rs（加密、数据模型、密码生成）          │ │
│  └────────────────────────────────────────────────────────────┘ │
│  commands.rs — 全平台共享；#[cfg(mobile)] 追加移动专属命令       │
│  clipboard.rs — cfg 分派：desktop=arboard / mobile=插件         │
│  biometric.rs — 仅 mobile 编译                                 │
│  state.rs     — 自动锁定（墙钟修复，见 §6.3）                   │
└────────────────────────────────────────────────────────────────┘
         │                                │
         ▼                                ▼
  桌面: app_data_dir/vault.enc      Android: 应用私有目录/vault.enc
  （Windows/Linux，行为不变）        （app_data_dir 自动映射，行为一致）
```

### 4.1 仓库结构变更

- `src-tauri/src/main.rs` → 薄壳，只调 `pmgr_gui::run()`；Builder 逻辑迁入新 `src-tauri/src/lib.rs`，`run()` 加 `#[cfg_attr(mobile, tauri::mobile_entry_point)]`
- `src-tauri/Cargo.toml` 增加 `[lib] crate-type = ["staticlib", "cdylib", "rlib"]`
- `.gitignore`：现在忽略整个 `src-tauri/gen/`，改为只忽略 `src-tauri/gen/schemas`，放行并提交 `src-tauri/gen/android`
- `identifier` 保持 `com.pmgr.password-manager`：Tauri CLI（≥2.0-rc.7）自动转换为 Android 包名 `com.pmgr.password_manager`；桌面数据目录名不受影响
- `capabilities/`：为 mobile 增加 clipboard-manager、biometric、keystore、fs 所需权限
- 前端目录：`src/ui/` 现有 6 个文件原样保留（桌面）；新增 `src/ui/mobile/`（独立 TS + mobile.css）

### 4.2 平台分流

`main.ts` 启动时 `platform()`：返回 `android` 挂载手机 UI，其余挂载现有桌面 UI。桌面 UI 代码路径不被手机逻辑污染；`api.ts` 仅追加移动专属命令封装。

## 5. 手机专属 UI（src/ui/mobile/）

导航结构：**底部 Tab 栏（条目 / 生成 / 设置）+ 层级页面导航**。

- **解锁屏**（全屏页）：手机样式大输入框；vault 不存在时走「创建保险库」流程；已开启生物识别时进入即自动弹指纹，失败/取消回落主密码输入
- **Tab 1 条目**：顶部搜索框 + 卡片列表（应用名/用户名）；点条目 → 全屏详情页（顶部返回箭头，支持系统返回手势）；详情页字段打码/显示切换、逐字段复制、编辑、删除（确认）；右下角悬浮「+」新建 → 全屏编辑页（含内嵌密码生成器）
- **Tab 2 生成**：独立密码生成器页——长度滑块、字符集开关、排除易混淆、重新生成、一键复制（secret=true 走定时清除）
- **Tab 3 设置**：自动锁定分钟数、剪贴板清除秒数、**生物识别解锁开关**（设备无生物识别时禁用并提示）、备份导出/导入、修改主密码、立即锁定
- 视觉：复用现有暗色主题 CSS 变量；触控目标 ≥44px；适配刘海屏 safe-area；对话框改全屏 sheet 风格；文案一律中文（同 AGENTS.md 约定）

## 6. 平台能力适配

### 6.1 剪贴板

- 桌面：`arboard` 路径**不变**
- Android：`tauri-plugin-clipboard-manager` 的 Rust 侧 `ClipboardExt`（`write_text`/`read_text`）
- `clipboard_copy` 命令与 `schedule_clear` 的「到期先比较内容、只清除未被覆盖的文本」安全逻辑两端一致
- 已知限制（接受并文档化）：Android 13+ 系统会弹「已复制」提示；进程被后台冻结时定时清除延迟到回前台执行

### 6.2 备份导出/导入（content URI 桥接）

- 桌面：现有 `backup_export(dest_path)` / `backup_import(src_path, password)` 不变
- Android：dialog `save`/`open` 返回 `content://` URI → 前端用 `@tauri-apps/plugin-fs` 读写 URI，Rust 侧新增两个命令：
  - `backup_export_data() -> Vec<u8>`：需解锁；返回 vault.enc 原始字节（**密文**出 Rust，永不导出明文）
  - `backup_import_data(bytes, backup_password)`：需解锁；core 增加字节切片入口的薄封装（`decrypt_backup_bytes`，复用现有验证解密逻辑），再用当前密钥重加密替换（`.bak` 行为不变）
- 导入前二次确认、导入后 reload 的流程与桌面一致

### 6.3 自动锁定（墙钟修复）

- 问题：Android 深睡时单调钟（`Instant`）不走，纯单调钟会漏锁
- 方案：`AppState::touch()` 同时记录 `Instant` 与 `SystemTime`；超时判定取「单调钟 elapsed」与「墙钟差值」中的**较大者**；判定逻辑做成纯函数并配单元测试
- 策略与桌面一致：空闲 ≥ `auto_lock_minutes` 锁定，0 = 关闭；Rust 30s 兜底线程 + 前端 15s 定时器的双实现结构保留，前端在 `visibilitychange` 回前台时立即检查一次

## 7. 生物识别解锁（仅 Android）

### 7.1 core 扩展（生物识别涉及的唯一加密层改动，带单元测试）

- `Vault::open_with_key(path, key: &[u8; KEY_LEN]) -> Result<Vault, VaultError>`：用裸密钥直接解密（跳过 Argon2）；GCM 校验失败统一返回 `WrongPasswordOrCorrupt`（不泄露失败原因）
- `Vault::export_key() -> Zeroizing<[u8; KEY_LEN]>`：仅已解锁实例可调用，供注册流程使用；`Zeroizing` 保证用完清零

### 7.2 密钥托管

- vault 的 32 字节 AES 密钥经 `tauri-plugin-keystore` 存入 Android Keystore（alias `pmgr_vault_key`）：**硬件背书、磁盘无明文**，存与取都强制弹生物识别
- app_data_dir 写标记文件 `biometric.json`（内容仅 `{"enabled": true}`，不含任何密钥材料），供解锁屏判断是否走生物识别

### 7.3 流程

- **开启**（设置 → 生物识别解锁）：需已解锁 → `checkStatus()` 确认设备可用 → `export_key()` → `keystore.store()`（弹指纹确认）→ 写标记文件 → 密钥字节立即清零
- **解锁**（解锁屏自动触发）：标记存在 → `keystore.retrieve()`（弹指纹）→ `open_with_key()` → 成功进主界面；失败/取消 → 回落主密码输入框
- **关闭**：`keystore.remove()` + 删标记文件

### 7.4 失效规则（防密钥分叉）

- 修改主密码成功后：vault 密钥已变 → 自动失效（remove + 删标记），需重新开启
- 生物识别解锁时 `open_with_key` 失败（密钥与文件不匹配，如数据被重建）→ 自动失效并提示改用主密码
- 导入备份不改变当前密钥，注册保持有效

### 7.5 安全边界

- 威胁模型变化（明示接受）：开启后「持有设备 + 通过生物识别」= 完全访问保险库，与 Bitwarden/1Password 的生物解锁等价；不开启则与桌面完全一致
- 主密码不落盘、不进 Keystore；Keystore 中只有硬件加密后的 vault 密钥
- 生物识别是**可选便捷功能**：App 重启后首次解锁、连续失败、修改主密码后均要求主密码
- 零网络、零遥测约定不变

## 8. 构建、签名与发布

### 8.1 本机环境（从零安装，一次性）

1. Android Studio（含 Android SDK、NDK side-by-side、自带 JDK 17）
2. `rustup target add aarch64-linux-android`
3. 环境变量：`ANDROID_HOME`、`NDK_HOME`、`JAVA_HOME`
4. 首次 `npm run tauri android init` 生成 Android 工程

### 8.2 构建与签名

- 构建：`npm run tauri android build -- --target aarch64 --apk`
- 签名：`keytool` 生成持久 keystore（口令与路径经环境变量/本地 properties 注入，**不进 git**）；`gen/android/app/build.gradle.kts` 配置 release signingConfig
- 开发调试：`npm run tauri android dev`（真机 USB 或模拟器）

### 8.3 CI（release.yml 增加 android job）

- ubuntu-latest（预装 SDK/NDK）+ JDK 17 + rustup android target
- 签名材料存 GitHub Secrets（keystore 文件 base64 + 口令）
- 产物：`password-manager_<version>_arm64.apk` 附到 draft Release；`v*` tag 触发方式不变
- 版本号同步清单在原有四处（package.json / tauri.conf.json / 两个 Cargo.toml）基础上新增：`gen/android` 的 versionName/versionCode 随 tauri.conf.json 自动派生，无需手动改

## 9. 测试策略

- **core（cargo test）**：`open_with_key` 正确密钥成功/错误密钥失败（统一错误）、`export_key` 与 `open_with_key` 往返、现有全部用例回归
- **src-tauri（cargo test）**：墙钟+单调钟取大者的纯函数用例（含「墙钟超时但单调钟未超时」场景）、现有 `should_auto_lock` 用例保留
- **前端**：`tsc --noEmit`（strict）；桌面 UI 手动冒烟（创建/解锁/CRUD/复制/备份），因代码未动风险低
- **Android 真机手测清单**：创建库 → 指纹注册 → 杀进程重启指纹解锁 → 修改主密码后指纹自动失效 → SAF 导出备份到 Download → 卸载重装 → 导回备份 → 剪贴板定时清除 → 退后台超时锁定 → 生成器复制
- **构建验证**：本地 `tauri android build` 出 APK + CI android job 绿

## 10. 对 AGENTS.md 的影响（实施完成后同步更新）

- 「新增 IPC 命令的流程」补充第 5 步：mobile 专属命令需在 `capabilities/` 加权限，并用 `#[cfg(mobile)]` 隔离注册
- 自动锁定「双实现」说明补充墙钟修复
- 剪贴板安全约定补充：Android 走 clipboard-manager 插件，但「比较后清除」逻辑不变
- 数据与发布：补充 Android 产物、签名 keystore 管理、版本号派生说明
