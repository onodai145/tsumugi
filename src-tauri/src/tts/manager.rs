//! 読み上げのワーカー(Issue #625)。キューと状態を持つ `SpeechManager` と、
//! 「取り出す → 合成 → 再生 → 完了待ち → 次へ」を繰り返すループ。
//! エンジン・出力・設定・イベント送信は `WorkerContext` の裏に隠し、Tauri なしで単体テストできる。

use super::engine::SpeechEngine;
use super::queue::{PushOutcome, SpeechItem, SpeechQueue};
use crate::domain::SpeechSettings;
use crate::error::Result;
use crate::speech_lane::SpeakOutcome;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// 連続でこの回数、合成に失敗したら、エンジンに繋がらない状態(`Error`)とみなして待ち項目を捨てる。
pub const FAIL_THRESHOLD: u32 = 3;
/// 音声の長さに足す余白。この時間を過ぎても再生が終わらなければ、出力が壊れたとみなして打ち切る。
pub const PLAYBACK_MARGIN: std::time::Duration = std::time::Duration::from_secs(5);
/// WAV の長さが読めなかったときの、再生の待ち時間の上限。
pub const FALLBACK_PLAYBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// WAV(PCM)の再生時間をヘッダから読む。読めなければ `None`。`data` チャンクのサイズが実際のバイト数より
/// 大きい(ストリーミングの WAV は 0xFFFFFFFF を入れることがある)ときは、残りのバイト数で数える。
pub(crate) fn wav_duration(wav: &[u8]) -> Option<std::time::Duration> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let u32_at = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(wav.get(at..at + 4)?.try_into().ok()?)) };
    let mut byte_rate: Option<u32> = None;
    let mut pos = 12usize;
    while pos + 8 <= wav.len() {
        let id = &wav[pos..pos + 4];
        let size = u32_at(pos + 4)? as usize;
        let body = pos + 8;
        if id == b"fmt " {
            if size < 16 || body + 16 > wav.len() {
                return None;
            }
            byte_rate = Some(u32_at(body + 8)?);
        } else if id == b"data" {
            let rate = byte_rate.filter(|r| *r > 0)?;
            let len = size.min(wav.len() - body);
            return Some(std::time::Duration::from_secs_f64(len as f64 / rate as f64));
        }
        pos = body.checked_add(size)?.checked_add(size & 1)?;
    }
    None
}

/// 読み上げの状態(設定画面に見せる)。状態が変わったときだけ `SpeechStatusChanged` で通知する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SpeechStatus {
    /// まだ一度も合成していない。
    Unknown,
    /// 直近の合成が成功した。
    Ok,
    /// 連続して合成に失敗している。
    Error { message: String },
}

/// 合成した音声の出力先。本番は `SoundPlayer` の読み上げレーン。
#[async_trait]
pub trait SpeechOutput: Send + Sync {
    /// 再生して、終わる(または止められる・失敗する)まで待つ。
    async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome;
    /// 再生中の読み上げを止める。
    fn stop(&self);
}

#[async_trait]
impl SpeechOutput for crate::sound::SoundPlayer {
    async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome {
        crate::sound::SoundPlayer::speak(self, wav).await
    }
    fn stop(&self) {
        self.stop_speech();
    }
}

/// ワーカーが外の世界を見るための窓口。本番は `runtime::AppContext`(`AppHandle` 経由)、テストは偽の実装。
pub trait WorkerContext: Send + Sync {
    fn settings(&self) -> SpeechSettings;
    fn engine(&self, settings: &SpeechSettings) -> Result<Arc<dyn SpeechEngine>>;
    fn output(&self) -> Arc<dyn SpeechOutput>;
    fn emit_status(&self, status: SpeechStatus);
}

struct State {
    queue: SpeechQueue,
    status: SpeechStatus,
    /// 連続した合成の失敗回数。成功で 0 に戻る。
    failures: u32,
}

/// キューと状態。`AppState.speech` に常駐する(依存を持たないので `AppState::new` のシグネチャは変わらない)。
pub struct SpeechManager {
    state: Mutex<State>,
    wake: Notify,
    /// `stop_all` のたびに増える世代。合成中に停止されたかを、合成の前後の差で検出する。
    epoch: AtomicU64,
    /// 音声の長さに足す、再生の待ち時間の余白(テストで短くできる)。
    playback_margin: std::time::Duration,
    /// `stop_all` で、再生を待っているワーカーを起こす(`wait_stopped`)。
    stopped: Notify,
}

impl Default for SpeechManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeechManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State { queue: SpeechQueue::new(), status: SpeechStatus::Unknown, failures: 0 }),
            wake: Notify::new(),
            epoch: AtomicU64::new(0),
            playback_margin: PLAYBACK_MARGIN,
            stopped: Notify::new(),
        }
    }

    #[cfg(test)]
    fn with_playback_margin(mut self, margin: std::time::Duration) -> Self {
        self.playback_margin = margin;
        self
    }

    /// 再生の完了を待つ上限。音声の長さ + 余白。長さが読めなければ既定の上限。
    fn playback_timeout(&self, wav: &[u8]) -> std::time::Duration {
        match wav_duration(wav) {
            Some(d) => d + self.playback_margin,
            None => FALLBACK_PLAYBACK_TIMEOUT,
        }
    }

    /// 待ち行列に積み、ワーカーを起こす。
    pub fn enqueue(&self, item: SpeechItem, column_limit: usize) -> PushOutcome {
        let outcome = self.state.lock().unwrap().queue.push(item, column_limit);
        if matches!(outcome, PushOutcome::Accepted { .. }) {
            // 起きているワーカーがいなくても、許可証が残るので、次の `notified()` はすぐ返る。
            self.wake.notify_one();
        }
        outcome
    }

    /// 指定カラムの待ち項目を消す。再生中・合成中の1件は止めない。
    pub fn remove_column(&self, column_id: &str) -> usize {
        self.state.lock().unwrap().queue.remove_column(column_id)
    }

    /// 待ち項目をすべて消し、合成中の項目の音声を捨てさせる(世代を進める)。再生中の音声は、呼び出し側が
    /// `SpeechOutput::stop` で止める。
    pub fn stop_all(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.state.lock().unwrap().queue.clear();
        self.stopped.notify_waiters();
    }

    /// `epoch` の後に `stop_all` が呼ばれる(世代が進む)まで待つ。すでに進んでいれば、すぐ戻る。
    async fn wait_stopped(&self, epoch: u64) {
        loop {
            // 先に待ち受けを登録してから世代を見る(確認と待機の間に `stop_all` が入っても、取りこぼさない)。
            let notified = self.stopped.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.epoch() != epoch {
                return;
            }
            notified.await;
        }
    }

    pub fn status(&self) -> SpeechStatus {
        self.state.lock().unwrap().status.clone()
    }

    fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    fn pop(&self) -> Option<SpeechItem> {
        self.state.lock().unwrap().queue.pop()
    }

    #[cfg(test)]
    fn pop_for_test(&self) -> Option<SpeechItem> {
        self.pop()
    }

    /// 他モジュールのテストから、待ち項目を1件取り出す(`pop_for_test` は同モジュール専用)。
    #[cfg(test)]
    pub(crate) fn pop_for_command_test(&self) -> Option<SpeechItem> {
        self.pop()
    }

    /// 合成に成功した。状態が `Ok` に変わったら、通知すべき新しい状態を返す。
    fn record_success(&self) -> Option<SpeechStatus> {
        let mut s = self.state.lock().unwrap();
        s.failures = 0;
        if s.status == SpeechStatus::Ok {
            return None;
        }
        s.status = SpeechStatus::Ok;
        Some(SpeechStatus::Ok)
    }

    /// 合成に失敗した。連続失敗が閾値に達して `Error` に変わったら、新しい状態を返し、待ち項目を捨てる。
    fn record_failure(&self, message: String) -> Option<SpeechStatus> {
        let mut s = self.state.lock().unwrap();
        s.failures = s.failures.saturating_add(1);
        if s.failures < FAIL_THRESHOLD {
            return None;
        }
        s.queue.clear();
        let next = SpeechStatus::Error { message };
        // 既に `Error` なら、通知は出さない(文面は失敗のたびに変わりうるので、種類で比べる)。最新の文面は保持する。
        let already_error = matches!(s.status, SpeechStatus::Error { .. });
        s.status = next.clone();
        if already_error {
            None
        } else {
            Some(next)
        }
    }
}

/// 1件を処理する。失敗してもパニックも早期リターンもせず、状態に反映するだけ(ノートの受信を妨げない)。
pub(crate) async fn handle_item(mgr: &SpeechManager, ctx: &dyn WorkerContext, item: SpeechItem) {
    let epoch = mgr.epoch();
    // 全体・カラムのオン・オフは、積む前(`enqueue_note_core`)と、オフにしたとき(`stop_all` /
    // `remove_column`)に処理済み。ここで `enabled` を見ないのは、設定画面の「試し聞き」を、
    // 全体スイッチがオフでも鳴らすため。
    let settings = ctx.settings();
    let engine = match ctx.engine(&settings) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("読み上げ: エンジンを用意できません: {e}");
            if let Some(st) = mgr.record_failure(e.to_string()) {
                ctx.emit_status(st);
            }
            return;
        }
    };
    let wav = match engine.synthesize(&item.text, &item.voice_id, &item.params).await {
        Ok(w) => w,
        Err(e) => {
            log::warn!("読み上げ: 合成に失敗しました: {e}");
            if let Some(st) = mgr.record_failure(e.to_string()) {
                ctx.emit_status(st);
            }
            return;
        }
    };
    // 合成が成功した = エンジンは生きている。
    if let Some(st) = mgr.record_success() {
        ctx.emit_status(st);
    }
    if mgr.epoch() != epoch {
        // 合成している間に停止された。できあがった音声は鳴らさない。
        return;
    }
    let timeout = mgr.playback_timeout(&wav);
    let output = ctx.output();
    // 再生の完了と、「停止された(世代が進んだ)」を競わせる。`stop_all` と `stop_speech` が、`Speak` が再生スレッドに
    // 届く直前に割り込むと、`StopSpeech` が先に処理されて空振りし、音声が最後まで鳴ってしまう。世代が進んだことを
    // ここで見つけたら、`Speak` を送ったあとで、もう一度 `stop()` を送る。
    tokio::select! {
        r = tokio::time::timeout(timeout, output.speak(wav)) => match r {
            Ok(SpeakOutcome::Finished | SpeakOutcome::Stopped) => {}
            // 出力の失敗はエンジンの失敗ではない。状態には反映せず、ログに残して次へ進む。
            Ok(SpeakOutcome::Failed(m)) => log::warn!("読み上げ: 再生に失敗しました: {m}"),
            // 出力デバイスが壊れる・切断されると、再生が終わらないまま戻らない。ワーカーが無期限に止まらないよう、
            // 音声の長さ + 余白で打ち切って、出力を止める(再生スレッドは、次の命令でデバイスを開き直す)。
            Err(_) => {
                log::warn!("読み上げ: 再生が終わらないため中断しました(出力デバイスの異常の可能性)");
                output.stop();
            }
        },
        _ = mgr.wait_stopped(epoch) => output.stop(),
    }
}

/// ワーカーが panic で落ちたときの、再起動までの待ち(panic が続いても、busy loop にならない)。
pub const WORKER_RESTART_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// ワーカーを監視つきで動かす。ワーカーの task が panic で落ちても、ログを残して再起動し、読み上げを続ける
/// (処理中だった1件は失われる)。呼び出し側が spawn する。
pub async fn run_supervised(mgr: Arc<SpeechManager>, ctx: Arc<dyn WorkerContext>, restart_delay: std::time::Duration) {
    loop {
        match tokio::spawn(run_loop(mgr.clone(), ctx.clone())).await {
            // `run_loop` は戻らないので、ここに来るのは通常は panic のとき。
            Ok(()) => return,
            Err(e) => {
                log::error!("読み上げ: ワーカーが異常終了しました。再起動します: {e}");
                tokio::time::sleep(restart_delay).await;
            }
        }
    }
}

/// ワーカーのループ。待ち行列から1件ずつ取り出して `handle_item` する。呼び出し側が spawn する。
pub async fn run_loop(mgr: Arc<SpeechManager>, ctx: Arc<dyn WorkerContext>) {
    loop {
        let item = loop {
            if let Some(i) = mgr.pop() {
                break i;
            }
            mgr.wake.notified().await;
        };
        handle_item(&mgr, ctx.as_ref(), item).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SpeechSettings;
    use crate::error::Error;
    use crate::tts::engine::{SpeechEngine, SynthParams, Voice};
    use crate::tts::queue::SpeechItem;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    fn item(note: &str, col: &str) -> SpeechItem {
        SpeechItem {
            note_id: note.into(),
            column_id: col.into(),
            text: format!("text-{note}"),
            voice_id: "3".into(),
            params: SynthParams { speed_scale: 1.0, volume: 1.0 },
        }
    }

    /// 偽のエンジン。`fail` が真なら合成に失敗する。`on_synth` は合成の最中に呼ばれる(停止の競合の再現用)。
    struct FakeEngine {
        calls: Mutex<Vec<(String, String)>>,
        fail: Mutex<bool>,
        on_synth: Mutex<Option<Box<dyn Fn() + Send>>>,
    }
    impl FakeEngine {
        fn new() -> Arc<Self> {
            Arc::new(Self { calls: Default::default(), fail: Mutex::new(false), on_synth: Mutex::new(None) })
        }
    }
    #[async_trait]
    impl SpeechEngine for FakeEngine {
        async fn voices(&self) -> crate::error::Result<Vec<Voice>> {
            Ok(vec![])
        }
        async fn synthesize(&self, text: &str, voice_id: &str, _p: &SynthParams) -> crate::error::Result<Vec<u8>> {
            self.calls.lock().unwrap().push((text.into(), voice_id.into()));
            if let Some(f) = self.on_synth.lock().unwrap().as_ref() {
                f();
            }
            if *self.fail.lock().unwrap() {
                // 実際の reqwest のエラーは、リクエスト URL(= 本文を含む)を文面に含む。失敗ごとに文面が変わっても、
                // 「状態が変わったとき」だけ通知できることを確かめるため、ここでも本文を含める。
                return Err(crate::tts::invalid(format!("{}: down ({text})", crate::tts::ERR_UNREACHABLE)));
            }
            Ok(format!("RIFF:{text}").into_bytes())
        }
    }

    #[derive(Default)]
    struct FakeOutput {
        played: Mutex<Vec<Vec<u8>>>,
        stops: AtomicUsize,
    }
    #[async_trait]
    impl SpeechOutput for FakeOutput {
        async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome {
            self.played.lock().unwrap().push(wav);
            SpeakOutcome::Finished
        }
        fn stop(&self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct FakeCtx {
        settings: Mutex<SpeechSettings>,
        engine: Arc<FakeEngine>,
        output: Arc<FakeOutput>,
        statuses: Mutex<Vec<SpeechStatus>>,
    }
    impl FakeCtx {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                settings: Mutex::new(SpeechSettings { enabled: true, external_url: "http://x".into(), ..Default::default() }),
                engine: FakeEngine::new(),
                output: Arc::new(FakeOutput::default()),
                statuses: Default::default(),
            })
        }
        fn played(&self) -> Vec<String> {
            self.output.played.lock().unwrap().iter().map(|b| String::from_utf8_lossy(b).into_owned()).collect()
        }
    }
    impl WorkerContext for FakeCtx {
        fn settings(&self) -> SpeechSettings {
            self.settings.lock().unwrap().clone()
        }
        fn engine(&self, _s: &SpeechSettings) -> crate::error::Result<Arc<dyn SpeechEngine>> {
            Ok(self.engine.clone())
        }
        fn output(&self) -> Arc<dyn SpeechOutput> {
            self.output.clone()
        }
        fn emit_status(&self, status: SpeechStatus) {
            self.statuses.lock().unwrap().push(status);
        }
    }

    #[tokio::test]
    async fn a_successful_item_is_synthesized_with_its_voice_played_and_marks_ok_once() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        handle_item(&mgr, ctx.as_ref(), item("n1", "a")).await;
        handle_item(&mgr, ctx.as_ref(), item("n2", "a")).await;

        assert_eq!(
            *ctx.engine.calls.lock().unwrap(),
            vec![("text-n1".to_string(), "3".to_string()), ("text-n2".to_string(), "3".to_string())]
        );
        assert_eq!(ctx.played(), ["RIFF:text-n1", "RIFF:text-n2"]);
        assert_eq!(*ctx.statuses.lock().unwrap(), vec![SpeechStatus::Ok], "状態が変わったときだけ1回通知する");
        assert_eq!(mgr.status(), SpeechStatus::Ok);
    }

    #[tokio::test]
    async fn an_item_is_spoken_even_when_the_master_switch_is_off_so_the_try_button_works() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        ctx.settings.lock().unwrap().enabled = false;
        handle_item(&mgr, ctx.as_ref(), item("n1", "__test__")).await;
        assert_eq!(ctx.played(), ["RIFF:text-n1"], "オフでも積まれた項目は読む(積む前の判定が責務)");
    }

    #[tokio::test]
    async fn three_consecutive_failures_report_an_error_once_and_clear_the_queue() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        *ctx.engine.fail.lock().unwrap() = true;
        for i in 0..5 {
            mgr.enqueue(item(&format!("q{i}"), "a"), 20);
        }
        for i in 0..FAIL_THRESHOLD {
            handle_item(&mgr, ctx.as_ref(), item(&format!("f{i}"), "a")).await;
        }
        let st = ctx.statuses.lock().unwrap().clone();
        assert_eq!(st.len(), 1, "閾値に達したときだけ1回: {st:?}");
        assert!(matches!(&st[0], SpeechStatus::Error { message } if message.contains(crate::tts::ERR_UNREACHABLE)), "{st:?}");
        assert!(mgr.pop_for_test().is_none(), "到達不能なら溜まった待ち項目は捨てる");

        // さらに失敗しても、同じ状態なので再通知しない。
        handle_item(&mgr, ctx.as_ref(), item("f9", "a")).await;
        assert_eq!(ctx.statuses.lock().unwrap().len(), 1);

        // 回復したら Ok に戻る。
        *ctx.engine.fail.lock().unwrap() = false;
        handle_item(&mgr, ctx.as_ref(), item("ok", "a")).await;
        assert_eq!(ctx.statuses.lock().unwrap().last(), Some(&SpeechStatus::Ok));
        assert_eq!(ctx.played(), ["RIFF:text-ok"]);
    }

    #[tokio::test]
    async fn a_failure_below_the_threshold_does_not_change_the_status() {
        let mgr = SpeechManager::new();
        let ctx = FakeCtx::new();
        *ctx.engine.fail.lock().unwrap() = true;
        handle_item(&mgr, ctx.as_ref(), item("f1", "a")).await;
        handle_item(&mgr, ctx.as_ref(), item("f2", "a")).await;
        assert!(ctx.statuses.lock().unwrap().is_empty());
        // 途中で成功すれば、失敗の連続は数え直す。
        *ctx.engine.fail.lock().unwrap() = false;
        handle_item(&mgr, ctx.as_ref(), item("ok", "a")).await;
        *ctx.engine.fail.lock().unwrap() = true;
        handle_item(&mgr, ctx.as_ref(), item("f3", "a")).await;
        handle_item(&mgr, ctx.as_ref(), item("f4", "a")).await;
        assert_eq!(
            *ctx.statuses.lock().unwrap(),
            vec![SpeechStatus::Ok],
            "成功で 0 に戻っているので、あと1回失敗しても Error にならない"
        );
    }

    #[tokio::test]
    async fn stopping_while_synthesizing_discards_the_audio() {
        let mgr = Arc::new(SpeechManager::new());
        let ctx = FakeCtx::new();
        let m2 = mgr.clone();
        *ctx.engine.on_synth.lock().unwrap() = Some(Box::new(move || m2.stop_all()));
        handle_item(&mgr, ctx.as_ref(), item("n1", "a")).await;
        assert!(ctx.played().is_empty(), "合成中に停止されたら、できあがった音声は再生しない");
        assert!(ctx.statuses.lock().unwrap().iter().all(|s| !matches!(s, SpeechStatus::Error { .. })), "停止は失敗ではない");
    }

    #[tokio::test]
    async fn stop_all_clears_the_queue() {
        let mgr = SpeechManager::new();
        mgr.enqueue(item("n1", "a"), 5);
        mgr.stop_all();
        assert!(mgr.pop_for_test().is_none());
    }

    #[tokio::test]
    async fn remove_column_drops_only_that_columns_pending_items() {
        let mgr = SpeechManager::new();
        mgr.enqueue(item("a1", "a"), 5);
        mgr.enqueue(item("b1", "b"), 5);
        assert_eq!(mgr.remove_column("a"), 1);
        assert_eq!(mgr.pop_for_test().unwrap().note_id, "b1");
    }

    /// 再生が永遠に終わらない出力(出力デバイスが壊れた・切断されたときの再現)。
    #[derive(Default)]
    struct HangingOutput {
        stops: AtomicUsize,
    }
    #[async_trait]
    impl SpeechOutput for HangingOutput {
        async fn speak(&self, _wav: Vec<u8>) -> SpeakOutcome {
            std::future::pending().await
        }
        fn stop(&self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// 1秒ぶん(8kHz・モノラル・16bit)の WAV。
    fn wav_of_one_second() -> Vec<u8> {
        let data = vec![0u8; 16_000];
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&8000u32.to_le_bytes());
        v.extend_from_slice(&16_000u32.to_le_bytes()); // byte rate
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(&data);
        v
    }

    #[test]
    fn wav_duration_reads_the_length_from_the_header() {
        assert_eq!(wav_duration(&wav_of_one_second()), Some(Duration::from_secs(1)));
    }

    #[test]
    fn wav_duration_uses_the_remaining_bytes_when_the_data_size_is_unknown_or_too_big() {
        let mut w = wav_of_one_second();
        let at = w.len() - 16_000 - 4; // data チャンクのサイズ欄
        w[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes()); // ストリーミングの WAV は 0xFFFFFFFF を入れることがある
        assert_eq!(wav_duration(&w), Some(Duration::from_secs(1)));
    }

    #[test]
    fn wav_duration_is_none_for_garbage_or_a_truncated_header() {
        assert_eq!(wav_duration(b""), None);
        assert_eq!(wav_duration(b"RIFF:not a wav"), None);
        assert_eq!(wav_duration(&wav_of_one_second()[..30]), None);
    }

    #[test]
    fn the_playback_timeout_is_the_audio_length_plus_a_margin_and_has_a_fallback() {
        let mgr = SpeechManager::new().with_playback_margin(Duration::from_millis(250));
        assert_eq!(mgr.playback_timeout(&wav_of_one_second()), Duration::from_millis(1250));
        assert_eq!(mgr.playback_timeout(b"RIFF:not a wav"), FALLBACK_PLAYBACK_TIMEOUT, "長さが読めなければ既定の上限");
    }

    #[tokio::test]
    async fn a_playback_that_never_finishes_is_cut_off_and_stopped_instead_of_wedging_the_worker() {
        struct WavEngine;
        #[async_trait]
        impl SpeechEngine for WavEngine {
            async fn voices(&self) -> crate::error::Result<Vec<Voice>> {
                Ok(vec![])
            }
            async fn synthesize(&self, _t: &str, _v: &str, _p: &SynthParams) -> crate::error::Result<Vec<u8>> {
                // 長さ 0 の WAV: 待ち時間は余白だけになる。
                let mut w = wav_of_one_second();
                w.truncate(44);
                Ok(w)
            }
        }
        struct HangCtx(Arc<HangingOutput>, Arc<FakeCtx>);
        impl WorkerContext for HangCtx {
            fn settings(&self) -> SpeechSettings {
                self.1.settings()
            }
            fn engine(&self, _s: &SpeechSettings) -> crate::error::Result<Arc<dyn SpeechEngine>> {
                Ok(Arc::new(WavEngine))
            }
            fn output(&self) -> Arc<dyn SpeechOutput> {
                self.0.clone()
            }
            fn emit_status(&self, s: SpeechStatus) {
                self.1.emit_status(s)
            }
        }
        let mgr = SpeechManager::new().with_playback_margin(Duration::from_millis(50));
        let out = Arc::new(HangingOutput::default());
        let ctx = HangCtx(out.clone(), FakeCtx::new());

        // 再生が戻らなくても、余白のぶんで打ち切って `stop()` し、次へ進める。
        tokio::time::timeout(Duration::from_secs(5), handle_item(&mgr, &ctx, item("n1", "a")))
            .await
            .expect("再生が終わらなくても、ワーカーは先へ進む");
        assert_eq!(out.stops.load(Ordering::SeqCst), 1, "打ち切ったら出力を止める(再生スレッドが壊れた出力デバイスを開き直すきっかけにもなる)");
    }

    /// 「`Speak` が再生スレッドに届く直前に、停止が割り込む」競合の再現。この停止(`StopSpeech`)は `Speak` より先に
    /// 処理されて空振りする。`Speak` の**後**に届いた `stop()` だけが、再生を止められる。
    struct RaceOutput {
        mgr: Arc<SpeechManager>,
        started: std::sync::atomic::AtomicBool,
        stopped_after_start: tokio::sync::Notify,
    }
    #[async_trait]
    impl SpeechOutput for RaceOutput {
        async fn speak(&self, _wav: Vec<u8>) -> SpeakOutcome {
            self.mgr.stop_all(); // 世代が進む(本番では、この直後に `stop_speech` が走り、`Speak` より先に届く)
            self.started.store(true, Ordering::SeqCst);
            self.stopped_after_start.notified().await;
            SpeakOutcome::Stopped
        }
        fn stop(&self) {
            if self.started.load(Ordering::SeqCst) {
                self.stopped_after_start.notify_one();
            }
        }
    }

    #[tokio::test]
    async fn a_stop_that_lands_just_before_the_speak_command_still_silences_it() {
        struct RaceCtx(Arc<RaceOutput>, Arc<FakeCtx>);
        impl WorkerContext for RaceCtx {
            fn settings(&self) -> SpeechSettings {
                self.1.settings()
            }
            fn engine(&self, s: &SpeechSettings) -> crate::error::Result<Arc<dyn SpeechEngine>> {
                self.1.engine(s)
            }
            fn output(&self) -> Arc<dyn SpeechOutput> {
                self.0.clone()
            }
            fn emit_status(&self, s: SpeechStatus) {
                self.1.emit_status(s)
            }
        }
        let mgr = Arc::new(SpeechManager::new());
        let out = Arc::new(RaceOutput {
            mgr: mgr.clone(),
            started: Default::default(),
            stopped_after_start: Default::default(),
        });
        let ctx = RaceCtx(out, FakeCtx::new());
        // 世代が変わったら、`Speak` の後でもう一度 `stop()` を送る。これが無いと、再生は最後まで鳴る
        // (長さの読めない音声は、60 秒の上限まで戻らない)。
        tokio::time::timeout(Duration::from_secs(3), handle_item(&mgr, &ctx, item("n1", "a")))
            .await
            .expect("停止が割り込んでも、再生を止めてワーカーは先へ進む");
    }

    /// 最初の1回だけ panic する窓口(Mutex の poison などで、ワーカーが落ちる場合の再現)。
    struct PanicOnceCtx {
        inner: Arc<FakeCtx>,
        panicked: std::sync::atomic::AtomicBool,
    }
    impl WorkerContext for PanicOnceCtx {
        fn settings(&self) -> SpeechSettings {
            if !self.panicked.swap(true, Ordering::SeqCst) {
                panic!("boom (simulated worker panic)");
            }
            self.inner.settings()
        }
        fn engine(&self, s: &SpeechSettings) -> crate::error::Result<Arc<dyn SpeechEngine>> {
            self.inner.engine(s)
        }
        fn output(&self) -> Arc<dyn SpeechOutput> {
            self.inner.output()
        }
        fn emit_status(&self, s: SpeechStatus) {
            self.inner.emit_status(s)
        }
    }

    #[tokio::test]
    async fn a_panicking_worker_is_restarted_and_keeps_speaking() {
        let mgr = Arc::new(SpeechManager::new());
        let inner = FakeCtx::new();
        let ctx = Arc::new(PanicOnceCtx { inner: inner.clone(), panicked: Default::default() });
        let worker = tokio::spawn(run_supervised(mgr.clone(), ctx, Duration::from_millis(10)));
        mgr.enqueue(item("n1", "a"), 5); // 最初の1件は、処理中の panic で失われる
        mgr.enqueue(item("n2", "a"), 5);

        tokio::time::timeout(Duration::from_secs(5), async {
            while inner.played().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("panic のあとも、ワーカーが再起動して読み上げを続ける");
        assert_eq!(inner.played(), ["RIFF:text-n2"]);
        worker.abort();
    }

    #[tokio::test]
    async fn the_run_loop_speaks_queued_items_in_order() {
        let mgr = Arc::new(SpeechManager::new());
        let ctx = FakeCtx::new();
        let worker = tokio::spawn(run_loop(mgr.clone(), ctx.clone()));
        mgr.enqueue(item("n1", "a"), 5);
        mgr.enqueue(item("n2", "b"), 5);
        mgr.enqueue(item("n3", "a"), 5);

        tokio::time::timeout(Duration::from_secs(5), async {
            while ctx.played().len() < 3 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("3件とも再生される");
        assert_eq!(ctx.played(), ["RIFF:text-n1", "RIFF:text-n2", "RIFF:text-n3"]);
        worker.abort();
    }

    #[tokio::test]
    async fn an_engine_construction_error_counts_as_a_failure() {
        struct BadCtx(Arc<FakeCtx>);
        impl WorkerContext for BadCtx {
            fn settings(&self) -> SpeechSettings {
                self.0.settings()
            }
            fn engine(&self, _s: &SpeechSettings) -> crate::error::Result<Arc<dyn SpeechEngine>> {
                Err(Error::Invalid(crate::tts::ERR_NOT_CONFIGURED.into()))
            }
            fn output(&self) -> Arc<dyn SpeechOutput> {
                self.0.output()
            }
            fn emit_status(&self, s: SpeechStatus) {
                self.0.emit_status(s)
            }
        }
        let mgr = SpeechManager::new();
        let ctx = BadCtx(FakeCtx::new());
        for i in 0..FAIL_THRESHOLD {
            handle_item(&mgr, &ctx, item(&format!("n{i}"), "a")).await;
        }
        let st = ctx.0.statuses.lock().unwrap().clone();
        assert!(matches!(&st[..], [SpeechStatus::Error { message }] if message.contains(crate::tts::ERR_NOT_CONFIGURED)), "{st:?}");
    }
}
