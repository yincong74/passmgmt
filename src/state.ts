import type { Entry, Settings } from "./api";

export interface UiState {
  entries: Entry[];
  selectedId: string | null;
  query: string;
  settings: Settings;
  showPassword: boolean;
}

export const state: UiState = {
  entries: [],
  selectedId: null,
  query: "",
  settings: { auto_lock_minutes: 15, clipboard_clear_seconds: 30 },
  showPassword: false,
};
