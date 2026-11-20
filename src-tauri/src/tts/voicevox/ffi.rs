//! VOICEVOX CORE の C API(公式の `libvoicevox_core`)を `libloading` で呼ぶ薄いラッパ(Issue #625 段階2)。
//! **`unsafe` はこのファイルに閉じ込める**。ネイティブの呼び出し(`NativeCore`)は `cfg(desktop)` だけ。
//! 背後の `CoreBackend` trait は全環境にあり、`engine.rs` のテストでは偽の実装に差し替える。
//! 関数の宣言は、公式ヘッダ `voicevox_core.h`(0.17.1)に合わせてある。

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum CoreError {
    /// ライブラリ・ONNX Runtime・辞書・VVM のロード、初期化の失敗(利用者には出さない。ログ用)。
    Load(String),
    /// クエリ作成・合成の失敗。
    Synthesis(String),
}

/// CORE の `Synthesizer` を包む。全メソッドは同期で、`spawn_blocking` から呼ぶ。
pub trait CoreBackend: Send + Sync + 'static {
    /// VVM をロードする。同じファイルがロード済みなら何もしない。
    fn load_voice_model(&self, vvm: &Path) -> Result<(), CoreError>;
    /// テキストから AudioQuery(JSON)を作る。
    fn audio_query(&self, text: &str, style_id: u32) -> Result<String, CoreError>;
    /// AudioQuery(JSON)から WAV のバイト列を作る。
    fn synthesis(&self, query_json: &str, style_id: u32) -> Result<Vec<u8>, CoreError>;
    /// ロード済みの VVM をすべて外す。
    fn unload_all(&self);
}

/// 推論のスレッド数。コア数の半分(最低 1)。CORE の既定(全コア)だと、合成中に UI が重くなるため。
pub fn default_cpu_threads() -> u16 {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    (cores / 2).clamp(1, u16::MAX as usize) as u16
}

#[cfg(desktop)]
pub use native::NativeCore;

#[cfg(desktop)]
mod native {
    use super::{CoreBackend, CoreError};
    use libloading::Library;
    use std::collections::HashMap;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[repr(C)]
    struct LoadOnnxruntimeOptions {
        filename: *const c_char,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct InitializeOptions {
        acceleration_mode: i32,
        cpu_num_threads: u16,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct LoadVoiceModelOptions {
        on_existing: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SynthesisOptions {
        enable_interrogative_upspeak: bool,
    }

    type Code = i32;
    const OK: Code = 0;
    /// `VOICEVOX_ACCELERATION_MODE_CPU`。
    const ACCELERATION_CPU: i32 = 1;

    /// ロードしたライブラリの関数。`Library` はプロセスの終わりまで外さない(下の `api`)ので、`'static` に持てる。
    struct Api {
        error_result_to_message: unsafe extern "C" fn(Code) -> *const c_char,
        onnxruntime_load_once: unsafe extern "C" fn(LoadOnnxruntimeOptions, *mut *const c_void) -> Code,
        open_jtalk_rc_new: unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> Code,
        open_jtalk_rc_delete: unsafe extern "C" fn(*mut c_void),
        synthesizer_new: unsafe extern "C" fn(*const c_void, *const c_void, InitializeOptions, *mut *mut c_void) -> Code,
        synthesizer_delete: unsafe extern "C" fn(*mut c_void),
        voice_model_file_open: unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> Code,
        voice_model_file_id: unsafe extern "C" fn(*const c_void, *mut [u8; 16]),
        voice_model_file_delete: unsafe extern "C" fn(*mut c_void),
        make_default_load_voice_model_options: unsafe extern "C" fn() -> LoadVoiceModelOptions,
        synthesizer_load_voice_model: unsafe extern "C" fn(*const c_void, *const c_void, LoadVoiceModelOptions) -> Code,
        synthesizer_unload_voice_model: unsafe extern "C" fn(*const c_void, *const [u8; 16]) -> Code,
        synthesizer_create_audio_query: unsafe extern "C" fn(*const c_void, *const c_char, u32, *mut *mut c_char) -> Code,
        make_default_synthesis_options: unsafe extern "C" fn() -> SynthesisOptions,
        synthesizer_synthesis: unsafe extern "C" fn(*const c_void, *const c_char, u32, SynthesisOptions, *mut usize, *mut *mut u8) -> Code,
        string_free: unsafe extern "C" fn(*mut c_char),
        bytes_free: unsafe extern "C" fn(*mut u8),
    }

    /// プロセスで 1 回だけロードする。失敗は覚えない(次の呼び出しで再試行する)。
    static API: Mutex<Option<&'static Api>> = Mutex::new(None);

    fn api(core_lib: &Path) -> Result<&'static Api, CoreError> {
        let mut slot = API.lock().unwrap();
        if let Some(a) = *slot {
            return Ok(a);
        }
        // SAFETY: 取得時に SHA-256 で検証した公式のライブラリを、アプリのデータディレクトリの固定の場所から読む。
        let lib = unsafe { Library::new(core_lib) }.map_err(|e| CoreError::Load(format!("library: {e}")))?;
        // プロセスの終わりまで外さない。CORE は ONNX Runtime を内部の static で持つので、外すと再ロードが壊れうる。
        let lib: &'static Library = Box::leak(Box::new(lib));
        macro_rules! sym {
            ($name:literal) => {
                // SAFETY: 宣言は、公式ヘッダ(0.17.1)の関数の型と一致させてある。
                *unsafe { lib.get(concat!($name, "\0").as_bytes()) }
                    .map_err(|e| CoreError::Load(format!("missing symbol {}: {e}", $name)))?
            };
        }
        let api = Api {
            error_result_to_message: sym!("voicevox_error_result_to_message"),
            onnxruntime_load_once: sym!("voicevox_onnxruntime_load_once"),
            open_jtalk_rc_new: sym!("voicevox_open_jtalk_rc_new"),
            open_jtalk_rc_delete: sym!("voicevox_open_jtalk_rc_delete"),
            synthesizer_new: sym!("voicevox_synthesizer_new"),
            synthesizer_delete: sym!("voicevox_synthesizer_delete"),
            voice_model_file_open: sym!("voicevox_voice_model_file_open"),
            voice_model_file_id: sym!("voicevox_voice_model_file_id"),
            voice_model_file_delete: sym!("voicevox_voice_model_file_delete"),
            make_default_load_voice_model_options: sym!("voicevox_make_default_load_voice_model_options"),
            synthesizer_load_voice_model: sym!("voicevox_synthesizer_load_voice_model"),
            synthesizer_unload_voice_model: sym!("voicevox_synthesizer_unload_voice_model"),
            synthesizer_create_audio_query: sym!("voicevox_synthesizer_create_audio_query"),
            make_default_synthesis_options: sym!("voicevox_make_default_synthesis_options"),
            synthesizer_synthesis: sym!("voicevox_synthesizer_synthesis"),
            string_free: sym!("voicevox_string_free"),
            bytes_free: sym!("voicevox_bytes_free"),
        };
        let api: &'static Api = Box::leak(Box::new(api));
        *slot = Some(api);
        Ok(api)
    }

    fn message(api: &Api, code: Code) -> String {
        // SAFETY: 戻り値は、CORE が持つ静的な NUL 終端の文字列(解放不要)。
        let p = unsafe { (api.error_result_to_message)(code) };
        if p.is_null() {
            return format!("code {code}");
        }
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }

    fn check(api: &Api, code: Code, what: &str, kind: fn(String) -> CoreError) -> Result<(), CoreError> {
        if code == OK {
            Ok(())
        } else {
            Err(kind(format!("{what}: {}", message(api, code))))
        }
    }

    fn c_path(p: &Path) -> Result<CString, CoreError> {
        p.to_str().and_then(|s| CString::new(s).ok()).ok_or_else(|| CoreError::Load("path is not valid UTF-8 or contains NUL".into()))
    }

    /// ロード済みの `Synthesizer`。**全メソッドは `state` の Mutex で直列化する**(下の `Send` / `Sync` の根拠)。
    pub struct NativeCore {
        api: &'static Api,
        open_jtalk: *mut c_void,
        synthesizer: *mut c_void,
        /// ロード済みの VVM(パス → モデル ID)。ロックは、すべての呼び出しの間、持ち続ける。
        state: Mutex<HashMap<PathBuf, [u8; 16]>>,
    }

    // SAFETY: 生ポインタ(`open_jtalk` と `synthesizer`)へのアクセスは、すべて `state` の Mutex を持った状態で行い、
    // `Drop` は他に参照がないときにだけ走る。
    unsafe impl Send for NativeCore {}
    unsafe impl Sync for NativeCore {}

    impl NativeCore {
        pub fn load(core_lib: &Path, ort_lib: &Path, dict_dir: &Path, cpu_threads: u16) -> Result<Self, CoreError> {
            let api = api(core_lib)?;
            let ort_path = c_path(ort_lib)?;
            let dict = c_path(dict_dir)?;
            let mut ort: *const c_void = std::ptr::null();
            // SAFETY: `ort_path` は、呼び出しの間生きている。`ort` は出力先。ONNX Runtime はプロセスで 1 度だけロードされ、以後は同じものが返る。
            let code = unsafe { (api.onnxruntime_load_once)(LoadOnnxruntimeOptions { filename: ort_path.as_ptr() }, &mut ort) };
            check(api, code, "onnxruntime", CoreError::Load)?;
            let mut open_jtalk: *mut c_void = std::ptr::null_mut();
            // SAFETY: `dict` は呼び出しの間生きている。
            let code = unsafe { (api.open_jtalk_rc_new)(dict.as_ptr(), &mut open_jtalk) };
            check(api, code, "dictionary", CoreError::Load)?;
            let mut synthesizer: *mut c_void = std::ptr::null_mut();
            let options = InitializeOptions { acceleration_mode: ACCELERATION_CPU, cpu_num_threads: cpu_threads };
            // SAFETY: `ort` と `open_jtalk` は、上の呼び出しで得た有効なポインタ。
            let code = unsafe { (api.synthesizer_new)(ort, open_jtalk, options, &mut synthesizer) };
            if let Err(e) = check(api, code, "synthesizer", CoreError::Load) {
                // SAFETY: `open_jtalk` は有効で、まだどこにも渡していない。
                unsafe { (api.open_jtalk_rc_delete)(open_jtalk) };
                return Err(e);
            }
            Ok(Self { api, open_jtalk, synthesizer, state: Mutex::new(HashMap::new()) })
        }
    }

    impl CoreBackend for NativeCore {
        fn load_voice_model(&self, vvm: &Path) -> Result<(), CoreError> {
            let mut loaded = self.state.lock().unwrap();
            if loaded.contains_key(vvm) {
                return Ok(());
            }
            let path = c_path(vvm)?;
            let mut model: *mut c_void = std::ptr::null_mut();
            // SAFETY: `path` は呼び出しの間生きている。
            let code = unsafe { (self.api.voice_model_file_open)(path.as_ptr(), &mut model) };
            check(self.api, code, "voice model", CoreError::Load)?;
            let mut id = [0u8; 16];
            // SAFETY: `model` は、上で開いた有効なハンドル。`id` は 16 バイトの出力先。
            unsafe { (self.api.voice_model_file_id)(model, &mut id) };
            // SAFETY: `synthesizer` と `model` は有効。
            let code = unsafe { (self.api.synthesizer_load_voice_model)(self.synthesizer, model, (self.api.make_default_load_voice_model_options)()) };
            // ハンドルは、ロードのあとは要らない(実測: 閉じても合成できる)。成否にかかわらず閉じる。
            // SAFETY: `model` は有効で、ここで 1 度だけ閉じる。
            unsafe { (self.api.voice_model_file_delete)(model) };
            check(self.api, code, "voice model", CoreError::Load)?;
            loaded.insert(vvm.to_path_buf(), id);
            Ok(())
        }

        fn audio_query(&self, text: &str, style_id: u32) -> Result<String, CoreError> {
            let _serial = self.state.lock().unwrap();
            let text = CString::new(text).map_err(|_| CoreError::Synthesis("text contains NUL".into()))?;
            let mut out: *mut c_char = std::ptr::null_mut();
            // SAFETY: `text` は呼び出しの間生きている。
            let code = unsafe { (self.api.synthesizer_create_audio_query)(self.synthesizer, text.as_ptr(), style_id, &mut out) };
            check(self.api, code, "audio query", CoreError::Synthesis)?;
            // SAFETY: 成功時の `out` は、CORE が確保した NUL 終端の UTF-8 文字列。コピーしてから `string_free` で解放する。
            let s = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
            unsafe { (self.api.string_free)(out) };
            Ok(s)
        }

        fn synthesis(&self, query_json: &str, style_id: u32) -> Result<Vec<u8>, CoreError> {
            let _serial = self.state.lock().unwrap();
            let q = CString::new(query_json).map_err(|_| CoreError::Synthesis("query contains NUL".into()))?;
            let mut len: usize = 0;
            let mut wav: *mut u8 = std::ptr::null_mut();
            // SAFETY: `q` は呼び出しの間生きている。
            let code = unsafe {
                (self.api.synthesizer_synthesis)(self.synthesizer, q.as_ptr(), style_id, (self.api.make_default_synthesis_options)(), &mut len, &mut wav)
            };
            check(self.api, code, "synthesis", CoreError::Synthesis)?;
            // SAFETY: 成功時の `wav` は、CORE が確保した `len` バイト。コピーしてから `bytes_free` で解放する。
            let bytes = unsafe { std::slice::from_raw_parts(wav, len) }.to_vec();
            unsafe { (self.api.bytes_free)(wav) };
            Ok(bytes)
        }

        fn unload_all(&self) {
            let mut loaded = self.state.lock().unwrap();
            for (_, id) in loaded.drain() {
                // SAFETY: `synthesizer` は有効で、`id` は、ロード時に CORE から得たモデル ID。失敗は無視する。
                let _ = unsafe { (self.api.synthesizer_unload_voice_model)(self.synthesizer, &id) };
            }
        }
    }

    impl Drop for NativeCore {
        fn drop(&mut self) {
            self.unload_all();
            // SAFETY: どちらも有効で、ここで 1 度だけ解放する(他に参照はない)。
            unsafe {
                (self.api.synthesizer_delete)(self.synthesizer);
                (self.api.open_jtalk_rc_delete)(self.open_jtalk);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_thread_count_is_at_least_one_and_at_most_half_of_the_cores() {
        let n = default_cpu_threads() as usize;
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        assert!(n >= 1);
        assert!(n <= cores.max(2) / 2 + 1, "{n} / {cores}");
    }
}

/// 実物の公式ライブラリで合成する(Linux x86_64 のみ)。`TSUMUGI_TEST_PROBE` に、Task 1 の Step 4 で用意した
/// ディレクトリ(`$HOME/.cache/tsumugi-voicevox-probe`)を渡す。
/// `TSUMUGI_TEST_PROBE=$HOME/.cache/tsumugi-voicevox-probe cargo test --lib tts::voicevox::ffi -- --ignored --nocapture`
#[cfg(all(test, desktop, target_os = "linux", target_arch = "x86_64"))]
mod real_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[ignore]
    fn native_core_loads_and_synthesizes_a_real_wav() {
        let p = PathBuf::from(std::env::var("TSUMUGI_TEST_PROBE").expect("TSUMUGI_TEST_PROBE を設定してください"));
        let core = NativeCore::load(
            &p.join("core/voicevox_core-linux-x64-0.17.1/lib/libvoicevox_core.so"),
            &p.join("ort/voicevox_onnxruntime-linux-x64-1.17.3/lib/libvoicevox_onnxruntime.so.1.17.3"),
            &p.join("dict/open_jtalk_dic_utf_8-1.11"),
            default_cpu_threads(),
        )
        .expect("load");
        let vvm = p.join("0.vvm");
        core.load_voice_model(&vvm).expect("load vvm");
        core.load_voice_model(&vvm).expect("2 回目のロードは何もしない");

        let q = core.audio_query("この音声は、ボイスボックスを使用して、出力されています。", 3).expect("audio_query");
        assert!(q.contains("speedScale") && q.contains("volumeScale"), "{}", &q[..q.len().min(200)]);
        let wav = core.synthesis(&q, 3).expect("synthesis");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert!(wav.len() > 100_000, "{}", wav.len());
        let pcm = &wav[44..];
        assert!(pcm.chunks_exact(2).any(|c| i16::from_le_bytes([c[0], c[1]]).abs() > 1000), "無音ではない");

        // 外したあとは、そのスタイルを合成できない。入れ直せば、また合成できる。
        core.unload_all();
        assert!(core.synthesis(&q, 3).is_err());
        core.load_voice_model(&vvm).expect("reload");
        assert!(core.synthesis(&q, 3).is_ok());

        // 存在しない VVM・NUL を含む文でも、落ちずに Err を返す。
        assert!(core.load_voice_model(&p.join("missing.vvm")).is_err());
        assert!(core.audio_query("a\0b", 3).is_err());
    }
}
