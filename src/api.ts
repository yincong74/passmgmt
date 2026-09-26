import { invoke } from "@tauri-apps/api/core";

export interface Entry {
  id: string;
  app_name: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  created_at: number;
  updated_at: number;
}

export interface Settings {
  auto_lock_minutes: number;
  clipboard_clear_seconds: number;
}

export interface GenOptions {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  exclude_ambiguous: boolean;
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export const vaultStatus = (): Promise<VaultStatus> => invoke("vault_status");
export const vaultCreate = (masterPassword: string): Promise<void> =>
  invoke("vault_create", { masterPassword });
export const vaultUnlock = (masterPassword: string): Promise<void> =>
  invoke("vault_unlock", { masterPassword });
export const vaultLock = (): Promise<void> => invoke("vault_lock");
export const activityPing = (): Promise<void> => invoke("activity_ping");

export const entriesList = (): Promise<Entry[]> => invoke("entries_list");
export const entrySave = (entry: Entry): Promise<Entry> => invoke("entry_save", { entry });
export const entryDelete = (id: string): Promise<void> => invoke("entry_delete", { id });
export const settingsGet = (): Promise<Settings> => invoke("settings_get");
export const settingsSave = (settings: Settings): Promise<void> =>
  invoke("settings_save", { settings });
export const passwordGenerate = (opts: GenOptions): Promise<string> =>
  invoke("password_generate", { opts });
export const clipboardCopy = (text: string, secret: boolean): Promise<void> =>
  invoke("clipboard_copy", { text, secret });
export const backupExport = (destPath: string): Promise<void> =>
  invoke("backup_export", { destPath });
export const backupImport = (srcPath: string, backupPassword: string): Promise<void> =>
  invoke("backup_import", { srcPath, backupPassword });
export const masterPasswordChange = (oldPassword: string, newPassword: string): Promise<void> =>
  invoke("master_password_change", { oldPassword, newPassword });
