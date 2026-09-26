import * as api from "./api";
import { initLock } from "./ui/lock";
import { toast } from "./ui/toast";

async function onUnlocked(): Promise<void> {
  toast("解锁成功（主界面在后续任务接入）");
}

async function boot(): Promise<void> {
  const st = await api.vaultStatus();
  if (st.unlocked) {
    await onUnlocked();
    return;
  }
  await initLock(onUnlocked);
}

void boot();
