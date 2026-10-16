# 着メロ打ち込み(MML)機能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 通知音を MML テキストで打ち込めるようにする。設定画面とカラムごとの通知音の両方で、入力・試聴・保存ができる。

**Architecture:** MML のパースは Rust の純粋関数(`mml.rs`)で行い、時刻解決済みのノート列(`MmlScore`)を返す。音の生成はフロントで Tone.js の `Tone.Offline` を使って WAV にし、`data:` URL として既存の `play_notify_sound`(rodio)に渡す。`sound_choice` に `mml:<テキスト>` を保存し、`ipc.ts` の `playNotifySound` が再生直前に解決する(結果はメモ化する)。

**Tech Stack:** Rust(tauri-specta、rodio は既存)、Svelte 5 + TypeScript、Vitest、`tone@15.1.22`(新規 npm 依存)。

**Spec:** `docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md`

## Global Constraints

- MML の文字数は 1000 以下、全トラックの再生時間は 10 秒以下、トラックは最大 4 つ、繰り返しはネストなしで回数 2〜99。
- 長さは `1 2 4 8 16 32 64` のみ。付点は最大 2 個。オクターブは 1〜8(既定 4)、テンポは 32〜255(既定 120)、音量は 0〜15(既定 10)。`>` で 1 オクターブ上げ、`<` で下げる。
- `o4 c` が MIDI ノート番号 60。ベロシティは音量 / 15。
- `sound_choice` の MML 形式は `mml:<生テキスト>`(接頭辞は小文字の `mml:` 固定)。
- 音色は `PolySynth(FMSynth)`、WAV は 22.05kHz・モノラル・16bit PCM。Tone.js は遅延 import し、`Tone.start()` と `toDestination()` 以外の出力経路は使わない(`AudioContext` から音を出さない)。
- 再生は Rust(`play_notify_sound`)のみ。MML の解釈や合成に失敗しても `beep` で代替し、通知は止めない。
- `play_notify_sound` / `resolve_audio_bytes` の既存シグネチャと、`NotifyConfig` / `ColumnDef` のフィールドは変えない。
- 生成物 `frontend/src/bindings/tauri.gen.ts` は手で編集せず、`cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- コミットメッセージは件名のみ(本文なし)。セッションの指示で決まる帰属トレーラーだけを別段落に付ける。`--no-verify` は使わない。コミットが失敗したら止めてユーザーに報告する(再試行しない)。
- 新しい UI は `docs/design/style-guide.md` に従い、`rounded-[Npx]` などの即値を作らない。
- ユーザーの `cargo tauri dev` はメインのワーキングツリーを監視してホットリビルドする。実装は `superpowers:using-git-worktrees` の隔離ワークツリーで行うことを推奨する(その場合、フロントは `pnpm install`、Rust は別 `target/` で初回ビルドが必要になる)。
- 実アプリの動作確認は、必ず Xvfb 越し(`WAYLAND_DISPLAY` を unset、`GDK_BACKEND=x11`、`dbus-run-session`、短い `/tmp` 直下の `XDG_*`)で行う。プロセスの後始末は正確な PID だけを `kill` し、`pkill` / `killall` は使わない。

## Review Focus

- 末尾の改行・CRLF・タブ・全角スペースを含む入力: 改行・タブ・半角空白は無視し、エラー位置は空白を含めた元のテキストの文字位置にする(Task 2)。
- 悪意または偶然の重い入力(`[c1]99`、1000 文字ちょうど、1001 文字): 固まらずエラー位置付きで拒否する(Task 2)。
- 保存済みの MML が再生時に解釈・レンダリングできない場合: 無音ではなく `beep` が鳴り、理由がログに残る(Task 5)。
- 初回レンダリング中に通知が連続した場合: 同じ MML を二重にレンダリングせず、同じ Promise を共有する(Task 5)。
- 不正な MML のまま保存しようとした場合: カラムのモーダルは `setColumnNotify` の失敗を握りつぶしてカラム作成後にエラー表示するため、ピッカー側の `invalid` 通知で保存ボタンを止め、Rust 側でも拒否する(Task 3 / Task 6)。

---

## File Structure

| ファイル | 役割 |
|---|---|
| `src-tauri/src/domain/mml.rs`(新規) | `NoteEvent` / `MmlScore` / `MmlError` / `MmlParseResult`(`specta::Type`) |
| `src-tauri/src/mml.rs`(新規) | MML パーサー(純粋関数)と `sound_choice` の保存時検証 |
| `src-tauri/src/commands/sound.rs`(変更) | `parse_mml` コマンドの追加。`mml:` が Rust まで届いた場合の `warn` |
| `src-tauri/src/commands/mute.rs`(変更) | `set_notify` で `sound_choice` を検証 |
| `src-tauri/src/commands/column.rs`(変更) | `set_column_notify` で `notify_sound_choice` を検証 |
| `src-tauri/src/lib.rs`, `commands/mod.rs`, `domain/mod.rs`(変更) | モジュールとコマンドの登録 |
| `frontend/src/lib/wav.ts`(新規) | PCM → WAV バイト列、`data:` URL 化 |
| `frontend/src/lib/mmlRender.ts`(新規) | `MmlScore` → Tone.js でレンダリング → WAV の `data:` URL |
| `frontend/src/lib/soundChoice.ts`(新規) | `mml:` の解決(パース → レンダリング)、メモ化、先読み |
| `frontend/src/lib/ipc.ts`(変更) | `playNotifySound` が再生前に `resolveSoundChoice` を待つ |
| `frontend/src/lib/store.svelte.ts`(変更) | 通知時のログ付き再生、先読みの呼び出し |
| `frontend/src/ui/settings/SoundChoicePicker.svelte`(新規) | 重複していた通知音選択 UI の共通化と MML モード |
| `frontend/src/ui/settings/NotifySection.svelte`, `frontend/src/ui/AddColumnModal.svelte`(変更) | 共通ピッカーに置き換え |
| `docs/guide/user-guide.md`(変更) | MML の説明と記法の表 |

---

### Task 1: Android での `Tone.Offline` 動作ゲート

リポジトリのコードは変更しない。結果をスペックに記録するだけ。Android WebView は Chrome と同じ Chromium エンジンなので、エミュレーターの Chrome で代用して確認する(実 WebView での最終確認は Task 8)。

**Files:**
- Modify: `docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md`(「実装前の検証ゲート」に結果を追記)

**Interfaces:**
- Consumes: なし
- Produces: ゲートの合否。不合格なら実装を止めてユーザーに報告する(C案への切り替えはスペックの書き直しから)。

- [ ] **Step 1: Android エミュレーターが使えるか確認する**

Run: `adb devices`
Expected: エミュレーターまたは実機が 1 台以上 `device` と表示される。表示されない場合は、ここで止めてユーザーに「Android 環境がない。ゲートをスキップして実装を進めるか」を確認する。

- [ ] **Step 2: 検証用ファイルを用意する(スクラッチパッドの新しい空ディレクトリに作る)**

```bash
GATE=<scratchpad>/android-gate && mkdir -p $GATE/tonepkg $GATE/www
cd $GATE/tonepkg && npm pack tone@15.1.22 --silent && tar xzf tone-15.1.22.tgz
cp $GATE/tonepkg/package/build/Tone.js $GATE/www/Tone.js
```

`$GATE/www/gate.html`:

```html
<!doctype html>
<meta charset="utf-8">
<script src="Tone.js"></script>
<script>
(async () => {
  const r = { ua: navigator.userAgent };
  try {
    r.ctx = Tone.getContext().state;
    const t0 = performance.now();
    const buf = await Tone.Offline(() => {
      new Tone.FMSynth().toDestination().triggerAttackRelease("C5", 0.3, 0);
    }, 0.5, 1, 22050);
    r.renderMs = Math.round(performance.now() - t0);
    const d = buf.get().getChannelData(0);
    r.peak = d.reduce((m, v) => Math.max(m, Math.abs(v)), 0);
  } catch (e) { r.error = String(e); }
  await fetch("/result", { method: "POST", body: JSON.stringify(r) });
})();
</script>
```

`$GATE/gate_server.py`(結果を標準出力に出す小さなサーバー):

```python
import http.server, pathlib, sys

root = pathlib.Path(sys.argv[1])

class H(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(root), **k)

    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        print("RESULT", body.decode(), flush=True)
        self.send_response(204)
        self.end_headers()

http.server.ThreadingHTTPServer(("127.0.0.1", 8765), H).serve_forever()
```

- [ ] **Step 3: サーバーを起動してエミュレーターから開く**

```bash
python3 $GATE/gate_server.py $GATE/www > $GATE/server.log 2>&1 &
echo $! > $GATE/server.pid
adb reverse tcp:8765 tcp:8765
adb shell am start -a android.intent.action.VIEW -d http://127.0.0.1:8765/gate.html
sleep 10; cat $GATE/server.log
```

Expected: `RESULT {"ua":"...Android...","ctx":"...","renderMs":...,"peak":0.xx}` が出る。`error` が無く `peak` が 0.01 より大きければ合格。

- [ ] **Step 4: 後始末**

```bash
kill $(cat $GATE/server.pid)
adb reverse --remove tcp:8765
```

- [ ] **Step 5: 結果をスペックに記録してコミットする**

スペックの「実装前の検証ゲート」の節の末尾に、結果(合否、`renderMs`、`peak`、`ctx`、使った端末の UA)を 1〜3 行で追記する。不合格ならここで止める。

```bash
git add docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md
git commit -m "docs: Issue #75 Android での Tone.Offline 検証結果を追記"
```

---

### Task 2: ドメイン型と MML パーサー(Rust)

**Files:**
- Create: `src-tauri/src/domain/mml.rs`
- Create: `src-tauri/src/mml.rs`
- Modify: `src-tauri/src/domain/mod.rs`(`mod mml;` と `pub use mml::{...}`)
- Modify: `src-tauri/src/lib.rs`(`mod mml;` を `mod mobile_intent;` の前に追加)

**Interfaces:**
- Consumes: なし
- Produces:
  - `crate::domain::{NoteEvent, MmlScore, MmlError, MmlParseResult}`(次のとおり)
  - `crate::mml::parse(text: &str) -> Result<MmlScore, MmlError>`
  - `crate::mml::{MAX_TEXT_CHARS, MAX_TRACKS, MAX_TOTAL_SEC}`

```rust
pub struct NoteEvent { pub track: u8, pub start_sec: f64, pub duration_sec: f64, pub midi: u8, pub velocity: f64 }
pub struct MmlScore { pub notes: Vec<NoteEvent>, pub total_sec: f64, pub track_count: u8 }
pub struct MmlError { pub position: u32, pub message: String }
#[serde(tag = "status")]
pub enum MmlParseResult { Parsed { score: MmlScore }, Invalid { error: MmlError } }
```

- [ ] **Step 1: ドメイン型を作る**

`src-tauri/src/domain/mml.rs`:

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

/// MML から展開した 1 音(Issue #75)。時刻は秒で、テンポ変化・繰り返し・タイは解決済み。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoteEvent {
    /// トラック番号(0 始まり)
    pub track: u8,
    pub start_sec: f64,
    pub duration_sec: f64,
    /// MIDI ノート番号(o4 の c が 60)
    pub midi: u8,
    /// 0.0〜1.0
    pub velocity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MmlScore {
    pub notes: Vec<NoteEvent>,
    /// 全トラックのうち最も長い再生時間(秒)
    pub total_sec: f64,
    pub track_count: u8,
}

/// MML の構文エラー。`position` は入力テキストの文字位置(0 始まり、空白を含む)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MmlError {
    pub position: u32,
    pub message: String,
}

/// `parse_mml` コマンドの結果。UI が位置を使うので、構文エラーも `Err` ではなくデータで返す。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum MmlParseResult {
    Parsed { score: MmlScore },
    Invalid { error: MmlError },
}
```

`src-tauri/src/domain/mod.rs` の `mod mute;` の次に `mod mml;` を、`pub use mute::MuteConfig;` の前に次を追加する:

```rust
pub use mml::{MmlError, MmlParseResult, MmlScore, NoteEvent};
```

`src-tauri/src/lib.rs` の `mod mobile_intent;` の直前に `mod mml;` を追加する。

- [ ] **Step 2: 失敗するテストを書く(パーサー本体はまだ無い)**

`src-tauri/src/mml.rs` を次の内容で作る(テストだけを先に置く。`parse` が無いのでコンパイルエラーになる):

```rust
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
        let midis = |t: &str| must_parse(t).notes.iter().map(|n| n.midi).collect::<Vec<_>>();
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
    fn loop_expansion_is_bounded_by_the_duration_limit() {
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
}
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib mml::`
Expected: コンパイルエラー(`cannot find function parse in this scope`)。

- [ ] **Step 4: パーサーを実装する**

`src-tauri/src/mml.rs` の先頭(`#[cfg(test)] mod tests` の前)に次を置く:

```rust
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
    MmlError { position: pos as u32, message: message.into() }
}

/// トラック 1 本分の命令。繰り返しはパース時に展開済みなので、ここには現れない。
#[derive(Debug, Clone)]
enum Op {
    Note { semitone: i32, len: Option<u32>, dots: u8 },
    Rest { len: Option<u32>, dots: u8 },
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
                Op::Note { semitone: base + accidental, len, dots }
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
/// 時間の積算は音符・休符ごとに行い、上限を超えた時点で打ち切る(展開後の長さに比例して固まらない)。
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
            Op::Note { semitone, len, dots } => {
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
        return Err(mml_err(MAX_TEXT_CHARS, "MML は 1000 文字以下にしてください"));
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
    Ok(MmlScore { notes, total_sec, track_count })
}

/// 保存時の検証。`mml:` で始まる `sound_choice` だけをパースし、不正なら拒否する。
/// それ以外の形式(空・プリセット ID・`data:`)は従来どおり検証しない。
pub fn validate_sound_choice(choice: &str) -> crate::error::Result<()> {
    if let Some(text) = choice.strip_prefix(SOUND_CHOICE_PREFIX) {
        parse(text).map_err(|e| {
            Error::Invalid(format!("MML が不正です (位置 {}): {}", e.position, e.message))
        })?;
    }
    Ok(())
}
```

注: このファイルは `crate::error::Result`(1 引数の別名)を import しない。`Result<T, MmlError>` は標準の `Result` で、`validate_sound_choice` だけが `crate::error::Result<()>` と完全修飾で書く。

さらに `validate_sound_choice` の単体テストを `mod tests` の末尾に追加する:

```rust
    #[test]
    fn validate_sound_choice_checks_only_mml() {
        assert!(validate_sound_choice("").is_ok());
        assert!(validate_sound_choice("beep").is_ok());
        assert!(validate_sound_choice("data:audio/wav;base64,AAAA").is_ok());
        assert!(validate_sound_choice("mml:c4d4").is_ok());
        assert!(matches!(validate_sound_choice("mml:"), Err(crate::error::Error::Invalid(_))));
        let err = validate_sound_choice("mml:x").unwrap_err();
        assert!(err.to_string().contains("位置 0"), "{err}");
    }
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib mml::`
Expected: 全テスト PASS。失敗したら、エラー位置のテスト(`must_fail(...).position`)の期待値と実装の位置の取り方(命令の先頭文字 / 数値の先頭 / `;` の位置)を突き合わせて直す。テストの期待値は Global Constraints とスペックから導いたものなので、実装側を直す。

- [ ] **Step 6: 警告とフォーマットを確認する**

Run: `cd src-tauri && cargo fmt --check && cargo clippy --lib -- -D warnings`
Expected: 指摘なし。`cargo clippy` が入っていない環境ならこのステップは `cargo fmt --check` のみ。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/mml.rs src-tauri/src/domain/mml.rs src-tauri/src/domain/mod.rs src-tauri/src/lib.rs
git commit -m "feat: MMLパーサーとドメイン型を追加(Issue #75)"
```

---

### Task 3: `parse_mml` コマンドと保存時の検証

**Files:**
- Modify: `src-tauri/src/commands/sound.rs`
- Modify: `src-tauri/src/commands/mod.rs:43`
- Modify: `src-tauri/src/commands/mute.rs:255-258`(`set_notify`)
- Modify: `src-tauri/src/commands/column.rs:712-722`(`set_column_notify`)
- Modify: `src-tauri/src/lib.rs:129`(`specta_builder()` と、`generates_frontend_bindings` テストのアサーション)
- Modify(再生成): `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: `crate::mml::{parse, validate_sound_choice, SOUND_CHOICE_PREFIX}`、`crate::domain::MmlParseResult`
- Produces: Tauri コマンド `parse_mml(text: String) -> MmlParseResult`(TS では `commands.parseMml(text): Promise<MmlParseResult>`)。`set_notify` / `set_column_notify` は不正な `mml:` を `Error::Invalid` で拒否する。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/sound.rs` の `mod tests` に追加する:

```rust
    #[test]
    fn mml_choice_that_reaches_rust_falls_back_to_beep() {
        // フロントがレンダリングに失敗したときの最後の砦(通知を止めない)
        assert_eq!(
            resolve_audio_bytes("mml:c4d4").unwrap().as_ref(),
            resolve_audio_bytes("beep").unwrap().as_ref()
        );
    }

    #[tokio::test]
    async fn parse_mml_returns_data_for_both_outcomes() {
        use crate::domain::MmlParseResult;
        match super::parse_mml("c4".to_string()).await {
            MmlParseResult::Parsed { score } => assert_eq!(score.notes.len(), 1),
            other => panic!("expected Parsed, got {other:?}"),
        }
        match super::parse_mml("c4 x".to_string()).await {
            MmlParseResult::Invalid { error } => assert_eq!(error.position, 3),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
```

`src-tauri/src/lib.rs` の `generates_frontend_bindings` の、他のアサーションの並びに追加する:

```rust
        // MML 着メロ(Issue #75)のコマンドと型
        assert!(ts.contains("parseMml"), "missing parseMml command");
        assert!(ts.contains("MmlParseResult"), "missing MmlParseResult type");
        assert!(ts.contains("startSec"), "NoteEvent.start_sec should be camelCase");
        assert!(ts.contains("\"parsed\"") && ts.contains("\"invalid\""), "MmlParseResult status tags");
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib sound:: && cargo test generates_frontend_bindings`
Expected: `parse_mml` が無いためコンパイルエラー、またはアサーション失敗。

- [ ] **Step 3: コマンドを実装する**

`src-tauri/src/commands/sound.rs`:

```rust
use crate::domain::MmlParseResult;
use crate::mml;
```

`play_notify_sound` の `match resolve_audio_bytes(...)` の直前に追加する:

```rust
    if choice.starts_with(mml::SOUND_CHOICE_PREFIX) {
        // フロント(ipc.ts)が mml: を data: に解決してから呼ぶ設計。ここに届くのは解決失敗の取りこぼし。
        log::warn!("通知音: MML がレンダリングされずに届いたため beep で代替します");
    }
```

(`resolve_audio_bytes` は、`mml:` を未知の文字列として `beep` に落とす既存の挙動のままにする。コードは変えず、上のテストで固定する。)

`parse_mml` コマンドを `play_notify_sound` の下に追加する:

```rust
/// MML を解釈して、時刻解決済みのノート列を返す(Issue #75)。構文エラーは Err ではなくデータで返す。
#[tauri::command]
#[specta::specta]
pub async fn parse_mml(text: String) -> MmlParseResult {
    match mml::parse(&text) {
        Ok(score) => MmlParseResult::Parsed { score },
        Err(error) => MmlParseResult::Invalid { error },
    }
}
```

`src-tauri/src/commands/mod.rs:43` を `pub use sound::{parse_mml, play_notify_sound};` に変更する。

`src-tauri/src/lib.rs:129` の `commands::sound::play_notify_sound,` の次の行に `commands::sound::parse_mml,` を追加する。

保存時の検証:

`src-tauri/src/commands/mute.rs` の `set_notify`:

```rust
pub async fn set_notify(state: State<'_, AppState>, config: NotifyConfig) -> Result<()> {
    crate::mml::validate_sound_choice(&config.sound_choice)?;
    state.settings.save_notify(&config)
}
```

`src-tauri/src/commands/column.rs` の `set_column_notify` の本体の先頭に追加する:

```rust
    crate::mml::validate_sound_choice(&notify_sound_choice)?;
```

- [ ] **Step 4: テストが通ることを確認する(バインディングも再生成される)**

Run: `cd src-tauri && cargo test --lib sound:: && cargo test generates_frontend_bindings && git diff --stat ../frontend/src/bindings/tauri.gen.ts`
Expected: PASS。`tauri.gen.ts` に `parseMml` / `MmlParseResult` / `NoteEvent` / `MmlScore` / `MmlError` が追加されている。

- [ ] **Step 5: Rust のテスト全体を確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS(`#[ignore]` のテストは除く)。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/sound.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/mute.rs src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: parse_mmlコマンドと通知音保存時のMML検証を追加(Issue #75)"
```

---

### Task 4: Tone.js 依存・WAV エンコーダ・レンダラー(フロント)

**Files:**
- Modify: `frontend/package.json`, `frontend/pnpm-lock.yaml`(`tone@15.1.22` を固定)
- Modify: `frontend/vite.config.ts`(`optimizeDeps.include` に `"tone"`)
- Create: `frontend/src/lib/wav.ts`
- Create: `frontend/src/lib/wav.test.ts`
- Create: `frontend/src/lib/mmlRender.ts`

**Interfaces:**
- Consumes: `MmlScore` / `NoteEvent`(`../bindings/tauri.gen`、Task 3 で再生成済み)
- Produces:
  - `encodeWavMono16(channels: Float32Array[], sampleRate: number): Uint8Array`
  - `wavToDataUrl(wav: Uint8Array): string`(`data:audio/wav;base64,...`)
  - `renderScore(score: MmlScore): Promise<string>`(WAV の `data:` URL)

- [ ] **Step 1: 依存を追加する**

Run: `cd frontend && pnpm add tone@15.1.22 --save-exact`
Expected: `package.json` の `dependencies` に `"tone": "15.1.22"` が入る。

`frontend/vite.config.ts` の `optimizeDeps.include` の配列に `"tone",` を追加する(遅延 import の初回で再最適化とフルリロードが走り、vite-plugin-svelte の仮想 CSS モジュール読み込みを壊すのを防ぐ。同ファイルの既存コメントを参照)。

- [ ] **Step 2: 失敗するテストを書く**

`frontend/src/lib/wav.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { encodeWavMono16, wavToDataUrl } from "./wav";

const ascii = (b: Uint8Array, from: number, to: number) => String.fromCharCode(...b.subarray(from, to));
const view = (b: Uint8Array) => new DataView(b.buffer, b.byteOffset, b.byteLength);

describe("encodeWavMono16", () => {
  it("書き出す WAV は 16bit モノラル PCM で、ヘッダと長さが正しい", () => {
    const wav = encodeWavMono16([new Float32Array(100)], 22050);
    const v = view(wav);
    expect(ascii(wav, 0, 4)).toBe("RIFF");
    expect(ascii(wav, 8, 12)).toBe("WAVE");
    expect(ascii(wav, 12, 16)).toBe("fmt ");
    expect(ascii(wav, 36, 40)).toBe("data");
    expect(v.getUint16(20, true)).toBe(1); // PCM
    expect(v.getUint16(22, true)).toBe(1); // mono
    expect(v.getUint32(24, true)).toBe(22050);
    expect(v.getUint16(34, true)).toBe(16);
    expect(v.getUint32(40, true)).toBe(200); // 100 サンプル × 2 バイト
    expect(v.getUint32(4, true)).toBe(wav.length - 8);
    expect(wav.length).toBe(44 + 200);
  });

  it("サンプルを 16bit に変換し、範囲外はクリップする", () => {
    const wav = encodeWavMono16([Float32Array.from([0, 0.25, -0.25, 1, -1, 2, -2])], 8000);
    const v = view(wav);
    const got = Array.from({ length: 7 }, (_, i) => v.getInt16(44 + i * 2, true));
    expect(got).toEqual([0, 8192, -8192, 32767, -32767, 32767, -32767]);
  });

  it("複数チャンネルは平均してモノラルにする", () => {
    const wav = encodeWavMono16([Float32Array.from([0.5, 0]), Float32Array.from([-0.5, 0.5])], 8000);
    const v = view(wav);
    expect(v.getInt16(44, true)).toBe(0);
    expect(v.getInt16(46, true)).toBe(8192); // (0 + 0.5) / 2 = 0.25
  });

  it("チャンネルが空でも例外にならず、データ長 0 の WAV を返す", () => {
    const wav = encodeWavMono16([], 8000);
    expect(view(wav).getUint32(40, true)).toBe(0);
  });
});

describe("wavToDataUrl", () => {
  it("base64 の data: URL にして、元のバイト列に戻せる", () => {
    const wav = encodeWavMono16([Float32Array.from([0.1, -0.2, 0.3])], 22050);
    const url = wavToDataUrl(wav);
    expect(url.startsWith("data:audio/wav;base64,")).toBe(true);
    const back = Uint8Array.from(atob(url.slice("data:audio/wav;base64,".length)), (c) => c.charCodeAt(0));
    expect(Array.from(back)).toEqual(Array.from(wav));
  });

  it("大きなバッファ(10 秒分)でも例外にならない", () => {
    const wav = encodeWavMono16([new Float32Array(22050 * 10)], 22050);
    expect(() => wavToDataUrl(wav)).not.toThrow();
  });
});
```

- [ ] **Step 3: 失敗を確認する**

Run: `cd frontend && npx vitest run src/lib/wav.test.ts`
Expected: FAIL(`./wav` が無い)。

- [ ] **Step 4: WAV エンコーダを実装する**

`frontend/src/lib/wav.ts`:

```ts
// PCM サンプルを 16bit モノラル WAV にする(Issue #75)。Tone.Offline の出力を data: URL にして
// 既存の play_notify_sound(Rust/rodio)に渡すために使う。

export function encodeWavMono16(channels: Float32Array[], sampleRate: number): Uint8Array {
  const length = channels[0]?.length ?? 0;
  const dataBytes = length * 2;
  const view = new DataView(new ArrayBuffer(44 + dataBytes));
  const writeStr = (offset: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(offset + i, s.charCodeAt(i));
  };
  writeStr(0, "RIFF");
  view.setUint32(4, 36 + dataBytes, true);
  writeStr(8, "WAVE");
  writeStr(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeStr(36, "data");
  view.setUint32(40, dataBytes, true);
  for (let i = 0; i < length; i++) {
    let s = 0;
    for (const ch of channels) s += ch[i] ?? 0;
    s /= channels.length;
    const clamped = Math.max(-1, Math.min(1, s));
    view.setInt16(44 + i * 2, Math.round(clamped * 0x7fff), true);
  }
  return new Uint8Array(view.buffer);
}

export function wavToDataUrl(wav: Uint8Array): string {
  let bin = "";
  const CHUNK = 0x4000;
  for (let i = 0; i < wav.length; i += CHUNK) {
    bin += String.fromCharCode(...wav.subarray(i, i + CHUNK));
  }
  return `data:audio/wav;base64,${btoa(bin)}`;
}
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/wav.test.ts`
Expected: PASS。

- [ ] **Step 6: レンダラーを実装する**

`frontend/src/lib/mmlRender.ts`(jsdom に Web Audio が無いので単体テストは書かず、Task 5 でモックし、Task 8 で実機確認する):

```ts
// MmlScore を Tone.js の Tone.Offline で WAV にする(Issue #75)。
// ここでは音を出さない。出力は data: URL で、再生は Rust(play_notify_sound)が行う。
// Tone.js は重いので遅延 import し、通常の起動コストに乗せない。
import type { MmlScore } from "../bindings/tauri.gen";
import { encodeWavMono16, wavToDataUrl } from "./wav";

const SAMPLE_RATE = 22050;
/** FM 音源の余韻(リリース)が切れないように足す秒数 */
const TAIL_SEC = 0.6;

export async function renderScore(score: MmlScore): Promise<string> {
  const Tone = await import("tone");
  const buffer = await Tone.Offline(
    () => {
      const synths = Array.from({ length: score.trackCount }, () =>
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 3,
          modulationIndex: 8,
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.4, release: 0.1 },
        }).toDestination(),
      );
      for (const n of score.notes) {
        const hz = Tone.Frequency(n.midi, "midi").toFrequency();
        synths[n.track].triggerAttackRelease(hz, n.durationSec, n.startSec, n.velocity);
      }
    },
    score.totalSec + TAIL_SEC,
    1,
    SAMPLE_RATE,
  );
  const audio = buffer.get();
  if (!audio) throw new Error("レンダリング結果が空でした");
  return wavToDataUrl(encodeWavMono16([audio.getChannelData(0)], audio.sampleRate));
}
```

- [ ] **Step 7: 型チェックする**

Run: `cd frontend && pnpm check`
Expected: エラーなし。`new Tone.PolySynth(Tone.FMSynth, {...})` のオプション引数の型でエラーが出た場合に限り、オプションのオブジェクトの後ろに `as never` を付けて黙らせる(動作は変えない。spike は JS で同じパラメータで動作確認済み)。

- [ ] **Step 8: Commit**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml frontend/vite.config.ts frontend/src/lib/wav.ts frontend/src/lib/wav.test.ts frontend/src/lib/mmlRender.ts
git commit -m "feat: ToneでMMLをWAVにレンダリングする処理を追加(Issue #75)"
```

---

### Task 5: `mml:` の解決・メモ化・`playNotifySound` への組み込み

**Files:**
- Create: `frontend/src/lib/soundChoice.ts`
- Create: `frontend/src/lib/soundChoice.test.ts`
- Create: `frontend/src/lib/ipc.playNotifySound.test.ts`
- Modify: `frontend/src/lib/ipc.ts:42-44`
- Modify: `frontend/src/lib/store.svelte.ts`(import、`boot()` の `this.notify = ...` の後、`setNotify`、通知時の 2 か所の `playNotifySound(...)`)

**Interfaces:**
- Consumes: `commands.parseMml(text): Promise<MmlParseResult>`(Task 3)、`renderScore(score): Promise<string>`(Task 4)
- Produces:
  - `MML_PREFIX = "mml:"`
  - `resolveSoundChoice(choice: string): Promise<string>`(`mml:` 以外はそのまま返す。失敗は reject)
  - `prewarmSoundChoice(choice: string): void`
  - `clearSoundChoiceCache(): void`(テスト用)
  - `playNotifySound(choice: string, onError?: (e: unknown) => void): void`(既存の呼び出しと互換)

- [ ] **Step 1: 失敗するテストを書く(`soundChoice`)**

`frontend/src/lib/soundChoice.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../bindings/tauri.gen", () => ({ commands: { parseMml: vi.fn() } }));
vi.mock("./mmlRender", () => ({ renderScore: vi.fn() }));

import { commands } from "../bindings/tauri.gen";
import { renderScore } from "./mmlRender";
import { clearSoundChoiceCache, prewarmSoundChoice, resolveSoundChoice } from "./soundChoice";

const parseMml = vi.mocked(commands.parseMml);
const render = vi.mocked(renderScore);

const SCORE = { notes: [], totalSec: 1, trackCount: 1 };
const parsed = { status: "parsed" as const, score: SCORE };

beforeEach(() => {
  clearSoundChoiceCache();
  parseMml.mockReset();
  render.mockReset();
  parseMml.mockResolvedValue(parsed);
  render.mockResolvedValue("data:audio/wav;base64,AAAA");
});

describe("resolveSoundChoice", () => {
  it("mml: 以外(空・プリセット・data:)はそのまま返し、パースもレンダリングもしない", async () => {
    for (const c of ["", "beep", "chime", "data:audio/wav;base64,AAAA"]) {
      expect(await resolveSoundChoice(c)).toBe(c);
    }
    expect(parseMml).not.toHaveBeenCalled();
    expect(render).not.toHaveBeenCalled();
  });

  it("mml: は パース → レンダリング の順に処理して data: URL を返す", async () => {
    expect(await resolveSoundChoice("mml:c4d4")).toBe("data:audio/wav;base64,AAAA");
    expect(parseMml).toHaveBeenCalledWith("c4d4");
    expect(render).toHaveBeenCalledWith(SCORE);
  });

  it("同じ MML は 2 回目以降メモ化された結果を返す", async () => {
    await resolveSoundChoice("mml:c4");
    await resolveSoundChoice("mml:c4");
    expect(parseMml).toHaveBeenCalledTimes(1);
    expect(render).toHaveBeenCalledTimes(1);
  });

  it("レンダリング中に同じ MML が再度要求されても二重にレンダリングしない", async () => {
    let finish!: (v: string) => void;
    render.mockReturnValue(new Promise<string>((r) => (finish = r)));
    const a = resolveSoundChoice("mml:c4");
    const b = resolveSoundChoice("mml:c4");
    await vi.waitFor(() => expect(render).toHaveBeenCalledTimes(1));
    finish("data:audio/wav;base64,BBBB");
    expect(await a).toBe("data:audio/wav;base64,BBBB");
    expect(await b).toBe("data:audio/wav;base64,BBBB");
    expect(render).toHaveBeenCalledTimes(1);
  });

  it("構文エラーは位置と理由を含む Error で reject し、失敗はキャッシュしない", async () => {
    parseMml.mockResolvedValueOnce({ status: "invalid", error: { position: 3, message: "不明な文字 `x` です" } });
    await expect(resolveSoundChoice("mml:c4 x")).rejects.toThrow(/位置 3.*不明な文字/);
    expect(render).not.toHaveBeenCalled();
    parseMml.mockResolvedValueOnce(parsed);
    await expect(resolveSoundChoice("mml:c4 x")).resolves.toBe("data:audio/wav;base64,AAAA");
  });

  it("レンダリング失敗は reject し、次の呼び出しでやり直せる", async () => {
    render.mockRejectedValueOnce(new Error("offline failed"));
    await expect(resolveSoundChoice("mml:c4")).rejects.toThrow("offline failed");
    await expect(resolveSoundChoice("mml:c4")).resolves.toBe("data:audio/wav;base64,AAAA");
    expect(render).toHaveBeenCalledTimes(2);
  });

  it("キャッシュは 16 件まで。古いものから捨てる", async () => {
    for (let i = 0; i < 17; i++) await resolveSoundChoice(`mml:c${i}`);
    expect(parseMml).toHaveBeenCalledTimes(17);
    await resolveSoundChoice("mml:c16"); // 新しいものは残っている
    expect(parseMml).toHaveBeenCalledTimes(17);
    await resolveSoundChoice("mml:c0"); // 最古は捨てられている
    expect(parseMml).toHaveBeenCalledTimes(18);
  });
});

describe("prewarmSoundChoice", () => {
  it("mml: だけを先にレンダリングし、失敗しても例外を投げない", async () => {
    prewarmSoundChoice("beep");
    expect(parseMml).not.toHaveBeenCalled();
    render.mockRejectedValueOnce(new Error("boom"));
    expect(() => prewarmSoundChoice("mml:c4")).not.toThrow();
    await vi.waitFor(() => expect(render).toHaveBeenCalledTimes(1));
  });
});
```

- [ ] **Step 2: 失敗するテストを書く(`playNotifySound`)**

`frontend/src/lib/ipc.playNotifySound.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../bindings/tauri.gen", () => ({ commands: { playNotifySound: vi.fn() } }));
vi.mock("./soundChoice", () => ({ resolveSoundChoice: vi.fn() }));

import { commands } from "../bindings/tauri.gen";
import { playNotifySound } from "./ipc";
import { resolveSoundChoice } from "./soundChoice";

const play = vi.mocked(commands.playNotifySound);
const resolve = vi.mocked(resolveSoundChoice);

beforeEach(() => {
  play.mockReset();
  resolve.mockReset();
  play.mockResolvedValue({ status: "ok", data: null });
});

describe("playNotifySound", () => {
  it("解決した値を Rust の play_notify_sound に渡す", async () => {
    resolve.mockResolvedValue("data:audio/wav;base64,AAAA");
    playNotifySound("mml:c4");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("data:audio/wav;base64,AAAA"));
  });

  it("プリセットなどはそのまま渡す", async () => {
    resolve.mockImplementation(async (c) => c);
    playNotifySound("chime");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("chime"));
  });

  it("解決に失敗したら beep で代替し、理由を onError に渡す(無音にしない)", async () => {
    const err = new Error("MML 構文エラー");
    resolve.mockRejectedValue(err);
    const onError = vi.fn();
    playNotifySound("mml:bad", onError);
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("beep"));
    expect(onError).toHaveBeenCalledWith(err);
  });

  it("onError が無くても解決失敗で beep を鳴らす", async () => {
    resolve.mockRejectedValue(new Error("x"));
    playNotifySound("mml:bad");
    await vi.waitFor(() => expect(play).toHaveBeenCalledWith("beep"));
  });

  it("IPC 自体が失敗しても未処理の reject を出さない", async () => {
    // Vitest は未処理の reject があるとテスト実行全体を失敗させるので、reject が漏れれば検知できる
    resolve.mockResolvedValue("beep");
    play.mockRejectedValue(new Error("ipc down"));
    playNotifySound("beep");
    await vi.waitFor(() => expect(play).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 20));
  });
});
```

- [ ] **Step 3: 失敗を確認する**

Run: `cd frontend && npx vitest run src/lib/soundChoice.test.ts src/lib/ipc.playNotifySound.test.ts`
Expected: FAIL(`./soundChoice` が無い)。

- [ ] **Step 4: `soundChoice.ts` を実装する**

`frontend/src/lib/soundChoice.ts`:

```ts
// sound_choice の "mml:<テキスト>" を、Rust が再生できる data: URL に解決する(Issue #75)。
// 解決 = Rust でパース → Tone.js でレンダリング。結果は choice をキーにメモ化し、
// 進行中の Promise ごと共有する(初回レンダリング中に通知が続いても二重に走らせない)。
import { commands } from "../bindings/tauri.gen";
import { renderScore } from "./mmlRender";

export const MML_PREFIX = "mml:";
const CACHE_LIMIT = 16;
const cache = new Map<string, Promise<string>>();

async function renderMml(text: string): Promise<string> {
  const r = await commands.parseMml(text);
  if (r.status !== "parsed") {
    throw new Error(`MML 構文エラー (位置 ${r.error.position}): ${r.error.message}`);
  }
  return renderScore(r.score);
}

/// mml: 以外はそのまま返す。mml: は data: URL に解決する(失敗したら reject。キャッシュはしない)。
export function resolveSoundChoice(choice: string): Promise<string> {
  if (!choice.startsWith(MML_PREFIX)) return Promise.resolve(choice);
  const hit = cache.get(choice);
  if (hit) {
    cache.delete(choice); // 末尾へ移して、最近使ったものを残す
    cache.set(choice, hit);
    return hit;
  }
  const p = renderMml(choice.slice(MML_PREFIX.length));
  cache.set(choice, p);
  p.catch(() => {
    if (cache.get(choice) === p) cache.delete(choice);
  });
  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
  return p;
}

/// 初回の遅れを隠すための先読み。結果は捨て、失敗しても何もしない。
export function prewarmSoundChoice(choice: string): void {
  if (!choice.startsWith(MML_PREFIX)) return;
  resolveSoundChoice(choice).catch(() => {});
}

/// テスト用。
export function clearSoundChoiceCache(): void {
  cache.clear();
}
```

- [ ] **Step 5: `ipc.ts` の `playNotifySound` を差し替える**

`frontend/src/lib/ipc.ts` の import 群に追加する: `import { resolveSoundChoice } from "./soundChoice";`

`playNotifySound`(42〜44 行)を次に置き換える。直前のドキュメントコメントの末尾に「mml: は再生前に data: URL へ解決し、失敗したら beep で代替する」旨を 1 行足す:

```ts
export function playNotifySound(choice: string, onError?: (e: unknown) => void): void {
  void (async () => {
    let resolved = choice;
    try {
      resolved = await resolveSoundChoice(choice);
    } catch (e) {
      onError?.(e);
      resolved = "beep"; // 解決できなくても通知は止めない
    }
    await unwrap(commands.playNotifySound(resolved));
  })().catch(() => {});
}
```

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/soundChoice.test.ts src/lib/ipc.playNotifySound.test.ts`
Expected: PASS。

- [ ] **Step 7: ストアに組み込む**

`frontend/src/lib/store.svelte.ts`:

1. 先頭付近の import に追加する: `import { prewarmSoundChoice } from "./soundChoice";`
2. `#resolveSoundChoice(tab)`(1878 行付近)の直後に追加する:

```ts
  /// 通知音を鳴らす。MML の生成に失敗して beep で代替した場合は、理由をデバッグログに残す。
  #playTabSound(tab: TabView) {
    playNotifySound(this.#resolveSoundChoice(tab), (e) =>
      this.#logDebug(`通知音の生成に失敗したため beep で代替します: ${String(e)}`),
    );
  }
```

3. 2 か所の `if (wantsSound) playNotifySound(this.#resolveSoundChoice(tab));`(998 行付近と 1086 行付近)を `if (wantsSound) this.#playTabSound(tab);` に置き換える。
4. `boot()` の `this.notify = { ...notify, soundChoice: notify.soundChoice ?? "" };`(294 行付近)の次の行に追加する: `prewarmSoundChoice(this.notify.soundChoice);`
5. `setNotify` の `this.notify = config;`(1530 行付近)の次の行に追加する: `prewarmSoundChoice(config.soundChoice);`

- [ ] **Step 8: 型チェックとフロント全テスト**

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS。`playNotifySound` の呼び出し側(`NotifySection.svelte`、`AddColumnModal.svelte`)は第 2 引数が任意なので無変更で通る。

- [ ] **Step 9: Commit**

```bash
git add frontend/src/lib/soundChoice.ts frontend/src/lib/soundChoice.test.ts frontend/src/lib/ipc.playNotifySound.test.ts frontend/src/lib/ipc.ts frontend/src/lib/store.svelte.ts
git commit -m "feat: sound_choiceのmml:を再生前にWAVへ解決する(Issue #75)"
```

---

### Task 6: 通知音ピッカーの共通化と MML 入力 UI

**Files:**
- Create: `frontend/src/ui/settings/SoundChoicePicker.svelte`
- Create: `frontend/src/ui/settings/SoundChoicePicker.test.ts`
- Modify: `frontend/src/ui/settings/NotifySection.svelte`
- Modify: `frontend/src/ui/AddColumnModal.svelte`(2 か所の同一ブロックと、関連する状態・import)

**Interfaces:**
- Consumes: `commands.parseMml`、`playNotifySound`(`../../lib/ipc`)、`MML_PREFIX`(`../../lib/soundChoice`)、`app.pickNotifySoundFile()` / `NOTIFY_SOUND_PRESETS`(`../../lib/store.svelte`)
- Produces: `<SoundChoicePicker bind:value allowInherit onvalidity />`
  - `value`: `sound_choice` 文字列(`""` / プリセット ID / `data:...` / `mml:...`)。bindable。
  - `allowInherit`: true のとき「継承(グローバル設定)」を選べ、`""` を継承として扱う。
  - `onvalidity(invalid: boolean)`: MML モードで構文が通っていない間は true。親は保存ボタンを止める。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/settings/SoundChoicePicker.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { commands } = await import("../../lib/ipc");
const { default: SoundChoicePicker } = await import("./SoundChoicePicker.svelte");

const okScore = { notes: [], totalSec: 2.5, trackCount: 1 };

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({ status: "parsed", score: okScore });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SoundChoicePicker の MML モード", () => {
  it("保存済みの mml: を開くと MML モードで、テキストエリアに本文が入る", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:t120 o5 c4" });
    expect((getByTestId("mml-input") as HTMLTextAreaElement).value).toBe("t120 o5 c4");
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("2.5"));
  });

  it("構文が通れば再生時間を出し、invalid=false を通知する", async () => {
    const onvalidity = vi.fn();
    render(SoundChoicePicker, { value: "mml:c4", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(false));
  });

  it("構文エラーは位置(1 始まり)と理由を出し、invalid=true を通知する", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue({
      status: "invalid",
      error: { position: 3, message: "不明な文字 `x` です" },
    });
    const onvalidity = vi.fn();
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4 x", onvalidity });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("4"));
    expect(getByTestId("mml-status").textContent).toContain("不明な文字");
    expect(onvalidity).toHaveBeenLastCalledWith(true);
  });

  it("本文が空なら parse_mml を呼ばずに invalid=true にする", async () => {
    const onvalidity = vi.fn();
    render(SoundChoicePicker, { value: "mml:", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(true));
    expect(commands.parseMml).not.toHaveBeenCalled();
  });

  it("入力すると 300ms 待って、最後のテキストだけをパースする", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c" });
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenCalledTimes(1));
    const input = getByTestId("mml-input") as HTMLTextAreaElement;
    await fireEvent.input(input, { target: { value: "cd" } });
    await fireEvent.input(input, { target: { value: "cde" } });
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenLastCalledWith("cde"));
    expect(commands.parseMml).toHaveBeenCalledTimes(2); // "c" と "cde"(途中の "cd" は呼ばれない)
  });

  it("プリセットのときは MML の入力欄を出さない", () => {
    const { queryByTestId } = render(SoundChoicePicker, { value: "chime" });
    expect(queryByTestId("mml-input")).toBeNull();
  });

  it("MML モードを抜けると invalid=false に戻す(親の保存ボタンを止めたままにしない)", async () => {
    const onvalidity = vi.fn();
    const { unmount } = render(SoundChoicePicker, { value: "mml:", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(true));
    unmount();
    expect(onvalidity).toHaveBeenLastCalledWith(false);
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/ui/settings/SoundChoicePicker.test.ts`
Expected: FAIL(`./SoundChoicePicker.svelte` が無い)。

- [ ] **Step 3: ピッカーを実装する**

`frontend/src/ui/settings/SoundChoicePicker.svelte`(既存の `NotifySection.svelte` / `AddColumnModal.svelte` の選択ロジックを統合し、MML モードを足す。`data:` のカスタム音声の挙動は既存のまま):

```svelte
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { app, NOTIFY_SOUND_PRESETS } from "../../lib/store.svelte";
  import { commands, playNotifySound } from "../../lib/ipc";
  import { MML_PREFIX } from "../../lib/soundChoice";
  import Dropdown from "../Dropdown.svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    value = $bindable(""),
    allowInherit = false,
    onvalidity,
  }: {
    value: string;
    allowInherit?: boolean;
    /// MML モードで構文が通っていない間は true。親は保存ボタンを止める。
    onvalidity?: (invalid: boolean) => void;
  } = $props();

  type Mode = "inherit" | "custom" | "mml" | (string & {});
  function modeFromChoice(choice: string): Mode {
    if (!choice) return allowInherit ? "inherit" : "beep";
    if (choice.startsWith("data:")) return "custom";
    if (choice.startsWith(MML_PREFIX)) return "mml";
    return choice;
  }

  let mode = $state<Mode>(untrack(() => modeFromChoice(value)));
  let mmlText = $state(untrack(() => (value.startsWith(MML_PREFIX) ? value.slice(MML_PREFIX.length) : "")));
  let pickingSound = $state(false);
  let pickErr = $state<string | null>(null);
  let previewErr = $state<string | null>(null);

  const modeOptions = [
    ...(allowInherit ? [{ value: "inherit", label: "継承（グローバル設定）" }] : []),
    ...NOTIFY_SOUND_PRESETS.map((p) => ({ value: p.id, label: p.label })),
    { value: "custom", label: "カスタム（音声ファイル）" },
    { value: "mml", label: "MML（打ち込み）" },
  ];

  $effect(() => {
    if (mode === "inherit") value = "";
    else if (mode === "custom") {
      if (!value.startsWith("data:")) value = "";
    } else if (mode === "mml") value = MML_PREFIX + mmlText;
    else value = mode;
  });

  type MmlStatus =
    | { kind: "idle" }
    | { kind: "pending" }
    | { kind: "ok"; totalSec: number }
    | { kind: "error"; position: number; message: string };
  let status = $state<MmlStatus>({ kind: "idle" });

  // 入力が止まって 300ms 後にパースする(初回だけ即時)。古い結果は捨てる。
  $effect(() => {
    if (mode !== "mml") return;
    const text = mmlText;
    if (!text.trim()) {
      status = { kind: "error", position: 0, message: "MML を入力してください" };
      return;
    }
    const first = untrack(() => status.kind === "idle");
    status = { kind: "pending" };
    const timer = setTimeout(
      async () => {
        const r = await commands.parseMml(text);
        if (text !== mmlText) return;
        status =
          r.status === "parsed"
            ? { kind: "ok", totalSec: r.score.totalSec }
            : { kind: "error", position: r.error.position, message: r.error.message };
      },
      first ? 0 : 300,
    );
    return () => clearTimeout(timer);
  });

  $effect(() => {
    onvalidity?.(mode === "mml" && status.kind !== "ok");
  });
  onMount(() => () => onvalidity?.(false));

  async function pickSound() {
    pickErr = null;
    pickingSound = true;
    try {
      const url = await app.pickNotifySoundFile();
      if (url) value = url;
    } catch (e) {
      pickErr = String(e);
    } finally {
      pickingSound = false;
    }
  }

  function preview(choice: string) {
    previewErr = null;
    playNotifySound(choice, (e) => (previewErr = String(e)));
  }
</script>

<Dropdown bind:value={mode} options={modeOptions} />
{#if mode === "custom"}
  <div class="flex items-center gap-2.5">
    <Button type="button" variant="outline" size="sm" disabled={pickingSound} onclick={pickSound}>
      {pickingSound ? "読み込み中…" : value.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
    </Button>
    {#if value.startsWith("data:")}
      <Button type="button" variant="outline" size="sm" onclick={() => preview(value)}>試聴</Button>
    {/if}
  </div>
{:else if mode === "mml"}
  <textarea
    class="resize-y rounded-md border border-border bg-muted px-[9px] py-[7px] font-mono text-xs text-foreground"
    rows="3"
    spellcheck="false"
    placeholder="t120 o5 l8 cdefgab>c"
    bind:value={mmlText}
    data-testid="mml-input"
  ></textarea>
  <p class="m-0 text-xs {status.kind === 'error' ? 'text-destructive' : 'text-muted-foreground'}" data-testid="mml-status">
    {#if status.kind === "ok"}再生時間 {status.totalSec.toFixed(1)} 秒
    {:else if status.kind === "error"}{status.position + 1} 文字目: {status.message}
    {:else}確認中…{/if}
  </p>
  <div class="flex items-center gap-2.5">
    <Button type="button" variant="outline" size="sm" disabled={status.kind !== "ok"} onclick={() => preview(value)}>試聴</Button>
  </div>
  <details class="text-xs text-muted-foreground">
    <summary class="cursor-pointer">記法</summary>
    <pre class="m-0 mt-1 whitespace-pre-wrap font-mono">音符 c d e f g a b（# や + で半音上げ、- で半音下げ）
長さ 音符の後ろに 1 2 4 8 16 32 64、付点は .（例: c8. ）
休符 r4  タイ c4&c8
o4 オクターブ指定  &gt; で上げる  &lt; で下げる
l8 既定の長さ  t120 テンポ  v10 音量(0〜15)
[cde]3 繰り返し(2〜99回、入れ子は不可)
; でトラックを分ける(最大4つ。テンポはトラックごとに書く)
全体で 1000 文字・10 秒まで</pre>
  </details>
{:else if mode !== "inherit"}
  <Button type="button" variant="outline" size="sm" onclick={() => preview(mode)}>試聴</Button>
{/if}
{#if pickErr}<p class="m-0 text-xs text-destructive">{pickErr}</p>{/if}
{#if previewErr}<p class="m-0 text-xs text-destructive">試聴に失敗しました: {previewErr}</p>{/if}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/settings/SoundChoicePicker.test.ts`
Expected: PASS。`status` のテキストの空白で `toContain` が合わない場合は、テスト側の期待を `textContent` の部分一致のままにして、テンプレートの空白を調整する。デバウンスのテスト(`toHaveBeenCalledTimes(2)`)がタイミングで揺れる場合は、`vi.useFakeTimers()` に切り替えて `await vi.advanceTimersByTimeAsync(300)` で進める。

- [ ] **Step 5: `NotifySection.svelte` を置き換える**

`frontend/src/ui/settings/NotifySection.svelte`:

- `<script>` から次を削除する: `untrack` の import、`NOTIFY_SOUND_PRESETS` の import、`playNotifySound` の import、`Dropdown` の import、`pickingSound`、`SoundMode` / `modeFromChoice` / `soundMode` / `soundModeOptions` / `$effect`、`pickSound`。
- 追加: `import SoundChoicePicker from "./SoundChoicePicker.svelte";` と `let soundInvalid = $state(false);`
- 通知音のブロック(`{#if sound}` の中の `<Dropdown ...>` から `{/if}`(`{:else}` の試聴ボタンまで)を次に置き換える:

```svelte
{#if sound}
  <div class="my-1 mb-3 flex flex-col gap-1.5 text-sm">
    <span class="text-muted-foreground">通知音の種類(既定。タブごとに上書き可)</span>
    <SoundChoicePicker bind:value={soundChoice} onvalidity={(v) => (soundInvalid = v)} />
  </div>
{/if}
```

- 保存ボタンを `<Button type="button" disabled={busy || soundInvalid} onclick={save}>` にする。

- [ ] **Step 6: `AddColumnModal.svelte` を置き換える**

`frontend/src/ui/AddColumnModal.svelte`:

- 状態とロジック(83〜114 行付近)のうち、`let notifySoundChoice = ...` は残し、次を削除する: `pickingSound`、`SoundMode` / `modeFromChoice` / `soundMode` / `soundModeOptions` / `$effect`、`pickSound`。追加: `let soundInvalid = $state(false);`
- import: `import SoundChoicePicker from "./settings/SoundChoicePicker.svelte";` を追加。`NOTIFY_SOUND_PRESETS`、`playNotifySound` の import は、他で使われていなければ削除する(`pnpm check` で未使用を確認)。`Dropdown` は他の箇所で使われていれば残す。
- 2 か所ある同一のブロック(`{#if notifySound}` の中の `<Dropdown bind:value={soundMode} ...>` から、`{:else if soundMode !== "inherit"} ... {/if}` まで)を、どちらも次に置き換える:

```svelte
    {#if notifySound}
      <div class="mb-2.5 flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">通知音の種類</span>
        <SoundChoicePicker bind:value={notifySoundChoice} allowInherit onvalidity={(v) => (soundInvalid = v)} />
      </div>
    {/if}
```

- 送信ボタン(`data-testid="add-column-submit"`)の `disabled` を `busy || !!filterErr || !!tqlErr || soundInvalid` にする。

- [ ] **Step 7: 型チェックとフロント全テスト**

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS。`AddColumnModal.test.ts` が通知音のドロップダウンの DOM に依存していて失敗した場合は、`SoundChoicePicker` の構造に合わせてテストを更新する(挙動の期待は変えない)。

- [ ] **Step 8: Commit**

```bash
git add frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts frontend/src/ui/settings/NotifySection.svelte frontend/src/ui/AddColumnModal.svelte
git commit -m "feat: 通知音の選択UIを共通化しMML入力モードを追加(Issue #75)"
```

---

### Task 7: ユーザーガイド

**Files:**
- Modify: `docs/guide/user-guide.md`(65 行付近の「通知音はプリセットの他、任意の音声ファイルを選んで…」の段落の直後と、149 行付近の「通知」の項)

**Interfaces:**
- Consumes: なし
- Produces: なし

- [ ] **Step 1: ガイドを更新する**

65 行付近の段落の直後に、次の小節を追加する:

````markdown
#### MML で通知音を作る

通知音の種類で「MML（打ち込み）」を選ぶと、着メロのように音符をテキストで打ち込んで通知音にできます。入力欄の下に再生時間が出れば構文は正しく、「試聴」で鳴らせます。書き方が誤っているときは、何文字目が原因かが表示され、直すまで保存できません。

例: `t120 o5 l8 cdefgab>c`

| 書き方 | 意味 |
|---|---|
| `c d e f g a b` | 音符。`#` または `+` で半音上げ、`-` で半音下げ |
| 音符の後ろの `1 2 4 8 16 32 64` | 長さ（4 が四分音符）。`.` を付けると付点（最大 2 個） |
| `r` | 休符（長さは音符と同じ書き方） |
| `&` | タイ。`c4&c8` のように同じ高さの音符をつなぐ |
| `o4`、`>`、`<` | オクターブ指定（1〜8、既定 4）、1 つ上げる、1 つ下げる |
| `l8` | 以降の既定の長さ（既定は 4） |
| `t120` | テンポ（32〜255、既定 120） |
| `v10` | 音量（0〜15、既定 10） |
| `[cde]3` | 繰り返し（2〜99 回、入れ子は不可） |
| `;` | トラックの区切り（最大 4 つ。和音になります） |

大文字小文字と空白・改行は区別しません。全体で 1000 文字、再生時間は 10 秒までです。トラックごとにテンポ・音量・オクターブは独立していて、テンポを揃えたいときは各トラックに `t` を書いてください。音色は全トラックで共通です。

保存した MML は、設定→通知のグローバル設定と、タブごとの通知音の両方で使えます。何らかの理由で音を作れなかったときは、通知を止めずにビープ音で代わりに鳴らします。
````

149 行付近の「通知」の項の末尾に 1 文追加する: `通知音には MML で打ち込んだ音も選べます（上の「MML で通知音を作る」を参照）。`

- [ ] **Step 2: Commit**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: ユーザーガイドにMMLで通知音を作る方法を追加(Issue #75)"
```

---

### Task 8: 全体の検証と実機確認

**Files:**
- なし(失敗した場合のみ該当ファイルを直す)。確認結果はスペックの末尾「実装後の確認」に追記してコミットする。

**Interfaces:**
- Consumes: Task 1〜7 の成果
- Produces: 検証結果の記録

- [ ] **Step 1: 自動テストを全て回す**

Run:
```bash
cd src-tauri && cargo fmt --check && cargo test
cd ../frontend && pnpm check && pnpm test
```
Expected: 全て PASS。

- [ ] **Step 2: 実アプリを隔離して起動する(Xvfb・仮想ディスプレイ越し)**

メモリの手順に従う: `Xvfb` + `dbus-run-session` + `gnome-keyring-daemon`(`e2e/scripts/run-app.sh` を参考にする)、`WAYLAND_DISPLAY` を unset して `GDK_BACKEND=x11`、`XDG_CACHE_HOME` / `XDG_CONFIG_HOME` / `XDG_DATA_HOME` は `/tmp` 直下の短いパス(debug bridge のソケットパス長の制限)。起動したプロセスには一意なトークン環境変数を付け、後始末は正確な PID だけを `kill` する(`pkill` / `killall` 禁止)。`cargo tauri dev` はリポジトリルートから実行する。debug bridge の JS は同期関数として実行されるため `await` は使えず、`window.__result` に書いて別リクエストで読む(kick-and-poll)。

- [ ] **Step 3: 次を実アプリで確認する**

1. 設定→通知で「通知音を鳴らす」を ON にし、種類を「MML（打ち込み）」にして `t120 o5 l8 cdefgab>c` を入力する。再生時間が表示される。
2. 「試聴」でエラー表示が出ない。`WAYLAND_DISPLAY` / ディスプレイの制約で音そのものが聞こえない環境では、Rust 側の `warn` ログが出ていないこと、`playNotifySound` が `beep` に落ちていない(onError が呼ばれていない)ことをデバッグログで確認する。
3. `c4 x` のような不正な MML で、位置と理由が表示され、保存ボタンが無効になる。
4. 有効な MML を保存して、アプリを再起動し、同じ MML が入力欄に復元される。
5. カラム追加モーダル(通知タブ)でも同様に MML を選べて、保存・再編集できる。不正な MML のとき送信ボタンが無効になる。
6. 通知カラムに新着を発生させ(隔離した Misskey を使う `e2e/` の Compose 環境、または `play_notify_sound` を debug bridge から直接呼ぶ)、MML の通知音が鳴る経路(`mml:` → `data:` → rodio)に例外が出ない。

- [ ] **Step 4: 後始末を確認する**

起動したプロセスを PID で全て止め、隔離した `/tmp` のディレクトリを削除し、5173 に自分のプロセスが残っていないことを `ss -ltnp` で確認する。

- [ ] **Step 5: 結果をスペックに記録してコミットする**

スペックの末尾に「実装後の確認」節を追加し、上の 1〜6 の結果(通った項目、確認できなかった項目とその理由)を書く。Android 実機/エミュレーターの WebView での確認は、できた範囲を書く。

```bash
git add docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md
git commit -m "docs: Issue #75 実装後の確認結果を追記"
```

- [ ] **Step 6: ブランチを仕上げる**

`superpowers:finishing-a-development-branch` を使う。PR 本文は `.github/pull_request_template.md` の構成に従い、`Fixes #75` を入れる。マージは通常のマージコミット(`gh pr merge --merge`)。push 後に CI を待つ処理は書かない(ユーザーが確認する)。
