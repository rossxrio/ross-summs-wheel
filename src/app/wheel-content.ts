import { championIcon, spellIcon } from "../league/ddragon";
import { INSPIRATION_TREE, isUnleashedTeleport, withHaste } from "../league/haste";
import { ALL_SPELL_IDS, normalizeSpellId, ROLE_LABEL, ROLES } from "../league/spells";
import type { Role } from "../league/types";
import { formatTime } from "../utils/format";
import type { WheelItem } from "../wheel/types";
import type { Store } from "./store";
import { remainingSec } from "./timers";

/** What the wheel shows, built from the current store. */

/** Role wheel: one slice per role, with the enemy champion when we know it. */
export function roleItems(store: Store): WheelItem[] {
  return ROLES.map((role) => {
    const enemy = store.roster[role];
    return {
      key: role,
      label: ROLE_LABEL[role],
      icon: enemy ? championIcon(store.spellData, enemy.championId) : undefined,
      sub: enemy?.champion,
    };
  });
}

/** Spell wheel: the enemy's two spells in a live game, otherwise every spell. */
export function spellItems(store: Store, role: Role): WheelItem[] {
  const ids = store.roster[role]?.spells.map(normalizeSpellId) ?? ALL_SPELL_IDS;
  return ids.map((id) => spellItem(store, role, id));
}

function spellItem(store: Store, role: Role, spellId: string): WheelItem {
  const raw = store.rawSpellId(role, spellId);
  const haste = store.hasteOf(role);
  const left = remainingSec(store.timers.get(role, spellId), haste);
  const cooldown = withHaste(store.baseCooldownOf(role, spellId), haste);
  return {
    key: spellId,
    label: isUnleashedTeleport(raw) ? "Unleashed TP" : (store.spellData.spells[spellId]?.name ?? spellId),
    icon: spellIcon(store.spellData, spellId),
    sub: left > 0 ? `${formatTime(left)} left · repick resets` : `${formatTime(cooldown)} cooldown`,
  };
}

export function spellTitle(store: Store, role: Role): string {
  return store.roster[role]?.champion ?? ROLE_LABEL[role];
}

/** Hub text on the spell wheel: the Cosmic Insight toggle, flagged when the enemy runs Inspiration. */
export function spellHint(store: Store, role: Role): string {
  const toggle = `scroll: Cosmic Insight ${store.cosmicInsight.has(role) ? "ON" : "off"}`;
  return store.roster[role]?.runeTrees.includes(INSPIRATION_TREE) ? `Inspiration · ${toggle}` : toggle;
}
