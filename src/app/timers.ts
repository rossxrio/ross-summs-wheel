import { withHaste } from "../league/haste";
import type { Role } from "../league/types";

export interface Timer {
  role: Role;
  /** Spell family id (Teleport and Unleashed Teleport share one), see normalizeSpellId. */
  spellId: string;
  /**
   * Cooldown left in *base* (un-hasted) seconds. Each real second removes
   * (100 + haste) / 100 of it, so haste gained mid-cooldown (boots bought)
   * speeds up the rest of the timer, like it does in game.
   */
  baseLeft: number;
  /** Epoch ms when it came back up, or null while still on cooldown. */
  readyAt: number | null;
}

/** How long a spell keeps flashing after it comes back up. */
export const READY_FLASH_MS = 4000;

/** Enemy summoner spell cooldowns, one per role + spell. */
export class Timers {
  private timers = new Map<string, Timer>();

  /** Starts (or restarts, on a re-pick) the cooldown for one enemy spell. */
  start(role: Role, spellId: string, baseCooldownSec: number): void {
    this.timers.set(key(role, spellId), { role, spellId, baseLeft: baseCooldownSec, readyAt: null });
  }

  get(role: Role, spellId: string): Timer | undefined {
    return this.timers.get(key(role, spellId));
  }

  all(): Timer[] {
    return [...this.timers.values()];
  }

  clear(): void {
    this.timers.clear();
  }

  /** Moves every running timer forward by `dtSec` real seconds at its enemy's current haste. */
  advance(dtSec: number, hasteOf: (role: Role) => number, now = Date.now()): void {
    for (const t of this.timers.values()) {
      if (t.readyAt !== null) continue;
      t.baseLeft -= (dtSec * (100 + hasteOf(t.role))) / 100;
      if (t.baseLeft <= 0) {
        t.baseLeft = 0;
        t.readyAt = now;
      }
    }
  }

  /** Drops timers that are up and have finished flashing. */
  prune(now = Date.now()): void {
    for (const [k, t] of this.timers) {
      if (t.readyAt !== null && now - t.readyAt > READY_FLASH_MS) this.timers.delete(k);
    }
  }
}

const key = (role: Role, spellId: string) => `${role}:${spellId}`;

/** Real seconds left at the given haste (0 when up or not tracked). */
export function remainingSec(t: Timer | undefined, haste: number): number {
  return t && t.readyAt === null ? withHaste(t.baseLeft, haste) : 0;
}
