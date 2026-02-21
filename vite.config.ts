import { defineConfig } from "vite";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        "action-menu": resolve(__dirname, "action-menu.html"),
        "confirm-dialog": resolve(__dirname, "confirm-dialog.html"),
        settings: resolve(__dirname, "settings.html"),
        "permission-prompt": resolve(__dirname, "permission-prompt.html"),
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
  },
});
