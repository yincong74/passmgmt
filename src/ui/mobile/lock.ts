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
  if (!keyHex) {
    // 标记存在但 Keystore 条目已丢失（数据恢复/系统清理）→ 清理孤儿标记（review M2）
    await api.biometricSetEnabled(false).catch(() => {});
    return false;
  }
  try {
    await api.vaultUnlockWithKey(keyHex);
    return true;
  } catch {
    // 密钥与文件不匹配（改密/数据重建）→ 自动失效并清理（Rust 侧已删标记）
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

/** 同步幂等标志：防 vault-locked 事件与前端定时检查并发触发双重渲染/双弹指纹（review M4）。 */
let onLock = false;

/** 显示解锁屏（幂等）。进入锁定态时清空前端明文缓存（review I1，AGENTS.md 清零约定）。 */
export async function showMobileLock(): Promise<void> {
  if (onLock) return;
  onLock = true;
  resetStack();
  // 纵深防御：Rust 侧锁定时已 zeroize，JS 侧不得残留条目明文
  state.entries = [];
  state.selectedId = null;
  state.query = "";
  state.showPassword = false;
  // 纵深防御（review M2）：先擦除上一页可能残留的明文 DOM，再等 IPC
  const el = document.getElementById("m-pages")!;
  el.innerHTML = "";
  const status = await api.vaultStatus().catch(() => ({ exists: true, unlocked: false }));
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
      } catch {
        // 安全要求：统一笼统文案，不区分密码错误与文件损坏
        showErr("主密码错误，或数据文件已损坏");
        return;
      }
      try {
        await enterMobileMain();
      } catch {
        showErr("数据加载失败，请重试");
      }
    };
    document.getElementById("m-unlock-pw")!.addEventListener("keydown", (e) => {
      if (e.key === "Enter") (document.getElementById("m-btn-unlock") as HTMLButtonElement).click();
    });
    // 已开启生物识别 → 自动弹指纹（表单已渲染，取消即回落）
    if (await api.biometricEnabled().catch(() => false)) {
      if (await tryBiometricUnlock()) {
        try {
          await enterMobileMain();
        } catch {
          showErr("数据加载失败，请重试");
        }
      }
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
    for (const id of ["m-create-pw1", "m-create-pw2"]) {
      document.getElementById(id)!.addEventListener("keydown", (e) => {
        if (e.key === "Enter") (document.getElementById("m-btn-create") as HTMLButtonElement).click();
      });
    }
  }
}
