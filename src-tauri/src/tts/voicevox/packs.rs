//! エンジン一式・声(VVM)の取得・削除・状態(Issue #625 段階2)。同時に走る取得は 1 つ。
//! 進捗は `Notify` で知らせる(Tauri の Event への変換は `commands/speech_core.rs`。ここは Tauri に依存しない)。
//! 取得は `translate/download.rs` の `download()`(Range 再開・SHA-256 検証・キャンセル)を使う。

use super::archive::{extract_tgz, extract_zip, ExtractError, Limits, Select};
use super::catalog::{catalog, terms_version, vvm_url, Asset, EngineAssets};
use super::engine::CoreEngine;
use super::store::Store;
use crate::error::{Error, Result};
use crate::translate::download::{download, DownloadError, ModelSpec, Phase};
use crate::translate::model_store::{available_space, LocalModelStatus};
use crate::tts::{invalid, ERR_UNSUPPORTED};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 状態が変わったことを知らせる。呼ぶ側が `Packs::snapshot` を引き直す。
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// 進捗の通知の最小間隔(検証中と終了状態は間引かない)。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// 展開後のサイズの上限。公式の取得物(CORE 4〜6 MB、ONNX Runtime 12〜18 MB、辞書 約 103 MB)に対して、余裕を持たせる。
const CORE_LIMITS: Limits = Limits {
    max_file: 64 << 20,
    max_total: 64 << 20,
};
const ORT_LIMITS: Limits = Limits {
    max_file: 64 << 20,
    max_total: 64 << 20,
};
const DICT_LIMITS: Limits = Limits {
    max_file: 160 << 20,
    max_total: 192 << 20,
};
const ENGINE_ARCHIVES: [&str; 3] = ["core.zip", "ort.tgz", "dict.tgz"];

/// 取得済みのアーカイブが、期待どおりか(サイズとハッシュが一致するか)。
fn archive_matches(path: &Path, spec: &ModelSpec) -> bool {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == spec.size) {
        return false;
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(_) => return false,
        }
    }
    format!("{:x}", h.finalize()) == spec.sha256
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackKey {
    Engine,
    Voice(String),
}

impl PackKey {
    /// コマンドの引数から。`"engine"` か、カタログにある VVM の ID だけを受け付ける。
    pub fn parse(s: &str) -> Result<Self> {
        if s == "engine" {
            Ok(Self::Engine)
        } else if catalog().vvm(s).is_some() {
            Ok(Self::Voice(s.to_string()))
        } else {
            Err(Error::Invalid("不明な取得対象です".into()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub status: LocalModelStatus,
    #[specta(type = specta_typescript::Number)]
    pub size_bytes: u64,
    /// 途中まで取得済みで、再開できるファイルのバイト数(無ければ 0)。
    #[specta(type = specta_typescript::Number)]
    pub partial_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StyleInfo {
    /// スタイル ID の数値の文字列(声の ID と同じ)。
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CharacterInfo {
    pub name: String,
    /// クレジット表記(例 `VOICEVOX:ずんだもん`)。
    pub credit: String,
    pub styles: Vec<StyleInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoicePackInfo {
    pub id: String,
    pub pack: PackInfo,
    pub characters: Vec<CharacterInfo>,
}

/// 設定画面に出す、内蔵エンジンの状態の全体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeechCoreInfo {
    /// この環境が内蔵エンジンに対応しているか。
    pub supported: bool,
    /// 現行の規約の版(`start_speech_core_download` の引数に渡す)。
    pub terms_version: String,
    /// 現行の版に同意済みか。
    pub terms_accepted: bool,
    pub engine: PackInfo,
    pub voices: Vec<VoicePackInfo>,
}

enum PackError {
    Download(DownloadError),
    Extract(ExtractError),
}

impl PackError {
    fn reason(&self) -> String {
        match self {
            Self::Download(e) => e.reason(),
            Self::Extract(e) => e.reason(),
        }
    }

    fn log_tag(&self) -> String {
        match self {
            Self::Download(e) => e.log_tag(),
            Self::Extract(e) => e.log_tag().to_string(),
        }
    }
}

fn io_dl(e: std::io::Error) -> PackError {
    PackError::Download(DownloadError::Io(e.to_string()))
}

#[derive(Clone, Copy)]
enum Kind {
    Core,
    Ort,
    Dict,
}

struct Inner {
    /// 実行中の取得のキャンセルフラグ。`Some` の間は、新しい開始・削除を拒否する。
    running: Option<Arc<AtomicBool>>,
    /// 取得中・検証中・失敗の状態。無ければディスクの状態(Ready / NotInstalled)。
    transient: HashMap<PackKey, LocalModelStatus>,
    /// 削除中(アンロードの待ち〜ファイルの削除)。この間は、開始も 2 つ目の削除も拒否する。
    deleting: bool,
}

pub struct Packs {
    store: Arc<Store>,
    /// `None` は、対応しない環境。
    assets: Option<EngineAssets>,
    http: reqwest::Client,
    engine: CoreEngine,
    vvm_specs: HashMap<String, ModelSpec>,
    inner: Mutex<Inner>,
}

impl Packs {
    pub fn new(
        store: Arc<Store>,
        assets: Option<EngineAssets>,
        http: reqwest::Client,
        engine: CoreEngine,
    ) -> Self {
        let vvm_specs = catalog()
            .vvms
            .iter()
            .map(|v| {
                (
                    v.id.clone(),
                    ModelSpec {
                        url: vvm_url(&v.id),
                        size: v.size,
                        sha256: v.sha256.clone(),
                    },
                )
            })
            .collect();
        Self {
            store,
            assets,
            http,
            engine,
            vvm_specs,
            inner: Mutex::new(Inner {
                running: None,
                transient: HashMap::new(),
                deleting: false,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn override_voice_spec(&mut self, id: &str, spec: ModelSpec) {
        self.vvm_specs.insert(id.to_string(), spec);
    }

    #[cfg(test)]
    pub(crate) fn is_running(&self) -> bool {
        self.inner.lock().unwrap().running.is_some()
    }

    fn size_of(&self, key: &PackKey) -> u64 {
        match key {
            PackKey::Engine => self
                .assets
                .as_ref()
                .map_or(0, |a| a.core.size + a.ort.size + a.dict.size),
            PackKey::Voice(id) => self.vvm_specs.get(id).map_or(0, |s| s.size),
        }
    }

    fn voice_file(id: &str) -> String {
        format!("{id}.vvm")
    }

    fn is_ready(&self, key: &PackKey) -> bool {
        match key {
            PackKey::Engine => self
                .assets
                .as_ref()
                .is_some_and(|a| self.store.engine_ready(a)),
            PackKey::Voice(id) => self.vvm_specs.get(id).is_some_and(|s| {
                std::fs::metadata(self.store.vvm_path(id))
                    .is_ok_and(|m| m.is_file() && m.len() == s.size)
            }),
        }
    }

    fn pack_info(&self, key: &PackKey, g: &Inner) -> PackInfo {
        let status = match g.transient.get(key) {
            Some(t) => t.clone(),
            None if self.is_ready(key) => LocalModelStatus::Ready,
            None => LocalModelStatus::NotInstalled,
        };
        let partial_bytes = match key {
            PackKey::Engine => 0,
            PackKey::Voice(id) => self.vvm_specs.get(id).map_or(0, |s| {
                self.store.partial_bytes(&Self::voice_file(id), &s.sha256)
            }),
        };
        PackInfo {
            status,
            size_bytes: self.size_of(key),
            partial_bytes,
        }
    }

    /// 設定画面に出す状態の全体。`terms_accepted` は設定(`SettingsData`)にあるので、呼び出し側が埋める。
    pub fn snapshot(&self) -> SpeechCoreInfo {
        let g = self.inner.lock().unwrap();
        let voices = catalog()
            .vvms
            .iter()
            .map(|v| VoicePackInfo {
                id: v.id.clone(),
                pack: self.pack_info(&PackKey::Voice(v.id.clone()), &g),
                characters: v
                    .characters
                    .iter()
                    .map(|c| CharacterInfo {
                        name: c.name.clone(),
                        credit: c.credit.clone(),
                        styles: c
                            .styles
                            .iter()
                            .map(|s| StyleInfo {
                                id: s.id.to_string(),
                                name: s.name.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
        SpeechCoreInfo {
            supported: self.assets.is_some(),
            terms_version: terms_version(),
            terms_accepted: false,
            engine: self.pack_info(&PackKey::Engine, &g),
            voices,
        }
    }

    /// 取得を始める。同時に走るのは 1 つだけ。取得済みは拒否する。進捗は `notify` で知らせる。
    pub fn start(self: &Arc<Self>, key: PackKey, notify: Notify) -> Result<()> {
        if key == PackKey::Engine && self.assets.is_none() {
            return Err(invalid(ERR_UNSUPPORTED));
        }
        if let PackKey::Voice(id) = &key {
            // 非対応の環境では、使えない声を取得させない(UI にはボタンが出ないが、IPC からは呼べる)。
            if self.assets.is_none() {
                return Err(invalid(ERR_UNSUPPORTED));
            }
            if !self.vvm_specs.contains_key(id) {
                return Err(Error::Invalid("不明な取得対象です".into()));
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let total = self.size_of(&key);
        {
            let mut g = self.inner.lock().unwrap();
            if g.running.is_some() {
                return Err(Error::Invalid("すでにダウンロード中です".into()));
            }
            if g.deleting {
                return Err(Error::Invalid(
                    "削除中です。完了してからやり直してください".into(),
                ));
            }
            if self.is_ready(&key) {
                return Err(Error::Invalid("取得済みです".into()));
            }
            g.running = Some(cancel.clone());
            g.transient.insert(
                key.clone(),
                LocalModelStatus::Downloading {
                    downloaded: 0,
                    total,
                },
            );
        }
        notify();
        let this = self.clone();
        // `tauri::async_runtime::spawn` ではなく `tokio::spawn`: この型は Tauri に依存させない(`LocalModels` と同じ)。
        tokio::spawn(async move { this.run(key, cancel, notify).await });
        Ok(())
    }

    /// 実行中の取得を止める。途中までのファイルは残り、次回は続きから再開する。
    pub fn cancel(&self) {
        if let Some(c) = &self.inner.lock().unwrap().running {
            c.store(true, Ordering::SeqCst);
        }
    }

    async fn run(self: Arc<Self>, key: PackKey, cancel: Arc<AtomicBool>, notify: Notify) {
        let r = match &key {
            PackKey::Engine => self.run_engine(&cancel, &notify).await,
            PackKey::Voice(id) => self.run_voice(id, &cancel, &notify).await,
        };
        self.finish(&key, r, &notify);
    }

    fn finish(&self, key: &PackKey, r: std::result::Result<(), PackError>, notify: &Notify) {
        {
            let mut g = self.inner.lock().unwrap();
            g.running = None;
            match r {
                Ok(()) | Err(PackError::Download(DownloadError::Cancelled)) => {
                    // キャンセルは失敗ではない。.part は残る(再開できる)。ディスクの状態に戻す。
                    g.transient.remove(key);
                }
                Err(e) => {
                    // 失敗の種類だけをログに残す(URL・パスは含めない)。
                    log::warn!("speech core pack failed: {}", e.log_tag());
                    g.transient
                        .insert(key.clone(), LocalModelStatus::Failed { reason: e.reason() });
                }
            }
        }
        notify();
    }

    fn set_status(&self, key: &PackKey, status: LocalModelStatus, notify: &Notify) {
        self.inner
            .lock()
            .unwrap()
            .transient
            .insert(key.clone(), status);
        notify();
    }

    /// 1 つのファイルを取得して検証する。進捗は、`base` バイト取得済みの続きとして `total` に対して出す。
    #[allow(clippy::too_many_arguments)]
    async fn fetch(
        &self,
        key: &PackKey,
        spec: &ModelSpec,
        part: &Path,
        dest: &Path,
        base: u64,
        total: u64,
        cancel: &AtomicBool,
        notify: &Notify,
    ) -> std::result::Result<(), PackError> {
        let mut last = Instant::now() - PROGRESS_INTERVAL;
        download(
            &self.http,
            spec,
            part,
            dest,
            cancel,
            &available_space,
            &mut |p| {
                let status = match p {
                    Phase::Downloading { downloaded, .. } => LocalModelStatus::Downloading {
                        downloaded: base + downloaded,
                        total,
                    },
                    Phase::Verifying => LocalModelStatus::Verifying,
                };
                // 状態は常に更新し、通知だけ間引く(検証中は必ず通知する)。
                self.inner
                    .lock()
                    .unwrap()
                    .transient
                    .insert(key.clone(), status.clone());
                if matches!(status, LocalModelStatus::Verifying)
                    || last.elapsed() >= PROGRESS_INTERVAL
                {
                    last = Instant::now();
                    notify();
                }
            },
        )
        .await
        .map_err(PackError::Download)
    }

    async fn run_voice(
        &self,
        id: &str,
        cancel: &AtomicBool,
        notify: &Notify,
    ) -> std::result::Result<(), PackError> {
        let spec = self
            .vvm_specs
            .get(id)
            .cloned()
            .ok_or(PackError::Download(DownloadError::BadResponse))?;
        std::fs::create_dir_all(self.store.vvm_dir()).map_err(io_dl)?;
        std::fs::create_dir_all(self.store.downloads_dir()).map_err(io_dl)?;
        let part = self.store.part_path(&Self::voice_file(id), &spec.sha256);
        let dest = self.store.vvm_path(id);
        self.fetch(
            &PackKey::Voice(id.to_string()),
            &spec,
            &part,
            &dest,
            0,
            spec.size,
            cancel,
            notify,
        )
        .await
    }

    async fn run_engine(
        &self,
        cancel: &AtomicBool,
        notify: &Notify,
    ) -> std::result::Result<(), PackError> {
        let a = self
            .assets
            .clone()
            .ok_or(PackError::Download(DownloadError::BadResponse))?;
        let total = a.core.size + a.ort.size + a.dict.size;
        let tmp = self.store.engine_tmp_dir();
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(self.store.downloads_dir()).map_err(io_dl)?;
        let r = self
            .engine_steps(&a, total, &tmp, cancel, notify)
            .await
            .and_then(|()| self.commit_engine(&a, &tmp));
        match &r {
            // 完成したら、取得済みのアーカイブはもう要らない。
            Ok(()) => {
                for name in ENGINE_ARCHIVES {
                    let _ = std::fs::remove_file(self.store.downloads_dir().join(name));
                }
            }
            // 途中のディレクトリは残さない。取得済みのアーカイブと `.part` は残す(次の取得で、取り直さない)。
            Err(_) => {
                let _ = std::fs::remove_dir_all(&tmp);
            }
        }
        r
    }

    /// 3 つのアーカイブを順に取得して、`tmp` へ展開する。
    async fn engine_steps(
        &self,
        a: &EngineAssets,
        total: u64,
        tmp: &Path,
        cancel: &AtomicBool,
        notify: &Notify,
    ) -> std::result::Result<(), PackError> {
        let steps: [(&Asset, &str, Kind); 3] = [
            (&a.core, "core.zip", Kind::Core),
            (&a.ort, "ort.tgz", Kind::Ort),
            (&a.dict, "dict.tgz", Kind::Dict),
        ];
        let mut base = 0u64;
        for (asset, name, kind) in steps {
            let spec = ModelSpec {
                url: asset.url.clone(),
                size: asset.size,
                sha256: asset.sha256.clone(),
            };
            let part = self.store.part_path(name, &asset.sha256);
            let archive = self.store.downloads_dir().join(name);
            // 前回の取得で完了していたアーカイブは、取り直さない。
            let (a_path, a_spec) = (archive.clone(), spec.clone());
            let have = tokio::task::spawn_blocking(move || archive_matches(&a_path, &a_spec))
                .await
                .unwrap_or(false);
            if !have {
                self.fetch(
                    &PackKey::Engine,
                    &spec,
                    &part,
                    &archive,
                    base,
                    total,
                    cancel,
                    notify,
                )
                .await?;
            }
            self.set_status(&PackKey::Engine, LocalModelStatus::Verifying, notify); // 展開中
            let (archive_c, tmp_c) = (archive.clone(), tmp.to_path_buf());
            let (core_lib, ort_lib) = (a.core_lib, a.ort_lib);
            let r = tokio::task::spawn_blocking(move || match kind {
                Kind::Core => {
                    let entry = format!("lib/{core_lib}");
                    extract_zip(
                        &archive_c,
                        &tmp_c.join("core"),
                        Select::Files(&[(entry.as_str(), core_lib)]),
                        CORE_LIMITS,
                    )
                }
                Kind::Ort => {
                    let entry = format!("lib/{ort_lib}");
                    extract_tgz(
                        &archive_c,
                        &tmp_c.join("ort"),
                        Select::Files(&[(entry.as_str(), ort_lib)]),
                        ORT_LIMITS,
                    )
                }
                Kind::Dict => {
                    extract_tgz(&archive_c, &tmp_c.join("dict"), Select::All, DICT_LIMITS)
                }
            })
            .await
            .map_err(|e| PackError::Extract(ExtractError::Io(e.to_string())))?;
            if let Err(e) = r {
                // 展開できないアーカイブは、残しても同じ結果になるので捨てる(取り直しになる)。
                let _ = std::fs::remove_file(&archive);
                return Err(PackError::Extract(e));
            }
            base += asset.size;
            if cancel.load(Ordering::SeqCst) {
                return Err(PackError::Download(DownloadError::Cancelled));
            }
        }
        Ok(())
    }

    /// 展開が全部済んだ `tmp` に READY を置き、本来の場所へリネームする(これが完了の瞬間)。
    fn commit_engine(&self, a: &EngineAssets, tmp: &Path) -> std::result::Result<(), PackError> {
        // 必要なファイルがそろっていなければ、READY を置かずに失敗にする(取得は成功、状態は未取得のまま、を防ぐ)。
        let required = [
            ("core", a.core_lib),
            ("ort", a.ort_lib),
            ("dict", "sys.dic"),
        ];
        for (dir, name) in required {
            if !std::fs::metadata(tmp.join(dir).join(name)).is_ok_and(|m| m.is_file()) {
                return Err(PackError::Extract(ExtractError::Missing(name.to_string())));
            }
        }
        Store::write_ready(tmp).map_err(io_dl)?;
        let dir = self.store.engine_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::rename(tmp, &dir).map_err(io_dl)
    }

    /// 取得済みのものと、途中までのファイルを削除する。ロード済みなら先に解放する。
    /// 取得中と、別の削除の最中は拒否する(削除中は `start` も拒否される)。
    pub async fn delete(&self, key: PackKey) -> Result<()> {
        {
            let mut g = self.inner.lock().unwrap();
            if g.running.is_some() {
                return Err(Error::Invalid("ダウンロード中は削除できません".into()));
            }
            if g.deleting {
                return Err(Error::Invalid(
                    "削除中です。完了してからやり直してください".into(),
                ));
            }
            g.deleting = true;
        }
        // 削除の間は、合成にバックエンドを作り直させない(消えかけのファイルを読ませない)。
        self.engine.set_paused(true);
        self.engine.unload();
        // 辞書など大きなファイルの削除は、非同期のスレッドを止めないよう、専用のスレッドで行う。
        let (store, k) = (self.store.clone(), key.clone());
        let r = tokio::task::spawn_blocking(move || Self::delete_files(&store, &k))
            .await
            .unwrap_or_else(|_| Err(Error::Invalid("ファイルを削除できませんでした。アプリを再起動してから、もう一度お試しください".into())));
        self.engine.set_paused(false);
        let mut g = self.inner.lock().unwrap();
        g.deleting = false;
        if r.is_ok() {
            g.transient.remove(&key);
        }
        r
    }

    fn delete_files(store: &Store, key: &PackKey) -> Result<()> {
        let mut targets = vec![];
        let mut dirs = vec![];
        match key {
            PackKey::Engine => {
                dirs.extend([store.engine_dir(), store.engine_tmp_dir()]);
                for name in ENGINE_ARCHIVES {
                    targets.push(store.downloads_dir().join(name));
                    targets.extend(store.part_files(name));
                }
            }
            PackKey::Voice(id) => {
                targets.push(store.vvm_path(id));
                targets.extend(store.part_files(&Self::voice_file(id)));
            }
        }
        let fail = |kind: std::io::ErrorKind| {
            log::warn!("speech core files could not be deleted: {kind:?}");
            Error::Invalid(
                "ファイルを削除できませんでした。アプリを再起動してから、もう一度お試しください"
                    .into(),
            )
        };
        for d in dirs {
            match std::fs::remove_dir_all(&d) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(fail(e.kind())),
            }
        }
        for p in targets {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(fail(e.kind())),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::voicevox::catalog::{engine_assets_for, Asset};
    use crate::tts::voicevox::ffi::CoreError;
    use crate::tts::voicevox::test_util::{
        install_fake_engine, install_fake_vvm, make_tgz, make_tgz_with_symlink, make_zip,
        sha256_hex, temp_root,
    };
    use std::sync::atomic::AtomicUsize;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct T {
        packs: Arc<Packs>,
        store: Arc<Store>,
        notified: Arc<AtomicUsize>,
        notify: Notify,
    }

    fn engine_stub(store: &Arc<Store>) -> CoreEngine {
        CoreEngine::new(
            store.clone(),
            None,
            Arc::new(|| Err(CoreError::Load("n/a".into()))),
        )
    }

    fn build(assets: Option<EngineAssets>, voice: Option<(&str, ModelSpec)>) -> T {
        let store = Arc::new(Store::new(temp_root()));
        let mut packs = Packs::new(
            store.clone(),
            assets,
            reqwest::Client::new(),
            engine_stub(&store),
        );
        if let Some((id, spec)) = voice {
            packs.override_voice_spec(id, spec);
        }
        let notified = Arc::new(AtomicUsize::new(0));
        let n = notified.clone();
        let notify: Notify = Arc::new(move || {
            n.fetch_add(1, Ordering::SeqCst);
        });
        T {
            packs: Arc::new(packs),
            store,
            notified,
            notify,
        }
    }

    fn voice_spec(server: &MockServer, body: &[u8]) -> ModelSpec {
        ModelSpec {
            url: format!("{}/0.vvm", server.uri()),
            size: body.len() as u64,
            sha256: sha256_hex(body),
        }
    }

    async fn serve(server: &MockServer, p: &str, body: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path(p.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(server)
            .await;
    }

    async fn wait_idle(p: &Arc<Packs>) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while p.is_running() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("取得が終わらない");
    }

    fn engine_status(t: &T) -> LocalModelStatus {
        t.packs.snapshot().engine.status
    }

    fn voice_status(t: &T, id: &str) -> LocalModelStatus {
        t.packs
            .snapshot()
            .voices
            .into_iter()
            .find(|v| v.id == id)
            .unwrap()
            .pack
            .status
    }

    fn archives() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let core = make_zip(&[
            (
                "voicevox_core-linux-x64-0.17.1/lib/libvoicevox_core.so",
                b"CORE",
            ),
            ("voicevox_core-linux-x64-0.17.1/README.txt", b"r"),
        ]);
        let ort = make_tgz_with_symlink(
            &[
                (
                    "voicevox_onnxruntime-linux-x64-1.17.3/lib/libvoicevox_onnxruntime.so.1.17.3",
                    b"ORT",
                ),
                ("voicevox_onnxruntime-linux-x64-1.17.3/TERMS.txt", b"t"),
            ],
            Some((
                "voicevox_onnxruntime-linux-x64-1.17.3/lib/libvoicevox_onnxruntime.so",
                "libvoicevox_onnxruntime.so.1.17.3",
            )),
        );
        let dict = make_tgz(&[
            ("open_jtalk_dic_utf_8-1.11/sys.dic", b"SYS"),
            ("open_jtalk_dic_utf_8-1.11/char.bin", b"C"),
        ]);
        (core, ort, dict)
    }

    fn fake_assets(uri: &str, core: &[u8], ort: &[u8], dict: &[u8]) -> EngineAssets {
        let mut a = engine_assets_for("linux", "x86_64").unwrap();
        let asset = |p: &str, b: &[u8]| Asset {
            url: format!("{uri}/{p}"),
            size: b.len() as u64,
            sha256: sha256_hex(b),
        };
        a.core = asset("core.zip", core);
        a.ort = asset("ort.tgz", ort);
        a.dict = asset("dict.tgz", dict);
        a
    }

    async fn serve_engine(server: &MockServer, core: &[u8], ort: &[u8], dict: &[u8]) {
        serve(server, "/core.zip", core.to_vec()).await;
        serve(server, "/ort.tgz", ort.to_vec()).await;
        serve(server, "/dict.tgz", dict.to_vec()).await;
    }

    #[tokio::test]
    async fn a_voice_pack_downloads_verifies_and_becomes_ready() {
        let server = MockServer::start().await;
        let body: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        serve(&server, "/0.vvm", body.clone()).await;
        let t = build(
            engine_assets_for("linux", "x86_64"),
            Some(("0", voice_spec(&server, &body))),
        );
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::NotInstalled);

        t.packs
            .start(PackKey::Voice("0".into()), t.notify.clone())
            .unwrap();
        wait_idle(&t.packs).await;

        assert_eq!(voice_status(&t, "0"), LocalModelStatus::Ready);
        assert_eq!(std::fs::read(t.store.vvm_path("0")).unwrap(), body);
        assert!(t.store.part_files("0.vvm").is_empty(), ".part は残らない");
        assert!(
            t.notified.load(Ordering::SeqCst) >= 2,
            "開始と完了で通知する"
        );
    }

    #[tokio::test]
    async fn a_hash_mismatch_fails_and_leaves_nothing() {
        let server = MockServer::start().await;
        let body = vec![1u8; 2048];
        serve(&server, "/0.vvm", body.clone()).await;
        let mut spec = voice_spec(&server, &body);
        spec.sha256 = "a".repeat(64);
        let t = build(engine_assets_for("linux", "x86_64"), Some(("0", spec)));
        t.packs
            .start(PackKey::Voice("0".into()), t.notify.clone())
            .unwrap();
        wait_idle(&t.packs).await;
        assert!(matches!(
            voice_status(&t, "0"),
            LocalModelStatus::Failed { .. }
        ));
        assert!(!t.store.vvm_path("0").exists());
        assert!(t.store.part_files("0.vvm").is_empty());
    }

    #[tokio::test]
    async fn a_failed_pack_can_be_retried() {
        let server = MockServer::start().await;
        let body = vec![5u8; 1024];
        Mock::given(method("GET"))
            .and(path("/0.vvm"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        serve(&server, "/0.vvm", body.clone()).await;
        let t = build(
            engine_assets_for("linux", "x86_64"),
            Some(("0", voice_spec(&server, &body))),
        );
        t.packs
            .start(PackKey::Voice("0".into()), t.notify.clone())
            .unwrap();
        wait_idle(&t.packs).await;
        assert!(
            matches!(voice_status(&t, "0"), LocalModelStatus::Failed { ref reason } if reason.contains("500")),
            "{:?}",
            voice_status(&t, "0")
        );
        t.packs
            .start(PackKey::Voice("0".into()), t.notify.clone())
            .unwrap();
        wait_idle(&t.packs).await;
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::Ready);
    }

    #[tokio::test]
    async fn unknown_packs_a_second_start_and_an_already_ready_pack_are_rejected() {
        assert!(PackKey::parse("nope").is_err());
        assert!(PackKey::parse("../0").is_err());
        assert!(PackKey::parse("s0").is_err(), "歌声だけの VVM はない");
        assert_eq!(PackKey::parse("engine").unwrap(), PackKey::Engine);
        assert_eq!(PackKey::parse("n0").unwrap(), PackKey::Voice("n0".into()));

        let server = MockServer::start().await;
        let body = vec![9u8; 1024];
        Mock::given(method("GET"))
            .and(path("/0.vvm"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body.clone())
                    .set_delay(Duration::from_millis(400)),
            )
            .mount(&server)
            .await;
        let t = build(
            engine_assets_for("linux", "x86_64"),
            Some(("0", voice_spec(&server, &body))),
        );
        t.packs
            .start(PackKey::Voice("0".into()), t.notify.clone())
            .unwrap();
        assert!(
            t.packs
                .start(PackKey::Voice("0".into()), t.notify.clone())
                .is_err(),
            "同時に走るのは 1 つ"
        );
        assert!(
            t.packs.delete(PackKey::Voice("0".into())).await.is_err(),
            "取得中は削除できない"
        );
        wait_idle(&t.packs).await;
        assert!(
            t.packs
                .start(PackKey::Voice("0".into()), t.notify.clone())
                .is_err(),
            "取得済みは拒否"
        );
    }

    #[tokio::test]
    async fn cancel_returns_to_not_installed() {
        let server = MockServer::start().await;
        let body = vec![3u8; 1024];
        Mock::given(method("GET"))
            .and(path("/0.vvm"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body.clone())
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;
        let t = build(
            engine_assets_for("linux", "x86_64"),
            Some(("0", voice_spec(&server, &body))),
        );
        t.packs
            .start(PackKey::Voice("0".into()), t.notify.clone())
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(matches!(
            voice_status(&t, "0"),
            LocalModelStatus::Downloading { .. }
        ));
        t.packs.cancel();
        wait_idle(&t.packs).await;
        assert_eq!(
            voice_status(&t, "0"),
            LocalModelStatus::NotInstalled,
            "キャンセルは失敗ではない"
        );
    }

    #[tokio::test]
    async fn the_engine_pack_downloads_extracts_only_what_it_needs_and_commits() {
        let server = MockServer::start().await;
        let (core, ort, dict) = archives();
        serve_engine(&server, &core, &ort, &dict).await;
        let a = fake_assets(&server.uri(), &core, &ort, &dict);
        let t = build(Some(a.clone()), None);

        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;

        assert_eq!(
            engine_status(&t),
            LocalModelStatus::Ready,
            "{:?}",
            engine_status(&t)
        );
        assert!(t.store.engine_ready(&a));
        assert_eq!(std::fs::read(t.store.core_lib(&a)).unwrap(), b"CORE");
        assert_eq!(std::fs::read(t.store.ort_lib(&a)).unwrap(), b"ORT");
        assert_eq!(
            std::fs::read(t.store.dict_dir().join("sys.dic")).unwrap(),
            b"SYS"
        );
        assert!(
            !t.store.engine_dir().join("core/README.txt").exists(),
            "必要なファイル以外は取り出さない"
        );
        assert!(
            !t.store
                .engine_dir()
                .join("ort/libvoicevox_onnxruntime.so")
                .exists(),
            "シンボリックリンクは取り出さない"
        );
        assert!(!t.store.engine_tmp_dir().exists());
        assert!(
            std::fs::read_dir(t.store.downloads_dir())
                .unwrap()
                .next()
                .is_none(),
            "アーカイブも .part も残らない"
        );
    }

    #[tokio::test]
    async fn a_malicious_engine_archive_fails_and_leaves_no_directory() {
        let server = MockServer::start().await;
        let (core, ort, _) = archives();
        let evil = make_tgz(&[("top/ok.bin", b"x"), ("top/../evil.txt", b"E")]);
        serve_engine(&server, &core, &ort, &evil).await;
        let a = fake_assets(&server.uri(), &core, &ort, &evil);
        let t = build(Some(a.clone()), None);
        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        match engine_status(&t) {
            LocalModelStatus::Failed { reason } => assert!(reason.contains("安全"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(!t.store.engine_tmp_dir().exists() && !t.store.engine_dir().exists());
        assert!(
            !t.store.root().join("evil.txt").exists()
                && !t.store.engine_tmp_dir().join("evil.txt").exists()
        );
    }

    #[tokio::test]
    async fn an_engine_archive_missing_the_library_fails() {
        let server = MockServer::start().await;
        let (_, ort, dict) = archives();
        let core = make_zip(&[("voicevox_core-linux-x64-0.17.1/README.txt", b"r")]);
        serve_engine(&server, &core, &ort, &dict).await;
        let t = build(Some(fake_assets(&server.uri(), &core, &ort, &dict)), None);
        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert!(matches!(engine_status(&t), LocalModelStatus::Failed { .. }));
        assert!(!t.store.engine_dir().exists());
    }

    #[tokio::test]
    async fn a_voice_pack_is_rejected_where_the_engine_is_unsupported() {
        let t = build(None, None);
        match t.packs.start(PackKey::Voice("0".into()), t.notify.clone()) {
            Err(Error::Invalid(m)) => assert_eq!(m, ERR_UNSUPPORTED),
            other => panic!("{other:?}"),
        }
        assert!(t.store.part_files("0.vvm").is_empty() && !t.store.vvm_dir().exists());
    }

    #[tokio::test]
    async fn an_engine_archive_missing_a_required_file_fails_instead_of_looking_done() {
        let server = MockServer::start().await;
        let (core, ort, _) = archives();
        let dict = make_tgz(&[("open_jtalk_dic_utf_8-1.11/char.bin", b"C")]); // sys.dic がない
        serve_engine(&server, &core, &ort, &dict).await;
        let t = build(Some(fake_assets(&server.uri(), &core, &ort, &dict)), None);
        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert!(
            matches!(engine_status(&t), LocalModelStatus::Failed { .. }),
            "{:?}",
            engine_status(&t)
        );
        assert!(!t.store.engine_dir().exists() && !t.store.engine_tmp_dir().exists());
    }

    #[tokio::test]
    async fn a_retry_after_an_interruption_does_not_download_finished_archives_again() {
        let server = MockServer::start().await;
        let (core, ort, dict) = archives();
        let once = |p: &'static str, body: Vec<u8>| {
            Mock::given(method("GET"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .expect(1)
        };
        once("/core.zip", core.clone()).mount(&server).await;
        once("/ort.tgz", ort.clone()).mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/dict.tgz"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        serve(&server, "/dict.tgz", dict.clone()).await;
        let t = build(Some(fake_assets(&server.uri(), &core, &ort, &dict)), None);

        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert!(matches!(engine_status(&t), LocalModelStatus::Failed { .. }));
        t.packs.start(PackKey::Engine, t.notify.clone()).unwrap();
        wait_idle(&t.packs).await;
        assert_eq!(
            engine_status(&t),
            LocalModelStatus::Ready,
            "{:?}",
            engine_status(&t)
        );
        // core.zip / ort.tgz は 1 回ずつしか取得していない(`expect(1)` は、サーバーの drop 時に検証される)。
        assert!(
            std::fs::read_dir(t.store.downloads_dir())
                .unwrap()
                .next()
                .is_none(),
            "完了後はアーカイブも残らない"
        );
    }

    #[tokio::test]
    async fn the_engine_pack_is_rejected_where_unsupported() {
        let t = build(None, None);
        let snap = t.packs.snapshot();
        assert!(!snap.supported);
        assert_eq!(snap.engine.status, LocalModelStatus::NotInstalled);
        match t.packs.start(PackKey::Engine, t.notify.clone()) {
            Err(Error::Invalid(m)) => assert_eq!(m, ERR_UNSUPPORTED),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn delete_removes_the_files_and_returns_to_not_installed() {
        let a = engine_assets_for("linux", "x86_64").unwrap();
        let t = build(Some(a.clone()), None);
        install_fake_engine(&t.store, &a);
        install_fake_vvm(&t.store, catalog().vvm("0").unwrap());
        std::fs::create_dir_all(t.store.downloads_dir()).unwrap();
        std::fs::write(
            t.store
                .part_path("0.vvm", &catalog().vvm("0").unwrap().sha256),
            b"partial",
        )
        .unwrap();
        assert_eq!(engine_status(&t), LocalModelStatus::Ready);
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::Ready);

        t.packs.delete(PackKey::Voice("0".into())).await.unwrap();
        assert_eq!(voice_status(&t, "0"), LocalModelStatus::NotInstalled);
        assert!(!t.store.vvm_path("0").exists() && t.store.part_files("0.vvm").is_empty());
        assert_eq!(
            engine_status(&t),
            LocalModelStatus::Ready,
            "声を消してもエンジンは残る"
        );

        t.packs.delete(PackKey::Engine).await.unwrap();
        assert_eq!(engine_status(&t), LocalModelStatus::NotInstalled);
        assert!(!t.store.engine_dir().exists());
        t.packs.delete(PackKey::Engine).await.unwrap();
    }

    #[tokio::test]
    async fn the_snapshot_lists_every_voice_pack_with_its_characters_credits_and_size() {
        let t = build(engine_assets_for("linux", "x86_64"), None);
        let snap = t.packs.snapshot();
        assert!(snap.supported);
        assert_eq!(snap.terms_version, terms_version());
        assert!(!snap.terms_accepted, "同意は呼び出し側が埋める");
        assert_eq!(snap.engine.size_bytes, 1_457_717 + 6_765_720 + 23_646_843);
        assert_eq!(snap.voices.len(), 27);
        let v0 = &snap.voices[0];
        assert_eq!(v0.id, "0");
        assert_eq!(v0.pack.size_bytes, 59_308_488);
        assert_eq!(v0.characters[0].name, "四国めたん");
        assert_eq!(v0.characters[0].credit, "VOICEVOX:四国めたん");
        assert_eq!(
            v0.characters[0].styles[0],
            StyleInfo {
                id: "2".into(),
                name: "ノーマル".into()
            }
        );
    }
}
