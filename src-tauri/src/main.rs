// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  // Linux(WebKitGTK) 既定: NVIDIA ドライバの explicit sync を無効化する。Wayland 環境で
  // WebKitGTK の DMABUF レンダラが NVIDIA ドライバの未対応バッファ形式を要求し
  // "Gdk Error 71 (protocol error)" を起こすことがあるが、この変数はパフォーマンスを落とさず
  // HW アクセラレーションを維持したまま回避できる（Tauri 公式ドキュメント推奨の第一選択肢）。
  // これで解決しない場合は WEBKIT_DISABLE_DMABUF_RENDERER=1 や GDK_BACKEND=x11 を手動で
  // 設定すること。明示的な指定がある場合は尊重する。
  #[cfg(target_os = "linux")]
  if std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
    std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
  }

  tsumugi_lib::run();
}
