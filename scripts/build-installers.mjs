// Builds both Windows installers for a release into src-tauri/target/release/installers/:
//   Summs-Wheel_<version>_x64-setup.exe       small; downloads WebView2 only if the PC lacks it
//   Summs-Wheel_<version>_x64-full-setup.exe  WebView2 bundled, installs fully offline
// Tauri names both bundles the same, so each one is copied out before the next build.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync } from "node:fs";

const { version } = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const tauri = "node_modules/@tauri-apps/cli/tauri.js";
const bundle = `src-tauri/target/release/bundle/nsis/Summs Wheel_${version}_x64-setup.exe`;
const out = "src-tauri/target/release/installers";
mkdirSync(out, { recursive: true });

for (const [extraArgs, suffix] of [
  [[], "setup"],
  [["--config", "src-tauri/tauri.full.conf.json"], "full-setup"],
]) {
  execFileSync(process.execPath, [tauri, "build", ...extraArgs], { stdio: "inherit" });
  const dest = `${out}/Summs-Wheel_${version}_x64-${suffix}.exe`;
  copyFileSync(bundle, dest);
  console.log(`-> ${dest}`);
}
