//! 通知音を鳴らす Tauri コマンド(Issue #12: webview の AudioContext 自動再生ポリシーに
//! 左右されないよう、実際の再生は Rust 側(rodio)で行う)。

use crate::domain::MmlParseResult;
use crate::error::{Error, Result};
use crate::mml;
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::borrow::Cow;
use tauri::{AppHandle, State};
#[cfg(target_os = "android")]
use tauri_plugin_haptics::HapticsExt;

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
///
/// Android で、触覚チャンネル入りの ogg(`has_haptic_channel`)だけは、rodio ではなく
/// tauri-plugin-haptics の MediaPlayer で、音と一緒に振動させて鳴らす(Issue #592)。
/// それ以外(プリセット・MML・触覚なしの音、デスクトップのすべて)は今まで通り rodio。
#[tauri::command]
#[specta::specta]
pub async fn play_notify_sound(
    #[cfg_attr(not(target_os = "android"), allow(unused_variables))] app: AppHandle,
    state: State<'_, AppState>,
    choice: String,
) -> Result<()> {
    if choice.starts_with(mml::SOUND_CHOICE_PREFIX) {
        // フロント(ipc.ts)が mml: を data: に解決してから呼ぶ設計。ここに届くのは解決失敗の取りこぼし。
        log::warn!("通知音: MML がレンダリングされずに届いたため beep で代替します");
    }
    match resolve_audio_bytes(&choice) {
        Ok(bytes) => {
            let bytes = bytes.into_owned();
            #[cfg(target_os = "android")]
            {
                if crate::sound::has_haptic_channel(&bytes) {
                    play_haptic_audio(&app, &state, bytes).await;
                    return Ok(());
                }
            }
            state.sound.play(bytes);
        }
        // 注: この warn は `enable_file_logging` 設定(既定オフ)が有効な場合のみ
        // ファイルに残る。lib.rs がそれを条件にログプラグイン登録を行っているため。
        Err(e) => log::warn!("通知音: 再生対象の解決に失敗: {e}"),
    }
    Ok(())
}

/// 触覚入りの音を、tauri-plugin-haptics 経由で MediaPlayer で鳴らす(Android のみ)。
/// 触覚の ON/OFF は設定のハプティクス(`UiPrefs.haptics_enabled`)に従う(読めなければ既定の true)。
/// 失敗(Kotlin 側のエラー・タイムアウト)したら、通知を無音にしないよう beep で代替する。
#[cfg(target_os = "android")]
async fn play_haptic_audio(app: &AppHandle, state: &AppState, bytes: Vec<u8>) {
    let haptics_enabled = state
        .settings
        .load_ui()
        .map(|ui| ui.haptics_enabled)
        .unwrap_or(true);
    let handle = app.clone();
    // run_mobile_plugin は結果が返るまで同期でブロックするので、async のワーカーを塞がないよう別スレッドで呼ぶ
    let result = tauri::async_runtime::spawn_blocking(move || {
        handle.haptics().play_haptic_audio(&bytes, haptics_enabled)
    })
    .await;
    let failure = match result {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e.to_string(),
        Err(e) => e.to_string(),
    };
    log::warn!("通知音: 触覚入りの音の再生に失敗したため beep で代替します: {failure}");
    state.sound.play(PRESET_BEEP.to_vec());
}

/// MML を解釈して、時刻解決済みのノート列を返す(Issue #75)。構文エラーは Err ではなくデータで返す。
#[tauri::command]
#[specta::specta]
pub async fn parse_mml(text: String) -> MmlParseResult {
    match mml::parse(&text) {
        Ok(score) => MmlParseResult::Parsed { score },
        Err(error) => MmlParseResult::Invalid { error },
    }
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

    #[test]
    fn mml_choice_that_reaches_rust_falls_back_to_beep() {
        // フロントがレンダリングに失敗したときの最後の砦(通知を止めない)
        assert_eq!(
            resolve_audio_bytes("mml:c4d4").unwrap().as_ref(),
            resolve_audio_bytes("beep").unwrap().as_ref()
        );
    }

    #[tokio::test]
    async fn parse_mml_returns_data_for_both_outcomes() {
        use crate::domain::MmlParseResult;
        match super::parse_mml("c4".to_string()).await {
            MmlParseResult::Parsed { score } => assert_eq!(score.notes.len(), 1),
            other => panic!("expected Parsed, got {other:?}"),
        }
        match super::parse_mml("c4 x".to_string()).await {
            MmlParseResult::Invalid { error } => assert_eq!(error.position, 3),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
