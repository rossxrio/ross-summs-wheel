import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;
const page = (file: string) => fileURLToPath(new URL(file, import.meta.url));

// Tauri expects a fixed dev port and doesn't want Vite clearing its Rust logs.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  // Two pages: the overlay and the settings window (src-tauri/src/settings.rs).
  build: {
    rolldownOptions: {
      input: { main: page("index.html"), settings: page("settings.html") },
    },
  },
});
