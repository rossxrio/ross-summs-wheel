# Summs Wheel

A radial wheel overlay for tracking enemy summoner spell cooldowns in League of Legends.

## Building

Needs Node, Rust (MSVC toolchain) and, for voice commands (Whisper), two C/C++ tools:

```
winget install Kitware.CMake LLVM.LLVM
```

Open a new terminal afterwards so both are on `PATH` (if bindgen can't find libclang,
set `LIBCLANG_PATH=C:\Program Files\LLVM\bin`). Then:

```
npm install
npm run app:dev     # run
npm run app:build   # installer
npm test            # parser + Rust tests
```

`src-tauri/.cargo/config.toml` sets the whisper.cpp compiler flags. Without them, MSVC
builds Whisper with no optimization and it runs about 20x slower.

## Voice commands

Hold a talk key, say the call, let go: **"ADC no flash"**, **"Jinx flash"**,
**"jungle no smite"**, **"support exhaust, mid flash"**, **"undo"**.

- Turn it on in settings (tap the hotkey 3 times): pick a talk key, download a model, and
  use **Hold to test** to check what it hears.
- The talk key is watched, not taken. The game and Discord still get it, so it can be your
  Discord push-to-talk key: say it once, and your team and the app both hear it.
- Timers start from when you *said* the call, not when Whisper finished.
- Champion names work, and so does a spell only one enemy has ("smite down"). Anything
  unclear is rejected rather than guessed. The pill above the timers shows what was heard.
- The mic is open only while you're in a game (or settings is open), and audio never
  leaves the PC.

Models (English, CPU): Fast `tiny.en` (78 MB, ~0.5 s), Balanced `base.en` (148 MB, ~1 s),
Accurate `small.en` (488 MB, ~3 s). Measure on your machine with
`cargo run --release --example voice_bench -- <model.bin> <clips dir>`.
