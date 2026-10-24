//! MML(Music Macro Language)パーサー(Issue #75)。
//!
//! 外部に依存しない純粋関数。繰り返しの展開・タイの連結・テンポの解決までをここで済ませ、
//! 各音を「開始秒・長さ秒・MIDI ノート番号・ベロシティ」で返す。音の生成はフロント(Tone.js)の責務。
//! 方言と上限は docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md、
//! 音色は 2026-10-16-issue-494-mml-timbre-design.md、
//! 構文の拡張(タイ・連符・コメントなど)は 2026-10-20-issue-499-mml-syntax-extension-design.md を参照。

use crate::domain::{MmlError, MmlScore, NoteEvent, Timbre};
use crate::error::Error;
use std::collections::HashMap;

pub const MAX_TEXT_CHARS: usize = 5000;
pub const MAX_TRACKS: usize = 4;
pub const MAX_TOTAL_SEC: f64 = 90.0;
pub const MAX_NOTES: usize = 3000;
/// 繰り返しを展開した後の命令数の上限。音符・休符は 3000 音・90 秒で抑えられるが、音を出さない命令
/// (`o` `v` など)は抑えられないので、ネストで展開が膨らむ入力のために命令数そのものを制限する。
/// v1 で通る最大の展開(ループ 1 組で本体が約 4996 文字、99 回で約 49.5 万)を弾かない値。
pub const MAX_EXPANDED_OPS: usize = 500_000;

/// `sound_choice` に MML を入れるときの接頭辞。
pub const SOUND_CHOICE_PREFIX: &str = "mml:";

/// 浮動小数点の積算誤差で、ちょうど上限(90 秒)の MML を弾かないための許容幅。
const TIME_EPS: f64 = 1e-6;
const VALID_LENGTHS: [u32; 7] = [1, 2, 4, 8, 16, 32, 64];
const MAX_DOTS: u8 = 2;
const DEFAULT_OCTAVE: i32 = 4;
const DEFAULT_LEN: u32 = 4;
const DEFAULT_TEMPO: u32 = 120;
const DEFAULT_VOLUME: u32 = 10;
/// ゲートタイム `q` の範囲(百分率)。既定は上限の 100(全長を鳴らす)。
const MAX_GATE: u32 = 100;

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
    /// `^`。直前の音符に、この長さを足す(長さ省略は `l` の値)。
    TieLen {
        len: Option<u32>,
        dots: u8,
    },
    Octave(u8),
    OctaveUp,
    OctaveDown,
    DefaultLen(u32),
    Tempo(u32),
    /// `t=<n>`。全トラックの初期テンポ(`parse` が集めて `run_track` に渡す。`run_track` では何もしない)。
    SharedTempo(u32),
    Volume(u8),
    /// `v+n` / `v-n`。現在の音量に足す(範囲外はエラー)。
    VolumeDelta(i32),
    /// `q<n>`。音を鳴らす長さの割合(百分率、1〜100)。
    Gate(u8),
    /// `{`。対応する `}` までの音符・休符・`^` の長さを、`}` の後の長さに収める。
    TupletStart,
    /// `}<長さ>`。長さ省略は `{` の時点の `l` の値。
    TupletEnd {
        len: Option<u32>,
        dots: u8,
    },
    /// `'`(和音の開き)。
    ChordStart,
    /// 和音の中の音符。長さを持たず、`ChordEnd` でまとめて鳴らす。
    ChordNote {
        semitone: i32,
    },
    /// `'<長さ>`(和音の閉じ)。長さ省略は `l` の値。
    ChordEnd {
        len: Option<u32>,
        dots: u8,
    },
    Timbre(Timbre),
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

    /// 先頭が音色名と一致するなら、その名前の分だけ読み進めて返す。
    /// どの名前も他の名前の接頭辞ではない(テストで確認)ので、一致は 1 つに決まる。
    fn timbre_name(&mut self) -> Option<Timbre> {
        let rest = &self.syms[self.i..];
        let t = Timbre::ALL.into_iter().find(|t| {
            let name = t.name();
            rest.len() >= name.len() && name.chars().zip(rest).all(|(a, (b, _))| a == *b)
        })?;
        self.i += t.name().len();
        Some(t)
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

/// 開いている括弧。繰り返し `[` と連符 `{` を、1 つのスタックで管理する(交差を弾くため)。
enum Frame {
    Loop {
        /// `out` での本体の開始位置
        start: usize,
        /// `[` の元の文字位置
        pos: usize,
        /// `:` が置かれた `out` での位置(最後の回は、ここから後ろを飛ばす)
        colon: Option<usize>,
    },
    Tuplet {
        /// `out` での `TupletStart` の位置
        start: usize,
        /// `{` の元の文字位置
        pos: usize,
    },
}

/// `out[start..start + len]` の複製を `out` の末尾に足す。展開後の命令数が上限を超えるなら、足さずに
/// `pos`(`]` の位置)でエラーにする。`len` が 0 なら何もしない(先に本体を複製しておくと、足さない周でも
/// 入れ子の数だけ本体のコストがかかる)。
fn extend_checked(
    out: &mut Vec<Item>,
    start: usize,
    len: usize,
    pos: usize,
) -> Result<(), MmlError> {
    if len == 0 {
        return Ok(());
    }
    if out.len() + len > MAX_EXPANDED_OPS {
        return Err(mml_err(
            pos,
            format!("繰り返しを展開した命令が {MAX_EXPANDED_OPS} 個を超えました"),
        ));
    }
    out.extend_from_within(start..start + len);
    Ok(())
}

/// 音名 `ch`(`c`〜`b`)と、続く半音記号(`#` `+` `-`)を読んで、オクターブ内の半音数を返す。
fn read_pitch(ch: char, c: &mut Cursor) -> i32 {
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
    base + accidental
}

/// 時間を進める命令(音符・休符・`^`・和音の閉じ)か。連符の中身の検査(`}` の前に長さを持つ命令があるか)に使う。
fn has_length_op(op: &Op) -> bool {
    matches!(
        op,
        Op::Note { .. } | Op::Rest { .. } | Op::TieLen { .. } | Op::ChordEnd { .. }
    )
}

/// 1 トラック分の文字列を命令列にする。繰り返し `[ ... ]n` はここで展開する。
/// 繰り返しは入れ子にできる。展開後の命令数は `MAX_EXPANDED_OPS` で抑える。
/// 連符 `{ ... }n` は入れ子にできず、繰り返しとは括弧が交差しない限り組み合わせられる。
/// 時間の上限はここでは見ず、`run_track` が実行しながら判定する。
fn parse_track(syms: &[(char, usize)], end: usize) -> Result<Vec<Item>, MmlError> {
    let mut c = Cursor { syms, i: 0, end };
    let mut out: Vec<Item> = Vec::new();
    // ここまでに音符・休符・和音を読んだか(`t=` の位置の検査用。`out` をなめ直すと、`t=` を並べた入力で遅くなる)。
    // `^` は数えない: トラックの最初には置けず(置けば `run_track` が `^` のエラーにする)、`t=` の規則に関わらない。
    // 繰り返しの展開は読み済みの命令を複製するだけなので、このフラグは `out` の中身と常に一致する。
    let mut seen_length = false;
    let mut stack: Vec<Frame> = Vec::new();

    while let Some(ch) = c.peek() {
        let pos = c.pos();
        c.bump();
        let op = match ch {
            'c' | 'd' | 'e' | 'f' | 'g' | 'a' | 'b' => {
                let semitone = read_pitch(ch, &mut c);
                let (len, dots) = c.length_and_dots()?;
                Op::Note {
                    semitone,
                    len,
                    dots,
                }
            }
            'r' => {
                let (len, dots) = c.length_and_dots()?;
                Op::Rest { len, dots }
            }
            '&' => Op::Tie,
            '^' => {
                let (len, dots) = c.length_and_dots()?;
                Op::TieLen { len, dots }
            }
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
            't' if c.peek() == Some('=') => {
                c.bump();
                if !stack.is_empty() {
                    return Err(mml_err(pos, "`t=` は繰り返し・連符の外に書いてください"));
                }
                if seen_length {
                    return Err(mml_err(
                        pos,
                        "`t=` はトラックの最初の音符・休符・和音より前に書いてください",
                    ));
                }
                let Some((n, p)) = c.number() else {
                    return Err(mml_err(pos, "`t=` の後に数値が必要です"));
                };
                if !(32..=255).contains(&n) {
                    return Err(mml_err(p, "テンポは 32〜255 です"));
                }
                Op::SharedTempo(n)
            }
            't' => {
                let (n, p) = c.require_number(pos, 't')?;
                if !(32..=255).contains(&n) {
                    return Err(mml_err(p, "テンポは 32〜255 です"));
                }
                Op::Tempo(n)
            }
            'v' => {
                if let Some(sign @ ('+' | '-')) = c.peek() {
                    c.bump();
                    let n = match c.number() {
                        Some((n, p)) => {
                            if !(1..=15).contains(&n) {
                                return Err(mml_err(p, "相対音量は 1〜15 です"));
                            }
                            n
                        }
                        None => 1,
                    };
                    Op::VolumeDelta(if sign == '+' { n as i32 } else { -(n as i32) })
                } else {
                    let (n, p) = c.require_number(pos, 'v')?;
                    if n > 15 {
                        return Err(mml_err(p, "音量は 0〜15 です"));
                    }
                    Op::Volume(n as u8)
                }
            }
            'q' => {
                let (n, p) = c.require_number(pos, 'q')?;
                if !(1..=MAX_GATE).contains(&n) {
                    return Err(mml_err(p, format!("ゲートタイムは 1〜{MAX_GATE} です")));
                }
                Op::Gate(n as u8)
            }
            '@' => {
                if c.peek().is_none() {
                    return Err(mml_err(pos, "`@` の後に音色の番号か名前が必要です"));
                }
                if let Some((n, p)) = c.number() {
                    let Some(t) = Timbre::from_number(n) else {
                        return Err(mml_err(
                            p,
                            format!("音色は 0〜{} です", Timbre::ALL.len() - 1),
                        ));
                    };
                    Op::Timbre(t)
                } else if let Some(t) = c.timbre_name() {
                    Op::Timbre(t)
                } else {
                    return Err(mml_err(c.pos(), "不明な音色です"));
                }
            }
            '\'' => {
                out.push(Item {
                    op: Op::ChordStart,
                    pos,
                });
                let mut count = 0usize;
                loop {
                    let Some(inner) = c.peek() else {
                        return Err(mml_err(pos, "閉じていない `'` があります"));
                    };
                    let inner_pos = c.pos();
                    c.bump();
                    match inner {
                        '\'' => break,
                        'c' | 'd' | 'e' | 'f' | 'g' | 'a' | 'b' => {
                            let semitone = read_pitch(inner, &mut c);
                            out.push(Item {
                                op: Op::ChordNote { semitone },
                                pos: inner_pos,
                            });
                            count += 1;
                        }
                        '<' => out.push(Item {
                            op: Op::OctaveDown,
                            pos: inner_pos,
                        }),
                        '>' => out.push(Item {
                            op: Op::OctaveUp,
                            pos: inner_pos,
                        }),
                        d if d.is_ascii_digit() => {
                            return Err(mml_err(
                                inner_pos,
                                "和音の長さは、閉じの `'` の後ろに書いてください",
                            ));
                        }
                        _ => {
                            return Err(mml_err(
                                inner_pos,
                                "和音の中には音符と `<` `>` だけ書けます",
                            ));
                        }
                    }
                }
                if count == 0 {
                    return Err(mml_err(pos, "和音の中に音符が必要です"));
                }
                let (len, dots) = c.length_and_dots()?;
                Op::ChordEnd { len, dots }
            }
            '[' => {
                stack.push(Frame::Loop {
                    start: out.len(),
                    pos,
                    colon: None,
                });
                continue;
            }
            ':' => match stack.last_mut() {
                Some(Frame::Loop { colon, .. }) if colon.is_none() => {
                    *colon = Some(out.len());
                    continue;
                }
                Some(Frame::Loop { .. }) => {
                    return Err(mml_err(pos, "`:` は繰り返し 1 つにつき 1 個までです"));
                }
                _ => {
                    return Err(mml_err(pos, "`:` は繰り返し `[ ]` の直下にだけ置けます"));
                }
            },
            ']' => {
                let (start, colon) = match stack.pop() {
                    Some(Frame::Loop { start, colon, .. }) => (start, colon),
                    Some(Frame::Tuplet { pos: open, .. }) => {
                        return Err(mml_err(open, "閉じていない `{` があります"));
                    }
                    None => return Err(mml_err(pos, "対応する `[` がありません")),
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
                // 最初の 1 周は `out` に入っている。`:` があれば、最後の 1 周は `:` の手前までだけを足す
                // (`:` が無ければ、全部を n-1 回足す。あれば、全部を n-2 回と、手前までを 1 回)。
                // 足す内容は `out` の中の本体(`start..start + body_len`)から直接複製する。
                let body_len = out.len() - start;
                let head = colon.map(|c| c - start);
                let full_rounds = count - 1 - u32::from(head.is_some());
                for _ in 0..full_rounds {
                    extend_checked(&mut out, start, body_len, pos)?;
                }
                if let Some(head) = head {
                    extend_checked(&mut out, start, head, pos)?;
                }
                continue;
            }
            '{' => {
                if stack.iter().any(|f| matches!(f, Frame::Tuplet { .. })) {
                    return Err(mml_err(pos, "連符はネストできません"));
                }
                stack.push(Frame::Tuplet {
                    start: out.len(),
                    pos,
                });
                Op::TupletStart
            }
            '}' => {
                let (start, open) = match stack.pop() {
                    Some(Frame::Tuplet { start, pos: open }) => (start, open),
                    Some(Frame::Loop { pos: open, .. }) => {
                        return Err(mml_err(open, "閉じていない `[` があります"));
                    }
                    None => return Err(mml_err(pos, "対応する `{` がありません")),
                };
                let (len, dots) = c.length_and_dots()?;
                let has_length = out[start + 1..].iter().any(|it| has_length_op(&it.op));
                if !has_length {
                    return Err(mml_err(open, "連符の中に音符か休符が必要です"));
                }
                Op::TupletEnd { len, dots }
            }
            other => return Err(mml_err(pos, format!("不明な文字 `{other}` です"))),
        };
        if matches!(op, Op::Note { .. } | Op::Rest { .. } | Op::ChordEnd { .. }) {
            seen_length = true;
        }
        out.push(Item { op, pos });
    }

    match stack.last() {
        Some(Frame::Loop { pos, .. }) => Err(mml_err(*pos, "閉じていない `[` があります")),
        Some(Frame::Tuplet { pos, .. }) => Err(mml_err(*pos, "閉じていない `{` があります")),
        None => Ok(out),
    }
}

struct TrackState {
    octave: i32,
    len: u32,
    tempo: u32,
    volume: u32,
    gate: u32,
    timbre: Timbre,
    t: f64,
    /// 直前の命令が音符・和音だったとき、その音の `notes` 内の index(`^` と `&` の連結先。休符で空になる)
    last_notes: Vec<usize>,
    /// 直前の命令が和音だったか(和音は `&` でつなげない)
    last_chord: bool,
    /// `^` で延ばした長さを、まだ `notes` に書いていないか(`flush_last` で書く)
    last_dirty: bool,
    /// 和音を始めたときのオクターブ(閉じで戻す)
    chord_octave: i32,
    /// 和音の中でここまでに読んだ音の (MIDI ノート番号, 元の文字位置)
    chord_notes: Vec<(i32, usize)>,
    /// 直前の音符の全長(秒。ゲートを掛ける前。タイで延びる)
    last_full: f64,
    /// 直前の音符を作ったときのゲート(タイで延びても、先頭の音符のものを使う)
    last_gate: u32,
    /// 直前に `&` があったとき、その位置
    tie_pos: Option<usize>,
    /// 連符の中で、拍の長さに掛ける縮小率(連符の外では 1.0)
    tuplet_scale: f64,
}

/// 長さ(拍。四分音符が 1)。
fn beats(len: u32, dots: u8) -> f64 {
    let dot_factor = match dots {
        0 => 1.0,
        1 => 1.5,
        _ => 1.75,
    };
    4.0 / f64::from(len) * dot_factor
}

/// 長さを秒にする。`scale` は連符の縮小率(連符の外では 1.0。1.0 を掛けても値は変わらない)。
fn duration_sec(len: u32, dots: u8, tempo: u32, scale: f64) -> f64 {
    beats(len, dots) * scale * 60.0 / f64::from(tempo)
}

/// 全長にゲートタイム(百分率)を掛ける。100 のときは掛け算をせず、v1 と同じ値のまま返す。
fn gated(full: f64, gate: u32) -> f64 {
    if gate >= MAX_GATE {
        full
    } else {
        full * f64::from(gate) / f64::from(MAX_GATE)
    }
}

fn check_limit(t: f64, pos: usize) -> Result<(), MmlError> {
    if t > MAX_TOTAL_SEC + TIME_EPS {
        return Err(mml_err(
            pos,
            format!("再生時間は {} 秒以下にしてください", MAX_TOTAL_SEC as u32),
        ));
    }
    Ok(())
}

/// `{` の直後の命令列 `rest`(最初の `TupletEnd` まで)から、連符の縮小率を求める。
/// 縮小率 = `}` の後の長さ(拍) ÷ 中の音符・休符・`^` の長さの合計(拍)。中の `l` の変更も追う。
/// `}` の長さが省略されているときは、`{` の時点の `l`(`l_at_open`)を使う。
/// 連符は入れ子にならず、`parse_track` が `{` と `}` を必ず対にするので、`TupletEnd` は必ずある。
fn tuplet_scale(rest: &[Item], l_at_open: u32) -> f64 {
    let mut l = l_at_open;
    let mut sum = 0.0;
    for it in rest {
        match &it.op {
            Op::DefaultLen(n) => l = *n,
            Op::Note { len, dots, .. }
            | Op::Rest { len, dots }
            | Op::TieLen { len, dots }
            | Op::ChordEnd { len, dots } => {
                sum += beats(len.unwrap_or(l), *dots);
            }
            Op::TupletEnd { len, dots } => {
                return beats(len.unwrap_or(l_at_open), *dots) / sum;
            }
            _ => {}
        }
    }
    unreachable!("parse_track pairs every `{{` with a `}}`")
}

/// `^` で延ばした直前の音符・和音の長さを、`notes` に書き込む。`^` のたびに和音の全部の音を書き直すと、
/// `^` を大量に展開した入力で遅くなるので、次の音符・和音・休符・トラックの終わりで、まとめて 1 回だけ書く。
fn flush_last(st: &mut TrackState, notes: &mut [NoteEvent]) {
    if st.last_dirty {
        let d = gated(st.last_full, st.last_gate);
        for &idx in &st.last_notes {
            notes[idx].duration_sec = d;
        }
        st.last_dirty = false;
    }
}

const CHORD_TIE_MESSAGE: &str =
    "`&` は和音とは組み合わせられません(長くしたいときは `^` を使ってください)";

/// 命令列を実行して音を `notes` に積み、トラックの終了時刻(秒)を返す。
/// 時間の積算は音符・休符ごとに行い、上限を超えた最初の音符・休符でエラーにして打ち切る。
/// (繰り返しは `parse_track` で先に展開済み。その大きさは `MAX_EXPANDED_OPS` で抑えられている。)
fn run_track(
    track: u8,
    items: &[Item],
    initial_tempo: u32,
    notes: &mut Vec<NoteEvent>,
) -> Result<f64, MmlError> {
    let mut st = TrackState {
        octave: DEFAULT_OCTAVE,
        len: DEFAULT_LEN,
        tempo: initial_tempo,
        volume: DEFAULT_VOLUME,
        gate: MAX_GATE,
        timbre: Timbre::Fm,
        t: 0.0,
        last_notes: Vec::new(),
        last_chord: false,
        last_dirty: false,
        chord_octave: DEFAULT_OCTAVE,
        chord_notes: Vec::new(),
        last_full: 0.0,
        last_gate: MAX_GATE,
        tie_pos: None,
        tuplet_scale: 1.0,
    };

    for (i, Item { op, pos }) in items.iter().enumerate() {
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
            Op::SharedTempo(_) => {}
            Op::Volume(n) => st.volume = u32::from(*n),
            Op::VolumeDelta(delta) => {
                let v = st.volume as i32 + delta;
                if !(0..=15).contains(&v) {
                    return Err(mml_err(pos, "音量は 0〜15 です"));
                }
                st.volume = v as u32;
            }
            Op::Gate(n) => st.gate = u32::from(*n),
            Op::Timbre(t) => st.timbre = *t,
            Op::TupletStart => st.tuplet_scale = tuplet_scale(&items[i + 1..], st.len),
            Op::TupletEnd { .. } => st.tuplet_scale = 1.0,
            Op::Tie => {
                if st.last_chord {
                    return Err(mml_err(pos, CHORD_TIE_MESSAGE));
                }
                if st.last_notes.is_empty() || st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の前に音符が必要です"));
                }
                st.tie_pos = Some(pos);
            }
            Op::TieLen { len, dots } => {
                if st.tie_pos.is_some() {
                    return Err(mml_err(pos, "`&` の直後には `^` を置けません"));
                }
                if st.last_notes.is_empty() {
                    return Err(mml_err(pos, "`^` の前に音符が必要です"));
                }
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                st.last_full += d;
                st.last_dirty = true;
                st.t += d;
                check_limit(st.t, pos)?;
            }
            Op::Rest { len, dots } => {
                if let Some(tp) = st.tie_pos {
                    return Err(mml_err(tp, "`&` の後に音符が必要です"));
                }
                st.t += duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                flush_last(&mut st, notes);
                st.last_notes.clear();
                st.last_chord = false;
                check_limit(st.t, pos)?;
            }
            Op::Note {
                semitone,
                len,
                dots,
            } => {
                let midi = 12 * (st.octave + 1) + semitone;
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                if st.tie_pos.take().is_some() {
                    let idx = st.last_notes[0];
                    if i32::from(notes[idx].midi) != midi {
                        return Err(mml_err(pos, "`&` は同じ高さの音符どうしだけをつなげます"));
                    }
                    st.last_full += d;
                    notes[idx].duration_sec = gated(st.last_full, st.last_gate);
                    st.last_dirty = false;
                } else {
                    flush_last(&mut st, notes);
                    if notes.len() >= MAX_NOTES {
                        return Err(mml_err(pos, format!("音符は {MAX_NOTES} 個までです")));
                    }
                    notes.push(NoteEvent {
                        track,
                        start_sec: st.t,
                        duration_sec: gated(d, st.gate),
                        midi: midi as u8,
                        velocity: f64::from(st.volume) / 15.0,
                        timbre: st.timbre,
                    });
                    st.last_notes.clear();
                    st.last_notes.push(notes.len() - 1);
                    st.last_chord = false;
                    st.last_full = d;
                    st.last_gate = st.gate;
                }
                st.t += d;
                check_limit(st.t, pos)?;
            }
            Op::ChordStart => {
                if let Some(tp) = st.tie_pos {
                    return Err(mml_err(tp, CHORD_TIE_MESSAGE));
                }
                st.chord_octave = st.octave;
                st.chord_notes.clear();
            }
            Op::ChordNote { semitone } => {
                st.chord_notes.push((12 * (st.octave + 1) + semitone, pos));
            }
            Op::ChordEnd { len, dots } => {
                let d = duration_sec(len.unwrap_or(st.len), *dots, st.tempo, st.tuplet_scale);
                let velocity = f64::from(st.volume) / 15.0 / st.chord_notes.len() as f64;
                flush_last(&mut st, notes);
                st.last_notes.clear();
                for &(midi, note_pos) in &st.chord_notes {
                    if notes.len() >= MAX_NOTES {
                        return Err(mml_err(note_pos, format!("音符は {MAX_NOTES} 個までです")));
                    }
                    notes.push(NoteEvent {
                        track,
                        start_sec: st.t,
                        duration_sec: gated(d, st.gate),
                        midi: midi as u8,
                        velocity,
                        timbre: st.timbre,
                    });
                    st.last_notes.push(notes.len() - 1);
                }
                st.last_chord = true;
                st.last_full = d;
                st.last_gate = st.gate;
                st.octave = st.chord_octave;
                st.t += d;
                check_limit(st.t, pos)?;
            }
        }
    }

    flush_last(&mut st, notes);
    if let Some(tp) = st.tie_pos {
        return Err(mml_err(tp, "`&` の後に音符が必要です"));
    }
    Ok(st.t)
}

/// 全トラックの `t=` を集めて、共有の初期テンポを 1 つに決める(書かれていなければ既定の 120)。
/// 2 個目は、その `t` の位置でエラー。
fn shared_tempo(tracks: &[Vec<Item>]) -> Result<u32, MmlError> {
    let mut found: Option<u32> = None;
    for it in tracks.iter().flatten() {
        if let Op::SharedTempo(n) = &it.op {
            if found.is_some() {
                return Err(mml_err(it.pos, "`t=` は 1 回だけ書けます"));
            }
            found = Some(*n);
        }
    }
    Ok(found.unwrap_or(DEFAULT_TEMPO))
}

/// (文字, 元の文字位置)。
type Sym = (char, usize);

/// コメントを取り除いた (文字, 元の文字位置) の列を作る。空白はそのまま残す(マクロの名前の区切りに使う)。
/// コメントは `// …`(改行の手前まで)と `/* … */`(入れ子にできない)で、開始位置の空白 1 つに置き換える
/// (コメントも名前の区切りとして働く)。`/` だけはコメントにならず、そのまま列に残って `parse_track` が
/// 「不明な文字」にする。`;` はトラック区切りなのでコメントの開始にしない。
/// 閉じていない `/*` は、その `/*` の位置のエラー。
fn strip_comments(text: &str) -> Result<Vec<Sym>, MmlError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            out.push((' ', i));
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let open = i;
            out.push((' ', open));
            i += 2;
            loop {
                if i + 1 >= chars.len() {
                    return Err(mml_err(open, "`/*` が閉じていません"));
                }
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        out.push((c, i));
        i += 1;
    }
    Ok(out)
}

/// 空白を除き、ASCII だけ小文字化する(パーサーが読む形)。
fn finish_symbols(chars: &[Sym]) -> Vec<Sym> {
    chars
        .iter()
        .filter(|(c, _)| !c.is_whitespace())
        .map(|&(c, p)| (c.to_ascii_lowercase(), p))
        .collect()
}

/// マクロを展開した後の文字数の上限。入れ子のマクロで展開が膨らむ入力を抑える。
const MAX_EXPANDED_SYMBOLS: usize = 100_000;

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `chars[i]`(`$`)の直後に途切れなく続く名前(小文字にしたもの)と、その次の添字を返す。
fn read_macro_name(chars: &[Sym], i: usize) -> (String, usize) {
    let mut j = i + 1;
    let mut name = String::new();
    while j < chars.len() && is_name_char(chars[j].0) {
        name.push(chars[j].0.to_ascii_lowercase());
        j += 1;
    }
    (name, j)
}

/// `from` から、空白を読み飛ばした次の添字。
fn skip_whitespace(chars: &[Sym], mut from: usize) -> usize {
    while from < chars.len() && chars[from].0.is_whitespace() {
        from += 1;
    }
    from
}

/// `chars[i]`(`$`)が定義の頭(`$名前 = {`)なら、名前と `{` の添字を返す。
fn definition_head(chars: &[Sym], i: usize) -> Option<(String, usize)> {
    let (name, j) = read_macro_name(chars, i);
    if name.is_empty() {
        return None;
    }
    let eq = skip_whitespace(chars, j);
    if chars.get(eq).map(|s| s.0) != Some('=') {
        return None;
    }
    let open = skip_whitespace(chars, eq + 1);
    if chars.get(open).map(|s| s.0) != Some('{') {
        return None;
    }
    Some((name, open))
}

/// 定義(`$名前 = { 本体 }`)を取り除き、表に入れる。戻り値は (定義を除いた列, 名前 → 本体)。
fn collect_macros(chars: &[Sym]) -> Result<(Vec<Sym>, HashMap<String, Vec<Sym>>), MmlError> {
    let mut rest = Vec::with_capacity(chars.len());
    let mut macros: HashMap<String, Vec<Sym>> = HashMap::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].0 == '$' {
            if let Some((name, open)) = definition_head(chars, i) {
                if macros.contains_key(&name) {
                    return Err(mml_err(
                        chars[i].1,
                        format!("マクロ `${name}` は既に定義されています"),
                    ));
                }
                // 本体は、対になる `}` まで(本体の中の `{ }` は深さで数える)
                let mut depth = 1usize;
                let mut k = open + 1;
                while k < chars.len() {
                    match chars[k].0 {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        ';' => {
                            return Err(mml_err(chars[k].1, "マクロの中には `;` を書けません"));
                        }
                        '$' if definition_head(chars, k).is_some() => {
                            return Err(mml_err(chars[k].1, "マクロの中には定義を書けません"));
                        }
                        _ => {}
                    }
                    k += 1;
                }
                if k >= chars.len() {
                    return Err(mml_err(
                        chars[open].1,
                        "マクロの本体の `{` が閉じていません",
                    ));
                }
                macros.insert(name, chars[open + 1..k].to_vec());
                i = k + 1;
                continue;
            }
        }
        rest.push(chars[i]);
        i += 1;
    }
    Ok((rest, macros))
}

/// 未定義のマクロのエラー。名前の先頭が定義済みの名前と一致するときは、区切りを案内する
/// (`$ab` は `$a` + `b` ではなく、名前 `ab` と読まれる)。
fn undefined_macro(pos: usize, name: &str, macros: &HashMap<String, Vec<Sym>>) -> MmlError {
    if name.is_empty() {
        return mml_err(pos, "`$` の後にマクロの名前が必要です");
    }
    let known = macros
        .keys()
        .filter(|k| name.starts_with(k.as_str()))
        .max_by_key(|k| k.len());
    match known {
        Some(k) => mml_err(
            pos,
            format!(
                "未定義のマクロ `${name}` です(`${k}` を使うなら、後ろの `{}` を空白で区切ってください)",
                &name[k.len()..]
            ),
        ),
        None => mml_err(
            pos,
            format!("未定義のマクロ `${name}` です(定義は `$名前={{…}}` の形で書きます)"),
        ),
    }
}

/// `chars` の `$名前` を、本体で置き換えながら `out` に写す(本体の中の `$` も再帰的に展開する)。
/// `stack` は展開中のマクロの名前(再帰の検出用)。`use_pos` は、いま展開している `$` の位置(上限のエラーの位置)。
fn expand_into(
    chars: &[Sym],
    macros: &HashMap<String, Vec<Sym>>,
    stack: &mut Vec<String>,
    out: &mut Vec<Sym>,
    use_pos: usize,
) -> Result<(), MmlError> {
    let mut i = 0;
    while i < chars.len() {
        let (c, pos) = chars[i];
        if c == '$' {
            let (name, j) = read_macro_name(chars, i);
            let Some(body) = macros.get(&name) else {
                return Err(undefined_macro(pos, &name, macros));
            };
            if stack.contains(&name) {
                return Err(mml_err(
                    pos,
                    format!("マクロ `${name}` が自分自身を呼んでいます"),
                ));
            }
            stack.push(name);
            expand_into(body, macros, stack, out, pos)?;
            stack.pop();
            i = j;
            continue;
        }
        if out.len() >= MAX_EXPANDED_SYMBOLS {
            return Err(mml_err(
                use_pos,
                format!("マクロを展開した文字数が {MAX_EXPANDED_SYMBOLS} を超えました"),
            ));
        }
        out.push((c, pos));
        i += 1;
    }
    Ok(())
}

/// マクロの定義を取り除き、`$名前` を本体で置き換える。`$` を含まない MML は、そのまま返す。
fn expand_macros(chars: Vec<Sym>) -> Result<Vec<Sym>, MmlError> {
    if !chars.iter().any(|s| s.0 == '$') {
        return Ok(chars);
    }
    let (rest, macros) = collect_macros(&chars)?;
    let mut out = Vec::with_capacity(rest.len());
    expand_into(&rest, &macros, &mut Vec::new(), &mut out, 0)?;
    Ok(out)
}

/// MML テキストを解釈する。
pub fn parse(text: &str) -> Result<MmlScore, MmlError> {
    let total_chars = text.chars().count();
    if total_chars > MAX_TEXT_CHARS {
        return Err(mml_err(
            MAX_TEXT_CHARS,
            format!("MML は {MAX_TEXT_CHARS} 文字以下にしてください"),
        ));
    }

    let syms = finish_symbols(&expand_macros(strip_comments(text)?)?);

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

    // 全トラックを先に構文解析する(`t=` は、どのトラックに書かれていても全トラックの初期テンポになる)
    let mut tracks: Vec<Vec<Item>> = Vec::new();
    for (seg, sep) in segments.into_iter().filter(|(s, _)| !s.is_empty()) {
        if tracks.len() >= MAX_TRACKS {
            return Err(mml_err(sep, "トラックは 4 つまでです"));
        }
        tracks.push(parse_track(seg, total_chars)?);
    }
    let initial_tempo = shared_tempo(&tracks)?;

    let mut notes: Vec<NoteEvent> = Vec::new();
    let mut total_sec = 0.0_f64;
    for (i, items) in tracks.iter().enumerate() {
        let end = run_track(i as u8, items, initial_tempo, &mut notes)?;
        total_sec = total_sec.max(end);
    }
    let track_count = tracks.len() as u8;

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
    use crate::domain::{MmlError, MmlScore, Timbre};

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    /// frontend/src/lib/mmlLimits.ts の MML_LIMITS と同じ表。片方だけ変えると、どちらかのテストが落ちる。
    #[test]
    fn limits_match_frontend_table() {
        assert_eq!(MAX_TEXT_CHARS, 5000);
        assert_eq!(MAX_NOTES, 3000);
        assert_eq!(MAX_TRACKS, 4);
        approx(MAX_TOTAL_SEC, 90.0);
    }

    /// frontend/src/lib/mmlSampleData.json(入力欄の「サンプル」ボタンが入れる例)は、すべて実際のパーサーと上限を通る。
    /// 文法を変えて通らなくなったサンプルを、ここで検出する。
    #[test]
    fn frontend_samples_parse_within_limits() {
        #[derive(serde::Deserialize)]
        struct Sample {
            id: String,
            label: String,
            mml: String,
        }
        let samples: Vec<Sample> = serde_json::from_str(include_str!("../../frontend/src/lib/mmlSampleData.json"))
            .expect("mmlSampleData.json は {id,label,mml} の配列");
        assert!(samples.len() >= 3, "サンプルは 3 つ以上");
        let mut ids: Vec<&str> = samples.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), samples.len(), "id が重複している");
        for s in &samples {
            assert!(!s.label.is_empty(), "{}: label が空", s.id);
            let score = must_parse(&s.mml);
            assert!(!score.notes.is_empty(), "{}: 音が無い", s.id);
        }
        let parsed: Vec<MmlScore> = samples.iter().map(|s| must_parse(&s.mml)).collect();
        assert!(parsed.iter().any(|p| p.track_count >= 2), "複数トラックの例が要る");
        assert!(
            samples.iter().any(|s| s.mml.contains('@')),
            "音色(@)を使う例が要る"
        );
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
        assert!(parse(&format!("c{}", " ".repeat(4999))).is_ok()); // ちょうど 5000 文字
        let e = must_fail(&format!("c{}", " ".repeat(5000)));
        assert_eq!(e.position, 5000);
        assert!(e.message.contains("5000"), "{}", e.message);
    }

    #[test]
    fn total_duration_limit_stops_at_the_offending_note() {
        // `c1` は 2 秒(t120 l4)。45 個でちょうど 90 秒。
        assert!(parse(&"c1".repeat(45)).is_ok());
        let e = must_fail(&"c1".repeat(46));
        assert_eq!(e.position, 90); // 46 個目の `c1`
        assert!(e.message.contains("90 秒"), "{}", e.message);
    }

    #[test]
    fn loop_expansion_stops_at_the_duration_limit() {
        // `c1` は 2 秒。`[c1]99` は先に展開され、46 個目の `c1` で 90 秒を超えたところで止まる。
        // 展開された命令は元の位置を引き継ぐので、エラー位置は繰り返し本体の `c1`(位置 1)を指す。
        assert_eq!(must_fail("[c1]99").position, 1);
    }

    #[test]
    fn note_count_limit_stops_at_the_offending_note() {
        // `t255 l64` の 1 音は約 0.0147 秒。3000 音でも約 44 秒で、90 秒にも 5000 文字にも収まる。
        let head = "t255l64"; // 7 文字
        let base = format!("{head}{}", "[c]99".repeat(30)); // 2970 音、157 文字
        let ok = format!("{base}[c]30"); // ちょうど 3000 音
        assert!(ok.chars().count() <= MAX_TEXT_CHARS);
        let s = must_parse(&ok);
        assert_eq!(s.notes.len(), MAX_NOTES);
        assert!(s.total_sec < MAX_TOTAL_SEC);
        // 3001 個目の音符(繰り返し本体の `c`、`[` は 157 文字目、`c` は 158 文字目)で止まる。
        let e = must_fail(&format!("{base}[c]31"));
        assert_eq!(e.position, 158);
        assert!(e.message.contains("3000"), "{}", e.message);
    }

    #[test]
    fn note_count_limit_is_shared_across_tracks() {
        let t0 = format!("t255l64{}", "[c]99".repeat(30)); // 2970 音、157 文字(; は 157)
        let ok = format!("{t0};t255l64[c]30");
        assert_eq!(must_parse(&ok).notes.len(), MAX_NOTES);
        // 2 本目のトラックの 31 個目で 3001 個目になる。`;` の後ろ(158〜)の `t255l64` 7 文字、`[` が 165、`c` が 166。
        let e = must_fail(&format!("{t0};t255l64[c]31"));
        assert_eq!(e.position, 166);
        assert!(e.message.contains("3000"), "{}", e.message);
    }

    #[test]
    fn tied_notes_count_as_one_for_the_note_limit() {
        // `c64&c64` は 1 音。16 回 × 99 = 1584 音(タイを数えると 3168 個で上限超過)。
        let text = format!("t255l64{}", "[c64&c64]99".repeat(16));
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        assert_eq!(must_parse(&text).notes.len(), 1584);
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

    /// v1 の記法は、v2 でも同じ結果になる(新しい記号は v1 では構文エラーだったので、意味は変わらない)。
    #[test]
    fn v1_syntax_keeps_its_results() {
        let s = must_parse("t120 o5 l8 cdefgab>c");
        let midis: Vec<u8> = s.notes.iter().map(|n| n.midi).collect();
        assert_eq!(midis, vec![72, 74, 76, 77, 79, 81, 83, 84]);
        for (i, n) in s.notes.iter().enumerate() {
            approx(n.start_sec, 0.25 * i as f64);
            approx(n.duration_sec, 0.25);
        }
        approx(s.total_sec, 2.0);
    }

    #[test]
    fn line_comments_run_to_the_end_of_the_line() {
        assert_eq!(must_parse("c4 // d4 e4\nf4"), must_parse("c4 f4"));
        assert_eq!(must_parse("c4 // 末尾までコメント"), must_parse("c4"));
        assert_eq!(must_parse("// 全部コメント\nc"), must_parse("c"));
        assert_eq!(must_parse("c4 // x\r\nd4"), must_parse("c4 d4")); // CRLF
    }

    #[test]
    fn block_comments_may_span_lines_and_do_not_nest() {
        assert_eq!(must_parse("c4 /* d4 */ e4"), must_parse("c4 e4"));
        assert_eq!(must_parse("c4/*複数行\nの*/e4"), must_parse("c4e4"));
        assert_eq!(must_parse("c/**/d"), must_parse("cd"));
        // 入れ子はできない: 最初の `*/` で閉じ、残りの ` c */ d` は音符 c と、不明な文字 `*`
        assert_eq!(must_fail("/* a /* b */ c */ d").position, 15);
    }

    #[test]
    fn unterminated_block_comment_is_an_error_at_its_opening() {
        assert_eq!(must_fail("c4 /* d4").position, 3);
        assert_eq!(must_fail("c /*/").position, 2); // `/*/` は閉じていない
    }

    #[test]
    fn a_lone_slash_is_still_an_unknown_character() {
        assert_eq!(must_fail("c/d").position, 1);
        assert_eq!(must_fail("c / / d").position, 2); // 間に空白があれば `//` ではない
    }

    #[test]
    fn comments_do_not_split_tracks() {
        assert_eq!(must_parse("c4 // ; d4\ne4").track_count, 1);
        assert_eq!(must_parse("c4 /* ; */ e4").track_count, 1);
        assert_eq!(must_parse("c4 /* x */ ; e4").track_count, 2);
    }

    #[test]
    fn error_positions_after_comments_stay_in_the_original_text() {
        // `/* あ */ c4 x` で x は 12 文字目(0 始まりの 11)
        assert_eq!(must_fail("/* あ */ c4 x").position, 11);
        assert_eq!(must_fail("// あ\nx").position, 5);
    }

    #[test]
    fn text_length_limit_counts_comments() {
        // `c //` が 4 文字、残り 4996 文字がコメント: ちょうど 5000 文字
        assert!(parse(&format!("c //{}", "あ".repeat(4996))).is_ok());
        let e = must_fail(&format!("c //{}", "あ".repeat(4997)));
        assert_eq!(e.position, 5000);
    }

    #[test]
    fn caret_extends_the_previous_note() {
        let s = must_parse("c4^8");
        assert_eq!(s.notes.len(), 1);
        approx(s.notes[0].duration_sec, 0.75);
        approx(must_parse("c4^").notes[0].duration_sec, 1.0); // 長さ省略は l(4)
        approx(must_parse("l8 c^").notes[0].duration_sec, 0.5);
        approx(must_parse("c4^8.").notes[0].duration_sec, 0.5 + 0.375);
        approx(must_parse("c4^4^4").notes[0].duration_sec, 1.5);
        approx(must_parse("c4&c8^8").notes[0].duration_sec, 1.0); // `&` との併用
        approx(must_parse("c4^8&c8").notes[0].duration_sec, 1.0);
        let s = must_parse("c4^8 d4");
        approx(s.notes[1].start_sec, 0.75);
        assert_eq!(must_parse("[c4^8]2").notes.len(), 2);
        assert_eq!(must_parse("C4^8"), must_parse("c4 ^ 8"));
    }

    #[test]
    fn invalid_carets() {
        assert_eq!(must_fail("^4 c").position, 0); // 前に音符が無い
        assert_eq!(must_fail("r4^4 c").position, 2); // 休符の後ろ
        assert_eq!(must_fail("c4 r4 ^4").position, 6);
        assert_eq!(must_fail("c4&^8").position, 3); // `&` の直後
        assert_eq!(must_fail("c4^3").position, 3); // 長さが不正
        assert_eq!(must_fail("c4^8...").position, 6); // 3 個目の付点
        assert_eq!(must_fail("c4^8&").position, 4); // 後ろに音符が無い `&`
    }

    #[test]
    fn relative_volume() {
        let vel = |t: &str| must_parse(t).notes[0].velocity;
        approx(vel("v5 v+2 c"), 7.0 / 15.0);
        approx(vel("v5 v-2 c"), 3.0 / 15.0);
        approx(vel("v5 v+ c"), 6.0 / 15.0); // 数字省略は 1
        approx(vel("v5 v- c"), 4.0 / 15.0);
        approx(vel("v+5 c"), 1.0); // 既定 v10 から 15
        approx(vel("v-10 c"), 0.0);
        // 絶対指定の v は今までどおり
        approx(vel("v15 c"), 1.0);
        approx(vel("V5 V + 2 c"), 7.0 / 15.0); // 大文字・空白
    }

    #[test]
    fn relative_volume_errors() {
        assert_eq!(must_fail("v+6 c").position, 0); // 10 + 6 = 16 は範囲外。`v` の位置
        assert_eq!(must_fail("v0 v- c").position, 3); // 0 - 1 は範囲外
        assert_eq!(must_fail("v+0 c").position, 2); // n は 1〜15。数字の位置
        assert_eq!(must_fail("v+16 c").position, 2);
        assert_eq!(must_fail("v c").position, 0); // 絶対指定は数字が必要なまま
    }

    #[test]
    fn gate_shortens_notes_but_not_the_timeline() {
        let s = must_parse("q50 c4 d4");
        approx(s.notes[0].duration_sec, 0.25);
        approx(s.notes[1].start_sec, 0.5); // 次の音の開始は変わらない
        approx(s.notes[1].duration_sec, 0.25);
        approx(s.total_sec, 1.0); // 再生時間は全長で数える
        approx(must_parse("c4 q50 d4").notes[0].duration_sec, 0.5); // 変更前の音は影響を受けない
        approx(must_parse("q100 c4").notes[0].duration_sec, 0.5);
        approx(must_parse("q1 c4").notes[0].duration_sec, 0.005);
        // 休符は q の影響を受けない(そのまま進む)
        approx(must_parse("q50 c4 r4 d4").notes[1].start_sec, 1.0);
    }

    #[test]
    fn gate_applies_to_the_whole_tied_note() {
        approx(must_parse("q50 c4&c4").notes[0].duration_sec, 0.5); // 全長 1.0 の半分
        approx(must_parse("q50 c4^4").notes[0].duration_sec, 0.5);
        // タイの途中で q を変えても、先頭の音の q を使う
        approx(must_parse("q50 c4&q100 c4").notes[0].duration_sec, 0.5);
        approx(must_parse("q50 c4^4&c4").notes[0].duration_sec, 0.75);
    }

    #[test]
    fn gate_is_per_track_and_validated() {
        approx(must_parse("q50 c4;c4").notes[1].duration_sec, 0.5);
        assert_eq!(must_fail("q0 c").position, 1);
        assert_eq!(must_fail("q101 c").position, 1);
        assert_eq!(must_fail("q c").position, 0);
    }

    #[test]
    fn default_gate_leaves_durations_bit_exact() {
        // q100 は掛け算をしないので、q を書かない場合と、浮動小数点の値まで完全に同じになる
        assert_eq!(
            must_parse("q100 t137 c4.").notes[0].duration_sec,
            must_parse("t137 c4.").notes[0].duration_sec
        );
    }

    fn midis(t: &str) -> Vec<u8> {
        must_parse(t).notes.iter().map(|n| n.midi).collect()
    }

    #[test]
    fn loop_escape_skips_the_tail_on_the_last_round() {
        assert_eq!(midis("[c:d]3"), vec![60, 62, 60, 62, 60]);
        assert_eq!(midis("[c:d]"), vec![60, 62, 60]); // 回数省略は 2
        assert_eq!(midis("[:c]2"), vec![60]);
        assert_eq!(midis("[c:]2"), vec![60, 60]);
        assert_eq!(
            midis("[cd:e]4"),
            vec![60, 62, 64, 60, 62, 64, 60, 62, 64, 60, 62]
        );
    }

    #[test]
    fn loops_can_be_nested() {
        assert_eq!(midis("[c[d]2]2"), vec![60, 62, 62, 60, 62, 62]);
        assert_eq!(midis("[[c]2 d]2"), vec![60, 60, 62, 60, 60, 62]);
        // 内側のループの `:` は内側に効く
        assert_eq!(midis("[c[d:e]2]2"), vec![60, 62, 64, 62, 60, 62, 64, 62]);
        // 外側の `:` は、内側のループを含めた手前までが「最後の回にも鳴る部分」
        assert_eq!(midis("[a[d]2:c]2"), vec![69, 62, 62, 60, 69, 62, 62]);
    }

    #[test]
    fn invalid_loop_escapes() {
        assert_eq!(must_fail("c:d").position, 1); // ループの外
        assert_eq!(must_fail("[c:d:e]2").position, 4); // 2 個目
        assert_eq!(must_fail("[c:d]1").position, 5); // 回数は 2〜99 のまま
        assert_eq!(must_fail("[c;d]2").position, 0); // `;` は繰り返しをまたげない
    }

    #[test]
    fn nested_loop_expansion_is_capped_before_it_explodes() {
        // 4 重のネスト: 99^4 個に展開されてしまうが、50 万を超えた `]` で止まる(メモリを使い切らない)。
        // 3 つ目の `]`(位置 11)の展開が 9801 × 99 = 970,299 個になるところで止まる。
        let e = must_fail("[[[[c]99]99]99]99");
        assert_eq!(e.position, 11);
        assert!(e.message.contains("命令"), "{}", e.message);
    }

    #[test]
    fn nested_empty_tail_loops_do_not_copy_the_body_quadratically() {
        // `[:` を 800 重に入れ子にし、一番内側に約 25 万命令に展開されるループを置く。外側の `]` は
        // どれも `:` が先頭なので 1 命令も足さない。足さない周で本体を複製していると、約 2 億個の複製で
        // 秒単位の時間がかかる(足した命令数は上限に効かない)。
        let text = format!(
            "c{}[{}]99{}",
            "[:".repeat(800),
            "><".repeat(1250),
            "]".repeat(800)
        );
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        let started = std::time::Instant::now();
        let _ = parse(&text);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "parse が {:?} かかった",
            started.elapsed()
        );
    }

    #[test]
    fn the_largest_v1_loop_still_parses() {
        // v1 で通る最大級: ループ 1 組で、本体が 4994 文字、99 回の繰り返しで約 49.4 万命令。
        // `><` はオクターブを往復するだけで音を出さない(音の上限・時間の上限では抑えられない命令)。
        let text = format!("c[{}]99", "><".repeat(2497));
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        assert_eq!(must_parse(&text).notes.len(), 1);
        // ネストで 50 万を超える: 内側が 198,000 命令、外側の 2 回目の複製で 594,000 になる。
        let body = "><".repeat(1000);
        let text = format!("c[[{body}]99]99");
        let e = must_fail(&text);
        assert_eq!(e.position as usize, text.chars().count() - 3); // 外側の `]`
        assert!(e.message.contains("命令"), "{}", e.message);
    }

    #[test]
    fn tuplet_fits_in_the_given_length() {
        let s = must_parse("{c d e}4");
        assert_eq!(s.notes.len(), 3);
        // 四分音符 1 つ分(0.5 秒)を 3 等分
        approx(s.notes[0].start_sec, 0.0);
        approx(s.notes[1].start_sec, 0.5 / 3.0);
        approx(s.notes[2].start_sec, 1.0 / 3.0);
        approx(s.notes[0].duration_sec, 0.5 / 3.0);
        approx(s.total_sec, 0.5);
        // 長さ省略は `{` の時点の l
        approx(must_parse("l8 {c d e}").total_sec, 0.25);
        // 付点
        approx(must_parse("{c d e}4.").total_sec, 0.75);
        // 連符の後は元の長さに戻る
        let s = must_parse("{c d e}4 f4");
        approx(s.notes[3].start_sec, 0.5);
        approx(s.notes[3].duration_sec, 0.5);
        assert_eq!(must_parse("{C D E}4"), must_parse("{c d e}4")); // 大文字・小文字は区別しない
    }

    #[test]
    fn tuplet_divides_by_the_ratio_of_the_inner_lengths() {
        let s = must_parse("{c4 d8}4"); // 1 拍 : 0.5 拍 を、1 拍に収める
        approx(s.notes[0].duration_sec, 0.5 * 2.0 / 3.0);
        approx(s.notes[1].start_sec, 0.5 * 2.0 / 3.0);
        approx(s.total_sec, 0.5);
        // 中の l の変更も追う(`{` の時点の l は 4、中は l8 の 3 音)
        let s = must_parse("{l8 c d e}4");
        approx(s.notes[0].duration_sec, 0.5 / 3.0);
        approx(s.total_sec, 0.5);
        // 中の `^` も長さに数える
        let s = must_parse("{c4^4 d4}2");
        assert_eq!(s.notes.len(), 2);
        approx(s.notes[0].duration_sec, 2.0 / 3.0);
        approx(s.notes[1].start_sec, 2.0 / 3.0);
        approx(s.total_sec, 1.0);
        // 休符も数える
        let s = must_parse("{c r e}4");
        assert_eq!(s.notes.len(), 2);
        approx(s.notes[1].start_sec, 1.0 / 3.0);
        approx(s.total_sec, 0.5);
        // 中でテンポを変えても、縮小率(拍の比)は変わらない
        let s = must_parse("{c t60 d e}4");
        approx(s.total_sec, 0.5 / 3.0 + 2.0 / 3.0);
    }

    #[test]
    fn tuplets_combine_with_loops_and_q() {
        let s = must_parse("[{c d e}4]2");
        assert_eq!(s.notes.len(), 6);
        approx(s.total_sec, 1.0);
        let s = must_parse("{[c]2 e}4");
        assert_eq!(s.notes.len(), 3);
        approx(s.total_sec, 0.5);
        // q は縮めた長さに掛かる
        let s = must_parse("q50 {c d e}4");
        approx(s.notes[0].duration_sec, 0.5 / 3.0 / 2.0);
        approx(s.notes[1].start_sec, 0.5 / 3.0);
    }

    #[test]
    fn invalid_tuplets() {
        assert_eq!(must_fail("{c d e").position, 0); // 閉じていない
        assert_eq!(must_fail("c d e}4").position, 5); // 開いていない
        assert_eq!(must_fail("{}4 c").position, 0); // 空
        assert_eq!(must_fail("{o5}4 c").position, 0); // 音符も休符もない
        assert_eq!(must_fail("{c {d}4}4").position, 3); // 入れ子
        assert_eq!(must_fail("{c;d}4").position, 0); // `;` は連符をまたげない
        assert_eq!(must_fail("{c d}3").position, 5); // 長さが不正
        assert_eq!(must_fail("{[c}2]").position, 1); // 括弧の交差(`[` が閉じていない)
        assert_eq!(must_fail("[{c]2}").position, 1); // 括弧の交差(`{` が閉じていない)
        assert_eq!(must_fail("{c [ {d}4 ]2}4").position, 5); // ループの中でも入れ子は不可
    }

    fn timbres(text: &str) -> Vec<Timbre> {
        must_parse(text).notes.iter().map(|n| n.timbre).collect()
    }

    /// Rust 側の番号・名前の表。フロントの `lib/mmlTimbres.test.ts` の `EXPECTED` と同じ並びにすること。
    const TIMBRE_TABLE: [(u32, &str, Timbre); 14] = [
        (0, "fm", Timbre::Fm),
        (1, "square", Timbre::Square),
        (2, "pulse", Timbre::Pulse),
        (3, "triangle", Timbre::Triangle),
        (4, "saw", Timbre::Saw),
        (5, "sine", Timbre::Sine),
        (6, "organ", Timbre::Organ),
        (7, "epiano", Timbre::Epiano),
        (8, "bell", Timbre::Bell),
        (9, "am", Timbre::Am),
        (10, "lead", Timbre::Lead),
        (11, "bass", Timbre::Bass),
        (12, "drum", Timbre::Drum),
        (13, "noise", Timbre::Noise),
    ];

    #[test]
    fn timbre_defaults_to_fm() {
        assert_eq!(timbres("c d e"), vec![Timbre::Fm; 3]);
    }

    #[test]
    fn every_timbre_is_reachable_by_number_and_by_name() {
        for (n, name, t) in TIMBRE_TABLE {
            assert_eq!(Timbre::ALL[n as usize], t);
            assert_eq!(t.name(), name);
            assert_eq!(timbres(&format!("@{n} c")), vec![t], "@{n}");
            assert_eq!(timbres(&format!("@{name} c")), vec![t], "@{name}");
        }
        assert_eq!(Timbre::ALL.len(), TIMBRE_TABLE.len());
    }

    #[test]
    fn timbre_names_are_not_prefixes_of_each_other() {
        for a in Timbre::ALL {
            for b in Timbre::ALL {
                if a != b {
                    assert!(!b.name().starts_with(a.name()), "{} は {} の接頭辞", a.name(), b.name());
                }
            }
        }
    }

    #[test]
    fn timbre_serializes_as_its_mml_name() {
        for t in Timbre::ALL {
            assert_eq!(serde_json::to_value(t).unwrap(), serde_json::json!(t.name()));
        }
        let note = &must_parse("c").notes[0];
        assert_eq!(serde_json::to_value(note).unwrap()["timbre"], "fm");
    }

    #[test]
    fn timbre_name_may_be_followed_directly_by_a_command() {
        assert_eq!(timbres("@bellc"), vec![Timbre::Bell]);
        assert_eq!(timbres("@amc4"), vec![Timbre::Am]);
        assert_eq!(timbres("@sawt60c"), vec![Timbre::Saw]);
        assert_eq!(timbres("@3c"), vec![Timbre::Triangle]);
        assert_eq!(timbres("@Bell C"), vec![Timbre::Bell]);
        assert_eq!(timbres("@ bell c"), vec![Timbre::Bell]); // 空白は先に除かれる
    }

    #[test]
    fn timbre_can_change_mid_track() {
        assert_eq!(
            timbres("@1 c @bell d @0 e f"),
            vec![Timbre::Square, Timbre::Bell, Timbre::Fm, Timbre::Fm]
        );
    }

    #[test]
    fn timbre_state_is_per_track() {
        let s = must_parse("@1 c; c; @drum c");
        let by_track: Vec<(u8, Timbre)> = s.notes.iter().map(|n| (n.track, n.timbre)).collect();
        assert_eq!(
            by_track,
            vec![(0, Timbre::Square), (1, Timbre::Fm), (2, Timbre::Drum)]
        );
    }

    #[test]
    fn timbre_inside_a_loop_is_reapplied_each_round() {
        assert_eq!(
            timbres("[@1c @2c]2"),
            vec![Timbre::Square, Timbre::Pulse, Timbre::Square, Timbre::Pulse]
        );
        // 繰り返しの外には 1 周目の最後の状態が残る
        assert_eq!(timbres("[@1c]2 d"), vec![Timbre::Square, Timbre::Square, Timbre::Square]);
    }

    #[test]
    fn tied_notes_keep_the_first_notes_timbre() {
        let s = must_parse("@1 c4&@2 c4");
        assert_eq!(s.notes.len(), 1);
        assert_eq!(s.notes[0].timbre, Timbre::Square);
        approx(s.notes[0].duration_sec, 1.0);
    }

    #[test]
    fn timbre_errors_report_where_to_look() {
        assert_eq!(must_fail("@14 c").position, 1); // 範囲外は数字の先頭
        assert!(must_fail("@14 c").message.contains("0〜13"));
        assert_eq!(must_fail("@xyz c").position, 1); // 不明な名前は `@` の次
        assert!(must_fail("@xyz c").message.contains("不明な音色"));
        assert_eq!(must_fail("@bel c").position, 1); // 名前の途中までは一致しない
        assert_eq!(must_fail("@c").position, 1); // 音符名は音色名ではない
        assert_eq!(must_fail("c @").position, 2); // 末尾の `@`
        assert_eq!(must_fail("@").position, 0);
        assert_eq!(must_fail("c; @").position, 3); // トラック末尾の `@`
        assert_eq!(must_fail("＠bell c").position, 0); // 全角の `＠` は不明な文字
        assert_eq!(must_fail("@3").position, 0); // 音符が無い(既存の規則)
    }

    #[test]
    fn shared_tempo_applies_to_every_track() {
        let s = must_parse("t=60 c4 ; c4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[1].duration_sec, 1.0);
        // 2 本目に書いても、全トラックに効く
        let s = must_parse("c4 ; t=60 c4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[1].duration_sec, 1.0);
        // 書かなければ 120(今までどおり)
        let s = must_parse("c4 ; c4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 0.5);
        approx(must_parse("t=240 c4").notes[0].duration_sec, 0.25);
        approx(must_parse("t=32 c4").notes[0].duration_sec, 60.0 / 32.0);
    }

    #[test]
    fn track_tempo_overrides_the_shared_tempo() {
        // 先頭の `t120`: そのトラックは最初から 120、他のトラックは共有の 60
        let s = must_parse("t=60 t120 c4 ; c4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 1.0);
        // 途中の `t120`: そのトラックは、そこから 120
        let s = must_parse("t=60 c4 t120 c4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[1].start_sec, 1.0);
        approx(s.notes[1].duration_sec, 0.5);
        // `t=` を 2 本目に書いて、1 本目が `t120` で上書きする
        let s = must_parse("t120 c4 ; t=60 c4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 1.0);
    }

    #[test]
    fn shared_tempo_notation_ignores_case_and_spaces() {
        assert_eq!(must_parse("T = 60 C4"), must_parse("t=60 c4"));
        // 他のコマンドの前でも後ろ(最初の音符より前)でも同じ
        assert_eq!(must_parse("o5 l8 t=60 c"), must_parse("t=60 o5 l8 c"));
    }

    #[test]
    fn a_track_with_only_the_shared_tempo_still_counts_as_a_track() {
        let s = must_parse("t=60 ; c4 ; d4");
        assert_eq!(s.track_count, 3);
        assert_eq!(s.notes[0].track, 1);
        approx(s.notes[0].duration_sec, 1.0);
        assert_eq!(must_fail("t=60 ; c ; d ; e ; f").position, 17); // 5 本目の手前の `;`
    }

    #[test]
    fn shared_tempo_combines_with_other_syntax() {
        let s = must_parse("t=60 {c d e}4 ; q50 c4^4");
        assert_eq!(s.notes.len(), 4);
        approx(s.total_sec, 2.0); // 1 本目は 1.0 秒、2 本目は 2.0 秒
        approx(s.notes[3].duration_sec, 1.0); // q50 は全長 2.0 秒の半分
    }

    #[test]
    fn invalid_shared_tempo() {
        assert_eq!(must_fail("t= c").position, 0); // 数字が無い
        assert_eq!(must_fail("t=31 c").position, 2); // 範囲外(数字の位置)
        assert_eq!(must_fail("t=256 c").position, 2);
        assert_eq!(must_fail("c t=140").position, 2); // 音符の後
        assert_eq!(must_fail("r4 t=140 c").position, 3); // 休符の後
        assert_eq!(must_fail("c ; d t=140").position, 6); // 2 本目の音符の後
        assert_eq!(must_fail("[t=140 c]2").position, 1); // 繰り返しの中
        assert_eq!(must_fail("{t=140 c}4").position, 1); // 連符の中
        assert_eq!(must_fail("t=140 t=100 c").position, 6); // 同じトラックに 2 個
        assert_eq!(must_fail("t=140 c ; t=100 d").position, 10); // 別のトラックに 2 個
        assert_eq!(must_fail("t=140").position, 0); // 音符が無い(今までと同じ)
    }

    #[test]
    fn a_later_tracks_syntax_error_comes_before_an_earlier_tracks_run_error() {
        // 全トラックを先に構文解析するので、後ろのトラックの構文エラーが先に出る(エラーが 2 つあるときだけ)
        let e = must_fail("t32 [c1]99 ; x"); // 1 本目は 90 秒を超える
        assert_eq!(e.position, 13);
        assert!(e.message.contains("不明な文字"), "{}", e.message);
        // エラーが 1 つなら、今までどおり実行時のエラー
        let e = must_fail("t32 [c1]99");
        assert!(e.message.contains("90"), "{}", e.message);
    }

    #[test]
    fn four_tracks_at_the_expansion_cap_parse_quickly() {
        // 各トラックが約 49.5 万命令に展開される(上限 50 万の内側)。全トラックを先に構文解析するので、
        // 4 本分の命令が同時にメモリに載る。時間だけを見る(メモリのピークは測っていない)。
        let track = format!("c[[{}]99]10", "><".repeat(250));
        let text = format!("t=120 {track};{track};{track};{track}");
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        let started = std::time::Instant::now();
        let s = must_parse(&text);
        assert_eq!(s.track_count, 4);
        assert_eq!(s.notes.len(), 4);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "parse が {:?} かかった",
            started.elapsed()
        );
    }

    #[test]
    fn many_shared_tempos_after_a_big_expansion_do_not_rescan_the_track() {
        // 音符の無い展開(約 49 万命令)の後に `t=32` を並べる。2 個目以降の `t=` は `shared_tempo` まで
        // 弾かれないので、`t=` のたびに命令列をなめ直すと、1246 回 × 約 49 万件の走査になる。
        let big = "[[[><]99]99]25";
        let text = format!("{big}{}", "t=32".repeat((MAX_TEXT_CHARS - big.len()) / 4));
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        let started = std::time::Instant::now();
        let e = must_fail(&text);
        assert!(e.message.contains("1 回だけ"), "{}", e.message);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "parse が {:?} かかった",
            started.elapsed()
        );
    }

    #[test]
    fn chord_plays_all_notes_together() {
        let s = must_parse("'ceg'");
        assert_eq!(s.notes.len(), 3);
        let midis: Vec<u8> = s.notes.iter().map(|n| n.midi).collect();
        assert_eq!(midis, vec![60, 64, 67]); // 書いた順
        for n in &s.notes {
            approx(n.start_sec, 0.0);
            approx(n.duration_sec, 0.5);
        }
        approx(s.total_sec, 0.5);
        // 長さと付点は、閉じの `'` の後ろ。省略は `l`
        approx(must_parse("'ceg'4.").notes[0].duration_sec, 0.75);
        approx(must_parse("l8 'ce'").notes[0].duration_sec, 0.25);
        // 後ろの音符は、和音の長さだけ後
        let s = must_parse("'ceg'4 d4");
        approx(s.notes[3].start_sec, 0.5);
        // 大文字・空白
        assert_eq!(must_parse("' C E G '4"), must_parse("'ceg'4"));
        // 1 音だけの和音も、同じ音を重ねた和音も書ける
        assert_eq!(must_parse("'c'").notes.len(), 1);
        assert_eq!(must_parse("'cc'").notes.len(), 2);
    }

    #[test]
    fn chord_velocity_is_divided_by_the_number_of_notes() {
        approx(must_parse("v15 'ce'").notes[0].velocity, 0.5);
        approx(must_parse("v15 'ceg'").notes[2].velocity, 1.0 / 3.0);
        approx(must_parse("v15 'c'").notes[0].velocity, 1.0); // 1 音は割らない
        approx(must_parse("v15 c").notes[0].velocity, 1.0);
    }

    #[test]
    fn chord_accidentals_and_octaves() {
        let midis = |t: &str| -> Vec<u8> { must_parse(t).notes.iter().map(|n| n.midi).collect() };
        assert_eq!(midis("'c#e-g+'"), vec![61, 63, 68]);
        assert_eq!(midis("o4 'c>c'"), vec![60, 72]);
        assert_eq!(midis("o4 'c<c'"), vec![60, 48]);
        // 和音の後ろは、元のオクターブに戻る
        assert_eq!(midis("o4 'c>c' d"), vec![60, 72, 62]);
        assert_eq!(midis("o4 'c>>c' d"), vec![60, 84, 62]);
        // 和音の外の `>` は、今までどおり残る
        assert_eq!(midis("'c' > d"), vec![60, 74]);
    }

    #[test]
    fn caret_extends_every_note_of_a_chord() {
        let s = must_parse("'ceg'4^4");
        assert_eq!(s.notes.len(), 3);
        for n in &s.notes {
            approx(n.duration_sec, 1.0);
        }
        approx(s.total_sec, 1.0);
        // q は全部の音に効き、延ばした全長に掛かる
        let s = must_parse("q50 'ce'4^4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[1].duration_sec, 0.5);
        let s = must_parse("q50 'ce'4 d4");
        approx(s.notes[0].duration_sec, 0.25);
        approx(s.notes[2].start_sec, 0.5); // 時間は全長だけ進む
    }

    #[test]
    fn chord_combines_with_tuplets_loops_and_shared_tempo() {
        // 連符の中の和音は、1 つの長さとして数える
        let s = must_parse("{'ce' d e}4");
        assert_eq!(s.notes.len(), 4);
        approx(s.notes[0].duration_sec, 0.5 / 3.0);
        approx(s.notes[2].start_sec, 0.5 / 3.0);
        approx(s.total_sec, 0.5);
        // 和音だけの連符
        approx(must_parse("{'ce'}4").total_sec, 0.5);
        // ループ
        let s = must_parse("['ce']2");
        assert_eq!(s.notes.len(), 4);
        approx(s.notes[2].start_sec, 0.5);
        // 共有テンポ
        let s = must_parse("t=60 'ce'4 ; 'g'4");
        approx(s.notes[0].duration_sec, 1.0);
        approx(s.notes[2].duration_sec, 1.0);
    }

    #[test]
    fn chord_counts_each_note_toward_the_limit() {
        let ok = format!("'{}'", "c".repeat(MAX_NOTES));
        assert_eq!(must_parse(&ok).notes.len(), MAX_NOTES);
        let over = format!("'{}'", "c".repeat(MAX_NOTES + 1));
        // 超えた 3001 個目の音の位置でエラー
        assert_eq!(must_fail(&over).position as usize, MAX_NOTES + 1);
        // ループで超える
        let looped = format!("['{}']99", "c".repeat(100));
        let started = std::time::Instant::now();
        let e = must_fail(&looped);
        assert!(e.message.contains("3000"), "{}", e.message);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn invalid_chords() {
        assert_eq!(must_fail("'ce").position, 0); // 閉じていない
        assert_eq!(must_fail("''").position, 0); // 空
        assert_eq!(must_fail("'cr'").position, 2); // 中の休符
        assert_eq!(must_fail("'c[e]2'").position, 2); // 中の `[`
        assert_eq!(must_fail("c 'c4e'").position, 4); // 中の数字
        assert_eq!(must_fail("'ceg'3").position, 5); // 長さが不正(今までと同じ)
        assert_eq!(must_fail("c&'ceg'").position, 1); // `&` の後の和音
        assert_eq!(must_fail("'ceg'&c").position, 5); // 和音の後の `&`
        assert_eq!(must_fail("'c'&c").position, 3); // 1 音の和音でも同じ
        assert_eq!(must_fail("o8 'c>c'").position, 5); // オクターブの範囲外(`>` の位置)
        assert_eq!(must_fail("o1 '<c'").position, 4); // 同じく `<`
        assert_eq!(must_fail("'ce;g'").position, 0); // `;` でトラックが分かれる
        assert_eq!(must_fail("'ce' t=60").position, 5); // `t=` は最初の音符・和音より前
    }

    #[test]
    fn chord_positions_stay_in_the_original_text() {
        // コメントと空白を挟んでも、位置は元のテキストのまま
        assert_eq!(must_fail("/* あ */ 'c x e'").position, 11); // `x` は 12 文字目
        assert_eq!(must_parse("'c/*x*/e'"), must_parse("'ce'"));
        assert_eq!(must_parse("' c e g '"), must_parse("'ceg'"));
    }

    #[test]
    fn a_rest_after_a_chord_forgets_it() {
        // 休符の後の `^` は、今までどおりエラー
        assert_eq!(must_fail("'ce' r ^4").position, 7);
        // 和音の後に普通の音符が来ても、`^` はその音符だけを延ばす
        let s = must_parse("'ce' d ^4");
        approx(s.notes[0].duration_sec, 0.5);
        approx(s.notes[2].duration_sec, 1.0);
    }

    #[test]
    fn many_carets_after_a_big_chord_do_not_rewrite_every_note() {
        // 3000 音の和音の後ろに、連符で縮めた `^` を約 49 万個並べる(連符の中なので 90 秒の上限には届かない)。
        // `^` のたびに和音の全部の音を書き直すと、約 15 億回の書き込みになる。
        let text = format!("'{}'{{[[[^]99]99]50}}", "c".repeat(MAX_NOTES));
        assert!(text.chars().count() <= MAX_TEXT_CHARS);
        let started = std::time::Instant::now();
        let s = must_parse(&text);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "parse が {:?} かかった",
            started.elapsed()
        );
        // 和音の長さ 0.5 秒に、連符の長さ(四分音符 1 つ分 = 0.5 秒)を足した全長が、全部の音に入っている
        assert_eq!(s.notes.len(), MAX_NOTES);
        for n in [&s.notes[0], &s.notes[MAX_NOTES - 1]] {
            assert!((n.duration_sec - 1.0).abs() < 1e-6, "{}", n.duration_sec);
        }
    }

    #[test]
    fn a_later_tracks_track_limit_error_comes_before_an_earlier_tracks_run_error() {
        // 全トラックを先に構文解析するので、5 本目のトラック数のエラーも、1 本目の 90 秒超過より先に出る
        let e = must_fail("t32 [c1]99 ; c ; d ; e ; f");
        assert_eq!(e.position, 23); // 5 本目の手前の `;`
        assert!(e.message.contains("4 つまで"), "{}", e.message);
    }

    #[test]
    fn shared_tempo_after_a_chord_is_rejected_and_the_message_names_chords() {
        let e = must_fail("'ce' t=60");
        assert_eq!(e.position, 5);
        assert!(e.message.contains("和音"), "{}", e.message);
    }

    #[test]
    fn the_octave_is_restored_after_a_chord_inside_loops_and_tuplets() {
        let midis = |t: &str| -> Vec<u8> { must_parse(t).notes.iter().map(|n| n.midi).collect() };
        assert_eq!(midis("['c>c' c]2"), vec![60, 72, 60, 60, 72, 60]);
        assert_eq!(midis("{'c>c' c}4 c"), vec![60, 72, 60, 60]);
        assert_eq!(midis("o4 'c>>>c' c"), vec![60, 96, 60]);
        assert_eq!(midis("o7 '>c' > c"), vec![108, 108]);
    }

    #[test]
    fn a_leading_caret_is_reported_as_the_caret_not_as_the_shared_tempo() {
        // `^` はトラックの最初には置けないので、`t=` の位置の検査には数えない(本当の原因の `^` を指す)
        let e = must_fail("^4 t=60");
        assert_eq!(e.position, 0);
        assert!(e.message.contains("`^`"), "{}", e.message);
        // 音符の後の `^` を挟んだ `t=` は、今までどおり `t=` の位置のエラー
        assert_eq!(must_fail("c ^4 t=60").position, 5);
        // 連符の中身の検査は、今までどおり `^` を数える(`c {^4}4` は通る)
        approx(must_parse("c {^4}4").notes[0].duration_sec, 1.0);
    }

    #[test]
    fn macros_define_and_expand() {
        assert_eq!(
            must_parse("$riff={c d e f} $riff $riff"),
            must_parse("cdefcdef")
        );
        // 定義は、使う前でも後ろでもよい
        assert_eq!(must_parse("$riff $riff={c d}"), must_parse("cd"));
        // 入れ子
        assert_eq!(must_parse("$a={$b $b} $b={c} $a"), must_parse("cc"));
        // 空白・大文字、定義の `=` と `{` の前後の空白
        assert_eq!(must_parse("$RIFF = { C D } $riff"), must_parse("cd"));
        // 展開の直後の `4` は、本体の最後の音符の長さ(文字の置き換えなので)
        assert_eq!(
            must_parse("$riff={c d e f} $riff 4"),
            must_parse("c d e f4")
        );
        // 本体の中の波括弧(連符)は、対で数える
        assert_eq!(must_parse("$u={{c d e}4} $u"), must_parse("{c d e}4"));
        // 本体に、状態を変えるコマンド・ループ・和音を書ける
        assert_eq!(
            must_parse("$p={o5 l8 [c d]2} $p"),
            must_parse("o5 l8 [c d]2")
        );
        assert_eq!(must_parse("$q={'ceg'4} $q"), must_parse("'ceg'4"));
        // 本体の中から、別のマクロを使える(使いの直後が空白でも `}` でもよい)
        assert_eq!(must_parse("$a={c} $b={$a d} $b"), must_parse("cd"));
        // 同じマクロを、複数のトラックで使える
        let s = must_parse("$m={c d} $m ; $m");
        assert_eq!(s.track_count, 2);
        assert_eq!(s.notes.len(), 4);
    }

    #[test]
    fn macro_names_end_at_the_first_non_name_character() {
        // 空白(かコメント)で区切る。記号の前は、区切らなくてよい
        assert_eq!(must_parse("$a={c} $a d"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a/**/d"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a\nd"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a\td"), must_parse("cd"));
        assert_eq!(must_parse("$a={c} $a;d"), must_parse("c;d"));
        assert_eq!(must_parse("$a={c} [$a]2"), must_parse("[c]2"));
        // `$a` と `$ab` は別のマクロ
        assert_eq!(must_parse("$a={c} $ab={d} $a $ab"), must_parse("cd"));
        // `$a t=60` は `$a` の展開と `t=60`。`$at=60` は名前 `at`
        assert_eq!(must_parse("$a={o5} $a t=60 c"), must_parse("o5 t=60 c"));
        let e = must_fail("$a={o5} $at=60 c");
        assert_eq!(e.position, 8);
        assert!(
            e.message.contains("`$a`") && e.message.contains("`t`"),
            "{}",
            e.message
        );
        // 区切らないと名前の一部になる
        let e = must_fail("$a={c} $ad");
        assert_eq!(e.position, 7);
        assert!(e.message.contains("空白で区切って"), "{}", e.message);
    }

    #[test]
    fn definition_only_sections_are_not_tracks() {
        let s = must_parse("$a={c} ; d");
        assert_eq!(s.track_count, 1);
        assert_eq!(s.notes.len(), 1);
        assert_eq!(must_parse("$a={c} $a ; $a").track_count, 2);
        // 本体の `t=` は、展開された場所の規則で検査される
        assert_eq!(must_parse("$t={t=60} $t c ; d"), must_parse("t=60 c ; d"));
    }

    #[test]
    fn invalid_macros() {
        let pos = |t: &str| must_fail(t).position;
        assert_eq!(pos("c $zzz"), 2); // 未定義
        assert_eq!(pos("c $"), 2); // 名前が無い
        assert_eq!(pos("$a=c"), 0); // `={` の形でなければ定義ではなく、未定義の展開
        assert_eq!(pos("$a={c d"), 3); // 閉じていない
        assert_eq!(pos("$a={c;d} $a"), 5); // 本体の `;`
        assert_eq!(pos("$a={$b={c}}"), 4); // 本体の中の定義
        assert_eq!(pos("$a={c} $a={d}"), 7); // 二重定義
        assert_eq!(pos("$a={$a} $a"), 4); // 再帰(本体の中の `$`)
        assert_eq!(pos("$a={$b} $b={$a} $a"), 12); // 間接の再帰
        assert_eq!(pos("$a={o9} $a"), 5); // 本体の誤りは、定義の中の文字を指す
        assert_eq!(pos("/* あ */ $zzz"), 8); // コメントの後ろでも、元の位置
        assert!(must_fail("$a={c} $ab").message.contains("空白で区切って"));
        assert!(must_fail("c $zzz").message.contains("未定義"));
        assert!(must_fail("$a={$a} $a").message.contains("自分自身"));
        assert!(must_fail("c $").message.contains("名前"));
    }

    #[test]
    fn macro_expansion_is_capped_and_deep_chains_work() {
        // 1000 文字の本体を 101 回並べたマクロ: 展開後 10 万文字を超える
        let big = format!(
            "$a={{{}}} $b={{{}}} $b",
            "c".repeat(1000),
            "$a ".repeat(101)
        );
        assert!(big.chars().count() <= MAX_TEXT_CHARS);
        let started = std::time::Instant::now();
        let e = must_fail(&big);
        assert!(e.message.contains("100000"), "{}", e.message);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        // 入れ子が深い連鎖(300 段)は、スタックを使い切らずに展開できる
        let mut chain = String::from("$m0={c} ");
        for i in 1..300 {
            chain.push_str(&format!("$m{i}={{$m{} }} ", i - 1));
        }
        chain.push_str("$m299");
        assert!(chain.chars().count() <= MAX_TEXT_CHARS);
        assert_eq!(must_parse(&chain), must_parse("c"));
    }
}
