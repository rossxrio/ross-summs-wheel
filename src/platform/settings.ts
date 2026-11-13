import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Role } from "../league/types";
import type { VoiceHeard, VoiceListening } from "./backend";

/** Mirrors `Config` in src-tauri/src/config.rs. */
export interface Settings {
  /** e.g. "Backquote", "Ctrl+Alt+KeyQ". */
  hotkey: string;
  quickBinds: QuickBind[];
  voice: VoiceSettings;
  /** Overlay sounds; set with `setSounds`, which applies and saves at once. */
  sounds: boolean;
}

/** Mirrors `VoiceConfig` in src-tauri/src/voice/mod.rs. */
export interface VoiceSettings {
  enabled: boolean;
  /** Key code of the push-to-talk key, e.g. "KeyV". Empty when not set. */
  key: string;
  /** Whisper model id, e.g. "tiny.en". */
  model: string;
}

/** Mirrors `VoiceStatus` in src-tauri/src/voice/mod.rs. */
export interface VoiceStatus {
  state: "off" | "needsModel" | "loading" | "ready" | "error";
  message: string | null;
  models: { id: string; label: string; sizeMb: number; downloaded: boolean }[];
}

export interface DownloadProgress {
  model: string;
  received: number;
  total: number;
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
  /** Turns the overlay's sounds on or off, and remembers it. */
  setSounds(on: boolean): Promise<void>;
  /** Exits the whole app. */
  quit(): Promise<void>;
  getVoiceStatus(): Promise<VoiceStatus>;
  onVoiceStatus(handler: (status: VoiceStatus) => void): Promise<void>;
  /** Resolves once downloaded and checked; rejects with a message. */
  downloadVoiceModel(model: string): Promise<void>;
  onVoiceDownload(handler: (progress: DownloadProgress) => void): Promise<void>;
  /** The "hold to test" button: true on press, false on release. */
  voiceTest(pressed: boolean): Promise<void>;
  onVoiceListening(handler: (listening: VoiceListening) => void): Promise<void>;
  onVoiceHeard(handler: (heard: VoiceHeard) => void): Promise<void>;
}

const tauriSettings: SettingsBackend = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings) => invoke<void>("save_settings", { settings }),
  close: () => getCurrentWindow().close(),
  isOverlayShown: () => invoke<boolean>("get_overlay_shown"),
  setOverlayShown: (shown) => invoke<void>("set_overlay_shown", { shown }),
  setSounds: (on) => invoke<void>("set_sounds", { on }),
  quit: () => invoke<void>("quit_app"),
  getVoiceStatus: () => invoke<VoiceStatus>("get_voice_status"),
  async onVoiceStatus(handler) {
    await listen<VoiceStatus>("voice-status", (e) => handler(e.payload));
  },
  downloadVoiceModel: (model) => invoke<void>("download_voice_model", { model }),
  async onVoiceDownload(handler) {
    await listen<DownloadProgress>("voice-download", (e) => handler(e.payload));
  },
  voiceTest: (pressed) => invoke<void>("voice_test", { pressed }),
  async onVoiceListening(handler) {
    await listen<VoiceListening>("voice-listening", (e) => handler(e.payload));
  },
  async onVoiceHeard(handler) {
    await listen<VoiceHeard>("voice-heard", (e) => handler(e.payload));
  },
};

/** `npm run dev` + /settings.html in a browser: remembers settings until reload. */
function createPreviewSettings(): SettingsBackend {
  document.body.classList.add("preview");
  let settings: Settings = {
    hotkey: "Backquote",
    quickBinds: [{ role: "TOP", spell: "SummonerFlash", keys: ["KeyT", "KeyD"] }],
    voice: { enabled: false, key: "", model: "tiny.en" },
    sounds: true,
  };
  const models = [
    { id: "tiny.en", label: "Fast (~0.5 s)", sizeMb: 78, downloaded: false },
    { id: "base.en", label: "Balanced (~1 s)", sizeMb: 148, downloaded: false },
    { id: "small.en", label: "Accurate (~3 s)", sizeMb: 488, downloaded: false },
  ];
  const status = (): VoiceStatus => {
    const ready = models.some((m) => m.id === settings.voice.model && m.downloaded);
    return { state: !settings.voice.enabled ? "off" : ready ? "ready" : "needsModel", message: null, models };
  };
  let statusHandler: (s: VoiceStatus) => void = () => {};
  let heardHandler: (h: VoiceHeard) => void = () => {};
  let listeningHandler: (l: VoiceListening) => void = () => {};
  let downloadHandler: (p: DownloadProgress) => void = () => {};
  let overlayShown = true;
  return {
    getSettings: async () => structuredClone(settings),
    saveSettings: async (next) => {
      settings = structuredClone(next);
      statusHandler(status());
    },
    close: async () => console.info("settings closed"),
    isOverlayShown: async () => overlayShown,
    setOverlayShown: async (shown) => {
      overlayShown = shown;
    },
    setSounds: async (on) => {
      settings.sounds = on;
    },
    quit: async () => console.info("app quit"),
    getVoiceStatus: async () => status(),
    onVoiceStatus: async (h) => void (statusHandler = h),
    downloadVoiceModel: async (id) => {
      const model = models.find((m) => m.id === id)!;
      const total = model.sizeMb * 1e6;
      for (let received = 0; received <= total; received += total / 20) {
        downloadHandler({ model: id, received, total });
        await new Promise((r) => setTimeout(r, 60));
      }
      model.downloaded = true;
      statusHandler(status());
    },
    onVoiceDownload: async (h) => void (downloadHandler = h),
    voiceTest: async (pressed) => {
      if (pressed) return listeningHandler({ state: "listening", test: true });
      listeningHandler({ state: "thinking", test: true });
      const text = "ADC no flash";
      setTimeout(
        () =>
          heardHandler({
            text,
            words: text.split(" ").map((t) => ({ text: t, agoMs: 900 })),
            decodeMs: 450,
            test: true,
          }),
        450,
      );
    },
    onVoiceListening: async (h) => void (listeningHandler = h),
    onVoiceHeard: async (h) => void (heardHandler = h),
  };
}

export const settingsBackend: SettingsBackend =
  "__TAURI_INTERNALS__" in window ? tauriSettings : createPreviewSettings();
