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

use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Source};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

pub struct SoundPlayer {
    tx: mpsc::Sender<Vec<u8>>,
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

/// コメントヘッダを探す範囲。触覚入り ogg のコメントヘッダは先頭近くにあるので、
/// 通知のたびに音声全体を舐めないよう先頭だけを見る。
const HAPTIC_SCAN_LIMIT: usize = 64 * 1024;

/// ogg(Vorbis)が Android の触覚チャンネル入りかを判定する(Issue #592)。
///
/// 触覚入りの ogg は、Vorbis コメントに `ANDROID_HAPTIC=<触覚チャンネル数>` を持つ
/// (AOSP「UX foundation for haptic framework」)。次をすべて満たすときだけ true:
/// 先頭が `OggS` / 先頭 64KiB に Vorbis のコメントヘッダ(`\x03vorbis`)がある /
/// `ANDROID_HAPTIC`(大文字小文字を区別しない)の値が 1 以上の整数。
///
/// 既知の制約: コメントヘッダが 64KiB の外にある、または Ogg のページ境界をまたぐ(カバー画像入りなど)
/// ときは判定できず false(触覚なしの音として rodio で鳴る)。Opus は対象外。
/// 壊れたデータでもパニックせず false を返す。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn has_haptic_channel(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"OggS") {
        return false;
    }
    let window = &bytes[..bytes.len().min(HAPTIC_SCAN_LIMIT)];
    let Some(pos) = window.windows(7).position(|w| w == b"\x03vorbis") else {
        return false;
    };
    let comments = &window[pos + 7..];
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
    let channels = std::str::from_utf8(value).ok().and_then(|s| s.parse::<u32>().ok());
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
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let healthy = Arc::new(AtomicBool::new(true));
            let mut handle = match open_sink(&healthy) {
                Some(h) => h,
                None => return,
            };

            for bytes in rx {
                if !healthy.load(Ordering::SeqCst) {
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

                match decode_fully(bytes) {
                    DecodeOutcome::Ready(buf) => handle.mixer().add(buf),
                    DecodeOutcome::Invalid(e) => log::warn!("通知音: デコードに失敗: {e}"),
                    DecodeOutcome::Panicked => {
                        log::warn!("通知音: デコード処理がパニックしました(不正なファイルの可能性)。この音声はスキップします");
                    }
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

    /// テスト用: 実際のオーディオデバイスを開かず、スレッドも立てない `SoundPlayer` を作る。
    /// `play` は呼び出せるが、受信側が存在しないため何も鳴らさずログのみ残る
    /// (テストからは通常 `play` を呼ばない)。CI 等のヘッドレス環境でテストのたびに
    /// 実デバイスをプローブするコスト/ノイズを避けるための構成。
    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        let (tx, _rx) = mpsc::channel::<Vec<u8>>();
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

    /// 判定の単体テスト用に、`padding` バイトの後ろに Vorbis コメントヘッダを持つ ogg 風のバイト列を作る
    /// (`has_haptic_channel` はヘッダしか見ないので、音声データは要らない)。
    fn ogg_with_comments_after(padding: usize, comments: &[&str]) -> Vec<u8> {
        let mut v = b"OggS".to_vec();
        v.extend(std::iter::repeat(0u8).take(padding));
        v.extend_from_slice(b"\x03vorbis");
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

    fn ogg_with_comments(comments: &[&str]) -> Vec<u8> {
        ogg_with_comments_after(23, comments)
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
        assert!(has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=1"])));
        assert!(has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=2"])));
        // 他のコメントと並んでいても見つかる
        assert!(has_haptic_channel(&ogg_with_comments(&["TITLE=x", "ANDROID_HAPTIC=1", "ARTIST=y"])));
        // 0・数値でない・負・u32 を超える・空は触覚なし
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=0"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=abc"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=-1"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC=99999999999"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC="])));
        // '=' が無い・キーが似ているだけのものは別のコメント
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["XANDROID_HAPTIC=1"])));
        assert!(!has_haptic_channel(&ogg_with_comments(&["ANDROID_HAPTIC_X=1"])));
    }

    #[test]
    fn haptic_tag_key_is_case_insensitive() {
        assert!(has_haptic_channel(&ogg_with_comments(&["android_haptic=1"])));
        assert!(has_haptic_channel(&ogg_with_comments(&["Android_Haptic=1"])));
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
    fn comment_header_beyond_the_scan_window_is_not_detected() {
        // 先頭 64KiB の外にあるコメントヘッダは見ない(既知の制約)
        assert!(has_haptic_channel(&ogg_with_comments_after(60_000, &["ANDROID_HAPTIC=1"])));
        assert!(!has_haptic_channel(&ogg_with_comments_after(70_000, &["ANDROID_HAPTIC=1"])));
    }

    #[test]
    fn corrupt_comment_lengths_do_not_panic() {
        // ベンダー文字列の長さが巨大
        let mut v = b"OggS".to_vec();
        v.extend_from_slice(&[0u8; 23]);
        v.extend_from_slice(b"\x03vorbis");
        v.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(!has_haptic_channel(&v));
        // コメント数が巨大で、中身が足りない
        let mut v = b"OggS".to_vec();
        v.extend_from_slice(&[0u8; 23]);
        v.extend_from_slice(b"\x03vorbis");
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(!has_haptic_channel(&v));
        // コメントの長さが残りより長い
        let mut v = b"OggS".to_vec();
        v.extend_from_slice(&[0u8; 23]);
        v.extend_from_slice(b"\x03vorbis");
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&1000u32.to_le_bytes());
        v.extend_from_slice(b"ANDROID_HAPTIC=1");
        assert!(!has_haptic_channel(&v));
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
}
