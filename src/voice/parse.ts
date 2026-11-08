/**
 * Turns what Whisper heard ("ADC no flash", "Jinx flash", "smite down") into
 * timer commands.
 *
 * In a game only about ten commands make sense (five enemies x two spells),
 * so this doesn't need to understand speech. It spots role, champion and
 * spell words, pairs them, and keeps only pairs the enemy can actually have.
 * Anything unclear is rejected: no timer beats a wrong timer.
 *
 * Imports are type-only so scripts/voice-eval.ts can run this file in Node.
 */
import type { Role } from "../league/types";

/** One word as heard, and how long ago it was said. */
export interface HeardWord {
  text: string;
  agoMs: number;
}

/** What the parser knows about an enemy. With no enemies (no game) every role/spell pair is allowed. */
export interface VoiceEnemy {
  role: Role;
  champion?: string;
  /** Base spell ids, e.g. "SummonerFlash". */
  spells: string[];
}

export type VoiceCommand =
  | { kind: "timer"; role: Role; spell: string; /** When the command started being said. */ agoMs: number }
  | { kind: "undo" };

export interface VoiceResult {
  commands: VoiceCommand[];
  /** Why nothing (or not everything) was used, for the toast. */
  problem?: string;
}

const ROLE_WORDS: Record<Role, string[]> = {
  TOP: ["top", "toplane", "toplaner", "tops"],
  JUNGLE: ["jungle", "jungler", "jungles", "junglers", "jg", "jgl", "jung", "jungla"],
  MIDDLE: ["mid", "middle", "midlane", "midlaner", "mids", "medio"],
  BOTTOM: ["adc", "adcs", "ad", "bot", "bots", "bottom", "botlane", "carry", "marksman", "adcarry", "tirador"],
  UTILITY: ["support", "supports", "supp", "supps", "sup", "sub", "supporter", "soporte"],
};

const SPELL_WORDS: Record<string, string[]> = {
  SummonerFlash: ["flash", "flashes", "flashed", "flesh", "flush"],
  SummonerDot: ["ignite", "ignites", "ignited", "ig", "ign", "dot"],
  SummonerTeleport: ["teleport", "teleports", "tp", "tps", "tele", "teepee", "tepee"],
  SummonerHeal: ["heal", "heals", "healed", "heel"],
  SummonerBarrier: ["barrier", "barriers"],
  SummonerExhaust: ["exhaust", "exhausts", "exhausted", "exh"],
  SummonerBoost: ["cleanse", "cleanses", "cleansed", "cleans"],
  SummonerHaste: ["ghost", "ghosts", "ghosted"],
  SummonerSmite: ["smite", "smites", "smited", "smight"],
};

const UNDO_WORDS = ["undo", "cancel", "nevermind", "scratchthat", "wrong"];

/** Words that can be glued to a role or spell ("midno", "noflash"). */
const FILLERS = ["no", "now", "know", "down", "used", "gone", "sin"];

export const SPELL_LABEL: Record<string, string> = {
  SummonerFlash: "Flash",
  SummonerDot: "Ignite",
  SummonerTeleport: "Teleport",
  SummonerHeal: "Heal",
  SummonerBarrier: "Barrier",
  SummonerExhaust: "Exhaust",
  SummonerBoost: "Cleanse",
  SummonerHaste: "Ghost",
  SummonerSmite: "Smite",
};

type Mention =
  | { kind: "role"; role: Role; agoMs: number }
  | { kind: "spell"; spell: string; agoMs: number }
  | { kind: "undo"; agoMs: number };

/** Lowercase letters and digits only, accents dropped: "Kai'Sa" -> "kaisa". */
const clean = (s: string) =>
  s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "");

export function parseVoice(words: HeardWord[], enemies: VoiceEnemy[]): VoiceResult {
  const mentions = findMentions(words, enemies);
  const commands: VoiceCommand[] = [];
  const problems: string[] = [];

  const pair = (role: Role, spell: Mention & { kind: "spell" }, roleAgo: number) => {
    const enemy = enemies.find((e) => e.role === role);
    if (enemies.length && !enemy?.spells.includes(spell.spell)) {
      problems.push(`${roleName(role, enemy)} has no ${SPELL_LABEL[spell.spell]}`);
      return;
    }
    if (commands.some((c) => c.kind === "timer" && c.role === role && c.spell === spell.spell)) return;
    commands.push({ kind: "timer", role, spell: spell.spell, agoMs: Math.max(roleAgo, spell.agoMs) });
  };

  // Spells pair with the role said before them ("ADC no flash no heal") or,
  // failing that, the next one ("flash on ADC").
  let current: (Mention & { kind: "role" }) | null = null;
  let waiting: (Mention & { kind: "spell" })[] = [];
  for (const m of mentions) {
    if (m.kind === "undo") commands.push({ kind: "undo" });
    else if (m.kind === "role") {
      current = m;
      for (const spell of waiting) pair(m.role, spell, m.agoMs);
      waiting = [];
    } else if (current) pair(current.role, m, current.agoMs);
    else waiting.push(m);
  }

  // A spell with no role: fine if only one enemy has it ("smite down").
  for (const spell of waiting) {
    const owners = enemies.filter((e) => e.spells.includes(spell.spell));
    if (owners.length === 1) pair(owners[0].role, spell, spell.agoMs);
    else problems.push(`Whose ${SPELL_LABEL[spell.spell]}?`);
  }

  if (!commands.length && !problems.length) {
    problems.push(mentions.some((m) => m.kind === "role") ? "Which spell?" : "Didn't catch a command");
  }
  return { commands, problem: problems[0] };
}

function roleName(role: Role, enemy?: VoiceEnemy): string {
  if (enemy?.champion) return enemy.champion;
  return { TOP: "Top", JUNGLE: "Jungle", MIDDLE: "Mid", BOTTOM: "ADC", UTILITY: "Support" }[role];
}

function findMentions(words: HeardWord[], enemies: VoiceEnemy[]): Mention[] {
  // "Mid-Flash", "Kai'Sa", "a.d.c." -> separate clean tokens, each keeping its word's time.
  const tokens = words.flatMap((w) =>
    w.text
      .split(/[\s\-–—/,.!?]+/)
      .map(clean)
      .filter(Boolean)
      .map((text) => ({ text, agoMs: w.agoMs })),
  );

  const champions = enemies
    .filter((e) => e.champion)
    .map((e) => ({ name: clean(e.champion!), role: e.role }))
    .filter((c) => c.name.length >= 3);

  const lookup = (text: string, fuzzy: boolean): LookupHit | null => {
    if (UNDO_WORDS.includes(text)) return { kind: "undo" };
    for (const [role, aliases] of Object.entries(ROLE_WORDS)) {
      if (aliases.includes(text)) return { kind: "role", role: role as Role };
    }
    for (const [spell, aliases] of Object.entries(SPELL_WORDS)) {
      if (aliases.includes(text)) return { kind: "spell", spell };
    }
    for (const c of champions) if (c.name === text) return { kind: "role", role: c.role };
    if (!fuzzy) return null;
    // Near misses on longer words only: "thrush" for Thresh, "flashe".
    for (const c of champions) if (close(text, c.name)) return { kind: "role", role: c.role };
    for (const [spell, aliases] of Object.entries(SPELL_WORDS)) {
      if (close(text, aliases[0])) return { kind: "spell", spell };
    }
    return null;
  };

  const mentions: Mention[] = [];
  const add = (hit: LookupHit, agoMs: number) => mentions.push({ ...hit, agoMs } as Mention);

  for (let i = 0; i < tokens.length;) {
    // Longest first: "lee sin", "a d c", "t p".
    let matched = 0;
    for (let len = Math.min(3, tokens.length - i); len >= 1 && !matched; len--) {
      const hit = lookup(
        tokens
          .slice(i, i + len)
          .map((t) => t.text)
          .join(""),
        len === 1,
      );
      if (hit) {
        add(hit, tokens[i].agoMs);
        matched = len;
      }
    }
    if (!matched) {
      for (const hit of splitGlued(tokens[i].text, lookup)) add(hit, tokens[i].agoMs);
      matched = 1;
    }
    i += matched;
  }
  return mentions;
}

type LookupHit = { kind: "role"; role: Role } | { kind: "spell"; spell: string } | { kind: "undo" };

/** "midno" -> mid, "noflash" -> flash, "midflash" -> mid + flash. */
function splitGlued(text: string, lookup: (t: string, fuzzy: boolean) => LookupHit | null): LookupHit[] {
  for (let cut = 2; cut <= text.length - 2; cut++) {
    const [a, b] = [text.slice(0, cut), text.slice(cut)];
    const [hitA, hitB] = [lookup(a, false), lookup(b, false)];
    if (hitA && hitB) return [hitA, hitB];
    if (hitA && FILLERS.includes(b)) return [hitA];
    if (hitB && FILLERS.includes(a)) return [hitB];
  }
  return [];
}

/** One edit apart for words of 5+ letters, two for 8+. */
function close(a: string, b: string): boolean {
  const max = b.length >= 8 ? 2 : b.length >= 5 ? 1 : 0;
  return max > 0 && Math.abs(a.length - b.length) <= max && editDistance(a, b) <= max;
}

function editDistance(a: string, b: string): number {
  let prev = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    for (let j = 1; j <= b.length; j++) {
      row[j] = Math.min(prev[j] + 1, row[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    }
    prev = row;
  }
  return prev[b.length];
}
