//! 通知音を鳴らす Tauri コマンド(Issue #12: webview の AudioContext 自動再生ポリシーに
//! 左右されないよう、実際の再生は Rust 側(rodio)で行う)。

use crate::error::{Error, Result};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::borrow::Cow;
use tauri::State;

const PRESET_BEEP: &[u8] = include_bytes!("../../assets/sounds/beep.wav");
const PRESET_CHIME: &[u8] = include_bytes!("../../assets/sounds/chime.wav");
const PRESET_PING: &[u8] = include_bytes!("../../assets/sounds/ping.wav");
const PRESET_POP: &[u8] = include_bytes!("../../assets/sounds/pop.wav");

/// choice(プリセットID または data: URL) から実際に再生するバイト列を解決する。
/// 副作用を持たない純粋関数(単体テスト用に分離)。
pub(crate) fn resolve_audio_bytes(choice: &str) -> Result<Cow<'static, [u8]>> {
    if let Some(rest) = choice.strip_prefix("data:") {
        let comma = rest
            .find(',')
            .ok_or_else(|| Error::Invalid(format!("malformed data URL: {choice}")))?;
        let bytes = STANDARD
            .decode(&rest[comma + 1..])
            .map_err(|e| Error::Invalid(format!("failed to decode data URL: {e}")))?;
        return Ok(Cow::Owned(bytes));
    }
    Ok(Cow::Borrowed(match choice {
        "chime" => PRESET_CHIME,
        "ping" => PRESET_PING,
        "pop" => PRESET_POP,
        _ => PRESET_BEEP, // "beep" と空文字(既定)・未知の文字列はここに含む(JS版のdefault分岐と同じ)
    }))
}

/// 通知音を鳴らす。choice は プリセットID / data URL(カスタム音声)。
/// 失敗しても通知フロー全体を止めないため、常に Ok を返す(失敗はログのみ)。
#[tauri::command]
#[specta::specta]
pub async fn play_notify_sound(state: State<'_, AppState>, choice: String) -> Result<()> {
    match resolve_audio_bytes(&choice) {
        Ok(bytes) => state.sound.play(bytes.into_owned()),
        Err(e) => log::warn!("通知音: 再生対象の解決に失敗: {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_audio_bytes;

    #[test]
    fn resolves_known_presets() {
        assert!(!resolve_audio_bytes("beep").unwrap().is_empty());
        assert!(!resolve_audio_bytes("chime").unwrap().is_empty());
        assert!(!resolve_audio_bytes("ping").unwrap().is_empty());
        assert!(!resolve_audio_bytes("pop").unwrap().is_empty());
    }

    #[test]
    fn distinct_presets_have_distinct_bytes() {
        let beep = resolve_audio_bytes("beep").unwrap();
        let chime = resolve_audio_bytes("chime").unwrap();
        assert_ne!(beep.as_ref(), chime.as_ref());
    }

    #[test]
    fn empty_choice_defaults_to_beep() {
        assert_eq!(
            resolve_audio_bytes("").unwrap().as_ref(),
            resolve_audio_bytes("beep").unwrap().as_ref()
        );
    }

    #[test]
    fn unknown_preset_id_falls_back_to_beep() {
        assert_eq!(
            resolve_audio_bytes("not-a-preset").unwrap().as_ref(),
            resolve_audio_bytes("beep").unwrap().as_ref()
        );
    }

    #[test]
    fn decodes_data_url() {
        // "data:audio/wav;base64,aGVsbG8=" -> "hello"
        let got = resolve_audio_bytes("data:audio/wav;base64,aGVsbG8=").unwrap();
        assert_eq!(got.as_ref(), b"hello");
    }

    #[test]
    fn rejects_data_url_without_comma() {
        assert!(resolve_audio_bytes("data:audio/wav;base64").is_err());
    }

    #[test]
    fn rejects_invalid_base64() {
        assert!(resolve_audio_bytes("data:audio/wav;base64,not base64!!").is_err());
    }
}
