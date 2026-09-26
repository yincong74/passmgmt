import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import { initLock } from "./ui/lock";
import { enterMain, initMain } from "./ui/list";

async function onUnlocked(): Promise<void> {
  await enterMain();
}

async function boot(): Promise<void> {
  initMain(onUnlocked);
  const st = await api.vaultStatus();
  if (st.unlocked) await enterMain();
  else await initLock(onUnlocked);
  // Rust 侧自动锁定线程触发时回到解锁屏
  await listen("vault-locked", () => { void initLock(onUnlocked); });
}

void boot();
