import { escapeHtml } from "../utils/html";
import { angleDiff, polar, ringSegment, sliceCenter, TAU } from "./geometry";
import type { WheelItem, WheelView } from "./types";

// Layout, in SVG units.
const SIZE = 520;
const CENTER = SIZE / 2;
const R_INNER = 84;
const R_OUTER = 236;
const GAP = 0.025; // radians trimmed off each side of a slice
const ICON = 58;
/** How far the stick dot can travel inside the hub. */
const STICK_TRAVEL = R_INNER - 22;

// Easing time constants (ms): smaller = snappier.
const STICK_TAU = 35;
const SELECTOR_TAU = 45;
const FADE_TAU = 40;

/**
 * Draws the wheel as SVG.
 *
 * The SVG is built once per set of slices and patched in place after that:
 * rebuilding would replay the open animation (flicker).
 *
 * Selection is instant in the controller. Only the visuals are eased here:
 * the selector wedge glides to the chosen slice and the stick dot trails the
 * mouse slightly, so the wheel moves smoothly instead of jumping.
 */
export class WheelRenderer {
  private layoutKey = "";
  private sliceCount = 0;
  private slices: SVGGElement[] = [];
  private subs: SVGTextElement[] = [];
  private selector!: SVGPathElement;
  private hubTitle!: SVGTextElement;
  private hubHint!: SVGTextElement;
  private stickDot!: SVGCircleElement;

  // Target values come from render(); shown values chase them in the frame loop.
  private target = { x: 0, y: 0, angle: 0, opacity: 0 };
  private shown = { x: 0, y: 0, angle: 0, opacity: 0 };
  private raf = 0;
  private lastFrame = 0;

  constructor(private root: HTMLElement) {}

  render(view: WheelView | null): void {
    if (!view) return this.hide();

    const layoutKey = view.items.map((i) => `${i.key}:${i.icon ?? ""}`).join("|");
    if (layoutKey !== this.layoutKey) {
      this.build(view.items);
      this.layoutKey = layoutKey;
      this.shown = { x: view.stick.x, y: view.stick.y, angle: 0, opacity: 0 };
    }
    this.root.hidden = false;

    this.updateText(view);
    this.updateTargets(view);
    this.startAnimating();
  }

  private hide(): void {
    cancelAnimationFrame(this.raf);
    this.raf = 0;
    this.root.hidden = true;
    this.root.innerHTML = "";
    this.layoutKey = "";
  }

  private updateText(view: WheelView): void {
    const selected = view.selected !== null ? view.items[view.selected] : null;
    this.slices.forEach((g, i) => g.classList.toggle("selected", i === view.selected));
    view.items.forEach((item, i) => (this.subs[i].textContent = item.sub ?? ""));
    this.hubTitle.textContent = selected?.label ?? view.title;
    this.hubHint.textContent = selected ? (selected.sub ?? "") : view.hint;
    this.stickDot.classList.toggle("active", selected !== null);
  }

  private updateTargets(view: WheelView): void {
    this.target.x = view.stick.x;
    this.target.y = view.stick.y;
    this.target.opacity = view.selected !== null ? 1 : 0;
    if (view.selected !== null) {
      const angle = sliceCenter(view.selected, this.sliceCount);
      // Appearing from nothing: start on the slice instead of sweeping in from the last one.
      if (this.shown.opacity < 0.05) this.shown.angle = angle;
      this.target.angle = angle;
    }
  }

  private startAnimating(): void {
    if (this.raf) return;
    this.lastFrame = performance.now();
    this.raf = requestAnimationFrame(this.frame);
  }

  private frame = (now: number): void => {
    const dt = Math.min(now - this.lastFrame, 50);
    this.lastFrame = now;
    const ease = (tau: number) => 1 - Math.exp(-dt / tau);
    const { shown, target } = this;

    shown.x += (target.x - shown.x) * ease(STICK_TAU);
    shown.y += (target.y - shown.y) * ease(STICK_TAU);
    const turn = angleDiff(target.angle, shown.angle);
    shown.angle += turn * ease(SELECTOR_TAU);
    shown.opacity += (target.opacity - shown.opacity) * ease(FADE_TAU);

    this.stickDot.setAttribute("cx", (CENTER + shown.x * STICK_TRAVEL).toFixed(2));
    this.stickDot.setAttribute("cy", (CENTER + shown.y * STICK_TRAVEL).toFixed(2));
    const degrees = ((shown.angle * 180) / Math.PI).toFixed(2);
    this.selector.setAttribute("transform", `rotate(${degrees} ${CENTER} ${CENTER})`);
    this.selector.style.opacity = shown.opacity.toFixed(3);

    const settled =
      Math.abs(target.x - shown.x) < 1e-3 &&
      Math.abs(target.y - shown.y) < 1e-3 &&
      Math.abs(turn) < 1e-3 &&
      Math.abs(target.opacity - shown.opacity) < 1e-3;
    // Go idle when nothing is moving; render() restarts the loop.
    this.raf = settled ? 0 : requestAnimationFrame(this.frame);
  };

  private build(items: WheelItem[]): void {
    const n = (this.sliceCount = items.length);
    const step = TAU / n;
    const segment = (r0: number, r1: number, mid: number) =>
      ringSegment(CENTER, r0, r1, mid - step / 2 + GAP, mid + step / 2 - GAP);

    const bases = items
      .map((_, i) => `<path class="slice-base" d="${segment(R_INNER, R_OUTER, sliceCenter(i, n))}" />`)
      .join("");
    const contents = items.map((item, i) => sliceContent(item, sliceCenter(i, n))).join("");

    this.root.innerHTML = `
      <svg viewBox="0 0 ${SIZE} ${SIZE}" width="${SIZE}" height="${SIZE}">
        ${bases}
        <path class="selector" d="${segment(R_INNER - 4, R_OUTER + 8, 0)}" style="opacity:0" />
        ${contents}
        <circle class="hub" cx="${CENTER}" cy="${CENTER}" r="${R_INNER - 8}" />
        <circle class="stick-dot" cx="${CENTER}" cy="${CENTER}" r="6" />
        <text class="hub-title" x="${CENTER}" y="${CENTER - 8}"></text>
        <text class="hub-hint" x="${CENTER}" y="${CENTER + 16}"></text>
      </svg>`;

    this.slices = [...this.root.querySelectorAll<SVGGElement>(".slice")];
    this.subs = [...this.root.querySelectorAll<SVGTextElement>(".slice .sub")];
    this.selector = this.root.querySelector(".selector")!;
    this.hubTitle = this.root.querySelector(".hub-title")!;
    this.hubHint = this.root.querySelector(".hub-hint")!;
    this.stickDot = this.root.querySelector(".stick-dot")!;
  }
}

/** Icon (or a text glyph), label and sub line for one slice, centered on angle `mid`. */
function sliceContent(item: WheelItem, mid: number): string {
  const [x, y] = polar(CENTER, (R_INNER + R_OUTER) / 2, mid);
  const glyph = item.icon
    ? `<image href="${escapeHtml(item.icon)}" x="${x - ICON / 2}" y="${y - ICON / 2 - 10}" width="${ICON}" height="${ICON}" clip-path="inset(0 round 10px)" />`
    : `<text class="glyph" x="${x}" y="${y - 2}">${escapeHtml(item.label.slice(0, 3))}</text>`;
  return `<g class="slice">
    ${glyph}
    <text class="label" x="${x}" y="${y + ICON / 2 + 8}">${escapeHtml(item.label)}</text>
    <text class="sub" x="${x}" y="${y + ICON / 2 + 26}"></text>
  </g>`;
}
