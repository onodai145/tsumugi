//! 読み上げの再生レーン(Issue #625)。`SoundPlayer` の再生スレッドの中で動く小さな部品で、
//! rodio の `Player`(`stop()` / `empty()` を持つ)を1発話ごとに作り、逐次再生・停止・完了通知を行う。
//! デバイスに触れないので、`Player::new()` が返すキューの出力を手で読み進めることで単体テストできる。

use rodio::buffer::SamplesBuffer;
use rodio::Player;
use tokio::sync::oneshot;

/// 1発話の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeakOutcome {
    /// 最後まで再生した。
    Finished,
    /// 途中で止められた(`stop_speech`・新しい発話への置き換え)。
    Stopped,
    /// 再生できなかった(デコード失敗・出力デバイスの再オープン・再生スレッドの終了)。
    Failed(String),
}

pub type DoneTx = oneshot::Sender<SpeakOutcome>;

#[derive(Default)]
pub struct SpeechLane {
    /// 再生中の発話。`Player` を落とすと音が止まる(rodio の `Drop`)。
    active: Option<(Player, DoneTx)>,
}

impl SpeechLane {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// 発話を始める。再生中の発話があれば、先に `Stopped` で終わらせる。
    pub fn start(&mut self, player: Player, buf: SamplesBuffer, done: DoneTx) {
        self.finish(SpeakOutcome::Stopped);
        player.append(buf);
        self.active = Some((player, done));
    }

    pub fn stop(&mut self) {
        self.finish(SpeakOutcome::Stopped);
    }

    pub fn fail(&mut self, reason: &str) {
        self.finish(SpeakOutcome::Failed(reason.to_string()));
    }

    /// 再生が終わっていれば `Finished` を通知して `true`。再生中・何も無いときは `false`。
    pub fn poll(&mut self) -> bool {
        let finished = matches!(&self.active, Some((p, _)) if p.empty());
        if finished {
            self.finish(SpeakOutcome::Finished);
        }
        finished
    }

    fn finish(&mut self, outcome: SpeakOutcome) {
        if let Some((player, done)) = self.active.take() {
            drop(player);
            // 受信側(待っている側)が既にいなくても構わない。
            let _ = done.send(outcome);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use rodio::Player;
    use std::num::{NonZeroU16, NonZeroU32};
    use tokio::sync::oneshot;

    fn buf(len: usize) -> SamplesBuffer {
        SamplesBuffer::new(
            NonZeroU16::new(1).unwrap(),
            NonZeroU32::new(8000).unwrap(),
            vec![0.1f32; len],
        )
    }

    /// キューの出力から `n` サンプルを読み進める(オーディオスレッドが再生するのと同じ動き)。
    fn drain(out: &mut impl Iterator<Item = f32>, n: usize) {
        for _ in 0..n {
            out.next();
        }
    }

    #[test]
    fn a_fresh_lane_is_idle() {
        let mut lane = SpeechLane::new();
        assert!(!lane.is_active());
        assert!(!lane.poll());
    }

    #[test]
    fn poll_reports_finished_once_the_buffer_has_been_played() {
        let mut lane = SpeechLane::new();
        let (player, mut out) = Player::new();
        let (tx, mut rx) = oneshot::channel();
        lane.start(player, buf(200), tx);
        assert!(lane.is_active());
        assert!(!lane.poll(), "まだ再生が終わっていない");
        assert!(rx.try_recv().is_err());

        drain(&mut out, 5_000);
        assert!(lane.poll(), "全サンプルを再生し終えた");
        assert_eq!(rx.try_recv().unwrap(), SpeakOutcome::Finished);
        assert!(!lane.is_active());
    }

    #[test]
    fn stop_ends_the_active_speech_with_stopped() {
        let mut lane = SpeechLane::new();
        let (player, _out) = Player::new();
        let (tx, mut rx) = oneshot::channel();
        lane.start(player, buf(200), tx);
        lane.stop();
        assert_eq!(rx.try_recv().unwrap(), SpeakOutcome::Stopped);
        assert!(!lane.is_active());
        lane.stop(); // 何もないときの stop は無害
    }

    #[test]
    fn starting_a_new_speech_stops_the_previous_one() {
        let mut lane = SpeechLane::new();
        let (p1, _o1) = Player::new();
        let (t1, mut r1) = oneshot::channel();
        lane.start(p1, buf(200), t1);
        let (p2, _o2) = Player::new();
        let (t2, mut r2) = oneshot::channel();
        lane.start(p2, buf(200), t2);
        assert_eq!(r1.try_recv().unwrap(), SpeakOutcome::Stopped);
        assert!(r2.try_recv().is_err());
        assert!(lane.is_active());
    }

    #[test]
    fn fail_reports_the_reason() {
        let mut lane = SpeechLane::new();
        let (player, _out) = Player::new();
        let (tx, mut rx) = oneshot::channel();
        lane.start(player, buf(200), tx);
        lane.fail("device reopened");
        assert_eq!(
            rx.try_recv().unwrap(),
            SpeakOutcome::Failed("device reopened".into())
        );
    }

    #[test]
    fn a_dropped_receiver_does_not_panic() {
        let mut lane = SpeechLane::new();
        let (player, _out) = Player::new();
        let (tx, rx) = oneshot::channel();
        drop(rx);
        lane.start(player, buf(200), tx);
        lane.stop(); // 受信側がいなくても panic しない
    }
}
