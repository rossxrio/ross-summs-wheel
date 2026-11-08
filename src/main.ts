import "./styles/base.css";
import "./styles/wheel.css";
import "./styles/hud.css";
import "./styles/voice.css";

import { Store } from "./app/store";
import type { Timer } from "./app/timers";
import { roleItems, spellHint, spellIdsOf, spellItems, spellTitle } from "./app/wheel-content";
import { renderHud } from "./hud/hud";
import { normalizeSpellId, ROLE_LABEL, ROLES } from "./league/spells";
import type { Role } from "./league/types";
import { backend } from "./platform/backend";
import { WheelController } from "./wheel/controller";
import { WheelRenderer } from "./wheel/renderer";
import { parseVoice, type VoiceEnemy } from "./voice/parse";
import { VoiceToast } from "./voice/toast";

/** How often timers advance and the HUD redraws. */
const TICK_MS = 250;

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
    store.startTimer(role, spellId);
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
  store.advance((now - lastTick) / 1000);
  lastTick = now;
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
  if (done.length) voiceToast.done(done.join(" · "));
  else voiceToast.failed(text ? (result.problem ?? "Didn't catch a command") : "Didn't hear anything", text);
  redraw();
}

/** The overlay only receives mouse events while the wheel is open. */
function listenToMouse(): void {
  window.addEventListener("pointermove", (e) => wheel.pointer(e.clientX, e.clientY));
  window.addEventListener("pointerdown", (e) => wheel.click(e.button));
  window.addEventListener("wheel", (e) => wheel.scroll(e.deltaY));
  window.addEventListener("contextmenu", (e) => e.preventDefault());
}

async function start(): Promise<void> {
  listenToMouse();
  await backend.onHotkey((pressed) => wheel.hotkey(pressed));
  await backend.onQuickBind(({ role, spell }) => {
    wheel.dismiss();
    // Same rule as the wheel: only spells this enemy actually has.
    if (!spellIdsOf(store, role).includes(spell)) return;
    store.startTimer(role, spell);
    redraw();
  });
  await backend.onVoiceListening((on) => voiceToast.listening(on));
  await backend.onVoiceHeard((heard) => {
    if (!heard.test) onVoice(heard.words, heard.text);
  });
  await backend.onGameState((state) => {
    store.applyGameState(state);
    redraw();
  });

  try {
    store.setSpellData(await backend.getSpellData());
  } catch (err) {
    console.warn("Data Dragon unavailable, using bundled cooldowns:", err);
  }
  store.applyGameState(await backend.getGameState());

  tick();
  window.setInterval(tick, TICK_MS);
}

void start();
