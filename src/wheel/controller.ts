import type { Role } from "../league/types";
import { angleDiff, sliceAt, sliceCenter, stickAngle } from "./geometry";
import type { WheelItem, WheelMode, WheelView } from "./types";

// ---- Tuning ----------------------------------------------------------------

/**
 * Mouse movement feeds a virtual stick capped at this length (CSS px). Because
 * it's capped, changing slices only ever takes a short flick, the same as a
 * GTA weapon wheel on a mouse.
 */
const STICK_RADIUS = 120;
/** Stick length below which nothing is selected (releasing there cancels). */
const DEAD_ZONE = 28;
/** Once selected, a slice stays selected until the stick is this far (rad, ~8°) past its edge. */
const STICKINESS = 0.14;
/** A single mouse event moving further than this (px) is the cursor being re-centered, not a flick. */
const MAX_STEP = 250;
/** The spell wheel stays open without holding the key, so close it if left alone. */
const SPELL_TIMEOUT_MS = 6000;
/** Touchpads send a burst of wheel events per gesture: one toggle per burst. */
const SCROLL_COOLDOWN_MS = 300;

// ---- Types -----------------------------------------------------------------

type Stage = { kind: "idle" } | { kind: "role" } | { kind: "spell"; role: Role };
type Point = { x: number; y: number };

/** What the controller needs from the rest of the app. */
export interface WheelDeps {
  /** Switch the overlay's mouse handling (see WheelMode). */
  setMode(mode: WheelMode): Promise<void>;
  roleItems(): WheelItem[];
  spellItems(role: Role): WheelItem[];
  spellTitle(role: Role): string;
  /** Hub text on the spell wheel while nothing is selected. */
  spellHint(role: Role): string;
  /** Cosmic Insight can't be read from the API, so the user flips it per enemy. */
  toggleCosmicInsight(role: Role): void;
  commit(role: Role, spellId: string): void;
  render(view: WheelView | null): void;
}

// ---- Controller ------------------------------------------------------------

/**
 * Turns hotkey, mouse and scroll input into wheel selections.
 *
 * The main loop:
 *   IDLE  --hotkey down-->  ROLE  --hotkey up on a role-->  SPELL  --click / hotkey down-->  commit, IDLE
 * Releasing or clicking with nothing selected (inside the dead zone) cancels,
 * as does a right click or the spell timeout. Scrolling on the spell wheel
 * toggles Cosmic Insight for that enemy.
 */
export class WheelController {
  private stage: Stage = { kind: "idle" };
  private selected: number | null = null;
  private stick: Point = { x: 0, y: 0 };
  /** Last cursor position seen while the wheel was open; null after opening or re-centering. */
  private last: Point | null = null;
  private timeout: number | undefined;
  private lastScroll = 0;
  /** Bumped on every stage change so a late setMode() reply can't touch a newer stage. */
  private generation = 0;

  constructor(private deps: WheelDeps) {}

  get isOpen(): boolean {
    return this.stage.kind !== "idle";
  }

  // -- Input ------------------------------------------------------------------

  hotkey(pressed: boolean): void {
    const { stage } = this;
    if (stage.kind === "idle" && pressed) {
      this.openRoles();
    } else if (stage.kind === "role" && !pressed) {
      const role = this.selectedItem()?.key as Role | undefined;
      if (role) this.openSpells(role);
      else this.close();
    } else if (stage.kind === "spell" && pressed) {
      this.commit(stage.role);
    }
  }

  pointer(x: number, y: number): void {
    if (!this.isOpen) return;
    const prev = this.last;
    this.last = { x, y };
    // No usable previous position right after opening or a re-center.
    if (!prev) return;

    const dx = x - prev.x;
    const dy = y - prev.y;
    if (Math.hypot(dx, dy) > MAX_STEP) return;

    let sx = this.stick.x + dx;
    let sy = this.stick.y + dy;
    const len = Math.hypot(sx, sy);
    if (len > STICK_RADIUS) {
      sx *= STICK_RADIUS / len;
      sy *= STICK_RADIUS / len;
    }
    this.stick = { x: sx, y: sy };
    this.selected = this.pick();
    this.render();
  }

  click(button: number): void {
    const { stage } = this;
    if (stage.kind === "idle") return;
    if (button === 2) this.close();
    else if (button === 0 && stage.kind === "spell") this.commit(stage.role);
  }

  scroll(deltaY: number): void {
    const { stage } = this;
    if (stage.kind !== "spell" || deltaY === 0) return;
    const now = performance.now();
    if (now - this.lastScroll < SCROLL_COOLDOWN_MS) return;
    this.lastScroll = now;
    this.deps.toggleCosmicInsight(stage.role);
    this.armSpellTimeout(); // the user is still busy with this wheel
    this.render();
  }

  /** Closes the wheel without picking anything (a quick bind did the job). */
  dismiss(): void {
    if (this.isOpen) this.close();
  }

  /** Re-render with fresh data (e.g. ticking timers) while open. */
  refresh(): void {
    if (this.isOpen) this.render();
  }

  // -- Selection --------------------------------------------------------------

  /** Slice under the stick, with stickiness so it doesn't flicker on a slice edge. */
  private pick(): number | null {
    const { x, y } = this.stick;
    const len = Math.hypot(x, y);
    const current = this.selected;
    // Slight radial stickiness too: easier to enter the dead zone on purpose than by drift.
    if (len < (current === null ? DEAD_ZONE : DEAD_ZONE * 0.75)) return null;

    const n = this.items().length;
    const next = sliceAt(x, y, n);
    if (current === null || next === current) return next;
    const offCenter = Math.abs(angleDiff(stickAngle(x, y), sliceCenter(current, n)));
    return offCenter < Math.PI / n + STICKINESS ? current : next;
  }

  // -- Stage changes ----------------------------------------------------------

  private openRoles(): void {
    this.stage = { kind: "role" };
    this.resetStick();
    this.render();
    this.changeMode("open");
  }

  private openSpells(role: Role): void {
    this.stage = { kind: "spell", role };
    // Start the spell pick from the center so the role flick doesn't carry over.
    this.resetStick();
    this.armSpellTimeout();
    this.render();
    this.changeMode("recenter");
  }

  private commit(role: Role): void {
    const spellId = this.selectedItem()?.key;
    if (spellId) this.deps.commit(role, spellId);
    this.close();
  }

  private close(): void {
    window.clearTimeout(this.timeout);
    this.stage = { kind: "idle" };
    this.resetStick();
    this.deps.render(null);
    this.changeMode("close");
  }

  private armSpellTimeout(): void {
    window.clearTimeout(this.timeout);
    this.timeout = window.setTimeout(() => this.close(), SPELL_TIMEOUT_MS);
  }

  private changeMode(mode: WheelMode): void {
    const gen = ++this.generation;
    this.deps.setMode(mode).then(
      // The cursor just jumped: measure movement from wherever it landed.
      () => {
        if (gen === this.generation) this.last = null;
      },
      (err) => console.warn(`setMode(${mode}) failed:`, err),
    );
  }

  private resetStick(): void {
    this.stick = { x: 0, y: 0 };
    this.selected = null;
    this.last = null;
  }

  // -- Rendering --------------------------------------------------------------

  private items(): WheelItem[] {
    const { stage } = this;
    if (stage.kind === "role") return this.deps.roleItems();
    if (stage.kind === "spell") return this.deps.spellItems(stage.role);
    return [];
  }

  private selectedItem(): WheelItem | undefined {
    return this.selected === null ? undefined : this.items()[this.selected];
  }

  private render(): void {
    const { stage } = this;
    if (stage.kind === "idle") return;
    this.deps.render({
      title: stage.kind === "role" ? "Enemy role" : this.deps.spellTitle(stage.role),
      hint: stage.kind === "role" ? "release on a role" : this.deps.spellHint(stage.role),
      items: this.items(),
      selected: this.selected,
      stick: { x: this.stick.x / STICK_RADIUS, y: this.stick.y / STICK_RADIUS },
    });
  }
}
