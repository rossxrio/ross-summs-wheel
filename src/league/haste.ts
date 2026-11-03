import { normalizeSpellId } from "./spells";
import type { Enemy, SpellData } from "./types";

/**
 * Summoner spell haste sources (League wiki, checked 2026-11, patch 16.21):
 *   Ionian Boots of Lucidity 10, Crimson Lucidity 20: read from Data Dragon item text.
 *   Cosmic Insight rune 18: NOT visible for enemies via the live API (only rune trees are), so it's a manual toggle.
 *   Howling Abyss (ARAM) aura 70.
 * cooldown = base × 100 / (100 + haste)
 */
export const COSMIC_INSIGHT_HASTE = 18;
const HOWLING_ABYSS_HASTE = 70;
const HOWLING_ABYSS_MAP = 12;
/** Rune tree id of Inspiration, the tree Cosmic Insight is in. */
export const INSPIRATION_TREE = 8300;

export interface HasteContext {
  itemHaste: SpellData["itemHaste"];
  mapNumber: number | null;
}

export function enemyHaste(enemy: Enemy | undefined, cosmicInsight: boolean, ctx: HasteContext): number {
  let haste = cosmicInsight ? COSMIC_INSIGHT_HASTE : 0;
  if (ctx.mapNumber === HOWLING_ABYSS_MAP) haste += HOWLING_ABYSS_HASTE;
  // Unique passives don't stack, so count each item once.
  for (const id of new Set(enemy?.items ?? [])) haste += ctx.itemHaste[id] ?? 0;
  return haste;
}

/** Real seconds for `seconds` of base cooldown at the given haste. */
export const withHaste = (seconds: number, haste: number) => (seconds * 100) / (100 + haste);

export const isUnleashedTeleport = (rawId: string) => rawId.includes("SummonerTeleportUpgrade");

/**
 * Base (un-hasted) cooldown in seconds. `rawId` is the id from the live API,
 * which tells Teleport and Unleashed Teleport apart.
 */
export function baseCooldown(rawId: string, data: SpellData, level: number): number {
  // Teleport becomes Unleashed Teleport at 10:00: 330s at level 1 down to 240s at 18.
  if (isUnleashedTeleport(rawId)) {
    const lvl = Math.min(18, Math.max(1, level || 1));
    return 330 - (90 * (lvl - 1)) / 17;
  }
  const id = normalizeSpellId(rawId);
  // Data Dragon lists Smite's 15s gap between casts; the charge recharge is 90s.
  if (id === "SummonerSmite") return 90;
  return data.spells[id]?.cooldown ?? 300;
}
