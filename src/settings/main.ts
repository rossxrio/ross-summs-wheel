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
import { heldModifiers, hotkeyFromEvent, hotkeyLabels, isModifier, keyLabel } from "./keys";
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
const quitButton = document.getElementById("quit") as HTMLButtonElement;
const voiceOn = document.getElementById("voice-on") as HTMLInputElement;
const voiceBody = document.getElementById("voice-body")!;
const voiceKeyButton = document.getElementById("voice-key") as HTMLButtonElement;
const voiceModel = document.getElementById("voice-model") as HTMLSelectElement;
const voiceDownload = document.getElementById("voice-download") as HTMLButtonElement;
const voiceTestButton = document.getElementById("voice-test") as HTMLButtonElement;
const voiceSub = document.getElementById("voice-sub")!;

// ---- State -----------------------------------------------------------------

type Recording =
  | { kind: "hotkey"; mods: string[] }
  | { kind: "bind"; index: number; before: string[]; idle?: number }
  | { kind: "voice" };

let saved: Settings = { hotkey: "", quickBinds: [], voice: { enabled: false, key: "", model: "tiny.en" } };
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
}

const voiceChanged = () => JSON.stringify(draft.voice) !== JSON.stringify(saved.voice);

function renderVoice(): void {
  const voice = draft.voice;
  voiceOn.checked = voice.enabled;
  voiceBody.classList.toggle("off", !voice.enabled);

  const rec = recording?.kind === "voice";
  voiceKeyButton.classList.toggle("recording", rec);
  voiceKeyButton.innerHTML = rec
    ? `<span class="placeholder">Press a key…</span>`
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
  overlayButton.textContent = overlayShown ? "Hide overlay" : "Show overlay";
  overlayButton.setAttribute("aria-pressed", String(!overlayShown));
  overlayButton.title = overlayShown
    ? "Hide the wheel and timers until you show them again (resets on restart)"
    : "Bring the wheel and timers back";
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
              <button class="remove" data-action="remove" aria-label="Remove">&times;</button>
            </li>`;
        })
        .join("")
    : `<li class="empty">No quick binds yet</li>`;
}

function renderFooter(): void {
  const problem = findProblem(draft);
  let text = "Tap the hotkey 3 times to open this";
  let tone = "";
  if (recording?.kind === "hotkey") text = "Press the new hotkey · Esc to cancel";
  else if (recording?.kind === "voice") text = "Press the talk key · Esc to cancel";
  else if (recording?.kind === "bind") text = "Type up to 3 keys · Enter to finish";
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
  const rec = recording!;
  if (rec.kind === "voice") {
    // A single key; it works whatever modifiers are held.
    if (isModifier(e.code)) return;
    draft.voice.key = e.code;
    return finishRecording();
  }
  if (rec.kind === "hotkey") {
    if (isModifier(e.code)) {
      rec.mods = heldModifiers(e);
      return renderHotkey();
    }
    draft.hotkey = hotkeyFromEvent(e);
    return finishRecording();
  }

  // Modifiers are implied: bind keys are pressed with the hotkey's modifiers held.
  if (isModifier(e.code)) return;
  const keys = draft.quickBinds[rec.index].keys;
  keys.push(e.code);
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
  renderFooter();
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

document.getElementById("add")!.addEventListener("click", addBind);
saveButton.addEventListener("click", () => void save());
document.getElementById("close")!.addEventListener("click", () => void backend.close());
overlayButton.addEventListener("click", () => void toggleOverlay());

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
  if (voiceTestButton.disabled) return;
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
    download = progress;
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
