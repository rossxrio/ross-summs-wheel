import type { GameState, SpellData } from "../league/types";
import type { Backend } from "./backend";

/**
 * Browser stand-in for the Rust backend, so the wheel and HUD can be tried
 * with `npm run dev` in a normal browser:
 *   - hold the backtick key instead of the global hotkey,
 *   - a fake mid-game enemy team,
 *   - Data Dragon fetched directly from the browser.
 */
export function createPreviewBackend(): Backend {
  document.body.classList.add("preview");
  return {
    getSpellData: fetchSpellData,
    getGameState: async () => PREVIEW_GAME,
    // No overlay window to switch, and the cursor stays where it is.
    setWheelMode: async () => {},

    async onHotkey(handler) {
      window.addEventListener("keydown", (e) => {
        if (e.code === "Backquote" && !e.repeat) handler(true);
      });
      window.addEventListener("keyup", (e) => {
        if (e.code === "Backquote") handler(false);
      });
    },

    // The fake game never changes.
    async onGameState() {},
  };
}

const PRECISION = 8000;
const DOMINATION = 8100;
const SORCERY = 8200;
const INSPIRATION = 8300;
const RESOLVE = 8400;

/**
 * Darius has Unleashed Teleport, Jinx Ionian Boots (10 haste), Thresh
 * Crimson Lucidity (20), Ahri runs the Inspiration tree.
 */
const PREVIEW_GAME: GameState = {
  inGame: true,
  mapNumber: 11,
  enemies: [
    {
      role: "TOP",
      champion: "Darius",
      championId: "Darius",
      spells: ["SummonerFlash", "S12_SummonerTeleportUpgrade"],
      level: 12,
      items: [3047],
      runeTrees: [PRECISION, RESOLVE],
    },
    {
      role: "JUNGLE",
      champion: "Lee Sin",
      championId: "LeeSin",
      spells: ["SummonerFlash", "SummonerSmite"],
      level: 11,
      items: [3111],
      runeTrees: [PRECISION, DOMINATION],
    },
    {
      role: "MIDDLE",
      champion: "Ahri",
      championId: "Ahri",
      spells: ["SummonerFlash", "SummonerDot"],
      level: 12,
      items: [3020],
      runeTrees: [DOMINATION, INSPIRATION],
    },
    {
      role: "BOTTOM",
      champion: "Jinx",
      championId: "Jinx",
      spells: ["SummonerFlash", "SummonerHeal"],
      level: 10,
      items: [3006, 3158],
      runeTrees: [PRECISION, SORCERY],
    },
    {
      role: "UTILITY",
      champion: "Thresh",
      championId: "Thresh",
      spells: ["SummonerFlash", "SummonerExhaust"],
      level: 9,
      items: [3171],
      runeTrees: [RESOLVE, INSPIRATION],
    },
  ],
};

/** Same data the Rust side builds (src-tauri/src/riot/ddragon.rs), fetched from the browser. */
async function fetchSpellData(): Promise<SpellData> {
  const base = "https://ddragon.leagueoflegends.com";
  const getJson = async (path: string) => (await fetch(`${base}${path}`)).json();

  const [version] = (await getJson("/api/versions.json")) as string[];

  type RawSpell = { id: string; name: string; cooldown: number[]; image: { full: string } };
  const summoners = (await getJson(`/cdn/${version}/data/en_US/summoner.json`)).data as Record<string, RawSpell>;
  const spells = Object.fromEntries(
    Object.values(summoners).map((s) => [
      s.id,
      { id: s.id, name: s.name, cooldown: s.cooldown[0], image: s.image.full },
    ]),
  );

  const items = (await getJson(`/cdn/${version}/data/en_US/item.json`)).data as Record<string, { description: string }>;
  const itemHaste: Record<string, number> = {};
  for (const [id, item] of Object.entries(items)) {
    const match = item.description.replace(/<[^>]*>/g, "").match(/([\d.]+)\s*Summoner Spell Haste/);
    if (match) itemHaste[id] = Number(match[1]);
  }

  return { version, spells, itemHaste };
}
