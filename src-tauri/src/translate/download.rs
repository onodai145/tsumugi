//! モデルのダウンロード（Range 再開・SHA-256 検証、Issue #603）。`LocalModels` から使う。
//! 本文は扱わない。エラーの理由（`reason`）は URL・パスを含めない。

use super::model_store::{has_enough_space, MODEL_SHA256, MODEL_SIZE_BYTES, MODEL_URL};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// 1チャンクを待つ上限。接続が黙って止まったときに、いつまでも進捗が止まったままにしない。
const CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

impl ModelSpec {
    pub fn official() -> Self {
        Self { url: MODEL_URL.into(), size: MODEL_SIZE_BYTES, sha256: MODEL_SHA256.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Downloading { downloaded: u64, total: u64 },
    Verifying,
}

#[derive(Debug, PartialEq)]
pub enum DownloadError {
    Cancelled,
    Http(u16),
    Network(String),
    HashMismatch,
    SizeMismatch,
    NoSpace,
    Io(String),
}

impl DownloadError {
    /// 利用者向けの理由。詳細（URL・パス）は含めない。
    pub fn reason(&self) -> String {
        match self {
            Self::Cancelled => "キャンセルしました".into(),
            Self::Http(s) => format!("サーバーがエラーを返しました（HTTP {s}）。時間をおいて再度お試しください"),
            Self::Network(_) => "通信が途切れました。再度ダウンロードすると途中から再開します".into(),
            Self::HashMismatch => "ダウンロードしたファイルの検証に失敗しました。もう一度お試しください".into(),
            Self::SizeMismatch => "ダウンロードしたファイルのサイズが想定と違います。もう一度お試しください".into(),
            Self::NoSpace => "ディスクの空き容量が足りません".into(),
            Self::Io(_) => "ファイルの書き込みに失敗しました（ディスクの空き容量を確認してください）".into(),
        }
    }
}

fn io_err(e: std::io::Error) -> DownloadError {
    DownloadError::Io(e.to_string())
}

fn net_err(e: reqwest::Error) -> DownloadError {
    // 詳細はログにも出さない（URL に署名つきのクエリが付くことがある）。種別だけ持つ。
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else {
        "request"
    };
    DownloadError::Network(kind.into())
}

/// `Content-Range: bytes 100-999/1000` の開始位置。
fn content_range_start(v: &str) -> Option<u64> {
    v.strip_prefix("bytes ")?.split('-').next()?.parse().ok()
}

/// `spec.url` を `part` へ取得し、SHA-256 を検証して `dest` へリネームする。`part` が残っていれば Range で続きから。
/// キャンセル・通信の中断では `part` を残す（再開できる）。ハッシュ・サイズの不一致では `part` を捨てる。
pub async fn download(
    client: &reqwest::Client,
    spec: &ModelSpec,
    part: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    free_space: &(dyn Fn(&Path) -> Option<u64> + Sync),
    on_phase: &mut (dyn FnMut(Phase) + Send),
) -> Result<(), DownloadError> {
    let mut restarted = false;
    loop {
        let mut have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if have > spec.size {
            std::fs::remove_file(part).map_err(io_err)?;
            have = 0;
        }
        if have < spec.size {
            if cancel.load(Ordering::SeqCst) {
                return Err(DownloadError::Cancelled);
            }
            let dir = part.parent().unwrap_or(Path::new("."));
            if let Some(avail) = free_space(dir) {
                if !has_enough_space(avail, spec.size, have) {
                    return Err(DownloadError::NoSpace);
                }
            }
            let mut req = client.get(&spec.url);
            if have > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
            }
            let resp = req.send().await.map_err(net_err)?;
            let append = match resp.status().as_u16() {
                206 => {
                    let start = resp
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                        .and_then(content_range_start);
                    if start != Some(have) {
                        // 想定した位置と違う続きは、継ぎ足せない。捨てて最初から。
                        std::fs::remove_file(part).ok();
                        if restarted {
                            return Err(DownloadError::Http(206));
                        }
                        restarted = true;
                        continue;
                    }
                    true
                }
                // Range を無視して全体を返した。続きではなく、最初から書き直す。
                200 => false,
                416 => {
                    std::fs::remove_file(part).ok();
                    if restarted {
                        return Err(DownloadError::Http(416));
                    }
                    restarted = true;
                    continue;
                }
                s => return Err(DownloadError::Http(s)),
            };
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(part)
                .map_err(io_err)?;
            let mut downloaded = if append { have } else { 0 };
            on_phase(Phase::Downloading { downloaded, total: spec.size });
            let mut stream = resp.bytes_stream();
            loop {
                if cancel.load(Ordering::SeqCst) {
                    return Err(DownloadError::Cancelled);
                }
                match tokio::time::timeout(CHUNK_TIMEOUT, stream.next()).await {
                    Err(_) => return Err(DownloadError::Network("timeout".into())),
                    Ok(None) => break,
                    Ok(Some(Err(e))) => return Err(net_err(e)),
                    Ok(Some(Ok(chunk))) => {
                        file.write_all(&chunk).map_err(io_err)?;
                        downloaded += chunk.len() as u64;
                        on_phase(Phase::Downloading { downloaded, total: spec.size });
                    }
                }
            }
            file.flush().map_err(io_err)?;
        }
        break;
    }

    let len = std::fs::metadata(part).map_err(io_err)?.len();
    if len < spec.size {
        // 途中で切れた。再開できるよう .part は残す。
        return Err(DownloadError::Network("incomplete".into()));
    }
    if len > spec.size {
        std::fs::remove_file(part).ok();
        return Err(DownloadError::SizeMismatch);
    }
    on_phase(Phase::Verifying);
    let part_buf = part.to_path_buf();
    let digest = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        let mut f = std::fs::File::open(part_buf)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(format!("{:x}", h.finalize()))
    })
    .await
    .map_err(|e| DownloadError::Io(e.to_string()))?
    .map_err(io_err)?;
    if !digest.eq_ignore_ascii_case(&spec.sha256) {
        std::fs::remove_file(part).ok();
        return Err(DownloadError::HashMismatch);
    }
    std::fs::rename(part, dest).map_err(io_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tsumugi_dl_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }

    fn body(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    /// `Range: bytes=N-` を解釈して 206 を返す。`honor_range = false` なら無視して常に 200 全体を返す。
    /// 受け取った Range ヘッダを記録する。
    struct Server {
        body: Vec<u8>,
        honor_range: bool,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
        hits: Arc<AtomicUsize>,
    }
    impl Respond for Server {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            self.hits.fetch_add(1, Ordering::SeqCst);
            let range = req.headers.get("range").map(|v| v.to_str().unwrap().to_string());
            self.ranges.lock().unwrap().push(range.clone());
            match (&range, self.honor_range) {
                (Some(r), true) => {
                    let start: usize = r.trim_start_matches("bytes=").trim_end_matches('-').parse().unwrap();
                    if start >= self.body.len() {
                        return ResponseTemplate::new(416);
                    }
                    ResponseTemplate::new(206)
                        .insert_header("content-range", format!("bytes {}-{}/{}", start, self.body.len() - 1, self.body.len()))
                        .set_body_bytes(self.body[start..].to_vec())
                }
                _ => ResponseTemplate::new(200).set_body_bytes(self.body.clone()),
            }
        }
    }

    struct Fixture {
        _server: MockServer,
        dir: PathBuf,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
        hits: Arc<AtomicUsize>,
        spec: ModelSpec,
    }

    async fn fixture(data: Vec<u8>, honor_range: bool, expected_sha: Option<String>) -> Fixture {
        let server = MockServer::start().await;
        let ranges = Arc::new(Mutex::new(vec![]));
        let hits = Arc::new(AtomicUsize::new(0));
        let spec = ModelSpec {
            url: format!("{}/m.gguf", server.uri()),
            size: data.len() as u64,
            sha256: expected_sha.unwrap_or_else(|| sha(&data)),
        };
        Mock::given(method("GET"))
            .respond_with(Server { body: data, honor_range, ranges: ranges.clone(), hits: hits.clone() })
            .mount(&server)
            .await;
        Fixture { _server: server, dir: temp_dir(), ranges, hits, spec }
    }

    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    async fn run(f: &Fixture, cancel: &AtomicBool) -> (Result<(), DownloadError>, Vec<Phase>) {
        let mut phases = vec![];
        let part = f.dir.join("m.gguf.part");
        let dest = f.dir.join("m.gguf");
        let r = download(&client(), &f.spec, &part, &dest, cancel, &|_| Some(u64::MAX), &mut |p| phases.push(p)).await;
        (r, phases)
    }

    #[tokio::test]
    async fn downloads_verifies_and_renames() {
        let data = body(300_000);
        let f = fixture(data.clone(), true, None).await;
        let (r, phases) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data);
        assert!(!f.dir.join("m.gguf.part").exists(), ".part は残さない");
        assert!(matches!(phases.last(), Some(Phase::Verifying)));
        // 進捗は単調に増え、最後に total へ届く。
        let downloaded: Vec<u64> = phases
            .iter()
            .filter_map(|p| match p {
                Phase::Downloading { downloaded, .. } => Some(*downloaded),
                _ => None,
            })
            .collect();
        assert!(downloaded.windows(2).all(|w| w[0] <= w[1]), "{downloaded:?}");
        assert_eq!(*downloaded.last().unwrap(), data.len() as u64);
    }

    #[tokio::test]
    async fn resumes_an_existing_part_with_a_range_request() {
        let data = body(300_000);
        let f = fixture(data.clone(), true, None).await;
        std::fs::write(f.dir.join("m.gguf.part"), &data[..100_000]).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data, "継ぎ足した結果がハッシュ検証を通る");
        assert_eq!(f.ranges.lock().unwrap().as_slice(), &[Some("bytes=100000-".to_string())]);
    }

    #[tokio::test]
    async fn restarts_from_zero_when_the_server_ignores_range() {
        let data = body(200_000);
        let f = fixture(data.clone(), false, None).await;
        std::fs::write(f.dir.join("m.gguf.part"), &data[..50_000]).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data, "200 の全体を、.part に継ぎ足さず上書きする");
    }

    #[tokio::test]
    async fn an_oversized_part_is_discarded_and_refetched_without_range() {
        let data = body(100_000);
        let f = fixture(data.clone(), true, None).await;
        // 期待サイズより大きい .part は、リクエスト前に捨てて最初から取り直す。
        std::fs::write(f.dir.join("m.gguf.part"), body(150_000)).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data);
        assert_eq!(f.ranges.lock().unwrap().as_slice(), &[None], "Range なしで取り直す");
    }

    #[tokio::test]
    async fn a_416_from_the_server_restarts_from_zero_exactly_once() {
        let data = body(50_000);
        let server = MockServer::start().await;
        let ranges = Arc::new(Mutex::new(vec![]));
        let hits = Arc::new(AtomicUsize::new(0));
        // 最初の Range 付きは 416、2回目（Range なし）は全体を返す。
        struct Reject416 {
            inner: Server,
        }
        impl Respond for Reject416 {
            fn respond(&self, req: &Request) -> ResponseTemplate {
                if req.headers.get("range").is_some() {
                    self.inner.hits.fetch_add(1, Ordering::SeqCst);
                    self.inner.ranges.lock().unwrap().push(Some("416".into()));
                    return ResponseTemplate::new(416);
                }
                self.inner.respond(req)
            }
        }
        Mock::given(method("GET"))
            .respond_with(Reject416 { inner: Server { body: data.clone(), honor_range: true, ranges: ranges.clone(), hits: hits.clone() } })
            .mount(&server)
            .await;
        let dir = temp_dir();
        std::fs::write(dir.join("m.gguf.part"), &data[..10_000]).unwrap();
        let spec = ModelSpec { url: format!("{}/m.gguf", server.uri()), size: data.len() as u64, sha256: sha(&data) };
        let r = download(&client(), &spec, &dir.join("m.gguf.part"), &dir.join("m.gguf"), &AtomicBool::new(false), &|_| None, &mut |_| {}).await;
        r.unwrap();
        assert_eq!(std::fs::read(dir.join("m.gguf")).unwrap(), data);
        assert_eq!(ranges.lock().unwrap().as_slice(), &[Some("416".to_string()), None]);
    }

    #[tokio::test]
    async fn a_complete_part_is_verified_without_any_request() {
        let data = body(80_000);
        let f = fixture(data.clone(), true, None).await;
        std::fs::write(f.dir.join("m.gguf.part"), &data).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data);
    }

    #[tokio::test]
    async fn a_hash_mismatch_discards_the_part_and_fails() {
        let data = body(80_000);
        let f = fixture(data, true, Some(sha(b"something else"))).await;
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        assert_eq!(r, Err(DownloadError::HashMismatch));
        assert!(!f.dir.join("m.gguf.part").exists(), "壊れた .part を再開に使わせない");
        assert!(!f.dir.join("m.gguf").exists(), "検証に通らないファイルをモデルとして置かない");
    }

    #[tokio::test]
    async fn an_http_error_fails_without_touching_the_destination() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&server).await;
        let dir = temp_dir();
        let spec = ModelSpec { url: format!("{}/m.gguf", server.uri()), size: 10, sha256: sha(b"x") };
        let r = download(&client(), &spec, &dir.join("p"), &dir.join("d"), &AtomicBool::new(false), &|_| None, &mut |_| {}).await;
        assert_eq!(r, Err(DownloadError::Http(500)));
        assert!(!dir.join("d").exists());
    }

    #[tokio::test]
    async fn not_enough_space_fails_before_any_request() {
        let f = fixture(body(1000), true, None).await;
        let mut phases = vec![];
        let r = download(&client(), &f.spec, &f.dir.join("p"), &f.dir.join("d"), &AtomicBool::new(false), &|_| Some(0), &mut |p| phases.push(p)).await;
        assert_eq!(r, Err(DownloadError::NoSpace));
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cancelling_before_start_keeps_the_destination_empty_and_a_rerun_completes() {
        let f = fixture(body(300_000), true, None).await;
        let cancel = AtomicBool::new(true);
        let (r, _) = run(&f, &cancel).await;
        assert_eq!(r, Err(DownloadError::Cancelled));
        assert!(!f.dir.join("m.gguf").exists());
        cancel.store(false, Ordering::SeqCst);
        let (r, _) = run(&f, &cancel).await;
        r.unwrap();
    }

    #[tokio::test]
    async fn cancelling_mid_download_keeps_the_part_for_resuming() {
        let data = body(600_000);
        let f = fixture(data.clone(), true, None).await;
        let cancel = AtomicBool::new(false);
        let mut seen = 0u64;
        let r = download(&client(), &f.spec, &f.dir.join("m.gguf.part"), &f.dir.join("m.gguf"), &cancel, &|_| None, &mut |p| {
            if let Phase::Downloading { downloaded, .. } = p {
                seen = downloaded;
                // 最初の進捗（0 バイト）の次に届いた、データを書いた通知で止める。
                if downloaded > 0 {
                    cancel.store(true, Ordering::SeqCst);
                }
            }
        })
        .await;
        assert_eq!(r, Err(DownloadError::Cancelled));
        assert!(seen > 0);
        let kept = std::fs::metadata(f.dir.join("m.gguf.part")).unwrap().len();
        assert!(kept > 0 && kept < data.len() as u64, "途中まで残る: {kept}");
        assert!(!f.dir.join("m.gguf").exists());
        // 残った .part から再開して完走する。
        cancel.store(false, Ordering::SeqCst);
        let (r, _) = run(&f, &cancel).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data);
        assert!(f.ranges.lock().unwrap().iter().any(|r| r.as_deref().is_some_and(|v| v.starts_with("bytes="))), "再開は Range で行う");
    }

    #[tokio::test]
    async fn a_body_shorter_than_expected_is_a_network_error_and_keeps_the_part() {
        // サイズが合わない（途中で切れた）ときは、再開できるよう .part を残す。
        let mut f = fixture(body(1000), true, None).await;
        f.spec.size = 2000; // サーバーは 1000 バイトしか返さない
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        assert!(matches!(r, Err(DownloadError::Network(_))), "{r:?}");
        assert_eq!(std::fs::metadata(f.dir.join("m.gguf.part")).unwrap().len(), 1000);
    }

    #[test]
    fn failure_reasons_are_japanese_and_leak_nothing() {
        for e in [
            DownloadError::Cancelled,
            DownloadError::Http(503),
            DownloadError::Network("connection reset https://secret.example/x?token=1".into()),
            DownloadError::HashMismatch,
            DownloadError::SizeMismatch,
            DownloadError::NoSpace,
            DownloadError::Io("/home/me/.local/share/x: disk full".into()),
        ] {
            let r = e.reason();
            assert!(!r.is_empty());
            assert!(!r.contains("secret.example") && !r.contains("/home/"), "{r}");
        }
        assert!(DownloadError::Http(503).reason().contains("503"));
    }
}
