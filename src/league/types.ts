/** Data shapes shared with the Rust backend (see src-tauri/src/riot). */

/** Riot's position names, as the Live Client Data API reports them. */
export type Role = "TOP" | "JUNGLE" | "MIDDLE" | "BOTTOM" | "UTILITY";

/** One enemy player in the current game. */
export interface Enemy {
  /** Riot position, or null when the game mode doesn't report one (blind, ARAM, custom). */
  role: Role | null;
  champion: string;
  /** Data Dragon champion id, e.g. "MonkeyKing" for Wukong. */
  championId: string;
  /** Raw spell ids, e.g. "SummonerFlash", "S12_SummonerTeleportUpgrade". */
  spells: [string, string];
  level: number;
  /** Item ids; some grant summoner spell haste (see SpellData.itemHaste). */
  items: number[];
  /** Primary and secondary rune tree ids. Minor runes aren't visible for enemies. */
  runeTrees: number[];
}

export interface GameState {
  inGame: boolean;
  /** 11 = Summoner's Rift, 12 = Howling Abyss. Null when unknown. */
  mapNumber: number | null;
  enemies: Enemy[];
}

export interface SpellInfo {
  id: string;
  name: string;
  /** Base cooldown in seconds. */
  cooldown: number;
  /** Data Dragon image file name, e.g. "SummonerFlash.png". */
  image: string;
}

/** Static game data for the current patch, from Data Dragon. */
export interface SpellData {
  /** Data Dragon version, or null when running on the bundled fallback. */
  version: string | null;
  spells: Record<string, SpellInfo>;
  /** Item id -> summoner spell haste it grants. */
  itemHaste: Record<string, number>;
}
