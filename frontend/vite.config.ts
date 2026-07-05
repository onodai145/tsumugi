import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri 推奨の dev サーバ設定。
// host/port を IPv4(127.0.0.1) に固定し、strictPort でポートずれを防ぐ。
// （Vite 8 は既定で ::1(IPv6) に bind することがあり、webview の localhost=IPv4
//   解決と食い違って "connection refused" になるため明示する。tauri.conf の devUrl と一致させる）
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    host: host || "127.0.0.1",
    port: 5173,
    strictPort: true,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      // Rust 側の変更で vite が再読込しないよう除外
      ignored: ["**/src-tauri/**"],
    },
  },
});
