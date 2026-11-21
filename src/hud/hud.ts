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
 *
 * It redraws four times a second for as long as the app runs, so the rows are
 * built only when what they show changes (roster, icons, haste); in between
 * only the cells that changed are touched. Rebuilding every tick would reload
 * the images, restart the "back up" pulse, and churn the DOM all session.
 */
export class Hud {
  private layoutKey: string | null = null;
  private cells: Cell[] = [];

  constructor(private readonly root: HTMLElement) {}

  render(store: Store, now = Date.now()): void {
    const rows = ROLES.map((role) => rowOf(store, role)).filter((row) => row.spellIds.length > 0);
    const layoutKey = JSON.stringify(rows);
    if (layoutKey !== this.layoutKey) {
      this.build(rows);
      this.layoutKey = layoutKey;
    }
    for (const cell of this.cells) cell.update(store, now);
  }

  private build(rows: Row[]): void {
    this.root.innerHTML = rows.map(rowHtml).join("");
    const elements = this.root.querySelectorAll<HTMLElement>(".spell");
    this.cells = rows
      .flatMap((row) => row.spellIds.map((spellId) => ({ role: row.role, spellId, haste: row.haste })))
      .map((c, i) => new Cell(elements[i], c.role, c.spellId, c.haste));
  }
}

/** Everything a row shows apart from the running timers. */
interface Row {
  role: Role;
  /** Champion icon and name, or null for a role-only row (no game). */
  head: { icon: string | undefined; name: string } | null;
  spellIds: string[];
  spells: { icon: string | undefined; name: string }[];
  haste: number;
  cosmicInsight: boolean;
}

function rowOf(store: Store, role: Role): Row {
  const enemy = store.roster[role];
  const spellIds = enemy
    ? enemy.spells.map(normalizeSpellId)
    : store.timers
        .all()
        .filter((t) => t.role === role)
        .map((t) => t.spellId);
  return {
    role,
    head: enemy ? { icon: championIcon(store.spellData, enemy.championId), name: enemy.champion } : null,
    spellIds,
    spells: spellIds.map((id) => ({
      icon: spellIcon(store.spellData, id),
      name: store.spellData.spells[id]?.name ?? id,
    })),
    haste: store.hasteOf(role),
    cosmicInsight: store.cosmicInsight.has(role),
  };
}

function rowHtml(row: Row): string {
  const head = row.head
    ? icon(row.head.icon, row.head.name)
    : `<span class="role-tag">${escapeHtml(ROLE_LABEL[row.role])}</span>`;
  const cells = row.spells
    .map(
      (s) => `<div class="spell" title="${escapeHtml(s.name)}">${icon(s.icon, s.name)}<span class="time"></span></div>`,
    )
    .join("");
  const hasteTag =
    row.haste > 0
      ? `<span class="haste" title="summoner spell haste">${row.haste}${row.cosmicInsight ? " CI" : ""}</span>`
      : "";
  return `<div class="hud-row">${head}${cells}${hasteTag}</div>`;
}

/** One spell's cell: greyed with the time left while on cooldown, pulsing just after. */
class Cell {
  private state = "";
  private text = "";
  private readonly time: HTMLElement;

  constructor(
    private readonly el: HTMLElement,
    private readonly role: Role,
    private readonly spellId: string,
    private readonly haste: number,
  ) {
    this.time = el.querySelector(".time")!;
  }

  update(store: Store, now: number): void {
    const timer = store.timers.get(this.role, this.spellId);
    const left = remainingSec(timer, this.haste);
    const justUp = timer?.readyAt != null && now - timer.readyAt < READY_FLASH_MS;
    const state = left > 0 ? "cooldown" : justUp ? "ready" : "";
    const text = left > 0 ? formatTime(left) : "";
    // Only on change: the same class or text written again still costs a style pass.
    if (state !== this.state) {
      this.el.className = state ? `spell ${state}` : "spell";
      this.state = state;
    }
    if (text !== this.text) {
      this.time.textContent = text;
      this.text = text;
    }
  }
}

/** Image when we have one, otherwise the first two letters. */
function icon(src: string | undefined, name: string): string {
  return src
    ? `<img class="icon" src="${escapeHtml(src)}" alt="${escapeHtml(name)}" />`
    : `<span class="icon text">${escapeHtml(name.slice(0, 2))}</span>`;
}
