import "../styles/settings.css";

import { ROLE_LABEL, ROLES, SPELL_NAMES } from "../league/spells";
import type { Role } from "../league/types";
import {
  settingsBackend as backend,
  type DownloadProgress,
  type Settings,
  type VoiceStatus,
} from "../platform/settings";
import { escapeHtml } from "../utils/html";
import { parseVoice, SPELL_LABEL } from "../voice/parse";
import { heldModifiers, hotkeyFrom, hotkeyLabels, isModifier, keyLabel, mouseCode, type ModifierState } from "./keys";
import { findProblem } from "./validate";

/** How long "Saved" shows before the window closes. */
const CLOSE_DELAY_MS = 450;
/** A bind sequence is at most this many keys... */
const MAX_BIND_KEYS = 3;
/** ...and recording stops this long after the last key. */
const RECORD_IDLE_MS = 1200;

const hotkeyButton = document.getElementById("hotkey") as HTMLButtonElement;
const bindList = document.getElementById("binds")!;
const bindsSub = document.getElementById("binds-sub")!;
const statusElement = document.getElementById("status")!;
const saveButton = document.getElementById("save") as HTMLButtonElement;
const overlayButton = document.getElementById("overlay-toggle") as HTMLButtonElement;
const soundsButton = document.getElementById("sounds-toggle") as HTMLButtonElement;
const quitButton = document.getElementById("quit") as HTMLButtonElement;
const voiceOn = document.getElementById("voice-on") as HTMLInputElement;
const voiceBody = document.getElementById("voice-body")!;
const voiceKeyButton = document.getElementById("voice-key") as HTMLButtonElement;
const voiceModel = document.getElementById("voice-model") as HTMLSelectElement;
const voiceDownload = document.getElementById("voice-download") as HTMLButtonElement;
const voiceTestButton = document.getElementById("voice-test") as HTMLButtonElement;
const voiceSub = document.getElementById("voice-sub")!;
const tabs = document.getElementById("tabs")!;
const bindsCount = document.getElementById("binds-count")!;
const voiceLive = document.getElementById("voice-live")!;

const ICON_X = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg>`;
const ICON_EYE = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/></svg>`;
const ICON_SPEAKER = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 6v4h2.5l3.5 3V3L5 6z"/><path d="M10.6 5.6a3.4 3.4 0 0 1 0 4.8M12.6 3.6a6.2 6.2 0 0 1 0 8.8"/></svg>`;
const ICON_SPEAKER_OFF = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 6v4h2.5l3.5 3V3L5 6z"/><path d="M11 6l3.5 4M14.5 6L11 10"/></svg>`;
const ICON_EYE_OFF = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6.6 3.7A6.6 6.6 0 0 1 8 3.5c4.1 0 6.5 4.5 6.5 4.5a11 11 0 0 1-1.7 2.3M10.2 11.9A5.8 5.8 0 0 1 8 12.5C3.9 12.5 1.5 8 1.5 8a11.3 11.3 0 0 1 2.8-3.2M2 2l12 12"/></svg>`;

// ---- State -----------------------------------------------------------------

type Recording =
  | { kind: "hotkey"; mods: string[] }
  | { kind: "bind"; index: number; before: string[]; idle?: number }
  | { kind: "voice" };

let saved: Settings = {
  hotkey: "",
  quickBinds: [],
  voice: { enabled: false, key: "", model: "tiny.en" },
  sounds: true,
};
let draft: Settings = structuredClone(saved);
let recording: Recording | null = null;
let saving = false;
/** The overlay window's visibility; changes apply right away, not on Save. */
let overlayShown = true;
/** Shown in the status line until the next change. */
let notice: { text: string; tone: "error" | "ok" } | null = null;

let voiceStatus: VoiceStatus | null = null;
let download: DownloadProgress | null = null;
/** Last test result or download error, under the voice controls. */
let voiceNote: { text: string; tone: "error" | "ok" | "" } | null = null;
let testing = false;

type Tab = "binds" | "voice";
const TAB_KEY = "settings.tab";
/** Which panel shows; remembered between openings (a convenience, so storage may fail). */
let tab: Tab = (() => {
  try {
    return localStorage.getItem(TAB_KEY) === "voice" ? "voice" : "binds";
  } catch {
    return "binds";
  }
})();

function showTab(next: Tab): void {
  if (next === tab) return;
  tab = next;
  try {
    localStorage.setItem(TAB_KEY, tab);
  } catch {
    // Not remembered; that's fine.
  }
  renderTabs();
}

const isDirty = () => JSON.stringify(draft) !== JSON.stringify(saved);

// ---- Rendering ---------------------------------------------------------------

const keycaps = (labels: string[], separator: string, extra = "") =>
  labels.map((label) => `<kbd class="${extra}">${escapeHtml(label)}</kbd>`).join(separator);

function render(): void {
  renderHotkey();
  renderBinds();
  renderVoice();
  renderFooter();
  renderAppActions();
  renderTabs();
}

function renderTabs(): void {
  tabs.dataset.active = tab;
  for (const button of tabs.querySelectorAll<HTMLElement>("[data-tab]")) {
    const selected = button.dataset.tab === tab;
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
    document.getElementById(button.getAttribute("aria-controls")!)!.hidden = !selected;
  }
  const count = draft.quickBinds.length;
  bindsCount.textContent = count ? String(count) : "";
  bindsCount.classList.toggle("problem", !recording && (findProblem(draft)?.rows.length ?? 0) > 0);
  voiceLive.classList.toggle("on", draft.voice.enabled);
}

const voiceChanged = () => JSON.stringify(draft.voice) !== JSON.stringify(saved.voice);

function renderVoice(): void {
  const voice = draft.voice;
  voiceOn.checked = voice.enabled;
  voiceBody.classList.toggle("off", !voice.enabled);

  const rec = recording?.kind === "voice";
  voiceKeyButton.classList.toggle("recording", rec);
  voiceKeyButton.innerHTML = rec
    ? `<span class="placeholder">Press a key or mouse button…</span>`
    : voice.key
      ? keycaps([keyLabel(voice.key)], "")
      : `<span class="placeholder">Set key</span>`;

  const models = voiceStatus?.models ?? [];
  voiceModel.innerHTML = models
    .map(
      (m) =>
        `<option value="${m.id}"${m.id === voice.model ? " selected" : ""}>${escapeHtml(m.label)}${
          m.downloaded ? "" : ` · ${m.sizeMb} MB`
        }</option>`,
    )
    .join("");
  const model = models.find((m) => m.id === voice.model);
  const downloading = download !== null;
  voiceDownload.hidden = !model || model.downloaded;
  voiceDownload.disabled = downloading;
  voiceDownload.textContent =
    download?.model === voice.model ? `${Math.floor((download.received / download.total) * 100)}%` : "Download";

  // The test uses what's saved and loaded, not the draft.
  const ready = voiceStatus?.state === "ready" && !voiceChanged();
  voiceTestButton.disabled = !ready && !testing;
  voiceTestButton.classList.toggle("active", testing);

  let note = voiceNote;
  if (!note) {
    let text = "";
    if (!voice.enabled) text = "Hold a key and say “ADC no flash”";
    else if (downloading) text = "Downloading the model…";
    else if (model && !model.downloaded) text = `Download the model (${model.sizeMb} MB) to start`;
    else if (voiceChanged()) text = "Save to try it";
    else if (voiceStatus?.message) text = voiceStatus.message;
    else if (ready) text = `In game: hold ${keyLabel(voice.key)}, say “ADC no flash”`;
    note = { text, tone: voiceStatus?.state === "error" ? "error" : "" };
  }
  voiceSub.textContent = note.text;
  voiceSub.className = `sub ${note.tone}`;
}

/** What a test clip turned into, as the overlay would read it (no game roster here). */
function describeTest(text: string, words: Parameters<typeof parseVoice>[0], decodeMs: number): string {
  if (!text) return "Didn't hear anything. Is the right mic the Windows default?";
  const { commands, problem } = parseVoice(words, []);
  const done = commands.map((c) =>
    c.kind === "undo" ? "Undo" : `${ROLE_LABEL[c.role]} ${SPELL_LABEL[c.spell] ?? c.spell}`,
  );
  const took = `${(decodeMs / 1000).toFixed(1)} s`;
  return done.length ? `“${text}” → ${done.join(", ")} (${took})` : `“${text}” → ${problem} (${took})`;
}

function renderAppActions(): void {
  const sounds = saved.sounds;
  soundsButton.innerHTML = sounds ? ICON_SPEAKER : ICON_SPEAKER_OFF;
  soundsButton.setAttribute("aria-label", sounds ? "Mute sounds" : "Unmute sounds");
  soundsButton.setAttribute("aria-pressed", String(!sounds));
  soundsButton.title = sounds
    ? "Mute sounds: voice cues and the chime when an enemy spell is back up"
    : "Sounds muted: click to turn them back on";

  overlayButton.innerHTML = overlayShown ? ICON_EYE : ICON_EYE_OFF;
  overlayButton.setAttribute("aria-label", overlayShown ? "Hide overlay" : "Show overlay");
  overlayButton.setAttribute("aria-pressed", String(!overlayShown));
  overlayButton.title = overlayShown
    ? "Hide overlay: the wheel and timers stay hidden until you show them again (resets on restart)"
    : "Overlay hidden: click to bring the wheel and timers back";
}

function renderHotkey(): void {
  const rec = recording?.kind === "hotkey" ? recording : null;
  hotkeyButton.classList.toggle("recording", !!rec);
  hotkeyButton.innerHTML = rec
    ? keycaps([...rec.mods, "…"], '<span class="plus">+</span>', "pending")
    : keycaps(hotkeyLabels(draft.hotkey), '<span class="plus">+</span>');
  bindsSub.textContent = `Hold ${hotkeyLabels(draft.hotkey).join(" + ")}, then type the keys`;
}

function renderBinds(): void {
  const problemRows = recording ? [] : (findProblem(draft)?.rows ?? []);
  const options = (items: { value: string; label: string }[], selected: string) =>
    items
      .map(
        (o) => `<option value="${o.value}"${o.value === selected ? " selected" : ""}>${escapeHtml(o.label)}</option>`,
      )
      .join("");

  bindList.innerHTML = draft.quickBinds.length
    ? draft.quickBinds
        .map((bind, i) => {
          const isRecording = recording?.kind === "bind" && recording.index === i;
          const keys = bind.keys.length
            ? keycaps(bind.keys.map(keyLabel), '<span class="then">›</span>')
            : `<span class="placeholder">${isRecording ? "Type keys…" : "Record"}</span>`;
          return `
            <li class="bind${problemRows.includes(i) ? " problem" : ""}" data-index="${i}">
              <select data-field="role" aria-label="Enemy">${options(
                ROLES.map((r) => ({ value: r, label: ROLE_LABEL[r] })),
                bind.role,
              )}</select>
              <select data-field="spell" aria-label="Spell">${options(
                SPELL_NAMES.map((s) => ({ value: s.id, label: s.name })),
                bind.spell,
              )}</select>
              <button class="keys${isRecording ? " recording" : ""}" data-action="record" title="Click to record">${keys}</button>
              <button class="remove" data-action="remove" aria-label="Remove" title="Remove">${ICON_X}</button>
            </li>`;
        })
        .join("")
    : `<li class="empty"><strong>No quick binds yet</strong>Start a timer with a few keys, no wheel needed</li>`;
}

function renderFooter(): void {
  const problem = findProblem(draft);
  let text = "Tap the hotkey 3 times to open this";
  let tone = "";
  if (recording?.kind === "hotkey") text = "Press a key or Mouse 4/5 · Esc to cancel";
  else if (recording?.kind === "voice") text = "Press a key or Mouse 4/5 · Esc to cancel";
  else if (recording?.kind === "bind") text = "Up to 3 keys or Mouse 4/5 · Enter to finish";
  else if (notice) ({ text, tone } = notice);
  else if (problem) [text, tone] = [problem.message, "error"];

  statusElement.textContent = text;
  statusElement.className = `status ${tone}`;
  saveButton.disabled = saving || !!recording || !!problem || !isDirty();
}

// ---- Recording -----------------------------------------------------------------

function startRecording(next: Recording): void {
  finishRecording();
  if (next.kind === "bind") draft.quickBinds[next.index].keys = [];
  recording = next;
  notice = null;
  render();
}

/** Keeps what was typed. A bind left empty gets its old keys back, or goes if it was new. */
function finishRecording(): void {
  const rec = recording;
  if (!rec) return;
  recording = null;
  if (rec.kind === "bind") {
    window.clearTimeout(rec.idle);
    const bind = draft.quickBinds[rec.index];
    if (bind.keys.length === 0) {
      if (rec.before.length) bind.keys = rec.before;
      else draft.quickBinds.splice(rec.index, 1);
    }
  }
  render();
}

/** Drops what was typed; finishRecording then restores the old keys (or drops a new row). */
function cancelRecording(): void {
  if (recording?.kind === "bind") draft.quickBinds[recording.index].keys = [];
  finishRecording();
}

function recordKey(e: KeyboardEvent): void {
  if (!isModifier(e.code)) return record(e.code, e);
  // A modifier alone only updates the hotkey preview.
  if (recording?.kind === "hotkey") {
    recording.mods = heldModifiers(e);
    renderHotkey();
  }
}

/** Takes a key or mouse side button (`code`) pressed with `mods` held. */
function record(code: string, mods: ModifierState): void {
  const rec = recording!;
  if (rec.kind === "voice") {
    // A single key or button; it works whatever modifiers are held.
    draft.voice.key = code;
    return finishRecording();
  }
  if (rec.kind === "hotkey") {
    draft.hotkey = hotkeyFrom(code, mods);
    return finishRecording();
  }

  // Modifiers are implied: bind keys are pressed with the hotkey's modifiers held.
  const keys = draft.quickBinds[rec.index].keys;
  keys.push(code);
  window.clearTimeout(rec.idle);
  if (keys.length >= MAX_BIND_KEYS) return finishRecording();
  rec.idle = window.setTimeout(finishRecording, RECORD_IDLE_MS);
  render();
}

// ---- Actions ---------------------------------------------------------------------

async function save(): Promise<void> {
  if (saveButton.disabled) return;
  saving = true;
  renderFooter();
  try {
    await backend.saveSettings(draft);
    const voiceTurnedOn = draft.voice.enabled && voiceChanged();
    saved = structuredClone(draft);
    voiceNote = null;
    if (voiceTurnedOn) {
      // Stay open so the new voice settings can be tried right away.
      notice = { text: "Saved · try Hold to test", tone: "ok" };
    } else {
      notice = { text: "Saved", tone: "ok" };
      window.setTimeout(() => void backend.close(), CLOSE_DELAY_MS);
    }
  } catch (err) {
    notice = { text: String(err), tone: "error" };
  }
  saving = false;
  // Everything, not just the footer: the voice test only unlocks once saved.
  render();
}

async function toggleOverlay(): Promise<void> {
  overlayButton.disabled = true;
  try {
    await backend.setOverlayShown(!overlayShown);
    overlayShown = !overlayShown;
    notice = overlayShown
      ? { text: "Overlay shown", tone: "ok" }
      : { text: "Overlay hidden · tap the hotkey 3× for settings", tone: "ok" };
  } catch (err) {
    notice = { text: String(err), tone: "error" };
  }
  overlayButton.disabled = false;
  render();
}

/** Applies and saves at once, like the overlay button; it isn't part of the Save draft. */
async function toggleSounds(): Promise<void> {
  soundsButton.disabled = true;
  const on = !saved.sounds;
  try {
    await backend.setSounds(on);
    saved.sounds = draft.sounds = on;
    notice = { text: on ? "Sounds on" : "Sounds muted", tone: "ok" };
  } catch (err) {
    notice = { text: String(err), tone: "error" };
  }
  soundsButton.disabled = false;
  render();
}

async function quit(): Promise<void> {
  quitButton.disabled = true;
  try {
    await backend.quit();
  } catch (err) {
    quitButton.disabled = false;
    notice = { text: String(err), tone: "error" };
    renderFooter();
  }
}

function addBind(): void {
  const used = new Set(draft.quickBinds.map((b) => b.role));
  const role: Role = ROLES.find((r) => !used.has(r)) ?? "TOP";
  showTab("binds");
  draft.quickBinds.push({ role, spell: "SummonerFlash", keys: [] });
  startRecording({ kind: "bind", index: draft.quickBinds.length - 1, before: [] });
  bindList.lastElementChild?.scrollIntoView({ block: "nearest" });
}

window.addEventListener("keydown", (e) => {
  if (saving) return e.preventDefault();
  if (recording) {
    e.preventDefault();
    if (e.repeat) return;
    if (e.code === "Escape") return cancelRecording();
    if (e.code === "Enter" || e.code === "NumpadEnter") return finishRecording();
    return recordKey(e);
  }
  // Enter and Esc act on the window, never on a focused button.
  if (e.code === "Escape") {
    e.preventDefault();
    void backend.close();
  } else if (e.code === "Enter" || e.code === "NumpadEnter") {
    e.preventDefault();
    void save();
  }
});

window.addEventListener("keyup", (e) => {
  // Modifiers let go without a key: drop them from the hotkey preview.
  if (recording?.kind === "hotkey" && isModifier(e.code)) {
    recording.mods = heldModifiers(e);
    renderHotkey();
  }
});

// A click anywhere else ends recording first.
document.addEventListener(
  "pointerdown",
  (e) => {
    // Mouse 4/5 while recording are the input itself, wherever the pointer is.
    const side = mouseCode(e.button);
    if (recording && side) {
      e.preventDefault();
      e.stopPropagation();
      return record(side, e);
    }
    const target = e.target as Element;
    const active =
      recording?.kind === "hotkey"
        ? hotkeyButton
        : recording?.kind === "voice"
          ? voiceKeyButton
          : bindList.querySelector(".keys.recording");
    if (recording && !active?.contains(target)) finishRecording();
  },
  true,
);

// Mouse 4/5 would otherwise mean back/forward in the webview.
for (const type of ["mousedown", "mouseup", "auxclick"] as const) {
  window.addEventListener(type, (e) => {
    if (mouseCode(e.button)) e.preventDefault();
  });
}

hotkeyButton.addEventListener("click", () => {
  if (recording?.kind === "hotkey") cancelRecording();
  else startRecording({ kind: "hotkey", mods: [] });
});

bindList.addEventListener("click", (e) => {
  const button = (e.target as Element).closest<HTMLElement>("[data-action]");
  const index = Number(button?.closest<HTMLElement>(".bind")?.dataset.index);
  if (!button || Number.isNaN(index)) return;

  if (button.dataset.action === "remove") {
    draft.quickBinds.splice(index, 1);
    notice = null;
    render();
  } else if (recording?.kind === "bind" && recording.index === index) {
    finishRecording();
  } else {
    startRecording({ kind: "bind", index, before: draft.quickBinds[index].keys });
  }
});

bindList.addEventListener("change", (e) => {
  const select = e.target as HTMLSelectElement;
  const index = Number(select.closest<HTMLElement>(".bind")?.dataset.index);
  const bind = draft.quickBinds[index];
  if (select.dataset.field === "role") bind.role = select.value as Role;
  else bind.spell = select.value;
  notice = null;
  render();
});

tabs.addEventListener("click", (e) => {
  const button = (e.target as Element).closest<HTMLElement>("[data-tab]");
  if (button) showTab(button.dataset.tab as Tab);
});

// Arrow keys move between tabs, as in any tab list.
tabs.addEventListener("keydown", (e) => {
  if (e.code !== "ArrowLeft" && e.code !== "ArrowRight") return;
  e.preventDefault();
  showTab(tab === "binds" ? "voice" : "binds");
  tabs.querySelector<HTMLElement>(`[data-tab="${tab}"]`)?.focus();
});

document.getElementById("add")!.addEventListener("click", addBind);
saveButton.addEventListener("click", () => void save());
document.getElementById("close")!.addEventListener("click", () => void backend.close());
overlayButton.addEventListener("click", () => void toggleOverlay());
soundsButton.addEventListener("click", () => void toggleSounds());

voiceOn.addEventListener("change", () => {
  draft.voice.enabled = voiceOn.checked;
  notice = null;
  voiceNote = null;
  if (voiceOn.checked && !draft.voice.key) startRecording({ kind: "voice" });
  else render();
});

voiceKeyButton.addEventListener("click", () => {
  if (recording?.kind === "voice") cancelRecording();
  else startRecording({ kind: "voice" });
});

voiceModel.addEventListener("change", () => {
  draft.voice.model = voiceModel.value;
  voiceNote = null;
  render();
});

voiceDownload.addEventListener("click", async () => {
  const model = draft.voice.model;
  download = { model, received: 0, total: 1 };
  voiceNote = null;
  render();
  try {
    await backend.downloadVoiceModel(model);
  } catch (err) {
    voiceNote = { text: String(err), tone: "error" };
  }
  download = null;
  voiceStatus = await backend.getVoiceStatus();
  render();
});

function stopTest(): void {
  if (!testing) return;
  testing = false;
  voiceNote = { text: "…", tone: "" };
  void backend.voiceTest(false);
  render();
}

voiceTestButton.addEventListener("pointerdown", (e) => {
  if (voiceTestButton.disabled || e.button !== 0) return;
  voiceTestButton.setPointerCapture(e.pointerId);
  testing = true;
  voiceNote = { text: "Listening… say a command, then let go", tone: "" };
  void backend.voiceTest(true);
  render();
});
voiceTestButton.addEventListener("pointerup", stopTest);
voiceTestButton.addEventListener("pointercancel", stopTest);
quitButton.addEventListener("click", () => void quit());

async function start(): Promise<void> {
  await backend.onVoiceStatus((status) => {
    voiceStatus = status;
    render();
  });
  await backend.onVoiceDownload((progress) => {
    // Events can arrive after the download command returned; don't bring it back.
    if (!download) return;
    download = progress;
    renderVoice();
  });
  await backend.onVoiceListening(({ state, test }) => {
    // The test press came to nothing: no transcript is coming.
    if (!test || state !== "idle" || testing) return;
    voiceNote = { text: "Too short: hold the button while you speak", tone: "error" };
    renderVoice();
  });
  await backend.onVoiceHeard((heard) => {
    if (!heard.test) return;
    voiceNote = { text: describeTest(heard.text, heard.words, heard.decodeMs), tone: heard.text ? "ok" : "error" };
    renderVoice();
  });
  [saved, overlayShown, voiceStatus] = await Promise.all([
    backend.getSettings(),
    backend.isOverlayShown(),
    backend.getVoiceStatus(),
  ]);
  draft = structuredClone(saved);
  render();
}

void start();
