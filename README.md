<div align="center">

# Summs Wheel

A radial wheel overlay for tracking enemy summoner spell cooldowns in League of Legends.

[![Download](https://img.shields.io/github/v/release/rossxrio/ross-summs-wheel?include_prereleases&style=for-the-badge&label=download&color=2ea44f)](https://github.com/rossxrio/ross-summs-wheel/releases)
[![Windows](https://img.shields.io/badge/windows-x64-0078D6?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/rossxrio/ross-summs-wheel/releases)
[![Buy me a coffee](https://img.shields.io/badge/buy%20me%20a%20coffee-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black)](https://buymeacoffee.com/rossxrio)

</div>

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
npm run app:release # both release installers, in src-tauri/target/release/installers
npm test            # parser + Rust tests
```

`app:release` builds a small installer that downloads WebView2 only if the PC lacks it, and
a `full-setup` one (~200 MB more) with WebView2 bundled that installs fully offline.

`src-tauri/.cargo/config.toml` sets the whisper.cpp compiler flags. Without them, MSVC
builds Whisper with no optimization and it runs about 20x slower. It also links the C
runtime into the exe, so players don't need the Visual C++ Redistributable.

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

---

<div align="center">

## ☕ Enjoying the wheel?

Every flash it catches is one you saw coming.<br>
If it won you a fight, fuel the next update.

<a href="https://buymeacoffee.com/rossxrio">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy me a coffee" height="60">
</a>

</div>
