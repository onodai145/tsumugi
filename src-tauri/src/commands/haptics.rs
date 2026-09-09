//! ハプティクス(振動)を発火する Tauri コマンド(Issue #26)。
//! 実体は tauri-plugin-haptics（Android実機のみ有効、デスクトップはno-op）。

use crate::error::{Error, Result};
use tauri::AppHandle;
pub use tauri_plugin_haptics::HapticPattern;
use tauri_plugin_haptics::HapticsExt;

/// 振動を発火する。失敗してもUXに影響しないため、呼び出し側(フロント)で例外を握りつぶす想定。
#[tauri::command]
#[specta::specta]
pub fn vibrate(app: AppHandle, pattern: HapticPattern) -> Result<()> {
    app.haptics()
        .vibrate(pattern)
        .map_err(|e| Error::Invalid(e.to_string()))
}
