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

pub struct ExternalEngine {
    client: reqwest::Client,
    /// 末尾のスラッシュを除いた基準 URL(例 `http://192.168.1.10:50021`)。
    base: String,
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
        Ok(Self { client, base: u.as_str().trim_end_matches('/').to_string() })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }
}

/// 通信の失敗(接続不可・タイムアウト)を識別コードつきの `Invalid` にする。
fn unreachable(e: reqwest::Error) -> Error {
    invalid(format!("{ERR_UNREACHABLE}: {e}"))
}

fn bad_response(detail: impl std::fmt::Display) -> Error {
    invalid(format!("{ERR_BAD_RESPONSE}: {detail}"))
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
        let speakers: Vec<SpeakerDto> = resp.json().await.map_err(bad_response)?;
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
        let mut query: Value = resp.json().await.map_err(bad_response)?;
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
        let bytes = resp.bytes().await.map_err(unreachable)?;
        if !bytes.starts_with(b"RIFF") {
            return Err(bad_response("synthesis の応答が WAV ではありません"));
        }
        Ok(bytes.to_vec())
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
}
