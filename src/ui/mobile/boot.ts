import { listen } from "@tauri-apps/api/event";
import * as api from "../../api";
import { state } from "../../state";
import { showMobileLock, enterMobileMain } from "./lock";
import { installShell } from "./shell";

let lastActivity = Date.now();
let lastPing = 0;

function bump(): void {
  lastActivity = Date.now();
  // 节流：最多每 10 秒向 Rust 侧汇报一次活动（Rust 兜底锁定用），与桌面 main.ts 同策略
  if (lastActivity - lastPing > 10_000) {
    lastPing = lastActivity;
    api.activityPing().catch(() => {});
  }
}

async function checkAutoLock(): Promise<void> {
  const mins = state.settings.auto_lock_minutes;
  if (mins <= 0 || Date.now() - lastActivity <= mins * 60_000) return;
  const s = await api.vaultStatus().catch(() => null);
  if (s?.unlocked) {
    await api.vaultLock().catch(() => {});
    await showMobileLock();
  }
}

export async function bootMobile(): Promise<void> {
  installShell();

  // 先装监听与定时器，再做初始渲染：即使初始渲染异常，Rust 自动锁定事件也不会丢失（review I1）
  await listen("vault-locked", () => { void showMobileLock(); });
  window.addEventListener("touchstart", bump, { passive: true });
  window.addEventListener("click", bump);
  window.addEventListener("keydown", bump);
  // 前端自动锁定：15 秒定时 + 回前台立即检查（Android 后台冻结时定时器停走，靠墙钟差值补判）
  window.setInterval(() => { void checkAutoLock(); }, 15_000);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") void checkAutoLock();
  });

  const st = await api.vaultStatus().catch(() => null);
  if (st?.unlocked) await enterMobileMain().catch(() => { void showMobileLock(); });
  else await showMobileLock();
}
