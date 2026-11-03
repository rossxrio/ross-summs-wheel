import type { Store } from "../app/store";
import { READY_FLASH_MS, remainingSec } from "../app/timers";
import { championIcon, spellIcon } from "../league/ddragon";
import { normalizeSpellId, ROLE_LABEL, ROLES } from "../league/spells";
import type { Role } from "../league/types";
import { formatTime } from "../utils/format";
import { escapeHtml } from "../utils/html";

/**
 * Click-through tracker strip on the left of the screen.
 *
 * In game it shows every enemy and both spells; outside a game (no live API)
 * only roles with running timers. A small tag shows the enemy's summoner spell
 * haste when they have any.
 */
export function renderHud(root: HTMLElement, store: Store, now = Date.now()): void {
  root.innerHTML = ROLES.map((role) => hudRow(store, role, now)).join("");
}

function hudRow(store: Store, role: Role, now: number): string {
  const enemy = store.roster[role];
  const timed = store.timers
    .all()
    .filter((t) => t.role === role)
    .map((t) => t.spellId);
  const spellIds = enemy ? enemy.spells.map(normalizeSpellId) : timed;
  if (!spellIds.length) return "";

  const haste = store.hasteOf(role);
  const head = enemy
    ? icon(championIcon(store.spellData, enemy.championId), enemy.champion)
    : `<span class="role-tag">${escapeHtml(ROLE_LABEL[role])}</span>`;
  const cells = spellIds.map((id) => spellCell(store, role, id, haste, now)).join("");
  const hasteTag =
    haste > 0
      ? `<span class="haste" title="summoner spell haste">${haste}${store.cosmicInsight.has(role) ? " CI" : ""}</span>`
      : "";
  return `<div class="hud-row">${head}${cells}${hasteTag}</div>`;
}

function spellCell(store: Store, role: Role, spellId: string, haste: number, now: number): string {
  const timer = store.timers.get(role, spellId);
  const left = remainingSec(timer, haste);
  const justUp = timer?.readyAt != null && now - timer.readyAt < READY_FLASH_MS;
  const state = left > 0 ? "cooldown" : justUp ? "ready" : "";
  const name = store.spellData.spells[spellId]?.name ?? spellId;
  return `<div class="spell ${state}" title="${escapeHtml(name)}">
    ${icon(spellIcon(store.spellData, spellId), name)}
    ${left > 0 ? `<span class="time">${formatTime(left)}</span>` : ""}
  </div>`;
}

/** Image when we have one, otherwise the first two letters. */
function icon(src: string | undefined, name: string): string {
  return src
    ? `<img class="icon" src="${escapeHtml(src)}" alt="${escapeHtml(name)}" />`
    : `<span class="icon text">${escapeHtml(name.slice(0, 2))}</span>`;
}
