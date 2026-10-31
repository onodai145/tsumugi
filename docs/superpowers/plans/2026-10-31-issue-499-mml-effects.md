# MML のエフェクト(リバーブ・ディレイ) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に `x 量,長さ`(リバーブ)と `z 量,間隔,繰り返し`(ディレイ)を足し、トラックごとに残響とやまびこを付けられるようにする(Issue #499 の最後の項目)。

**Architecture:** Rust の `run_track` が、トラックの状態 `reverb` / `delay` を音ごとに `NoteEvent.reverb` / `NoteEvent.delay` に載せる。フロントの `renderScore` は、エフェクトを使うトラックにだけ「音源を束ねるバス(`Tone.Gain`)」を作り、(トラック, エフェクト, 長さ・間隔・繰り返し)の組ごとに「送りの Gain → `Tone.Reverb` / `Tone.FeedbackDelay` → 出力」を作る。各音の始まりの時刻に、その音の組の送りだけを量にする(`gain.setValueAtTime`)。音源の作成関数は、つなぎ先 `out` を受け取る。描画の長さには、エフェクトの尾を足す。

**Tech Stack:** Rust(`src-tauri/src/mml.rs`、`domain/mml.rs`、tauri-specta)、Svelte + TypeScript(`frontend/src/lib/mmlEffects.ts`(新規)`mmlRender.ts` `mmlTimbres.ts` `mmlLexer.ts` `mmlKeys.ts`)、Tone.js 15.1.22(`Tone.Offline`、`Tone.Reverb`、`Tone.FeedbackDelay`)、Vitest。

**Spec:** `docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md`(承認済み)

## Global Constraints

- コミットメッセージは**件名 1 行だけ**(本文なし)。`git commit -m "<件名>" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` の形で、トレーラーは 2 つ目の `-m` で付ける。`git commit` が失敗・タイムアウトしたら、**再試行せず止めてユーザーに報告**する。`--no-verify` `--no-gpg-sign` は使わない。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集せず、`cd src-tauri && cargo test` が再生成したものをコミットする。
- `cargo` の実行は 120 秒を超えることがある。`run_in_background` で起動し、出力をファイルに落として末尾だけ読む(数百行のテスト出力を会話に流さない)。
- `cd src-tauri && rustfmt --check src/mml.rs` の差分は、`main` で 4 件。増やさない(`cargo fmt` でファイル全体を整形しない)。新しく書く Rust は、rustfmt の整形後の形で書く。`src/domain/mml.rs` と `src/domain/mod.rs` も、`rustfmt --check` に新しい差分を出さない。
- `x` の範囲: 量 1〜100(%)、残響の長さ 100〜5000(ミリ秒)。`z` の範囲: 量 1〜100(%)、間隔 50〜1000(ミリ秒)、繰り返し 0〜90(%)。すべて整数。解除は `x0` / `z0`。和音の中には書けない。打楽器にも効く。
- 1 トラックに書ける組の数: リバーブ 4 種(長さで数える)、ディレイ 4 種((間隔, 繰り返し)で数える)。5 種目を書いた命令を、その命令の位置の構文エラーにする。`x0` / `z0` は数えない。量だけが違うものは同じ種類。
- 描画の長さは `(totalSec ?? 0) + TAIL_SEC + effectTail`。`effectTail` は、使ったエフェクトの尾の最大(最大 5 秒)。何も使わなければ 0(今までと同じ)。
- 今まで通っていた MML は、同じ音・同じ結果のままにする。`x` `z` を書かない MML の `NoteEvent.reverb` / `delay` は `None`(TS では `null`)で、描画の長さも音源のつなぎ方も変わらない。
- 実機確認は、隔離した Xvfb 上の別インスタンスだけで行う(ユーザーの `cargo tauri dev` に触らない)。プロセスは、トークンで特定した PID だけを止める。`pkill` `killall` は使わない。
- 音が実際に鳴ること・聞こえ方は確認できない(音声デバイスなし)。波形の数値までを確認し、そう報告する。

## Review Focus

1. 組の数の上限(4 種)の境界: 同じ組に戻る・量だけが違う・`x0` / `z0`・ループで繰り返す・トラックごと・リバーブとディレイは別に数える(Task 1 のテスト)。
2. 送りの切り替え: 音の始まりの時刻で、その音の組だけが量になり、ほかの組は 0 になる。同じ状態を重ねて設定しない。`x0` の後の音には送らない。トラックをまたいで混ざらない(Task 2 のテスト + ゲート)。
3. エフェクトを使わない MML の描画が変わらない: 描画の長さ・音源のつなぎ先・Tone のノードの数(Task 2 のテスト)と、変更前のバイナリとの WAV のハッシュの一致(Task 2 のゲート)。
4. 尾が切れない: リバーブ 5000ms・ディレイの最大(間隔 1000ms、繰り返し 90%)でも、終端で急に切れず、WAV の長さが予定どおりになる(Task 2 のゲート)。
5. 4 種 + 4 種(複数トラック)を使っても描画が終わり、`Tone.Reverb` の非同期の生成で詰まらず、遅くならない(Task 2 のゲート、Task 4)。

---

### Task 1: Rust — `x` `z` コマンドと `NoteEvent.reverb` / `delay`

**Files:**
- Modify: `src-tauri/src/domain/mml.rs`(`ReverbParams`、`DelayParams`、`NoteEvent.reverb` / `delay`)
- Modify: `src-tauri/src/domain/mod.rs`(再エクスポート)
- Modify: `src-tauri/src/mml.rs`(`Op::Reverb` / `Op::Delay`、パース、`TrackState`、組の数の上限、和音の中のエラー、テスト)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`(`cargo test` が行う)
- Modify: `frontend/src/lib/mmlRender.test.ts`(`note()` ヘルパーに `reverb: null, delay: null` を足すだけ。TS の型を通すため)

**Interfaces:**
- Consumes: 既存の `NoteEvent`、`TrackState`、`run_track`、`Cursor::require_number(pos, char) -> Result<(u32, usize), MmlError>`、`Cursor::comma_numbers(first, count, usage)`、`mml_err`、テスト用の `must_parse(&str) -> MmlScore` と `must_fail(&str) -> MmlError` と `parse(&str) -> Result<MmlScore, MmlError>`。
- Produces:
  - `pub struct ReverbParams { pub level: u8, pub decay_ms: u16 }` と `pub struct DelayParams { pub level: u8, pub time_ms: u16, pub feedback: u8 }`(どちらも `Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type`、`camelCase`)。`crate::domain::{ReverbParams, DelayParams}` として再エクスポート。
  - `NoteEvent.reverb: Option<ReverbParams>`、`NoteEvent.delay: Option<DelayParams>`。TS では `reverb: ReverbParams | null`、`delay: DelayParams | null`。JSON は `{"level":30,"decayMs":2000}` / `{"level":25,"timeMs":300,"feedback":40}` / `null`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/mml.rs` のテストモジュールの末尾(`only_drum_noise_and_drumkit_are_percussive` の後、モジュールを閉じる `}` の手前)に足す:

```rust
    fn fx_of(text: &str) -> Vec<(Option<ReverbParams>, Option<DelayParams>)> {
        must_parse(text)
            .notes
            .iter()
            .map(|n| (n.reverb, n.delay))
            .collect()
    }

    fn reverb(level: u8, decay_ms: u16) -> Option<ReverbParams> {
        Some(ReverbParams { level, decay_ms })
    }

    fn delay(level: u8, time_ms: u16, feedback: u8) -> Option<DelayParams> {
        Some(DelayParams {
            level,
            time_ms,
            feedback,
        })
    }

    #[test]
    fn effects_are_none_unless_set() {
        assert_eq!(fx_of("c e g"), vec![(None, None); 3]);
        assert_eq!(fx_of("x0 z0 c e"), vec![(None, None); 2]);
        assert_eq!(
            fx_of("x30,2000 z25,300,40 c x0 z0 e"),
            vec![(reverb(30, 2000), delay(25, 300, 40)), (None, None)]
        );
    }

    #[test]
    fn reverb_is_read_with_a_range() {
        assert_eq!(fx_of("x30,2000 c")[0].0, reverb(30, 2000));
        assert_eq!(fx_of("x1,100 c")[0].0, reverb(1, 100));
        assert_eq!(fx_of("x100,5000 c")[0].0, reverb(100, 5000));
        // 後から書いたものが効く
        assert_eq!(fx_of("x30,2000 x60,500 c")[0].0, reverb(60, 500));
        for (text, position, word) in [
            ("x0,2000 c", 1, "リバーブの量"),
            ("x101,2000 c", 1, "リバーブの量"),
            ("x30,99 c", 4, "残響の長さ"),
            ("x30,5001 c", 4, "残響の長さ"),
        ] {
            let e = must_fail(text);
            assert_eq!(e.position, position, "{text}");
            assert!(e.message.contains(word), "{text}: {}", e.message);
        }
    }

    #[test]
    fn delay_is_read_with_a_range() {
        assert_eq!(fx_of("z25,300,40 c")[0].1, delay(25, 300, 40));
        assert_eq!(fx_of("z1,50,0 c")[0].1, delay(1, 50, 0));
        assert_eq!(fx_of("z100,1000,90 c")[0].1, delay(100, 1000, 90));
        for (text, position, word) in [
            ("z0,300,40 c", 1, "ディレイの量"),
            ("z101,300,40 c", 1, "ディレイの量"),
            ("z25,49,40 c", 4, "間隔"),
            ("z25,1001,40 c", 4, "間隔"),
            ("z25,300,91 c", 8, "繰り返し"),
        ] {
            let e = must_fail(text);
            assert_eq!(e.position, position, "{text}");
            assert!(e.message.contains(word), "{text}: {}", e.message);
        }
    }

    #[test]
    fn effect_usage_errors_point_at_the_missing_part() {
        // 数字がない: 命令の文字
        for text in ["x c", "z c", "x", "z"] {
            let e = must_fail(text);
            assert_eq!(e.position, 0, "{text}");
            assert!(e.message.contains("数値"), "{text}: {}", e.message);
        }
        // 0 以外の単独の数 / 数が足りない: 足りなくなった位置(次の文字)
        let e = must_fail("x30 c");
        assert_eq!(e.position, 4);
        assert!(e.message.contains("x0"), "{}", e.message);
        let e = must_fail("z25 c");
        assert_eq!(e.position, 4);
        assert!(e.message.contains("z0"), "{}", e.message);
        let e = must_fail("z25,300 c");
        assert_eq!(e.position, 8);
        assert!(e.message.contains("3 つの数"), "{}", e.message);
        // 多すぎる: 余分なカンマの位置
        let e = must_fail("x30,2000,5 c");
        assert_eq!(e.position, 8);
        assert!(e.message.contains("2 つの数"), "{}", e.message);
    }

    #[test]
    fn effects_are_track_state_and_independent() {
        // 書いた後の音から効き、リバーブとディレイは別々に解除できる
        assert_eq!(
            fx_of("z25,300,40 c x30,2000 e x0 g"),
            vec![
                (None, delay(25, 300, 40)),
                (reverb(30, 2000), delay(25, 300, 40)),
                (None, delay(25, 300, 40)),
            ]
        );
        // 音色を切り替えても保たれる
        let both = (reverb(30, 2000), None);
        assert_eq!(fx_of("x30,2000 c @lead e"), vec![both; 2]);
        // トラックごとに独立
        assert_eq!(
            fx_of("x30,2000 c ; c"),
            vec![(reverb(30, 2000), None), (None, None)]
        );
    }

    #[test]
    fn effects_work_inside_loops_macros_tuplets_and_ties() {
        let on = (reverb(30, 2000), None);
        assert_eq!(
            fx_of("[x30,2000 c x0 e]2"),
            vec![on, (None, None), on, (None, None)]
        );
        assert_eq!(fx_of("$f={x30,2000} $f c"), vec![on]);
        assert_eq!(fx_of("{x30,2000 c e g}3"), vec![on; 3]);
        // タイで延びた 1 つの音は、1 つの音として効く
        assert_eq!(fx_of("x30,2000 c&c"), vec![on]);
    }

    #[test]
    fn effects_apply_to_every_chord_note_and_to_percussion() {
        assert_eq!(
            fx_of("z25,300,40 'ceg'4"),
            vec![(None, delay(25, 300, 40)); 3]
        );
        for timbre in ["@drum", "@noise", "@drumkit"] {
            assert_eq!(
                fx_of(&format!("{timbre} x30,2000 c")),
                vec![(reverb(30, 2000), None)],
                "{timbre}"
            );
        }
    }

    #[test]
    fn at_most_four_reverb_and_four_delay_kinds_per_track() {
        // 長さが違う 4 種までは通る。同じ長さに戻っても、量だけが違っても、数え直さない
        assert!(parse("x30,500 c x30,1000 c x30,1500 c x30,2000 c x30,500 c x99,2000 c").is_ok());
        let text = "x30,500 c x30,1000 c x30,1500 c x30,2000 c x30,2500 c";
        let e = must_fail(text);
        assert_eq!(e.position as usize, text.rfind("x30,2500").unwrap());
        assert!(e.message.contains("リバーブ"), "{}", e.message);
        assert!(e.message.contains('4'), "{}", e.message);
        // x0 は数えない
        assert!(parse("x30,500 c x0 c x30,1000 c x30,1500 c x30,2000 c x0 c").is_ok());
        // ディレイは (間隔, 繰り返し) の組で数える。量だけが違うものは同じ種類
        assert!(parse("z10,100,10 c z20,100,10 c z10,100,20 c z10,200,10 c z10,200,20 c").is_ok());
        let text = "z10,100,10 c z10,100,20 c z10,200,10 c z10,200,20 c z10,300,10 c";
        let e = must_fail(text);
        assert_eq!(e.position as usize, text.rfind("z10,300,10").unwrap());
        assert!(e.message.contains("ディレイ"), "{}", e.message);
        assert!(e.message.contains('4'), "{}", e.message);
        // リバーブとディレイは別に数える(4 + 4)
        assert!(parse(
            "x30,500 z10,100,10 c x30,1000 z10,100,20 c x30,1500 z10,200,10 c x30,2000 z10,200,20 c"
        )
        .is_ok());
        // 繰り返しの展開では増えない
        assert!(parse("[x30,500 c x30,1000 c x30,1500 c x30,2000 c]99").is_ok());
        // トラックごとに数える
        assert!(parse(
            "x30,500 c x30,1000 c x30,1500 c x30,2000 c ; x30,2500 c x30,3000 c x30,3500 c x30,4000 c"
        )
        .is_ok());
    }

    #[test]
    fn note_event_serializes_effects_as_camel_case_or_null() {
        let score = must_parse("x30,2000 z25,300,40 c x0 z0 e");
        let first = serde_json::to_value(&score.notes[0]).unwrap();
        assert_eq!(
            first["reverb"],
            serde_json::json!({ "level": 30, "decayMs": 2000 })
        );
        assert_eq!(
            first["delay"],
            serde_json::json!({ "level": 25, "timeMs": 300, "feedback": 40 })
        );
        let second = serde_json::to_value(&score.notes[1]).unwrap();
        assert_eq!(second["reverb"], serde_json::Value::Null);
        assert_eq!(second["delay"], serde_json::Value::Null);
    }
```

既存のテスト `params_are_rejected_inside_a_chord` を直す(`x` `z` が和音の中で書けない命令になるため。`'cz'4` は、もう「読めない文字」の例にならない):

```rust
        for text in [
            "'cy1,2,3,4'4",
            "'cm5,6'4",
            "'ck12'4",
            "'cp80'4",
            "'cx30,500'4",
            "'cz25,300,40'4",
        ] {
```
(元の `for text in ["'cy1,2,3,4'4", "'cm5,6'4", "'ck12'4", "'cp80'4"] {` の 1 行をこの形に置き換える。)

```rust
        // 書けない命令以外は、今までの案内のまま
        assert!(must_fail("'cw'4").message.contains("音符と"));
```
(元の `// y m k 以外は、今までの案内のまま` と `assert!(must_fail("'cz'4")...` の 2 行をこの 2 行に置き換える。)

さらに、既存のテストが `x` を「読めない文字」の例に使っている箇所は、`x` が命令になるので、別の文字 `j` に置き換える(意味は同じ。`j` は今の MML で読めない文字):

- `unknown_characters_report_their_position`: `must_fail("c4 x")` → `must_fail("c4 j")`、`e.message.contains('x')` → `e.message.contains('j')`。
- `error_positions_count_whitespace`: `"c4  x"` → `"c4  j"`、`"c4\r\nx"` → `"c4\r\nj"`。
- `error_positions_after_comments_stay_in_the_original_text`: `"/* あ */ c4 x"` → `"/* あ */ c4 j"`、`"// あ\nx"` → `"// あ\nj"`(コメントの `// 位置は 12 文字目の x` のような説明文は `j` に直す)。
- `a_later_tracks_syntax_error_comes_before_an_earlier_tracks_run_error`: `"t32 [c1]99 ; x"` → `"t32 [c1]99 ; j"`。
- `chord_positions_stay_in_the_original_text`: `"/* あ */ 'c x e'"` → `"/* あ */ 'c j e'"`、コメントの `x` は 12 文字目 → `j` は 12 文字目。
- `validate_sound_choice("mml:x")`(`sound_choice` のテスト) → `validate_sound_choice("mml:j")`。

(上の一覧に無い箇所で、`cargo test` が `x` / `z` の変更で落ちたものがあれば、それが「読めない文字の例」のときだけ同じように `j` に直す。ほかの理由で落ちたら、直さずに止める。)

- [ ] **Step 2: 失敗を確かめる**

Run(バックグラウンドで。出力はファイルへ): `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: > /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/af5fb360-aecc-4ab1-94bb-9cff6a1a0969/scratchpad/t1-red.txt 2>&1; tail -20 <そのファイル>`
Expected: コンパイルエラー(`ReverbParams` `DelayParams` が見つからない、`NoteEvent` に `reverb` / `delay` が無い)。

- [ ] **Step 3: ドメインの型を足す**

`src-tauri/src/domain/mml.rs`: `Glide` の定義の後に足す:

```rust
/// リバーブ(Issue #499)。`level` は送る量(1〜100 %)、`decay_ms` は残響の長さ(100〜5000 ミリ秒)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReverbParams {
    pub level: u8,
    pub decay_ms: u16,
}

/// ディレイ(Issue #499)。`level` は送る量(1〜100 %)、`time_ms` は間隔(50〜1000 ミリ秒)、`feedback` は繰り返し(0〜90 %)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DelayParams {
    pub level: u8,
    pub time_ms: u16,
    pub feedback: u8,
}
```

同じファイルの `NoteEvent` の `glide` の後に足す:

```rust
    /// `x` のリバーブ(効かせない音は `None`)
    pub reverb: Option<ReverbParams>,
    /// `z` のディレイ(効かせない音は `None`)
    pub delay: Option<DelayParams>,
```

`src-tauri/src/domain/mod.rs`: `pub use mml::{ … };` に `DelayParams` と `ReverbParams` を足す(アルファベット順。整形は rustfmt に合わせる):

```rust
pub use mml::{
    DelayParams, EnvelopeParams, Glide, MmlError, MmlParseResult, MmlScore, NoteEvent,
    ReverbParams, Timbre, TimbreParams, VibratoParams,
};
```
(`rustfmt --check --edition 2021 src/domain/mod.rs` で差分が出たら、出力のとおりに折り返しを直す。)

- [ ] **Step 4: パーサーと実行部を足す**

`src-tauri/src/mml.rs`:

1. 先頭の `use crate::domain::{ … };` を次にする:

```rust
use crate::domain::{
    DelayParams, EnvelopeParams, Glide, MmlError, MmlScore, NoteEvent, ReverbParams, Timbre,
    TimbreParams, VibratoParams,
};
```

2. `MAX_PORTAMENTO_MS` の定数の下に足す:

```rust
/// エフェクト `x` `z` の範囲。量は百分率、時間はミリ秒、繰り返しは百分率。
const MAX_EFFECT_LEVEL: u32 = 100;
const MIN_REVERB_DECAY_MS: u32 = 100;
const MAX_REVERB_DECAY_MS: u32 = 5000;
const MIN_DELAY_TIME_MS: u32 = 50;
const MAX_DELAY_TIME_MS: u32 = 1000;
const MAX_DELAY_FEEDBACK: u32 = 90;
/// 1 トラックに書ける、リバーブ(長さ)とディレイ((間隔, 繰り返し))の、種類の数の上限。
/// 種類ごとに描画のエフェクトを作る(リバーブは作るたびに重い)ので、数を抑える。
const MAX_EFFECT_KINDS: usize = 4;
const REVERB_KINDS_MESSAGE: &str =
    "1 トラックに書けるリバーブは 4 種類までです(長さが違うものを数えます)";
const DELAY_KINDS_MESSAGE: &str =
    "1 トラックに書けるディレイは 4 種類までです(間隔・繰り返しが違うものを数えます)";
```

3. `enum Op` の `Portamento(u16),` の後に足す:

```rust
    /// `x 量,長さ`(`None` は `x0` の解除)。
    Reverb(Option<ReverbParams>),
    /// `z 量,間隔,繰り返し`(`None` は `z0` の解除)。
    Delay(Option<DelayParams>),
```

4. `parse_track` の `'p' => { … }` の腕の後(`'@' => {` の前)に足す:

```rust
            'x' => {
                let usage = "`x` は `x0`(解除)か `x 量,長さ`(2 つの数)です";
                let first = c.require_number(pos, 'x')?;
                if c.peek() != Some(',') {
                    if first.0 != 0 {
                        return Err(mml_err(c.pos(), usage));
                    }
                    Op::Reverb(None)
                } else {
                    let nums = c.comma_numbers(first, 2, usage)?;
                    if !(1..=MAX_EFFECT_LEVEL).contains(&nums[0].0) {
                        return Err(mml_err(nums[0].1, "リバーブの量は 1〜100 です"));
                    }
                    if !(MIN_REVERB_DECAY_MS..=MAX_REVERB_DECAY_MS).contains(&nums[1].0) {
                        return Err(mml_err(nums[1].1, "残響の長さは 100〜5000 ミリ秒です"));
                    }
                    Op::Reverb(Some(ReverbParams {
                        level: nums[0].0 as u8,
                        decay_ms: nums[1].0 as u16,
                    }))
                }
            }
            'z' => {
                let usage = "`z` は `z0`(解除)か `z 量,間隔,繰り返し`(3 つの数)です";
                let first = c.require_number(pos, 'z')?;
                if c.peek() != Some(',') {
                    if first.0 != 0 {
                        return Err(mml_err(c.pos(), usage));
                    }
                    Op::Delay(None)
                } else {
                    let nums = c.comma_numbers(first, 3, usage)?;
                    if !(1..=MAX_EFFECT_LEVEL).contains(&nums[0].0) {
                        return Err(mml_err(nums[0].1, "ディレイの量は 1〜100 です"));
                    }
                    if !(MIN_DELAY_TIME_MS..=MAX_DELAY_TIME_MS).contains(&nums[1].0) {
                        return Err(mml_err(nums[1].1, "ディレイの間隔は 50〜1000 ミリ秒です"));
                    }
                    if nums[2].0 > MAX_DELAY_FEEDBACK {
                        return Err(mml_err(nums[2].1, "ディレイの繰り返しは 0〜90 です"));
                    }
                    Op::Delay(Some(DelayParams {
                        level: nums[0].0 as u8,
                        time_ms: nums[1].0 as u16,
                        feedback: nums[2].0 as u8,
                    }))
                }
            }
```

5. 和音の中の腕を直す(`'y' | 'm' | 'k' | 'p' => {` → `x` `z` を足し、メッセージも):

```rust
                        'y' | 'm' | 'k' | 'p' | 'x' | 'z' => {
                            return Err(mml_err(
                                inner_pos,
                                "`y` `m` `k` `p` `x` `z` は和音の中には書けません(和音の前に書いてください)",
                            ));
                        }
```

6. `TrackState` の `portamento_ms` の後に足す:

```rust
    /// `x` のリバーブ(`None` は無効)
    reverb: Option<ReverbParams>,
    /// `z` のディレイ(`None` は無効)
    delay: Option<DelayParams>,
    /// このトラックで使ったリバーブの長さ(ミリ秒)の種類。`MAX_EFFECT_KINDS` まで
    reverb_kinds: Vec<u16>,
    /// このトラックで使ったディレイの (間隔(ミリ秒), 繰り返し) の種類。`MAX_EFFECT_KINDS` まで
    delay_kinds: Vec<(u16, u8)>,
```

7. `run_track` の `TrackState { … }` の初期化の `portamento_ms: 0,` の後に足す:

```rust
        reverb: None,
        delay: None,
        reverb_kinds: Vec::new(),
        delay_kinds: Vec::new(),
```

8. `run_track` の `Op::Portamento(n) => st.portamento_ms = *n,` の後に足す:

```rust
            Op::Reverb(r) => {
                if let Some(p) = r {
                    if !st.reverb_kinds.contains(&p.decay_ms) {
                        if st.reverb_kinds.len() >= MAX_EFFECT_KINDS {
                            return Err(mml_err(pos, REVERB_KINDS_MESSAGE));
                        }
                        st.reverb_kinds.push(p.decay_ms);
                    }
                }
                st.reverb = *r;
            }
            Op::Delay(d) => {
                if let Some(p) = d {
                    let kind = (p.time_ms, p.feedback);
                    if !st.delay_kinds.contains(&kind) {
                        if st.delay_kinds.len() >= MAX_EFFECT_KINDS {
                            return Err(mml_err(pos, DELAY_KINDS_MESSAGE));
                        }
                        st.delay_kinds.push(kind);
                    }
                }
                st.delay = *d;
            }
```

9. `run_track` の 2 か所の `notes.push(NoteEvent { … })`(単音と和音の各音)の `glide` の後に、それぞれ足す:

```rust
                        reverb: st.reverb,
                        delay: st.delay,
```
(単音のほうは `glide,` の後、和音のほうは `glide: None,` の後。インデントは周りの行に合わせる。)

- [ ] **Step 5: テストを通す**

Run(バックグラウンドで): `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: > <スクラッチパッド>/t1-green.txt 2>&1; tail -15 <そのファイル>`
Expected: `mml::` のテストがすべて通る(失敗 0)。落ちたものは、Step 1 の「`x` / `z` を読めない文字の例にしていたテスト」の取りこぼしか、実装の誤りか、切り分けて直す。

- [ ] **Step 6: 全体のテストと書式**

Run(バックグラウンドで): `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test > <スクラッチパッド>/t1-all.txt 2>&1; tail -8 <そのファイル>`
Expected: 失敗 0(前: 813 件。増えるのは、この Task の新しいテストの数)。`generates_frontend_bindings` が `tauri.gen.ts` を再生成する。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && rustfmt --check src/mml.rs 2>&1 | grep -c '^Diff in'; rustfmt --check --edition 2021 src/domain/mml.rs src/domain/mod.rs 2>&1 | grep -c '^Diff in'`
Expected: 1 つ目は 4 以下(`main` で 4 件)、2 つ目は 0(新しい差分を出さない。出たら、出力どおりに直す)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi && git diff --stat frontend/src/bindings/tauri.gen.ts`
Expected: `ReverbParams` `DelayParams` と、`NoteEvent` の `reverb` / `delay` が足されている(数行)。

- [ ] **Step 7: TS の型を通す(`note()` ヘルパー)**

`frontend/src/lib/mmlRender.test.ts` の `note()` の戻り値に、`glide,` の次の行として足す:

```ts
  reverb: null,
  delay: null,
```
(`NoteEvent` の型に新しい項目が増え、TS の型エラーになるのを防ぐだけ。)

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlRender.test.ts 2>&1 | tail -6; pnpm check 2>&1 | tail -4`
Expected: テスト通過、`0 errors`(既存の警告 1 は残る)。

- [ ] **Step 8: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/domain/mml.rs src-tauri/src/domain/mod.rs src-tauri/src/mml.rs frontend/src/bindings/tauri.gen.ts frontend/src/lib/mmlRender.test.ts
git commit -m "feat: MML にリバーブ x とディレイ z を足し、NoteEvent に reverb と delay を載せる(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: フロント — バスとエフェクト、`renderScore`、そして実測のゲート

**Files:**
- Create: `frontend/src/lib/mmlEffects.ts`、`frontend/src/lib/mmlEffects.test.ts`
- Modify: `frontend/src/lib/mmlTimbres.ts`(`VoiceOutput`、`create` の 4 つ目の引数 `out`、`connectOut`、15 の音色のつなぎ先)
- Modify: `frontend/src/lib/mmlRender.ts`(バスとエフェクトを作る、送りを切り替える、尾を足す、`Tone.Offline` のコールバックを `async` に)
- Modify: `frontend/src/lib/mmlTimbres.test.ts`、`frontend/src/lib/mmlRender.test.ts`
- Modify: `docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md`(尾の式を実装に合わせる。Ruling)

**Interfaces:**
- Consumes: Task 1 の `NoteEvent.reverb: ReverbParams | null`(`{ level, decayMs }`)、`NoteEvent.delay: DelayParams | null`(`{ level, timeMs, feedback }`)。既存の `TIMBRES`、`poly()`、`drumkit()`、`renderScore`、`TAIL_SEC`。
- Produces:
  - `mmlTimbres.ts`: `export type VoiceOutput = import("tone").ToneAudioNode;`、`TimbrePreset.create(Tone, volumeDb, params?, out?: VoiceOutput): Voice`(`out` があれば、出力でなくそこへつなぐ。省略時は今までどおり)。
  - `mmlEffects.ts`: `REVERB_PREDELAY_SEC = 0.01`、`MAX_EFFECT_TAIL_SEC = 5`、`REVERB_TRIM_DB`、`DELAY_TRIM_DB`(どちらも初期値 0。ゲートで必要なら直す)、`reverbTailSec(decayMs)`、`delayTailSec(timeMs, feedbackPercent)`、`effectTailSec(notes: NoteEvent[]): number`、`createTrackEffects(Tone, notes): TrackEffects`(`outFor(track): VoiceOutput | undefined`、`ready(): Promise<unknown>`、`apply(n: NoteEvent): void`)。
  - `renderScore`: 描画の長さ = `(totalSec ?? 0) + TAIL_SEC + effectTailSec(notes)`。

- [ ] **Step 1: 変更前のバイナリで、既存の MML の WAV のハッシュを取る(Review Focus 3)**

この段階では、`src-tauri/target/debug/tsumugi` は `main`(ポルタメントまで)の中身でビルドされたまま(Task 1 は `cargo test` だけで、デバッグ用バイナリは作り直していない)。これを変更前のバイナリとして使う。確認:

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
ls -l --time-style=full-iso src-tauri/target/debug/tsumugi frontend/dist/index.html
git log -1 --format='%h %cd' --date=iso ace1cad
git log -1 --format='%h %cd' --date=iso -- frontend/src src-tauri/src
```

バイナリと `frontend/dist` の更新時刻が、`ace1cad`(PR #527 のマージ)の中身を作った後で、Task 1 の変更(`git log` の最後の行より前に作ったもの)を含まないなら、変更前として使える。**古い・分からない・Task 1 の後に作り直した場合は、この Step を飛ばして**、台帳に `Task 2: Ruling: 変更前のバイナリが使えないので、Review Focus 3 は単体テストだけで確認` と書く。

使えるなら、下の Step 10 の a(準備)と同じ手順で隔離インスタンスを起動し(ただし `pnpm build` と `cargo build` はしない。変更前のバイナリをそのまま使う)、次の 5 つの MML の WAV の SHA-256 を `$SP/baseline.txt` に記録し、`bash $SP/stop.sh` で止める:

```
@0 t120 c4 e4 g4
@1 t120 c4 e4 g4
@9 t120 c4 e4 g4
@10 t120 c4 e4 g4
@11 t120 c4 e4 g4
```

1 本ごとに `typemml '<MML>'; preview 60; sha256sum $H/last.b64` を使う(`$H/last.b64` に WAV の data URL が入る)。

- [ ] **Step 2: 失敗するテストを書く(`mmlEffects.test.ts`、新規)**

`frontend/src/lib/mmlEffects.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { DelayParams, NoteEvent, ReverbParams } from "../bindings/tauri.gen";
import {
  DELAY_TRIM_DB,
  MAX_EFFECT_TAIL_SEC,
  REVERB_PREDELAY_SEC,
  REVERB_TRIM_DB,
  createTrackEffects,
  delayTailSec,
  effectTailSec,
  reverbTailSec,
} from "./mmlEffects";
import type { ToneModule, VoiceOutput } from "./mmlTimbres";

const r = (level: number, decayMs: number): ReverbParams => ({ level, decayMs });
const d = (level: number, timeMs: number, feedback: number): DelayParams => ({ level, timeMs, feedback });
const note = (track: number, startSec: number, fx: { reverb?: ReverbParams; delay?: DelayParams } = {}): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi: 60,
  velocity: 0.5,
  timbre: "fm",
  params: null,
  glide: null,
  reverb: fx.reverb ?? null,
  delay: fx.delay ?? null,
});

/** Tone のノードの代役。作ったもの・つないだ先・送りの設定を記録する */
function fakeTone(options: { reverbReady?: Promise<unknown> } = {}) {
  const nodes: FakeNode[] = [];
  class FakeNode {
    connected: unknown[] = [];
    destinations = 0;
    sets: [number, number][] = [];
    gain = { setValueAtTime: (v: number, t: number) => void this.sets.push([v, t]) };
    ready: Promise<unknown>;
    constructor(
      public kind: string,
      public args: unknown[],
    ) {
      this.ready = kind === "Reverb" && options.reverbReady ? options.reverbReady : Promise.resolve();
      nodes.push(this);
    }
    connect(to: unknown) {
      this.connected.push(to);
      return this;
    }
    toDestination() {
      this.destinations++;
      return this;
    }
  }
  const cls = (kind: string) =>
    class extends FakeNode {
      constructor(...args: unknown[]) {
        super(kind, args);
      }
    };
  const Tone = { Gain: cls("Gain"), Reverb: cls("Reverb"), FeedbackDelay: cls("FeedbackDelay") } as unknown as ToneModule;
  return { Tone, nodes, of: (kind: string) => nodes.filter((n) => n.kind === kind) };
}

describe("エフェクトの尾", () => {
  it("リバーブの尾は、長さ + プリディレイ。5 秒で頭打ち", () => {
    expect(reverbTailSec(2000)).toBeCloseTo(2 + REVERB_PREDELAY_SEC, 6);
    expect(reverbTailSec(5000)).toBe(MAX_EFFECT_TAIL_SEC);
  });

  it("ディレイの尾は、間隔 ×(1/1000 に下がるまでの繰り返し回数 + 1)。5 秒で頭打ち", () => {
    expect(delayTailSec(300, 0)).toBeCloseTo(0.3, 6); // 繰り返しなし: 1 回だけ
    expect(delayTailSec(300, 40)).toBeCloseTo(0.3 * 9, 6); // ceil(ln 0.001 / ln 0.4) = 8 回 + 元の 1
    expect(delayTailSec(1000, 90)).toBe(MAX_EFFECT_TAIL_SEC);
  });

  it("使ったエフェクトの尾の最大を返す。使わなければ 0", () => {
    expect(effectTailSec([note(0, 0), note(1, 1)])).toBe(0);
    expect(effectTailSec([note(0, 0, { reverb: r(30, 2000) })])).toBeCloseTo(2.01, 6);
    // ディレイ(2.7 秒)のほうが長い
    expect(effectTailSec([note(0, 0, { reverb: r(30, 2000) }), note(1, 0, { delay: d(25, 300, 40) })])).toBeCloseTo(2.7, 6);
  });
});

describe("createTrackEffects", () => {
  it("エフェクトを使わなければ、ノードを何も作らず、つなぎ先も無い。apply は何もしない", () => {
    const { Tone, nodes } = fakeTone();
    const fx = createTrackEffects(Tone, [note(0, 0), note(1, 0.5)]);
    expect(nodes).toHaveLength(0);
    expect(fx.outFor(0)).toBeUndefined();
    expect(() => fx.apply(note(0, 0))).not.toThrow();
  });

  it("使うトラックにだけバス(Gain)を作って出力へつなぎ、そのトラックの音源のつなぎ先にする", () => {
    const { Tone, of } = fakeTone();
    const fx = createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) }), note(1, 0)]);
    const bus = of("Gain")[0];
    expect(bus.args).toEqual([1]);
    expect(bus.destinations).toBe(1);
    expect(fx.outFor(0)).toBe(bus);
    expect(fx.outFor(1)).toBeUndefined();
  });

  it("リバーブは、長さと PreDelay を渡して作り、送り(Gain 0)→ リバーブ → 補正(Gain)→ 出力につなぐ", () => {
    const { Tone, of } = fakeTone();
    createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) })]);
    const [bus, send, trim] = of("Gain");
    const [reverb] = of("Reverb");
    expect(reverb.args).toEqual([{ decay: 2, preDelay: REVERB_PREDELAY_SEC }]);
    expect(send.args).toEqual([0]);
    expect(bus.connected).toEqual([send]);
    expect(send.connected).toEqual([reverb]);
    expect(reverb.connected).toEqual([trim]);
    expect(trim.args[0]).toBeCloseTo(10 ** (REVERB_TRIM_DB / 20), 9);
    expect(trim.destinations).toBe(1);
  });

  it("ディレイは、間隔(秒)・繰り返し(0〜1)・最大遅延 1 秒で作り、補正して出力へつなぐ", () => {
    const { Tone, of } = fakeTone();
    createTrackEffects(Tone, [note(0, 0, { delay: d(25, 300, 40) })]);
    const [, send, trim] = of("Gain");
    const [delay] = of("FeedbackDelay");
    expect(delay.args).toEqual([{ delayTime: 0.3, feedback: 0.4, maxDelay: 1 }]);
    expect(send.connected).toEqual([delay]);
    expect(delay.connected).toEqual([trim]);
    expect(trim.args[0]).toBeCloseTo(10 ** (DELAY_TRIM_DB / 20), 9);
  });

  it("組が同じなら 1 つを使い回し、長さや(間隔, 繰り返し)が違えば別に作る。量が違うだけなら同じ組", () => {
    const { Tone, of } = fakeTone();
    createTrackEffects(Tone, [
      note(0, 0, { reverb: r(30, 2000) }),
      note(0, 1, { reverb: r(60, 2000) }),
      note(0, 2, { reverb: r(30, 500) }),
      note(0, 3, { delay: d(25, 300, 40) }),
      note(0, 4, { delay: d(50, 300, 40) }),
      note(0, 5, { delay: d(25, 300, 60) }),
    ]);
    expect(of("Reverb")).toHaveLength(2);
    expect(of("FeedbackDelay")).toHaveLength(2);
    expect(of("Gain").filter((g) => g.args[0] === 1 && g.destinations === 1 && g.connected.length > 0)).toHaveLength(1); // バスは 1 つ
  });

  it("別のトラックは別のバスと別のエフェクトを持つ", () => {
    const { Tone, of } = fakeTone();
    const fx = createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) }), note(1, 0, { reverb: r(30, 2000) })]);
    expect(of("Reverb")).toHaveLength(2);
    expect(fx.outFor(0)).not.toBe(fx.outFor(1));
  });

  it("apply は、音の始まりの時刻に、その音の組の送りだけを量(0〜1)にし、ほかの組は 0 にする", () => {
    const { Tone, of } = fakeTone();
    const notes = [
      note(0, 0, { reverb: r(30, 2000) }),
      note(0, 0.5, { reverb: r(60, 2000) }),
      note(0, 1, { reverb: r(40, 500) }),
      note(0, 1.5),
    ];
    const fx = createTrackEffects(Tone, notes);
    for (const n of notes) fx.apply(n);
    const sends = of("Gain").filter((g) => g.args[0] === 0); // 作った順: 2000ms の組、500ms の組
    expect(sends[0].sets).toEqual([[0.3, 0], [0.6, 0.5], [0, 1]]);
    expect(sends[1].sets).toEqual([[0.4, 1], [0, 1.5]]);
  });

  it("同じ状態が続くときは、送りを重ねて設定しない(0 のままの組にも設定しない)", () => {
    const { Tone, of } = fakeTone();
    const notes = [note(0, 0, { reverb: r(30, 2000) }), note(0, 0.5, { reverb: r(30, 2000) }), note(0, 1, { reverb: r(30, 2000) })];
    const fx = createTrackEffects(Tone, notes);
    for (const n of notes) fx.apply(n);
    expect(of("Gain").find((g) => g.args[0] === 0)!.sets).toEqual([[0.3, 0]]);
  });

  it("リバーブとディレイを同時に使う音は、両方の送りを量にする", () => {
    const { Tone, of } = fakeTone();
    const n = note(0, 0.25, { reverb: r(30, 2000), delay: d(50, 300, 40) });
    const fx = createTrackEffects(Tone, [n]);
    fx.apply(n);
    const sends = of("Gain").filter((g) => g.args[0] === 0);
    expect(sends.map((s) => s.sets)).toEqual([[[0.3, 0.25]], [[0.5, 0.25]]]);
  });

  it("トラックをまたいで送りが混ざらない", () => {
    const { Tone, of } = fakeTone();
    const a = note(0, 0, { reverb: r(30, 2000) });
    const b = note(1, 0, { reverb: r(60, 2000) });
    const fx = createTrackEffects(Tone, [a, b]);
    fx.apply(a);
    fx.apply(b);
    const sends = of("Gain").filter((g) => g.args[0] === 0);
    expect(sends.map((s) => s.sets)).toEqual([[[0.3, 0]], [[0.6, 0]]]);
  });

  it("ready は、リバーブのインパルス応答の生成が終わるまで待つ", async () => {
    let release!: () => void;
    const { Tone } = fakeTone({ reverbReady: new Promise<void>((resolve) => (release = resolve)) });
    const fx = createTrackEffects(Tone, [note(0, 0, { reverb: r(30, 2000) })]);
    let done = false;
    void fx.ready().then(() => (done = true));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(done).toBe(false);
    release();
    await fx.ready();
    expect(done).toBe(true);
  });
});

// 型の確認用(VoiceOutput が Tone のノードであること)
const _typecheck: VoiceOutput | undefined = undefined;
void _typecheck;
```

- [ ] **Step 3: 失敗するテストを書く(`mmlTimbres.test.ts`)**

ファイル先頭の import を直す:
`import { TIMBRES, TIMBRE_LIST, glideVoice, type ToneModule } from "./mmlTimbres";` → `import { TIMBRES, TIMBRE_LIST, glideVoice, type ToneModule, type VoiceOutput } from "./mmlTimbres";`

ファイルの末尾に足す:

```ts
describe("エフェクトのバスへのつなぎ(Issue #499)", () => {
  const MELODIC: Timbre[] = ["fm", "square", "pulse", "triangle", "saw", "sine", "organ", "epiano", "bell", "am", "lead", "bass"];
  const out = { name: "bus" } as unknown as VoiceOutput;
  type Wired = { destinations: number; connected: unknown[] };
  const wired = (made: ReturnType<typeof fakeTone>["made"], cls: string) => made.find((m) => m.cls === cls)!.node as unknown as Wired;

  it("out を渡すと、旋律の 12 種は出力でなく out に繋ぐ", () => {
    for (const name of MELODIC) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3, null, out);
      const poly = wired(made, "PolySynth");
      expect(poly.destinations, name).toBe(0);
      expect(poly.connected, name).toEqual([out]);
    }
  });

  it("ビブラートがあるときは、ビブラートが out に繋ぎ、音源はビブラートに繋ぐ", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.fm.create(Tone, -3, { envelope: null, vibrato: { rateHz: 5, depth: 30 }, detune: 0 }, out);
    const vibrato = wired(made, "Vibrato");
    expect(vibrato.connected).toEqual([out]);
    expect(vibrato.destinations).toBe(0);
    const poly = wired(made, "PolySynth");
    expect(poly.destinations).toBe(0);
    expect(poly.connected).toEqual([made.find((m) => m.cls === "Vibrato")!.node]);
  });

  it("打楽器(drum・noise・drumkit)も out に繋ぐ", () => {
    const percussion: [Timbre, string][] = [["drum", "PolySynth"], ["noise", "NoiseSynth"], ["drumkit", "Volume"]];
    for (const [name, cls] of percussion) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3, null, out);
      const node = wired(made, cls);
      expect(node.destinations, name).toBe(0);
      expect(node.connected, name).toEqual([out]);
    }
  });

  it("out を渡さなければ、全 15 種が今までどおり出力へ繋ぐ", () => {
    const all: [Timbre, string][] = [
      ...MELODIC.map((n): [Timbre, string] => [n, "PolySynth"]),
      ["drum", "PolySynth"],
      ["noise", "NoiseSynth"],
      ["drumkit", "Volume"],
    ];
    for (const [name, cls] of all) {
      const { Tone, made } = fakeTone();
      TIMBRES[name].create(Tone, -3);
      const node = wired(made, cls);
      expect(node.destinations, name).toBe(1);
      expect(node.connected, name).toEqual([]);
    }
  });
});
```

- [ ] **Step 4: 失敗するテストを書く(`mmlRender.test.ts`)**

1. 先頭の import に `DelayParams` と `ReverbParams` を足し、`MAX_EFFECT_TAIL_SEC` を読み込む:

```ts
import type { DelayParams, Glide, MmlScore, NoteEvent, ReverbParams, Timbre, TimbreParams } from "../bindings/tauri.gen";
```
(元の import の型の並びに `DelayParams` と `ReverbParams` を足す。)

2. `created` の型に `out` を足す:

```ts
const created: { timbre: string; volumeDb: number; params: unknown; out: unknown; calls: Call[] }[] = [];
```

3. `created` の下(`vi.mock("./mmlTimbres", …)` の前)に、エフェクト用ノードの代役を足す:

```ts
/** Tone のエフェクト用ノードの代役。作ったもの・送りの設定を記録する */
const toneNodes: { kind: string; args: unknown[]; connected: unknown[]; destinations: number; sets: [number, number][] }[] = [];
let reverbReady: Promise<unknown> = Promise.resolve();
class FakeToneNode {
  connected: unknown[] = [];
  destinations = 0;
  sets: [number, number][] = [];
  gain = { setValueAtTime: (v: number, t: number) => void this.sets.push([v, t]) };
  ready: Promise<unknown>;
  constructor(
    public kind: string,
    public args: unknown[],
  ) {
    this.ready = kind === "Reverb" ? reverbReady : Promise.resolve();
    toneNodes.push(this);
  }
  connect(to: unknown) {
    this.connected.push(to);
    return this;
  }
  toDestination() {
    this.destinations++;
    return this;
  }
}
const toneClass = (kind: string) =>
  class extends FakeToneNode {
    constructor(...args: unknown[]) {
      super(kind, args);
    }
  };
```

4. 音源のモックの `create` が `out` を記録するようにする(`create: (_Tone: unknown, volumeDb: number, params?: unknown) => { const rec = { timbre: name, volumeDb, params, calls: [] as Call[] };` の部分):

```ts
    create: (_Tone: unknown, volumeDb: number, params?: unknown, out?: unknown) => {
      const rec = { timbre: name, volumeDb, params, out, calls: [] as Call[] };
```

5. `offline` のモックがコールバックの完了を待つようにする:

```ts
const offline = vi.fn(async (cb: () => void | Promise<void>, _dur: number, _ch: number, sr: number) => {
  await cb();
  return { get: () => ({ sampleRate: sr, getChannelData: () => new Float32Array([0, 0.5, -0.5]) }) };
});
```

6. `tone` のモックに、エフェクト用のクラスを足す:

```ts
vi.mock("tone", () => ({
  Offline: offline,
  Frequency: (midi: number) => ({ toFrequency: () => midi * 10 }),
  gainToDb: (g: number) => 20 * Math.log10(g),
  Gain: toneClass("Gain"),
  Reverb: toneClass("Reverb"),
  FeedbackDelay: toneClass("FeedbackDelay"),
}));
```

7. `beforeEach` に足す:

```ts
  toneNodes.length = 0;
  reverbReady = Promise.resolve();
```

8. ファイルの末尾に足す:

```ts
describe("renderScore: エフェクト(Issue #499)", () => {
  const withFx = (n: NoteEvent, fx: { reverb?: ReverbParams; delay?: DelayParams }): NoteEvent => ({
    ...n,
    reverb: fx.reverb ?? null,
    delay: fx.delay ?? null,
  });
  const reverb = (level: number, decayMs: number): ReverbParams => ({ level, decayMs });
  const delay = (level: number, timeMs: number, feedback: number): DelayParams => ({ level, timeMs, feedback });

  it("エフェクトを使わない MML は、音源のつなぎ先を渡さず、エフェクト用のノードも作らない", async () => {
    await renderScore(score([note(0, "fm"), note(1, "square")], 2));
    expect(created.every((c) => c.out === undefined)).toBe(true);
    expect(toneNodes).toHaveLength(0);
  });

  it("エフェクトを使うトラックだけ、音源をそのトラックのバスにつなぐ", async () => {
    await renderScore(score([withFx(note(0, "fm"), { reverb: reverb(30, 2000) }), note(1, "fm")], 2));
    const bus = toneNodes.find((n) => n.kind === "Gain")!;
    expect(created[0].out).toBe(bus);
    expect(created[1].out).toBeUndefined();
  });

  it("エフェクトの量・種類が違っても、音源の組は増えない", async () => {
    await renderScore(
      score([
        withFx(note(0, "fm", 0), { reverb: reverb(30, 2000) }),
        withFx(note(0, "fm", 0.5), { reverb: reverb(60, 500) }),
        withFx(note(0, "fm", 1), { delay: delay(25, 300, 40) }),
        note(0, "fm", 1.5),
      ]),
    );
    expect(created).toHaveLength(1);
    expect(created[0].calls).toHaveLength(4);
  });

  it("描画の長さは、曲の長さ + TAIL_SEC + エフェクトの尾(最大 5 秒)", async () => {
    await renderScore(score([withFx(note(0, "fm"), { reverb: reverb(30, 2000) })], 1, 2));
    expect(offline.mock.calls[0][1]).toBeCloseTo(2 + TAIL_SEC + 2.01, 6);
    offline.mockClear();
    await renderScore(score([withFx(note(0, "fm"), { delay: delay(100, 1000, 90) })], 1, 2));
    expect(offline.mock.calls[0][1]).toBeCloseTo(2 + TAIL_SEC + MAX_EFFECT_TAIL_SEC, 6);
  });

  it("送りは、各音の始まりの時刻に、その音の組の量にする。ほかの組は 0 にする", async () => {
    await renderScore(
      score([
        withFx(note(0, "fm", 0), { reverb: reverb(30, 2000) }),
        withFx(note(0, "fm", 0.5), { reverb: reverb(60, 2000) }),
        withFx(note(0, "fm", 1), { reverb: reverb(40, 500) }),
        note(0, "fm", 1.5),
      ]),
    );
    const sends = toneNodes.filter((n) => n.kind === "Gain" && n.args[0] === 0); // 2000ms の組、500ms の組
    expect(sends[0].sets).toEqual([[0.3, 0], [0.6, 0.5], [0, 1]]);
    expect(sends[1].sets).toEqual([[0.4, 1], [0, 1.5]]);
  });

  it("リバーブの準備(インパルス応答の生成)が終わるまで、音を鳴らさない", async () => {
    let release!: () => void;
    reverbReady = new Promise<void>((resolve) => (release = resolve));
    const done = renderScore(score([withFx(note(0, "fm"), { reverb: reverb(30, 2000) })]));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(created).toHaveLength(0);
    release();
    await done;
    expect(created[0].calls).toHaveLength(1);
  });
});
```
(`MAX_EFFECT_TAIL_SEC` を使うので、ファイル先頭に `import { MAX_EFFECT_TAIL_SEC } from "./mmlEffects";` を足す。`mmlEffects.ts` は型の import だけで Tone を読み込まないので、`vi.mock` と衝突しない。)

- [ ] **Step 5: 失敗を確かめる**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlEffects.test.ts src/lib/mmlTimbres.test.ts src/lib/mmlRender.test.ts 2>&1 | tail -30`
Expected: `mmlEffects.test.ts` は import できず失敗、`mmlTimbres.test.ts` の「エフェクトのバスへのつなぎ」が失敗(`out` が使われない)、`mmlRender.test.ts` の「エフェクト」が失敗(`mmlEffects` が無い、`out` が渡らない)。既存のテストは通る。

- [ ] **Step 6: 実装 — `mmlTimbres.ts`**

次の置き換えを、アサーション付きの Python で一括で行う(置き換え前の文字列が見つからない・回数が違うときは止まる)。`/tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/af5fb360-aecc-4ab1-94bb-9cff6a1a0969/scratchpad/edit_timbres.py` に書いて `python3 -I` で実行する:

```python
import sys
path = "/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/mmlTimbres.ts"
s = open(path, encoding="utf-8").read()

def rep(old, new, count=1):
    global s
    n = s.count(old)
    if n != count:
        sys.exit(f"anchor count {n} != {count}: {old[:70]!r}")
    s = s.replace(old, new)

# 1. 型
rep('export type ToneModule = typeof import("tone");\n',
    'export type ToneModule = typeof import("tone");\n\n'
    '/** 音源のつなぎ先(エフェクトのバス、Issue #499)。`create` の `out` で渡す。省略すると出力へつなぐ */\n'
    'export type VoiceOutput = import("tone").ToneAudioNode;\n')

# 2. create の型
rep('  /** `Tone.Offline` のコールバック内で呼ぶ。出力先と音量(`volumeDb`)を設定済みの音源を返す。`params` は旋律の音色だけが使う。 */\n'
    '  create(Tone: ToneModule, volumeDb: number, params?: TimbreParams | null): Voice;',
    '  /** `Tone.Offline` のコールバック内で呼ぶ。出力先と音量(`volumeDb`)を設定済みの音源を返す。`params` は旋律の音色だけが使う。`out` があれば、出力でなくそこへつなぐ(エフェクトのバス)。 */\n'
    '  create(Tone: ToneModule, volumeDb: number, params?: TimbreParams | null, out?: VoiceOutput): Voice;')

# 3. Fx に out
rep('  params: TimbreParams | null | undefined;\n  duo?: boolean;\n}',
    '  params: TimbreParams | null | undefined;\n  duo?: boolean;\n  /** 音源のつなぎ先。省略すると出力 */\n  out?: VoiceOutput;\n}')

# 4. connectOut を poly の前に
rep('/**\n * 出力に繋いで音量を設定する。',
    '/** `out` があればそこへ、無ければ出力へつなぐ */\n'
    'function connectOut(node: { connect(to: VoiceOutput): unknown; toDestination(): unknown }, out?: VoiceOutput): void {\n'
    '  if (out) node.connect(out);\n'
    '  else node.toDestination();\n'
    '}\n\n'
    '/**\n * 出力に繋いで音量を設定する。')

# 5. poly のつなぎ先
rep('    vibrato.toDestination();\n    synth.connect(vibrato);\n  } else {\n    synth.toDestination();\n  }',
    '    connectOut(vibrato, fx.out);\n    synth.connect(vibrato);\n  } else {\n    connectOut(synth, fx?.out);\n  }')

# 6. drumkit
rep('function drumkit(Tone: ToneModule, volumeDb: number): Voice {\n  const out = new Tone.Volume(volumeDb).toDestination();',
    'function drumkit(Tone: ToneModule, volumeDb: number, dest?: VoiceOutput): Voice {\n  const out = new Tone.Volume(volumeDb);\n  connectOut(out, dest);')

# 7. basic
rep('  (Tone: ToneModule, db: number, params?: TimbreParams | null): Voice =>\n'
    '    poly(glideSynth(Tone, Tone.Synth, { oscillator, envelope: BASIC_ENV }), db, { Tone, params });',
    '  (Tone: ToneModule, db: number, params?: TimbreParams | null, out?: VoiceOutput): Voice =>\n'
    '    poly(glideSynth(Tone, Tone.Synth, { oscillator, envelope: BASIC_ENV }), db, { Tone, params, out });')

# 8. 旋律の 7 つの create(fm・organ・epiano・bell・am・lead・bass)
rep('    create: (Tone, db, params) =>', '    create: (Tone, db, params, out) =>', 6)
rep('    create: (Tone, db, params) => {', '    create: (Tone, db, params, out) => {', 1)
rep('        { Tone, params },\n', '        { Tone, params, out },\n', 6)
rep('        { Tone, params, duo: true },', '        { Tone, params, out, duo: true },')

# 9. drum
rep('    create: (Tone, db) =>\n      poly(\n        new Tone.PolySynth(Tone.MembraneSynth, {\n'
    '          envelope: { attack: 0.001, decay: 0.3, sustain: 0, release: 0.3 },\n        }),\n        db,\n      ),',
    '    create: (Tone, db, _params, out) =>\n      poly(\n        new Tone.PolySynth(Tone.MembraneSynth, {\n'
    '          envelope: { attack: 0.001, decay: 0.3, sustain: 0, release: 0.3 },\n        }),\n        db,\n'
    '        { Tone, params: null, out },\n      ),')

# 10. noise
rep('    create: (Tone, db) => {\n      // NoiseSynth は単音用で PolySynth に包めない。音程は無視する。\n'
    '      const synth = new Tone.NoiseSynth({\n        noise: { type: "white" },\n'
    '        envelope: { attack: 0.001, decay: 0.15, sustain: 0, release: 0.05 },\n      }).toDestination();\n'
    '      synth.volume.value = db;',
    '    create: (Tone, db, _params, out) => {\n      // NoiseSynth は単音用で PolySynth に包めない。音程は無視する。\n'
    '      const synth = new Tone.NoiseSynth({\n        noise: { type: "white" },\n'
    '        envelope: { attack: 0.001, decay: 0.15, sustain: 0, release: 0.05 },\n      });\n'
    '      connectOut(synth, out);\n      synth.volume.value = db;')

# 11. drumkit のプリセット
rep('    create: (Tone, db) => drumkit(Tone, db),', '    create: (Tone, db, _params, out) => drumkit(Tone, db, out),')

open(path, "w", encoding="utf-8").write(s)
print("ok")
```

Run: `python3 -I /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/af5fb360-aecc-4ab1-94bb-9cff6a1a0969/scratchpad/edit_timbres.py`
Expected: `ok`。アンカーの不一致で止まったら、`mmlTimbres.ts` の該当箇所を読んで、同じ意味になるように Edit で直す(置き換えの意味は、旋律・打楽器の全 15 種の音源を `out` につなぐこと)。

- [ ] **Step 7: 実装 — `mmlEffects.ts`(新規)**

`frontend/src/lib/mmlEffects.ts`:

```ts
// MML のエフェクト(リバーブ・ディレイ、Issue #499)。トラックごとのバス、送りの切り替え、エフェクトの尾の長さ。
// 設計: docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md
import type { NoteEvent } from "../bindings/tauri.gen";
import type { ToneModule, VoiceOutput } from "./mmlTimbres";

/** `Tone.Reverb` のプリディレイ(秒)。Tone の既定と同じ。インパルス応答の長さは `decay + preDelay` */
export const REVERB_PREDELAY_SEC = 0.01;
/** エフェクトの尾(音源の余韻が終わった後に鳴り続ける時間)の上限(秒) */
export const MAX_EFFECT_TAIL_SEC = 5;
/** ディレイの繰り返しが、元の 1/1000(−60 dB)まで下がったら、尾を終える */
const DELAY_FLOOR = 0.001;
/** エフェクトの出力の補正(dB)。リバーブは畳み込みで音量が変わるので、実測で決める(計画の Task 2 のゲート) */
export const REVERB_TRIM_DB = 0;
export const DELAY_TRIM_DB = 0;

export function reverbTailSec(decayMs: number): number {
  return Math.min(MAX_EFFECT_TAIL_SEC, decayMs / 1000 + REVERB_PREDELAY_SEC);
}

export function delayTailSec(timeMs: number, feedbackPercent: number): number {
  const feedback = feedbackPercent / 100;
  const repeats = feedback <= 0 ? 0 : Math.ceil(Math.log(DELAY_FLOOR) / Math.log(feedback));
  return Math.min(MAX_EFFECT_TAIL_SEC, (timeMs / 1000) * (repeats + 1));
}

/** 使ったエフェクトの尾の最大(秒)。何も使わなければ 0 */
export function effectTailSec(notes: NoteEvent[]): number {
  let tail = 0;
  for (const n of notes) {
    if (n.reverb) tail = Math.max(tail, reverbTailSec(n.reverb.decayMs));
    if (n.delay) tail = Math.max(tail, delayTailSec(n.delay.timeMs, n.delay.feedback));
  }
  return tail;
}

type GainNode = InstanceType<ToneModule["Gain"]>;
type ToneNode = import("tone").ToneAudioNode;

/** (エフェクトの種類, 長さや間隔・繰り返し)の組 1 つ。`send` を量にした音だけが、このエフェクトへ送る */
interface Group {
  send: GainNode;
  /** その音が、この組へ送る量(0〜1) */
  level(n: NoteEvent): number;
}

interface TrackFx {
  /** そのトラックの音源の合流点。出力へも(ドライ)、各組の送りへもつなぐ */
  bus: GainNode;
  groups: Map<string, Group>;
  /** 組ごとに、最後に設定した送りの量(初期値 0) */
  sent: Map<Group, number>;
}

export interface TrackEffects {
  /** そのトラックの音源のつなぎ先。エフェクトを使わないトラックは undefined(出力へ直接つなぐ) */
  outFor(track: number): VoiceOutput | undefined;
  /** リバーブのインパルス応答の生成が終わるまで待つ(終わる前に音を鳴らすと、残響が付かない) */
  ready(): Promise<unknown>;
  /**
   * 音の始まりの時刻に、その音の組の送りだけを量にし、ほかの組を 0 にする。
   * 音は、トラックごとに時刻の昇順で渡すこと(同じ状態を重ねて設定しないため、直前の設定を覚えている)。
   */
  apply(n: NoteEvent): void;
}

const gainOf = (db: number) => 10 ** (db / 20);

/**
 * エフェクトを使うトラックにだけ、バスと、組ごとの「送り → エフェクト → 補正 → 出力」を作る。
 * `Tone.Offline` のコールバックの中で、音源を作る前に呼ぶ。
 */
export function createTrackEffects(Tone: ToneModule, notes: NoteEvent[]): TrackEffects {
  const tracks = new Map<number, TrackFx>();
  const pending: Promise<unknown>[] = [];

  const fxFor = (track: number): TrackFx => {
    let fx = tracks.get(track);
    if (!fx) {
      fx = { bus: new Tone.Gain(1).toDestination(), groups: new Map(), sent: new Map() };
      tracks.set(track, fx);
    }
    return fx;
  };

  const addGroup = (fx: TrackFx, key: string, effect: ToneNode, trimDb: number, level: Group["level"]) => {
    const send = new Tone.Gain(0);
    const trim = new Tone.Gain(gainOf(trimDb)).toDestination();
    fx.bus.connect(send);
    send.connect(effect);
    effect.connect(trim);
    fx.groups.set(key, { send, level });
  };

  for (const n of notes) {
    const rv = n.reverb;
    if (rv) {
      const fx = fxFor(n.track);
      const key = `reverb:${rv.decayMs}`;
      if (!fx.groups.has(key)) {
        const reverb = new Tone.Reverb({ decay: rv.decayMs / 1000, preDelay: REVERB_PREDELAY_SEC });
        pending.push(reverb.ready);
        addGroup(fx, key, reverb, REVERB_TRIM_DB, (x) => (x.reverb && x.reverb.decayMs === rv.decayMs ? x.reverb.level / 100 : 0));
      }
    }
    const dl = n.delay;
    if (dl) {
      const fx = fxFor(n.track);
      const key = `delay:${dl.timeMs}:${dl.feedback}`;
      if (!fx.groups.has(key)) {
        const delay = new Tone.FeedbackDelay({ delayTime: dl.timeMs / 1000, feedback: dl.feedback / 100, maxDelay: 1 });
        addGroup(fx, key, delay, DELAY_TRIM_DB, (x) =>
          x.delay && x.delay.timeMs === dl.timeMs && x.delay.feedback === dl.feedback ? x.delay.level / 100 : 0,
        );
      }
    }
  }

  return {
    outFor: (track) => tracks.get(track)?.bus,
    ready: () => Promise.all(pending),
    apply(n) {
      const fx = tracks.get(n.track);
      if (!fx) return;
      const t = n.startSec ?? 0;
      for (const g of fx.groups.values()) {
        const level = g.level(n);
        if ((fx.sent.get(g) ?? 0) === level) continue;
        g.send.gain.setValueAtTime(level, t);
        fx.sent.set(g, level);
      }
    },
  };
}
```

- [ ] **Step 8: 実装 — `mmlRender.ts`**

import を足す:

```ts
import { createTrackEffects, effectTailSec } from "./mmlEffects";
```

`renderScore` を次のようにする(変更点: コールバックを `async` に、エフェクトを作って待つ、`create` に `out` を渡す、各音の前に `apply`、描画の長さに尾を足す):

```ts
export async function renderScore(score: MmlScore): Promise<string> {
  const Tone = await import("tone");
  const buffer = await Tone.Offline(
    async () => {
      // 複数トラックを足すと 1.0 を超えて WAV 化で頭打ちになるので、トラック数で割って揃える(単一トラックは補正なし)。
      const trackDb = score.trackCount > 1 ? Tone.gainToDb(1 / score.trackCount) : 0;
      // エフェクト(Issue #499)。使うトラックにだけバスを作る(使わないトラックは、今までどおり音源を出力へ直接つなぐ)。
      // リバーブのインパルス応答の生成が終わるまで待ってから、音源を作って鳴らす。
      const effects = createTrackEffects(Tone, score.notes);
      await effects.ready();
      // 音源は (トラック, 音色) ごとに最初に使うときに作る。
      const voices = new Map<string, Voice>();
      const voiceFor = (track: number, timbre: Timbre, noteParams: TimbreParams | null): Voice => {
        const preset = TIMBRES[timbre];
        const volumeDb = preset.gainDb + trackDb;
        // 打楽器は、パラメーターを無視する(パラメーターが違っても 1 つの音源を使い回す)。
        // デチューンは音源に渡さず、鳴らす周波数の倍率にする(下)。音源の組は、エンベロープとビブラートだけで決める
        const params =
          preset.percussive || !noteParams || (!noteParams.envelope && !noteParams.vibrato)
            ? null
            : { envelope: noteParams.envelope, vibrato: noteParams.vibrato, detune: 0 };
        // つなぎ先はトラックごとに決まるので、音源の組(下の key)には入れない
        const out = effects.outFor(track);
        if (preset.perNote) return preset.create(Tone, volumeDb, params, out);
        // PolySynth は描画中に設定を時刻つきで変えられないので、パラメーターの組ごとに別の音源を作る
        const key = `${track}:${timbre}:${params === null ? "" : JSON.stringify(params)}`;
        let voice = voices.get(key);
        if (!voice) {
          voice = preset.create(Tone, volumeDb, params, out);
          voices.set(key, voice);
        }
        return voice;
      };
      for (const n of score.notes) {
        // 音の始まりの時刻に、エフェクトへの送りを切り替える(音は、トラックごとに時刻の昇順)
        effects.apply(n);
        const percussive = TIMBRES[n.timbre].percussive;
        // デチューン(セント)は、周波数に 2^(セント/1200) を掛ける。打楽器は無視する
        const cents = percussive ? 0 : (n.params?.detune ?? 0);
        const ratio = cents === 0 ? 1 : 2 ** (cents / 1200);
        const hz = Tone.Frequency(n.midi, "midi").toFrequency() * ratio;
        // ポルタメント: 滑りの長さは音の長さ以下に縮める。滑り始めにも、同じ倍率を掛ける
        const durationSec = n.durationSec ?? 0;
        const slideSec = n.glide ? Math.min(n.glide.ms / 1000, durationSec) : 0;
        const glide =
          n.glide && !percussive && slideSec > 0
            ? { fromHz: Tone.Frequency(n.glide.fromMidi, "midi").toFrequency() * ratio, sec: slideSec }
            : undefined;
        voiceFor(n.track, n.timbre, n.params ?? null).trigger(hz, durationSec, n.startSec ?? 0, n.velocity ?? 0, n.midi, glide);
      }
    },
    // 音源の余韻(TAIL_SEC)の後に、エフェクトの尾(使わなければ 0)が続く
    (score.totalSec ?? 0) + TAIL_SEC + effectTailSec(score.notes),
    1,
    SAMPLE_RATE,
  );
  const audio = buffer.get();
  if (!audio) throw new Error("レンダリング結果が空でした");
  return wavToDataUrl(encodeWavMono16([audio.getChannelData(0)], audio.sampleRate));
}
```
(`for` の中身は、`apply` の 1 行と、その前のコメントを足した以外、現在の `mmlRender.ts` のままにする。上の内容と現在のファイルに違いがあれば、現在のファイルを正とする。ファイル先頭のコメントに「エフェクトのバスは `mmlEffects.ts`」と 1 行足す。)

- [ ] **Step 9: テストと型を通す、設計書を直す、コミット**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlEffects.test.ts src/lib/mmlTimbres.test.ts src/lib/mmlRender.test.ts 2>&1 | tail -15; pnpm check 2>&1 | tail -4`
Expected: すべて通過、`0 errors`(既存の警告 1 は残る)。型エラーが出たら(`new Tone.Reverb({...})` のオプション名、`Gain.connect` の引数の型など)、`node_modules/tone/build/esm/` の `.d.ts` を読んで、型だけを直す(動作は変えない)。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run 2>&1 | tail -6`
Expected: 全ファイル通過(失敗 0)。

設計書を、実装に合わせて直す(Ruling: 音源の余韻 `TAIL_SEC` の後にエフェクトの尾が続くので、描画の長さは `曲の長さ + TAIL_SEC + エフェクトの尾`。設計書の「`曲の長さ + 尾`、尾は最大値(`TAIL_SEC` 以上)」だと、最後の音のリリースの後のエフェクトの尾が切れる)。`docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md` を Edit で 2 か所直す:

1. 「決定」の表の「余韻(尾)」の行を:
`| 余韻(尾) | 描画の長さ = 曲の長さ + `TAIL_SEC`(0.6 秒。音源の余韻)+ エフェクトの尾。エフェクトの尾は、使ったエフェクトの尾の最大(最大 5 秒で頭打ち)。何も使わなければ 0(今のまま) | 残響・やまびこが切れずに入るため。最後の音のリリースが終わった後に、エフェクトの尾が始まる。90 秒の上限は、今までどおり曲の長さにかかる(尾は外)。 |`
に置き換える。

2. 「フロントの変更 → 描画」の「**余韻(尾)**」の項を:
`- **余韻(尾)**: `renderScore` の描画の長さを、`(score.totalSec ?? 0) + TAIL_SEC + tail` にする。`tail` は、実際に使ったエフェクトの尾の最大。リバーブの尾 = 長さ + プリディレイ(0.01 秒)。ディレイの尾 = 間隔 × (繰り返し回数 + 1)、繰り返し回数 = 音量が 1/1000 に下がるまでの回数(`ceil(ln 0.001 / ln(繰り返し))`、繰り返し 0 なら 0)。どちらも最大 5 秒で頭打ち。使わなければ 0(`TAIL_SEC` だけ)。`
に置き換える。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/mmlEffects.ts frontend/src/lib/mmlEffects.test.ts frontend/src/lib/mmlTimbres.ts frontend/src/lib/mmlTimbres.test.ts frontend/src/lib/mmlRender.ts frontend/src/lib/mmlRender.test.ts docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md
git commit -m "feat: MML のエフェクトを、トラックのバスと送りで描画する(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 10: ゲート — 実際の描画で、リバーブとディレイが動くことを実測する**

**このゲートに通らなければ、先に進まない。** 通らない項目があれば、台帳に結果を書いて**止め、ユーザーに報告して相談**する(spec の「未確認・リスク」の最初の項目。特に a の `Tone.Reverb` が `Tone.Offline` の中で動くこと)。

a. 準備。ヘルパーのスクリプトは、前回のセッションのものが残っていないので作る(`SP` はスクラッチパッド、`H=/tmp/tsv499k` は隔離ホーム)。

```bash
SP=/tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/af5fb360-aecc-4ab1-94bb-9cff6a1a0969/scratchpad
H=/tmp/tsv499k; rm -rf $H; mkdir -p $H; echo $SP > $H/sp
cat > $H/lib.sh <<'EOF'
H=/tmp/tsv499k; export DISPLAY=:95
br(){ curl -s --unix-socket $H/cache/com.onodai.tsumugi/debug-bridge.sock http://localhost/ --data-binary "$1" | jq -r .; }
click(){ br "[...document.querySelectorAll('button, label, [role=radio]')].find(b=>b.textContent.trim().startsWith('$1')).click(); return 1" >/dev/null; sleep ${2:-0.8}; }
typemml(){ br 'document.querySelector(".cm-content").focus(); return 1' >/dev/null; xdotool key ctrl+a BackSpace; xdotool type --delay 20 -- "$1"; sleep 0.8; }
preview(){ br 'window.__n=(window.__h||[]).length; return 1' >/dev/null; br '[...document.querySelectorAll("button")].find(b=>b.textContent.trim()==="試聴").click(); return 1' >/dev/null; for w in $(seq 1 ${1:-30}); do sleep 1; r=$(br 'return String((window.__h||[]).length>window.__n)'); case "$r" in *'"value":"true"'*) break;; esac; done; br 'return window.__h[window.__h.length-1]' | jq -r .value > $H/last.b64 2>/dev/null || true; echo "waited ${w}s"; }
meas(){ typemml "$2"; preview ${3:-30} >/dev/null; echo -n "$1: "; python3 -I $(cat $H/sp)/wav.py $H/last.b64; }
EOF
cat > $SP/stop.sh <<'EOF'
#!/bin/bash
# 隔離インスタンスを、トークンで特定した PID だけ止める
T=$(cat /tmp/tsv499k/token); pids=""
for d in /proc/[0-9]*; do p=${d#/proc/}; [ "$p" = "$$" ] && continue; grep -qaz "TSUMUGI_VERIFY_TOKEN=$T" $d/environ 2>/dev/null && pids="$pids $p"; done
[ -n "$pids" ] && kill -TERM $pids 2>/dev/null; sleep 3
for p in $pids; do if [ -d /proc/$p ] && grep -qaz "TSUMUGI_VERIFY_TOKEN=$T" /proc/$p/environ 2>/dev/null; then kill -KILL $p; fi; done
sleep 1; for p in $pids; do [ -d /proc/$p ] && echo "alive $p"; done
rm -f /tmp/.X95-lock /tmp/.X11-unix/X95; echo stopped
EOF
cat > $SP/launch.sh <<'EOF'
#!/bin/bash
# 隔離インスタンスを起動し、設定→通知→MML まで進めて、fetch を包む
cd /home/onodai145/repos/github.com/onodai145/tsumugi
H=/tmp/tsv499k
mkdir -p $H/config/com.onodai.tsumugi $H/cache $H/data
[ -f $H/config/com.onodai.tsumugi/settings.json ] || echo '{"accounts":[{"id":"00000000-0000-4000-8000-000000000499","host":"misskey.invalid","username":"verify","userId":"verifyuser","displayName":"verify","avatarUrl":null}]}' > $H/config/com.onodai.tsumugi/settings.json
export TSUMUGI_VERIFY_TOKEN=$(cat /proc/sys/kernel/random/uuid); echo $TSUMUGI_VERIFY_TOKEN > $H/token
(setsid Xvfb :95 -screen 0 1280x900x24 >$H/xvfb.log 2>&1 &); sleep 1.5
(unset WAYLAND_DISPLAY; export DISPLAY=:95 GDK_BACKEND=x11 XDG_CONFIG_HOME=$H/config XDG_CACHE_HOME=$H/cache XDG_DATA_HOME=$H/data; setsid dbus-run-session -- src-tauri/target/debug/tsumugi >$H/app.log 2>&1 &)
. $H/lib.sh
for i in $(seq 1 40); do r=$(br 'return document.querySelector("[data-testid=app-menu-trigger]") ? "YES":"NO"' 2>/dev/null); case "$r" in *YES*) break;; esac; sleep 8; done
br 'document.querySelector("[data-testid=app-menu-trigger]").click(); return 1' >/dev/null; sleep 1; click 設定 1.5; click 通知 1.2
br '[...document.querySelectorAll("label")].find(l=>l.textContent.includes("通知音を鳴らす")).querySelector("input").click(); return 1' >/dev/null; sleep 0.8; click ビープ
br '[...document.querySelectorAll("[role=listbox] button")].find(b=>b.textContent.includes("MML")).click(); return 1' >/dev/null; sleep 1.5
br 'window.__h=[]; const of=window.fetch; window.fetch=function(u,o){ try{ if(String(u&&u.url||u).includes("play_notify_sound")) window.__h.push(String(o.body)); }catch(e){} return of.apply(this,arguments)}; return "hooked"'
EOF
cat > $SP/wav.py <<'EOF'
import sys, json, base64, struct, math
raw = open(sys.argv[1]).read().strip()
if not raw: print("EMPTY"); sys.exit(1)
d = json.loads(raw) if raw.startswith("{") else {"choice": raw}
b = base64.b64decode(d["choice"].split(",",1)[1])
n = struct.unpack("<I", b[40:44])[0] // 2
s = struct.unpack("<%dh" % n, b[44:44+2*n])
x = [v/32768 for v in s]
peak = max(abs(v) for v in x) if x else 0
rms = math.sqrt(sum(v*v for v in x)/len(x)) if x else 0
bad = sum(1 for v in x if v != v)
tail = max((abs(v) for v in x[-int(0.05*22050):]), default=0)
print(f"samples={n} sec={n/22050:.2f} peak={peak:.4f} rms={rms:.5f} nan={bad} tailmax={tail:.4f}")
EOF
cat > $SP/env.py <<'EOF'
import sys, json, base64, struct, math
raw = open(sys.argv[1]).read().strip()
d = json.loads(raw) if raw.startswith("{") else {"choice": raw}
b = base64.b64decode(d["choice"].split(",",1)[1])
n = struct.unpack("<I", b[40:44])[0] // 2
x = [v/32768 for v in struct.unpack("<%dh" % n, b[44:44+2*n])]
sr = 22050
for w in sys.argv[2:]:
    t0, t1 = map(float, w.split(":"))
    seg = x[int(t0*sr):int(t1*sr)]
    rms = math.sqrt(sum(v*v for v in seg)/len(seg)) if seg else 0
    pk = max((abs(v) for v in seg), default=0)
    print(f"{t0:.2f}-{t1:.2f}s  rms={rms:.5f}  peak={pk:.4f}")
EOF
chmod +x $SP/*.sh
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm build 2>&1 | tail -2
cd ../src-tauri && cargo build 2>&1 | tail -2
cd .. && (bash $SP/launch.sh > $H/launch.log 2>&1; echo done >> $H/launch.log) > /dev/null 2>&1 &
```

(`cargo build` は 120 秒を超えるので、バックグラウンドで実行し、終わりを待つ。Step 1 でベースラインを取ったときと違い、ここは Task 1・2 の変更を含めてビルドする。)

起動に 2〜3 分かかる。`tail $H/launch.log` に `done` が出るまで、間をあけて見る(`sleep` で待たず、別のことをするか、短い確認を繰り返す)。

(以降の b〜h で使う `$SP` と `$H` は、a で定義したもの。新しいシェルで続けるときは、先に `SP=<上のスクラッチパッド>; H=/tmp/tsv499k` を定義し直す。)

b. 変更前との比較(Step 1 で `baseline.txt` を取れていたら):

```bash
. $H/lib.sh
for tb in 0 1 9 10 11; do typemml "@$tb t120 c4 e4 g4"; preview 60; sha256sum $H/last.b64; done
```
Expected: `baseline.txt` と、5 つとも同じハッシュ(`x` `z` を書かない MML の結果が変わらない)。違ったら、`wav.py` でピーク・RMS・サンプル数を比べ、同じ MML を 2 回測って、変更前同士でハッシュが安定していたかも確かめる(安定していなければ、波形の値の一致で判断する)。

c. リバーブが動く(Review Focus 4 の一部)。`@5` は純音のサイン波で、`c4` は 0.5 秒(t120)。

```bash
. $H/lib.sh
typemml "@5 t120 c4"; preview 60; echo "== リバーブなし"; python3 -I $SP/wav.py $H/last.b64; python3 -I $SP/env.py $H/last.b64 0.0:0.3 0.6:0.8 1.5:1.7
typemml "@5 t120 x100,3000 c4"; preview 90; echo "== x100,3000"; python3 -I $SP/wav.py $H/last.b64; python3 -I $SP/env.py $H/last.b64 0.0:0.3 0.6:0.8 1.0:1.2 2.0:2.2 3.0:3.2 3.3:3.5
```

**合格の条件**:
- リバーブなしの WAV の長さは 0.5 + 0.6 = 1.1 秒前後、`0.6:0.8` の rms はほぼ 0。
- `x100,3000` の WAV の長さは 0.5 + 0.6 + 3.01 = 4.11 秒前後(±0.15)。
- 音の終わり(0.5 秒)より後の `0.6:0.8` の rms が、リバーブなしのときの `0.6:0.8` の 10 倍以上(リバーブが付いている)。
- `1.0:1.2` → `2.0:2.2` → `3.0:3.2` と、rms が**減っていく**(残響が減衰する)。
- 最後の 50ms の最大(`tailmax`)が、ピーク(`peak`)の 10 % 以下(終端で急に切れていない)。

d. ディレイが動く。`c16` は 0.125 秒(t120)で、間隔 250ms、繰り返し 50%。

```bash
. $H/lib.sh
typemml "@5 t120 z100,250,50 c16"; preview 90; python3 -I $SP/wav.py $H/last.b64; python3 -I $SP/env.py $H/last.b64 0.0:0.12 0.25:0.37 0.50:0.62 0.75:0.87 1.0:1.12
```
**合格の条件**:
- WAV の長さが 0.125 + 0.6 + 2.75 = 3.475 秒前後(±0.15)。
- 元の音(`0.0:0.12`)に対する、最初のやまびこ(`0.25:0.37`)の rms の比が 0.7〜1.3。
- やまびこ 2 つ目 / 1 つ目、3 つ目 / 2 つ目 の rms の比が、どちらも 0.5 ± 0.12。
- `tailmax` が `peak` の 10 % 以下。

e. 途中で止める・変える(Review Focus 2):

```bash
. $H/lib.sh
typemml "@5 t120 c4 x100,500 c4 x0 r4 r4 r4 c4"; preview 90; python3 -I $SP/wav.py $H/last.b64; python3 -I $SP/env.py $H/last.b64 0.1:0.3 1.1:1.3 1.7:1.9 3.1:3.3
```
(`c4` は 0〜0.5 秒、リバーブ付きの `c4` は 0.5〜1.0 秒、`x0`、休符 1.0〜2.5 秒、最後の `c4` は 2.5〜3.0 秒。)
**合格の条件**: `1.1:1.3`(リバーブ付きの音の直後)の rms が大きく、`1.7:1.9`(長さ 500ms の残響が消えた後)は `1.1:1.3` の 5 % 以下。`3.1:3.3`(`x0` の後の音の直後)の rms は、`1.1:1.3` の 5 % 以下(リバーブが付いていない)。

f. 音量とピーク、15 種の音色(NaN・ピーク):

```bash
. $H/lib.sh
for tb in $(seq 0 14); do meas "@$tb 無し" "@$tb t120 c4" 60; meas "@$tb x100,5000 z100,1000,90" "@$tb t120 x100,5000 z100,1000,90 c4" 120; done
```
**合格の条件**: すべて `nan=0`、`peak` ≤ 1.0。**1 発のピークの条件**: エフェクトありの `peak` が 0.36 以下(ノイズ系の `@13` `@14` は実行ごとに揺れるので 0.40 以下を許す)。超えた音色があるときは、その音色のエフェクトなしの `peak` と比べ、エフェクトが原因なら `mmlEffects.ts` の `REVERB_TRIM_DB` / `DELAY_TRIM_DB` を下げて(例: −3 dB ずつ)ビルドし直して測り直す。補正を変えた値は、台帳と、設計書の「実装後の確認」に記録する。調整しても 0.36 に収まらないときは**止めて報告**する(spec の「ドライとウェットの音量差」)。

g. 複数トラック:

```bash
. $H/lib.sh
meas "3 トラック" "@0 x30,2000 c4 e4 ; @1 z25,300,40 g4 b4 ; @5 c4 e4" 90
```
**合格の条件**: `nan=0`、`peak` ≤ 1.0。

h. 結果(各項目の窓の値・ハッシュの一致・補正値)を台帳に書く。インスタンスを止める: `bash $SP/stop.sh; rm -rf /tmp/tsv499k`。

---

### Task 3: 字句解析・入力補助キー・ヘルプ・ガイド

**Files:**
- Modify: `frontend/src/lib/mmlLexer.ts`、`frontend/src/lib/mmlLexer.test.ts`
- Modify: `frontend/src/lib/mmlKeys.ts`、`frontend/src/lib/mmlKeys.test.ts`
- Modify: `frontend/src/ui/settings/SoundChoicePicker.svelte`、`frontend/src/ui/settings/SoundChoicePicker.test.ts`
- Modify: `docs/guide/user-guide.md`

**Interfaces:**
- Consumes: Task 1 で `x` `z` が MML として通ること。
- Produces: なし(UI の追随だけ)。

- [ ] **Step 1: 失敗するテストを書く**

`mmlLexer.test.ts`:

1. 「構文が誤っていても例外を出さず、最後まで分類する」のテストで、`x` と `@zzz` を「読めない文字」の例として使っているので、`j` に変える(`x` `z` が命令になるため):

```ts
    expect(lex("c4 j")).toEqual([
      ["c", "note"],
      ["4", "number"],
      ["j", "unknown"],
    ]);
    expect(lex("@")).toEqual([["@", "timbre"]]);
    expect(lexMml("@jjj").map((t) => t.kind)).toEqual(["timbre", "unknown", "unknown", "unknown"]);
```
(元の `lex("c4 x")` の期待の `["x", "unknown"]` と、`lexMml("@zzz")` の行を、この形に置き換える。)

2. 「ポルタメント p は command、数字は number」のテストの後に足す:

```ts
  it("エフェクト x z は command、数字は number、カンマは command(Issue #499)", () => {
    expect(lex("x30,2000 z25,300,40 x0 z0")).toEqual([
      ["x", "command"],
      ["30", "number"],
      [",", "command"],
      ["2000", "number"],
      ["z", "command"],
      ["25", "number"],
      [",", "command"],
      ["300", "number"],
      [",", "command"],
      ["40", "number"],
      ["x", "command"],
      ["0", "number"],
      ["z", "command"],
      ["0", "number"],
    ]);
  });
```

`mmlKeys.test.ts`:
- `expect(inserts("params")).toEqual(["y", "m", "k", "p", ","]);` → `expect(inserts("params")).toEqual(["y", "m", "k", "p", "x", "z", ","]);`
- キーだけで打った列のテストの説明文 `k 2 p 8 c $ c` → `k 2 p 8 x 4 , 8 z 4 c $ c`、列の `"param-p", "len-8", "c",` → `"param-p", "len-8", "param-x", "len-4", "param-comma", "len-8", "param-z", "len-4", "c",`。

`SoundChoicePicker.test.ts` の「記法ヘルプに、ポルタメント p の…」のテストの後に足す:

```ts
  it("記法ヘルプに、リバーブ x とディレイ z の書き方・範囲・解除・種類の上限が出る(Issue #499)", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    expect(text).toContain("x30,2000");
    expect(text).toContain("リバーブ");
    expect(text).toContain("z25,300,40");
    expect(text).toContain("ディレイ");
    expect(text).toContain("x0 z0");
    expect(text).toContain("4 種");
  });
```

- [ ] **Step 2: 失敗を確かめる**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | tail -25`
Expected: 3 ファイルで失敗(`x` `z` が unknown のまま、キーが無い、ヘルプに無い)。

- [ ] **Step 3: 実装**

`mmlLexer.ts`: 音色パラメーターの枝の条件とコメントを直す。

```ts
    } else if (ch === "y" || ch === "m" || ch === "k" || ch === "x" || ch === "z") {
      // 音色パラメーター(Issue #499): `y a,d,s,r` `m 速さ,深さ` `k セント`、エフェクト: `x 量,長さ` `z 量,間隔,繰り返し`。`k+` `k-` は符号まで、カンマは command にする
```
(元の `} else if (ch === "y" || ch === "m" || ch === "k") {` と、その次のコメント行を置き換える。枝の中身は変えない。)

`mmlKeys.ts`: `params` グループの `param-p` の後(カンマの前)に足す:

```ts
      { id: "param-x", label: "x", insert: "x", aria: "リバーブ x" },
      { id: "param-z", label: "z", insert: "z", aria: "ディレイ z" },
```

`SoundChoicePicker.svelte`: `p80 ポルタメント…` の行の直後に、1 行足す:

```
x30,2000 リバーブ(量 1〜100 %、残響の長さ 100〜5000 ミリ秒)  z25,300,40 ディレイ(量 1〜100 %、間隔 50〜1000 ミリ秒、繰り返し 0〜90 %)。x0 z0 で解除。トラックの状態で、次の音から効く。和音の中には書けない。打楽器にも効く。長さ・間隔・繰り返しが違うものは、1 トラックにリバーブ 4 種、ディレイ 4 種まで。残響の尾の分、再生が最大 5 秒ほど長くなる
```

`docs/guide/user-guide.md`:
1. 入力補助キーの文の `音色パラメーター \`y\` \`m\` \`k\` \`p\` \`,\`` を `音色パラメーター \`y\` \`m\` \`k\` \`p\` \`x\` \`z\` \`,\`` に変える。
2. 「ポルタメント」の節(最後の箇条書き `- \`q\` でゲートを短くしても、…` まで)の直後、「保存した MML は…」の段落の直前に、新しい節を足す:

```
**エフェクト**: `x`(リバーブ)と `z`(ディレイ)で、トラックごとに残響とやまびこを付けられます。`y` `m` `k` `p` と同じトラックの状態で、書いた所から後の音に効き、ループやマクロの中にも書けます。和音の中には書けません(和音の前に書きます)。`drum` `noise` `drumkit` にも効きます。

| 書き方 | 意味 |
|---|---|
| `x30,2000` | リバーブ。量(1〜100 %)、残響の長さ(ミリ秒、100〜5000)。`x0` で解除します |
| `z25,300,40` | ディレイ。量(1〜100 %)、間隔(ミリ秒、50〜1000)、繰り返し(0〜90 %。大きいほど、長く繰り返します)。`z0` で解除します |

例: `@lead x30,2000 z25,300,40 o4 c4 e4 g4 x0 z0 c4`(最後の `c` には効きません。解除した後も、すでに鳴っている残響とやまびこは、自然に消えるまで続きます)。

- 量が大きいほど、エフェクトの音が大きくなります。
- 残響の長さや、ディレイの間隔・繰り返しを変えて書くと、別のエフェクトとして鳴ります。1 トラックに書ける種類は、リバーブが 4 種類、ディレイが 4 種類までです(量だけが違うものは同じ種類です)。
- 残響とやまびこが消えるまで再生が続くので、曲の長さより、最大 5 秒ほど長くなります(90 秒の上限は、曲の長さにかかります)。
- リバーブは、再生のたびに響きが少し変わります。
```

- [ ] **Step 4: テストと型を通す**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run 2>&1 | tail -6; pnpm check 2>&1 | tail -4`
Expected: 全テスト通過、0 errors。落ちたテストが、`x` / `z` を「読めない文字」の例に使っているだけのものなら、`j` に直す(それ以外の理由で落ちたら、直さずに止める)。

- [ ] **Step 5: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts frontend/src/lib/mmlKeys.ts frontend/src/lib/mmlKeys.test.ts frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md
git commit -m "docs: MML のリバーブ x とディレイ z を、色分け・入力補助キー・ヘルプ・ガイドに追記(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 実機での通し確認と、実装後の確認の記録

**Files:**
- Modify: `docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md`(「実装後の確認」を足す)

**Interfaces:**
- Consumes: Task 1〜3 のすべて。
- Produces: spec の「実装後の確認」節。

- [ ] **Step 1: ビルドして、隔離インスタンスを起動する**

Task 2 の Step 10 の a と同じ(`lib.sh` `stop.sh` `launch.sh` `wav.py` `env.py` は、Task 2 で作ったものがスクラッチパッドと `/tmp/tsv499k` にあれば作り直さない。無ければ作る)。`pnpm build` → `cargo build` → `launch.sh` をバックグラウンドで起動。ビルドは Task 3 の変更を含める。

- [ ] **Step 2: 極端な値と、組の数の上限**

```bash
. $H/lib.sh
meas "最長リバーブ" "@5 t120 x100,5000 c4" 90
meas "最長ディレイ" "@5 t120 z100,1000,90 c4" 90
meas "組を最大(4+4)" "@0 t120 x30,500 z10,100,10 c8 x30,1000 z10,100,20 c8 x30,1500 z10,200,10 c8 x30,2000 z10,200,20 c8 x30,500 z10,100,10 c8" 120
```
Expected: どれも描画が終わり、`nan=0`、`peak` ≤ 1.0。最長リバーブの `sec` は 0.5 + 0.6 + 5 = 6.1 前後、最長ディレイの `sec` も 6.1 前後(尾が 5 秒で頭打ち)。最長ディレイの終端 `tailmax` が `peak` に対して大きい(切れている)ときは、その値を記録する(設計書の「尾が 5 秒で打ち切られること」のリスク)。

5 種目を書くと、エラーになることを UI で見る: `x30,500 c x30,1000 c x30,1500 c x30,2000 c x30,2500 c` を入力して、「N 文字目」(5 種目の `x` の位置)とメッセージ(リバーブは 4 種類まで)が出ること。「N 文字目」を押すと、カーソルがその位置に移ること。

- [ ] **Step 3: 描画時間**

`@0 t240 l64` で、2800 音の MML を 3 通り作り、`preview 300` で待ち時間を比べる(MML は 5000 文字の上限を超えないよう、`[…]N` の繰り返しを使う):

```bash
a='@0 t240 l64 [c e g > c < b g e c d f a > d < c a f d]175'
b='@0 t240 l64 x30,500 z20,100,10 [c e g > c < b g e c d f a > d < c a f d]175'
c='@0 t240 l64 [x30,500 z10,100,10 c e g > c x30,1000 z10,100,20 < b g e c x30,1500 z10,200,10 d f a > x30,2000 z10,200,20 d < c a f d]175'
```
(a) エフェクトなし、(b) 1 組ずつ(全部の音が 2 つのエフェクトに送る)、(c) 4 種 + 4 種を行き来する。Expected: (b) と (c) の待ち時間が、(a) の 3 倍以内で、60 秒以内。`nan=0`、`peak` ≤ 1.0。

- [ ] **Step 4: UI の確認**

- 「記法」を開いて、`x30,2000 リバーブ…` と `z25,300,40 ディレイ…` の行があること。
- 入力欄に `x30,2000 z25,300,40 c4` を打って、`x` `z` が command の色、数字が number の色、`,` が command の色に分かれること。
- エラー: `x30 c` が「4 文字目」で `x0` を案内すること、`'cx30,500'4` が「3 文字目」で「和音の前」を案内すること。「N 文字目」を押すとカーソルがその位置に移ること。
- モバイル UI の入力補助キーに `x` `z` が出るかは、`useMobileUi()` の設定を実 UI で切り替えられないので、確認できない場合は、そう書く(キー定義はユニットテストで確認)。

- [ ] **Step 5: 後始末**

```bash
bash $SP/stop.sh; rm -rf /tmp/tsv499k; cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short
```
Expected: `git status` はクリーン(スクラッチパッドと `/tmp/tsv499k` はリポジトリの外)。

- [ ] **Step 6: 「実装後の確認」を書く**

設計書(`2026-10-31-issue-499-mml-effects-design.md`)の末尾に `## 実装後の確認` を足す。形式は `2026-10-31-issue-499-mml-portamento-design.md` の同じ節に合わせる(日付・ブランチ・隔離インスタンスの手順、「仕様からの変更」、「確認できたこと」、「確認できなかったこと」)。次の項目の実測値を入れる(Task 2 のゲートの値も含める):

- `Tone.Reverb` が `Tone.Offline` の中で動くこと(窓ごとの rms、WAV の長さ、`tailmax`)。
- ディレイのやまびこの間隔と減衰の比。
- 途中でやめる・変える(`x0`、組の切り替え)の挙動。
- 変更前とのハッシュの一致(取れていなければ、その旨)。
- 15 種のピーク・NaN、複数トラック、補正値(`REVERB_TRIM_DB` `DELAY_TRIM_DB`)を変えたなら、その値と理由。
- 極端な値、組の数の上限のエラー、描画時間(a・b・c の待ち時間)、UI。
- 確認できなかったこと: 実際に音が鳴ること・聞こえ方(音声デバイスなし)、モバイル UI の入力補助キー、Android・macOS・Windows、2800 音を超える曲の描画時間。
- 仕様からの変更(尾の式を `TAIL_SEC + エフェクトの尾` にしたこと、ほかにあれば)。

- [ ] **Step 7: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add docs/superpowers/specs/2026-10-31-issue-499-mml-effects-design.md
git commit -m "docs: MML のエフェクトの実装後の確認を記録(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
