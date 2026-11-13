/**
 * Short UI sounds, synthesized with Web Audio (no files to ship). Kept quiet
 * and mostly dry: they play over the game. Muted from the settings window.
 *
 * Each sound is built from layers: a short noise transient for the attack and
 * a tonal body. "I'm listening", "got it" and the spell alert are one soft
 * chime (A5, E6) played different ways; "didn't catch that" is muted knocks.
 */

/** Overall loudness, 0..1. */
const VOLUME = 0.35;
/** The audio device is let go this long after the last note ends. */
const IDLE_SUSPEND_MS = 1000;

let on = true;
let ctx: AudioContext | null = null;
let master: GainNode | null = null;
/** Set when Web Audio can't start (no output device): don't retry on every note. */
let broken = false;
let suspendTimer = 0;

export function setSoundsOn(next: boolean): void {
  on = next;
}

function audio(): { ctx: AudioContext; out: GainNode } | null {
  if (!on || broken) return null;
  try {
    if (!ctx) {
      ctx = new AudioContext();
      master = ctx.createGain();
      master.gain.value = VOLUME;
      master.connect(ctx.destination);
    }
    window.clearTimeout(suspendTimer);
    // Always, not only when state says "suspended": a suspend() still in flight
    // reads "running" and would land after these notes are scheduled.
    void ctx.resume();
    return { ctx, out: master! };
  } catch (err) {
    console.warn("sounds off, Web Audio unavailable:", err);
    broken = true;
    return null;
  }
}

/** Builds a sound into `out` starting at time `t`; returns its length in seconds. */
type Sound = (ac: BaseAudioContext, out: AudioNode, t: number) => number;

function play(sound: Sound): void {
  const a = audio();
  if (!a) return;
  // A few ms of lead so the first ramp isn't scheduled in the past.
  const length = sound(a.ctx, a.out, a.ctx.currentTime + 0.01) + 0.01;
  // Sounds are rare: suspend between them so the audio thread and device aren't held all game.
  suspendTimer = window.setTimeout(() => void ctx?.suspend(), length * 1000 + IDLE_SUSPEND_MS);
}

// ---- Layers ------------------------------------------------------------------

const SILENT = 0.0001;

/** Exponential attack/decay envelope on a fresh gain node. */
function envelope(ac: BaseAudioContext, t: number, peak: number, attack: number, decay: number): GainNode {
  const g = ac.createGain();
  g.gain.setValueAtTime(SILENT, t);
  g.gain.exponentialRampToValueAtTime(peak, t + attack);
  g.gain.exponentialRampToValueAtTime(SILENT, t + attack + decay);
  // Land on true zero so the oscillator never stops mid-wave (no click).
  g.gain.linearRampToValueAtTime(0, t + attack + decay + 0.005);
  return g;
}

const noiseBuffers = new WeakMap<BaseAudioContext, AudioBuffer>();

function noise(ac: BaseAudioContext): AudioBuffer {
  let buf = noiseBuffers.get(ac);
  if (!buf) {
    buf = ac.createBuffer(1, Math.ceil(ac.sampleRate * 0.25), ac.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    noiseBuffers.set(ac, buf);
  }
  return buf;
}

/** A filtered noise burst: the "tick" that gives a note its attack. */
function transient(ac: BaseAudioContext, out: AudioNode, t: number, freq: number, gain: number, decay = 0.012): void {
  const src = ac.createBufferSource();
  src.buffer = noise(ac);
  const bp = ac.createBiquadFilter();
  bp.type = "bandpass";
  bp.frequency.value = freq;
  bp.Q.value = 1.2;
  src
    .connect(bp)
    .connect(envelope(ac, t, gain, 0.001, decay))
    .connect(out);
  src.start(t);
  src.stop(t + decay + 0.02);
}

/**
 * An FM bell: a sine whose brightness comes from a modulator that dies away
 * faster than the note, so it strikes bright and rings out pure. A whole-number
 * `ratio` (1, 2) keeps it soft and glassy; `index` sets how bright the strike is.
 */
function bell(
  ac: BaseAudioContext,
  out: AudioNode,
  t: number,
  o: { freq: number; decay: number; gain: number; ratio: number; index: number },
): void {
  const car = ac.createOscillator();
  const mod = ac.createOscillator();
  const depth = ac.createGain();
  car.frequency.value = o.freq;
  mod.frequency.value = o.freq * o.ratio;
  const peak = o.index * o.freq * o.ratio;
  depth.gain.setValueAtTime(peak, t);
  depth.gain.exponentialRampToValueAtTime(peak * 0.02, t + o.decay * 0.35);
  mod.connect(depth).connect(car.frequency);
  car.connect(envelope(ac, t, o.gain, 0.003, o.decay)).connect(out);
  const end = t + o.decay + 0.02;
  for (const osc of [car, mod]) {
    osc.start(t);
    osc.stop(end);
  }
}

/** A muted knock: a sine that drops in pitch as it dies, like tapping wood. */
function knock(ac: BaseAudioContext, out: AudioNode, t: number, freq: number, gain: number, decay: number): void {
  const osc = ac.createOscillator();
  osc.frequency.setValueAtTime(freq * 1.25, t);
  osc.frequency.exponentialRampToValueAtTime(freq, t + 0.035);
  osc.connect(envelope(ac, t, gain, 0.002, decay)).connect(out);
  osc.start(t);
  osc.stop(t + decay + 0.02);
  transient(ac, out, t, freq * 3, gain * 0.35, 0.008);
}

function lowpass(ac: BaseAudioContext, out: AudioNode, freq: number): BiquadFilterNode {
  const f = ac.createBiquadFilter();
  f.type = "lowpass";
  f.frequency.value = freq;
  f.Q.value = 0.5;
  f.connect(out);
  return f;
}

// ---- Sounds ------------------------------------------------------------------

/**
 * Soft glassy notes, one after another: "got it", "I'm listening" and the
 * spell alert all share it, so they sound like one family.
 */
function chime(ac: BaseAudioContext, out: AudioNode, t: number, first: number, second?: number): number {
  const bus = lowpass(ac, out, 6000);
  transient(ac, bus, t, 5000, 0.06);
  bell(ac, bus, t, { freq: first, decay: 0.28, gain: 0.32, ratio: 1, index: 1.2 });
  if (second === undefined) return 0.28 + 0.03;
  bell(ac, bus, t + 0.065, { freq: second, decay: 0.42, gain: 0.3, ratio: 2, index: 0.9 });
  return 0.065 + 0.42 + 0.03;
}

/** Exported so the sounds can be rendered offline (OfflineAudioContext) and checked. */
export const sfx = {
  /** "Got it": two soft glassy notes rising a fifth (A5, E6). */
  understood: ((ac, out, t) => chime(ac, out, t, 880, 1319)) satisfies Sound,

  /** "I'm listening": the same chime going the other way, falling a fifth (E6, A5). */
  listening: ((ac, out, t) => chime(ac, out, t, 1319, 880)) satisfies Sound,

  /** "Didn't catch that": two muted knocks stepping down a minor third (E4, C#4). */
  notUnderstood: ((ac, out, t) => {
    const bus = lowpass(ac, out, 1800);
    knock(ac, bus, t, 330, 0.36, 0.11);
    knock(ac, bus, t + 0.1, 277, 0.33, 0.16);
    return 0.1 + 0.16 + 0.03;
  }) satisfies Sound,

  /** An enemy summoner spell is back up: one note of the same chime (E6). */
  spellUp: ((ac, out, t) => chime(ac, out, t, 1319)) satisfies Sound,
};

// ---- Voice -------------------------------------------------------------------

/** The talk key went down and the mic is open. */
export function voiceListening(state: "listening" | "thinking" | "idle"): void {
  if (state === "listening") play(sfx.listening);
}

/** One clip's answer: a rising "got it", or two muted knocks when nothing was understood. */
export function voiceResult(understood: boolean): void {
  play(understood ? sfx.understood : sfx.notUnderstood);
}

// ---- Timers ------------------------------------------------------------------

/** An enemy summoner spell is back up. */
export function playSpellUp(): void {
  play(sfx.spellUp);
}
