import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import { state } from "./state";
import { initLock } from "./ui/lock";
import { enterMain, initMain } from "./ui/list";

async function onUnlocked(): Promise<void> {
  await enterMain();
}

let lastActivity = Date.now();
let lastPing = 0;

function bump(): void {
  lastActivity = Date.now();
  // 节流：最多每 10 秒向 Rust 侧汇报一次活动（Rust 兜底锁定用）
  if (lastActivity - lastPing > 10_000) {
    lastPing = lastActivity;
    api.activityPing().catch(() => {});
  }
}

async function boot(): Promise<void> {
  initMain(onUnlocked);
  const st = await api.vaultStatus();
  if (st.unlocked) await enterMain();
  else await initLock(onUnlocked);
  // Rust 侧自动锁定线程触发时回到解锁屏
  await listen("vault-locked", () => { void initLock(onUnlocked); });

  window.addEventListener("keydown", bump);
  window.addEventListener("mousedown", bump);

  // 前端自动锁定：每 15 秒检查一次无活动时长
  window.setInterval(() => {
    const mins = state.settings.auto_lock_minutes;
    if (mins <= 0 || Date.now() - lastActivity <= mins * 60_000) return;
    void (async () => {
      const s = await api.vaultStatus().catch(() => null);
      if (s?.unlocked) {
        await api.vaultLock().catch(() => {});
        await initLock(onUnlocked);
      }
    })();
  }, 15_000);
}

void boot();
