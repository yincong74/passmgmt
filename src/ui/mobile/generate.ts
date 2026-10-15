import * as api from "../../api";
import { state } from "../../state";
import { toast } from "../toast";

function val(id: string): string {
  return (document.getElementById(id) as HTMLInputElement).value;
}
function chk(id: string): boolean {
  return (document.getElementById(id) as HTMLInputElement).checked;
}

export function renderGenerate(el: HTMLElement): void {
  el.innerHTML = `
    <div class="m-page">
      <div class="m-nav"><h2>密码生成器</h2></div>
      <label class="m-inline">长度
        <input id="g-length" type="range" min="8" max="64" value="20" />
        <span id="g-len-val">20</span></label>
      <label class="m-inline"><input id="g-lower" type="checkbox" checked /> 小写 a-z</label>
      <label class="m-inline"><input id="g-upper" type="checkbox" checked /> 大写 A-Z</label>
      <label class="m-inline"><input id="g-digits" type="checkbox" checked /> 数字 0-9</label>
      <label class="m-inline"><input id="g-symbols" type="checkbox" checked /> 符号 !@#$…</label>
      <label class="m-inline"><input id="g-amb" type="checkbox" checked /> 排除易混淆字符 0O1lI|</label>
      <output id="g-out" class="m-gen-out"></output>
      <div class="m-row">
        <button class="primary m-big m-flex1" id="g-go" type="button">生成</button>
        <button class="m-big m-flex1" id="g-copy" type="button">复制</button>
      </div>
    </div>`;

  const regen = async (): Promise<void> => {
    const out = document.getElementById("g-out") as HTMLOutputElement;
    try {
      const pw = await api.passwordGenerate({
        length: Number(val("g-length")) || 20,
        lowercase: chk("g-lower"),
        uppercase: chk("g-upper"),
        digits: chk("g-digits"),
        symbols: chk("g-symbols"),
        exclude_ambiguous: chk("g-amb"),
      });
      out.textContent = pw;
      out.dataset.pw = pw;
    } catch (err) {
      out.textContent = String(err);
      delete out.dataset.pw;
    }
  };
  document.getElementById("g-length")!.addEventListener("input", (ev) => {
    document.getElementById("g-len-val")!.textContent = (ev.target as HTMLInputElement).value;
  });
  document.getElementById("g-go")!.onclick = () => { void regen(); };
  document.getElementById("g-copy")!.onclick = async () => {
    const out = document.getElementById("g-out") as HTMLOutputElement;
    const pw = out.dataset.pw ?? "";
    if (!pw) { toast("请先生成密码"); return; }
    await api.clipboardCopy(pw, true);
    const s = state.settings.clipboard_clear_seconds;
    toast(s > 0 ? `已复制，${s} 秒后自动清除剪贴板` : "已复制");
  };
  void regen();
}
