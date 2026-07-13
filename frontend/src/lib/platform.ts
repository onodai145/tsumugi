import { platform } from "@tauri-apps/plugin-os";

const current = platform();

// スマホ(Android/iOS)実行時は投稿欄を常時表示せず、FABからモーダルで開く操作に切り替える。
export const isMobilePlatform = current === "android" || current === "ios";
