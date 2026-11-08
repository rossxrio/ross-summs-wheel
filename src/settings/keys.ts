/**
 * Hotkey strings as stored in config.json and parsed by the Rust side
 * (global-hotkey): modifiers then one W3C key code, joined by "+",
 * e.g. "Backquote", "Ctrl+Alt+KeyQ", "F13".
 */

/** Modifiers in the order they're written. */
const MODIFIERS = [
  { token: "Ctrl", label: "Ctrl", held: (e: KeyboardEvent) => e.ctrlKey },
  { token: "Shift", label: "Shift", held: (e: KeyboardEvent) => e.shiftKey },
  { token: "Alt", label: "Alt", held: (e: KeyboardEvent) => e.altKey },
  { token: "Super", label: "Win", held: (e: KeyboardEvent) => e.metaKey },
];

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
export function heldModifiers(e: KeyboardEvent): string[] {
  return MODIFIERS.filter((m) => m.held(e)).map((m) => m.label);
}

/** The hotkey string for a key press. Call only for non-modifier keys. */
export function hotkeyFromEvent(e: KeyboardEvent): string {
  const mods = MODIFIERS.filter((m) => m.held(e)).map((m) => m.token);
  return [...mods, e.code].join("+");
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
};

/** Label for one key code, e.g. "KeyQ" -> "Q". */
export function keyLabel(code: string): string {
  if (KEY_LABELS[code]) return KEY_LABELS[code];
  const match = code.match(/^(?:Key|Digit)(.)$/);
  if (match) return match[1];
  if (code.startsWith("Numpad")) return `Num ${code.slice(6)}`;
  return code;
}
