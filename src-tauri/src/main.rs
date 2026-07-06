// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  // Linux(WebKitGTK) 既定: DMABUF レンダラを無効化する。有効だと Hyprland 等の wlroots 系
  // Wayland コンポジタでプロトコル不整合を起こし "Gdk Error 71 (protocol error)" で描画が
  // 落ちる。明示的な指定がある場合は尊重する。
  #[cfg(target_os = "linux")]
  if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
  }

  tsumugi_lib::run();
}
