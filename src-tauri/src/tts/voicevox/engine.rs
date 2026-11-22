//! 内蔵 VOICEVOX CORE の `SpeechEngine`(Issue #625 段階2)。声の ID(スタイル ID)から VVM を引き、
//! 必要なものだけをロードして合成する。ネイティブの呼び出しは `CoreBackend`(`ffi.rs`)の背後。

use super::catalog::{catalog, EngineAssets};
use super::ffi::{CoreBackend, CoreError};
use super::store::Store;
use crate::error::Result;
use crate::tts::engine::{SpeechEngine, SynthParams, Voice};
use crate::tts::{invalid, ERR_BAD_RESPONSE, ERR_ENGINE_LOAD, ERR_NOT_READY, ERR_NO_VOICE, ERR_UNSUPPORTED};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 使われなくなってから、ロード済みのモデルを解放するまで(翻訳モデルと同じ)。
/// Android は、合成のピークが約 500 MB(Pixel 8 Pro の実測)でメモリが厳しいので短くする。
#[cfg(target_os = "android")]
pub const IDLE_UNLOAD: Duration = Duration::from_secs(180);
#[cfg(not(target_os = "android"))]
pub const IDLE_UNLOAD: Duration = Duration::from_secs(600);
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// 汚染された Mutex(別のスレッドが panic した)でも、中身を使い続ける。解放・Drop の途中で再び panic しないため。
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub type BackendFactory = Arc<dyn Fn() -> std::result::Result<Arc<dyn CoreBackend>, CoreError> + Send + Sync>;

struct Inner {
    store: Arc<Store>,
    /// `None` は、対応しない環境。
    assets: Option<EngineAssets>,
    factory: BackendFactory,
    backend: Mutex<Option<Arc<dyn CoreBackend>>>,
    last_used: Mutex<Instant>,
    in_flight: AtomicUsize,
    /// 声の削除の間は、合成を始めさせない(消えかけのファイルから、バックエンドを作り直さないため)。
    paused: AtomicBool,
}

/// 合成の間だけ、解放を止める。
struct InFlight<'a>(&'a Inner);

impl<'a> InFlight<'a> {
    fn new(inner: &'a Inner) -> Self {
        inner.in_flight.fetch_add(1, Ordering::SeqCst);
        *lock(&inner.last_used) = Instant::now();
        Self(inner)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        *lock(&self.0.last_used) = Instant::now();
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Inner {
    /// ロード済みのバックエンドを返す。なければ作る。失敗は覚えない(次の呼び出しで再試行する)。
    fn backend(&self) -> Result<Arc<dyn CoreBackend>> {
        let mut slot = lock(&self.backend);
        if let Some(b) = slot.as_ref() {
            return Ok(b.clone());
        }
        let b = (self.factory)().map_err(|e| {
            // ログには失敗の種類だけを出す(ライブラリ・CORE のメッセージには、ホームディレクトリを含むパスが入りうる)。
            let _ = e;
            log::warn!("voicevox core could not be loaded");
            invalid(ERR_ENGINE_LOAD)
        })?;
        *slot = Some(b.clone());
        Ok(b)
    }
}

#[derive(Clone)]
pub struct CoreEngine {
    inner: Arc<Inner>,
}

impl CoreEngine {
    pub fn new(store: Arc<Store>, assets: Option<EngineAssets>, factory: BackendFactory) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                assets,
                factory,
                backend: Mutex::new(None),
                last_used: Mutex::new(Instant::now()),
                in_flight: AtomicUsize::new(0),
                paused: AtomicBool::new(false),
            }),
        }
    }

    /// 削除の間だけ `true` にする。`true` の間の合成は `ERR_NOT_READY` で、バックエンドも作らない。
    pub fn set_paused(&self, paused: bool) {
        self.inner.paused.store(paused, Ordering::SeqCst);
    }

    pub fn is_loaded(&self) -> bool {
        lock(&self.inner.backend).is_some()
    }

    /// ロード済みのモデルを解放する。ファイルを消す前に呼ぶ(Windows は使用中のファイルを消せない)。
    /// 合成の最中なら、その合成が終わったときに解放される(バックエンドの最後の参照が落ちるとき)。
    pub fn unload(&self) {
        let b = lock(&self.inner.backend).take();
        drop(b);
    }

    /// `idle` 以上使われていなければ解放する。合成の最中は解放しない。解放したら `true`。
    pub fn unload_if_idle(&self, now: Instant, idle: Duration) -> bool {
        // スロットのロックを持ったまま、合成の最中かを見る(合成の開始は `InFlight::new` → `backend()` の順なので、
        // 数えたあとに割り込まれて、始まったばかりの合成の足もとを外すことがない)。
        let mut slot = lock(&self.inner.backend);
        if self.inner.in_flight.load(Ordering::SeqCst) > 0 {
            return false;
        }
        let last = *lock(&self.inner.last_used);
        if slot.is_some() && now.saturating_duration_since(last) >= idle {
            *slot = None;
            true
        } else {
            false
        }
    }
}

/// 一定間隔でアイドルを見て、解放する。アプリの起動時に 1 つだけ動かす。
pub async fn run_idle_sweeper(engine: CoreEngine, idle: Duration, interval: Duration) {
    loop {
        tokio::time::sleep(interval).await;
        engine.unload_if_idle(Instant::now(), idle);
    }
}

fn core_err(e: CoreError) -> crate::error::Error {
    match e {
        CoreError::Load(_) => {
            log::warn!("voicevox core load error");
            invalid(ERR_ENGINE_LOAD)
        }
        CoreError::Synthesis(_) => {
            log::warn!("voicevox core synthesis error");
            invalid(ERR_BAD_RESPONSE)
        }
    }
}

/// AudioQuery に、話速と音量を設定する。
fn apply_params(query_json: &str, p: &SynthParams) -> Result<String> {
    let mut q: serde_json::Value = serde_json::from_str(query_json).map_err(|_| invalid(ERR_BAD_RESPONSE))?;
    let obj = q.as_object_mut().ok_or_else(|| invalid(ERR_BAD_RESPONSE))?;
    obj.insert("speedScale".into(), serde_json::json!(p.speed_scale));
    obj.insert("volumeScale".into(), serde_json::json!(p.volume));
    serde_json::to_string(&q).map_err(|_| invalid(ERR_BAD_RESPONSE))
}

#[async_trait]
impl SpeechEngine for CoreEngine {
    /// 取得済みの声だけ。
    async fn voices(&self) -> Result<Vec<Voice>> {
        let mut out = vec![];
        for v in &catalog().vvms {
            if !self.inner.store.vvm_ready(v) {
                continue;
            }
            for c in &v.characters {
                for s in &c.styles {
                    out.push(Voice { id: s.id.to_string(), label: format!("{}（{}）", c.name, s.name) });
                }
            }
        }
        Ok(out)
    }

    async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>> {
        // 声の ID は、スタイル ID の数値。空・数値でない・カタログにない ID は、ロードより前に弾く。
        let style: u32 = voice_id.trim().parse().map_err(|_| invalid(ERR_NO_VOICE))?;
        let (vvm, _, _) = catalog().voice(style).ok_or_else(|| invalid(ERR_NO_VOICE))?;
        let assets = self.inner.assets.as_ref().ok_or_else(|| invalid(ERR_UNSUPPORTED))?;
        if self.inner.paused.load(Ordering::SeqCst) || !self.inner.store.engine_ready(assets) || !self.inner.store.vvm_ready(vvm) {
            return Err(invalid(ERR_NOT_READY));
        }
        let vvm_path = self.inner.store.vvm_path(&vvm.id);
        let inner = self.inner.clone();
        let (text, p) = (text.to_string(), *p);
        // ロードも合成も CPU を使う同期処理なので、ランタイムを止めないよう、専用のスレッドで行う。
        let joined = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let _busy = InFlight::new(&inner);
            let backend = inner.backend()?;
            backend.load_voice_model(&vvm_path).map_err(core_err)?;
            let query = backend.audio_query(&text, style).map_err(core_err)?;
            let query = apply_params(&query, &p)?;
            backend.synthesis(&query, style).map_err(core_err)
        })
        .await;
        match joined {
            Ok(r) => r,
            // ネイティブ側の panic。アプリを落とさず、失敗として扱う(次の合成で、バックエンドを作り直す)。
            Err(_) => {
                self.unload();
                Err(invalid(ERR_ENGINE_LOAD))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::tts::voicevox::catalog::engine_assets_for;
    use crate::tts::voicevox::test_util::{install_fake_engine, install_fake_vvm, temp_root};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    #[derive(Default)]
    struct Fake {
        loaded: Mutex<Vec<PathBuf>>,
        queries: Mutex<Vec<String>>,
        fail_synthesis: AtomicBool,
        panic_in_synthesis: AtomicBool,
    }

    impl CoreBackend for Fake {
        fn load_voice_model(&self, vvm: &Path) -> std::result::Result<(), CoreError> {
            self.loaded.lock().unwrap().push(vvm.to_path_buf());
            Ok(())
        }
        fn audio_query(&self, text: &str, style_id: u32) -> std::result::Result<String, CoreError> {
            Ok(format!(r#"{{"speedScale":1.0,"volumeScale":1.0,"text":"{text}","style":{style_id}}}"#))
        }
        fn synthesis(&self, query_json: &str, _style_id: u32) -> std::result::Result<Vec<u8>, CoreError> {
            if self.panic_in_synthesis.load(Ordering::SeqCst) {
                panic!("boom");
            }
            if self.fail_synthesis.load(Ordering::SeqCst) {
                return Err(CoreError::Synthesis("fake".into()));
            }
            self.queries.lock().unwrap().push(query_json.to_string());
            Ok(b"RIFFfake".to_vec())
        }
        fn unload_all(&self) {}
    }

    struct Setup {
        engine: CoreEngine,
        fake: Arc<Fake>,
        factory_calls: Arc<AtomicUsize>,
        store: Arc<Store>,
    }

    fn setup(installed: bool) -> Setup {
        let store = Arc::new(Store::new(temp_root()));
        let assets = engine_assets_for("linux", "x86_64");
        if installed {
            install_fake_engine(&store, assets.as_ref().unwrap());
            install_fake_vvm(&store, catalog().vvm("0").unwrap());
        }
        let fake = Arc::new(Fake::default());
        let factory_calls = Arc::new(AtomicUsize::new(0));
        let (f, c) = (fake.clone(), factory_calls.clone());
        let factory: BackendFactory = Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(f.clone() as Arc<dyn CoreBackend>)
        });
        Setup { engine: CoreEngine::new(store.clone(), assets, factory), fake, factory_calls, store }
    }

    fn params() -> SynthParams {
        SynthParams { speed_scale: 1.5, volume: 0.8 }
    }

    fn code(r: Result<Vec<u8>>) -> String {
        match r {
            Err(Error::Invalid(m)) => m,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn it_synthesizes_with_the_speed_and_volume_applied() {
        let s = setup(true);
        let wav = s.engine.synthesize("こんにちは", "3", &params()).await.unwrap();
        assert_eq!(wav, b"RIFFfake");
        assert_eq!(*s.fake.loaded.lock().unwrap(), vec![s.store.vvm_path("0")], "声が入っている VVM をロードする");
        let q: serde_json::Value = serde_json::from_str(&s.fake.queries.lock().unwrap()[0]).unwrap();
        assert_eq!(q["speedScale"].as_f64().unwrap(), 1.5);
        assert!((q["volumeScale"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert_eq!(q["style"], 3);
        assert_eq!(q["text"], "こんにちは");
    }

    #[tokio::test]
    async fn a_bad_voice_id_is_no_voice_and_never_touches_the_backend() {
        let s = setup(true);
        for id in ["", "  ", "abc", "-1", "3.5", "99999999999", "99999"] {
            assert_eq!(code(s.engine.synthesize("a", id, &params()).await), crate::tts::ERR_NO_VOICE, "{id:?}");
        }
        assert_eq!(s.factory_calls.load(Ordering::SeqCst), 0, "ロードもしない");
    }

    #[tokio::test]
    async fn a_missing_engine_or_voice_model_is_not_ready() {
        let s = setup(false);
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_NOT_READY);
        // エンジン一式だけ取得済みで、その声の VVM がない。
        install_fake_engine(&s.store, &engine_assets_for("linux", "x86_64").unwrap());
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_NOT_READY);
        assert_eq!(s.factory_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_unsupported_platform_reports_unsupported() {
        let store = Arc::new(Store::new(temp_root()));
        let factory: BackendFactory = Arc::new(|| Err(CoreError::Load("never".into())));
        let engine = CoreEngine::new(store, None, factory);
        assert_eq!(code(engine.synthesize("a", "3", &params()).await), crate::tts::ERR_UNSUPPORTED);
        assert!(engine.voices().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_load_failure_is_reported_and_not_remembered() {
        let store = Arc::new(Store::new(temp_root()));
        let assets = engine_assets_for("linux", "x86_64");
        install_fake_engine(&store, assets.as_ref().unwrap());
        install_fake_vvm(&store, catalog().vvm("0").unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fake = Arc::new(Fake::default());
        let f = fake.clone();
        let factory: BackendFactory = Arc::new(move || {
            if c.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(CoreError::Load("first time fails".into()))
            } else {
                Ok(f.clone() as Arc<dyn CoreBackend>)
            }
        });
        let engine = CoreEngine::new(store, assets, factory);
        assert_eq!(code(engine.synthesize("a", "3", &params()).await), crate::tts::ERR_ENGINE_LOAD);
        assert!(engine.synthesize("a", "3", &params()).await.is_ok(), "次の合成で再試行する");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_synthesis_failure_is_a_bad_response_and_a_panic_does_not_escape() {
        let s = setup(true);
        s.fake.fail_synthesis.store(true, Ordering::SeqCst);
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_BAD_RESPONSE);
        s.fake.fail_synthesis.store(false, Ordering::SeqCst);
        s.fake.panic_in_synthesis.store(true, Ordering::SeqCst);
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_ENGINE_LOAD, "ネイティブ側の panic は Err にする");
        s.fake.panic_in_synthesis.store(false, Ordering::SeqCst);
        assert!(s.engine.synthesize("a", "3", &params()).await.is_ok(), "panic のあとも使える");
    }

    #[tokio::test]
    async fn voices_lists_only_the_installed_voice_models() {
        let s = setup(true);
        let voices = s.engine.voices().await.unwrap();
        assert!(voices.iter().any(|v| v.id == "3" && v.label == "ずんだもん（ノーマル）"));
        assert!(!voices.iter().any(|v| v.id == "14"), "VVM 1 は未取得");
        assert_eq!(voices.len(), catalog().vvm("0").unwrap().characters.iter().map(|c| c.styles.len()).sum::<usize>());
    }

    #[tokio::test]
    async fn a_paused_engine_reports_not_ready_and_never_loads_until_resumed() {
        let s = setup(true);
        s.engine.set_paused(true);
        assert_eq!(code(s.engine.synthesize("a", "3", &params()).await), crate::tts::ERR_NOT_READY);
        assert_eq!(s.factory_calls.load(Ordering::SeqCst), 0);
        s.engine.set_paused(false);
        assert!(s.engine.synthesize("a", "3", &params()).await.is_ok());
    }

    #[test]
    fn a_poisoned_lock_does_not_stop_the_engine_from_unloading() {
        let s = setup(true);
        let e = s.engine.clone();
        let _ = std::thread::spawn(move || {
            let _g = e.inner.backend.lock().unwrap();
            panic!("poison");
        })
        .join();
        s.engine.unload();
        assert!(!s.engine.is_loaded());
        assert!(!s.engine.unload_if_idle(Instant::now(), Duration::ZERO));
    }

    #[tokio::test]
    async fn the_backend_is_not_unloaded_while_a_synthesis_is_in_flight() {
        let s = setup(true);
        s.engine.synthesize("a", "3", &params()).await.unwrap();
        let busy = InFlight::new(&s.engine.inner);
        let idle = Duration::from_secs(1);
        assert!(!s.engine.unload_if_idle(Instant::now() + idle * 10, idle), "合成の最中は解放しない");
        drop(busy);
        assert!(s.engine.unload_if_idle(Instant::now() + idle * 10, idle));
    }

    #[tokio::test]
    async fn the_backend_is_unloaded_when_idle_and_loaded_again_on_demand() {
        let s = setup(true);
        let idle = Duration::from_secs(600);
        assert!(!s.engine.unload_if_idle(Instant::now() + idle * 2, idle), "まだ何もロードしていない");
        s.engine.synthesize("a", "3", &params()).await.unwrap();
        assert!(s.engine.is_loaded());
        assert!(!s.engine.unload_if_idle(Instant::now(), idle), "使った直後は解放しない");
        assert!(s.engine.unload_if_idle(Instant::now() + idle + Duration::from_secs(1), idle));
        assert!(!s.engine.is_loaded());
        s.engine.synthesize("a", "3", &params()).await.unwrap();
        assert_eq!(s.factory_calls.load(Ordering::SeqCst), 2, "必要になったら、また作る");
        s.engine.unload();
        assert!(!s.engine.is_loaded());
    }

    #[cfg(target_os = "android")]
    #[test]
    fn idle_unload_is_three_minutes_on_android() {
        assert_eq!(IDLE_UNLOAD, Duration::from_secs(180));
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn idle_unload_is_ten_minutes_on_desktop() {
        assert_eq!(IDLE_UNLOAD, Duration::from_secs(600));
    }
}
