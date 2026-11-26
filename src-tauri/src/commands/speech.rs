//! 読み上げのコマンド(Issue #625)。判断はすべて `*_core`(`AppState` だけに依存・Tauri なし)に置き、
//! `#[tauri::command]` はそれを呼ぶ薄い殻にする(`commands/mcp.rs` と同じ流儀)。

use crate::domain::speech::{QUEUE_LIMIT_DEFAULT, QUEUE_LIMIT_MAX};
use crate::domain::{ColumnSpeech, SpeechEngineKind, SpeechSettings};
use crate::error::{Error, Result};
use crate::state::AppState;
use crate::tts::engine::{SpeechEngine, SynthParams, Voice};
use crate::tts::external::ExternalEngine;
use crate::tts::manager::SpeechStatus;
use crate::tts::queue::SpeechItem;
use crate::tts::{invalid, ERR_NOT_CONFIGURED, ERR_NO_VOICE, ERR_TEXT_TOO_LONG};
use tauri::State;

/// 1件の読み上げ文の最大長(文字数)。フロントは 120 文字(+投稿者名)に切るので、これは不正な入力への備え。
pub const SPEECH_TEXT_MAX_CHARS: usize = 300;
/// 「試し聞き」を積むときの擬似カラム ID。実在のカラムの待ち項目とは混ざらない。
pub const TEST_COLUMN_ID: &str = "__test__";

pub(crate) fn get_settings_core(s: &AppState) -> SpeechSettings {
    s.settings.load_speech()
}

/// 検証・整形して保存する。オフにしたら、待ち行列を空にして再生中の読み上げも止める。
pub(crate) fn set_settings_core(
    s: &AppState,
    mut settings: SpeechSettings,
) -> Result<SpeechSettings> {
    settings.validate().map_err(Error::Invalid)?;
    settings.external_url = settings
        .external_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    settings.default_voice_id = settings.default_voice_id.trim().to_string();
    let saved = s.settings.update_speech(|v| *v = settings)?;
    if !saved.enabled {
        s.speech.stop_all();
        s.sound.stop_speech();
    }
    Ok(saved)
}

/// カラムの読み上げ設定を保存し、**丸めたあとの値**を返す(UI が保存された値を表示できるように)。
/// オフにしたら、そのカラムの待ち項目だけを消す。
pub(crate) fn set_column_speech_core(
    s: &AppState,
    column_id: &str,
    speech: ColumnSpeech,
) -> Result<ColumnSpeech> {
    let speech = speech.normalized();
    if !s.settings.set_column_speech(column_id, speech.clone())? {
        return Err(Error::NotFound(format!("column {column_id}")));
    }
    if !speech.enabled {
        s.speech.remove_column(column_id);
    }
    Ok(speech)
}

/// エンジンを使える設定か。外部は URL が要る。内蔵は URL を使わない(未取得・非対応は、合成のときに
/// `ERR_NOT_READY` / `ERR_UNSUPPORTED` になる)。
fn engine_is_configured(settings: &SpeechSettings) -> bool {
    settings.engine == SpeechEngineKind::VoicevoxCore || !settings.external_url.trim().is_empty()
}

fn params_of(settings: &SpeechSettings) -> SynthParams {
    SynthParams {
        speed_scale: settings.speed_scale,
        volume: settings.volume,
    }
}

/// 整形済みの読み上げ文を、キューに積む。全体・カラムのどちらかがオフなら、何もせず `Ok`
/// (設定を切り替えた直後の競合を許す)。
pub(crate) fn enqueue_note_core(
    s: &AppState,
    column_id: &str,
    note_id: &str,
    text: &str,
) -> Result<()> {
    let settings = s.settings.load_speech();
    let column = s
        .settings
        .load_column(column_id)
        .ok_or_else(|| Error::NotFound(format!("column {column_id}")))?;
    if !settings.enabled || !column.speech.enabled {
        return Ok(());
    }
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    if text.chars().count() > SPEECH_TEXT_MAX_CHARS {
        return Err(invalid(ERR_TEXT_TOO_LONG));
    }
    if !engine_is_configured(&settings) {
        return Err(invalid(ERR_NOT_CONFIGURED));
    }
    let voice_id = if column.speech.voice_id.is_empty() {
        settings.default_voice_id.clone()
    } else {
        column.speech.voice_id.clone()
    };
    if voice_id.is_empty() {
        return Err(invalid(ERR_NO_VOICE));
    }
    s.speech.enqueue(
        SpeechItem {
            note_id: note_id.to_string(),
            column_id: column_id.to_string(),
            text: text.to_string(),
            voice_id,
            params: params_of(&settings),
        },
        column.speech.queue_limit.clamp(1, QUEUE_LIMIT_MAX) as usize,
    );
    Ok(())
}

/// 設定画面の「試し聞き」。全体のスイッチがオフでも鳴らす(設定を確かめるためのボタンなので)。
pub(crate) fn test_speech_core(s: &AppState, text: &str) -> Result<()> {
    let settings = s.settings.load_speech();
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    if text.chars().count() > SPEECH_TEXT_MAX_CHARS {
        return Err(invalid(ERR_TEXT_TOO_LONG));
    }
    if !engine_is_configured(&settings) {
        return Err(invalid(ERR_NOT_CONFIGURED));
    }
    if settings.default_voice_id.is_empty() {
        return Err(invalid(ERR_NO_VOICE));
    }
    s.speech.enqueue(
        SpeechItem {
            // 毎回ちがう ID にして、キューの重複排除に弾かれないようにする。
            note_id: format!("test-{}", uuid::Uuid::new_v4()),
            column_id: TEST_COLUMN_ID.to_string(),
            text: text.to_string(),
            voice_id: settings.default_voice_id.clone(),
            params: params_of(&settings),
        },
        QUEUE_LIMIT_DEFAULT as usize,
    );
    Ok(())
}

// ---- Tauri コマンド(薄い殻) ----

#[tauri::command]
#[specta::specta]
pub async fn get_speech_settings(state: State<'_, AppState>) -> Result<SpeechSettings> {
    Ok(get_settings_core(&state))
}

#[tauri::command]
#[specta::specta]
pub async fn set_speech_settings(
    state: State<'_, AppState>,
    settings: SpeechSettings,
) -> Result<SpeechSettings> {
    set_settings_core(&state, settings)
}

#[tauri::command]
#[specta::specta]
pub async fn set_column_speech(
    state: State<'_, AppState>,
    column_id: String,
    speech: ColumnSpeech,
) -> Result<ColumnSpeech> {
    set_column_speech_core(&state, &column_id, speech)
}

/// 入力欄の URL(未保存でよい)のエンジンから、選べる声を取得する。設定画面の「接続テスト」。
#[tauri::command]
#[specta::specta]
pub async fn list_speech_voices(external_url: String) -> Result<Vec<Voice>> {
    ExternalEngine::new(&external_url)?.voices().await
}

/// フロントが整形した読み上げ文を依頼する。失敗しても、フロントは握りつぶす(通知音と同じ fire-and-forget)。
#[tauri::command]
#[specta::specta]
pub async fn speak_note(
    state: State<'_, AppState>,
    column_id: String,
    note_id: String,
    text: String,
) -> Result<()> {
    enqueue_note_core(&state, &column_id, &note_id, &text)
}

#[tauri::command]
#[specta::specta]
pub async fn test_speech(state: State<'_, AppState>, text: String) -> Result<()> {
    test_speech_core(&state, &text)
}

/// 待ち行列を空にして、再生中の読み上げを止める。
#[tauri::command]
#[specta::specta]
pub async fn stop_speech(state: State<'_, AppState>) -> Result<()> {
    state.speech.stop_all();
    state.sound.stop_speech();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_speech_status(state: State<'_, AppState>) -> Result<SpeechStatus> {
    Ok(state.speech.status())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Column, ColumnKind, ColumnSpeech, FilterQuery, SpeechSettings};
    use crate::store::SettingsStore;
    use crate::tts::{ERR_NOT_CONFIGURED, ERR_NO_VOICE, ERR_TEXT_TOO_LONG};

    fn column(id: &str, speech: ColumnSpeech) -> Column {
        Column {
            id: id.into(),
            account_id: "a1".into(),
            kind: ColumnKind::Home,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
            speech,
        }
    }

    fn state_with(settings: SpeechSettings, cols: Vec<Column>) -> AppState {
        let store = SettingsStore::new_in_memory();
        store.update_speech(|s| *s = settings).unwrap();
        for c in &cols {
            store.upsert_column(c).unwrap();
        }
        AppState::new_for_test(store)
    }

    fn on() -> SpeechSettings {
        SpeechSettings {
            enabled: true,
            external_url: "http://h:1".into(),
            default_voice_id: "3".into(),
            ..Default::default()
        }
    }
    fn col_on() -> ColumnSpeech {
        ColumnSpeech {
            enabled: true,
            ..Default::default()
        }
    }
    fn pending(s: &AppState) -> Vec<crate::tts::queue::SpeechItem> {
        std::iter::from_fn(|| s.speech.pop_for_command_test()).collect()
    }

    #[test]
    fn set_settings_validates_normalizes_and_saves() {
        let s = state_with(SpeechSettings::default(), vec![]);
        let bad = SpeechSettings {
            external_url: "ftp://x".into(),
            ..Default::default()
        };
        assert!(set_settings_core(&s, bad).is_err());

        let got = set_settings_core(
            &s,
            SpeechSettings {
                external_url: "  http://h:1/  ".into(),
                ..on()
            },
        )
        .unwrap();
        assert_eq!(
            got.external_url, "http://h:1",
            "前後の空白と末尾のスラッシュを落として保存する"
        );
        assert_eq!(s.settings.load_speech().external_url, "http://h:1");
    }

    #[test]
    fn turning_the_master_switch_off_clears_the_queue() {
        let s = state_with(on(), vec![column("c1", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "こんにちは").unwrap();
        set_settings_core(
            &s,
            SpeechSettings {
                enabled: false,
                ..on()
            },
        )
        .unwrap();
        assert!(pending(&s).is_empty());
    }

    #[test]
    fn set_column_speech_normalizes_and_clears_that_columns_queue_when_turned_off() {
        let s = state_with(on(), vec![column("c1", col_on()), column("c2", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        enqueue_note_core(&s, "c2", "n2", "b").unwrap();

        let saved = set_column_speech_core(
            &s,
            "c1",
            ColumnSpeech {
                enabled: false,
                queue_limit: 99,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            saved.queue_limit,
            crate::domain::speech::QUEUE_LIMIT_MAX,
            "丸めた値を返す(UI が保存された値を表示できるように)"
        );
        assert_eq!(s.settings.load_column("c1").unwrap().speech, saved);
        let left = pending(&s);
        assert_eq!(left.len(), 1);
        assert_eq!(
            left[0].note_id, "n2",
            "オフにしたカラムの待ち項目だけが消える"
        );
    }

    #[test]
    fn set_column_speech_on_a_missing_column_is_not_found() {
        let s = state_with(on(), vec![]);
        assert!(matches!(
            set_column_speech_core(&s, "nope", ColumnSpeech::default()),
            Err(crate::error::Error::NotFound(_))
        ));
    }

    #[test]
    fn enqueue_resolves_the_column_voice_then_falls_back_to_the_default_voice() {
        let s = state_with(
            on(),
            vec![
                column("c1", col_on()),
                column(
                    "c2",
                    ColumnSpeech {
                        voice_id: "9".into(),
                        ..col_on()
                    },
                ),
            ],
        );
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        enqueue_note_core(&s, "c2", "n2", "b").unwrap();
        let q = pending(&s);
        assert_eq!(q[0].voice_id, "3", "カラムの声が空なら既定の声");
        assert_eq!(q[1].voice_id, "9", "カラムの声が優先");
        assert_eq!(q[0].params.speed_scale, 1.0);
    }

    #[test]
    fn enqueue_is_a_silent_noop_when_speech_is_off_globally_or_for_the_column() {
        let s = state_with(
            SpeechSettings {
                enabled: false,
                ..on()
            },
            vec![column("c1", col_on())],
        );
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        let s2 = state_with(on(), vec![column("c1", ColumnSpeech::default())]);
        enqueue_note_core(&s2, "c1", "n1", "a").unwrap();
        assert!(pending(&s).is_empty());
        assert!(pending(&s2).is_empty());
    }

    #[test]
    fn enqueue_ignores_blank_text_and_rejects_overlong_text() {
        let s = state_with(on(), vec![column("c1", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "  \n  ").unwrap();
        assert!(pending(&s).is_empty());

        let long = "あ".repeat(SPEECH_TEXT_MAX_CHARS + 1);
        let err = enqueue_note_core(&s, "c1", "n2", &long).unwrap_err();
        assert!(err.to_string().contains(ERR_TEXT_TOO_LONG), "{err}");
        enqueue_note_core(&s, "c1", "n3", &"あ".repeat(SPEECH_TEXT_MAX_CHARS)).unwrap();
        assert_eq!(pending(&s).len(), 1, "上限ちょうどは通る");
    }

    #[test]
    fn enqueue_reports_a_missing_column_url_or_voice() {
        let s = state_with(on(), vec![]);
        assert!(matches!(
            enqueue_note_core(&s, "nope", "n", "a"),
            Err(crate::error::Error::NotFound(_))
        ));

        let s = state_with(
            SpeechSettings {
                external_url: String::new(),
                ..on()
            },
            vec![column("c1", col_on())],
        );
        let e = enqueue_note_core(&s, "c1", "n", "a").unwrap_err();
        assert!(e.to_string().contains(ERR_NOT_CONFIGURED), "{e}");

        let s = state_with(
            SpeechSettings {
                default_voice_id: String::new(),
                ..on()
            },
            vec![column("c1", col_on())],
        );
        let e = enqueue_note_core(&s, "c1", "n", "a").unwrap_err();
        assert!(e.to_string().contains(ERR_NO_VOICE), "{e}");
    }

    #[test]
    fn the_built_in_engine_needs_no_url_to_queue_notes_or_the_test_phrase() {
        // 実アプリの確認(Task 12)で、内蔵エンジンなのに ERR_NOT_CONFIGURED になった不具合の再現。
        let core = SpeechSettings {
            engine: crate::domain::SpeechEngineKind::VoicevoxCore,
            external_url: String::new(),
            ..on()
        };
        let s = state_with(core, vec![column("c1", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "こんにちは").unwrap();
        test_speech_core(&s, "テストです").unwrap();
        assert_eq!(pending(&s).len(), 2, "内蔵エンジンは URL が空でも積める");

        // 外部エンジンで URL が空なら、従来どおり未設定。
        let ext = SpeechSettings {
            external_url: String::new(),
            ..on()
        };
        let s = state_with(ext, vec![column("c1", col_on())]);
        assert!(test_speech_core(&s, "テストです")
            .unwrap_err()
            .to_string()
            .contains(ERR_NOT_CONFIGURED));
    }

    #[test]
    fn the_same_note_from_two_columns_is_queued_once() {
        let s = state_with(on(), vec![column("c1", col_on()), column("c2", col_on())]);
        enqueue_note_core(&s, "c1", "n1", "a").unwrap();
        enqueue_note_core(&s, "c2", "n1", "a").unwrap();
        assert_eq!(pending(&s).len(), 1);
    }

    #[test]
    fn the_column_limit_is_applied_per_column() {
        let s = state_with(
            on(),
            vec![column(
                "c1",
                ColumnSpeech {
                    queue_limit: 2,
                    ..col_on()
                },
            )],
        );
        for i in 0..4 {
            enqueue_note_core(&s, "c1", &format!("n{i}"), "a").unwrap();
        }
        let ids: Vec<_> = pending(&s).into_iter().map(|i| i.note_id).collect();
        assert_eq!(ids, ["n2", "n3"], "古い方から捨てる");
    }

    #[test]
    fn test_speech_queues_under_the_test_column_even_when_the_master_switch_is_off() {
        let s = state_with(
            SpeechSettings {
                enabled: false,
                ..on()
            },
            vec![],
        );
        test_speech_core(&s, "テストです").unwrap();
        let q = pending(&s);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].column_id, TEST_COLUMN_ID);
    }
}
