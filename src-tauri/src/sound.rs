//! 通知音のネイティブ再生。webview の AudioContext 自動再生ポリシーに左右されないよう、
//! Rust 側で rodio 経由で鳴らす(Issue #12)。
//!
//! 出力デバイスは起動時に一度だけ開いてバックグラウンドスレッドに保持する。
//! 再生要求はチャンネル経由で送るだけで即座に返り、実際のデコード/再生は
//! そのスレッド内で行う。複数の通知音が重なっても Mixer が重畳して鳴らすため、
//! 呼び出しごとに出力デバイスを開き直す必要はない。

use rodio::{Decoder, DeviceSinkBuilder};
use std::io::Cursor;
use std::sync::mpsc;

pub struct SoundPlayer {
    tx: mpsc::Sender<Vec<u8>>,
}

impl SoundPlayer {
    /// 再生用バックグラウンドスレッドを起動する。出力デバイスが無い等で開けない
    /// 場合はログを残してスレッドを終了する(以降の`play`はログのみで何も鳴らさない)。
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut handle = match DeviceSinkBuilder::open_default_sink() {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("通知音: 出力デバイスの取得に失敗: {e}");
                    return;
                }
            };
            handle.log_on_drop(false);
            for bytes in rx {
                let cursor = Cursor::new(bytes);
                match Decoder::try_from(cursor) {
                    Ok(source) => handle.mixer().add(source),
                    Err(e) => log::warn!("通知音: デコードに失敗: {e}"),
                }
            }
        });
        Self { tx }
    }

    /// バイト列を非同期に再生する(デコード/再生はバックグラウンドスレッドで行うため即座に返る)。
    pub fn play(&self, bytes: Vec<u8>) {
        if self.tx.send(bytes).is_err() {
            log::warn!("通知音: 再生スレッドが終了しているため再生できません");
        }
    }
}
