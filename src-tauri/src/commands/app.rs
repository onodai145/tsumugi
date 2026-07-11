//! アプリ情報系コマンド。

/// ビルド時の短縮gitコミットハッシュ（`build.rs` が `TSUMUGI_GIT_HASH` として埋め込む）。
/// `.git` が無い環境(tarball等)からのビルドでは "unknown" になる。
#[tauri::command]
#[specta::specta]
pub fn git_commit_hash() -> String {
    env!("TSUMUGI_GIT_HASH").to_string()
}
