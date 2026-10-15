import * as api from "../../api";
import { state } from "../../state";
import { esc } from "../list";
import { toast } from "../toast";
import { popPage } from "./shell";

function val(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}
function chk(id: string): boolean {
  return (document.getElementById(id) as HTMLInputElement).checked;
}

async function regenerate(): Promise<void> {
  const opts: api.GenOptions = {
    length: Number(val("m-gen-length")) || 20,
    lowercase: chk("m-gen-lower"),
    uppercase: chk("m-gen-upper"),
    digits: chk("m-gen-digits"),
    symbols: chk("m-gen-symbols"),
    exclude_ambiguous: chk("m-gen-amb"),
  };
  const out = document.getElementById("m-gen-out") as HTMLOutputElement;
  try {
    const pw = await api.passwordGenerate(opts);
    out.textContent = pw;
    out.dataset.pw = pw; // 真正的密码存 dataset；textContent 可能显示错误信息
  } catch (err) {
    out.textContent = String(err);
    delete out.dataset.pw;
  }
}

/** entry=null 表示新增 */
export function renderEditor(el: HTMLElement, entry: api.Entry | null): void {
  el.innerHTML = `
    <div class="m-page">
      <div class="m-nav">
        <button class="m-btn-back" id="m-ed-cancel" type="button">←</button>
        <h2>${entry ? "编辑条目" : "新增条目"}</h2>
      </div>
      <label class="m-field">应用名 *
        <input id="m-ed-app" type="text" value="${esc(entry?.app_name ?? "")}" placeholder="例如：Gmail、网银" /></label>
      <label class="m-field">用户名
        <input id="m-ed-user" type="text" autocomplete="off" value="${esc(entry?.username ?? "")}" /></label>
      <label class="m-field">密码
        <input id="m-ed-pw" type="text" autocomplete="off" value="${esc(entry?.password ?? "")}" /></label>
      <div class="m-gen">
        <label class="m-inline">长度
          <input id="m-gen-length" type="range" min="8" max="64" value="20" />
          <span id="m-gen-len-val">20</span></label>
        <label class="m-inline"><input id="m-gen-lower" type="checkbox" checked /> 小写 a-z</label>
        <label class="m-inline"><input id="m-gen-upper" type="checkbox" checked /> 大写 A-Z</label>
        <label class="m-inline"><input id="m-gen-digits" type="checkbox" checked /> 数字 0-9</label>
        <label class="m-inline"><input id="m-gen-symbols" type="checkbox" checked /> 符号 !@#$…</label>
        <label class="m-inline"><input id="m-gen-amb" type="checkbox" checked /> 排除易混淆字符 0O1lI|</label>
        <output id="m-gen-out" class="m-gen-out"></output>
        <div class="m-row">
          <button class="small" id="m-gen-go" type="button">随机生成</button>
          <button class="small primary" id="m-gen-use" type="button">填入密码</button>
        </div>
      </div>
      <label class="m-field">URL
        <input id="m-ed-url" type="text" value="${esc(entry?.url ?? "")}" placeholder="https://…" /></label>
      <label class="m-field">备注
        <textarea id="m-ed-notes" rows="3">${esc(entry?.notes ?? "")}</textarea></label>
      <button class="primary m-big" id="m-ed-save" type="button">保存</button>
    </div>`;

  document.getElementById("m-ed-cancel")!.onclick = () => popPage();
  document.getElementById("m-gen-go")!.onclick = () => { void regenerate(); };
  document.getElementById("m-gen-use")!.onclick = () => {
    const out = document.getElementById("m-gen-out") as HTMLOutputElement;
    const pw = out.dataset.pw ?? "";
    if (!pw) { toast("请先生成密码"); return; }
    (document.getElementById("m-ed-pw") as HTMLInputElement).value = pw;
  };
  document.getElementById("m-gen-length")!.addEventListener("input", (ev) => {
    document.getElementById("m-gen-len-val")!.textContent = (ev.target as HTMLInputElement).value;
  });
  document.getElementById("m-ed-save")!.onclick = async () => {
    const appName = val("m-ed-app").trim();
    if (!appName) { toast("应用名不能为空"); return; }
    // 防双击重复保存（review Task14 I1）：请求期间禁用按钮，失败再恢复
    const saveBtn = document.getElementById("m-ed-save") as HTMLButtonElement;
    saveBtn.disabled = true;
    const payload: api.Entry = {
      id: entry?.id ?? "",
      app_name: appName,
      username: val("m-ed-user").trim(),
      password: val("m-ed-pw"),
      url: val("m-ed-url").trim(),
      notes: val("m-ed-notes"),
      created_at: entry?.created_at ?? 0,
      updated_at: 0,
    };
    try {
      const saved = await api.entrySave(payload);
      const i = state.entries.findIndex((x) => x.id === saved.id);
      if (i >= 0) state.entries[i] = saved;
      else state.entries.unshift(saved);
      state.showPassword = false;
      toast("已保存");
      popPage();
    } catch (err) {
      saveBtn.disabled = false;
      toast(`保存失败：${String(err)}`);
    }
  };
}
