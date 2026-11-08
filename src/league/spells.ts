import type { Role, SpellData, SpellInfo } from "./types";

/** Order of the role wheel, clockwise from the top. Bot and support sit next to each other. */
export const ROLES: Role[] = ["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY"];

export const ROLE_LABEL: Record<Role, string> = {
  TOP: "Top",
  JUNGLE: "Jungle",
  MIDDLE: "Mid",
  BOTTOM: "ADC",
  UTILITY: "Support",
};

/**
 * Used only when Data Dragon can't be reached. Data Dragon is the source of
 * truth because cooldowns change between patches.
 */
const FALLBACK_SPELLS: SpellInfo[] = [
  { id: "SummonerFlash", name: "Flash", cooldown: 300, image: "SummonerFlash.png" },
  { id: "SummonerTeleport", name: "Teleport", cooldown: 300, image: "SummonerTeleport.png" },
  { id: "SummonerDot", name: "Ignite", cooldown: 180, image: "SummonerDot.png" },
  { id: "SummonerHeal", name: "Heal", cooldown: 240, image: "SummonerHeal.png" },
  { id: "SummonerBarrier", name: "Barrier", cooldown: 180, image: "SummonerBarrier.png" },
  { id: "SummonerExhaust", name: "Exhaust", cooldown: 240, image: "SummonerExhaust.png" },
  { id: "SummonerBoost", name: "Cleanse", cooldown: 240, image: "SummonerBoost.png" },
  { id: "SummonerHaste", name: "Ghost", cooldown: 240, image: "SummonerHaste.png" },
  { id: "SummonerSmite", name: "Smite", cooldown: 90, image: "SummonerSmite.png" },
];

/** Spells offered on the wheel when we don't know what the enemy took. */
export const ALL_SPELL_IDS = FALLBACK_SPELLS.map((s) => s.id);

/** Spell names for pickers that can't wait for Data Dragon (settings page). */
export const SPELL_NAMES: { id: string; name: string }[] = FALLBACK_SPELLS.map(({ id, name }) => ({ id, name }));

export const fallbackSpellData = (): SpellData => ({
  version: null,
  spells: Object.fromEntries(FALLBACK_SPELLS.map((s) => [s.id, s])),
  // Ionian Boots of Lucidity, Crimson Lucidity.
  itemHaste: { "3158": 10, "3171": 20 },
});

/**
 * The live API reports upgraded variants such as "S12_SummonerTeleportUpgrade"
 * (Unleashed Teleport, from 10:00) or smite upgrades. Collapse them onto the
 * base spell id, so a timer survives the upgrade. Cooldowns use the raw id
 * (see baseCooldown in haste.ts).
 */
export function normalizeSpellId(raw: string): string {
  if (ALL_SPELL_IDS.includes(raw)) return raw;
  return ALL_SPELL_IDS.find((id) => raw.includes(id)) ?? raw;
}
