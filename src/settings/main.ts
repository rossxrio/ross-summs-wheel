import "../styles/settings.css";

import { settingsBackend as backend } from "../platform/settings";
import { escapeHtml } from "../utils/html";
import { heldModifiers, hotkeyFromEvent, hotkeyLabels, isModifier } from "./keys";

/** How long "Saved" shows before the window closes. */
const CLOSE_DELAY_MS = 450;

const keysElement = document.getElementById("keys")!;
const hintElement = document.getElementById("hint")!;
const saveButton = document.getElementById("save") as HTMLButtonElement;

let current = "";
/** The captured hotkey, until it's saved or discarded. */
let draft: string | null = null;
let saving = false;

function render(labels: string[], pending = false): void {
  keysElement.innerHTML = labels
    .map((label) => `<kbd class="${pending ? "pending" : ""}">${escapeHtml(label)}</kbd>`)
    .join('<span class="plus">+</span>');
}

function hint(text: string, tone: "" | "error" | "ok" = ""): void {
  hintElement.textContent = text;
  hintElement.className = `hint ${tone}`;
}

function showDraft(): void {
  render(hotkeyLabels(draft ?? current));
  saveButton.disabled = draft === null;
  if (draft) hint("Enter to save · Esc to cancel");
  else hint("Press a new key");
}

async function save(): Promise<void> {
  if (!draft || saving) return;
  if (draft === current) return void backend.close();
  saving = true;
  saveButton.disabled = true;
  try {
    await backend.setHotkey(draft);
    current = draft;
    draft = null;
    keysElement.classList.add("saved");
    hint("Saved", "ok");
    window.setTimeout(() => void backend.close(), CLOSE_DELAY_MS);
  } catch (err) {
    saving = false;
    saveButton.disabled = false;
    hint(String(err), "error");
  }
}

window.addEventListener("keydown", (e) => {
  // Keys are hotkey candidates here, not shortcuts or button presses.
  e.preventDefault();
  if (saving || e.repeat) return;

  if (e.code === "Escape") return void backend.close();
  if (e.code === "Enter" || e.code === "NumpadEnter") return void save();

  if (isModifier(e.code)) {
    const mods = heldModifiers(e);
    if (mods.length) render([...mods, "…"], true);
    return;
  }

  draft = hotkeyFromEvent(e);
  showDraft();
});

window.addEventListener("keyup", (e) => {
  // Modifiers let go without a key: back to what was showing.
  if (!saving && isModifier(e.code) && heldModifiers(e).length === 0) showDraft();
});

saveButton.addEventListener("click", () => void save());
document.getElementById("close")!.addEventListener("click", () => void backend.close());

async function start(): Promise<void> {
  current = await backend.getHotkey();
  showDraft();
}

void start();
