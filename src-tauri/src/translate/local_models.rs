//! モデルの取得・状態・削除の管理（Issue #603）。同時に走るダウンロードは1つ。進捗は `Notify` で通知する
//! （Tauri の Event への変換は `commands/local_model.rs`。ここは Tauri に依存しない）。

use super::download::{download, DownloadError, ModelSpec, Phase};
use super::manager::LocalTranslate;
use super::model_store::{available_space, model_path, part_path, status_on_disk, LocalModelStatus};
use super::{invalid, ERR_UNSUPPORTED};
use crate::error::{Error, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub type Notify = Arc<dyn Fn(LocalModelStatus) + Send + Sync>;

/// 進捗の通知の最小間隔（Verifying と終了状態は間引かない）。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

struct Inner {
    /// 実行中のダウンロードのキャンセルフラグ。`Some` の間は新しい開始・削除を拒否する。
    running: Option<Arc<AtomicBool>>,
    /// ダウンロード中・検証中・失敗の状態。`None` ならディスクの状態（Ready / NotInstalled）。
    transient: Option<LocalModelStatus>,
}

pub struct LocalModels {
    /// `None` は、ローカル翻訳に対応しない構成（`LocalTranslate::unconfigured` と同じ）。
    models_dir: Option<PathBuf>,
    http: reqwest::Client,
    translator: Arc<LocalTranslate>,
    spec: ModelSpec,
    inner: Mutex<Inner>,
}

impl LocalModels {
    pub fn unconfigured() -> Self {
        Self {
            models_dir: None,
            http: reqwest::Client::new(),
            translator: Arc::new(LocalTranslate::unconfigured()),
            spec: ModelSpec::official(),
            inner: Mutex::new(Inner { running: None, transient: None }),
        }
    }

    pub fn new(models_dir: PathBuf, http: reqwest::Client, translator: Arc<LocalTranslate>) -> Self {
        Self::with_spec(models_dir, http, translator, ModelSpec::official())
    }

    pub(crate) fn with_spec(models_dir: PathBuf, http: reqwest::Client, translator: Arc<LocalTranslate>, spec: ModelSpec) -> Self {
        Self { models_dir: Some(models_dir), http, translator, spec, inner: Mutex::new(Inner { running: None, transient: None }) }
    }

    pub fn size_bytes(&self) -> u64 {
        self.spec.size
    }

    pub(crate) fn is_running(&self) -> bool {
        self.inner.lock().unwrap().running.is_some()
    }

    pub fn status(&self) -> LocalModelStatus {
        let Some(dir) = &self.models_dir else { return LocalModelStatus::NotInstalled };
        let g = self.inner.lock().unwrap();
        match &g.transient {
            Some(t) => t.clone(),
            None => status_on_disk(dir, self.spec.size),
        }
    }

    /// ダウンロードを始める。同時に走るのは1つだけ。取得済みなら拒否する。
    pub fn start(self: &Arc<Self>, notify: Notify) -> Result<()> {
        let Some(dir) = self.models_dir.clone() else { return Err(invalid(ERR_UNSUPPORTED)) };
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut g = self.inner.lock().unwrap();
            if g.running.is_some() {
                return Err(Error::Invalid("すでにダウンロード中です".into()));
            }
            if status_on_disk(&dir, self.spec.size) == LocalModelStatus::Ready {
                return Err(Error::Invalid("モデルは取得済みです".into()));
            }
            g.running = Some(cancel.clone());
            g.transient = Some(LocalModelStatus::Downloading { downloaded: 0, total: self.spec.size });
        }
        notify(LocalModelStatus::Downloading { downloaded: 0, total: self.spec.size });
        let this = self.clone();
        tokio::spawn(async move { this.run(dir, cancel, notify).await });
        Ok(())
    }

    async fn run(self: Arc<Self>, dir: PathBuf, cancel: Arc<AtomicBool>, notify: Notify) {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.finish(Err(DownloadError::Io(e.to_string())), &notify);
            return;
        }
        let (part, dest) = (part_path(&dir), model_path(&dir));
        let mut last = Instant::now() - PROGRESS_INTERVAL;
        let this = self.clone();
        let n = notify.clone();
        let r = download(&self.http, &self.spec, &part, &dest, &cancel, &available_space, &mut |p| {
            let status = match p {
                Phase::Downloading { downloaded, total } => LocalModelStatus::Downloading { downloaded, total },
                Phase::Verifying => LocalModelStatus::Verifying,
            };
            // 状態は常に更新し、通知だけ間引く（Verifying は必ず通知する）。
            this.inner.lock().unwrap().transient = Some(status.clone());
            if matches!(status, LocalModelStatus::Verifying) || last.elapsed() >= PROGRESS_INTERVAL {
                last = Instant::now();
                n(status);
            }
        })
        .await;
        self.finish(r, &notify);
    }

    fn finish(&self, r: std::result::Result<(), DownloadError>, notify: &Notify) {
        let final_status = {
            let mut g = self.inner.lock().unwrap();
            g.running = None;
            match r {
                Ok(()) => {
                    g.transient = None;
                    LocalModelStatus::Ready
                }
                // キャンセルは失敗ではない。.part は残る（再開できる）。ディスクの状態（NotInstalled）に戻す。
                Err(DownloadError::Cancelled) => {
                    g.transient = None;
                    LocalModelStatus::NotInstalled
                }
                Err(e) => {
                    let f = LocalModelStatus::Failed { reason: e.reason() };
                    g.transient = Some(f.clone());
                    f
                }
            }
        };
        notify(final_status);
    }

    /// 実行中のダウンロードを止める。`.part` は残す。
    pub fn cancel(&self) {
        if let Some(c) = &self.inner.lock().unwrap().running {
            c.store(true, Ordering::SeqCst);
        }
    }

    /// モデルを削除する（`.part` も）。読み込み中なら先にアンロードする。ダウンロード中は拒否する。
    pub async fn delete(&self) -> Result<()> {
        let Some(dir) = &self.models_dir else { return Ok(()) };
        if self.is_running() {
            return Err(Error::Invalid("ダウンロード中は削除できません".into()));
        }
        self.translator.unload().await;
        for p in [model_path(dir), part_path(dir)] {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::Invalid(format!("モデルを削除できませんでした: {}", e.kind()))),
            }
        }
        self.inner.lock().unwrap().transient = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::download::DownloadError;
    use super::super::engine::LocalTranslator;
    use super::super::model_store::{model_path, part_path};
    use super::super::{ERR_UNSUPPORTED};
    use super::*;
    use crate::error::Error;
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tsumugi_lm_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }

    fn body(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    /// Range に対応するサーバー。`delay` を付けると、応答が遅れる（ダウンロードを「実行中」に保つため）。
    struct Server {
        body: Vec<u8>,
        delay: Option<Duration>,
        hits: Arc<AtomicUsize>,
    }
    impl Respond for Server {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            self.hits.fetch_add(1, Ordering::SeqCst);
            let range = req.headers.get("range").map(|v| v.to_str().unwrap().to_string());
            let t = match range {
                Some(r) => {
                    let start: usize = r.trim_start_matches("bytes=").trim_end_matches('-').parse().unwrap();
                    ResponseTemplate::new(206)
                        .insert_header("content-range", format!("bytes {}-{}/{}", start, self.body.len() - 1, self.body.len()))
                        .set_body_bytes(self.body[start..].to_vec())
                }
                None => ResponseTemplate::new(200).set_body_bytes(self.body.clone()),
            };
            match self.delay {
                Some(d) => t.set_delay(d),
                None => t,
            }
        }
    }

    struct Fx {
        _server: MockServer,
        dir: PathBuf,
        hits: Arc<AtomicUsize>,
        models: Arc<LocalModels>,
        events: Arc<Mutex<Vec<LocalModelStatus>>>,
        notify: Notify,
        data: Vec<u8>,
    }

    async fn fx(delay: Option<Duration>, wrong_sha: bool, translator: Arc<LocalTranslate>) -> Fx {
        let data = body(200_000);
        let server = MockServer::start().await;
        let hits = Arc::new(AtomicUsize::new(0));
        Mock::given(method("GET"))
            .respond_with(Server { body: data.clone(), delay, hits: hits.clone() })
            .mount(&server)
            .await;
        let spec = ModelSpec {
            url: format!("{}/m.gguf", server.uri()),
            size: data.len() as u64,
            sha256: if wrong_sha { sha(b"other") } else { sha(&data) },
        };
        let dir = temp_dir();
        let models = Arc::new(LocalModels::with_spec(dir.clone(), reqwest::Client::new(), translator, spec));
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let notify: Notify = Arc::new(move |s| ev.lock().unwrap().push(s));
        Fx { _server: server, dir, hits, models, events, notify, data }
    }

    fn plain() -> Arc<LocalTranslate> {
        Arc::new(LocalTranslate::unconfigured())
    }

    async fn wait_idle(m: &LocalModels) {
        tokio::time::timeout(Duration::from_secs(20), async {
            while m.is_running() {
                tokio::time::sleep(Duration::from_millis(10)).await
            }
        })
        .await
        .expect("ダウンロードが終わらない");
    }

    fn assert_invalid(r: Result<()>, text: &str) {
        match r {
            Err(Error::Invalid(m)) => assert!(m.contains(text), "{m}"),
            other => panic!("expected Invalid containing {text:?}, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_successful_download_ends_ready_and_reports_progress_then_verifying_then_ready() {
        let f = fx(None, false, plain()).await;
        assert_eq!(f.models.status(), LocalModelStatus::NotInstalled);
        f.models.start(f.notify.clone()).unwrap();
        wait_idle(&f.models).await;

        assert_eq!(f.models.status(), LocalModelStatus::Ready);
        assert_eq!(std::fs::read(model_path(&f.dir)).unwrap(), f.data);
        assert!(!part_path(&f.dir).exists());
        let ev = f.events.lock().unwrap();
        assert_eq!(ev.last(), Some(&LocalModelStatus::Ready));
        let verifying = ev.iter().position(|s| *s == LocalModelStatus::Verifying).expect("Verifying の通知がある");
        assert_eq!(verifying, ev.len() - 2, "Verifying は Ready の直前");
        let downloaded: Vec<u64> = ev
            .iter()
            .filter_map(|s| match s {
                LocalModelStatus::Downloading { downloaded, .. } => Some(*downloaded),
                _ => None,
            })
            .collect();
        assert!(!downloaded.is_empty());
        assert!(downloaded.windows(2).all(|w| w[0] <= w[1]), "{downloaded:?}");
    }

    #[tokio::test]
    async fn a_second_start_while_running_is_rejected_and_does_not_disturb_the_first() {
        let f = fx(Some(Duration::from_millis(300)), false, plain()).await;
        f.models.start(f.notify.clone()).unwrap();
        assert!(matches!(f.models.status(), LocalModelStatus::Downloading { .. }));
        assert_invalid(f.models.start(f.notify.clone()), "すでにダウンロード中です");
        wait_idle(&f.models).await;
        assert_eq!(f.models.status(), LocalModelStatus::Ready);
        assert_eq!(f.hits.load(Ordering::SeqCst), 1, "2本目はリクエストを出さない");
    }

    #[tokio::test]
    async fn start_is_rejected_when_the_model_is_already_ready() {
        let f = fx(None, false, plain()).await;
        std::fs::write(model_path(&f.dir), &f.data).unwrap();
        assert_eq!(f.models.status(), LocalModelStatus::Ready);
        assert_invalid(f.models.start(f.notify.clone()), "取得済み");
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
        assert!(f.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancel_stops_a_running_download_reports_not_installed_and_a_restart_completes() {
        let f = fx(Some(Duration::from_millis(300)), false, plain()).await;
        f.models.start(f.notify.clone()).unwrap();
        f.models.cancel();
        wait_idle(&f.models).await;
        assert_eq!(f.models.status(), LocalModelStatus::NotInstalled, "キャンセルは失敗ではない");
        assert_eq!(f.events.lock().unwrap().last(), Some(&LocalModelStatus::NotInstalled));
        assert!(!model_path(&f.dir).exists());

        f.models.start(f.notify.clone()).unwrap();
        wait_idle(&f.models).await;
        assert_eq!(f.models.status(), LocalModelStatus::Ready);
    }

    #[tokio::test]
    async fn a_failed_download_keeps_the_failed_status_until_the_next_start() {
        let f = fx(None, true, plain()).await;
        f.models.start(f.notify.clone()).unwrap();
        wait_idle(&f.models).await;
        let reason = DownloadError::HashMismatch.reason();
        assert_eq!(f.models.status(), LocalModelStatus::Failed { reason: reason.clone() });
        assert_eq!(f.events.lock().unwrap().last(), Some(&LocalModelStatus::Failed { reason }));
        assert!(!model_path(&f.dir).exists());

        // 失敗のあとでも、もう一度始められる（拒否されない）。
        f.models.start(f.notify.clone()).unwrap();
        wait_idle(&f.models).await;
        assert!(matches!(f.models.status(), LocalModelStatus::Failed { .. }));
        assert_eq!(f.hits.load(Ordering::SeqCst), 2);
    }

    struct Echo;
    impl LocalTranslator for Echo {
        fn translate(&self, text: &str, _lang: &str) -> Result<String> {
            Ok(text.to_string())
        }
    }

    #[tokio::test]
    async fn delete_unloads_the_model_and_removes_the_file_and_the_part() {
        let translator = Arc::new(LocalTranslate::new(Arc::new(|| Ok(Arc::new(Echo) as Arc<dyn LocalTranslator>))));
        let f = fx(None, false, translator.clone()).await;
        std::fs::write(model_path(&f.dir), &f.data).unwrap();
        std::fs::write(part_path(&f.dir), b"leftover").unwrap();
        translator.translate("hi", "en").await.unwrap();
        assert!(translator.is_loaded().await);

        f.models.delete().await.unwrap();

        assert!(!translator.is_loaded().await, "読み込み中のファイルは消せないので、先にアンロードする");
        assert!(!model_path(&f.dir).exists());
        assert!(!part_path(&f.dir).exists());
        assert_eq!(f.models.status(), LocalModelStatus::NotInstalled);
    }

    #[tokio::test]
    async fn delete_clears_a_failed_status() {
        let f = fx(None, true, plain()).await;
        f.models.start(f.notify.clone()).unwrap();
        wait_idle(&f.models).await;
        assert!(matches!(f.models.status(), LocalModelStatus::Failed { .. }));
        f.models.delete().await.unwrap();
        assert_eq!(f.models.status(), LocalModelStatus::NotInstalled);
    }

    #[tokio::test]
    async fn delete_is_rejected_while_downloading_and_the_download_continues() {
        let f = fx(Some(Duration::from_millis(300)), false, plain()).await;
        f.models.start(f.notify.clone()).unwrap();
        assert_invalid(f.models.delete().await, "ダウンロード中");
        wait_idle(&f.models).await;
        assert_eq!(f.models.status(), LocalModelStatus::Ready);
    }

    #[tokio::test]
    async fn the_unconfigured_instance_reports_unsupported_on_start() {
        let m = Arc::new(LocalModels::unconfigured());
        let notify: Notify = Arc::new(|_| {});
        match m.start(notify) {
            Err(Error::Invalid(msg)) => assert_eq!(msg, ERR_UNSUPPORTED),
            other => panic!("{other:?}"),
        }
        assert_eq!(m.status(), LocalModelStatus::NotInstalled);
        m.delete().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "実際に Hugging Face から約 573MB を取得する: TSUMUGI_TEST_DOWNLOAD_DIR=<ディスク上の空のディレクトリ> cargo test --lib translate::local_models::tests::downloads_the_real_model -- --ignored --nocapture"]
    async fn downloads_the_real_model() {
        // /tmp は tmpfs（メモリ）なので、必ずディスク上のディレクトリを指定させる。
        let dir = PathBuf::from(std::env::var_os("TSUMUGI_TEST_DOWNLOAD_DIR").expect("TSUMUGI_TEST_DOWNLOAD_DIR を指定する"));
        let http = crate::net_guard::build_fetch_client("tsumugi-test", crate::net_guard::FetchAllowlist::default()).unwrap();
        let m = Arc::new(LocalModels::new(dir.clone(), http, plain()));
        let last = Arc::new(Mutex::new(None));
        let l = last.clone();
        m.start(Arc::new(move |s| *l.lock().unwrap() = Some(s))).unwrap();
        tokio::time::timeout(Duration::from_secs(1800), async {
            while m.is_running() {
                tokio::time::sleep(Duration::from_millis(200)).await
            }
        })
        .await
        .expect("30分で終わらない");
        assert_eq!(m.status(), LocalModelStatus::Ready, "最後の通知: {:?}", last.lock().unwrap());
        assert_eq!(std::fs::metadata(model_path(&dir)).unwrap().len(), super::super::model_store::MODEL_SIZE_BYTES);
    }
}
