import * as api from "../api";
import { showScreen } from "./screens";

let wired = false;

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
