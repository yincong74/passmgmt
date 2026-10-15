import {
  save as saveDialog,
  open as openDialog,
  confirm as confirmDialog,
} from "@tauri-apps/plugin-dialog";
import { readFile, stat, writeFile } from "@tauri-apps/plugin-fs";
import { checkStatus, BiometryType } from "@tauri-apps/plugin-biometric";
import { remove, store } from "@impierce/tauri-plugin-keystore";
import * as api from "../../api";
import { state } from "../../state";
import { toast } from "../toast";
import { KEY_ALIAS } from "./bio";
import { showMobileLock } from "./lock";

/** 导入备份文件大小上限（review Task8#3：number[] JSON 桥约有 3-4 倍膨胀，防止误选大文件打爆内存） */
const MAX_BACKUP_BYTES = 50 * 1024 * 1024;

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
  // 容量守卫：stat 可用则先查大小；stat 对某些 content URI 失败时放行，由后端校验兜底
  try {
    const info = await stat(String(uri));
    if (info.size > MAX_BACKUP_BYTES) {
      toast(`文件过大（${Math.round(info.size / 1024 / 1024)} MB），不是有效的备份文件`);
      return;
    }
  } catch { /* stat 不支持该 URI → 继续，读取或后端验证会拦截 */ }
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
