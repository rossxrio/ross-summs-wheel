import "./styles/base.css";
import "./styles/wheel.css";
import "./styles/hud.css";
import "./styles/voice.css";

import { playSpellUp, setSoundsOn, voiceListening, voiceResult } from "./app/sounds";
import { Store } from "./app/store";
import type { Timer } from "./app/timers";
import { roleItems, spellHint, spellIdsOf, spellItems, spellTitle } from "./app/wheel-content";
import { renderHud } from "./hud/hud";
import { normalizeSpellId, ROLE_LABEL, ROLES } from "./league/spells";
import type { GameState, Role } from "./league/types";
import { backend } from "./platform/backend";
import { WheelController } from "./wheel/controller";
import { WheelRenderer } from "./wheel/renderer";
import { parseVoice, type VoiceEnemy } from "./voice/parse";
import { VoiceToast } from "./voice/toast";

/** How often timers advance and the HUD redraws. */
const TICK_MS = 250;
/** Data Dragon is retried this often until it loads (the PC may start offline). */
const SPELL_DATA_RETRY_MS = 60_000;

const store = new Store();
const hudElement = document.getElementById("hud")!;
const voiceToast = new VoiceToast(document.getElementById("voice")!);
const wheelRenderer = new WheelRenderer(document.getElementById("wheel")!);

const wheel = new WheelController({
  setMode: backend.setWheelMode,
  roleItems: () => roleItems(store),
  spellItems: (role) => spellItems(store, role),
  spellTitle: (role) => spellTitle(store, role),
  spellHint: (role) => spellHint(store, role),
  toggleCosmicInsight: (role) => {
    store.toggleCosmicInsight(role);
    redraw();
  },
  commit: (role, spellId) => {
    store.pickTimer(role, spellId);
    redraw();
  },
  render: (view) => wheelRenderer.render(view),
});

function redraw(): void {
  renderHud(hudElement, store);
  wheel.refresh();
}

let lastTick = performance.now();
function tick(): void {
  const now = performance.now();
  const up = store.advance((now - lastTick) / 1000);
  lastTick = now;
  if (up.length) playSpellUp();
  redraw();
}

/** Timers started by voice, newest last, so "undo" can put each back. */
const voiceUndo: { role: Role; spell: string; before?: Timer }[] = [];
const VOICE_UNDO_DEPTH = 10;

function voiceEnemies(): VoiceEnemy[] {
  return ROLES.flatMap((role) => {
    const enemy = store.roster[role];
    return enemy ? [{ role, champion: enemy.champion, spells: enemy.spells.map(normalizeSpellId) }] : [];
  });
}

function commandLabel(role: Role, spell: string): string {
  const who = store.roster[role]?.champion ?? ROLE_LABEL[role];
  return `${who} ${store.spellData.spells[spell]?.name ?? spell}`;
}

/** Acts on what Whisper heard: start (backdated) timers, or undo the last one. */
function onVoice(words: Parameters<typeof parseVoice>[0], text: string): void {
  const result = parseVoice(words, voiceEnemies());
  const done: string[] = [];
  for (const command of result.commands) {
    if (command.kind === "undo") {
      const last = voiceUndo.pop();
      if (last) {
        store.timers.restore(last.role, last.spell, last.before);
        done.push(`Undid ${commandLabel(last.role, last.spell)}`);
      }
      continue;
    }
    const before = store.timers.get(command.role, command.spell);
    voiceUndo.push({ role: command.role, spell: command.spell, before: before && { ...before } });
    if (voiceUndo.length > VOICE_UNDO_DEPTH) voiceUndo.shift();
    store.startTimer(command.role, command.spell, command.agoMs / 1000);
    done.push(commandLabel(command.role, command.spell));
  }
  if (done.length) {
    voiceToast.done(done.join(" · "));
  } else {
    voiceToast.failed(text ? (result.problem ?? "Didn't catch a command") : "Didn't hear anything", text);
  }
  voiceResult(done.length > 0);
  redraw();
}

function applyGameState(state: GameState): void {
  // "Undo" must never bring back a timer from the last match.
  if (store.applyGameState(state)) voiceUndo.length = 0;
  redraw();
}

/** The overlay only receives mouse events while the wheel is open. */
function listenToMouse(): void {
  window.addEventListener("pointermove", (e) => wheel.pointer(e.clientX, e.clientY));
  window.addEventListener("pointerdown", (e) => wheel.click(e.button));
  window.addEventListener("wheel", (e) => wheel.scroll(e.deltaY));
  window.addEventListener("contextmenu", (e) => e.preventDefault());
}

/** Icons and this patch's cooldowns; the bundled cooldowns stand in until then. */
async function loadSpellData(): Promise<void> {
  try {
    store.setSpellData(await backend.getSpellData());
    redraw();
  } catch (err) {
    console.warn("Data Dragon unavailable, using bundled cooldowns for now:", err);
    window.setTimeout(() => void loadSpellData(), SPELL_DATA_RETRY_MS);
  }
}

async function start(): Promise<void> {
  listenToMouse();
  await backend.onHotkey((pressed) => wheel.hotkey(pressed));
  await backend.onQuickBind(({ role, spell }) => {
    wheel.dismiss();
    // Same rule as the wheel: only spells this enemy actually has.
    if (!spellIdsOf(store, role).includes(spell)) return;
    store.pickTimer(role, spell);
    redraw();
  });
  // The settings test button shows its own result.
  await backend.onVoiceListening(({ state, test }) => {
    if (test) return;
    voiceToast.listening(state);
    voiceListening(state);
  });
  await backend.onVoiceHeard((heard) => {
    if (!heard.test) onVoice(heard.words, heard.text);
  });
  await backend.onGameState(applyGameState);
  await backend.onSoundsOn(setSoundsOn);
  setSoundsOn(await backend.getSoundsOn());

  await loadSpellData();
  applyGameState(await backend.getGameState());

  tick();
  window.setInterval(tick, TICK_MS);
}

void start();
