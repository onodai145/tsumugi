//! MCP サーバー(axum + rmcp、Issue #557)。`127.0.0.1` だけで待ち受け、ステートレスで、
//! リクエストごとに guard(Host / Origin / bearer)を通す。
//! rmcp 側にも同じ Host / Origin の検証を入れておく(二重の防御)。

use crate::mcp::guard::{self, Verdict};
use crate::mcp::token::SharedToken;
use crate::mcp::tools::McpTools;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Router,
};
use rmcp::transport::streamable_http_server::{
    session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Runtime};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// POST ボディの上限。ツールの引数は小さい(rmcp の既定は 4MiB)。
const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunState {
    Stopped,
    Running { port: u16 },
    Error(String),
}

struct Running {
    shutdown: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

pub struct McpRuntime {
    /// 実行中のサーバーが、リクエストごとに読むトークン。
    pub token: SharedToken,
    state: Mutex<RunState>,
    running: tokio::sync::Mutex<Option<Running>>,
}

impl Default for McpRuntime {
    fn default() -> Self {
        Self { token: SharedToken::default(), state: Mutex::new(RunState::Stopped), running: tokio::sync::Mutex::new(None) }
    }
}

struct GuardState {
    port: u16,
    token: SharedToken,
}

async fn guard_middleware(State(g): State<Arc<GuardState>>, req: Request, next: Next) -> Response {
    match guard::check(req.headers(), g.port, g.token.get().as_deref()) {
        Verdict::Allow => next.run(req).await,
        Verdict::Forbidden => (StatusCode::FORBIDDEN, "Forbidden").into_response(),
        Verdict::Unauthorized => (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Bearer")], "Unauthorized").into_response(),
    }
}

impl McpRuntime {
    pub fn status(&self) -> RunState {
        self.state.lock().unwrap().clone()
    }

    fn set_state(&self, s: RunState) {
        *self.state.lock().unwrap() = s;
    }

    /// `127.0.0.1:<port>` で起動し、束縛したポートを返す(`0` なら空きポート。テスト用)。
    /// すでに動いていれば先に止める。失敗したら状態を `Error` にして `Err` を返す。
    pub async fn start<R: Runtime>(&self, app: AppHandle<R>, port: u16) -> Result<u16, String> {
        self.stop().await;
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => l,
            Err(e) => {
                let msg = format!("127.0.0.1:{port} で待ち受けできません: {e}");
                self.set_state(RunState::Error(msg.clone()));
                return Err(msg);
            }
        };
        let bound = listener.local_addr().map_err(|e| e.to_string())?.port();
        let shutdown = CancellationToken::new();

        let config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_allowed_hosts([format!("127.0.0.1:{bound}"), format!("localhost:{bound}")])
            .enforce_origin_validation()
            .with_max_request_body_bytes(MAX_BODY_BYTES)
            .with_cancellation_token(shutdown.child_token());
        let service = StreamableHttpService::new(
            move || Ok(McpTools::new(app.clone())),
            Arc::new(NeverSessionManager::default()),
            config,
        );
        let guard_state = Arc::new(GuardState { port: bound, token: self.token.clone() });
        let router: Router = Router::new()
            .nest_service("/mcp", service)
            .layer(axum::middleware::from_fn_with_state(guard_state, guard_middleware));

        let ct = shutdown.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).with_graceful_shutdown(async move { ct.cancelled().await }).await;
        });
        *self.running.lock().await = Some(Running { shutdown, task });
        self.set_state(RunState::Running { port: bound });
        Ok(bound)
    }

    pub async fn stop(&self) {
        let running = self.running.lock().await.take();
        if let Some(r) = running {
            r.shutdown.cancel();
            // 進行中の接続を待ちすぎない。
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), r.task).await;
        }
        self.set_state(RunState::Stopped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Visibility;
    use crate::mcp::testutil::{account, note, user};
    use crate::state::AppState;
    use crate::store::SettingsStore;
    use serde_json::{json, Value};
    use tauri::Manager;

    const TOKEN: &str = "test-token";

    /// モックのアプリに、公開ONのアカウント a1 とノートを用意してサーバーを起動する。
    struct Harness {
        app: tauri::App<tauri::test::MockRuntime>,
        port: u16,
        client: reqwest::Client,
    }

    impl Harness {
        async fn start() -> Self {
            let app = tauri::test::mock_app();
            let state = AppState::new_for_test(SettingsStore::new_in_memory());
            state.accounts.lock().unwrap().upsert(account("a1", "one.example"));
            state.settings.update_mcp(|m| {
                m.enabled = true;
                m.exposed_account_ids = vec!["a1".into()];
            }).unwrap();
            let alice = user("u1", "alice", None);
            let notes = vec![
                note("p", 500, Visibility::Public, false, "public rust", alice.clone()),
                note("f", 300, Visibility::Followers, false, "followers rust", alice),
            ];
            state.cache.cache_notes("col1", &notes).await.unwrap();
            state.cache.record_note_accounts("a1", &["p".to_string(), "f".to_string()]).await.unwrap();
            state.mcp.token.set(TOKEN.into());
            app.manage(state);
            let port = app.state::<AppState>().mcp.start(app.handle().clone(), 0).await.unwrap();
            // システムのプロキシ設定が 127.0.0.1 宛を横取りしないよう no_proxy にする。
            let client = reqwest::Client::builder().no_proxy().build().unwrap();
            Harness { app, port, client }
        }

        fn state(&self) -> tauri::State<'_, AppState> {
            self.app.state::<AppState>()
        }

        async fn post(&self, token: Option<&str>, extra: &[(&str, &str)], body: Value) -> (u16, Value) {
            let mut req = self
                .client
                .post(format!("http://127.0.0.1:{}/mcp", self.port))
                .header("Content-Type", "application/json")
                .header("Accept", "application/json, text/event-stream");
            if let Some(t) = token {
                req = req.header("Authorization", format!("Bearer {t}"));
            }
            for (k, v) in extra {
                req = req.header(*k, *v);
            }
            let resp = req.json(&body).send().await.unwrap();
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            (status, serde_json::from_str(&text).unwrap_or(Value::Null))
        }

        async fn call_tool(&self, name: &str, args: Value) -> Value {
            let (status, v) = self
                .post(Some(TOKEN), &[], json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}))
                .await;
            assert_eq!(status, 200, "{v}");
            v
        }
    }

    fn init_body() -> Value {
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}})
    }

    #[tokio::test]
    async fn rejects_missing_and_wrong_tokens_with_401() {
        let h = Harness::start().await;
        assert_eq!(h.post(None, &[], init_body()).await.0, 401);
        assert_eq!(h.post(Some("wrong"), &[], init_body()).await.0, 401);
        assert_eq!(h.post(Some(TOKEN), &[], init_body()).await.0, 200);
    }

    #[tokio::test]
    async fn rejects_bad_host_and_any_origin_with_403_even_with_a_valid_token() {
        let h = Harness::start().await;
        assert_eq!(h.post(Some(TOKEN), &[("Host", "evil.example")], init_body()).await.0, 403);
        assert_eq!(h.post(Some(TOKEN), &[("Origin", "http://localhost")], init_body()).await.0, 403);
    }

    #[tokio::test]
    async fn rejects_oversized_bodies() {
        let h = Harness::start().await;
        let big = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_notes","arguments":{"account":"a1","text":"x".repeat(70 * 1024)}}});
        assert_eq!(h.post(Some(TOKEN), &[], big).await.0, 413);
    }

    #[tokio::test]
    async fn initialize_list_and_call_round_trip() {
        let h = Harness::start().await;
        let (_, init) = h.post(Some(TOKEN), &[], init_body()).await;
        assert!(init["result"]["capabilities"]["tools"].is_object(), "{init}");

        let (_, list) = h.post(Some(TOKEN), &[], json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).await;
        let mut names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        names.sort();
        assert_eq!(names, vec!["get_note", "list_accounts", "search_notes"]);
        for t in list["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["annotations"]["readOnlyHint"], true, "読み取り専用の注釈: {t}");
            let d = t["description"].as_str().unwrap();
            assert!(d.contains("cached"), "キャッシュ済みのみが対象であることを説明に含める: {d}");
        }

        let v = h.call_tool("list_accounts", json!({})).await;
        assert_eq!(v["result"]["structuredContent"]["accounts"][0]["id"], "a1");

        let v = h.call_tool("search_notes", json!({"account":"a1","text":"rust"})).await;
        let notes = v["result"]["structuredContent"]["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1, "followers のノートは返らない: {v}");
        assert_eq!(notes[0]["id"], "p");

        let v = h.call_tool("get_note", json!({"account":"a1","id":"p"})).await;
        assert_eq!(v["result"]["structuredContent"]["id"], "p");
    }

    #[tokio::test]
    async fn out_of_scope_and_unexposed_return_the_same_tool_error() {
        let h = Harness::start().await;
        let a = h.call_tool("get_note", json!({"account":"a1","id":"f"})).await;
        let b = h.call_tool("get_note", json!({"account":"zzz","id":"p"})).await;
        assert_eq!(a["result"]["isError"], true);
        assert_eq!(a["result"], b["result"], "存在の有無を区別できてはいけない");
    }

    #[tokio::test]
    async fn turning_exposure_off_applies_to_the_next_request() {
        let h = Harness::start().await;
        let ok = h.call_tool("search_notes", json!({"account":"a1"})).await;
        assert_ne!(ok["result"]["isError"], true, "{ok}");
        h.state().settings.update_mcp(|m| m.exposed_account_ids.clear()).unwrap();
        let denied = h.call_tool("search_notes", json!({"account":"a1"})).await;
        assert_eq!(denied["result"]["isError"], true);
    }

    #[tokio::test]
    async fn regenerating_the_token_invalidates_the_old_one_immediately() {
        let h = Harness::start().await;
        assert_eq!(h.post(Some(TOKEN), &[], init_body()).await.0, 200);
        h.state().mcp.token.set("new-token".into());
        assert_eq!(h.post(Some(TOKEN), &[], init_body()).await.0, 401);
        assert_eq!(h.post(Some("new-token"), &[], init_body()).await.0, 200);
    }

    #[tokio::test]
    async fn bind_failure_sets_the_error_state() {
        // 先にポートを塞いでおく。
        let blocker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = blocker.local_addr().unwrap().port();
        let app = tauri::test::mock_app();
        app.manage(AppState::new_for_test(SettingsStore::new_in_memory()));
        let state = app.state::<AppState>();
        let r = state.mcp.start(app.handle().clone(), port).await;
        assert!(r.is_err());
        assert!(matches!(state.mcp.status(), RunState::Error(_)));
    }

    #[tokio::test]
    async fn stop_closes_the_port_and_status_follows() {
        let h = Harness::start().await;
        assert_eq!(h.state().mcp.status(), RunState::Running { port: h.port });
        h.state().mcp.stop().await;
        assert_eq!(h.state().mcp.status(), RunState::Stopped);
        let r = h.client.post(format!("http://127.0.0.1:{}/mcp", h.port)).send().await;
        assert!(r.is_err(), "停止後はポートが閉じている");
    }
}
