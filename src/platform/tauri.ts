import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { GameState, SpellData } from "../league/types";
import type { Backend, QuickBindHit } from "./backend";

/** Commands and events defined in src-tauri/src (names must match there). */
export const tauriBackend: Backend = {
  getSpellData: () => invoke<SpellData>("get_spell_data"),
  getGameState: () => invoke<GameState>("get_game_state"),
  setWheelMode: (mode) => invoke<void>("set_wheel", { mode }),

  async onHotkey(handler) {
    await listen<boolean>("hotkey", (event) => handler(event.payload));
  },

  async onGameState(handler) {
    await listen<GameState>("game-state", (event) => handler(event.payload));
  },

  async onQuickBind(handler) {
    await listen<QuickBindHit>("quick-bind", (event) => handler(event.payload));
  },
};
