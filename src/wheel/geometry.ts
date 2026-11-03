/**
 * Wheel math. Angles are in radians: 0 is straight up, increasing clockwise,
 * matching how the slices are laid out on screen.
 */

export const TAU = 2 * Math.PI;

/** Remainder that is always positive, for wrapping angles. */
export const mod = (a: number, m: number) => ((a % m) + m) % m;

/**
 * Angle of slice 0's center. Two slices sit left and right, which is easier
 * to flick to than up and down.
 */
export const wheelRotation = (n: number) => (n === 2 ? -Math.PI / 2 : 0);

export const sliceCenter = (i: number, n: number) => wheelRotation(n) + (i * TAU) / n;

/** Direction of a stick/mouse vector (screen coordinates, y down). */
export const stickAngle = (dx: number, dy: number) => Math.atan2(dx, -dy);

/** Signed shortest difference a - b, in -π..π. */
export const angleDiff = (a: number, b: number) => mod(a - b + Math.PI, TAU) - Math.PI;

/** GTA-style selection: the slice is chosen by stick direction, not by hovering it. */
export function sliceAt(dx: number, dy: number, n: number): number {
  const step = TAU / n;
  const a = mod(stickAngle(dx, dy) - wheelRotation(n) + step / 2, TAU);
  return Math.floor(a / step) % n;
}

/** Point at radius `r` and angle `a` around (`center`, `center`). */
export function polar(center: number, r: number, a: number): [number, number] {
  return [center + r * Math.sin(a), center - r * Math.cos(a)];
}

/** SVG path for a ring segment between radii r0..r1 and angles a0..a1. */
export function ringSegment(center: number, r0: number, r1: number, a0: number, a1: number): string {
  const large = a1 - a0 > Math.PI ? 1 : 0;
  const [x0, y0] = polar(center, r1, a0);
  const [x1, y1] = polar(center, r1, a1);
  const [x2, y2] = polar(center, r0, a1);
  const [x3, y3] = polar(center, r0, a0);
  return `M${x0} ${y0} A${r1} ${r1} 0 ${large} 1 ${x1} ${y1} L${x2} ${y2} A${r0} ${r0} 0 ${large} 0 ${x3} ${y3}Z`;
}
