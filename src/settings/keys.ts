/**
 * Hotkey strings as stored in config.json and parsed by the Rust side
 * (src-tauri/src/key_watch/button.rs): modifiers then one W3C key code,
 * joined by "+", e.g. "Backquote", "Ctrl+Alt+KeyQ", "F13". The mouse side
 * buttons are "Mouse4" and "Mouse5".
 */

/** The modifier state any keyboard or mouse event carries. */
export type ModifierState = Pick<KeyboardEvent, "ctrlKey" | "shiftKey" | "altKey" | "metaKey">;

/** Modifiers in the order they're written. */
const MODIFIERS = [
  { token: "Ctrl", label: "Ctrl", held: (e: ModifierState) => e.ctrlKey },
  { token: "Shift", label: "Shift", held: (e: ModifierState) => e.shiftKey },
  { token: "Alt", label: "Alt", held: (e: ModifierState) => e.altKey },
  { token: "Super", label: "Win", held: (e: ModifierState) => e.metaKey },
];

/** `MouseEvent.button` for the side buttons: 3 is back (Mouse 4), 4 is forward (Mouse 5). */
const MOUSE_CODES: Record<number, string> = { 3: "Mouse4", 4: "Mouse5" };

/** The code for a mouse side button, or undefined for any other button. */
export function mouseCode(button: number): string | undefined {
  return MOUSE_CODES[button];
}

const MODIFIER_CODES = /^(Control|Alt|Meta|OS|Shift)(Left|Right)$/;

export function isModifier(code: string): boolean {
  return MODIFIER_CODES.test(code);
}

/** The key code of a hotkey's main key, e.g. "Ctrl+Q" -> "KeyQ". */
export function mainKeyCode(hotkey: string): string {
  const key = hotkey.split("+").pop()!.trim();
  if (/^[a-z]$/i.test(key)) return `Key${key.toUpperCase()}`;
  if (/^\d$/.test(key)) return `Digit${key}`;
  return key === "`" ? "Backquote" : key;
}

/** Labels of the modifiers held during `e`. */
export function heldModifiers(e: ModifierState): string[] {
  return MODIFIERS.filter((m) => m.held(e)).map((m) => m.label);
}

/** The hotkey string for a key or side button pressed with `e`'s modifiers. Not for modifier keys. */
export function hotkeyFrom(code: string, e: ModifierState): string {
  const mods = MODIFIERS.filter((m) => m.held(e)).map((m) => m.token);
  return [...mods, code].join("+");
}

/** Splits a hotkey string into keycap labels, e.g. "Ctrl+KeyQ" -> ["Ctrl", "Q"]. */
export function hotkeyLabels(hotkey: string): string[] {
  return hotkey.split("+").map((token) => {
    const upper = token.trim().toUpperCase();
    const modifier = MODIFIER_ALIASES[upper];
    return modifier ?? keyLabel(token.trim());
  });
}

const MODIFIER_ALIASES: Record<string, string> = {
  CTRL: "Ctrl",
  CONTROL: "Ctrl",
  ALT: "Alt",
  OPTION: "Alt",
  SUPER: "Win",
  CMD: "Win",
  COMMAND: "Win",
  SHIFT: "Shift",
};

const KEY_LABELS: Record<string, string> = {
  Backquote: "~", // a lone backtick is too thin to read on a keycap
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  PageUp: "PgUp",
  PageDown: "PgDn",
  Delete: "Del",
  Insert: "Ins",
  CapsLock: "Caps",
  Mouse4: "Mouse 4",
  Mouse5: "Mouse 5",
};

/** Label for one key code, e.g. "KeyQ" -> "Q". */
export function keyLabel(code: string): string {
  if (KEY_LABELS[code]) return KEY_LABELS[code];
  const match = code.match(/^(?:Key|Digit)(.)$/);
  if (match) return match[1];
  if (code.startsWith("Numpad")) return `Num ${code.slice(6)}`;
  return code;
}
