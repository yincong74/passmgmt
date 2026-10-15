import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../../api";
import { state } from "../../state";
import { esc } from "../list";
import { toast } from "../toast";
import { renderEditor } from "./editor";
import { popPage, pushPage } from "./shell";

function fmtTime(secs: number): string {
  return secs === 0 ? "—" : new Date(secs * 1000).toLocaleString();
}

export async function renderDetail(el: HTMLElement, id: string): Promise<void> {
  const e = state.entries.find((x) => x.id === id);
  if (!e) { popPage(); return; }
  const dots = "•".repeat(Math.max(Math.min(e.password.length, 24), 8));
  el.innerHTML = `
    <div class="m-page">
      <div class="m-nav">
        <button class="m-btn-back" id="d-back" type="button">←</button>
        <h2>${esc(e.app_name)}</h2>
      </div>
      <div class="m-detail-row">
        <span class="k">用户名</span>
        <span class="v">${esc(e.username) || "—"}</span>
        <div class="m-row"><button class="small" id="d-copy-user" type="button">复制用户名</button></div>
      </div>
      <div class="m-detail-row">
        <span class="k">密码</span>
        <span class="v">${state.showPassword ? esc(e.password) : dots}</span>
        <div class="m-row">
          <button class="small" id="d-toggle" type="button">${state.showPassword ? "隐藏" : "显示"}</button>
          <button class="small" id="d-copy-pw" type="button">复制密码</button>
        </div>
      </div>
      ${e.url ? `<div class="m-detail-row"><span class="k">URL</span><span class="v">${esc(e.url)}</span></div>` : ""}
      ${e.notes ? `<div class="m-detail-row"><span class="k">备注</span><span class="v m-pre-wrap">${esc(e.notes)}</span></div>` : ""}
      <p class="m-hint">创建：${fmtTime(e.created_at)} ・ 更新：${fmtTime(e.updated_at)}</p>
      <div class="m-row">
        <button class="primary m-big m-flex1" id="d-edit" type="button">编辑</button>
        <button class="danger m-big m-flex1" id="d-del" type="button">删除</button>
      </div>
    </div>`;

  document.getElementById("d-back")!.onclick = () => popPage();
  document.getElementById("d-copy-user")!.onclick = async () => {
    await api.clipboardCopy(e.username, false);
    toast("已复制");
  };
  document.getElementById("d-toggle")!.onclick = () => {
    state.showPassword = !state.showPassword;
    void renderDetail(el, id);
  };
  document.getElementById("d-copy-pw")!.onclick = async () => {
    await api.clipboardCopy(e.password, true);
    const s = state.settings.clipboard_clear_seconds;
    toast(s > 0 ? `密码已复制，${s} 秒后自动清除剪贴板` : "密码已复制");
  };
  document.getElementById("d-edit")!.onclick = () => pushPage((p) => renderEditor(p, e));
  document.getElementById("d-del")!.onclick = async () => {
    const ok = await confirmDialog(`确定删除「${e.app_name}」的条目？此操作不可恢复。`, {
      title: "确认删除",
      kind: "warning",
    });
    if (!ok) return;
    try {
      await api.entryDelete(e.id);
      state.entries = state.entries.filter((x) => x.id !== e.id);
      toast("已删除");
      popPage();
    } catch (err) {
      toast(`删除失败：${String(err)}`);
    }
  };
}
