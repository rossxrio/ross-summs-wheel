import "./styles/base.css";
import "./styles/wheel.css";
import "./styles/hud.css";

import { Store } from "./app/store";
import { roleItems, spellHint, spellItems, spellTitle } from "./app/wheel-content";
import { renderHud } from "./hud/hud";
import { backend } from "./platform/backend";
import { WheelController } from "./wheel/controller";
import { WheelRenderer } from "./wheel/renderer";

/** How often timers advance and the HUD redraws. */
const TICK_MS = 250;

const store = new Store();
const hudElement = document.getElementById("hud")!;
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
