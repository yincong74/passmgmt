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
