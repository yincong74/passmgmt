import * as api from "../api";
import { state } from "../state";
import { toast } from "./toast";
import { showScreen } from "./screens";

let wired = false;
/** 解锁成功回调（main.ts boot 注册）：handleLockedError 跳回解锁屏时复用。 */
let onUnlockedHook: (() => Promise<void>) | null = null;

export function setOnUnlocked(cb: () => Promise<void>): void {
  onUnlockedHook = cb;
}

/** 锁定时收起并清空仍打开的对话框。
 * 两个 dialog 是悬浮在 screen 之上的独立层（z-index:10），不清会盖住解锁屏、
 * 让用户困在“看起来能用、实际全部报未解锁”的弹窗里；且输入框里的
 * 明文密码/条目字段违反 AGENTS.md 锁定清零约定（JS 侧不得残留明文）。 */
function wipeDialogs(): void {
  for (const id of ["dialog-editor", "dialog-settings"]) {
    const dlg = document.getElementById(id);
    if (!dlg) continue;
    dlg
      .querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLOutputElement>(
        "input, textarea, output",
      )
      .forEach((el) => {
        el.value = "";
      });
    // 生成器面板的密码另存于 dataset（editor.ts regenerate），一并擦除
    dlg.querySelectorAll<HTMLOutputElement>("output").forEach((el) => {
      delete el.dataset.pw;
    });
    dlg.classList.add("hidden");
  }
  document.getElementById("ed-gen")?.classList.add("hidden");
}

/** 受保护命令返回「未解锁」时的统一处理（设计 §196：前端跳回解锁屏）。
 * 返回 true 表示已接管，调用方无需再 toast 原始错误。 */
export async function handleLockedError(err: unknown): Promise<boolean> {
  if (String(err) !== "未解锁") return false;
  toast("应用已锁定，请重新解锁后再操作");
  await initLock(onUnlockedHook ?? (async () => {}));
  return true;
}

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
  onUnlockedHook = onUnlocked;
  // 安全约定（AGENTS.md）：锁定时清空前端明文缓存，JS 侧不残留条目明文
  state.entries = [];
  state.selectedId = null;
  state.query = "";
  state.showPassword = false;
  // 隐藏的 #screen-main 里旧渲染结果仍是明文 DOM，一并擦除（enterMain→renderAll 会重新填充）
  const list = document.getElementById("entry-list");
  const detail = document.getElementById("entry-detail");
  if (list) list.innerHTML = "";
  if (detail) detail.innerHTML = "";
  // 仍打开的编辑/设置对话框：收起并清空（盖在解锁屏之上 + 残留明文，见 wipeDialogs）
  wipeDialogs();

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
