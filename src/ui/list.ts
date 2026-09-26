import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import { state } from "../state";
import { openEditor } from "./editor";
import { initLock } from "./lock";
import { showScreen } from "./screens";
import { openSettings } from "./settings";
import { toast } from "./toast";

/** Review Focus #3：所有用户数据入 innerHTML 前必须经此转义 */
export function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export async function enterMain(): Promise<void> {
  state.entries = await api.entriesList();
  state.settings = await api.settingsGet();
  state.query = "";
  state.showPassword = false;
  (document.getElementById("search") as HTMLInputElement).value = "";
  state.selectedId = state.entries.length > 0 ? state.entries[0].id : null;
  showScreen("main");
  renderAll();
}

export function filteredEntries(): api.Entry[] {
  const q = state.query.trim().toLowerCase();
  if (!q) return state.entries;
  return state.entries.filter((e) =>
    [e.app_name, e.username, e.url, e.notes].some((f) => f.toLowerCase().includes(q)),
  );
}

export function renderAll(): void {
  renderList();
  renderDetail();
}

function fmtTime(secs: number): string {
  return secs === 0 ? "—" : new Date(secs * 1000).toLocaleString();
}

function renderList(): void {
  const el = document.getElementById("entry-list")!;
  const items = filteredEntries();
  el.innerHTML = items.length
    ? items
        .map(
          (e) => `
      <div class="entry-item ${e.id === state.selectedId ? "selected" : ""}" data-id="${e.id}">
        <div class="entry-app">${esc(e.app_name)}</div>
        <div class="entry-user">${esc(e.username)}</div>
      </div>`,
        )
        .join("")
    : `<p class="hint" style="padding:12px">${
        state.query ? "无匹配条目" : "还没有条目，点击右上「＋ 新增条目」"
      }</p>`;
  el.querySelectorAll<HTMLElement>(".entry-item").forEach((item) => {
    item.onclick = () => {
      state.selectedId = item.dataset.id!;
      state.showPassword = false;
      renderAll();
    };
  });
}

function fieldRow(label: string, valueHtml: string, copyText: string): string {
  return `<tr><th>${label}</th><td><code>${valueHtml}</code>` +
    `<button class="small btn-copy" data-copy="${encodeURIComponent(copyText)}">复制</button></td></tr>`;
}

function renderDetail(): void {
  const el = document.getElementById("entry-detail")!;
  const e = state.entries.find((x) => x.id === state.selectedId);
  if (!e) {
    el.innerHTML = `<p class="hint">在左侧选择一个条目，或点击「＋ 新增条目」</p>`;
    return;
  }
  const dots = "•".repeat(Math.max(Math.min(e.password.length, 24), 8));
  const pwShown = state.showPassword ? esc(e.password) : dots;
  el.innerHTML = `
    <h2>${esc(e.app_name)}</h2>
    <table class="detail">
      ${fieldRow("用户名", esc(e.username), e.username)}
      <tr><th>密码</th><td>
        <code>${pwShown}</code>
        <button class="small" id="btn-toggle-pw" type="button">${state.showPassword ? "隐藏" : "显示"}</button>
        <button class="small" id="btn-copy-pw" type="button">复制</button>
      </td></tr>
      ${e.url ? fieldRow("URL", esc(e.url), e.url) : ""}
      ${e.notes ? `<tr><th>备注</th><td class="notes-cell">${esc(e.notes)}</td></tr>` : ""}
    </table>
    <div class="detail-actions">
      <button class="primary" id="btn-edit" type="button">编辑</button>
      <button class="danger" id="btn-delete" type="button">删除</button>
    </div>
    <p class="hint">创建：${fmtTime(e.created_at)} ・ 更新：${fmtTime(e.updated_at)}</p>`;

  el.querySelectorAll<HTMLButtonElement>(".btn-copy").forEach((b) => {
    b.onclick = async () => {
      await api.clipboardCopy(decodeURIComponent(b.dataset.copy!), false);
      toast("已复制");
    };
  });
  el.querySelector<HTMLButtonElement>("#btn-toggle-pw")!.onclick = () => {
    state.showPassword = !state.showPassword;
    renderDetail();
  };
  el.querySelector<HTMLButtonElement>("#btn-copy-pw")!.onclick = async () => {
    await api.clipboardCopy(e.password, true);
    const s = state.settings.clipboard_clear_seconds;
    toast(s > 0 ? `密码已复制，${s} 秒后自动清除剪贴板` : "密码已复制");
  };
  el.querySelector<HTMLButtonElement>("#btn-edit")!.onclick = () =>
    openEditor(e, onEditorSaved);
  el.querySelector<HTMLButtonElement>("#btn-delete")!.onclick = async () => {
    const ok = await confirmDialog(`确定删除「${e.app_name}」的条目？此操作不可恢复。`, {
      title: "确认删除",
      kind: "warning",
    });
    if (!ok) return;
    try {
      await api.entryDelete(e.id);
      state.entries = state.entries.filter((x) => x.id !== e.id);
      state.selectedId = state.entries.length > 0 ? state.entries[0].id : null;
      renderAll();
      toast("已删除");
    } catch (err) {
      toast(`删除失败：${String(err)}`);
    }
  };
}

async function onEditorSaved(saved: api.Entry): Promise<void> {
  const i = state.entries.findIndex((x) => x.id === saved.id);
  if (i >= 0) state.entries[i] = saved;
  else state.entries.unshift(saved);
  state.selectedId = saved.id;
  state.showPassword = false;
  renderAll();
}

let wired = false;

/** 一次性绑定工具栏事件。 */
export function initMain(onUnlocked: () => Promise<void>): void {
  if (wired) return;
  wired = true;
  document.getElementById("search")!.addEventListener("input", (ev) => {
    state.query = (ev.target as HTMLInputElement).value;
    renderList();
  });
  document.getElementById("btn-add")!.onclick = () => openEditor(null, onEditorSaved);
  document.getElementById("btn-settings")!.onclick = () => { void openSettings(); };
  document.getElementById("btn-lock")!.onclick = async () => {
    await api.vaultLock();
    await initLock(onUnlocked);
  };
}
