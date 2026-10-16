//! MML(Music Macro Language)パーサー(Issue #75)。
//!
//! 外部に依存しない純粋関数。繰り返しの展開・タイの連結・テンポの解決までをここで済ませ、
//! 各音を「開始秒・長さ秒・MIDI ノート番号・ベロシティ」で返す。音の生成はフロント(Tone.js)の責務。
//! 方言と上限は docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md を参照。

use crate::domain::{MmlError, MmlScore, NoteEvent};
use crate::error::Error;

pub const MAX_TEXT_CHARS: usize = 1000;
pub const MAX_TRACKS: usize = 4;
pub const MAX_TOTAL_SEC: f64 = 10.0;

/// `sound_choice` に MML を入れるときの接頭辞。
pub const SOUND_CHOICE_PREFIX: &str = "mml:";

/// 浮動小数点の積算誤差で、ちょうど 10 秒の MML を弾かないための許容幅。
const TIME_EPS: f64 = 1e-6;
const VALID_LENGTHS: [u32; 7] = [1, 2, 4, 8, 16, 32, 64];
const MAX_DOTS: u8 = 2;
const DEFAULT_OCTAVE: i32 = 4;
const DEFAULT_LEN: u32 = 4;
const DEFAULT_TEMPO: u32 = 120;
const DEFAULT_VOLUME: u32 = 10;

fn mml_err(pos: usize, message: impl Into<String>) -> MmlError {
    MmlError {
        position: pos as u32,
        message: message.into(),
    }
}

/// トラック 1 本分の命令。繰り返しはパース時に展開済みなので、ここには現れない。
#[derive(Debug, Clone)]
enum Op {
    Note {
        semitone: i32,
        len: Option<u32>,
        dots: u8,
    },
    Rest {
        len: Option<u32>,
        dots: u8,
    },
    Tie,
    Octave(u8),
    OctaveUp,
    OctaveDown,
    DefaultLen(u32),
    Tempo(u32),
    Volume(u8),
}

#[derive(Debug, Clone)]
struct Item {
    op: Op,
    /// 元のテキストでの文字位置(エラー報告用)
    pos: usize,
}

/// 空白を除いた (小文字化した文字, 元の文字位置) の列を読み進めるカーソル。
struct Cursor<'a> {
    syms: &'a [(char, usize)],
    i: usize,
    /// 入力の末尾位置(終端でのエラー報告用)
    end: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.syms.get(self.i).map(|s| s.0)
    }

    fn pos(&self) -> usize {
        self.syms.get(self.i).map_or(self.end, |s| s.1)
    }

    fn bump(&mut self) {
        self.i += 1;
    }

    /// 連続する数字を読む。数字が無ければ None。値は (数値, 先頭の位置)。
    fn number(&mut self) -> Option<(u32, usize)> {
        let start = self.pos();
        let mut n: Option<u32> = None;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            n = Some(n.unwrap_or(0).saturating_mul(10).saturating_add(d));
            self.bump();
        }
        n.map(|n| (n, start))
    }

    fn require_number(&mut self, cmd_pos: usize, cmd: char) -> Result<(u32, usize), MmlError> {
        self.number()
            .ok_or_else(|| mml_err(cmd_pos, format!("`{cmd}` の後に数値が必要です")))
    }

    /// 音符・休符の後ろの「長さ(省略可)」と「付点」を読む。
    fn length_and_dots(&mut self) -> Result<(Option<u32>, u8), MmlError> {
        let len = match self.number() {
            Some((n, p)) => {
                if !VALID_LENGTHS.contains(&n) {
                    return Err(mml_err(p, "長さは 1, 2, 4, 8, 16, 32, 64 のいずれかです"));
                }
                Some(n)
            }
            None => None,
        };
        let mut dots = 0u8;
        while self.peek() == Some('.') {
            let p = self.pos();
            self.bump();
            dots += 1;
            if dots > MAX_DOTS {
                return Err(mml_err(p, "付点は 2 個までです"));
            }
        }
        Ok((len, dots))
    }
}

/// 1 トラック分の文字列を命令列にする。繰り返し `[ ... ]n` はここで展開する。
/// 繰り返しは入れ子にできないので、展開後の命令数は入力サイズ(1000 文字 × 最大 99 回)で抑えられる。
/// 時間の上限はここでは見ず、`run_track` が実行しながら判定する。
fn parse_track(syms: &[(char, usize)], end: usize) -> Result<Vec<Item>, MmlError> {
    let mut c = Cursor { syms, i: 0, end };
    let mut out: Vec<Item> = Vec::new();
    // 開いている繰り返しの (out の開始 index, `[` の位置)
    let mut open_loop: Option<(usize, usize)> = None;

    while let Some(ch) = c.peek() {
        let pos = c.pos();
        c.bump();
        let op = match ch {
            'c' | 'd' | 'e' | 'f' | 'g' | 'a' | 'b' => {
                let base = match ch {
                    'c' => 0,
                    'd' => 2,
                    'e' => 4,
                    'f' => 5,
                    'g' => 7,
                    'a' => 9,
                    _ => 11,
                };
                let accidental = match c.peek() {
                    Some('+') | Some('#') => {
                        c.bump();
                        1
                    }
                    Some('-') => {
                        c.bump();
                        -1
                    }
                    _ => 0,
                };
                let (len, dots) = c.length_and_dots()?;
                Op::Note {
                    semitone: base + accidental,
                    len,
                    dots,
                }
            }
            'r' => {
                let (len, dots) = c.length_and_dots()?;
                Op::Rest { len, dots }
            }
            '&' => Op::Tie,
            '>' => Op::OctaveUp,
            '<' => Op::OctaveDown,
            'o' => {
                let (n, p) = c.require_number(pos, 'o')?;
                if !(1..=8).contains(&n) {
                    return Err(mml_err(p, "オクターブは 1〜8 です"));
                }
                Op::Octave(n as u8)
            }
            'l' => {
                let (n, p) = c.require_number(pos, 'l')?;
                if !VALID_LENGTHS.contains(&n) {
                    return Err(mml_err(p, "長さは 1, 2, 4, 8, 16, 32, 64 のいずれかです"));
                }
                Op::DefaultLen(n)
            }
            't' => {
                let (n, p) = c.require_number(pos, 't')?;
                if !(32..=255).contains(&n) {
                    return Err(mml_err(p, "テンポは 32〜255 です"));
                }
                Op::Tempo(n)
            }
            'v' => {
                let (n, p) = c.require_number(pos, 'v')?;
                if n > 15 {
                    return Err(mml_err(p, "音量は 0〜15 です"));
                }
                Op::Volume(n as u8)
            }
            '[' => {
                if open_loop.is_some() {
                    return Err(mml_err(pos, "繰り返しはネストできません"));
                }
                open_loop = Some((out.len(), pos));
                continue;
            }
            ']' => {
                let Some((start, _)) = open_loop.take() else {
                    return Err(mml_err(pos, "対応する `[` がありません"));
                };
                let count = match c.number() {
                    Some((n, p)) => {
                        if !(2..=99).contains(&n) {
                            return Err(mml_err(p, "繰り返し回数は 2〜99 です"));
                        }
                        n
                    }
                    None => 2,
                };
                let body: Vec<Item> = out[start..].to_vec();
                for _ in 1..count {
                    out.extend(body.iter().cloned());
                }
                continue;
            }
            other => return Err(mml_err(pos, format!("不明な文字 `{other}` です"))),
        };
        out.push(Item { op, pos });
    }

    if let Some((_, p)) = open_loop {
        return Err(mml_err(p, "閉じていない `[` があります"));
    }
    Ok(out)
}

struct TrackState {
    octave: i32,
    len: u32,
    tempo: u32,
    volume: u32,
    t: f64,
    /// 直前の命令が音符だったとき、その音符の `notes` 内の index(タイの連結先)
    last_note: Option<usize>,
    /// 直前に `&` があったとき、その位置
    tie_pos: Option<usize>,
}

fn duration_sec(len: u32, dots: u8, tempo: u32) -> f64 {
    let dot_factor = match dots {
        0 => 1.0,
        1 => 1.5,
        _ => 1.75,
    };
    4.0 / f64::from(len) * dot_factor * 60.0 / f64::from(tempo)
}

fn check_limit(t: f64, pos: usize) -> Result<(), MmlError> {
    if t > MAX_TOTAL_SEC + TIME_EPS {
        return Err(mml_err(pos, "再生時間は 10 秒以下にしてください"));
    }
    Ok(())
}

/// 命令列を実行して音を `notes` に積み、トラックの終了時刻(秒)を返す。
/// 時間の積算は音符・休符ごとに行い、上限を超えた最初の音符・休符でエラーにして打ち切る。
/// (繰り返しは `parse_track` で先に展開済み。その大きさは入力サイズで抑えられている。)
fn run_track(track: u8, items: &[Item], notes: &mut Vec<NoteEvent>) -> Result<f64, MmlError> {
    let mut st = TrackState {
        octave: DEFAULT_OCTAVE,
        len: DEFAULT_LEN,
        tempo: DEFAULT_TEMPO,
        volume: DEFAULT_VOLUME,
        t: 0.0,
        last_note: None,
        tie_pos: None,
    };

    for Item { op, pos } in items {
        let pos = *pos;
        match op {
            Op::Octave(n) => st.octave = i32::from(*n),
            Op::OctaveUp => {
                if st.octave >= 8 {
                    return Err(mml_err(pos, "オクターブは 1〜8 です"));
                }
                st.octave += 1;
            }
            Op::OctaveDown => {
                if st.octave <= 1 {
                    return Err(mml_err(pos, "オクターブは 1〜8 です"));
                }
                st.octave -= 1;
            }
            Op::DefaultLen(n) => st.len = *n,
            Op::Tempo(n) => st.tempo = *n,
            Op::Volume(n) => st.volume = u32::from(*n),
            Op::Tie => {
                if st.last_note.is_none() || st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の前に音符が必要です"));
                }
                st.tie_pos = Some(pos);
            }
            Op::Rest { len, dots } => {
                if let Some(tp) = st.tie_pos {
                    return Err(mml_err(tp, "`&` の後に音符が必要です"));
                }
                st.t += duration_sec(len.unwrap_or(st.len), *dots, st.tempo);
                st.last_note = None;
                check_limit(st.t, pos)?;
            }
            Op::Note {
                semitone,
                len,
                dots,
            } => {
                let midi = 12 * (st.octave + 1) + semitone;
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo);
                if st.tie_pos.take().is_some() {
                    let idx = st.last_note.expect("a tie is only armed after a note");
                    if i32::from(notes[idx].midi) != midi {
                        return Err(mml_err(pos, "`&` は同じ高さの音符どうしだけをつなげます"));
                    }
                    notes[idx].duration_sec += d;
                } else {
                    notes.push(NoteEvent {
                        track,
                        start_sec: st.t,
                        duration_sec: d,
                        midi: midi as u8,
                        velocity: f64::from(st.volume) / 15.0,
                    });
                    st.last_note = Some(notes.len() - 1);
                }
                st.t += d;
                check_limit(st.t, pos)?;
            }
        }
    }

    if let Some(tp) = st.tie_pos {
        return Err(mml_err(tp, "`&` の後に音符が必要です"));
    }
    Ok(st.t)
}

/// MML テキストを解釈する。
pub fn parse(text: &str) -> Result<MmlScore, MmlError> {
    let total_chars = text.chars().count();
    if total_chars > MAX_TEXT_CHARS {
        return Err(mml_err(
            MAX_TEXT_CHARS,
            "MML は 1000 文字以下にしてください",
        ));
    }

    let syms: Vec<(char, usize)> = text
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(i, c)| (c.to_ascii_lowercase(), i))
        .collect();

    // ';' でトラックに分ける。各区間に「直前の ';' の位置」を添える(トラック数超過の報告用)。
    let mut segments: Vec<(&[(char, usize)], usize)> = Vec::new();
    let mut start = 0;
    let mut sep_pos = 0;
    for (idx, (c, p)) in syms.iter().enumerate() {
        if *c == ';' {
            segments.push((&syms[start..idx], sep_pos));
            start = idx + 1;
            sep_pos = *p;
        }
    }
    segments.push((&syms[start..], sep_pos));

    let mut notes: Vec<NoteEvent> = Vec::new();
    let mut total_sec = 0.0_f64;
    let mut track_count = 0u8;
    for (seg, sep) in segments.into_iter().filter(|(s, _)| !s.is_empty()) {
        if usize::from(track_count) >= MAX_TRACKS {
            return Err(mml_err(sep, "トラックは 4 つまでです"));
        }
        let items = parse_track(seg, total_chars)?;
        let end = run_track(track_count, &items, &mut notes)?;
        total_sec = total_sec.max(end);
        track_count += 1;
    }

    if notes.is_empty() {
        return Err(mml_err(0, "音符がありません"));
    }
    Ok(MmlScore {
        notes,
        total_sec,
        track_count,
    })
}

/// 保存時の検証。`mml:` で始まる `sound_choice` だけをパースし、不正なら拒否する。
/// それ以外の形式(空・プリセット ID・`data:`)は従来どおり検証しない。
pub fn validate_sound_choice(choice: &str) -> crate::error::Result<()> {
    if let Some(text) = choice.strip_prefix(SOUND_CHOICE_PREFIX) {
        parse(text).map_err(|e| {
            Error::Invalid(format!(
                "MML が不正です ({} 文字目): {}",
                e.position + 1,
                e.message
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MmlError, MmlScore};

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    fn must_parse(text: &str) -> MmlScore {
        parse(text).unwrap_or_else(|e| panic!("{text:?} should parse: {e:?}"))
    }

    fn must_fail(text: &str) -> MmlError {
        parse(text).expect_err(&format!("{text:?} should fail"))
    }

    #[test]
    fn single_note_uses_defaults() {
        let s = must_parse("c");
        assert_eq!(s.notes.len(), 1);
        let n = &s.notes[0];
        assert_eq!((n.track, n.midi), (0, 60));
        approx(n.start_sec, 0.0);
        approx(n.duration_sec, 0.5); // t120 l4
        approx(n.velocity, 10.0 / 15.0);
        approx(s.total_sec, 0.5);
        assert_eq!(s.track_count, 1);
    }

    #[test]
    fn note_names_and_accidentals() {
        let midis = |t: &str| {
            must_parse(t)
                .notes
                .iter()
                .map(|n| n.midi)
                .collect::<Vec<_>>()
        };
        assert_eq!(midis("cdefgab"), vec![60, 62, 64, 65, 67, 69, 71]);
        assert_eq!(midis("c+ c# d-"), vec![61, 61, 61]);
        assert_eq!(midis("b+"), vec![72]);
        assert_eq!(midis("c-"), vec![59]);
    }

    #[test]
    fn lengths_and_dots() {
        let dur = |t: &str| must_parse(t).notes[0].duration_sec;
        approx(dur("c1"), 2.0);
        approx(dur("c2"), 1.0);
        approx(dur("c8"), 0.25);
        approx(dur("c16"), 0.125);
        approx(dur("c4."), 0.75);
        approx(dur("c4.."), 0.875);
        approx(dur("l8 c"), 0.25);
        approx(dur("l8 c4"), 0.5);
        approx(dur("l8 c."), 0.375);
    }

    #[test]
    fn rest_advances_time() {
        let s = must_parse("c4 r4 c4");
        assert_eq!(s.notes.len(), 2);
        approx(s.notes[1].start_sec, 1.0);
        approx(s.total_sec, 1.5);
    }

    #[test]
    fn ignores_case_and_whitespace_including_crlf() {
        let a = must_parse("C4 D4");
        let b = must_parse("c4d4");
        let c = must_parse("c4\r\n\td4\n");
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn octave_commands() {
        let first = |t: &str| must_parse(t).notes[0].midi;
        assert_eq!(first("o5 c"), 72);
        assert_eq!(first("o1 c"), 24);
        assert_eq!(first("o8 c"), 108);
        assert_eq!(first(">c"), 72);
        assert_eq!(first("<c"), 48);
        assert_eq!(first("o4 > > c"), 84);
    }

    #[test]
    fn octave_out_of_range_is_an_error_at_the_offending_char() {
        assert_eq!(must_fail("o9 c").position, 1);
        assert_eq!(must_fail("o0 c").position, 1);
        assert_eq!(must_fail(">>>>>c").position, 4); // o4 -> o9 になる 5 個目の '>'
        assert_eq!(must_fail("<<<<c").position, 3); // o4 -> o0 になる 4 個目の '<'
    }

    #[test]
    fn tempo_changes_apply_from_that_point() {
        approx(must_parse("t60 c4").notes[0].duration_sec, 1.0);
        approx(must_parse("t240 c4").notes[0].duration_sec, 0.25);
        let s = must_parse("t120 c4 t60 c4");
        approx(s.notes[1].start_sec, 0.5);
        approx(s.notes[1].duration_sec, 1.0);
        approx(s.total_sec, 1.5);
        must_fail("t31 c");
        must_fail("t256 c");
    }

    #[test]
    fn volume_maps_to_velocity() {
        approx(must_parse("v15 c").notes[0].velocity, 1.0);
        approx(must_parse("v0 c").notes[0].velocity, 0.0);
        approx(must_parse("v5 c v10 c").notes[1].velocity, 10.0 / 15.0);
        must_fail("v16 c");
    }

    #[test]
    fn invalid_lengths_and_missing_numbers() {
        assert_eq!(must_fail("l3 c").position, 1);
        assert_eq!(must_fail("c3").position, 1);
        assert_eq!(must_fail("c128").position, 1);
        assert_eq!(must_fail("l c").position, 0);
        assert_eq!(must_fail("c4...").position, 4); // 3 個目の付点
    }

    #[test]
    fn ties_merge_same_pitch_notes() {
        let s = must_parse("c4&c4");
        assert_eq!(s.notes.len(), 1);
        approx(s.notes[0].duration_sec, 1.0);
        approx(must_parse("c4&c8").notes[0].duration_sec, 0.75);
        approx(must_parse("c4&c4&c4").notes[0].duration_sec, 1.5);
        // 間に音量変更が入っても連結でき、先頭の音のベロシティを保つ
        let s = must_parse("c4&v5c4");
        assert_eq!(s.notes.len(), 1);
        approx(s.notes[0].velocity, 10.0 / 15.0);
    }

    #[test]
    fn invalid_ties() {
        assert_eq!(must_fail("c4&d4").position, 3); // 高さが違う
        must_fail("c4&r4");
        assert_eq!(must_fail("r4&c4").position, 2); // 前に音符が無い
        assert_eq!(must_fail("c4&").position, 2); // 後ろに音符が無い
        assert_eq!(must_fail("c4&&c").position, 3);
    }

    #[test]
    fn loops_repeat_the_body() {
        let s = must_parse("[c4]3");
        assert_eq!(s.notes.len(), 3);
        approx(s.notes[2].start_sec, 1.0);
        assert_eq!(must_parse("[cd]2").notes.len(), 4);
        assert_eq!(must_parse("[c4]").notes.len(), 2); // 回数省略は 2
                                                       // 状態(オクターブ)は繰り返しをまたいで持ち越す
        let midis: Vec<u8> = must_parse("[c>]2").notes.iter().map(|n| n.midi).collect();
        assert_eq!(midis, vec![60, 72]);
    }

    #[test]
    fn invalid_loops() {
        assert_eq!(must_fail("[c4]1").position, 4);
        assert_eq!(must_fail("[c4]100").position, 4);
        assert_eq!(must_fail("[c4").position, 0);
        assert_eq!(must_fail("c4]").position, 2);
        assert_eq!(must_fail("[[c]2]2").position, 1);
        assert_eq!(must_fail("[c;d]2").position, 0); // ';' は繰り返しをまたげない
    }

    #[test]
    fn tracks_are_independent_and_start_at_zero() {
        let s = must_parse("c4;e4");
        assert_eq!(s.track_count, 2);
        assert_eq!((s.notes[0].track, s.notes[0].midi), (0, 60));
        assert_eq!((s.notes[1].track, s.notes[1].midi), (1, 64));
        approx(s.notes[1].start_sec, 0.0);
        // テンポはトラックごと(v1 の制約)
        approx(must_parse("t60 c4;c4").notes[1].duration_sec, 0.5);
        approx(must_parse("c4;c1").total_sec, 2.0);
        // 空のトラックは無視する
        assert_eq!(must_parse("c;;e").track_count, 2);
        assert_eq!(must_parse("c;").track_count, 1);
    }

    #[test]
    fn at_most_four_tracks() {
        assert_eq!(must_parse("c;d;e;f").track_count, 4);
        assert_eq!(must_fail("c;d;e;f;g").position, 7); // 5 本目の直前の ';'
    }

    #[test]
    fn text_length_limit_counts_whitespace() {
        assert!(parse(&format!("c{}", " ".repeat(999))).is_ok()); // ちょうど 1000 文字
        assert_eq!(must_fail(&format!("c{}", " ".repeat(1000))).position, 1000);
    }

    #[test]
    fn total_duration_limit_stops_at_the_offending_note() {
        assert!(parse("c1c1c1c1c1").is_ok()); // ちょうど 10 秒
        assert_eq!(must_fail("c1c1c1c1c1c").position, 10);
    }

    #[test]
    fn loop_expansion_stops_at_the_duration_limit() {
        // `c1` は 2 秒。`[c1]99` は先に展開され、6 個目の `c1` で 10 秒を超えたところで止まる。
        // 展開された命令は元の位置を引き継ぐので、エラー位置は繰り返し本体の `c1`(位置 1)を指す。
        assert_eq!(must_fail("[c1]99").position, 1);
    }

    #[test]
    fn unknown_characters_report_their_position() {
        let e = must_fail("c4 x");
        assert_eq!(e.position, 3);
        assert!(e.message.contains('x'));
        assert_eq!(must_fail("c4 あ").position, 3);
    }

    #[test]
    fn error_positions_count_whitespace() {
        assert_eq!(must_fail("c4  x").position, 4);
        assert_eq!(must_fail("c4\r\nx").position, 4);
    }

    #[test]
    fn input_without_any_note_is_rejected() {
        assert_eq!(must_fail("").position, 0);
        assert_eq!(must_fail("   ").position, 0);
        assert_eq!(must_fail("r4").position, 0);
        assert_eq!(must_fail("t120 o4 l8").position, 0);
    }

    #[test]
    fn validate_sound_choice_checks_only_mml() {
        assert!(validate_sound_choice("").is_ok());
        assert!(validate_sound_choice("beep").is_ok());
        assert!(validate_sound_choice("data:audio/wav;base64,AAAA").is_ok());
        assert!(validate_sound_choice("mml:c4d4").is_ok());
        assert!(matches!(
            validate_sound_choice("mml:"),
            Err(crate::error::Error::Invalid(_))
        ));
        let err = validate_sound_choice("mml:x").unwrap_err();
        assert!(err.to_string().contains("1 文字目"), "{err}");
    }
}
