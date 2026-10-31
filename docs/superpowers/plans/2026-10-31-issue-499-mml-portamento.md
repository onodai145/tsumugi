# MML のポルタメント Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に `p<ミリ秒>` を足し、休符・和音をはさまずに続く旋律の音が、前の音の高さから滑って入るようにする(Issue #499)。

**Architecture:** Rust の `run_track` が、トラック状態 `portamento_ms` と直前の単音から、音ごとに `glide: Option<Glide { from_midi, ms }>` を `NoteEvent` に載せる。フロントの `renderScore` が、`glide` を「滑り始めの周波数と秒」にして音源の `trigger` に渡す。旋律の音源は、`PolySynth` に渡す声のクラスを `setNote` 上書きの継承クラス(`glideVoice`)に替え、アタックの時刻に周波数を指数的に動かす。`glide` は音源の組の鍵に入れない。

**Tech Stack:** Rust(`src-tauri/src/mml.rs`、`domain/mml.rs`、tauri-specta)、Svelte + TypeScript(`frontend/src/lib/mmlTimbres.ts` `mmlRender.ts` `mmlLexer.ts` `mmlKeys.ts`)、Tone.js 15.1.22(`Tone.Offline`)、Vitest。

**Spec:** `docs/superpowers/specs/2026-10-31-issue-499-mml-portamento-design.md`(承認済み)

## Global Constraints

- コミットメッセージは**件名 1 行だけ**(本文なし)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別に付く。`git commit` が失敗・タイムアウトしたら、**再試行せず止めてユーザーに報告**する。`--no-verify` `--no-gpg-sign` は使わない。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集せず、`cd src-tauri && cargo test` が再生成したものをコミットする。
- `cd src-tauri && rustfmt --check src/mml.rs` の差分は、`main` で 4 件。増やさない(`cargo fmt` でファイル全体を整形しない)。新しく書く Rust は、rustfmt の整形後の形で書く。
- `p` の範囲は 1〜1000 ミリ秒の整数。`p0` で解除。和音の中には書けない。打楽器(`drum` `noise` `drumkit`)は無視する。
- 今まで通っていた MML は、同じ音・同じ結果のままにする。`p` を書かない MML の `NoteEvent.glide` は `None`(TS では `null`)。
- 実機確認は、隔離した Xvfb 上の別インスタンスだけで行う(ユーザーの `cargo tauri dev` に触らない)。プロセスは、トークンで特定した PID だけを止める。`pkill` `killall` は使わない。
- 音が実際に鳴ること・聞こえ方は確認できない(音声デバイスなし)。波形の数値までを確認し、そう報告する。
- 数百行のテスト出力は、ファイルに落として末尾だけ読む。

## Review Focus

1. 休符・和音・タイ・同じ高さ・打楽器・`p0` の境界で、`glide` が付く/付かないが仕様どおりか(Task 1 のテスト)。
2. `PolySynth` が終わった声を別の音に再利用したとき、前の滑りの周波数の動きが残らず、次の音の高さがずれないこと(Task 2 のゲート)。
3. 滑る時間が音の長さより長いとき、または極端に短い音のとき、目的の高さに着き、NaN や例外が出ないこと(Task 2 のテスト + ゲート)。
4. `k`(デチューン)と同時に使うと、滑り始めと目的の高さの両方に同じ倍率が掛かること(Task 2 のテスト)。
5. `p` を書かない既存の MML の描画結果が、変わらないこと(Task 2 のゲート。変更前のバイナリでの WAV のハッシュと比べる)。

---

### Task 1: Rust — `p` コマンドと `NoteEvent.glide`

**Files:**
- Modify: `src-tauri/src/domain/mml.rs`(`Timbre::is_percussive`、`Glide`、`NoteEvent.glide`)
- Modify: `src-tauri/src/domain/mod.rs`(再エクスポート)
- Modify: `src-tauri/src/mml.rs`(`Op::Portamento`、パース、`TrackState`、`glide_for`、和音の中のエラー、テスト)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`(`cargo test` が行う)
- Modify: `frontend/src/lib/mmlRender.test.ts`(`note()` ヘルパーに `glide: null` を足すだけ。TS の型を通すため)

**Interfaces:**
- Consumes: 既存の `Timbre`、`NoteEvent`、`TrackState`、`run_track`、`Cursor::require_number(pos, char) -> Result<(u32, usize), MmlError>`、テスト用の `must_parse(&str) -> MmlScore` と `must_fail(&str) -> MmlError`。
- Produces:
  - `Timbre::is_percussive(self) -> bool`(`Drum | Noise | Drumkit` が true)。
  - `pub struct Glide { pub from_midi: u8, pub ms: u16 }`(`Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type`、`camelCase`)。`crate::domain::Glide` として再エクスポート。
  - `NoteEvent.glide: Option<Glide>`。TS では `glide: Glide | null`、JSON は `{"fromMidi":60,"ms":80}` か `null`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/mml.rs` のテストモジュールの末尾(`note_event_serializes_params_as_camel_case_or_null` の後、モジュールを閉じる `}` の手前)に足す。テストモジュールの先頭の `use` に `Glide` が無ければ `use crate::domain::Glide;` を足す(`TimbreParams` を使っている行の近く)。

```rust
    fn glides_of(text: &str) -> Vec<Option<Glide>> {
        must_parse(text).notes.iter().map(|n| n.glide).collect()
    }

    fn glide(from_midi: u8, ms: u16) -> Option<Glide> {
        Some(Glide { from_midi, ms })
    }

    #[test]
    fn glide_is_none_unless_p_is_set() {
        assert_eq!(glides_of("c e g"), vec![None; 3]);
        assert_eq!(glides_of("p0 c e"), vec![None; 2]);
        // 解除した後は滑らない
        assert_eq!(glides_of("p80 c p0 e"), vec![None; 2]);
    }

    #[test]
    fn portamento_is_read_as_milliseconds_with_a_range() {
        assert_eq!(glides_of("p80 c e")[1], glide(60, 80));
        assert_eq!(glides_of("p1 c e")[1], glide(60, 1));
        assert_eq!(glides_of("p1000 c e")[1], glide(60, 1000));
        // 後から書いたものが効く
        assert_eq!(glides_of("p80 p200 c e")[1], glide(60, 200));
        let e = must_fail("p1001 c");
        assert_eq!(e.position, 1); // 数字の先頭
        assert!(e.message.contains("1000"), "{}", e.message);
        let e = must_fail("p c");
        assert_eq!(e.position, 0);
        assert!(e.message.contains("数値"), "{}", e.message);
    }

    #[test]
    fn glide_starts_from_the_previous_single_note() {
        assert_eq!(
            glides_of("p80 c e g"),
            vec![None, glide(60, 80), glide(64, 80)]
        );
        // オクターブが違うだけでも、高さが違えば滑る
        assert_eq!(glides_of("p80 c > c")[1], glide(60, 80));
    }

    #[test]
    fn glide_needs_adjacent_single_notes_of_different_pitch() {
        // 休符をはさむと滑らない。その次の音からは、また滑る
        assert_eq!(glides_of("p80 c r e"), vec![None; 2]);
        assert_eq!(
            glides_of("p80 c e r g a"),
            vec![None, glide(60, 80), None, glide(67, 80)]
        );
        // 同じ高さは滑らない。その次の音は、その高さから滑る
        assert_eq!(glides_of("p80 c c"), vec![None; 2]);
        assert_eq!(glides_of("p80 c c e")[2], glide(60, 80));
        // 和音の音も、和音の直後の音も滑らない。その次の音からは滑る
        assert_eq!(
            glides_of("p80 c 'eg' a b"),
            vec![None, None, None, None, glide(69, 80)]
        );
    }

    #[test]
    fn glide_after_a_tie_starts_from_the_tied_pitch() {
        assert_eq!(glides_of("p80 c&c e"), vec![None, glide(60, 80)]);
        assert_eq!(glides_of("p80 c e&e g"), vec![None, glide(60, 80), glide(64, 80)]);
        assert_eq!(glides_of("p80 c^4 e"), vec![None, glide(60, 80)]);
    }

    #[test]
    fn glide_is_independent_of_gate_tuplets_and_loops() {
        // ゲートを短くして、前の音が先に消えても、休符を書いていないので滑る
        assert_eq!(glides_of("p80 q50 c e"), vec![None, glide(60, 80)]);
        assert_eq!(
            glides_of("p80 {c e g}3"),
            vec![None, glide(60, 80), glide(64, 80)]
        );
        // 繰り返しの 2 周目の先頭は、1 周目の最後の音から滑る
        assert_eq!(
            glides_of("p80 [c e]2"),
            vec![None, glide(60, 80), glide(64, 80), glide(60, 80)]
        );
    }

    #[test]
    fn glide_ignores_percussive_timbres_on_either_side() {
        for percussive in ["@drum", "@noise", "@drumkit"] {
            assert_eq!(glides_of(&format!("p80 {percussive} c e")), vec![None; 2]);
            // 前の音が打楽器
            assert_eq!(
                glides_of(&format!("p80 {percussive} c @1 e")),
                vec![None; 2],
                "{percussive}"
            );
            // 次の音が打楽器
            assert_eq!(
                glides_of(&format!("p80 @1 c {percussive} e")),
                vec![None; 2],
                "{percussive}"
            );
        }
        // 音色が違う旋律どうしは滑る
        assert_eq!(glides_of("p80 @1 c @2 e")[1], glide(60, 80));
    }

    #[test]
    fn portamento_is_per_track_and_works_in_macros() {
        let score = must_parse("p80 c e; c e");
        let of_track = |t: u8| -> Vec<Option<Glide>> {
            score.notes.iter().filter(|n| n.track == t).map(|n| n.glide).collect()
        };
        assert_eq!(of_track(0), vec![None, glide(60, 80)]);
        assert_eq!(of_track(1), vec![None; 2]);
        assert_eq!(glides_of("$f={p80} $f c e"), vec![None, glide(60, 80)]);
    }

    #[test]
    fn note_event_serializes_glide_as_camel_case_or_null() {
        let score = must_parse("p80 c e");
        assert_eq!(
            serde_json::to_value(&score.notes[0]).unwrap()["glide"],
            serde_json::Value::Null
        );
        assert_eq!(
            serde_json::to_value(&score.notes[1]).unwrap()["glide"],
            serde_json::json!({ "fromMidi": 60, "ms": 80 })
        );
    }

    #[test]
    fn only_drum_noise_and_drumkit_are_percussive() {
        let percussive: Vec<&str> = Timbre::ALL
            .iter()
            .filter(|t| t.is_percussive())
            .map(|t| t.name())
            .collect();
        assert_eq!(percussive, vec!["drum", "noise", "drumkit"]);
    }
```

既存のテスト `params_are_rejected_inside_a_chord` の配列に `p` を足す(1 行の変更):

```rust
        for text in ["'cy1,2,3,4'4", "'cm5,6'4", "'ck12'4", "'cp80'4"] {
```

- [ ] **Step 2: 失敗を確かめる**

Run: `cd src-tauri && cargo test --lib mml:: 2>&1 | tail -30`
Expected: コンパイルエラー(`Glide` `is_percussive` `glide` が無い)。これがこの段階の RED。

- [ ] **Step 3: ドメインの型を足す**

`src-tauri/src/domain/mml.rs`:

`impl Timbre` の `name()` の後に足す:

```rust

    /// 打楽器(音の高さを持たない `drum` `noise` `drumkit`)か。`y` `m` `k` `p` はこれらに効かない。
    pub fn is_percussive(self) -> bool {
        matches!(self, Timbre::Drum | Timbre::Noise | Timbre::Drumkit)
    }
```

`TimbreParams` の定義の後(`NoteEvent` の前)に足す:

```rust
/// ポルタメント(Issue #499)。この音は、`from_midi` の高さから目的の高さまで、`ms` ミリ秒(音の長さより長ければ音の長さ)で滑って入る。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Glide {
    pub from_midi: u8,
    pub ms: u16,
}
```

`NoteEvent` の `params` の後に足す:

```rust
    /// `p` のポルタメント(滑らない音は `None`)
    pub glide: Option<Glide>,
```

`src-tauri/src/domain/mod.rs` の再エクスポートを、`Glide` を足した形にする:

```rust
pub use mml::{
    EnvelopeParams, Glide, MmlError, MmlParseResult, MmlScore, NoteEvent, Timbre, TimbreParams,
    VibratoParams,
};
```

- [ ] **Step 4: パーサーと実行部を足す**

`src-tauri/src/mml.rs`:

1. インポートに `Glide` を足す:

```rust
use crate::domain::{
    EnvelopeParams, Glide, MmlError, MmlScore, NoteEvent, Timbre, TimbreParams, VibratoParams,
};
```

2. 定数(`MAX_GATE` の後):

```rust
/// ポルタメント `p` の最大(ミリ秒)。
const MAX_PORTAMENTO_MS: u32 = 1000;
```

3. `Op` の `Detune(i16),` の後:

```rust
    /// `p<n>`。ポルタメントの時間(ミリ秒。`0` は解除)。
    Portamento(u16),
```

4. パース。`'k' => { … }` の腕の直後(`'@' => {` の前)に足す:

```rust
            'p' => {
                let (n, p) = c.require_number(pos, 'p')?;
                if n > MAX_PORTAMENTO_MS {
                    return Err(mml_err(
                        p,
                        format!("ポルタメントの時間は 1〜{MAX_PORTAMENTO_MS} ミリ秒です(`p0` で解除)"),
                    ));
                }
                Op::Portamento(n as u16)
            }
```

5. 和音の中の腕を `p` 入りにする:

```rust
                        'y' | 'm' | 'k' | 'p' => {
                            return Err(mml_err(
                                inner_pos,
                                "`y` `m` `k` `p` は和音の中には書けません(和音の前に書いてください)",
                            ));
                        }
```

6. `TrackState` の `detune: i16,` の後に `portamento_ms: u16,`(ドキュメント行付き)を足す:

```rust
    /// `p` のポルタメントの時間(ミリ秒。0 は無効)
    portamento_ms: u16,
```

`run_track` の初期化(`detune: 0,` の後)に `portamento_ms: 0,` を足す。

7. `run_track` の命令の処理。`Op::Detune(d) => st.detune = *d,` の後に:

```rust
            Op::Portamento(n) => st.portamento_ms = *n,
```

8. `timbre_params` の後に関数を足す:

```rust
/// 新しい単音 `midi` が滑るなら、その指定。条件は、ポルタメントが有効で、直前が休符をはさまない単音(和音でない)で、
/// 高さが違い、どちらも旋律の音色であること(`docs/superpowers/specs/2026-10-31-issue-499-mml-portamento-design.md`)。
fn glide_for(st: &TrackState, notes: &[NoteEvent], midi: i32) -> Option<Glide> {
    if st.portamento_ms == 0 || st.last_chord || st.timbre.is_percussive() {
        return None;
    }
    let prev = &notes[*st.last_notes.first()?];
    if prev.timbre.is_percussive() || i32::from(prev.midi) == midi {
        return None;
    }
    Some(Glide {
        from_midi: prev.midi,
        ms: st.portamento_ms,
    })
}
```

9. 通常の音符(タイの延長でない側)。`flush_last(&mut st, notes);` の前で `glide` を決め、`NoteEvent` に載せる:

```rust
                } else {
                    let glide = glide_for(&st, notes, midi);
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
                        params: timbre_params(&st),
                        glide,
                    });
```

10. 和音の音(`Op::ChordEnd` の `NoteEvent`)に `glide: None,` を足す。

- [ ] **Step 5: テストを通す**

Run: `cd src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: 新しいテストと既存のテストがすべて通る(0 failed)。落ちたら、テストを直さずコードを直す(位置のずれ以外)。

- [ ] **Step 6: 全体のテストと書式**

Run: `cd src-tauri && cargo test 2>&1 | tail -8; rustfmt --check --edition 2021 src/mml.rs 2>&1 | grep -c '^Diff in'`
Expected: `test result: ok.`(失敗 0。件数は 803 + 新規 11 前後)。rustfmt の差分は 4。増えていたら、足したコードを整形後の形に直す。`git status` に `frontend/src/bindings/tauri.gen.ts` の変更(`Glide` の追加と `NoteEvent.glide`)が出ていること。

- [ ] **Step 7: TS の型を通す(`note()` ヘルパー)**

`frontend/src/lib/mmlRender.test.ts` の先頭の型 import に `Glide` を足し、`note()` ヘルパーに引数 `glide: Glide | null = null` を最後に足して、返すオブジェクトに `glide` を足す:

```ts
import type { Glide, MmlScore, NoteEvent, Timbre, TimbreParams } from "../bindings/tauri.gen";
```

```ts
const note = (
  track: number,
  timbre: Timbre,
  startSec = 0,
  midi = 60,
  params: TimbreParams | null = null,
  glide: Glide | null = null,
): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi,
  velocity: 0.5,
  timbre,
  params,
  glide,
});
```

Run: `cd frontend && pnpm check 2>&1 | tail -5; npx vitest run 2>&1 | tail -6`
Expected: 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件のみ)、全テスト通過。

- [ ] **Step 8: コミット**

```bash
git add src-tauri/src/domain/mml.rs src-tauri/src/domain/mod.rs src-tauri/src/mml.rs frontend/src/bindings/tauri.gen.ts frontend/src/lib/mmlRender.test.ts
git commit -m "feat: MML にポルタメント p を足し、NoteEvent に glide を載せる(Issue #499)"
```

---

### Task 2: フロント — 滑る声と `renderScore`、そして実測のゲート

**Files:**
- Modify: `frontend/src/lib/mmlTimbres.ts`(`VoiceGlide`、`Voice.trigger` の引数、`glideVoice`、`glideSynth`、`poly` の trigger、旋律の 9 つのプリセットを `glideSynth` に)
- Modify: `frontend/src/lib/mmlRender.ts`(`glide` を `fromHz`/`sec` にして渡す)
- Modify: `frontend/src/lib/mmlTimbres.test.ts`、`frontend/src/lib/mmlRender.test.ts`

**Interfaces:**
- Consumes: Task 1 の `NoteEvent.glide: Glide | null`(`{ fromMidi: number; ms: number }`)。既存の `Voice`、`TIMBRES`、`poly()`、`AnyPoly`。
- Produces:
  - `export interface VoiceGlide { fromHz: number; sec: number }`
  - `Voice.trigger(hz, durationSec, startSec, velocity, midi?, glide?: VoiceGlide): void`
  - `export function glideVoice<T extends new (...args: any[]) => any>(Base: T, glides: Map<number, VoiceGlide>): T`
  - `renderScore` は、`n.glide` があり、打楽器でなく、滑る秒が 0 より大きいときだけ `glide` を渡す。`fromHz` = `Frequency(fromMidi) × 倍率`、`sec` = `min(ms / 1000, durationSec)`。倍率 = `2^(detune/1200)`(`hz` と同じ)。

- [ ] **Step 1: 変更前のバイナリで、既存の MML の WAV のハッシュを取る(Review Focus 5)**

この段階では、`src-tauri/target/debug/tsumugi` は `main` の中身(`eeec5b7`)でビルドされたまま(Task 1 で `cargo build` はしていない)。これを変更前のバイナリとして使う。確認:

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
ls -l --time-style=full-iso src-tauri/target/debug/tsumugi; git log -1 --format='%H %cd' --date=iso eeec5b7
git diff eeec5b7 main --stat | tail -1   # 空であること(main は eeec5b7 と同じ中身)
```

バイナリの更新時刻がコミット `eeec5b7` より後で、`frontend/dist` もそのとき作られたものなら、変更前として使える(`frontend/dist` の更新時刻が `frontend/src` の最終変更より新しいかも見る)。**古い・分からない場合は、この Step を飛ばして**、台帳に `Task 2: Ruling: 変更前のバイナリが使えないので、Review Focus 5 は単体テストだけで確認` と書く。

使えるなら、実機確認の準備(下の Step 9 の手順 a〜c と同じ。`lib.sh` を作り、`launch.sh` で起動)をして、次の 5 つの MML の WAV の SHA-256 を `$SP/baseline.txt` に記録し、`bash $SP/stop.sh` で止める:

```
@0 t120 c4 e4 g4
@1 t120 c4 e4 g4
@9 t120 c4 e4 g4
@10 t120 c4 e4 g4
@11 t120 c4 e4 g4
```

1 本ごとに `typemml '<MML>'; preview 60; sha256sum $H/last.b64` を使う(`$H/last.b64` に WAV の data URL が入る)。

- [ ] **Step 2: 失敗するテストを書く(`mmlRender.test.ts`)**

`Call` の型とモックの `trigger` を、`glide` を記録するようにする:

```ts
type Call = { hz: number; d: number; t: number; v: number; midi?: number; glide?: { fromHz: number; sec: number } };
```

```ts
      return {
        trigger: (hz: number, d: number, t: number, v: number, midi?: number, glide?: { fromHz: number; sec: number }) =>
          rec.calls.push({ hz, d, t, v, midi, glide }),
      };
```

ファイルの末尾に足す(`describe` の外、既存の `describe` と同じ階層):

```ts
describe("renderScore: ポルタメント(Issue #499)", () => {
  const g = (fromMidi: number, ms: number): Glide => ({ fromMidi, ms });

  it("glide を、滑り始めの周波数 fromHz と秒 sec にして、音源の trigger に渡す", async () => {
    await renderScore(score([note(0, "fm", 0, 60), note(0, "fm", 0.5, 64, null, g(60, 80))]));
    expect(created[0].calls[0].glide).toBeUndefined();
    expect(created[0].calls[1].hz).toBe(640);
    expect(created[0].calls[1].glide).toEqual({ fromHz: 600, sec: 0.08 });
  });

  it("滑りが音の長さ(0.25 秒)より長いときは、音の長さに縮める", async () => {
    await renderScore(score([note(0, "fm", 0.5, 64, null, g(60, 1000))]));
    expect(created[0].calls[0].glide).toEqual({ fromHz: 600, sec: 0.25 });
  });

  it("音の長さが 0 のときは、glide を渡さない(0 秒の滑りはない)", async () => {
    const n = { ...note(0, "fm", 0.5, 64, null, g(60, 80)), durationSec: 0 };
    await renderScore(score([n]));
    expect(created[0].calls[0].glide).toBeUndefined();
  });

  it("デチューンは、滑り始めと目的の高さの両方に同じ倍率で掛かる", async () => {
    const octave: TimbreParams = { envelope: null, vibrato: null, detune: 1200 };
    await renderScore(score([note(0, "fm", 0.5, 64, octave, g(60, 80))]));
    expect(created[0].calls[0].hz).toBe(1280);
    expect(created[0].calls[0].glide).toEqual({ fromHz: 1200, sec: 0.08 });
  });

  it("glide があっても、音源の組は増えない", async () => {
    await renderScore(score([note(0, "fm", 0, 60), note(0, "fm", 0.5, 64, null, g(60, 80)), note(0, "fm", 1, 67, null, g(64, 80))]));
    expect(created).toHaveLength(1);
    expect(created[0].calls).toHaveLength(3);
  });

  it("打楽器は glide を無視する", async () => {
    await renderScore(score([note(0, "drum", 0.5, 64, null, g(60, 80)), note(0, "noise", 1, 64, null, g(60, 80))]));
    expect(created.every((c) => c.calls.every((x) => x.glide === undefined))).toBe(true);
  });
});
```

- [ ] **Step 3: 失敗するテストを書く(`mmlTimbres.test.ts`)**

`fakeTone()` の定義の後、`describe("drumkit の音源", …)` の前に足す:

```ts
/** `glideVoice` の土台にする、`Monophonic` の代役。`frequency` の呼び出しと、元の `setNote` の呼び出しを記録する */
class FakeMonophonic {
  calls: unknown[][] = [];
  frequency = {
    setValueAtTime: (...a: unknown[]) => this.calls.push(["set", ...a]),
    exponentialRampToValueAtTime: (...a: unknown[]) => this.calls.push(["ramp", ...a]),
  };
  setNote(note: unknown, time?: number) {
    this.calls.push(["base", note, time]);
    return this;
  }
}

describe("glideVoice(ポルタメント、Issue #499)", () => {
  it("指定が無い時刻は、元の setNote のまま", () => {
    const Voice = glideVoice(FakeMonophonic, new Map());
    const v = new Voice();
    v.setNote(440, 1);
    expect(v.calls).toEqual([["base", 440, 1]]);
  });

  it("指定のある時刻は、fromHz に置いて、目的の高さへ指数的に動かす(元の setNote は呼ばない)", () => {
    const glides = new Map([[1.25, { fromHz: 300, sec: 0.08 }]]);
    const v = new (glideVoice(FakeMonophonic, glides))();
    v.setNote(440, 1.25);
    expect(v.calls).toHaveLength(2);
    expect(v.calls[0]).toEqual(["set", 300, 1.25]);
    expect(v.calls[1][0]).toBe("ramp");
    expect(v.calls[1][1]).toBe(440);
    expect(v.calls[1][2]).toBeCloseTo(1.33, 9);
  });

  it("指定は 1 回使うと消え、ほかの時刻には影響しない", () => {
    const glides = new Map([[1, { fromHz: 300, sec: 0.1 }]]);
    const v = new (glideVoice(FakeMonophonic, glides))();
    v.setNote(440, 2); // 別の時刻
    expect(glides.size).toBe(1);
    v.setNote(440, 1);
    expect(glides.size).toBe(0);
    v.setNote(440, 1); // 2 回目は元の setNote
    expect(v.calls.map((c) => c[0])).toEqual(["base", "set", "ramp", "base"]);
  });

  it("音名など数値でない note は、指定があっても元の setNote", () => {
    const v = new (glideVoice(FakeMonophonic, new Map([[1, { fromHz: 300, sec: 0.1 }]])))();
    v.setNote("C4", 1);
    expect(v.calls).toEqual([["base", "C4", 1]]);
  });
});

describe("旋律の音源とポルタメント(Issue #499)", () => {
  it("PolySynth に渡す声は glideVoice の継承クラスで、trigger の glide が、その時刻の setNote に届く", () => {
    const { Tone, made } = fakeTone();
    (Tone as unknown as Record<string, unknown>).FMSynth = FakeMonophonic;
    const voice = TIMBRES.fm.create(Tone, -3);
    voice.trigger(440, 0.5, 1.25, 0.8, 64, { fromHz: 300, sec: 0.08 });
    voice.trigger(440, 0.5, 2, 0.8, 64); // 滑らない音
    const Cls = made.find((m) => m.cls === "PolySynth")!.args[0] as new () => FakeMonophonic;
    expect(Object.getPrototypeOf(Cls.prototype)).toBe(FakeMonophonic.prototype); // 元の声の継承クラス
    const slid = new Cls();
    slid.setNote(440, 1.25);
    expect(slid.calls.map((c) => c[0])).toEqual(["set", "ramp"]);
    const plain = new Cls();
    plain.setNote(440, 2);
    expect(plain.calls.map((c) => c[0])).toEqual(["base"]);
  });

  it("旋律の 12 種すべてが、元の声(Synth・FMSynth・AMSynth・DuoSynth・MonoSynth のどれか)の継承クラスを PolySynth に渡す", () => {
    for (const p of TIMBRE_LIST.filter((p) => !p.percussive)) {
      const { Tone, made } = fakeTone();
      p.create(Tone, -3);
      const Cls = made.find((m) => m.cls === "PolySynth")!.args[0] as object;
      const bases = ["Synth", "FMSynth", "AMSynth", "DuoSynth", "MonoSynth"].map((n) => (Tone as unknown as Record<string, unknown>)[n]);
      expect(bases, p.name).toContain(Object.getPrototypeOf(Cls)); // 元の声そのものではなく、その継承クラス
      expect(bases, p.name).not.toContain(Cls);
    }
  });

  it("打楽器の音源は、glide が付いても例外を出さず、そのまま鳴らす", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drum.create(Tone, -3);
    expect(() => voice.trigger(440, 0.5, 1, 0.8, 60, { fromHz: 300, sec: 0.08 })).not.toThrow();
    expect(hits).toHaveLength(1);
  });
});
```

先頭の import を `import { TIMBRES, TIMBRE_LIST, glideVoice, type ToneModule } from "./mmlTimbres";` にする。

- [ ] **Step 4: 失敗を確かめる**

Run: `cd frontend && npx vitest run src/lib/mmlRender.test.ts src/lib/mmlTimbres.test.ts 2>&1 | tail -30`
Expected: `glideVoice` が無い・`glide` が渡らない、で失敗(RED)。

- [ ] **Step 5: 実装 — `mmlTimbres.ts`**

1. `Voice` を変える:

```ts
/** ポルタメント(Issue #499)。`fromHz` から鳴らす周波数まで、`sec` 秒で指数的に(音程が等速で)動く。`renderScore` が作る */
export interface VoiceGlide {
  fromHz: number;
  sec: number;
}

/** Offline コンテキスト内に作った音源 1 つ。 */
export interface Voice {
  /** `midi` はノート番号。打楽器の `drumkit` だけが使う(音の高さで楽器を選ぶ)。`glide` は旋律の音源だけが使う */
  trigger(hz: number, durationSec: number, startSec: number, velocity: number, midi?: number, glide?: VoiceGlide): void;
}
```

2. `type AnyPoly` の下に、声の継承クラスと、それを使う `PolySynth` を作る関数を足す:

```ts
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Ctor = new (...args: any[]) => any;

/**
 * `PolySynth` が音ごとに作る声(`Synth` `FMSynth` など。どれも `Monophonic`)を、周波数を滑らせられるようにした継承クラス。
 * `glides` に、鳴らす時刻で入れた指定があれば、その声の `setNote`(アタックのときに周波数を置く)で、`fromHz` に置いて
 * 目的の高さへ指数的に動かす。Tone の `portamento` は、鳴り続けている同じ声にだけ効くので使えない(`PolySynth` は音ごとに別の声を使う)。
 */
export function glideVoice<T extends Ctor>(Base: T, glides: Map<number, VoiceGlide>): T {
  return class extends Base {
    setNote(note: unknown, time?: number) {
      const glide = time === undefined ? undefined : glides.get(time);
      if (time === undefined || !glide || typeof note !== "number") return super.setNote(note, time);
      glides.delete(time);
      this.frequency.setValueAtTime(glide.fromHz, time);
      this.frequency.exponentialRampToValueAtTime(note, time + glide.sec);
      return this;
    }
  };
}

/** `poly` が `trigger` の glide を入れる、`PolySynth` ごとの表(時刻 → 滑りの指定) */
const GLIDES = new WeakMap<object, Map<number, VoiceGlide>>();

/** 滑れる声を使う `PolySynth`。`options` は、元の声のコンストラクターの引数 */
function glideSynth<V extends Ctor>(Tone: ToneModule, Base: V, options: ConstructorParameters<V>[0]): AnyPoly {
  const glides = new Map<number, VoiceGlide>();
  const synth = new Tone.PolySynth(glideVoice(Base, glides) as never, options as never) as unknown as AnyPoly;
  GLIDES.set(synth, glides);
  return synth;
}
```

(型で `pnpm check` が通らない箇所は、`as never` / `as unknown as` を足して通す。ふるまいは変えない。)

3. `poly()` の最後の `return` を変える:

```ts
  return {
    trigger: (hz, d, t, v, _midi, glide) => {
      // 滑る指定は、鳴らす時刻で表に入れる。声が、アタックで同じ時刻の `setNote` を呼ぶときに取り出す
      if (glide) GLIDES.get(synth)?.set(t, glide);
      synth.triggerAttackRelease(hz, d, t, v);
    },
  };
```

4. 旋律のプリセット(`basic`、`fm`、`organ`、`epiano`、`bell`、`am`、`lead`、`bass`)の `new Tone.PolySynth(Tone.X, options)` を、`glideSynth(Tone, Tone.X, options)` に置き換える(`drum` の `new Tone.PolySynth(Tone.MembraneSynth, …)` と drumkit の膜は、そのまま)。例:

```ts
const basic =
  (oscillator: SynthOscillator) =>
  (Tone: ToneModule, db: number, params?: TimbreParams | null): Voice =>
    poly(glideSynth(Tone, Tone.Synth, { oscillator, envelope: BASIC_ENV }), db, { Tone, params });
```

```ts
      poly(
        glideSynth(Tone, Tone.FMSynth, {
          harmonicity: 3,
          modulationIndex: 8,
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.4, release: 0.1 },
        }),
        db,
        { Tone, params },
      ),
```

`lead` は `glideSynth(Tone, Tone.DuoSynth, { harmonicity: 1, vibratoAmount: 0.3, vibratoRate: 5, voice0: voice, voice1: voice })`、`bass` は `glideSynth(Tone, Tone.MonoSynth, {…})`、`am` は `Tone.AMSynth`、`organ` は `Tone.Synth`。オプションの中身は変えない。

- [ ] **Step 6: 実装 — `mmlRender.ts`**

`for (const n of score.notes) { … }` のループを変える:

```ts
      for (const n of score.notes) {
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
```

- [ ] **Step 7: テストと型を通す**

Run: `cd frontend && npx vitest run src/lib/mmlRender.test.ts src/lib/mmlTimbres.test.ts 2>&1 | tail -15; pnpm check 2>&1 | tail -5`
Expected: すべて通る。型エラーが出たら Step 5 の注記の通り、キャストで通す。落ちるテストは、テストを弱めずコードを直す(「11 種すべて」のテストが、継承クラスの判定で不安定なら、`Object.getPrototypeOf(Cls.prototype)` が `Tone` の元の声のクラスの `prototype` であることの検査に替えてよい。ただし検査自体は残す)。

- [ ] **Step 8: 全体のテストとコミット**

Run: `cd frontend && npx vitest run 2>&1 | tail -6`
Expected: 全ファイル通過。

```bash
git add frontend/src/lib/mmlTimbres.ts frontend/src/lib/mmlRender.ts frontend/src/lib/mmlTimbres.test.ts frontend/src/lib/mmlRender.test.ts
git commit -m "feat: MML のポルタメントを、旋律の音源の周波数を滑らせて描画する(Issue #499)"
```

- [ ] **Step 9: ゲート — 実際の描画で、5 種の声が滑ることを実測する**

**このゲートに通らなければ、先に進まない。** 通らない声があれば、台帳に結果を書いて**止め、ユーザーに報告して相談**する(spec の「未確認・リスク」の最初の項目)。

a. 準備(スクラッチパッドを `SP` に、隔離ホームを `/tmp/tsv499k` に置く。`SP=/tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/af5fb360-aecc-4ab1-94bb-9cff6a1a0969/scratchpad`。`launch.sh` `stop.sh` `wav.py` `pitch.py` は既にそこにある):

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm build 2>&1 | tail -2
cd ../src-tauri && cargo build 2>&1 | tail -2
SP=/tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/af5fb360-aecc-4ab1-94bb-9cff6a1a0969/scratchpad
H=/tmp/tsv499k; mkdir -p $H; echo $SP > $H/sp
cat > $H/lib.sh <<'EOF'
H=/tmp/tsv499k; export DISPLAY=:95
br(){ curl -s --unix-socket $H/cache/com.onodai.tsumugi/debug-bridge.sock http://localhost/ --data-binary "$1" | jq -r .; }
click(){ br "[...document.querySelectorAll('button, label, [role=radio]')].find(b=>b.textContent.trim().startsWith('$1')).click(); return 1" >/dev/null; sleep ${2:-0.8}; }
typemml(){ br 'document.querySelector(".cm-content").focus(); return 1' >/dev/null; xdotool key ctrl+a BackSpace; xdotool type --delay 20 -- "$1"; sleep 0.8; }
preview(){ br 'window.__n=(window.__h||[]).length; return 1' >/dev/null; br '[...document.querySelectorAll("button")].find(b=>b.textContent.trim()==="試聴").click(); return 1' >/dev/null; for w in $(seq 1 ${1:-30}); do sleep 1; _r=$(br 'return String((window.__h||[]).length>window.__n)'); case "$_r" in *'"value":"true"'*) break;; esac; done; br 'return window.__h[window.__h.length-1]' | jq -r .value > $H/last.b64 2>/dev/null || true; echo "waited ${w}s"; }
EOF
(bash $SP/launch.sh > $H/launch.log 2>&1; echo done >> $H/launch.log) > /dev/null 2>&1 &
```

起動に 2〜3 分かかる。`tail $H/launch.log` に `done` が出るまで、間をあけて見る(`sleep` で待たず、別のことをするか、短い確認を繰り返す)。

b. 周波数の軌跡を測るスクリプトを `$SP/glide.py` に作る(窓ごとの基本周波数を、自己相関で出す):

```python
import sys, json, base64, struct, math
raw = open(sys.argv[1]).read().strip()
d = json.loads(raw) if raw.startswith("{") else {"choice": raw}
b = base64.b64decode(d["choice"].split(",", 1)[1])
n = struct.unpack("<I", b[40:44])[0] // 2
x = [v / 32768 for v in struct.unpack("<%dh" % n, b[44:44 + 2 * n])]
sr = 22050
def f0(t0, t1):
    seg = x[int(t0 * sr):int(t1 * sr)]
    lo, hi = int(sr / 700), int(sr / 100)
    ac = {lag: sum(seg[i] * seg[i + lag] for i in range(len(seg) - lag)) for lag in range(lo - 1, hi + 2)}
    best = max(range(lo, hi + 1), key=lambda l: ac[l])
    a, bb, c = ac[best - 1], ac[best], ac[best + 1]
    den = a - 2 * bb + c
    off = 0.5 * (a - c) / den if den else 0
    return sr / (best + off)
for w in sys.argv[2:]:
    t0, t1 = map(float, w.split(":"))
    print(f"{t0:.2f}-{t1:.2f}s  {f0(t0, t1):7.1f} Hz")
```

c. 測る。`$H/lib.sh` を読み込み、設定→通知→MML は `launch.sh` が済ませている。音色ごとに実行する(`@5 sine` は純音なので基準。`@0 fm` `@1 square` `@9 am` `@10 lead` `@11 bass` が 5 種の声):

```bash
. $H/lib.sh
for tb in 5 0 1 9 10 11; do
  typemml "@$tb t120 p80 c4 e4"; preview 60
  echo "== @$tb"; python3 $SP/glide.py $H/last.b64 0.38:0.48 0.50:0.53 0.53:0.56 0.56:0.58 0.62:0.9
done
```

(`c4` は 0〜0.5 秒、`e4` は 0.5〜1.0 秒。滑りは 0.50〜0.58 秒。C4 = 261.6 Hz、E4 = 329.6 Hz。)

**合格の条件(音色ごと)**:
- 1 つ目の窓(滑る前)が 261.6 Hz の ±1.5 %(257.7〜265.5)。
- 滑りの 3 つの窓(0.50〜0.58)が、**増えていく**(単調増加)。最初の窓は 262〜290 Hz、最後の窓(0.56〜0.58)は 310〜330 Hz。
- 最後の窓(0.62〜0.9)が 329.6 Hz の ±1.5 %(324.7〜334.5)。

`@5 sine` を最初に測り、基準として合格することを確かめてから、ほかの 5 種を測る。複合音の声(`@0` `@9` `@10`)で自己相関が基本周波数でなく倍音を拾うときは、`@5` が合格した上で、その音色の最後の窓だけでも合格しているなら、「基本周波数が取りにくい」として、`p80` なしの同じ MML(`@N t120 c4 e4`)の窓の値と比べて、同じ窓が同じ値になること、滑りの窓が下の値(c の値)から上の値(e の値)へ動いていることを確かめる。

d. 声の再利用で、滑りが残らないことを測る:

```bash
. $H/lib.sh
typemml "@5 t120 p80 c4 e4 p0 r4 e4"; preview 60
python3 $SP/glide.py $H/last.b64 0.62:0.9 1.52:1.95
```

Expected: 2 つ目の窓(休符の後の `e4`。1.5 秒から)が 329.6 Hz の ±1.5 %。

e. 変更前との比較(Step 1 で `baseline.txt` を取れていたら):

```bash
for tb in 0 1 9 10 11; do typemml "@$tb t120 c4 e4 g4"; preview 60; sha256sum $H/last.b64; done
```

Expected: `baseline.txt` と、5 つとも同じハッシュ(`p` を書かない MML の結果が変わらない)。違ったら、WAV を `wav.py` で比べ(ピーク・RMS・サンプル数)、同じ MML を 2 回測って同じ変更前同士でハッシュが安定していたかも確かめる(安定していなければ、ハッシュでなく波形の値の一致で判断する)。

f. 結果(各音色の窓の値、ハッシュの一致)を台帳に書く。インスタンスを止める: `bash $SP/stop.sh; rm -rf /tmp/tsv499k`。

---

### Task 3: 字句解析・入力補助キー・ヘルプ・ガイド

**Files:**
- Modify: `frontend/src/lib/mmlLexer.ts`、`frontend/src/lib/mmlLexer.test.ts`
- Modify: `frontend/src/lib/mmlKeys.ts`、`frontend/src/lib/mmlKeys.test.ts`
- Modify: `frontend/src/ui/settings/SoundChoicePicker.svelte`、`frontend/src/ui/settings/SoundChoicePicker.test.ts`
- Modify: `docs/guide/user-guide.md`

**Interfaces:**
- Consumes: Task 1 で `p` が MML として通ること。
- Produces: なし(UI の追随だけ)。

- [ ] **Step 1: 失敗するテストを書く**

`mmlLexer.test.ts` の「音色パラメーター y m k は…」のテストの後に足す:

```ts
  it("ポルタメント p は command、数字は number(Issue #499)", () => {
    expect(lex("p80 p0")).toEqual([
      ["p", "command"],
      ["80", "number"],
      ["p", "command"],
      ["0", "number"],
    ]);
  });
```

`mmlKeys.test.ts`: `expect(inserts("params")).toEqual(["y", "m", "k", ","]);` を `["y", "m", "k", "p", ","]` に変える。キーだけで打った列のテストの説明文と列を変える。`"param-k", "len-2", "c",` の後ろ(`"macro"` の前)を次のようにする:

```ts
      "param-y", "len-4", "param-comma", "len-8", "param-m", "len-4", "param-k", "len-2", "param-p", "len-8", "c",
```

(テストの説明文の `y 4 , 8 m 4 k 2 c` も `y 4 , 8 m 4 k 2 p 8 c` に直す。)

`SoundChoicePicker.test.ts` の「記法ヘルプに、音色パラメーター…」のテストの後に足す:

```ts
  it("記法ヘルプに、ポルタメント p の書き方・範囲・解除が出る(Issue #499)", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    expect(text).toContain("p80");
    expect(text).toContain("ポルタメント");
    expect(text).toContain("1〜1000");
    expect(text).toContain("p0");
  });
```

- [ ] **Step 2: 失敗を確かめる**

Run: `cd frontend && npx vitest run src/lib/mmlLexer.test.ts src/lib/mmlKeys.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | tail -25`
Expected: 3 ファイルで失敗(`p` が unknown、キーが無い、ヘルプに無い)。

- [ ] **Step 3: 実装**

`mmlLexer.ts`: `const NUMBERED_COMMANDS = new Set(["o", "l", "t", "v", "q", "p"]);`(`p` は数字 1 つを後ろに取る命令なので、`q` と同じ枝で色分けされる)。

`mmlKeys.ts`: `params` グループの `param-k` の後(カンマの前)に足す:

```ts
      { id: "param-p", label: "p", insert: "p", aria: "ポルタメント p" },
```

`SoundChoicePicker.svelte`: 「y0 m0 k0 で音色の既定に戻す…」の行の直後に、1 行足す:

```
p80 ポルタメント(1〜1000 ミリ秒。休符や和音をはさまずに続く音が、前の音の高さから滑って入る。p0 で解除)。トラックの状態で、和音の中には書けない。打楽器には効かない
```

`docs/guide/user-guide.md`:
1. 入力補助キーの文の `音色パラメーター \`y\` \`m\` \`k\` \`,\`` を `音色パラメーター \`y\` \`m\` \`k\` \`p\` \`,\`` に変える。
2. 「音色のパラメーター」の節の、例の段落(`例: \`@lead y5,100,70,400 …\``)の直前に、新しい段落を足す:

```
**ポルタメント**: `p80` と書くと、そこから後は、休符も和音もはさまずに続く音が、前の音の高さから滑って入ります(`80` は滑る時間のミリ秒で、1〜1000。`p0` で止めます)。`y` `m` `k` と同じトラックの状態で、ループやマクロの中にも書けます。和音の中には書けません。

例: `@lead p120 o4 c4 e4 g4 p0 c4`(`e` は `c` から、`g` は `e` から滑って入り、最後の `c` は滑りません)。

- 滑る音も、頭から立ち上がります(1 つの音を鳴らし続けて高さだけ変える、本物のレガートとは違います)。
- 休符の後の音、和音と和音の直後の音、前と同じ高さの音は、滑りません。`drum` `noise` `drumkit` にも効きません。
- 滑る時間が音の長さより長いときは、音の長さに縮まります(音が消えるまでに、目的の高さに着きます)。
- `q` でゲートを短くしても、休符を書いていなければ滑ります。`k` のデチューンとは、同時に使えます。
```

- [ ] **Step 4: テストと型を通す**

Run: `cd frontend && npx vitest run 2>&1 | tail -6; pnpm check 2>&1 | tail -4`
Expected: 全テスト通過、0 errors。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts frontend/src/lib/mmlKeys.ts frontend/src/lib/mmlKeys.test.ts frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md
git commit -m "docs: MML のポルタメント p を、色分け・入力補助キー・ヘルプ・ガイドに追記(Issue #499)"
```

---

### Task 4: 実機での通し確認と、実装後の確認の記録

**Files:**
- Modify: `docs/superpowers/specs/2026-10-31-issue-499-mml-portamento-design.md`(「実装後の確認」を書く)

**Interfaces:**
- Consumes: Task 1〜3 のすべて。
- Produces: spec の「実装後の確認」節。

- [ ] **Step 1: ビルドして、隔離インスタンスを起動する**

Task 2 の Step 9 の a と同じ(`pnpm build` → `cargo build` → `lib.sh` を作る → `launch.sh` をバックグラウンドで起動)。ビルドは Task 3 の変更を含める。

- [ ] **Step 2: 15 種の音色で、描画が壊れないこと**

`$SP/wav.py` を使い、`@N t120 p80 c4 e4 g4 e4 c4`(N = 0〜14)をすべて `preview` して、エラー・NaN なし(`nan=0`)、ピーク 0.36 以下、`tailmax` が小さい(余韻が切れている)ことを確かめる。打楽器(12〜14)は、`p80` を付けない同じ MML と WAV のハッシュが同じであること(`p` を無視)。

```bash
. $H/lib.sh
for tb in $(seq 0 14); do typemml "@$tb t120 p80 c4 e4 g4 e4 c4"; preview 90; echo "== @$tb"; python3 $SP/wav.py $H/last.b64; done
```

- [ ] **Step 3: 極端な値**

- `@5 t120 p1000 l16 c e c e c e c e`(滑りが音の長さより長い。音が短い): 描画が失敗せず、`nan=0`、ピーク 0.36 以下。`glide.py` で最後の音(`e`。約 1.5 秒付近の窓)が 329.6 Hz の ±3 % に着いていること。
- `@5 t120 p80 k+100 c4 e4`: 滑りが `k` の倍率を含むこと(`glide.py` で、前の窓が 261.6×2^(100/1200)=277.2 Hz、最後の窓が 329.6×2^(100/1200)=349.2 Hz の ±1.5 %)。
- `@5 t120 p80 'ce'4 g4 a4`(和音の後): 描画が通り、`g4` は滑らない(0.5 秒の窓が 392.0 Hz の ±1.5 %)。
- `p` を使わない既存のサンプル MML(音色ボタンの隣の「サンプル」)を 1 つ描画して、エラーなし。

- [ ] **Step 4: 描画時間**

`@0 t240 l64` で、2814 音ほど(Task 2 の spec の測定と同じ形)の MML を 2 通り作り、`preview 300` で待ち時間を比べる。(a) `p` なし、(b) 先頭に `p20` を付ける(ほとんどの音が滑る)。(b) が (a) の 1.5 倍以内であること。MML の生成例(コマンドラインで作って `typemml` に渡す。5000 文字の上限を超えないよう、`[…]N` の繰り返しを使う):

```bash
a='@0 t240 l64 [c e g > c < b g e c d f a > d < c a f d]175'   # 16 音 × 175 = 2800 音(再生は約 44 秒)
```

(`(b)` は `p20 ` を先頭に足すだけ。同じ音の数・長さで比べる。)

- [ ] **Step 5: UI の確認**

- 「記法」を開いて、`p80 ポルタメント…` の行があること。
- 入力欄に `p80 c4 e4` を打って、`p` と `80` が色分けされること(`p` が command の色、`80` が number の色)。
- モバイル幅(ウィンドウを狭くするか、`tsumugi` の設定)で、入力補助キーに `p` が出て、押すとカーソル位置に入ること。
- エラー: `p1001 c` が「2 文字目」、`'cp80'4` が「3 文字目」で、メッセージがそれぞれ範囲と「和音の前」を案内すること。「N 文字目」を押すとカーソルがその位置に移ること(印の文字を打って確かめる)。

- [ ] **Step 6: 後始末**

```bash
bash $SP/stop.sh; rm -rf /tmp/tsv499k; git status --short
```

Expected: `git status` はクリーン(スクラッチパッドと `/tmp/tsv499k` はリポジトリの外)。

- [ ] **Step 7: 「実装後の確認」を書く**

spec の `## 実装後の確認` の「(実装後に記録する。)」を、次の項目の実測値で置き換える(Task 2 のゲートの値も含める)。形式は `2026-10-30-issue-499-mml-timbre-params-design.md` の同じ節に合わせる(日付・ブランチ・隔離インスタンスの手順、「仕様からの変更」、「確認できたこと」、「確認できなかったこと」)。

- 5 種の声(`Synth` `FMSynth` `AMSynth` `DuoSynth` `MonoSynth`)の周波数の軌跡(窓ごとの値)。
- 声の再利用で、滑りが残らないこと。
- 変更前とのハッシュの一致(取れていなければ、その旨)。
- 15 種のピーク・NaN、打楽器の無視、極端な値、描画時間、UI。
- 確認できなかったこと: 実際に音が鳴ること・聞こえ方(音声デバイスなし)、Android・macOS・Windows、`p` を使った長い曲の描画時間(2814 音までを測った)。
- 仕様からの変更があれば、その理由。

- [ ] **Step 8: コミット**

```bash
git add docs/superpowers/specs/2026-10-31-issue-499-mml-portamento-design.md
git commit -m "docs: MML のポルタメントの実装後の確認を記録(Issue #499)"
```
