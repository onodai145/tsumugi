# MML の音色パラメーター y m k Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に、トラックの状態としてのエンベロープ `y a,d,s,r`、ビブラート `m 速さ,深さ`、デチューン `k セント` を足し、旋律の音色に効かせる。

**Architecture:** Rust のパーサーが `y` `m` `k` をトラックの状態として持ち、音符ごとに `NoteEvent.params`(すべて既定なら `null`)に載せる。フロントの描画は、(トラック, 音色, パラメーターの組)ごとに別の `PolySynth` を作り、作った直後に `set` でエンベロープとデチューンを反映し、ビブラートは `Tone.Vibrato` を出力に挟む。打楽器(`drum` `noise` `drumkit`)は `params` を無視する。

**Tech Stack:** Rust(`mml.rs`、`domain/mml.rs`)、Svelte 5 + TypeScript + Vitest、Tone.js 15.1.22(`PolySynth.set`、`Vibrato`)。

**Spec:** `docs/superpowers/specs/2026-10-30-issue-499-mml-timbre-params-design.md`(承認済み。矛盾したらこちらが優先)

## Global Constraints

- 全ユーザー向けの文言(エラー、ヘルプ、ガイド)は日本語。
- 記法(spec のとおり):
  - `y a,d,s,r`: アタック 0〜2000 ミリ秒、ディケイ 0〜2000 ミリ秒、サステイン 0〜100、リリース 0〜600 ミリ秒。4 つすべて必要。`y0`(`0` 1 つ)で解除。
  - `m 速さ,深さ`: 速さ 1〜20(Hz)、深さ 1〜100。`m0` で解除。
  - `k セント`: −100〜100(`k+12` `k-12` と書ける)。`k0` で解除。
  - トラックの状態として、書いた所から後の音に効く(トラックごとに独立)。和音の中には書けない(既存のエラー「和音の中には音符と `<` `>` だけ書けます」のまま、位置はその文字)。範囲外は数字の先頭、形の誤りは「次の区切りの位置」を指す。
- `NoteEvent` に `params: Option<TimbreParams>` を足す(`TimbreParams { envelope: Option<EnvelopeParams>, vibrato: Option<VibratoParams>, detune: i16 }`。すべて既定なら `None`)。`MmlScore` / `MmlError` の形は変えない。`frontend/src/bindings/tauri.gen.ts` は手で編集せず、`cargo test` の `generates_frontend_bindings` で再生成する。
- 保存済みの MML(`y` `m` `k` を含まない)の結果は変えない(`params` が `null` になるだけ)。打楽器(`drum` `noise` `drumkit`)の音は変えない。
- `y` はエンベロープ(`envelope`)だけを置き換え、FM・AM の `modulationEnvelope` は変えない。`lead` が持つ組み込みのビブラートは残す。
- コミットメッセージは 1 行だけ(本文なし)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別に付ける。`git commit` が失敗・タイムアウトしたら止めて報告し、再試行や `--no-gpg-sign` / `--no-verify` はしない。
- `rustfmt --check --edition 2021 src/mml.rs` の差分の数は 4 を超えない(`main` は 4)。新しいコードは整形済みの形で書く。
- 音が実際に鳴ること(聞こえ方)は確認できない(音声デバイスなし)。そのことを検証の記録に書く。実画面の確認は隔離した Xvfb で行い、起動したプロセスは、環境変数のトークンで特定した PID だけを止める(`pkill` / `killall` は使わない)。

## Review Focus

仕様が黙っているが、使う人が踏みやすい入力と条件。それぞれ、持ち主のタスクにテストを足してある。

1. 引数の数と `0` の扱い: `y` の後に数字がない、`y10`(`0` 以外の 1 つ)、`y10,200,60`(3 つ)、`y0,200,60,300`(先頭が `0` でカンマが続く = 解除ではなく 4 つの数)、`m0,5`(速さ 0 は範囲外)、`k` の後に数字がない、`k-` `k+` だけ、`k+101`。数の途中の空白(`y10, 200`)。(Task 1)
2. `@drum` との読み分け: `@drumk0 c`(= `@drum` + `k0`)、`@drumk c`(位置 5 のエラー)、`@drumkit c`(最長一致のまま)。(Task 1)
3. 状態の扱い: トラックごとに独立、ループの中、マクロ `$` の中、タイ `&` で延びた音(先頭の音の `params`)、和音(和音の前の状態)、`@` で音色を切り替えても保たれること、`y0` `m0` `k0` で既定に戻ること。(Task 1)
4. 互換: `y` `m` `k` を含まない MML の `NoteEvent` が、`params == None` 以外は今までと同じ。`NoteEvent` の JSON に `params: null`(camelCase のキー)。(Task 1)
5. 描画のパラメーターの組ごとの音源: 組が同じなら使い回し、違えば別の音源。打楽器は `params` を無視して使い回す。デチューン・エンベロープは `set`、ビブラートは `Vibrato` を出力に挟み、`toDestination` を二重に繋がない。(Task 2)
6. 実際の Tone で効くこと: エンベロープ・デチューン・ビブラートが、実際の描画で音の波形を変える(同じ MML でパラメーターの有無を比べる)。`PolySynth.set` が、作った後に鳴らす音に反映されること。(Task 4)
7. 極端な値の音量: `y0,0,100,0`(サステイン 100)・`y2000,0,100,600` などで、全旋律音色のピークが 0.36 以下。リリース 600 ミリ秒でも、最後の音の余韻が切れないこと。(Task 4)
8. パラメーターを音ごとに変える MML(201 通りのデチューン)の描画時間。(Task 4)

---

### Task 1: Rust — `y` `m` `k` のパースと `NoteEvent.params`

**Files:**
- Modify: `src-tauri/src/domain/mml.rs`(型と `NoteEvent`)、`src-tauri/src/domain/mod.rs`(再エクスポート)
- Modify: `src-tauri/src/mml.rs`(`Op`、パース、`TrackState`、`run_track`、`mod tests`)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`(`cargo test` が書き換える。手で編集しない)

**Interfaces:**
- Consumes: 既存の `Cursor`(`peek` / `pos` / `bump` / `number` / `require_number`)、`mml_err`、`Op`、`TrackState`、`run_track`、`must_parse` / `must_fail` / `timbres`(テスト)。
- Produces: `EnvelopeParams { attack_ms: u32, decay_ms: u32, sustain: u8, release_ms: u32 }`、`VibratoParams { rate_hz: u8, depth: u8 }`、`TimbreParams { envelope, vibrato, detune: i16 }`、`NoteEvent.params`。TS 側では、`TimbreParams`(`envelope: EnvelopeParams | null`、`vibrato: VibratoParams | null`、`detune: number`)と `NoteEvent.params: TimbreParams | null`(Task 2 が使う)。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/mml.rs` の `mod tests` の末尾(最後の `}` の前)に追加する:

```rust

    fn params_of(text: &str) -> Vec<Option<TimbreParams>> {
        must_parse(text).notes.iter().map(|n| n.params).collect()
    }

    fn envelope(a: u32, d: u32, s: u8, r: u32) -> EnvelopeParams {
        EnvelopeParams {
            attack_ms: a,
            decay_ms: d,
            sustain: s,
            release_ms: r,
        }
    }

    #[test]
    fn params_are_none_unless_written() {
        assert_eq!(params_of("c d e"), vec![None; 3]);
        assert_eq!(params_of("y0 m0 k0 c"), vec![None]);
    }

    #[test]
    fn envelope_is_read_and_applies_to_following_notes() {
        let want = Some(TimbreParams {
            envelope: Some(envelope(10, 200, 60, 300)),
            vibrato: None,
            detune: 0,
        });
        assert_eq!(params_of("c y10,200,60,300 d e"), vec![None, want, want]);
        assert_eq!(params_of("Y10, 200 ,60,300 d")[0], want);
        // 先頭が 0 でカンマが続くときは、解除ではなく 4 つの数
        let zero = Some(TimbreParams {
            envelope: Some(envelope(0, 200, 60, 300)),
            vibrato: None,
            detune: 0,
        });
        assert_eq!(params_of("y0,200,60,300 c"), vec![zero]);
        // 境界値
        assert!(params_of("y2000,2000,100,600 c")[0].is_some());
        assert!(params_of("y0,0,0,0 c")[0].is_some());
        // 解除
        assert_eq!(params_of("y10,200,60,300 y0 c"), vec![None]);
    }

    #[test]
    fn envelope_errors_report_where_to_look() {
        assert_eq!(must_fail("y c").position, 0); // 数字がない
        assert!(must_fail("y c").message.contains("数値"));
        assert_eq!(must_fail("y10 c").position, 4); // 0 以外の 1 つ: 次の文字
        assert!(must_fail("y10 c").message.contains("y0"));
        assert_eq!(must_fail("y10,200,60 c").position, 11); // 4 つに足りない
        assert_eq!(must_fail("y10,200, c").position, 9); // カンマの後に数字がない(次の文字)
        assert_eq!(must_fail("y2001,0,0,0 c").position, 1); // 範囲外は数字の先頭
        assert!(must_fail("y2001,0,0,0 c").message.contains("アタック"));
        assert_eq!(must_fail("y0,2001,0,0 c").position, 3);
        assert!(must_fail("y0,2001,0,0 c").message.contains("ディケイ"));
        assert_eq!(must_fail("y0,0,101,0 c").position, 5);
        assert!(must_fail("y0,0,101,0 c").message.contains("サステイン"));
        assert_eq!(must_fail("y0,0,0,601 c").position, 7);
        assert!(must_fail("y0,0,0,601 c").message.contains("リリース"));
    }

    #[test]
    fn vibrato_is_read_and_errors_report_where_to_look() {
        let want = Some(TimbreParams {
            envelope: None,
            vibrato: Some(VibratoParams { rate_hz: 5, depth: 30 }),
            detune: 0,
        });
        assert_eq!(params_of("m5,30 c m0 d"), vec![want, None]);
        assert!(params_of("m20,100 c")[0].is_some());
        assert!(params_of("m1,1 c")[0].is_some());
        assert_eq!(must_fail("m c").position, 0);
        assert_eq!(must_fail("m5 c").position, 3); // 0 以外の 1 つ(次の文字)
        assert!(must_fail("m5 c").message.contains("m0"));
        assert_eq!(must_fail("m21,1 c").position, 1);
        assert!(must_fail("m21,1 c").message.contains("速さ"));
        assert_eq!(must_fail("m0,5 c").position, 1); // 速さ 0 は範囲外
        assert_eq!(must_fail("m5,101 c").position, 3);
        assert!(must_fail("m5,101 c").message.contains("深さ"));
        assert_eq!(must_fail("m5,0 c").position, 3); // 深さ 0 は範囲外
    }

    #[test]
    fn detune_is_read_with_an_optional_sign() {
        let detune = |text: &str| params_of(text)[0].map(|p| p.detune);
        assert_eq!(detune("k12 c"), Some(12));
        assert_eq!(detune("k+12 c"), Some(12));
        assert_eq!(detune("k-12 c"), Some(-12));
        assert_eq!(detune("k100 c"), Some(100));
        assert_eq!(detune("k-100 c"), Some(-100));
        assert_eq!(detune("k12 k0 c"), None);
        assert_eq!(must_fail("k c").position, 0);
        assert!(must_fail("k c").message.contains("数値"));
        assert_eq!(must_fail("k- c").position, 0); // 符号だけ
        assert_eq!(must_fail("k+ c").position, 0);
        assert_eq!(must_fail("k101 c").position, 1);
        assert_eq!(must_fail("k+101 c").position, 2); // 符号の次の数字
        assert_eq!(must_fail("k-101 c").position, 2);
    }

    #[test]
    fn params_combine_and_follow_track_state_rules() {
        let all = Some(TimbreParams {
            envelope: Some(envelope(1, 2, 3, 4)),
            vibrato: Some(VibratoParams { rate_hz: 5, depth: 6 }),
            detune: 7,
        });
        assert_eq!(params_of("y1,2,3,4 m5,6 k7 c"), vec![all]);
        // トラックごとに独立
        assert_eq!(params_of("y1,2,3,4 c; c")[1], None);
        // 音色を切り替えても保たれる
        assert_eq!(params_of("y1,2,3,4 c @1 d")[0], params_of("y1,2,3,4 c @1 d")[1]);
        assert!(params_of("y1,2,3,4 c @1 d")[1].is_some());
        // 繰り返しの中、マクロの中
        assert_eq!(params_of("[y1,2,3,4 c y0 c]2"), vec![params_of("y1,2,3,4 c")[0], None, params_of("y1,2,3,4 c")[0], None]);
        assert!(params_of("$f={y1,2,3,4} $f c")[0].is_some());
        // タイでつないだ音は 1 つの音(先頭の音の params)
        let tied = must_parse("y1,2,3,4 c&c");
        assert_eq!(tied.notes.len(), 1);
        assert!(tied.notes[0].params.is_some());
        // 和音の各音は、和音の前の状態
        let chord = params_of("y1,2,3,4 'ce'");
        assert_eq!(chord.len(), 2);
        assert!(chord.iter().all(Option::is_some));
    }

    #[test]
    fn params_are_rejected_inside_a_chord() {
        for text in ["'cy1,2,3,4'4", "'cm5,6'4", "'ck12'4"] {
            let e = must_fail(text);
            assert_eq!(e.position, 2, "{text}");
            assert!(e.message.contains("和音の中"), "{text}: {}", e.message);
        }
    }

    #[test]
    fn drum_followed_by_detune_is_still_distinct_from_drumkit() {
        assert_eq!(timbres("@drumk0 c"), vec![Timbre::Drum]); // `@drum` + `k0`
        assert_eq!(timbres("@drumkit c"), vec![Timbre::Drumkit]);
        assert_eq!(timbres("@drumk12 c"), vec![Timbre::Drum]);
        assert_eq!(must_fail("@drumk c").position, 5); // `k` の後に数字がない
    }

    #[test]
    fn note_event_serializes_params_as_camel_case_or_null() {
        let note = &must_parse("c").notes[0];
        assert_eq!(serde_json::to_value(note).unwrap()["params"], serde_json::Value::Null);
        let note = &must_parse("y10,200,60,300 m5,30 k-12 c").notes[0];
        assert_eq!(
            serde_json::to_value(note).unwrap()["params"],
            serde_json::json!({
                "envelope": { "attackMs": 10, "decayMs": 200, "sustain": 60, "releaseMs": 300 },
                "vibrato": { "rateHz": 5, "depth": 30 },
                "detune": -12
            })
        );
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | grep -E "^error|cannot find|no field|test result" | head -5`
Expected: `cannot find type 'TimbreParams'` / `no field 'params'` などのコンパイルエラー(テストはまだ動かない)。

- [ ] **Step 3: 実装する**

(a) `src-tauri/src/domain/mml.rs` の `NoteEvent` の前に、型を足す:

```rust
/// 音色のパラメーター(Issue #499)。MML の `y a,d,s,r` のエンベロープ。時間はミリ秒、`sustain` は 0〜100(%)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnvelopeParams {
    pub attack_ms: u32,
    pub decay_ms: u32,
    pub sustain: u8,
    pub release_ms: u32,
}

/// `m 速さ,深さ` のビブラート。`rate_hz` は 1〜20(Hz)、`depth` は 1〜100(相対値)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VibratoParams {
    pub rate_hz: u8,
    pub depth: u8,
}

/// 音に付くパラメーター。`y` `m` `k` のどれも書かれていなければ、`NoteEvent.params` は `None`。
/// `detune` は `k` のセント(−100〜100。0 は指定なし)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TimbreParams {
    pub envelope: Option<EnvelopeParams>,
    pub vibrato: Option<VibratoParams>,
    pub detune: i16,
}

```

`NoteEvent` の最後の項目の後に足す:

```rust
    /// `y` `m` `k` のパラメーター(どれも書かれていなければ `None`)
    pub params: Option<TimbreParams>,
```

`src-tauri/src/domain/mod.rs` の `pub use mml::{MmlError, MmlParseResult, MmlScore, NoteEvent, Timbre};` を置き換える:

```rust
pub use mml::{
    EnvelopeParams, MmlError, MmlParseResult, MmlScore, NoteEvent, Timbre, TimbreParams,
    VibratoParams,
};
```

(b) `src-tauri/src/mml.rs` を編集する。

- `use crate::domain::{MmlError, MmlScore, NoteEvent, Timbre};` を置き換える:

```rust
use crate::domain::{
    EnvelopeParams, MmlError, MmlScore, NoteEvent, Timbre, TimbreParams, VibratoParams,
};
```

- `enum Op` の `Gate(u8),` の次に追加する:

```rust
    /// `y a,d,s,r`(`None` は `y0` の解除)。
    Envelope(Option<EnvelopeParams>),
    /// `m 速さ,深さ`(`None` は `m0` の解除)。
    Vibrato(Option<VibratoParams>),
    /// `k セント`(`0` は解除)。
    Detune(i16),
```

- `impl Cursor<'_>` の `require_number` の次に、カンマ区切りの数を読む関数を足す:

```rust

    /// `y a,d,s,r` / `m 速さ,深さ` の、カンマで区切った `count` 個の数。最初の数は読み済みで、その (値, 先頭の位置) を渡す。
    /// 足りないときは、足りなくなった位置(次の文字)を `usage` のメッセージで指す。
    fn comma_numbers(
        &mut self,
        first: (u32, usize),
        count: usize,
        usage: &str,
    ) -> Result<Vec<(u32, usize)>, MmlError> {
        let mut out = vec![first];
        while out.len() < count {
            if self.peek() != Some(',') {
                return Err(mml_err(self.pos(), usage));
            }
            self.bump();
            let Some(n) = self.number() else {
                return Err(mml_err(self.pos(), usage));
            };
            out.push(n);
        }
        Ok(out)
    }
```

- `parse_track` のコマンドの `match` で、`'q' => { … }` の腕の次(`'@' => {` の前)に、3 つの腕を足す:

```rust
            'y' => {
                let usage = "`y` は `y0`(解除)か `y a,d,s,r`(4 つの数)です";
                let first = c.require_number(pos, 'y')?;
                if c.peek() != Some(',') {
                    if first.0 != 0 {
                        return Err(mml_err(c.pos(), usage));
                    }
                    Op::Envelope(None)
                } else {
                    let nums = c.comma_numbers(first, 4, usage)?;
                    let limits = [
                        (2000, "アタックは 0〜2000 ミリ秒です"),
                        (2000, "ディケイは 0〜2000 ミリ秒です"),
                        (100, "サステインは 0〜100 です"),
                        (600, "リリースは 0〜600 ミリ秒です"),
                    ];
                    for (&(n, p), (max, message)) in nums.iter().zip(limits) {
                        if n > max {
                            return Err(mml_err(p, message));
                        }
                    }
                    Op::Envelope(Some(EnvelopeParams {
                        attack_ms: nums[0].0,
                        decay_ms: nums[1].0,
                        sustain: nums[2].0 as u8,
                        release_ms: nums[3].0,
                    }))
                }
            }
            'm' => {
                let usage = "`m` は `m0`(解除)か `m 速さ,深さ`(2 つの数)です";
                let first = c.require_number(pos, 'm')?;
                if c.peek() != Some(',') {
                    if first.0 != 0 {
                        return Err(mml_err(c.pos(), usage));
                    }
                    Op::Vibrato(None)
                } else {
                    let nums = c.comma_numbers(first, 2, usage)?;
                    if !(1..=20).contains(&nums[0].0) {
                        return Err(mml_err(nums[0].1, "ビブラートの速さは 1〜20 Hz です"));
                    }
                    if !(1..=100).contains(&nums[1].0) {
                        return Err(mml_err(nums[1].1, "ビブラートの深さは 1〜100 です"));
                    }
                    Op::Vibrato(Some(VibratoParams {
                        rate_hz: nums[0].0 as u8,
                        depth: nums[1].0 as u8,
                    }))
                }
            }
            'k' => {
                let negative = match c.peek() {
                    Some('+') => {
                        c.bump();
                        false
                    }
                    Some('-') => {
                        c.bump();
                        true
                    }
                    _ => false,
                };
                let Some((n, p)) = c.number() else {
                    return Err(mml_err(pos, "`k` の後に数値が必要です"));
                };
                if n > 100 {
                    return Err(mml_err(p, "デチューンは −100〜100 セントです"));
                }
                Op::Detune(if negative { -(n as i16) } else { n as i16 })
            }
```

- `struct TrackState` の `timbre: Timbre,` の次に足す:

```rust
    envelope: Option<EnvelopeParams>,
    vibrato: Option<VibratoParams>,
    detune: i16,
```

- `run_track` の `TrackState { … }` の初期化の `timbre: Timbre::Fm,` の次に足す:

```rust
        envelope: None,
        vibrato: None,
        detune: 0,
```

- `run_track` の `Op::Timbre(t) => st.timbre = *t,` の次に足す:

```rust
            Op::Envelope(e) => st.envelope = *e,
            Op::Vibrato(v) => st.vibrato = *v,
            Op::Detune(d) => st.detune = *d,
```

- `fn gated` の前に、関数を足す:

```rust
/// いまのトラックの状態から、音に付ける `params`(どれも既定なら `None`)。
fn timbre_params(st: &TrackState) -> Option<TimbreParams> {
    if st.envelope.is_none() && st.vibrato.is_none() && st.detune == 0 {
        None
    } else {
        Some(TimbreParams {
            envelope: st.envelope,
            vibrato: st.vibrato,
            detune: st.detune,
        })
    }
}

```

- `NoteEvent { … }` を作る 2 か所(単音と和音)の、`timbre: st.timbre,` の次に、`params: timbre_params(&st),` を足す。

- テスト `only_drum_is_a_prefix_of_another_timbre_name` の先頭のコメント(`// 音色名の読み取りは最長一致。…残りの `kit` は命令の先頭にならない`)を置き換える:

```rust
        // 音色名の読み取りは最長一致。接頭辞の組は drum と drumkit だけ(`@drum` の後に `k0` と続く MML は、drum + デチューンになる)
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "FAILED|panicked|test result: .* [1-9][0-9]* passed|^error"; rustfmt --check --edition 2021 src/mml.rs 2>&1 | grep -c '^Diff in'; cd .. && git status --short; git diff frontend/src/bindings/tauri.gen.ts | grep -c "params"`
Expected: すべて PASS(失敗 0)。rustfmt の差分は 4 以下(新しいコードに差分が出たら、整形済みの形に直す)。`git status` に `domain/mml.rs`・`domain/mod.rs`・`mml.rs` と、再生成された `tauri.gen.ts` が出る。`tauri.gen.ts` の差分は、`EnvelopeParams` `VibratoParams` `TimbreParams` の追加と `NoteEvent` の `params: TimbreParams | null` だけであること(`git diff frontend/src/bindings/tauri.gen.ts` で確認する)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/domain/mml.rs src-tauri/src/domain/mod.rs src-tauri/src/mml.rs frontend/src/bindings/tauri.gen.ts && git commit -m "feat: MML に音色パラメーター y m k を追加(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: フロント — パラメーターごとの音源(エンベロープ・デチューン・ビブラート)

**Files:**
- Modify: `frontend/src/lib/mmlTimbres.ts`、`frontend/src/lib/mmlTimbres.test.ts`
- Modify: `frontend/src/lib/mmlRender.ts`、`frontend/src/lib/mmlRender.test.ts`

**Interfaces:**
- Consumes: Task 1 の TS 型 `TimbreParams` / `NoteEvent.params`(`tauri.gen.ts`)、既存の `TimbrePreset`・`Voice`・`TIMBRES`・`poly`・`fakeTone`(`mmlTimbres.test.ts` の drumkit のテスト用)。
- Produces: `TimbrePreset.create(Tone, volumeDb, params?: TimbreParams | null)`、`TimbrePreset.percussive?: boolean`(`drum` `noise` `drumkit` が true)。`renderScore` は、(トラック, 音色, パラメーターの組)ごとに音源を作る。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlTimbres.test.ts` を編集する。

(a) `fakeTone()` に、`set` の記録と、旋律の音色が使うクラスを足す。

置き換える前:
```ts
    toDestination() {
      return this;
    }
```
置き換えた後:
```ts
    destinations = 0;
    sets: unknown[] = [];
    toDestination() {
      this.destinations++;
      return this;
    }
    set(options: unknown) {
      this.sets.push(options);
      return this;
    }
```

置き換える前:
```ts
    Filter: cls("Filter"),
  } as unknown as ToneModule;
```
置き換えた後:
```ts
    Filter: cls("Filter"),
    Vibrato: cls("Vibrato"),
    Synth: cls("Synth"),
    FMSynth: cls("FMSynth"),
    AMSynth: cls("AMSynth"),
    DuoSynth: cls("DuoSynth"),
    MonoSynth: cls("MonoSynth"),
  } as unknown as ToneModule;
```

(b) ファイルの最後に追加する:

```ts

describe("音色のパラメーター(Issue #499)", () => {
  const MELODIC: Timbre[] = ["fm", "square", "pulse", "triangle", "saw", "sine", "organ", "epiano", "bell", "am", "lead", "bass"];
  const ENV = { attackMs: 10, decayMs: 200, sustain: 60, releaseMs: 300 };
  const params = (p: Partial<{ envelope: typeof ENV | null; vibrato: { rateHz: number; depth: number } | null; detune: number }>) => ({
    envelope: null,
    vibrato: null,
    detune: 0,
    ...p,
  });
  const polyOf = (made: ReturnType<typeof fakeTone>["made"]) => made.find((m) => m.cls === "PolySynth")!.node as unknown as { sets: unknown[]; destinations: number; connected: unknown[] };

  it("打楽器だけが percussive で、パラメーターを無視する", () => {
    expect(TIMBRE_LIST.filter((p) => p.percussive).map((p) => p.name)).toEqual(["drum", "noise", "drumkit"]);
  });

  it("パラメーターなしでは、set もビブラートも使わず、そのまま出力に繋ぐ", () => {
    for (const name of MELODIC) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3);
      TIMBRES[name].create(Tone, -3, null);
      expect(made.filter((m) => m.cls === "Vibrato"), name).toHaveLength(0);
      for (const m of made.filter((x) => x.cls === "PolySynth")) {
        const node = m.node as unknown as { sets: unknown[]; destinations: number };
        expect(node.sets, name).toEqual([]);
        expect(node.destinations, name).toBe(1);
      }
    }
  });

  it("エンベロープは、秒とサステインの割合にして set する(modulationEnvelope は触らない)", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.fm.create(Tone, -3, params({ envelope: ENV }));
    expect(polyOf(made).sets).toEqual([{ envelope: { attack: 0.01, decay: 0.2, sustain: 0.6, release: 0.3 } }]);
  });

  it("lead(DuoSynth)は、2 つの声の両方のエンベロープを置き換える", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.lead.create(Tone, -3, params({ envelope: ENV }));
    const e = { attack: 0.01, decay: 0.2, sustain: 0.6, release: 0.3 };
    expect(polyOf(made).sets).toEqual([{ voice0: { envelope: e }, voice1: { envelope: e } }]);
  });

  it("デチューンは set({ detune })", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.square.create(Tone, -3, params({ detune: -12 }));
    expect(polyOf(made).sets).toEqual([{ detune: -12 }]);
  });

  it("ビブラートは Tone.Vibrato(速さ, 深さ/100)を出力に挟み、音源を出力に二重に繋がない", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.saw.create(Tone, -3, params({ vibrato: { rateHz: 5, depth: 30 } }));
    const vibrato = made.find((m) => m.cls === "Vibrato")!;
    expect(vibrato.args).toEqual([5, 0.3]);
    const synth = polyOf(made);
    expect(synth.connected).toEqual([vibrato.node]);
    expect(synth.destinations).toBe(0);
    expect((vibrato.node as unknown as { destinations: number }).destinations).toBe(1);
  });

  it("全旋律音色が、3 つのパラメーターをすべて受け取れる", () => {
    for (const name of MELODIC) {
      const { Tone, made } = fakeTone();
      expect(() => TIMBRES[name].create(Tone, -3, params({ envelope: ENV, vibrato: { rateHz: 5, depth: 30 }, detune: 7 })), name).not.toThrow();
      expect(polyOf(made).sets.length, name).toBe(2);
    }
  });

  it("打楽器は、パラメーターを渡されても何もしない", () => {
    for (const name of ["drum", "drumkit"] as const) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3, params({ envelope: ENV, vibrato: { rateHz: 5, depth: 30 }, detune: 7 }));
      expect(made.filter((m) => m.cls === "Vibrato"), name).toHaveLength(0);
      for (const m of made.filter((x) => x.cls === "PolySynth")) expect((m.node as unknown as { sets: unknown[] }).sets, name).toEqual([]);
    }
  });
});
```

`frontend/src/lib/mmlRender.test.ts` を編集する。

(a) 型・モックに `params` と `percussive` を足す。

置き換える前:
```ts
import type { MmlScore, NoteEvent, Timbre } from "../bindings/tauri.gen";

type Call = { hz: number; d: number; t: number; v: number; midi?: number };
const created: { timbre: string; volumeDb: number; calls: Call[] }[] = [];
```
置き換えた後:
```ts
import type { MmlScore, NoteEvent, Timbre, TimbreParams } from "../bindings/tauri.gen";

type Call = { hz: number; d: number; t: number; v: number; midi?: number };
const created: { timbre: string; volumeDb: number; params: unknown; calls: Call[] }[] = [];
```

置き換える前:
```ts
  const mk = (name: string, gainDb: number, perNote = false) => ({
    number: 0,
    name,
    gainDb,
    releaseSec: 0.1,
    perNote,
    create: (_Tone: unknown, volumeDb: number) => {
      const rec = { timbre: name, volumeDb, calls: [] as Call[] };
```
置き換えた後:
```ts
  const mk = (name: string, gainDb: number, perNote = false, percussive = false) => ({
    number: 0,
    name,
    gainDb,
    releaseSec: 0.1,
    perNote,
    percussive,
    create: (_Tone: unknown, volumeDb: number, params?: unknown) => {
      const rec = { timbre: name, volumeDb, params, calls: [] as Call[] };
```

置き換える前:
```ts
  return { TIMBRES: { fm: mk("fm", 0), square: mk("square", -10), noise: mk("noise", -4.5, true) } };
```
置き換えた後:
```ts
  return {
    TIMBRES: { fm: mk("fm", 0), square: mk("square", -10), noise: mk("noise", -4.5, true, true), drum: mk("drum", -6, false, true) },
  };
```

(b) `note()` を置き換える。

置き換える前:
```ts
const note = (track: number, timbre: Timbre, startSec = 0, midi = 60): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi,
  velocity: 0.5,
  timbre,
});
```
置き換えた後:
```ts
const note = (track: number, timbre: Timbre, startSec = 0, midi = 60, params: TimbreParams | null = null): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi,
  velocity: 0.5,
  timbre,
  params,
});
```

(c) `describe("renderScore", ...)` の中、`"音源の trigger に、ノート番号 midi も渡す"` の `it` の次に追加する:

```ts

  const P1: TimbreParams = { envelope: { attackMs: 10, decayMs: 200, sustain: 60, releaseMs: 300 }, vibrato: null, detune: 0 };
  const P2: TimbreParams = { envelope: null, vibrato: { rateHz: 5, depth: 30 }, detune: 12 };

  it("音源を作るとき、パラメーターを渡す。パラメーターなしは null", async () => {
    await renderScore(score([note(0, "fm", 0, 60, P1), note(0, "square", 0.5)]));
    expect(created.map((c) => [c.timbre, c.params])).toEqual([
      ["fm", P1],
      ["square", null],
    ]);
  });

  it("同じ (トラック, 音色, パラメーター) の音は 1 つの音源、パラメーターが違えば別の音源", async () => {
    await renderScore(
      score([note(0, "fm", 0, 60, P1), note(0, "fm", 0.5, 62, { ...P1 }), note(0, "fm", 1, 64, P2), note(0, "fm", 1.5, 65)]),
    );
    expect(created).toHaveLength(3);
    expect(created.map((c) => c.calls.length).sort()).toEqual([1, 1, 2]);
  });

  it("打楽器の音色は、パラメーターが違っても 1 つの音源を使い回し、パラメーターは null で作る", async () => {
    await renderScore(score([note(0, "drum", 0, 60, P1), note(0, "drum", 0.5, 60, P2), note(0, "drum", 1)]));
    expect(created).toHaveLength(1);
    expect(created[0].params).toBeNull();
    expect(created[0].calls).toHaveLength(3);
  });

  it("単音用(perNote)の打楽器も、パラメーターは null で作る", async () => {
    await renderScore(score([note(0, "noise", 0, 60, P1)]));
    expect(created[0].params).toBeNull();
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlTimbres.test.ts src/lib/mmlRender.test.ts 2>&1 | grep -E "×|FAIL|Test Files|Tests |Error" | head -20`
Expected: `mmlTimbres.test.ts` の「音色のパラメーター」のテストと、`mmlRender.test.ts` の新しいテストが FAIL(`percussive` や `params` が無い)。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlTimbres.ts` を編集する。

- import を置き換える:

置き換える前:
```ts
import type { Timbre } from "../bindings/tauri.gen";
```
置き換えた後:
```ts
import type { Timbre, TimbreParams } from "../bindings/tauri.gen";
```

- `TimbrePreset` に `percussive` を足し、`create` の型を置き換える。

置き換える前:
```ts
  /** `Tone.Offline` のコールバック内で呼ぶ。出力先と音量(`volumeDb`)を設定済みの音源を返す。 */
  create(Tone: ToneModule, volumeDb: number): Voice;
```
置き換えた後:
```ts
  /** true なら打楽器で、`y` `m` `k` のパラメーターを無視する(描画は、パラメーターが違っても音源を使い回す) */
  percussive?: boolean;
  /** `Tone.Offline` のコールバック内で呼ぶ。出力先と音量(`volumeDb`)を設定済みの音源を返す。`params` は旋律の音色だけが使う。 */
  create(Tone: ToneModule, volumeDb: number, params?: TimbreParams | null): Voice;
```

- `poly` を置き換える。

置き換える前:
```ts
function poly(synth: PolyLike, volumeDb: number): Voice {
  synth.toDestination();
  synth.volume.value = volumeDb;
  return { trigger: (hz, d, t, v) => synth.triggerAttackRelease(hz, d, t, v) };
}
```
置き換えた後:
```ts
/** パラメーター(Issue #499)を音源に反映するための材料。`duo` は DuoSynth(エンベロープが 2 つの声の中にある) */
interface Fx {
  Tone: ToneModule;
  params: TimbreParams | null | undefined;
  duo?: boolean;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyPoly = import("tone").PolySynth<any>;

/**
 * 出力に繋いで音量を設定する。`fx.params` があれば、エンベロープ(`envelope` だけ。modulationEnvelope は変えない)とデチューンを
 * `set` で反映し、ビブラートは `Tone.Vibrato` を出力に挟む。`PolySynth` は描画中に設定を時刻つきで変えられないので、
 * パラメーターの組ごとに別の音源を作る(呼び出し側 `renderScore` の責任)。
 */
function poly(synth: AnyPoly, volumeDb: number, fx?: Fx): Voice {
  synth.volume.value = volumeDb;
  const params = fx?.params;
  if (fx && params) {
    if (params.envelope) {
      const e = {
        attack: params.envelope.attackMs / 1000,
        decay: params.envelope.decayMs / 1000,
        sustain: params.envelope.sustain / 100,
        release: params.envelope.releaseMs / 1000,
      };
      synth.set(fx.duo ? { voice0: { envelope: e }, voice1: { envelope: e } } : { envelope: e });
    }
    if (params.detune !== 0) synth.set({ detune: params.detune });
  }
  if (fx && params?.vibrato) {
    const vibrato = new fx.Tone.Vibrato(params.vibrato.rateHz, Math.min(1, params.vibrato.depth / 100));
    vibrato.toDestination();
    synth.connect(vibrato);
  } else {
    synth.toDestination();
  }
  return { trigger: (hz, d, t, v) => synth.triggerAttackRelease(hz, d, t, v) };
}
```

- `basic` を置き換える。

置き換える前:
```ts
const basic =
  (oscillator: SynthOscillator) =>
  (Tone: ToneModule, db: number): Voice =>
    poly(new Tone.PolySynth(Tone.Synth, { oscillator, envelope: BASIC_ENV }), db);
```
置き換えた後:
```ts
const basic =
  (oscillator: SynthOscillator) =>
  (Tone: ToneModule, db: number, params?: TimbreParams | null): Voice =>
    poly(new Tone.PolySynth(Tone.Synth, { oscillator, envelope: BASIC_ENV }), db, { Tone, params });
```

- 旋律の音色の `create` を、`params` を受けて `poly` に渡す形にする。次の Python で、`fm` `organ` `epiano` `bell` `am` `lead` `bass` の各ブロックを書き換える(置き換え前の文字列が、各ブロックの中でちょうど 1 回出ることを確かめる):

```python
import re
p = "frontend/src/lib/mmlTimbres.ts"
s = open(p, encoding="utf-8").read()
for name in ["fm", "organ", "epiano", "bell", "am", "lead", "bass"]:
    start = s.index(f"\n  {name}: {{")
    end = s.index("\n  },\n", start) + len("\n  },\n")
    block = s[start:end]
    assert block.count("create: (Tone, db) =>") == 1, name
    block = block.replace("create: (Tone, db) =>", "create: (Tone, db, params) =>")
    fx = "{ Tone, params, duo: true }" if name == "lead" else "{ Tone, params }"
    old_tail = "        db,\n      );\n    }," if name == "lead" else "        db,\n      ),\n  },\n"
    # lead は `return poly(...)` の形、ほかは `poly(...),` の形
    if name == "lead":
        assert block.count("        db,\n      );") == 1, name
        block = block.replace("        db,\n      );", f"        db,\n        {fx},\n      );")
    else:
        assert block.count("        db,\n      ),\n  },\n") == 1, name
        block = block.replace("        db,\n      ),\n  },\n", f"        db,\n        {fx},\n      ),\n  }},\n")
    s = s[:start] + block + s[end:]
open(p, "w", encoding="utf-8").write(s)
```

- 打楽器の 3 つ(`drum` `noise` `drumkit`)に `percussive: true,` を足す。`drum` と `noise` は、`releaseSec: …,` の行の次に 1 行足す。`drumkit` は `releaseSec: 0.2,` の次に足す。例(`drum`):

置き換える前:
```ts
    name: "drum",
    gainDb: -6.1,
    releaseSec: 0.3,
```
置き換えた後:
```ts
    name: "drum",
    gainDb: -6.1,
    releaseSec: 0.3,
    percussive: true,
```

`noise` の `releaseSec: 0.05,` の次にも `percussive: true,`(`perNote: true,` の前)、`drumkit` の `releaseSec: 0.2,` の次にも `percussive: true,` を足す。

(b) `frontend/src/lib/mmlRender.ts` の `voiceFor` を置き換える。

置き換える前:
```ts
      const voiceFor = (track: number, timbre: Timbre): Voice => {
        const preset = TIMBRES[timbre];
        const volumeDb = preset.gainDb + trackDb;
        if (preset.perNote) return preset.create(Tone, volumeDb);
        const key = `${track}:${timbre}`;
        let voice = voices.get(key);
        if (!voice) {
          voice = preset.create(Tone, volumeDb);
          voices.set(key, voice);
        }
        return voice;
      };
```
置き換えた後:
```ts
      const voiceFor = (track: number, timbre: Timbre, noteParams: TimbreParams | null): Voice => {
        const preset = TIMBRES[timbre];
        const volumeDb = preset.gainDb + trackDb;
        // 打楽器は、パラメーターを無視する(パラメーターが違っても 1 つの音源を使い回す)
        const params = preset.percussive ? null : noteParams;
        if (preset.perNote) return preset.create(Tone, volumeDb, params);
        // PolySynth は描画中に設定を時刻つきで変えられないので、パラメーターの組ごとに別の音源を作る
        const key = `${track}:${timbre}:${params === null ? "" : JSON.stringify(params)}`;
        let voice = voices.get(key);
        if (!voice) {
          voice = preset.create(Tone, volumeDb, params);
          voices.set(key, voice);
        }
        return voice;
      };
```

`voiceFor(...)` の呼び出しと import を直す。

置き換える前:
```ts
        voiceFor(n.track, n.timbre).trigger(hz, n.durationSec ?? 0, n.startSec ?? 0, n.velocity ?? 0, n.midi);
```
置き換えた後:
```ts
        voiceFor(n.track, n.timbre, n.params ?? null).trigger(hz, n.durationSec ?? 0, n.startSec ?? 0, n.velocity ?? 0, n.midi);
```

置き換える前:
```ts
import type { MmlScore, Timbre } from "../bindings/tauri.gen";
```
置き換えた後:
```ts
import type { MmlScore, Timbre, TimbreParams } from "../bindings/tauri.gen";
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib 2>&1 | grep -E "×|FAIL|Test Files|Tests |Error" | head -20`
Expected: すべて PASS。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -4`
Expected: 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件だけは出てよい)。型エラーが出たら、`AnyPoly` への代入や `synth.set` の引数の型を、`node_modules/tone/build/esm/instrument/PolySynth.d.ts` に合わせて直す(挙動は変えない。直した内容は台帳に Ruling として書く)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlTimbres.ts frontend/src/lib/mmlTimbres.test.ts frontend/src/lib/mmlRender.ts frontend/src/lib/mmlRender.test.ts && git commit -m "feat: MML の音色パラメーターを、音源ごとに反映して描画する(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 字句解析・入力補助キー・ヘルプ・ガイド

**Files:**
- Modify: `frontend/src/lib/mmlLexer.ts`、`frontend/src/lib/mmlLexer.test.ts`
- Modify: `frontend/src/lib/mmlKeys.ts`、`frontend/src/lib/mmlKeys.test.ts`
- Modify: `frontend/src/ui/settings/SoundChoicePicker.svelte`、`frontend/src/ui/settings/SoundChoicePicker.test.ts`
- Modify: `docs/guide/user-guide.md`

**Interfaces:**
- Consumes: `lexMml` の `push` / `at` / `digitsEnd`(`ch` は小文字化済み)、`MML_KEY_GROUPS`、`data-testid="mml-syntax-help"` の `<pre>`、ガイドの `@drumkit` の割り当て表の後ろ。
- Produces: なし(表示のみ)。入力補助キーのグループ `params`(`y` `m` `k` `,`)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlLexer.test.ts` の `describe("lexMml", ...)` の中、「`$a t=60` は展開と t=、…」の `it` の前に追加する:

```ts
  it("音色パラメーター y m k は command、数字は number、カンマは command(Issue #499)", () => {
    expect(lex("y10,200,60,300")).toEqual([
      ["y", "command"],
      ["10", "number"],
      [",", "command"],
      ["200", "number"],
      [",", "command"],
      ["60", "number"],
      [",", "command"],
      ["300", "number"],
    ]);
    expect(lex("m5,30 k-12 k+7 y0")).toEqual([
      ["m", "command"],
      ["5", "number"],
      [",", "command"],
      ["30", "number"],
      ["k-", "command"],
      ["12", "number"],
      ["k+", "command"],
      ["7", "number"],
      ["y", "command"],
      ["0", "number"],
    ]);
  });

  it("y m k の後ろに数字がなくても、読めない文字にはしない。途中のカンマだけ・末尾のカンマも", () => {
    expect(lex("y")).toEqual([["y", "command"]]);
    expect(lex("m5,")).toEqual([
      ["m", "command"],
      ["5", "number"],
      [",", "command"],
    ]);
    expect(lex("k c")).toEqual([
      ["k", "command"],
      ["c", "note"],
    ]);
  });

  it("@drumk0 は、音色 @drum と、デチューン k0 に分かれる。@drumkit は 1 つの音色", () => {
    expect(lex("@drumk0")).toEqual([
      ["@drum", "timbre"],
      ["k", "command"],
      ["0", "number"],
    ]);
    expect(lex("@drumkit")).toEqual([["@drumkit", "timbre"]]);
  });

```

`frontend/src/lib/mmlKeys.test.ts` を編集する。

(a) グループの並びを置き換える。

置き換える前:
```ts
    expect(MML_KEY_GROUPS.map((g) => g.id)).toEqual(["notes", "modifiers", "octave", "lengths", "structure"]);
```
置き換えた後:
```ts
    expect(MML_KEY_GROUPS.map((g) => g.id)).toEqual(["notes", "modifiers", "octave", "lengths", "params", "structure"]);
```

(b) 「修飾は # - . & ^…」の `it` の、`structure` の期待の次の行に追加する。

置き換える前:
```ts
    expect(inserts("structure")).toEqual(["[", "]", ":", "{", "}", "'", "$", ";"]);
```
置き換えた後:
```ts
    expect(inserts("structure")).toEqual(["[", "]", ":", "{", "}", "'", "$", ";"]);
    expect(inserts("params")).toEqual(["y", "m", "k", ","]);
```

(c) 「キーだけで打った列…」の `it` の、キー列に追加する。

置き換える前:
```ts
      "macro", "c",
      "track", "r", "len-4", "oct-down",
```
置き換えた後:
```ts
      "macro", "c",
      "param-y", "len-4", "param-comma", "len-8", "param-m", "len-4", "param-k", "len-2", "c",
      "track", "r", "len-4", "oct-down",
```

題名の `' c e g ' 4 $ c ; r 4 <` は `' c e g ' 4 $ c y 4 , 8 m 4 k 2 c ; r 4 <` にする。

`frontend/src/ui/settings/SoundChoicePicker.test.ts` の「記法ヘルプに、@drumkit の割り当て…」の `it` の次に追加する:

```ts

  it("記法ヘルプに、音色パラメーター y m k の書き方と、解除の書き方が出る(Issue #499)", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    expect(text).toContain("y10,200,60,300");
    expect(text).toContain("m5,30");
    expect(text).toContain("k+12");
    expect(text).toContain("y0 m0 k0");
    expect(text).toContain("打楽器");
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | grep -E "×|Test Files|Tests "`
Expected: 追加したテストが FAIL(字句解析・キー・ヘルプ)。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlLexer.ts` の、`} else if (ch === "<" || ch === ">") {` の**前**に、分岐を足す:

```ts
    } else if (ch === "y" || ch === "m" || ch === "k") {
      // 音色パラメーター(Issue #499): `y a,d,s,r` `m 速さ,深さ` `k セント`。`k+` `k-` は符号まで、カンマは command にする
      const signed = ch === "k" && (at(i + 1) === "+" || at(i + 1) === "-");
      const cmdEnd = i + (signed ? 2 : 1);
      push(i, cmdEnd, "command");
      let j = digitsEnd(cmdEnd);
      if (j > cmdEnd) push(cmdEnd, j, "number");
      while (at(j) === ",") {
        push(j, j + 1, "command");
        const next = digitsEnd(j + 1);
        if (next > j + 1) push(j + 1, next, "number");
        j = next;
      }
      i = j;
```

(b) `frontend/src/lib/mmlKeys.ts` の `lengths` グループの次(`structure` の前)に追加する:

```ts
  {
    id: "params",
    label: "音色パラメーター",
    keys: [
      { id: "param-y", label: "y", insert: "y", aria: "エンベロープ y" },
      { id: "param-m", label: "m", insert: "m", aria: "ビブラート m" },
      { id: "param-k", label: "k", insert: "k", aria: "デチューン k" },
      { id: "param-comma", label: ",", insert: ",", aria: "区切り ," },
    ],
  },
```

(c) `frontend/src/ui/settings/SoundChoicePicker.svelte` の「記法」の `<pre>` の、`@drumkit ドラム(…)` の行の**次**に、2 行足す:

```
y10,200,60,300 エンベロープ(アタック・ディケイ ミリ秒、サステイン 0〜100、リリース 0〜600 ミリ秒)  m5,30 ビブラート(速さ 1〜20 Hz、深さ 1〜100)  k+12 デチューン(−100〜100 セント)
y0 m0 k0 で音色の既定に戻す。トラックの状態で、次の音から効く。和音の中には書けない。打楽器(drum noise drumkit)には効かない
```

(d) `docs/guide/user-guide.md` を編集する(置き換え前の文字列がちょうど 1 回出ることを確かめながら置き換える)。

1. `@drumkit` の割り当て表の後の、`例: \`@drumkit t120 o2 [c8 f+8 d8 f+8]4\`…` の行の**次**に、次の節を足す:

```markdown

**音色のパラメーター**: `y` `m` `k` で、音の立ち上がり・余韻やビブラート、音程のずれを、トラックごとに変えられます。書いた所から後の音に効き(次の音から)、`t` `v` `q` と同じように、トラックごとに独立します。ループやマクロの中にも書けます。和音の中には書けません(和音の前に書きます)。`drum` `noise` `drumkit` には効きません。

| 書き方 | 意味 |
|---|---|
| `y10,200,60,300` | エンベロープ。アタック(ミリ秒、0〜2000)、ディケイ(ミリ秒、0〜2000)、サステイン(0〜100 %)、リリース(ミリ秒、0〜600)。4 つすべて書きます。`y0` で、音色の既定に戻します |
| `m5,30` | ビブラート。速さ(Hz、1〜20)、深さ(1〜100。大きいほど深い)。`m0` で解除します。`lead` が持つ組み込みのビブラートは、`m` と関係なく残ります |
| `k+12`、`k-12` | デチューン(音程のずれ、セント。−100〜100)。`k0` で解除します |

例: `@lead y5,100,70,400 m5,40 o4 [c d e f]2`。`@` で音色を切り替えても、パラメーターは保たれます。`m` の深さの大きさや、`y` のサステインを大きくしたときの音の大きさは、実際に聴いて調整してください。
```

2. 入力補助キーの列挙を置き換える(ガイドの MML の段落にある文):

置き換える前:
```
長さ(`1 2 4 8 16 32`)、`[` `]` `:` `{` `}` `'` `$` `;` を押すと
```
置き換えた後:
```
長さ(`1 2 4 8 16 32`)、音色パラメーター `y` `m` `k` `,`、`[` `]` `:` `{` `}` `'` `$` `;` を押すと
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib src/ui/settings 2>&1 | grep -E "×|Test Files|Tests "; pnpm check 2>&1 | tail -2`
Expected: すべて PASS、0 errors。`SoundChoicePicker.keys.test.ts` は、新しいキー(`y` `m` `k` `,`)も自動で押して確かめる。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts frontend/src/lib/mmlKeys.ts frontend/src/lib/mmlKeys.test.ts frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md && git commit -m "docs: MML の音色パラメーターを、色分け・入力補助キー・ヘルプ・ガイドに追記(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 実機の確認(隔離した Xvfb)

**Files:**
- Modify(実測で直す場合のみ): `frontend/src/lib/mmlTimbres.ts`
- Modify: `docs/superpowers/specs/2026-10-30-issue-499-mml-timbre-params-design.md`(末尾に「実装後の確認」を追記)

**Interfaces:**
- Consumes: Task 1〜3 の成果。`frontend/dist` と `src-tauri/target/debug/tsumugi` は、必ず作り直してから確認する(`pnpm build` → `cargo build`。直接ビルドしたデバッグバイナリは `frontend/dist` を埋め込む)。
- Produces: spec の記録。

- [ ] **Step 1: 自動テストをすべて走らせる**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|panicked" | head
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check --edition 2021 src/mml.rs 2>&1 | grep -c '^Diff in'
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -3
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm test 2>&1 | grep -E "Test Files|Tests "
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short
```
Expected: `cargo test` は失敗 0。rustfmt の差分は 4 以下。`pnpm check` は 0 errors。`pnpm test` は全件 PASS。`git status` が空(`tauri.gen.ts` を含め、未コミットの変更なし)。

- [ ] **Step 2: 実アプリを隔離して起動する**

メモリの `feedback-dev-server-verification-must-use-virtual-display.md` / `tauri-single-instance-blocks-xdg-isolated-verification.md` / `reference-isolated-instance-needs-short-xdg-path.md` / `debug-bridge-does-not-support-await.md` を先に読む。`cd frontend && pnpm build` と `cd src-tauri && cargo build` の後、これまでの PR(`docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md` の「実装後の確認」)と同じ方法で行う:

- 起動: `Xvfb :<空き番号>`(`ls /tmp/.X11-unix` で選ぶ)、`dbus-run-session`、`XDG_*` を `/tmp` 直下の短いパス(例: `/tmp/tsv499p`)、`WAYLAND_DISPLAY` は unset、`GDK_BACKEND=x11`、`target/debug/tsumugi` を直接起動。起動した全プロセスに、一意なトークン(`TSUMUGI_VERIFY_TOKEN`)を環境変数で付ける。
- アカウント: 起動前に `$XDG_CONFIG_HOME/com.onodai.tsumugi/settings.json` へ、ダミーのアカウントだけを書く(`{"accounts":[{"id":"00000000-0000-4000-8000-000000000499","host":"misskey.invalid","username":"verify","userId":"verifyuser","displayName":"verify","avatarUrl":null}]}`)。
- 起動後、`data-testid="app-menu-trigger"` が出るのを待つ(数分かかる)。メニュー → 設定 → 通知 → 「通知音を鳴らす」ON → 種類「MML（打ち込み）」。入力は `xdotool type`(エディタに `.focus()` した後。`ctrl+a` `BackSpace` で空にしてから)。
- 試聴で `play_notify_sound` に渡る `data:audio/wav;base64,…` を、bridge で `fetch` を包んで `window.__h` に記録し、Python(`python3 -I`、標準ライブラリの `base64` / `struct` / `math` だけ)でピークと RMS を計算する。bridge の応答は JSON の中に JSON が入るので、`jq -r .` で 1 段外し、`.value` を取る。`await` は使えない(同期関数として実行される)ので、「試聴」を押してから、`window.__h.length` が増えるまで、シェルのループで待つ。待ちのループの判定は、応答全体ではなく `"value":"true"` を含むかで行う(`"ok":true` に釣られない)。

- [ ] **Step 3: パラメーターが実際の描画に効くこと**

各旋律の音色(`@0`〜`@11`)について、次を測り、結果を表にする(ピーク・RMS・エラー・NaN の有無):

1. `@N c4`(パラメーターなし。基準)。
2. 極端なエンベロープ: `@N y0,0,100,0 c4`(サステイン最大)、`@N y2000,0,100,600 c4`(アタック最長・リリース最長)。**ピークが 0.36 を超えたら、止めてユーザーに相談する**(音色の既定の音量を変えるか、`y` のサステインに上限を設けるかは、spec の外の判断)。リリース 600 ミリ秒でも、WAV の末尾 0.05 秒の最大値が小さいこと(余韻が切れていない)。
3. 効いていること: `@0 c4` と `@0 y2000,0,100,0 c4`(アタック 2 秒。0.5 秒の音がほとんど立ち上がらない)で、ピークが後者の方が明らかに小さいこと。`@0 c4` と `@0 k100 c4`、`@0 c4` と `@0 m20,100 c4` で、WAV の内容が違うこと(波形のバイト列が一致しないこと)。
4. 打楽器: `@12 y0,0,100,0 c4` と `@12 c4`、`@13 …`、`@drumkit o2 y…,k12 c4` が、パラメーターなしの結果とバイト列まで一致すること(無視されている)。

- [ ] **Step 4: パラメーターの組が多い MML の描画時間**

1. 201 通りのデチューン: `@0 t240 l64 [k-100 c k-99 c … k100 c]14` に当たる文字列を、Python で生成して入力する(`k-100` から `k100` までを 1 ずつ、各 `k<n> c`。`l64` の `c` が 201 個 × 14 回 = 2814 音。3000 音以下・5000 文字以下・90 秒以下になること。使用量の表示で確認する)。入力できないほど長いときは、貼り付けの代わりに `xdotool type` を分割する。
2. 「試聴」を押してから `play_notify_sound` が呼ばれるまでの時間を、`date +%s.%N` の差で測る。比較として、パラメーターなしの同じ長さ(`@0 t240 l64 [c c … ]` 2814 音)の時間も測る。
3. 遅すぎる(体感で 60 秒を超える)ときは、止めてユーザーに相談する(組の数に上限を設けるか、`set` ではなくコンストラクターの引数にするかなど)。

- [ ] **Step 5: UI の表示を確かめる**

- 「記法」を開くと、`y10,200,60,300 エンベロープ(…)` と `y0 m0 k0 で…` の行が出る。
- 色分け: `y10,200,60,300 m5,30 k-12 c` の `y` `m` `k` とカンマが `mml-command`、数字が `mml-number`(DOM のクラスで確かめる)。
- 入力補助キー(モバイル UI。設定 → レイアウトで「モバイル版」にして保存する。隔離したホームの中だけが変わる): `y` `m` `k` `,` のキーが出て、押すとカーソル位置に入る。
- エラー位置: `y10 c`(位置 4 → 画面は 5 文字目)、`y2001,0,0,0 c`(2 文字目)、`'cy1,2,3,4'4`(3 文字目)で、「N 文字目」を押したときにカーソルが移ること(印の文字を打って、位置を確かめる)。
- スクリーンショットを撮って、自分で見る。

確認が終わったら、自分で起動した Xvfb / `dbus-run-session` / アプリのプロセスを、トークンで特定した PID を指定して止める(`pkill` / `killall` は使わない)。`/tmp` に作った隔離ホームと、Xvfb のロックファイルも片付ける。ユーザー自身の `cargo tauri dev` のプロセスには触れない。

- [ ] **Step 6: 結果を spec の末尾に記録する**

`docs/superpowers/specs/2026-10-30-issue-499-mml-timbre-params-design.md` の末尾に、次の節を追記する(実際に確認したことと、確認できなかったことを、事実のとおりに書く。数値と所要時間はこの手順で得た実測値を入れる。括弧書きのまま残さない):

```markdown
## 実装後の確認

(確認した日付・ブランチ・HEAD のコミット、隔離インスタンスの起動方法を書く。)

仕様からの変更:
- (計画から変えた点があれば書く。なければ「なし」。)

確認できたこと:
- 自動テスト: `cargo test` / `pnpm check` / `pnpm test` の結果(件数)。`rustfmt --check src/mml.rs` の差分の数。
- 旋律の音色 12 種の、パラメーターなし・極端なエンベロープ(2 通り)でのピーク・RMS・末尾の最大値の表。エラー・NaN の有無。
- パラメーターが実際に効くこと(エンベロープ・デチューン・ビブラートの、有無の比較の結果)。
- 打楽器が、パラメーターを無視すること。
- 201 通りのデチューンを含む 2814 音の描画時間と、パラメーターなしの同じ長さの描画時間。
- UI: 「記法」の行、色分け、入力補助キー、エラー位置。

確認できなかったこと:
- 実際に音が鳴ること、聞こえ方(エンベロープ・ビブラート・デチューンの効き具合、ビブラートの深さの大きさ、音量バランス)の良し悪し。音声デバイスが無いので、波形の数値までを確認した。聴いた人の評価で、後から直す前提。
- Android の実機・エミュレーター、macOS / Windows。
```

- [ ] **Step 7: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add docs/superpowers/specs/2026-10-30-issue-499-mml-timbre-params-design.md frontend/src/lib/mmlTimbres.ts && git commit -m "docs: MML の音色パラメーターの実装後の確認を記録(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: ブランチの扱いを決める(ユーザーに確認する)**

`superpowers:finishing-a-development-branch` に従う。PR を作る場合は、`.github/pull_request_template.md` の構成に手で合わせ、本文に `Refs #499` と書く(`Fixes` / `Closes` は書かない。#499 の候補が未着手で残るため。ポルタメントが未実装)。`gh pr merge` は `--merge`(squash にしない。必須チェックの `e2e` が実行中だとブロックされるので、その場合は待たずにユーザーに伝える)。push 後に CI を待つループは作らない(ユーザーが見る)。#499 のチェックボックス(「音色のパラメータ」)は、ポルタメントが未実装なので、更新するかをユーザーに確認してから行う。作業ブランチの削除も、ユーザーが頼んだときだけ行う。
