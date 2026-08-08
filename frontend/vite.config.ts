/// <reference types="vitest/config" />
import path from "node:path";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// Tauri 推奨の dev サーバ設定。
// host/port を IPv4(127.0.0.1) に固定し、strictPort でポートずれを防ぐ。
// （Vite 8 は既定で ::1(IPv6) に bind することがあり、webview の localhost=IPv4
//   解決と食い違って "connection refused" になるため明示する。tauri.conf の devUrl と一致させる）
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  clearScreen: false,
  // 依存を起動時に事前バンドルし、実行時の再最適化→フルリロードを防ぐ
  // （再最適化が vite-plugin-svelte の仮想CSSモジュール読込を壊すため）
  optimizeDeps: {
    include: [
      "@tauri-apps/api/core",
      "@tauri-apps/api/event",
      "@tauri-apps/plugin-opener",
      "@tauri-apps/plugin-dialog",
      "@tauri-apps/plugin-notification",
      "mfm-js",
    ],
  },
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
  test: {
    environment: "jsdom",
  },
  // vitest実行時、Svelteパッケージがサーバー向けビルドに解決され
  // mount()が使えなくなる(lifecycle_function_unavailable)ため、
  // テスト時のみ browser 条件で解決させる。
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, "./src/lib"),
    },
    ...(process.env.VITEST ? { conditions: ["browser"] } : undefined),
  },
});
