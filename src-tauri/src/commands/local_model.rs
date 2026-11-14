//! ローカル翻訳のモデルの取得・状態・削除のコマンド（Issue #603）。

use crate::error::{Error, Result};
use crate::events::LocalModelStatusChanged;
use crate::state::AppState;
use crate::translate::local_models::Notify;
use crate::translate::model_store::{LocalModelStatus, LICENSE_URL, LICENSE_VERSION};
use crate::translate::{invalid, is_supported, lang, ERR_UNSUPPORTED};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_specta::Event as _;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelInfo {
    pub status: LocalModelStatus,
    #[specta(type = specta_typescript::Number)]
    pub size_bytes: u64,
    /// 現行のライセンス版（`start_local_model_download` の引数に渡す）。
    pub license_version: String,
    /// ライセンス全文（日本語）の写しの URL。
    pub license_url: String,
    /// 現行の版に同意済みか。
    pub license_accepted: bool,
    /// 途中まで取得済みで、再開できるファイルのバイト数（無ければ 0）。
    #[specta(type = specta_typescript::Number)]
    pub partial_bytes: u64,
    /// このビルド・プラットフォームがローカル翻訳に対応しているか。
    pub supported: bool,
}

pub(crate) fn info(s: &AppState) -> LocalModelInfo {
    LocalModelInfo {
        status: s.local_models.status(),
        size_bytes: s.local_models.size_bytes(),
        license_version: LICENSE_VERSION.into(),
        license_url: LICENSE_URL.into(),
        license_accepted: s.settings.local_model_license_version().as_deref() == Some(LICENSE_VERSION),
        partial_bytes: s.local_models.partial_bytes(),
        supported: is_supported(),
    }
}

/// ライセンスへの同意を Rust 側で強制してから、ダウンロードを始める。同意は、開始できたあとに記録する。
pub(crate) async fn start_core(s: &AppState, accepted_license_version: &str, notify: Notify) -> Result<()> {
    if accepted_license_version != LICENSE_VERSION {
        return Err(Error::Invalid("ライセンスへの同意が必要です（版が一致しません）".into()));
    }
    if !is_supported() {
        return Err(invalid(ERR_UNSUPPORTED));
    }
    s.local_models.start(notify)?;
    s.settings.set_local_model_license_version(LICENSE_VERSION)
}

pub(crate) fn is_lang_supported(lang: &str) -> bool {
    lang::to_plamo_lang(lang).is_some()
}

/// モデルの状態と、ダウンロードの前提（サイズ・ライセンス・対応可否）を返す。
#[tauri::command]
#[specta::specta]
pub async fn get_local_model_status(state: State<'_, AppState>) -> Result<LocalModelInfo> {
    Ok(info(&state))
}

/// モデルのダウンロードを始める。`accepted_license_version` が現行のライセンス版と一致しなければ拒否する。
/// 進捗は Event `LocalModelStatusChanged`。設定画面を閉じても続く。
#[tauri::command]
#[specta::specta]
pub async fn start_local_model_download(
    app: AppHandle,
    state: State<'_, AppState>,
    accepted_license_version: String,
) -> Result<()> {
    let notify: Notify = Arc::new(move |status| {
        let _ = LocalModelStatusChanged { status }.emit(&app);
    });
    start_core(&state, &accepted_license_version, notify).await
}

/// 実行中のダウンロードを止める。途中までのファイルは残り、次回は続きから再開する。
#[tauri::command]
#[specta::specta]
pub async fn cancel_local_model_download(state: State<'_, AppState>) -> Result<()> {
    state.local_models.cancel();
    Ok(())
}

/// 取得済みのモデルと、途中までのファイルを削除する。ダウンロード中は拒否する。
#[tauri::command]
#[specta::specta]
pub async fn delete_local_model(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.local_models.delete().await?;
    let _ = LocalModelStatusChanged { status: state.local_models.status() }.emit(&app);
    Ok(())
}

/// 翻訳先の言語コードが PLaMo の対応言語か（`pt` などは非対応）。
#[tauri::command]
#[specta::specta]
pub async fn is_translate_lang_supported(lang: String) -> Result<bool> {
    Ok(is_lang_supported(&lang))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SettingsStore;
    use crate::translate::download::ModelSpec;
    use crate::translate::local_models::LocalModels;
    use crate::translate::manager::LocalTranslate;
    use crate::translate::model_store::{LICENSE_URL, MODEL_SIZE_BYTES};
    use std::path::PathBuf;
    use std::time::Duration;

    fn noop_notify() -> Notify {
        Arc::new(|_| {})
    }

    fn state() -> AppState {
        AppState::new_for_test(SettingsStore::new_in_memory())
    }

    #[tokio::test]
    async fn a_wrong_license_version_is_rejected_and_nothing_is_recorded() {
        let s = state();
        let r = start_core(&s, "1999-01-01", noop_notify()).await;
        assert!(matches!(r, Err(Error::Invalid(ref m)) if m.contains("ライセンス")), "{r:?}");
        assert_eq!(s.settings.local_model_license_version(), None);
    }

    #[tokio::test]
    async fn an_empty_license_version_is_rejected() {
        let s = state();
        assert!(matches!(start_core(&s, "", noop_notify()).await, Err(Error::Invalid(_))));
        assert_eq!(s.settings.local_model_license_version(), None);
    }

    #[tokio::test]
    async fn the_right_version_on_an_unconfigured_state_reports_unsupported_and_is_not_recorded() {
        let s = state();
        let r = start_core(&s, LICENSE_VERSION, noop_notify()).await;
        assert!(matches!(r, Err(Error::Invalid(ref m)) if m == ERR_UNSUPPORTED), "{r:?}");
        assert_eq!(s.settings.local_model_license_version(), None, "開始できなかったら同意を記録しない");
    }

    #[cfg(all(feature = "local-translate", desktop))]
    #[tokio::test]
    async fn the_right_version_starts_and_records_the_acceptance() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use sha2::{Digest, Sha256};

        let data = vec![7u8; 10_000];
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone())).mount(&server).await;
        let spec = ModelSpec {
            url: format!("{}/m.gguf", server.uri()),
            size: data.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&data)),
        };
        let dir: PathBuf = std::env::temp_dir().join(format!("tsumugi_cmd_{}", uuid::Uuid::new_v4()));
        let models = Arc::new(LocalModels::with_spec(dir, reqwest::Client::new(), Arc::new(LocalTranslate::unconfigured()), spec));
        let s = state().with_local_models(models);

        assert!(!info(&s).license_accepted);
        start_core(&s, LICENSE_VERSION, noop_notify()).await.unwrap();
        assert_eq!(s.settings.local_model_license_version().as_deref(), Some(LICENSE_VERSION));
        assert!(info(&s).license_accepted);

        tokio::time::timeout(Duration::from_secs(20), async {
            while info(&s).status != LocalModelStatus::Ready {
                tokio::time::sleep(Duration::from_millis(10)).await
            }
        })
        .await
        .expect("モデルが Ready にならない");
    }

    #[test]
    fn info_reports_supported_size_license_and_acceptance() {
        let s = state();
        let i = info(&s);
        assert_eq!(i.license_version, LICENSE_VERSION);
        assert_eq!(i.license_url, LICENSE_URL);
        assert!(!i.license_accepted);
        assert_eq!(i.size_bytes, MODEL_SIZE_BYTES);
        assert_eq!(i.supported, is_supported());
        assert_eq!(i.status, LocalModelStatus::NotInstalled);
        assert_eq!(i.partial_bytes, 0);
        s.settings.set_local_model_license_version("1999-01-01").unwrap();
        assert!(!info(&s).license_accepted, "古い版の同意は、現行の同意ではない");
        s.settings.set_local_model_license_version(LICENSE_VERSION).unwrap();
        assert!(info(&s).license_accepted);
    }

    #[test]
    fn lang_support_follows_the_plamo_table() {
        assert!(is_lang_supported("ja"));
        assert!(is_lang_supported("en-US"));
        assert!(is_lang_supported("zh-TW"));
        assert!(!is_lang_supported("pt"));
        assert!(!is_lang_supported(""));
    }
}
