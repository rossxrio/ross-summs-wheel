import "../styles/settings.css";

import { ROLE_LABEL, ROLES, SPELL_NAMES } from "../league/spells";
import type { Role } from "../league/types";
import { settingsBackend as backend, type Settings } from "../platform/settings";
import { escapeHtml } from "../utils/html";
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

// ---- State -----------------------------------------------------------------

type Recording = { kind: "hotkey"; mods: string[] } | { kind: "bind"; index: number; before: string[]; idle?: number };

let saved: Settings = { hotkey: "", quickBinds: [] };
let draft: Settings = structuredClone(saved);
let recording: Recording | null = null;
let saving = false;
/** The overlay window's visibility; changes apply right away, not on Save. */
let overlayShown = true;
/** Shown in the status line until the next change. */
let notice: { text: string; tone: "error" | "ok" } | null = null;

const isDirty = () => JSON.stringify(draft) !== JSON.stringify(saved);

// ---- Rendering ---------------------------------------------------------------

const keycaps = (labels: string[], separator: string, extra = "") =>
  labels.map((label) => `<kbd class="${extra}">${escapeHtml(label)}</kbd>`).join(separator);

function render(): void {
  renderHotkey();
  renderBinds();
  renderFooter();
  renderAppActions();
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
    saved = structuredClone(draft);
    notice = { text: "Saved", tone: "ok" };
    window.setTimeout(() => void backend.close(), CLOSE_DELAY_MS);
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
    const active = recording?.kind === "hotkey" ? hotkeyButton : bindList.querySelector(".keys.recording");
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
quitButton.addEventListener("click", () => void quit());

async function start(): Promise<void> {
  [saved, overlayShown] = await Promise.all([backend.getSettings(), backend.isOverlayShown()]);
  draft = structuredClone(saved);
  render();
}

void start();
