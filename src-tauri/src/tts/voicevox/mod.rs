//! 内蔵 VOICEVOX CORE(Issue #625 段階2)。設計は `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`。
//! ネイティブの呼び出し(`ffi::NativeCore`)だけ `cfg(desktop)`。ほかは全環境でビルドし、Android などでは `supported = false`。

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
#[cfg_attr(not(desktop), allow(unused_imports))]
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
        let engine = CoreEngine::new(store.clone(), assets.clone(), native_factory(store.clone(), assets.clone()));
        let packs = Arc::new(Packs::new(store.clone(), assets, http, engine.clone()));
        Self { store, engine, packs }
    }

    /// 非対応(テスト・`AppState::new` の既定)。何もディスクに書かず、取得もできない。
    pub fn unconfigured() -> Self {
        let store = Arc::new(Store::new(std::env::temp_dir().join("tsumugi-voicevox-unconfigured")));
        let factory: BackendFactory = Arc::new(|| Err(CoreError::Load("unconfigured".into())));
        let engine = CoreEngine::new(store.clone(), None, factory);
        let packs = Arc::new(Packs::new(store.clone(), None, reqwest::Client::new(), engine.clone()));
        Self { store, engine, packs }
    }
}

#[cfg(desktop)]
fn native_factory(store: Arc<Store>, assets: Option<EngineAssets>) -> BackendFactory {
    Arc::new(move || {
        let a = assets.as_ref().ok_or_else(|| CoreError::Load("unsupported platform".into()))?;
        let core = ffi::NativeCore::load(&store.core_lib(a), &store.ort_lib(a), &store.dict_dir(), ffi::default_cpu_threads())?;
        Ok(Arc::new(core) as Arc<dyn CoreBackend>)
    })
}

#[cfg(not(desktop))]
fn native_factory(_store: Arc<Store>, _assets: Option<EngineAssets>) -> BackendFactory {
    Arc::new(|| Err(CoreError::Load("unsupported platform".into())))
}
