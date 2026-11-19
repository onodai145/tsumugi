//! 外部の VOICEVOX 互換エンジン(VOICEVOX ENGINE / AivisSpeech Engine)への HTTP クライアント(Issue #625)。
//! 使う API は `/speakers`・`/audio_query`・`/synthesis`(VOICEVOX ENGINE の README で確認済み)。

use super::engine::{SpeechEngine, SynthParams, Voice};
use super::{invalid, ERR_BAD_RESPONSE, ERR_UNREACHABLE};
use crate::domain::speech::parse_base_url;
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// 接続のタイムアウト。LAN 内の HomeLab を想定した短めの値。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 1リクエスト全体のタイムアウト。長めの文の合成が間に合う程度。
const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
/// JSON の応答(`/speakers`・`/audio_query`)の最大バイト数。誤った URL や悪意のあるサーバーが、巨大な応答で
/// メモリを圧迫しないための上限(声の一覧も、300 文字の読みも、これよりずっと小さい)。
const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
/// 音声(`/synthesis` の WAV)の最大バイト数。300 文字の読み上げでも数 MB なので、十分に余裕がある。
const MAX_WAV_BYTES: usize = 32 * 1024 * 1024;

pub struct ExternalEngine {
    client: reqwest::Client,
    /// 末尾のスラッシュを除いた基準 URL(例 `http://192.168.1.10:50021`)。
    base: String,
    max_json: usize,
    max_wav: usize,
}

impl ExternalEngine {
    pub fn new(base_url: &str) -> Result<Self> {
        Self::with_timeouts(base_url, CONNECT_TIMEOUT, TOTAL_TIMEOUT)
    }

    /// タイムアウトを指定して作る(テスト用に公開。本番は `new`)。
    pub fn with_timeouts(base_url: &str, connect: Duration, total: Duration) -> Result<Self> {
        let u = parse_base_url(base_url).map_err(Error::Invalid)?;
        // reqwest の既定は無制限なので、必ずタイムアウトを付ける。リダイレクトは追わない
        // (設定した URL 以外へ接続しないため)。
        let client = reqwest::Client::builder()
            .user_agent(crate::state::USER_AGENT)
            .connect_timeout(connect)
            .timeout(total)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| Error::Invalid(format!("HTTP クライアントを作れません: {e}")))?;
        Ok(Self {
            client,
            base: u.as_str().trim_end_matches('/').to_string(),
            max_json: MAX_JSON_BYTES,
            max_wav: MAX_WAV_BYTES,
        })
    }

    /// 応答サイズの上限を指定する(テスト用に公開。本番は `new` の既定値)。
    pub fn with_limits(mut self, max_json: usize, max_wav: usize) -> Self {
        self.max_json = max_json;
        self.max_wav = max_wav;
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }
}

/// 通信の失敗(接続不可・タイムアウト)を識別コードつきの `Invalid` にする。
/// reqwest のエラーはリクエスト URL(クエリに読み上げる本文が入る)を文面に出すので、`without_url` で落とす
/// (設定画面・ログ・イベントに、ノートの本文を漏らさない)。
fn unreachable(e: reqwest::Error) -> Error {
    invalid(format!("{ERR_UNREACHABLE}: {}", e.without_url()))
}

/// 応答の形が想定外。`reqwest::Error` を渡すときは、`without_url` を通してから渡す(`json_err`)。
fn bad_response(detail: impl std::fmt::Display) -> Error {
    invalid(format!("{ERR_BAD_RESPONSE}: {detail}"))
}

/// 応答の本文を、`limit` バイトまでに制限して読む。`Content-Length` が上限を超えていれば読み始めず、チャンク転送などで
/// 長さが分からなくても、上限を超えた時点で打ち切る。
async fn read_limited(mut resp: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    if resp.content_length().is_some_and(|len| len > limit as u64) {
        return Err(bad_response("応答が大きすぎます"));
    }
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(unreachable)? {
        if buf.len() + chunk.len() > limit {
            return Err(bad_response("応答が大きすぎます"));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// JSON の応答を、上限つきで読んで解釈する。
async fn read_json<T: serde::de::DeserializeOwned>(resp: reqwest::Response, limit: usize) -> Result<T> {
    let bytes = read_limited(resp, limit).await?;
    serde_json::from_slice(&bytes).map_err(bad_response)
}

#[derive(Deserialize)]
struct SpeakerDto {
    name: String,
    styles: Vec<StyleDto>,
}

#[derive(Deserialize)]
struct StyleDto {
    name: String,
    id: serde_json::Number,
}

#[async_trait]
impl SpeechEngine for ExternalEngine {
    async fn voices(&self) -> Result<Vec<Voice>> {
        let resp = self.client.get(self.url("speakers")).send().await.map_err(unreachable)?;
        if !resp.status().is_success() {
            return Err(bad_response(format!("HTTP {}", resp.status())));
        }
        let speakers: Vec<SpeakerDto> = read_json(resp, self.max_json).await?;
        Ok(speakers
            .into_iter()
            .flat_map(|s| {
                let name = s.name;
                s.styles.into_iter().map(move |st| Voice {
                    id: st.id.to_string(),
                    label: format!("{name}（{}）", st.name),
                })
            })
            .collect())
    }

    async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>> {
        // 1. 読み・イントネーションのクエリを作る。応答は不透明な JSON のまま扱い、
        //    話速と音量だけを上書きする(エンジン間の差異やフィールド追加に強くするため)。
        let resp = self
            .client
            .post(self.url("audio_query"))
            .query(&[("speaker", voice_id), ("text", text)])
            .send()
            .await
            .map_err(unreachable)?;
        if !resp.status().is_success() {
            return Err(bad_response(format!("audio_query: HTTP {}", resp.status())));
        }
        let mut query: Value = read_json(resp, self.max_json).await?;
        let obj = query
            .as_object_mut()
            .ok_or_else(|| bad_response("audio_query の応答が JSON のオブジェクトではありません"))?;
        obj.insert("speedScale".into(), Value::from(p.speed_scale));
        obj.insert("volumeScale".into(), Value::from(p.volume));

        // 2. 合成。
        let resp = self
            .client
            .post(self.url("synthesis"))
            .query(&[("speaker", voice_id)])
            .json(&query)
            .send()
            .await
            .map_err(unreachable)?;
        if !resp.status().is_success() {
            return Err(bad_response(format!("synthesis: HTTP {}", resp.status())));
        }
        let bytes = read_limited(resp, self.max_wav).await?;
        if !bytes.starts_with(b"RIFF") {
            return Err(bad_response("synthesis の応答が WAV ではありません"));
        }
        Ok(bytes)
    }
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;
    use crate::tts::{ERR_BAD_RESPONSE, ERR_UNREACHABLE};
    use axum::extract::{Query, State};
    use axum::http::{header, StatusCode};
    use axum::response::Redirect;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct Seen {
        audio_query: Arc<Mutex<Vec<HashMap<String, String>>>>,
        synthesis: Arc<Mutex<Vec<(HashMap<String, String>, Value)>>>,
    }

    fn wav() -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"WAVEfmt ");
        v
    }

    /// フェイクのエンジンを 127.0.0.1 の空きポートで起動し、`http://127.0.0.1:<port>` を返す。
    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn engine_app(seen: Seen) -> Router {
        Router::new()
            .route(
                "/speakers",
                get(|| async {
                    Json(json!([
                        {"name": "ずんだもん", "speaker_uuid": "u1", "styles": [{"name": "ノーマル", "id": 3}, {"name": "あまあま", "id": 1}]},
                        {"name": "四国めたん", "speaker_uuid": "u2", "styles": [{"name": "ノーマル", "id": 2}]}
                    ]))
                }),
            )
            .route(
                "/audio_query",
                post(|State(s): State<Seen>, Query(q): Query<HashMap<String, String>>| async move {
                    s.audio_query.lock().unwrap().push(q);
                    Json(json!({"accent_phrases": [], "speedScale": 1.0, "volumeScale": 1.0, "pitchScale": 0.0}))
                }),
            )
            .route(
                "/synthesis",
                post(|State(s): State<Seen>, Query(q): Query<HashMap<String, String>>, Json(body): Json<Value>| async move {
                    s.synthesis.lock().unwrap().push((q, body));
                    ([(header::CONTENT_TYPE, "audio/wav")], wav())
                }),
            )
            .with_state(seen)
    }

    #[tokio::test]
    async fn voices_flattens_speakers_and_styles_into_labelled_ids() {
        let url = serve(engine_app(Seen::default())).await;
        let e = ExternalEngine::new(&url).unwrap();
        let v = e.voices().await.unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], Voice { id: "3".into(), label: "ずんだもん（ノーマル）".into() });
        assert_eq!(v[1].id, "1");
        assert_eq!(v[2], Voice { id: "2".into(), label: "四国めたん（ノーマル）".into() });
    }

    #[tokio::test]
    async fn synthesize_sends_text_and_speaker_then_applies_speed_and_volume() {
        let seen = Seen::default();
        let url = serve(engine_app(seen.clone())).await;
        let e = ExternalEngine::new(&format!("{url}/")).unwrap(); // 末尾スラッシュがあっても動く
        let out = e
            .synthesize("こんにちは&テスト", "3", &SynthParams { speed_scale: 1.5, volume: 0.8 })
            .await
            .unwrap();
        assert!(out.starts_with(b"RIFF"));

        let aq = seen.audio_query.lock().unwrap();
        assert_eq!(aq[0].get("speaker").map(String::as_str), Some("3"));
        assert_eq!(aq[0].get("text").map(String::as_str), Some("こんにちは&テスト"), "記号を含む本文が壊れずに届く");

        let sy = seen.synthesis.lock().unwrap();
        assert_eq!(sy[0].0.get("speaker").map(String::as_str), Some("3"));
        assert_eq!(sy[0].1["speedScale"], json!(1.5));
        assert_eq!(sy[0].1["volumeScale"], json!(0.8f32));
        assert_eq!(sy[0].1["pitchScale"], json!(0.0), "上書きしない項目はそのまま渡す");
    }

    #[tokio::test]
    async fn an_http_error_is_a_bad_response() {
        let app = Router::new().route("/audio_query", post(|| async { StatusCode::INTERNAL_SERVER_ERROR }));
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap();
        let err = e.synthesize("a", "1", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap_err();
        assert!(err.to_string().contains(ERR_BAD_RESPONSE), "{err}");
    }

    #[tokio::test]
    async fn a_non_wav_body_is_a_bad_response() {
        // `engine_app` の `/synthesis` と重ならないよう、WAV でない応答を返すルーターを組み直す。
        let app = Router::new()
            .route("/audio_query", post(|| async { Json(json!({"speedScale": 1.0, "volumeScale": 1.0})) }))
            .route("/synthesis", post(|| async { "not a wav" }));
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap();
        let err = e.synthesize("a", "1", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap_err();
        assert!(err.to_string().contains(ERR_BAD_RESPONSE), "{err}");
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let app = Router::new().route("/speakers", get(|| async { Redirect::temporary("http://127.0.0.1:1/") }));
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap();
        let err = e.voices().await.unwrap_err();
        assert!(err.to_string().contains(ERR_BAD_RESPONSE), "リダイレクト先へは行かず、応答の異常として扱う: {err}");
    }

    #[tokio::test]
    async fn an_unreachable_engine_is_reported_as_unreachable() {
        // 一度開いて閉じたポート = 誰も待ち受けていない。
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let e = ExternalEngine::new(&format!("http://127.0.0.1:{port}")).unwrap();
        let err = e.voices().await.unwrap_err();
        assert!(err.to_string().contains(ERR_UNREACHABLE), "{err}");
    }

    #[tokio::test]
    async fn an_oversized_json_response_is_rejected_by_size_not_buffered_forever() {
        let app = Router::new().route(
            "/speakers",
            get(|| async { Json(json!([{ "name": "x".repeat(10_000), "styles": [] }])) }),
        );
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap().with_limits(1_000, 1_000_000);
        let err = e.voices().await.unwrap_err().to_string();
        assert!(err.contains(ERR_BAD_RESPONSE) && err.contains("大きすぎ"), "{err}");
        // 上限以内なら通る。
        let e = ExternalEngine::new(&url).unwrap().with_limits(1_000_000, 1_000_000);
        assert!(e.voices().await.is_ok());
    }

    #[tokio::test]
    async fn an_oversized_wav_is_rejected_even_without_a_content_length() {
        use axum::body::{Body, Bytes};
        let app = Router::new()
            .route("/audio_query", post(|| async { Json(json!({"speedScale": 1.0, "volumeScale": 1.0})) }))
            .route(
                "/synthesis",
                post(|| async {
                    // Content-Length の無い(チャンク転送の)大きな応答。
                    let chunks = (0..20).map(|i| {
                        let mut c = vec![0u8; 1_000];
                        if i == 0 {
                            c[..4].copy_from_slice(b"RIFF");
                        }
                        Ok::<_, std::io::Error>(Bytes::from(c))
                    });
                    Body::from_stream(futures_util::stream::iter(chunks))
                }),
            );
        let url = serve(app).await;
        let p = SynthParams { speed_scale: 1.0, volume: 1.0 };
        let e = ExternalEngine::new(&url).unwrap().with_limits(1_000_000, 5_000);
        let err = e.synthesize("a", "1", &p).await.unwrap_err().to_string();
        assert!(err.contains(ERR_BAD_RESPONSE) && err.contains("大きすぎ"), "{err}");
        let e = ExternalEngine::new(&url).unwrap().with_limits(1_000_000, 1_000_000);
        assert_eq!(e.synthesize("a", "1", &p).await.unwrap().len(), 20_000);
    }

    #[tokio::test]
    async fn errors_do_not_leak_the_request_url_or_the_note_text() {
        // 接続できないエンジン。reqwest のエラーは既定でリクエスト URL(クエリの本文を含む)を文面に出す。
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let e = ExternalEngine::new(&format!("http://127.0.0.1:{port}")).unwrap();
        let err = e.synthesize("秘密の本文", "1", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap_err().to_string();
        assert!(err.contains(ERR_UNREACHABLE), "{err}");
        assert!(!err.contains("audio_query") && !err.contains("秘密") && !err.contains("%E7%A7%98"), "本文や URL を含まない: {err}");

        // 応答の JSON が壊れているときも同じ。
        let app = Router::new().route("/audio_query", post(|| async { "not json" }));
        let url = serve(app).await;
        let e = ExternalEngine::new(&url).unwrap();
        let err = e.synthesize("秘密の本文", "1", &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap_err().to_string();
        assert!(err.contains(ERR_BAD_RESPONSE), "{err}");
        assert!(!err.contains("audio_query") && !err.contains("秘密") && !err.contains("%E7%A7%98"), "本文や URL を含まない: {err}");
    }

    #[tokio::test]
    async fn a_hanging_engine_times_out_instead_of_blocking_forever() {
        let app = Router::new().route(
            "/speakers",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Json(json!([]))
            }),
        );
        let url = serve(app).await;
        let e = ExternalEngine::with_timeouts(&url, Duration::from_millis(200), Duration::from_millis(300)).unwrap();
        let started = std::time::Instant::now();
        let err = e.voices().await.unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5), "タイムアウトで返る: {:?}", started.elapsed());
        assert!(err.to_string().contains(ERR_UNREACHABLE), "{err}");
    }

    #[test]
    fn new_rejects_a_bad_url() {
        assert!(ExternalEngine::new("").is_err());
        assert!(ExternalEngine::new("ftp://example.com").is_err());
        assert!(ExternalEngine::new("http://user:pw@example.com").is_err());
    }

    /// 実際の VOICEVOX 互換エンジンに対する確認(Docker などで起動したものの URL を `TSUMUGI_TEST_SPEECH_URL` に渡す)。
    /// 例: `docker run -d -p 127.0.0.1:50021:50021 voicevox/voicevox_engine:cpu-latest` のあと
    /// `TSUMUGI_TEST_SPEECH_URL=http://127.0.0.1:50021 cargo test --lib tts::external -- --ignored`。
    /// 終わったら `docker rm -fv <id>`(`-v` が無いと匿名ボリュームが孤児で残る)。
    #[tokio::test]
    #[ignore = "実エンジンが要る: TSUMUGI_TEST_SPEECH_URL"]
    async fn real_voicevox_engine() {
        let url = std::env::var("TSUMUGI_TEST_SPEECH_URL").expect("TSUMUGI_TEST_SPEECH_URL を設定してください");
        let e = ExternalEngine::new(&url).unwrap();
        let voices = e.voices().await.unwrap();
        assert!(!voices.is_empty(), "声が1つ以上ある");
        let id = voices[0].id.clone();

        let normal = e.synthesize("こんにちは、読み上げのテストです", &id, &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap();
        let fast = e.synthesize("こんにちは、読み上げのテストです", &id, &SynthParams { speed_scale: 2.0, volume: 1.0 }).await.unwrap();
        assert!(normal.starts_with(b"RIFF") && fast.starts_with(b"RIFF"));
        assert!(fast.len() < normal.len() * 3 / 4, "話速 2.0 は、1.0 よりだいぶ短い: {} vs {}", fast.len(), normal.len());
        // 記号・改行・絵文字を含む本文でも合成できる(クエリの URL エンコードが壊れていない)。
        let odd = e.synthesize("A&B=C?\nうさぎ", &id, &SynthParams { speed_scale: 1.0, volume: 1.0 }).await.unwrap();
        assert!(odd.starts_with(b"RIFF"));
    }
}
