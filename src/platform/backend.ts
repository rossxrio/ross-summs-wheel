import type { GameState, Role, SpellData } from "../league/types";
import type { WheelMode } from "../wheel/types";
import { createPreviewBackend } from "./preview";
import { tauriBackend } from "./tauri";

/** Everything the frontend needs from the Rust side. */
export interface Backend {
  /** Spell and item data for the current patch (Data Dragon). */
  getSpellData(): Promise<SpellData>;
  /** Latest known game state (Live Client Data API). */
  getGameState(): Promise<GameState>;
  setWheelMode(mode: WheelMode): Promise<void>;
  /** Global hotkey: true on press, false on release. */
  onHotkey(handler: (pressed: boolean) => void): Promise<void>;
  onGameState(handler: (state: GameState) => void): Promise<void>;
  /** A quick bind was typed while holding the hotkey (see src-tauri/src/quick_binds.rs). */
  onQuickBind(handler: (bind: QuickBindHit) => void): Promise<void>;
}

/** `spell` is a base spell id, e.g. "SummonerFlash". */
export interface QuickBindHit {
  role: Role;
  spell: string;
}

/** True inside the Tauri webview; false when the page is opened in a normal browser. */
const inTauri = "__TAURI_INTERNALS__" in window;

/** The real app talks to Rust; `npm run dev` in a browser gets a fake game instead. */
export const backend: Backend = inTauri ? tauriBackend : createPreviewBackend();
