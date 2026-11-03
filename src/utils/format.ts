/** Seconds as "m:ss", rounded up so a timer never shows 0:00 while still running. */
export function formatTime(seconds: number): string {
  const s = Math.ceil(seconds);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
