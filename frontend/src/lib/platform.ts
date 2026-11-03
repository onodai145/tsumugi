import { platform } from "@tauri-apps/plugin-os";

const current = platform();

// スマホ(Android/iOS)実行時は投稿欄を常時表示せず、FABからモーダルで開く操作に切り替える。
export const isMobilePlatform = current === "android" || current === "ios";

// Linux(WebKitGTK)実行時だけ有効にする処理の判定(IME の候補位置補正など)。
export const isLinuxPlatform = current === "linux";
