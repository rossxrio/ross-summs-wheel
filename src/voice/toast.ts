import type { VoiceListening } from "../platform/backend";
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

  listening(state: VoiceListening["state"]): void {
    if (state === "listening") this.show("listening", "Listening");
    else if (state === "thinking") this.show("thinking", "…", THINKING_MS);
    // A quick tap (often Discord push-to-talk): nothing was sent to Whisper.
    else this.hide();
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
    if (hideAfterMs) this.hideTimer = window.setTimeout(() => this.hide(), hideAfterMs);
  }

  private hide(): void {
    window.clearTimeout(this.hideTimer);
    this.root.hidden = true;
  }
}
