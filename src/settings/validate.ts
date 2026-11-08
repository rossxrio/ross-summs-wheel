import { ROLE_LABEL, SPELL_NAMES } from "../league/spells";
import type { QuickBind, Settings } from "../platform/settings";
import { keyLabel, mainKeyCode } from "./keys";

export interface Problem {
  /** Indexes of the binds to highlight. */
  rows: number[];
  message: string;
}

export const bindName = (bind: QuickBind): string =>
  `${ROLE_LABEL[bind.role]} ${SPELL_NAMES.find((s) => s.id === bind.spell)?.name ?? bind.spell}`;

const startsWith = (keys: string[], prefix: string[]) => prefix.every((key, i) => keys[i] === key);

/**
 * The first thing that would stop the settings from saving, or null.
 *
 * One bind can't be the start of another (T vs T D): the shorter one would
 * always fire first. The wheel key can't be in a bind: it's already held.
 */
export function findProblem(settings: Settings): Problem | null {
  const binds = settings.quickBinds;
  const wheelKey = mainKeyCode(settings.hotkey);

  for (const [i, bind] of binds.entries()) {
    if (bind.keys.length === 0) return { rows: [i], message: `Record keys for ${bindName(bind)}` };
    if (bind.keys.includes(wheelKey)) {
      return { rows: [i], message: `${keyLabel(wheelKey)} is the wheel hotkey` };
    }
  }

  for (let i = 0; i < binds.length; i++) {
    for (let j = i + 1; j < binds.length; j++) {
      const [a, b] = [binds[i].keys, binds[j].keys];
      if (startsWith(a, b) || startsWith(b, a)) {
        return { rows: [i, j], message: `${bindName(binds[i])} and ${bindName(binds[j])} overlap` };
      }
    }
  }
  return null;
}
