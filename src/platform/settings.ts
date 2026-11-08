import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Role } from "../league/types";

/** Mirrors `Config` in src-tauri/src/config.rs. */
export interface Settings {
  /** e.g. "Backquote", "Ctrl+Alt+KeyQ". */
  hotkey: string;
  quickBinds: QuickBind[];
}

export interface QuickBind {
  role: Role;
  /** Base spell id, e.g. "SummonerFlash". */
  spell: string;
  /** Key codes typed in order while holding the hotkey, e.g. ["KeyT", "KeyD"]. */
  keys: string[];
}

/** What the settings page needs from the Rust side (src-tauri/src/settings.rs). */
export interface SettingsBackend {
  getSettings(): Promise<Settings>;
  /** Applies and saves; rejects with a message for the user. */
  saveSettings(settings: Settings): Promise<void>;
  close(): Promise<void>;
  /** Whether the overlay window is showing (hidden = hotkey only opens settings). */
  isOverlayShown(): Promise<boolean>;
  /** Hides or shows the overlay for this session. */
  setOverlayShown(shown: boolean): Promise<void>;
  /** Exits the whole app. */
  quit(): Promise<void>;
}

const tauriSettings: SettingsBackend = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings) => invoke<void>("save_settings", { settings }),
  close: () => getCurrentWindow().close(),
  isOverlayShown: () => invoke<boolean>("get_overlay_shown"),
  setOverlayShown: (shown) => invoke<void>("set_overlay_shown", { shown }),
  quit: () => invoke<void>("quit_app"),
};

/** `npm run dev` + /settings.html in a browser: remembers settings until reload. */
function createPreviewSettings(): SettingsBackend {
  document.body.classList.add("preview");
  let settings: Settings = {
    hotkey: "Backquote",
    quickBinds: [{ role: "TOP", spell: "SummonerFlash", keys: ["KeyT", "KeyD"] }],
  };
  let overlayShown = true;
  return {
    getSettings: async () => structuredClone(settings),
    saveSettings: async (next) => {
      settings = structuredClone(next);
    },
    close: async () => console.info("settings closed"),
    isOverlayShown: async () => overlayShown,
    setOverlayShown: async (shown) => {
      overlayShown = shown;
    },
    quit: async () => console.info("app quit"),
  };
}

export const settingsBackend: SettingsBackend =
  "__TAURI_INTERNALS__" in window ? tauriSettings : createPreviewSettings();
