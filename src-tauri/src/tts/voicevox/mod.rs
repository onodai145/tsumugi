//! 内蔵 VOICEVOX CORE(Issue #625 段階2)。設計は `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`。
//! ネイティブの呼び出し(`ffi::NativeCore`)は iOS 以外(デスクトップと Android)。ほかは全環境でビルドし、armv7・i686 の Android などでは `supported = false`。

pub mod archive;
pub mod catalog;
pub mod engine;
pub mod ffi;
pub mod packs;
pub mod store;
#[cfg(test)]
pub(crate) mod test_util;

use catalog::EngineAssets;
use engine::{BackendFactory, CoreEngine};
#[cfg_attr(target_os = "ios", allow(unused_imports))]
use ffi::{CoreBackend, CoreError};
use packs::Packs;
use std::path::PathBuf;
use std::sync::Arc;
use store::Store;

/// 内蔵エンジンの、置き場・エンジン本体・取得の状態機械をまとめたもの。`AppState.speech_core`。
pub struct SpeechCore {
    pub store: Arc<Store>,
    pub engine: CoreEngine,
    pub packs: Arc<Packs>,
}

impl SpeechCore {
    /// `root` は `app_data_dir/voicevox`。`http` は、固定の GitHub の URL だけに接続するダウンロード用のクライアント。
    pub fn new(root: PathBuf, http: reqwest::Client) -> Self {
        let assets = catalog::engine_assets();
        let store = Arc::new(Store::new(root));
        let engine = CoreEngine::new(
            store.clone(),
            assets.clone(),
            native_factory(store.clone(), assets.clone()),
        );
        let packs = Arc::new(Packs::new(store.clone(), assets, http, engine.clone()));
        Self {
            store,
            engine,
            packs,
        }
    }

    /// 非対応(テスト・`AppState::new` の既定)。何もディスクに書かず、取得もできない。
    pub fn unconfigured() -> Self {
        let store = Arc::new(Store::new(
            std::env::temp_dir().join("tsumugi-voicevox-unconfigured"),
        ));
        let factory: BackendFactory = Arc::new(|| Err(CoreError::Load("unconfigured".into())));
        let engine = CoreEngine::new(store.clone(), None, factory);
        let packs = Arc::new(Packs::new(
            store.clone(),
            None,
            reqwest::Client::new(),
            engine.clone(),
        ));
        Self {
            store,
            engine,
            packs,
        }
    }
}

#[cfg(not(target_os = "ios"))]
fn native_factory(store: Arc<Store>, assets: Option<EngineAssets>) -> BackendFactory {
    Arc::new(move || {
        let a = assets
            .as_ref()
            .ok_or_else(|| CoreError::Load("unsupported platform".into()))?;
        let core = ffi::NativeCore::load(
            &store.core_lib(a),
            &store.ort_lib(a),
            &store.dict_dir(),
            ffi::default_cpu_threads(),
        )?;
        Ok(Arc::new(core) as Arc<dyn CoreBackend>)
    })
}

#[cfg(target_os = "ios")]
fn native_factory(_store: Arc<Store>, _assets: Option<EngineAssets>) -> BackendFactory {
    Arc::new(|| Err(CoreError::Load("unsupported platform".into())))
}

#[cfg(all(test, desktop))]
mod real_tests {
    use super::*;
    use crate::translate::model_store::LocalModelStatus;
    use crate::tts::engine::{SpeechEngine, SynthParams};
    use packs::PackKey;
    use std::time::Duration;

    /// 実物の取得 → 展開 → ロード → 合成を、通しで行う。
    /// `TSUMUGI_TEST_CORE_ROOT` に、ディスク上の空のディレクトリ(`/tmp` ではない)を渡す。
    /// `TSUMUGI_TEST_CORE_ROOT=$HOME/.cache/tsumugi-voicevox-e2e cargo test --lib tts::voicevox::real_tests -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn real_end_to_end() {
        let root = PathBuf::from(
            std::env::var("TSUMUGI_TEST_CORE_ROOT")
                .expect("TSUMUGI_TEST_CORE_ROOT を設定してください"),
        );
        let core = SpeechCore::new(root, reqwest::Client::new());
        let notify: packs::Notify = Arc::new(|| {});
        for key in [PackKey::Engine, PackKey::Voice("0".into())] {
            if let Err(e) = core.packs.start(key, notify.clone()) {
                // 取得済み(前回の実行の残り)なら、そのまま進む。
                eprintln!("start skipped: {e:?}");
            }
            tokio::time::timeout(Duration::from_secs(600), async {
                while core.packs.is_running() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .expect("取得が終わらない");
        }
        let snap = core.packs.snapshot();
        assert_eq!(
            snap.engine.status,
            LocalModelStatus::Ready,
            "{:?}",
            snap.engine.status
        );
        assert_eq!(
            snap.voices[0].pack.status,
            LocalModelStatus::Ready,
            "{:?}",
            snap.voices[0].pack.status
        );

        let wav = core
            .engine
            .synthesize(
                "この音声は、ボイスボックスを使用して、出力されています。",
                "3",
                &SynthParams {
                    speed_scale: 1.0,
                    volume: 1.0,
                },
            )
            .await
            .unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert!(wav.len() > 100_000, "{}", wav.len());
        let fast = core
            .engine
            .synthesize(
                "この音声は、ボイスボックスを使用して、出力されています。",
                "3",
                &SynthParams {
                    speed_scale: 2.0,
                    volume: 1.0,
                },
            )
            .await
            .unwrap();
        assert!(
            fast.len() < wav.len() * 3 / 4,
            "話速 2.0 は、1.0 より十分短い: {} vs {}",
            fast.len(),
            wav.len()
        );
        let quiet = core
            .engine
            .synthesize(
                "あ",
                "3",
                &SynthParams {
                    speed_scale: 1.0,
                    volume: 0.1,
                },
            )
            .await
            .unwrap();
        let loud = core
            .engine
            .synthesize(
                "あ",
                "3",
                &SynthParams {
                    speed_scale: 1.0,
                    volume: 2.0,
                },
            )
            .await
            .unwrap();
        let peak = |w: &[u8]| {
            w[44..]
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs())
                .max()
                .unwrap_or(0)
        };
        assert!(
            peak(&quiet) < peak(&loud),
            "音量が効く: {} vs {}",
            peak(&quiet),
            peak(&loud)
        );
        assert!(!core.engine.voices().await.unwrap().is_empty());

        // 取得していない声は、エンジンが動いていても、`ERR_NOT_READY`。
        let r = core
            .engine
            .synthesize(
                "a",
                "14",
                &SynthParams {
                    speed_scale: 1.0,
                    volume: 1.0,
                },
            )
            .await;
        assert!(
            matches!(r, Err(crate::error::Error::Invalid(ref m)) if m == crate::tts::ERR_NOT_READY),
            "{r:?}"
        );
    }
}
