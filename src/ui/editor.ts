import * as api from "../api";
import { toast } from "./toast";

function getVal(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}
function setVal(id: string, v: string): void {
  (document.getElementById(id) as HTMLInputElement).value = v;
}
function checked(id: string): boolean {
  return (document.getElementById(id) as HTMLInputElement).checked;
}

async function regenerate(): Promise<void> {
  const opts: api.GenOptions = {
    length: Number(getVal("gen-length")) || 20,
    lowercase: checked("gen-lower"),
    uppercase: checked("gen-upper"),
    digits: checked("gen-digits"),
    symbols: checked("gen-symbols"),
    exclude_ambiguous: checked("gen-amb"),
  };
  const out = document.getElementById("gen-out") as HTMLOutputElement;
  try {
    const pw = await api.passwordGenerate(opts);
    out.textContent = pw;
    out.dataset.pw = pw; // 真正的密码存 dataset；textContent 可能显示错误信息
  } catch (err) {
    out.textContent = String(err);
    delete out.dataset.pw;
  }
}

/** 打开条目编辑对话框。entry=null 表示新增。 */
export function openEditor(
  entry: api.Entry | null,
  onSaved: (e: api.Entry) => void | Promise<void>,
): void {
  const dlg = document.getElementById("dialog-editor")!;
  document.getElementById("ed-title")!.textContent = entry ? "编辑条目" : "新增条目";
  setVal("ed-app", entry?.app_name ?? "");
  setVal("ed-username", entry?.username ?? "");
  setVal("ed-password", entry?.password ?? "");
  setVal("ed-url", entry?.url ?? "");
  setVal("ed-notes", entry?.notes ?? "");
  document.getElementById("ed-gen")!.classList.add("hidden");
  dlg.classList.remove("hidden");

  document.getElementById("ed-cancel")!.onclick = () => dlg.classList.add("hidden");

  document.getElementById("ed-save")!.onclick = async () => {
    const appName = getVal("ed-app").trim();
    if (!appName) { toast("应用名不能为空"); return; }
    const payload: api.Entry = {
      id: entry?.id ?? "",
      app_name: appName,
      username: getVal("ed-username").trim(),
      password: getVal("ed-password"),
      url: getVal("ed-url").trim(),
      notes: getVal("ed-notes"),
      created_at: entry?.created_at ?? 0,
      updated_at: 0,
    };
    try {
      const saved = await api.entrySave(payload);
      dlg.classList.add("hidden");
      await onSaved(saved);
      toast("已保存");
    } catch (err) {
      toast(`保存失败：${String(err)}`);
    }
  };

  document.getElementById("ed-generate")!.onclick = () => {
    document.getElementById("ed-gen")!.classList.remove("hidden");
    void regenerate();
  };
  document.getElementById("gen-go")!.onclick = () => { void regenerate(); };
  document.getElementById("gen-use")!.onclick = () => {
    const out = document.getElementById("gen-out") as HTMLOutputElement;
    const pw = out.dataset.pw ?? "";
    if (!pw) { toast("请先生成密码"); return; }
    setVal("ed-password", pw);
    document.getElementById("ed-gen")!.classList.add("hidden");
  };
  document.getElementById("gen-length")!.addEventListener("input", (e) => {
    document.getElementById("gen-length-val")!.textContent =
      (e.target as HTMLInputElement).value;
  });
}
