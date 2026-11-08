import { baseCooldown, enemyHaste } from "../league/haste";
import { assignRoles, matchKey, type Roster } from "../league/roster";
import { fallbackSpellData, normalizeSpellId } from "../league/spells";
import type { GameState, Role, SpellData } from "../league/types";
import { Timers } from "./timers";

/** Everything the app knows about the current game, plus the user's timers and toggles. */
export class Store {
  spellData: SpellData = fallbackSpellData();
  roster: Roster = {};
  readonly timers = new Timers();
  /** Enemies the user marked as running Cosmic Insight (not visible through the API). */
  readonly cosmicInsight = new Set<Role>();
  private mapNumber: number | null = null;
  private currentMatch = "";

  /** Data Dragon data, keeping fallback entries for anything it doesn't list. */
  setSpellData(live: SpellData): void {
    const fallback = fallbackSpellData();
    this.spellData = {
      version: live.version,
      spells: { ...fallback.spells, ...live.spells },
      itemHaste: { ...fallback.itemHaste, ...live.itemHaste },
    };
  }

  applyGameState(state: GameState): void {
    this.roster = assignRoles(state.enemies);
    this.mapNumber = state.mapNumber;
    // A different set of enemy champions means a new match: old timers and toggles are meaningless.
    const match = matchKey(state.enemies);
    if (match && match !== this.currentMatch) {
      this.timers.clear();
      this.cosmicInsight.clear();
    }
    if (match) this.currentMatch = match;
  }

  /** Current summoner spell haste of the enemy in this role. */
  hasteOf = (role: Role): number =>
    enemyHaste(this.roster[role], this.cosmicInsight.has(role), {
      itemHaste: this.spellData.itemHaste,
      mapNumber: this.mapNumber,
    });

  /** The live API's id for this spell family (e.g. Unleashed Teleport after 10:00). */
  rawSpellId(role: Role, spellId: string): string {
    return this.roster[role]?.spells.find((raw) => normalizeSpellId(raw) === spellId) ?? spellId;
  }

  baseCooldownOf(role: Role, spellId: string): number {
    return baseCooldown(this.rawSpellId(role, spellId), this.spellData, this.roster[role]?.level ?? 1);
  }

  toggleCosmicInsight(role: Role): void {
    if (!this.cosmicInsight.delete(role)) this.cosmicInsight.add(role);
  }

  /**
   * Starts from the base cooldown: timers apply the enemy's haste as they run.
   * `agoSec` backdates it, for a spell called out a moment ago.
   */
  startTimer(role: Role, spellId: string, agoSec = 0): void {
    this.timers.start(role, spellId, this.baseCooldownOf(role, spellId), agoSec, this.hasteOf(role));
  }

  /** Moves time forward by `dtSec` real seconds. */
  advance(dtSec: number): void {
    this.timers.advance(dtSec, this.hasteOf);
    this.timers.prune();
  }
}
