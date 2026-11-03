/** A single slice on the wheel. */
export interface WheelItem {
  key: string;
  label: string;
  icon?: string;
  /** Extra line under the label, e.g. a champion name or a running timer. */
  sub?: string;
}

/** Everything the renderer needs to draw one frame of the wheel. */
export interface WheelView {
  title: string;
  hint: string;
  items: WheelItem[];
  selected: number | null;
  /** Virtual stick position, each axis in -1..1. */
  stick: { x: number; y: number };
}

/**
 * How the overlay window should treat the mouse:
 *   open: take the mouse and move the cursor to the screen center.
 *   recenter: cursor back to the center (role wheel -> spell wheel).
 *   close: give the mouse back to the game and restore the cursor.
 */
export type WheelMode = "open" | "recenter" | "close";
