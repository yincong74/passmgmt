import "./mobile.css";

export type TabName = "entries" | "generate" | "settings";

type PageRender = (el: HTMLElement) => void | Promise<void>;

const stack: PageRender[] = [];
let currentTab: TabName = "entries";
// 锁定屏状态：禁止 popstate 渲染任何数据页（防系统返回手势回放明文，review C1）
let locked = false;

function pages(): HTMLElement {
  return document.getElementById("m-pages")!;
}

function showTabbar(v: boolean): void {
  document.getElementById("m-tabbar")!.classList.toggle("hidden", !v);
}

export function installShell(): void {
  document.body.innerHTML = `
    <div id="m-root">
      <main id="m-pages"></main>
      <nav id="m-tabbar" class="hidden">
        <button type="button" class="tab" data-tab="entries">🗂️<span>条目</span></button>
        <button type="button" class="tab" data-tab="generate">🎲<span>生成</span></button>
        <button type="button" class="tab" data-tab="settings">⚙️<span>设置</span></button>
      </nav>
    </div>
    <div id="toast" class="hidden"></div>`;
  document.querySelectorAll<HTMLButtonElement>("#m-tabbar .tab").forEach((b) => {
    b.onclick = () => switchTab(b.dataset.tab as TabName);
  });
  // 系统返回手势/返回键 → popstate → 弹页面栈
  window.addEventListener("popstate", (e) => {
    if (locked) {
      showTabbar(false);
      return;
    }
    const depth: number = (e.state as { depth?: number } | null)?.depth ?? 0;
    stack.length = Math.min(stack.length, depth);
    if (stack.length === 0) {
      showTabbar(true);
      void renderTab(currentTab);
    } else {
      showTabbar(false);
      void stack[stack.length - 1](pages());
    }
  });
}

export function switchTab(name: TabName): void {
  locked = false;
  currentTab = name;
  stack.length = 0;
  history.replaceState({ depth: 0 }, "");
  showTabbar(true);
  document
    .querySelectorAll("#m-tabbar .tab")
    .forEach((b) => b.classList.toggle("active", b.getAttribute("data-tab") === name));
  void renderTab(name);
}

export function pushPage(render: PageRender): void {
  stack.push(render);
  history.pushState({ depth: stack.length }, "");
  showTabbar(false);
  void render(pages());
}

/** 返回上一页；渲染统一由 popstate 处理 */
export function popPage(): void {
  if (stack.length > 0) history.back();
}

/** 锁定屏接管：清空页面栈（不触发 history） */
export function resetStack(): void {
  locked = true;
  stack.length = 0;
  history.replaceState({ depth: 0 }, "");
  showTabbar(false);
}

async function renderTab(name: TabName): Promise<void> {
  const el = pages();
  // 动态 import 打破 shell ↔ 页面模块的循环依赖；await 期间可能进入锁定态，渲染前复查
  if (name === "entries") {
    const { renderEntries } = await import("./entries");
    if (locked) return;
    await renderEntries(el);
  } else if (name === "generate") {
    const { renderGenerate } = await import("./generate");
    if (locked) return;
    renderGenerate(el);
  } else {
    const { renderSettings } = await import("./settings");
    if (locked) return;
    await renderSettings(el);
  }
}
