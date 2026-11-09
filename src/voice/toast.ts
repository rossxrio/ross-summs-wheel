import { escapeHtml } from "../utils/html";

/** How long a result stays up. */
const RESULT_MS = 2600;
/** Give up waiting for Whisper after this long. */
const THINKING_MS = 8000;

/**
 * The small pill above the timer strip: "Listening" while the talk key is
 * held, then what was understood (or why nothing was).
 */
export class VoiceToast {
  private hideTimer = 0;

  constructor(private readonly root: HTMLElement) {}

  listening(on: boolean): void {
    if (on) this.show("listening", "Listening");
    else this.show("thinking", "…", THINKING_MS);
  }

  done(text: string): void {
    this.show("ok", text, RESULT_MS);
  }

  failed(problem: string, heard: string): void {
    const quote = heard.length > 42 ? `${heard.slice(0, 40)}…` : heard;
    this.show("failed", heard ? `${problem} · “${quote}”` : problem, RESULT_MS);
  }

  private show(state: string, text: string, hideAfterMs?: number): void {
    window.clearTimeout(this.hideTimer);
    this.root.className = state;
    this.root.innerHTML = `<span class="dot"></span>${escapeHtml(text)}`;
    this.root.hidden = false;
    if (hideAfterMs) this.hideTimer = window.setTimeout(() => (this.root.hidden = true), hideAfterMs);
  }
}
