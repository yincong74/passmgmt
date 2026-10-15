import { state } from "../../state";
import { esc, filteredEntries } from "../list";
import { renderDetail } from "./detail";
import { renderEditor } from "./editor";
import { pushPage } from "./shell";

export async function renderEntries(el: HTMLElement): Promise<void> {
  el.innerHTML = `
    <div class="m-toolbar">
      <input type="search" id="m-search" placeholder="搜索应用名 / 用户名 / URL / 备注…"
        value="${esc(state.query)}" />
    </div>
    <div id="m-list"></div>
    <button id="m-fab" type="button">＋</button>`;
  document.getElementById("m-search")!.addEventListener("input", (ev) => {
    state.query = (ev.target as HTMLInputElement).value;
    renderMList();
  });
  document.getElementById("m-fab")!.onclick = () => pushPage((p) => renderEditor(p, null));
  renderMList();
}

function renderMList(): void {
  const el = document.getElementById("m-list")!;
  const items = filteredEntries();
  el.innerHTML = items.length
    ? items
        .map(
          (e) => `
      <div class="m-card" data-id="${e.id}">
        <div class="t">${esc(e.app_name)}</div>
        <div class="s">${esc(e.username)}</div>
      </div>`,
        )
        .join("")
    : `<p class="m-hint m-empty">${
        state.query ? "无匹配条目" : "还没有条目，点击右下「＋」新建"
      }</p>`;
  el.querySelectorAll<HTMLElement>(".m-card").forEach((c) => {
    c.onclick = () => pushPage((p) => renderDetail(p, c.dataset.id!));
  });
}
