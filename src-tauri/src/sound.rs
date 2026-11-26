//! 通知音のネイティブ再生。webview の AudioContext 自動再生ポリシーに左右されないよう、
//! Rust 側で rodio 経由で鳴らす(Issue #12)。
//!
//! 出力デバイスは起動時に一度だけ開いてバックグラウンドスレッドに保持する。
//! 再生要求はチャンネル経由で送るだけで即座に返り、実際のデコード/再生は
//! そのスレッド内で行う。複数の通知音が重なっても Mixer が重畳して鳴らすため、
//! 呼び出しごとに出力デバイスを開き直す必要はない。
//!
//! デコードはこのスレッド上で完結させる(`decode_fully`)。rodio の `Decoder` は
//! `Decoder::try_from` でヘッダのみを検証し、実際のパケットごとのデコードは何もしなければ
//! cpal のオーディオコールバックスレッド上で遅延実行されるため、不正なファイルによる
//! パニックがそちらで起きるとこのスレッドの channel は健全なまま(`play` は成功し続ける)
//! なのに二度と音が鳴らなくなる、というログに残らない障害が起こり得る(Issue #12 が
//! 診断困難だった原因そのもの)。事前に全サンプルを読み切って `SamplesBuffer` に詰め替える
//! ことで、デコードはこのスレッド上の `catch_unwind` の内側で完結し、Mixer/cpal 側は
//! 単純なバッファ読み出ししか行わなくなる。
//!
//! 出力デバイス自体が失われた場合(ヘッドフォン抜去、Android のバックグラウンド化による
//! ストリーム破棄など)は、decode の成否とは無関係に cpal の `StreamError` として
//! 非同期に通知される。`open_sink` で登録する `error_callback` がそれを検知して
//! `healthy` フラグを倒し、次の `play()` 処理時に出力デバイスを開き直す。
//!
//! なお、このファイルの `log::warn!` は `enable_file_logging` 設定(既定オフ)が有効な
//! 場合のみファイルに残る。設定していないユーザには見えない点に注意(Issue #12 の設計上の
//! 既知の制約。ロギング基盤自体の変更は本 Issue のスコープ外)。
//!
//! 読み上げ(Issue #625)は、同じ再生スレッドの中に `SpeechLane`(`speech_lane.rs`)を持つ。
//! 通知音(`Cmd::Play`)とは別の `rodio::Player` で逐次再生し、完了・停止を `speak()` の戻り値で返す。
//! 通知音側の挙動(`play` の即時復帰・Mixer への重畳)は変えない。

use crate::speech_lane::{DoneTx, SpeakOutcome, SpeechLane};
use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{mpsc, Arc};
use std::time::Duration;

/// 再生スレッドへの命令。`Play` は従来どおりの投げっぱなしの通知音、`Speak` / `StopSpeech` は読み上げレーン(Issue #625)。
enum Cmd {
    Play(Vec<u8>),
    Speak { wav: Vec<u8>, done: DoneTx },
    StopSpeech,
}

/// 読み上げレーンの完了を確認する間隔。レーンが空いている間は、これまでどおりブロッキングで待つ(ポーリングしない)。
const LANE_POLL_INTERVAL: Duration = Duration::from_millis(40);

#[derive(Clone)]
pub struct SoundPlayer {
    tx: mpsc::Sender<Cmd>,
}

/// `decode_fully` の結果。
enum DecodeOutcome {
    /// 再生可能なサンプル列に変換できた。
    Ready(SamplesBuffer),
    /// バイト列がそもそも音声として解釈できなかった(デコード対象の問題。デバイスとは無関係)。
    Invalid(String),
    /// デコード処理そのものがパニックした(不正なファイルによるデコーダのバグ等)。
    /// これはデコード対象データの問題であり、出力デバイス側の異常を意味しない
    /// (デバイス異常の検知/再オープンは `error_callback`/`healthy` フラグの役目)。
    Panicked,
}

/// パニックする可能性のある処理を実行し、パニックした場合は捕まえて `Panicked` を返す。
/// デバイス I/O から分離した純粋なラッパなので、実際のパニックを起こす任意のクロージャで
/// 単体テストできる。
fn catch_decode_panic(f: impl FnOnce() -> DecodeOutcome + std::panic::UnwindSafe) -> DecodeOutcome {
    match std::panic::catch_unwind(f) {
        Ok(outcome) => outcome,
        Err(_) => DecodeOutcome::Panicked,
    }
}

/// バイト列を全サンプル読み切った `SamplesBuffer` にデコードする。
/// cpal のオーディオコールバックスレッドではなく、呼び出し元のスレッド上で
/// (この関数の中で) デコードが完結するため、`catch_decode_panic` でパニックを捕捉できる。
/// 副作用を持たないため単体テストで直接検証できる(デバイス I/O から分離)。
fn decode_fully(bytes: Vec<u8>) -> DecodeOutcome {
    catch_decode_panic(move || {
        let cursor = Cursor::new(bytes);
        match Decoder::try_from(cursor) {
            Ok(source) => {
                let channels = source.channels();
                let sample_rate = source.sample_rate();
                let samples: Vec<f32> = source.collect();
                DecodeOutcome::Ready(SamplesBuffer::new(channels, sample_rate, samples))
            }
            Err(e) => DecodeOutcome::Invalid(e.to_string()),
        }
    })
}

/// ogg(Vorbis)が Android の触覚チャンネル入りかを判定する(Issue #592)。
///
/// 触覚入りの ogg は、Vorbis コメントに `ANDROID_HAPTIC=<触覚チャンネル数>` を持つ
/// (AOSP「UX foundation for haptic framework」)。次をすべて満たすときだけ true:
/// 先頭が `OggS` / Vorbis のストリームのコメントヘッダ(`\x03vorbis`)が読める /
/// `ANDROID_HAPTIC`(大文字小文字を区別しない)の値が 1 以上の整数。
///
/// コメントヘッダは、カバー画像入りなどで大きいと複数の Ogg ページに分かれるので、
/// ページをつなげて読む(Issue #654)。Opus は対象外。壊れたデータでもパニックせず false を返す。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn has_haptic_channel(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"OggS") {
        return false;
    }
    let Some(packet) = read_vorbis_comment_packet(bytes) else {
        return false;
    };
    let Some(comments) = packet.strip_prefix(b"\x03vorbis") else {
        return false;
    };
    let mut cursor = 0usize;
    let Some(vendor_len) = read_u32_le(comments, &mut cursor) else {
        return false;
    };
    if read_slice(comments, &mut cursor, vendor_len as usize).is_none() {
        return false;
    }
    let Some(count) = read_u32_le(comments, &mut cursor) else {
        return false;
    };
    // count が巨大でも、各反復で必ず読み進めるか None で抜けるので、バッファの長さで止まる
    for _ in 0..count {
        let Some(len) = read_u32_le(comments, &mut cursor) else {
            return false;
        };
        let Some(comment) = read_slice(comments, &mut cursor, len as usize) else {
            return false;
        };
        if let Some(is_haptic) = parse_haptic_comment(comment) {
            return is_haptic;
        }
    }
    false
}

/// 最初の Vorbis ストリームの 2 番目のパケット(コメントヘッダ)を、ページをつなげて組み立てる。
/// 通知のたびに走るので、パケットが終わった所で読むのをやめる。ファイルが途中で切れている・
/// ページが壊れているときは、そこまでに読めた分を返す。CRC は検証しない。
fn read_vorbis_comment_packet(mut bytes: &[u8]) -> Option<Vec<u8>> {
    let mut vorbis_serial = None;
    let mut packet = Vec::new();
    while let Some(page) = read_ogg_page(&mut bytes) {
        match vorbis_serial {
            // 識別ヘッダは単独で 1 ページになるので、コメントヘッダは同じストリームの次のページから始まる
            None if page.body.starts_with(b"\x01vorbis") => vorbis_serial = Some(page.serial),
            Some(serial) if serial == page.serial => {
                let mut body = page.body;
                for &segment_len in page.lacing {
                    let segment_len = usize::from(segment_len);
                    let (segment, rest) = body.split_at(segment_len.min(body.len()));
                    packet.extend_from_slice(segment);
                    body = rest;
                    // 255 未満のセグメントでパケットが終わる
                    if segment_len < 255 {
                        return Some(packet);
                    }
                }
            }
            _ => {}
        }
    }
    (!packet.is_empty()).then_some(packet)
}

struct OggPage<'a> {
    serial: u32,
    /// セグメントテーブル(各セグメントの長さ)
    lacing: &'a [u8],
    /// ファイルが途中で切れていると、`lacing` の合計より短い
    body: &'a [u8],
}

/// 先頭の Ogg ページを読んで、`bytes` をその後ろまで進める。ページでなければ None。
fn read_ogg_page<'a>(bytes: &mut &'a [u8]) -> Option<OggPage<'a>> {
    let data = *bytes;
    if !data.starts_with(b"OggS") {
        return None;
    }
    let mut cursor = 14usize; // capture pattern・バージョン・種別・granule position の後
    let serial = read_u32_le(data, &mut cursor)?;
    cursor += 8; // ページ番号・CRC
    let segment_count = usize::from(*read_slice(data, &mut cursor, 1)?.first()?);
    let lacing = read_slice(data, &mut cursor, segment_count)?;
    let body_len: usize = lacing.iter().map(|&n| usize::from(n)).sum();
    let (body, rest) = data[cursor..].split_at(body_len.min(data.len() - cursor));
    *bytes = rest;
    Some(OggPage {
        serial,
        lacing,
        body,
    })
}

fn read_u32_le(buf: &[u8], pos: &mut usize) -> Option<u32> {
    let end = pos.checked_add(4)?;
    let value = u32::from_le_bytes(buf.get(*pos..end)?.try_into().ok()?);
    *pos = end;
    Some(value)
}

fn read_slice<'a>(buf: &'a [u8], pos: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = pos.checked_add(len)?;
    let slice = buf.get(*pos..end)?;
    *pos = end;
    Some(slice)
}

/// `KEY=VALUE` のコメントが `ANDROID_HAPTIC` なら、値が 1 以上の整数かを返す。別のキーなら None。
fn parse_haptic_comment(comment: &[u8]) -> Option<bool> {
    let eq = comment.iter().position(|&b| b == b'=')?;
    let (key, value) = (&comment[..eq], &comment[eq + 1..]);
    if !key.eq_ignore_ascii_case(b"ANDROID_HAPTIC") {
        return None;
    }
    let channels = std::str::from_utf8(value)
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    Some(channels.is_some_and(|n| n >= 1))
}

/// 出力デバイスを(再)オープンする。`healthy` にはストリームエラー発生時に降ろされる
/// フラグを登録する(cpal の `error_callback` は再生中いつでも別スレッドから非同期に
/// 呼ばれ得るため、成否をここで同期的に受け取る手段がない)。
///
/// `error_callback` を差し込むには rodio 0.22 の API 上 `DeviceSinkBuilder::from_default_device()`
/// を使う必要があり、`open_default_sink()` が持つ「他の出力デバイスへのフォールバック探索」は
/// そのままでは使えない(`with_error_callback` はビルダーインスタンスに対してのみ呼べ、
/// 静的関数の `open_default_sink()` はそのビルダーを外に出さない)。ただし同一デバイス内の
/// 代替設定探索(`open_sink_or_fallback`)は使えるため、デフォルト出力デバイス自体が
/// 開けない場合の完全な他デバイス探索のみ諦めている。それでも開けない場合は失敗として
/// 扱う(次の `play()` で再度この関数を呼ぶことで自然にリトライされる)。
fn open_sink(healthy: &Arc<AtomicBool>) -> Option<MixerDeviceSink> {
    let flag = Arc::clone(healthy);
    let result = DeviceSinkBuilder::from_default_device().and_then(|b| {
        b.with_error_callback(move |err| {
            log::warn!("通知音: 出力ストリームでエラーが発生しました: {err}");
            flag.store(false, Ordering::SeqCst);
        })
        .open_sink_or_fallback()
    });
    match result {
        Ok(mut h) => {
            h.log_on_drop(false);
            healthy.store(true, Ordering::SeqCst);
            Some(h)
        }
        Err(e) => {
            log::warn!("通知音: 出力デバイスの取得に失敗: {e}");
            None
        }
    }
}

impl SoundPlayer {
    /// 再生用バックグラウンドスレッドを起動する。出力デバイスが無い等で開けない
    /// 場合はログを残してスレッドを終了する(以降の`play`はログのみで何も鳴らさない)。
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::spawn(move || {
            let healthy = Arc::new(AtomicBool::new(true));
            let mut handle = match open_sink(&healthy) {
                Some(h) => h,
                None => return,
            };
            let mut lane = SpeechLane::new();

            loop {
                // 読み上げ中だけ短い間隔で起きて完了を確認する。何も再生していなければ、従来どおりブロッキングで待つ。
                let cmd = if lane.is_active() {
                    match rx.recv_timeout(LANE_POLL_INTERVAL) {
                        Ok(c) => Some(c),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                } else {
                    match rx.recv() {
                        Ok(c) => Some(c),
                        Err(_) => break,
                    }
                };
                lane.poll();
                let Some(cmd) = cmd else { continue };

                if !healthy.load(Ordering::SeqCst) {
                    // 出力デバイスを開き直すと、古い Mixer 上の読み上げは失われる。
                    lane.fail("出力デバイスを開き直したため読み上げを中断しました");
                    match open_sink(&healthy) {
                        Some(h) => {
                            handle = h;
                            log::warn!("通知音: 出力ストリーム異常を検知したため出力デバイスを再オープンしました");
                        }
                        None => {
                            log::warn!(
                                "通知音: 出力ストリーム異常を検知しましたが再オープンに失敗したため、既存のデバイスで再生を試みます"
                            );
                        }
                    }
                }

                match cmd {
                    Cmd::Play(bytes) => match decode_fully(bytes) {
                        DecodeOutcome::Ready(buf) => handle.mixer().add(buf),
                        DecodeOutcome::Invalid(e) => log::warn!("通知音: デコードに失敗: {e}"),
                        DecodeOutcome::Panicked => {
                            log::warn!("通知音: デコード処理がパニックしました(不正なファイルの可能性)。この音声はスキップします");
                        }
                    },
                    Cmd::Speak { wav, done } => match decode_fully(wav) {
                        DecodeOutcome::Ready(buf) => {
                            lane.start(Player::connect_new(handle.mixer()), buf, done);
                        }
                        DecodeOutcome::Invalid(e) => {
                            let _ = done.send(SpeakOutcome::Failed(format!("デコードに失敗: {e}")));
                        }
                        DecodeOutcome::Panicked => {
                            let _ = done.send(SpeakOutcome::Failed(
                                "デコード処理がパニックしました".into(),
                            ));
                        }
                    },
                    Cmd::StopSpeech => lane.stop(),
                }
            }
        });
        Self { tx }
    }

    /// バイト列を非同期に再生する(デコード/再生はバックグラウンドスレッドで行うため即座に返る)。
    pub fn play(&self, bytes: Vec<u8>) {
        if self.tx.send(Cmd::Play(bytes)).is_err() {
            log::warn!("通知音: 再生スレッドが終了しているため再生できません");
        }
    }

    /// 読み上げの音声(WAV)を、読み上げレーンで再生し、終わる(または止められる・失敗する)まで待つ(Issue #625)。
    /// 通知音(`play`)とは Mixer 上でミックスされる。
    pub async fn speak(&self, wav: Vec<u8>) -> SpeakOutcome {
        let (done, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(Cmd::Speak { wav, done }).is_err() {
            return SpeakOutcome::Failed("再生スレッドが終了しているため再生できません".into());
        }
        rx.await
            .unwrap_or_else(|_| SpeakOutcome::Failed("再生スレッドが終了しました".into()))
    }

    /// 再生中の読み上げを止める(待っている `speak` は `Stopped` で返る)。
    pub fn stop_speech(&self) {
        let _ = self.tx.send(Cmd::StopSpeech);
    }

    /// テスト用: 実際のオーディオデバイスを開かず、スレッドも立てない `SoundPlayer` を作る。
    /// `play` は呼び出せるが、受信側が存在しないため何も鳴らさずログのみ残る
    /// (テストからは通常 `play` を呼ばない)。CI 等のヘッドレス環境でテストのたびに
    /// 実デバイスをプローブするコスト/ノイズを避けるための構成。
    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        let (tx, _rx) = mpsc::channel::<Cmd>();
        Self { tx }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_decode_panic_survives_and_reports_panicked() {
        // decode_fully が実際に不正なファイルでパニックするかどうかに依存せず、
        // catch_unwind でパニックを捕まえて呼び出し元へ安全に返す経路そのものを検証する。
        let outcome = catch_decode_panic(|| panic!("boom (simulated decoder panic)"));
        assert!(matches!(outcome, DecodeOutcome::Panicked));
    }

    #[test]
    fn decode_fully_returns_invalid_for_garbage_bytes() {
        // ヘッダの時点で音声として認識できないバイト列。パニックはしない。
        match decode_fully(vec![0u8, 1, 2, 3, 4, 5]) {
            DecodeOutcome::Invalid(_) => {}
            DecodeOutcome::Ready(_) => panic!("expected Invalid, got Ready"),
            DecodeOutcome::Panicked => panic!("expected Invalid, got Panicked"),
        }
    }

    #[test]
    fn decode_fully_does_not_let_a_panic_escape_the_caller() {
        // sample_rate = 0 を宣言する WAV。`SamplesBuffer::new` はサンプルレート 0 で
        // パニックする仕様があるため、実際のデコーダ実装次第でこの経路がパニックし得る。
        // decode_fully がそれを捕まえて戻ってくること自体がこのテストの主張であり、
        // Invalid/Panicked のどちらとして分類されるかはデコーダの実装詳細なので固定しない
        // (決定的にパニック経路だけを検証したい場合は
        // `catch_decode_panic_survives_and_reports_panicked` を参照)。
        let fmt_size: u32 = 16;
        let data: [u8; 8] = [0; 8];
        let data_size: u32 = data.len() as u32;
        let riff_size: u32 = 4 + (8 + fmt_size) + (8 + data_size);

        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&riff_size.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&fmt_size.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&2u16.to_le_bytes()); // channels = 2
        bytes.extend_from_slice(&0u32.to_le_bytes()); // sample_rate = 0 (不正)
        bytes.extend_from_slice(&0u32.to_le_bytes()); // byte_rate
        bytes.extend_from_slice(&4u16.to_le_bytes()); // block_align
        bytes.extend_from_slice(&16u16.to_le_bytes()); // bits_per_sample
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.extend_from_slice(&data);

        let result = std::panic::catch_unwind(|| decode_fully(bytes));
        assert!(result.is_ok(), "decode_fully must not let a panic escape");
        assert!(!matches!(result.unwrap(), DecodeOutcome::Ready(_)));
    }

    // ---- has_haptic_channel (Issue #592) ----

    const HAPTIC_OGG: &[u8] = include_bytes!("../tests/fixtures/haptic_ogg/haptic_2ch.ogg");
    const PLAIN_OGG: &[u8] = include_bytes!("../tests/fixtures/haptic_ogg/plain_2ch.ogg");

    /// テスト用の Ogg ページを 1 つ作る。CRC は判定で見ないので 0 のまま。
    fn ogg_page(serial: u32, header_type: u8, lacing: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut v = b"OggS".to_vec();
        v.push(0); // バージョン
        v.push(header_type);
        v.extend_from_slice(&[0u8; 8]); // granule position
        v.extend_from_slice(&serial.to_le_bytes());
        v.extend_from_slice(&[0u8; 8]); // ページ番号・CRC
        v.push(lacing.len() as u8);
        v.extend_from_slice(lacing);
        v.extend_from_slice(payload);
        v
    }

    /// 1 つのパケットを Ogg のページ列にする(1 ページは最大 255 セグメント)。
    /// 長さが 255 の倍数のパケットは、長さ 0 のセグメントで終わる。
    fn ogg_packet_pages(serial: u32, first_header_type: u8, packet: &[u8]) -> Vec<Vec<u8>> {
        let mut lacing = vec![255u8; packet.len() / 255];
        lacing.push((packet.len() % 255) as u8);
        let mut pages = Vec::new();
        let mut offset = 0;
        for (i, segments) in lacing.chunks(255).enumerate() {
            let len: usize = segments.iter().map(|&n| n as usize).sum();
            let header_type = if i == 0 { first_header_type } else { 0x01 };
            pages.push(ogg_page(
                serial,
                header_type,
                segments,
                &packet[offset..offset + len],
            ));
            offset += len;
        }
        pages
    }

    /// Vorbis の識別ヘッダ(中身は判定で見ないので 0)。
    fn vorbis_id_packet() -> Vec<u8> {
        let mut v = b"\x01vorbis".to_vec();
        v.extend_from_slice(&[0u8; 23]);
        v
    }

    fn vorbis_comment_packet(comments: &[&str]) -> Vec<u8> {
        let mut v = b"\x03vorbis".to_vec();
        let vendor = b"test";
        v.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        v.extend_from_slice(vendor);
        v.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            v.extend_from_slice(&(c.len() as u32).to_le_bytes());
            v.extend_from_slice(c.as_bytes());
        }
        v.push(1); // framing bit
        v
    }

    /// 識別ヘッダと `comment_packet` を持つ Vorbis の ogg を作る
    /// (`has_haptic_channel` はヘッダしか見ないので、音声データは要らない)。
    fn ogg_with_comment_packet(comment_packet: &[u8]) -> Vec<u8> {
        let mut pages = ogg_packet_pages(1, 0x02, &vorbis_id_packet());
        pages.extend(ogg_packet_pages(1, 0x00, comment_packet));
        pages.concat()
    }

    fn ogg_with_comments(comments: &[&str]) -> Vec<u8> {
        ogg_with_comment_packet(&vorbis_comment_packet(comments))
    }

    /// Vorbis の識別ヘッダ(`\x01vorbis` + バージョン(4 バイト) + チャンネル数(1 バイト))から、チャンネル総数を読む。
    fn vorbis_channel_count(bytes: &[u8]) -> Option<u8> {
        let pos = bytes.windows(7).position(|w| w == b"\x01vorbis")?;
        bytes.get(pos + 7 + 4).copied()
    }

    #[test]
    fn haptic_fixture_is_detected_and_plain_one_is_not() {
        assert!(has_haptic_channel(HAPTIC_OGG));
        assert!(!has_haptic_channel(PLAIN_OGG));
    }

    #[test]
    fn haptic_fixture_has_the_layout_of_android_own_haptic_oggs() {
        // 端末に入っている Android 標準の触覚入り ogg(アラーム音 26 個)は、すべて 2ch で
        // `ANDROID_HAPTIC=1`、つまり「音 1ch + 触覚 1ch(最後のチャンネル)」。ffmpeg の libvorbis は、
        // 3ch を Vorbis の順(L・C・R)に並べ替えて格納するので、「ステレオ + 触覚」の 3ch は
        // 触覚が真ん中に入ってしまう。fixture もガイドの作り方も、公式と同じ 2ch にそろえる。
        assert_eq!(vorbis_channel_count(HAPTIC_OGG), Some(2));
        assert_eq!(vorbis_channel_count(PLAIN_OGG), Some(2));
    }

    #[test]
    fn haptic_tag_values() {
        assert!(has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC=1"
        ])));
        assert!(has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC=2"
        ])));
        // 他のコメントと並んでいても見つかる
        assert!(has_haptic_channel(&ogg_with_comments(&[
            "TITLE=x",
            "ANDROID_HAPTIC=1",
            "ARTIST=y"
        ])));
        // 0・数値でない・負・u32 を超える・空は触覚なし
        assert!(!has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC=0"
        ])));
        assert!(!has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC=abc"
        ])));
        assert!(!has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC=-1"
        ])));
        assert!(!has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC=99999999999"
        ])));
        assert!(!has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC="
        ])));
        // '=' が無い・キーが似ているだけのものは別のコメント
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&[
            "XANDROID_HAPTIC=1"
        ])));
        assert!(!has_haptic_channel(&ogg_with_comments(&[
            "ANDROID_HAPTIC_X=1"
        ])));
    }

    #[test]
    fn haptic_tag_key_is_case_insensitive() {
        assert!(has_haptic_channel(&ogg_with_comments(&[
            "android_haptic=1"
        ])));
        assert!(has_haptic_channel(&ogg_with_comments(&[
            "Android_Haptic=1"
        ])));
    }

    #[test]
    fn non_ogg_and_non_vorbis_are_never_haptic() {
        assert!(!has_haptic_channel(&[]));
        assert!(!has_haptic_channel(b"Ogg"));
        assert!(!has_haptic_channel(b"RIFF\x24\x00\x00\x00WAVEfmt "));
        // 先頭が OggS でなければ、タグがあっても false
        let mut not_ogg = ogg_with_comments(&["ANDROID_HAPTIC=1"]);
        not_ogg[0] = b'X';
        assert!(!has_haptic_channel(&not_ogg));
        // Opus(OpusTags)は対象外。\x03vorbis ヘッダが無いので false
        let mut opus = ogg_with_comments(&["ANDROID_HAPTIC=1"]);
        let p = opus.windows(7).position(|w| w == b"\x03vorbis").unwrap();
        opus[p..p + 7].copy_from_slice(b"OpusTag");
        assert!(!has_haptic_channel(&opus));
    }

    #[test]
    fn preset_sounds_are_never_haptic() {
        // 通知音のプリセット(WAV)と、MML が生成する WAV は従来経路(rodio)のまま
        for bytes in [
            include_bytes!("../assets/sounds/beep.wav").as_slice(),
            include_bytes!("../assets/sounds/chime.wav").as_slice(),
            include_bytes!("../assets/sounds/ping.wav").as_slice(),
            include_bytes!("../assets/sounds/pop.wav").as_slice(),
        ] {
            assert!(!has_haptic_channel(bytes));
        }
    }

    #[test]
    fn comment_header_spanning_pages_is_detected() {
        // タグより前に大きなコメント(カバー画像など)があると、コメントヘッダは複数のページに分かれる
        // (1 ページは約 64KiB まで)。ページをつなげて読むので、大きさに関係なく見つかる
        for size in [60_000, 70_000, 200_000] {
            let big = format!("COVER={}", "x".repeat(size));
            assert!(
                has_haptic_channel(&ogg_with_comments(&[&big, "ANDROID_HAPTIC=1"])),
                "size {size}"
            );
            assert!(
                !has_haptic_channel(&ogg_with_comments(&[&big, "ANDROID_HAPTIC=0"])),
                "size {size}"
            );
        }
    }

    #[test]
    fn comment_packet_ending_on_a_page_boundary_is_detected() {
        // 1 ページ分(255 セグメント)ちょうどのパケットは、長さ 0 のセグメントだけが次のページに行く
        let overhead = vorbis_comment_packet(&["COVER=", "ANDROID_HAPTIC=1"]).len();
        let big = format!("COVER={}", "x".repeat(255 * 255 - overhead));
        let packet = vorbis_comment_packet(&[&big, "ANDROID_HAPTIC=1"]);
        assert_eq!(packet.len(), 255 * 255);
        assert!(has_haptic_channel(&ogg_with_comment_packet(&packet)));
    }

    #[test]
    fn pages_of_other_streams_are_skipped() {
        // 多重化された ogg: 先頭が別のストリームで、コメントヘッダのページの間にも別のページが挟まる
        let big = format!("COVER={}", "x".repeat(70_000));
        let other = ogg_page(2, 0x00, &[16], b"ANDROID_HAPTIC=0");
        let mut pages = vec![ogg_page(2, 0x02, &[8], b"fishead\0")];
        pages.extend(ogg_packet_pages(1, 0x02, &vorbis_id_packet()));
        for page in ogg_packet_pages(1, 0x00, &vorbis_comment_packet(&[&big, "ANDROID_HAPTIC=1"])) {
            pages.push(other.clone());
            pages.push(page);
        }
        assert!(has_haptic_channel(&pages.concat()));
    }

    #[test]
    fn corrupt_comment_lengths_do_not_panic() {
        // ベンダー文字列の長さが巨大
        let mut packet = b"\x03vorbis".to_vec();
        packet.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(!has_haptic_channel(&ogg_with_comment_packet(&packet)));
        // コメント数が巨大で、中身が足りない
        let mut packet = b"\x03vorbis".to_vec();
        packet.extend_from_slice(&0u32.to_le_bytes());
        packet.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(!has_haptic_channel(&ogg_with_comment_packet(&packet)));
        // コメントの長さが残りより長い
        let mut packet = b"\x03vorbis".to_vec();
        packet.extend_from_slice(&0u32.to_le_bytes());
        packet.extend_from_slice(&1u32.to_le_bytes());
        packet.extend_from_slice(&1000u32.to_le_bytes());
        packet.extend_from_slice(b"ANDROID_HAPTIC=1");
        assert!(!has_haptic_channel(&ogg_with_comment_packet(&packet)));
    }

    #[test]
    fn corrupt_page_structure_does_not_panic() {
        let id = ogg_packet_pages(1, 0x02, &vorbis_id_packet()).concat();
        let comment = ogg_packet_pages(1, 0x00, &vorbis_comment_packet(&["ANDROID_HAPTIC=1"]));
        // 2 ページ目の先頭が OggS でなければ、そこで打ち切る
        let mut broken = comment.concat();
        broken[0] = b'X';
        assert!(!has_haptic_channel(&[id.clone(), broken].concat()));
        // セグメント数だけがあって、セグメントテーブルが足りない
        let mut short_table = ogg_page(1, 0x00, &[], &[]);
        *short_table.last_mut().unwrap() = 255;
        assert!(!has_haptic_channel(&[id.clone(), short_table].concat()));
        // セグメントが 0 個のページが続いても、読み進めて終わる
        let empty = ogg_page(1, 0x00, &[], &[]).repeat(1000);
        assert!(!has_haptic_channel(&[id.clone(), empty].concat()));
        // 識別ヘッダのページしかない
        assert!(!has_haptic_channel(&id));
    }

    #[test]
    fn every_truncation_of_a_real_file_is_safe() {
        // どの長さで切れていてもパニックしない。コメントヘッダが読める長さ以降は true
        for n in 0..=HAPTIC_OGG.len() {
            let _ = has_haptic_channel(&HAPTIC_OGG[..n]);
        }
        assert!(!has_haptic_channel(&HAPTIC_OGG[..50]));
        // 先頭 700 バイト(コメントヘッダは読めて、音声データは壊れている)。受け入れ基準 5 の壊れた ogg に使う
        assert!(has_haptic_channel(&HAPTIC_OGG[..700]));
    }

    #[test]
    fn every_truncation_of_a_multi_page_header_is_safe() {
        let big = format!("COVER={}", "x".repeat(70_000));
        let ogg = ogg_with_comments(&[&big, "ANDROID_HAPTIC=1"]);
        // 全バイト位置は重いので間引き、末尾(タグのある所)は 1 バイトずつ見る
        for n in (0..ogg.len())
            .step_by(97)
            .chain(ogg.len() - 600..=ogg.len())
        {
            let _ = has_haptic_channel(&ogg[..n]);
        }
        // タグの手前で切れていれば false、末尾の framing bit だけが欠けても true
        assert!(!has_haptic_channel(&ogg[..ogg.len() - 2]));
        assert!(has_haptic_channel(&ogg[..ogg.len() - 1]));
    }

    #[tokio::test]
    async fn speak_on_a_player_without_a_thread_fails_instead_of_hanging() {
        let p = SoundPlayer::new_for_test();
        let r =
            tokio::time::timeout(std::time::Duration::from_secs(2), p.speak(vec![0, 1, 2])).await;
        assert!(
            matches!(r, Ok(crate::speech_lane::SpeakOutcome::Failed(_))),
            "{r:?}"
        );
        p.stop_speech(); // 受信側がいなくても panic しない
    }
}
