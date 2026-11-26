//! 内蔵 VOICEVOX CORE の取得・状態・削除のコマンド(Issue #625 段階2)。

use crate::error::{Error, Result};
use crate::events::SpeechCoreStatusChanged;
use crate::state::AppState;
use crate::tts::engine::{SpeechEngine, Voice};
use crate::tts::voicevox::catalog::{terms, terms_version};
use crate::tts::voicevox::packs::{Notify, PackKey, SpeechCoreInfo};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event as _;

/// 規約の確認画面に出す 2 つの規約の全文と、現行の版。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCoreTerms {
    pub version: String,
    pub voice_models: String,
    pub onnxruntime: String,
}

pub(crate) fn info(s: &AppState) -> SpeechCoreInfo {
    let mut i = s.speech_core.packs.snapshot();
    i.terms_accepted =
        s.settings.speech_core_terms_version().as_deref() == Some(i.terms_version.as_str());
    i
}

pub(crate) fn terms_core() -> SpeechCoreTerms {
    let t = terms();
    SpeechCoreTerms {
        version: terms_version(),
        voice_models: t.voice_models.replace('\r', ""),
        onnxruntime: t.onnxruntime.replace('\r', ""),
    }
}

/// 規約への同意を Rust 側で強制してから、取得を始める。同意は、開始できたあとに記録する。
pub(crate) fn start_core(
    s: &AppState,
    pack: &str,
    accepted_terms_version: &str,
    notify: Notify,
) -> Result<()> {
    if accepted_terms_version != terms_version() {
        return Err(Error::Invalid(
            "規約への同意が必要です(版が一致しません)".into(),
        ));
    }
    let key = PackKey::parse(pack)?;
    s.speech_core.packs.start(key, notify)?;
    // 取得はもう始まっている。同意の保存に失敗しても、取得は止めずに成功として返す(次回の取得の前に、また確認が出るだけ)。
    if let Err(e) = s.settings.set_speech_core_terms_version(&terms_version()) {
        log::warn!("speech core terms consent could not be saved: {e}");
    }
    Ok(())
}

pub(crate) async fn list_voices_core(s: &AppState) -> Result<Vec<Voice>> {
    s.speech_core.engine.voices().await
}

/// 状態が変わるたびに、スナップショットを Event で送る。
fn notify_for(app: AppHandle) -> Notify {
    Arc::new(move || {
        let state = app.state::<AppState>();
        let _ = SpeechCoreStatusChanged { info: info(&state) }.emit(&app);
    })
}

/// 内蔵エンジンの状態(エンジン一式・声ごと・対応可否・規約の版と同意済みか)。
#[tauri::command]
#[specta::specta]
pub async fn get_speech_core_info(state: State<'_, AppState>) -> Result<SpeechCoreInfo> {
    Ok(info(&state))
}

/// 規約の全文(確認画面用)。
#[tauri::command]
#[specta::specta]
pub async fn get_speech_core_terms() -> Result<SpeechCoreTerms> {
    Ok(terms_core())
}

/// 取得を始める。`pack` は `"engine"` か、声(VVM)の ID。`accepted_terms_version` が現行の版と一致しなければ拒否する。
/// 進捗は Event `SpeechCoreStatusChanged`。設定画面を閉じても続く。
#[tauri::command]
#[specta::specta]
pub async fn start_speech_core_download(
    app: AppHandle,
    state: State<'_, AppState>,
    pack: String,
    accepted_terms_version: String,
) -> Result<()> {
    start_core(&state, &pack, &accepted_terms_version, notify_for(app))
}

/// 実行中の取得を止める。途中までのファイルは残り、次回は続きから再開する。
#[tauri::command]
#[specta::specta]
pub async fn cancel_speech_core_download(state: State<'_, AppState>) -> Result<()> {
    state.speech_core.packs.cancel();
    Ok(())
}

/// 取得済みのものと、途中までのファイルを削除する。取得中は拒否する。
#[tauri::command]
#[specta::specta]
pub async fn delete_speech_core_pack(
    app: AppHandle,
    state: State<'_, AppState>,
    pack: String,
) -> Result<()> {
    state
        .speech_core
        .packs
        .delete(PackKey::parse(&pack)?)
        .await?;
    let _ = SpeechCoreStatusChanged { info: info(&state) }.emit(&app);
    Ok(())
}

/// 取得済みの声(設定画面・カラムの声の選択肢)。
#[tauri::command]
#[specta::specta]
pub async fn list_speech_core_voices(state: State<'_, AppState>) -> Result<Vec<Voice>> {
    list_voices_core(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SettingsStore;
    use crate::translate::download::ModelSpec;
    use crate::tts::voicevox::engine::CoreEngine;
    use crate::tts::voicevox::ffi::CoreError;
    use crate::tts::voicevox::packs::Packs;
    use crate::tts::voicevox::store::Store;
    use crate::tts::voicevox::test_util::{sha256_hex, temp_root};
    use crate::tts::voicevox::SpeechCore;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn noop() -> Notify {
        Arc::new(|| {})
    }

    fn unconfigured() -> AppState {
        AppState::new_for_test(SettingsStore::new_in_memory())
    }

    /// 声 `0` の取得先を、偽のサーバーに向けた状態。
    fn with_server(server: &MockServer, body: &[u8]) -> AppState {
        let store = Arc::new(Store::new(temp_root()));
        let engine = CoreEngine::new(
            store.clone(),
            None,
            Arc::new(|| Err(CoreError::Load("n/a".into()))),
        );
        let mut packs = Packs::new(
            store.clone(),
            crate::tts::voicevox::catalog::engine_assets_for("linux", "x86_64"),
            reqwest::Client::new(),
            engine.clone(),
        );
        packs.override_voice_spec(
            "0",
            ModelSpec {
                url: format!("{}/0.vvm", server.uri()),
                size: body.len() as u64,
                sha256: sha256_hex(body),
            },
        );
        let core = SpeechCore {
            store,
            engine,
            packs: Arc::new(packs),
        };
        unconfigured().with_speech_core(Arc::new(core))
    }

    async fn wait_idle(s: &AppState) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while s.speech_core.packs.is_running() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("取得が終わらない");
    }

    #[tokio::test]
    async fn a_wrong_or_empty_terms_version_is_rejected_and_nothing_is_recorded() {
        let s = unconfigured();
        for v in ["", "1999-01-01", "  ", &terms_version().to_uppercase()] {
            let r = start_core(&s, "0", v, noop());
            assert!(
                matches!(r, Err(Error::Invalid(ref m)) if m.contains("規約")),
                "{v:?}: {r:?}"
            );
        }
        assert_eq!(s.settings.speech_core_terms_version(), None);
    }

    #[tokio::test]
    async fn an_unknown_pack_and_an_unsupported_environment_are_rejected_without_recording_consent()
    {
        let s = unconfigured();
        assert!(start_core(&s, "nope", &terms_version(), noop()).is_err());
        assert!(
            start_core(&s, "s0", &terms_version(), noop()).is_err(),
            "歌声だけの VVM は取得できない"
        );
        match start_core(&s, "engine", &terms_version(), noop()) {
            Err(Error::Invalid(m)) => assert_eq!(m, crate::tts::ERR_UNSUPPORTED),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            s.settings.speech_core_terms_version(),
            None,
            "開始できなかったら同意を記録しない"
        );
    }

    #[tokio::test]
    async fn consent_is_recorded_after_a_successful_start_and_shown_in_the_info() {
        let server = MockServer::start().await;
        let body = vec![7u8; 2048];
        Mock::given(method("GET"))
            .and(path("/0.vvm"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let s = with_server(&server, &body);
        assert!(!info(&s).terms_accepted);

        start_core(&s, "0", &terms_version(), noop()).unwrap();
        wait_idle(&s).await;

        assert_eq!(
            s.settings.speech_core_terms_version().as_deref(),
            Some(terms_version().as_str())
        );
        let i = info(&s);
        assert!(i.terms_accepted);
        assert_eq!(
            i.voices[0].pack.status,
            crate::translate::model_store::LocalModelStatus::Ready
        );
    }

    #[test]
    fn an_older_consent_is_not_accepted_for_the_current_terms() {
        let s = unconfigured();
        s.settings
            .set_speech_core_terms_version("older-version")
            .unwrap();
        assert!(!info(&s).terms_accepted, "規約が変わったら、再同意が要る");
    }

    #[test]
    fn the_terms_command_returns_both_texts_and_the_current_version() {
        let t = terms_core();
        assert_eq!(t.version, terms_version());
        assert!(t.voice_models.contains("音声モデル") && t.onnxruntime.contains("ONNX Runtime"));
    }

    #[tokio::test]
    async fn the_unconfigured_engine_reports_unsupported_and_lists_no_voices() {
        let s = unconfigured();
        assert!(!info(&s).supported);
        assert!(list_voices_core(&s).await.unwrap().is_empty());
        let r = s
            .speech_core
            .engine
            .synthesize(
                "a",
                "3",
                &crate::tts::engine::SynthParams {
                    speed_scale: 1.0,
                    volume: 1.0,
                },
            )
            .await;
        assert!(
            matches!(r, Err(Error::Invalid(ref m)) if m == crate::tts::ERR_UNSUPPORTED),
            "{r:?}"
        );
    }

    #[test]
    fn the_engine_can_be_switched_to_the_built_in_core_with_no_url() {
        let s = unconfigured();
        let saved = crate::commands::speech::set_settings_core(
            &s,
            crate::domain::SpeechSettings {
                enabled: true,
                engine: crate::domain::SpeechEngineKind::VoicevoxCore,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(saved.engine, crate::domain::SpeechEngineKind::VoicevoxCore);
        assert_eq!(
            s.settings.load_speech().engine,
            crate::domain::SpeechEngineKind::VoicevoxCore
        );
    }
}
