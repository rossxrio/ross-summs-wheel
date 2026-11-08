import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** What the settings page needs from the Rust side (src-tauri/src/settings.rs). */
export interface SettingsBackend {
  getHotkey(): Promise<string>;
  /** Applies and saves the hotkey; rejects with a message for the user. */
  setHotkey(hotkey: string): Promise<void>;
  close(): Promise<void>;
}

const tauriSettings: SettingsBackend = {
  getHotkey: () => invoke<string>("get_hotkey"),
  setHotkey: (hotkey) => invoke<void>("set_hotkey", { hotkey }),
  close: () => getCurrentWindow().close(),
};

/** `npm run dev` + /settings.html in a browser: remembers the hotkey until reload. */
function createPreviewSettings(): SettingsBackend {
  document.body.classList.add("preview");
  let hotkey = "Backquote";
  return {
    getHotkey: async () => hotkey,
    setHotkey: async (next) => {
      hotkey = next;
    },
    close: async () => console.info("settings closed"),
  };
}

export const settingsBackend: SettingsBackend =
  "__TAURI_INTERNALS__" in window ? tauriSettings : createPreviewSettings();
