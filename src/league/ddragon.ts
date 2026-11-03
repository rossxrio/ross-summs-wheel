import type { SpellData } from "./types";

/** Image URLs on Riot's Data Dragon CDN. Undefined when running on fallback data (no version). */

const CDN = "https://ddragon.leagueoflegends.com/cdn";

export function spellIcon(data: SpellData, spellId: string): string | undefined {
  const info = data.spells[spellId];
  return data.version && info ? `${CDN}/${data.version}/img/spell/${info.image}` : undefined;
}

export function championIcon(data: SpellData, championId: string): string | undefined {
  return data.version && championId ? `${CDN}/${data.version}/img/champion/${championId}.png` : undefined;
}
