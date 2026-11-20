//! 読み上げワーカーの本番の配線(Issue #625)。`AppHandle` 経由で設定・出力・イベントを引く。

use super::engine::SpeechEngine;
use super::external::ExternalEngine;
use super::manager::{run_supervised, SpeechOutput, SpeechStatus, WorkerContext, WORKER_RESTART_DELAY};
use super::{invalid, ERR_NOT_CONFIGURED};
use crate::domain::{SpeechEngineKind, SpeechSettings};
use crate::error::Result;
use crate::events::SpeechStatusChanged;
use crate::state::AppState;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_specta::Event as _;

pub struct AppContext {
    app: AppHandle,
    /// 直近に作ったエンジンを、URL が同じ間は使い回す(接続の再利用)。
    engine: Mutex<Option<(String, Arc<dyn SpeechEngine>)>>,
}

impl AppContext {
    pub fn new(app: AppHandle) -> Self {
        Self { app, engine: Mutex::new(None) }
    }
}

impl WorkerContext for AppContext {
    fn settings(&self) -> SpeechSettings {
        self.app.state::<AppState>().settings.load_speech()
    }

    fn engine(&self, settings: &SpeechSettings) -> Result<Arc<dyn SpeechEngine>> {
        // 内蔵エンジンは、`AppState` が持つ 1 つを使い回す(ロード済みのモデルを共有する)。未取得・非対応のときは、
        // `synthesize` が `ERR_NOT_READY` / `ERR_UNSUPPORTED` を返し、失敗の連続で `Error` 状態になる。
        if settings.engine == SpeechEngineKind::VoicevoxCore {
            return Ok(Arc::new(self.app.state::<AppState>().speech_core.engine.clone()));
        }
        let url = settings.external_url.trim().to_string();
        if url.is_empty() {
            return Err(invalid(ERR_NOT_CONFIGURED));
        }
        let mut cached = self.engine.lock().unwrap();
        if let Some((u, e)) = cached.as_ref() {
            if *u == url {
                return Ok(e.clone());
            }
        }
        let e: Arc<dyn SpeechEngine> = Arc::new(ExternalEngine::new(&url)?);
        *cached = Some((url, e.clone()));
        Ok(e)
    }

    fn output(&self) -> Arc<dyn SpeechOutput> {
        Arc::new(self.app.state::<AppState>().sound.clone())
    }

    fn emit_status(&self, status: SpeechStatus) {
        let _ = SpeechStatusChanged { status }.emit(&self.app);
    }
}

/// ワーカーを起動する。`AppState` を `manage` した後に呼ぶ。
pub fn spawn(app: AppHandle) {
    let mgr = app.state::<AppState>().speech.clone();
    let ctx: Arc<dyn WorkerContext> = Arc::new(AppContext::new(app));
    tauri::async_runtime::spawn(run_supervised(mgr, ctx, WORKER_RESTART_DELAY));
}
