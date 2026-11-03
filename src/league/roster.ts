import { normalizeSpellId, ROLES } from "./spells";
import type { Enemy, Role } from "./types";

/** The enemy team by role. A role can be empty (no game, or fewer than five enemies). */
export type Roster = Partial<Record<Role, Enemy>>;

/**
 * Map enemies onto roles. Ranked and draft report positions. Blind, ARAM and
 * custom games report an empty position, so we guess: the smite holder is the
 * jungler and everyone else fills the remaining roles in API order.
 */
export function assignRoles(enemies: Enemy[]): Roster {
  const roster: Roster = {};
  const unplaced: Enemy[] = [];

  for (const enemy of enemies) {
    if (enemy.role && ROLES.includes(enemy.role) && !roster[enemy.role]) {
      roster[enemy.role] = enemy;
    } else {
      unplaced.push(enemy);
    }
  }

  if (!roster.JUNGLE) {
    const i = unplaced.findIndex((e) => e.spells.some((s) => normalizeSpellId(s) === "SummonerSmite"));
    if (i >= 0) roster.JUNGLE = unplaced.splice(i, 1)[0];
  }

  for (const role of ROLES) {
    if (!roster[role] && unplaced.length) roster[role] = unplaced.shift();
  }
  return roster;
}

/** Identifies a match by its enemy champions, so we can tell when a new game starts. */
export function matchKey(enemies: Enemy[]): string {
  return enemies
    .map((e) => e.championId)
    .sort()
    .join(",");
}
