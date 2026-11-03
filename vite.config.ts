import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// Tauri expects a fixed dev port and doesn't want Vite clearing its Rust logs.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
