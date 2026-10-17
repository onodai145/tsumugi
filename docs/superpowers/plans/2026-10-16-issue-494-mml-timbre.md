# MML 着メロの音色切り替え Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に `@<番号>` / `@<名前>` を足し、トラックごと・途中でも 14 種の音色を切り替えられるようにする。

**Architecture:** Rust に `Timbre` 列挙型を 1 つ置き、パーサーが `@` を解釈して各 `NoteEvent` に `timbre` を入れる。生成バインディングでフロントに型が渡る。フロントは `lib/mmlTimbres.ts` の `Record<Timbre, TimbrePreset>`(音源の作り方・音量補正 `gainDb`・リリース)で音色を引き、`renderScore` が `(トラック, 音色)` ごとに音源を作って `Tone.Offline` で WAV にする。再生と保存形式(`mml:<テキスト>`)は #75 のまま。

**Tech Stack:** Rust(tauri-specta)、Svelte 5 + TypeScript、Vitest、`tone@15.1.22`(既存)。

**Spec:** `docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md`(関連: `2026-10-15-issue-75-mml-ringtone-design.md`)

## Global Constraints

- 音色は 14 種。番号と名前は次のとおり(Rust の `Timbre::ALL` の並び順が番号):
  `0 fm / 1 square / 2 pulse / 3 triangle / 4 saw / 5 sine / 6 organ / 7 epiano / 8 bell / 9 am / 10 lead / 11 bass / 12 drum / 13 noise`
- `@` を使わない MML は全ノートが `fm` で、音は今と同じ(`fm` の音源パラメータと `gainDb: 0` を変えない)。`fm` は `harmonicity: 3, modulationIndex: 8, envelope: { attack: 0.005, decay: 0.1, sustain: 0.4, release: 0.1 }`。
- 状態: トラックごとに独立、各トラックの先頭は `@0`。途中で切り替え可。`[...]n` の中に書いてもよい。タイでつないだ音は先頭の音の音色のまま。
- 名前は表の名前に一致して読み(どの名前も他の名前の接頭辞ではない)、残りは次の命令。空白は先に除かれるので `@bell c` と `@bellc` は同じ。`@3c` のように数字の直後に音符を続けてもよい。
- エラー(位置は 0 始まりの元テキストの文字位置): `@` の直後が数字でも名前でもない → 「不明な音色です」(位置は `@` の次の文字)。番号 14 以上 → 「音色は 0〜13 です」(位置は数字の先頭)。`@` がトラックの末尾(直後に何もない) → 「`@` の後に音色の番号か名前が必要です」(位置は `@`)。
- 既存の上限(1000 文字・10 秒・4 トラック)、`sound_choice` の形式(`mml:<テキスト>`)、`NotifyConfig` / `ColumnDef` のフィールド、`play_notify_sound` / `resolve_audio_bytes` のシグネチャは変えない。
- 全プリセットのリリース(`releaseSec`)は 0.6 秒以下(`TAIL_SEC` 以下)。エンベロープは全プリセットで明示する(Tone.js の既定はリリース約 1 秒)。
- WAV は 22.05kHz・モノラル・16bit。音量は `preset.gainDb` に、複数トラックのときだけ `gainToDb(1 / trackCount)` を足す。
- 生成物 `frontend/src/bindings/tauri.gen.ts` は手で編集せず、`cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- コミットメッセージは件名のみ(本文なし)。セッションの指示で決まる帰属トレーラーだけを別段落に付ける。`--no-verify` / `--no-gpg-sign` は使わない。コミットが失敗したら止めてユーザーに報告する(再試行しない)。
- 新しい UI は `docs/design/style-guide.md` に従い、`rounded-[Npx]` などの即値を作らない。
- ユーザーの `cargo tauri dev` はメインのワーキングツリーを監視してホットリビルドする(ブランチ上の編集やコミットがそのまま走る)。実アプリの確認は、必ず Xvfb 越し(`WAYLAND_DISPLAY` を unset、`GDK_BACKEND=x11`、`dbus-run-session`、短い `/tmp` 直下の `XDG_*`)で行う。プロセスの後始末は正確な PID だけを `kill` し、`pkill` / `killall` は使わない。`./target/debug/tsumugi` を直接起動するときも隔離した環境でだけ行う。自分で起動した開発サーバーやプロセスは、タスクの完了前に自分で止める。

## Review Focus

- `@` の直後が名前の一部に見えるが一致しない入力(`@bel c`、`@xyz`、全角の `＠bell`): 位置つきの構文エラーになり、`fm` に黙って落ちない(Task 1)。
- 名前の直後に音符・コマンドが続く入力(`@bellc`、`@amc4`、`@sawt60c`、`@Bell c`): 一意に読め、大文字小文字は無視される(Task 1)。
- タイの途中での音色変更(`c4&@2c4`): 1 音に連結され、先頭の音の音色のまま(Task 1)。
- トラックをまたぐ状態の漏れと繰り返し内の `@`(`@1 c;c`、`[@1c @2c]2`): 別トラックは `fm` から始まり、繰り返しは展開後の各回で再適用される(Task 1)。
- 音色が違う 4 トラックを同時に鳴らしたとき、WAV 化でクリップしない(ピークが 1 未満)。単音のピークは 0.36 以下(Task 4 の実描画)。
- 1 音ごとに音源を作る `noise` で、200 音規模でも許容時間(数秒以内)で描画できる(Task 4)。
- 保存済みの `@` なし MML が、変更前と同じ音(`fm`・補正 0 dB)で描画される(Task 3 の単体テストと Task 4 の実描画で `fm` の基準値を確認)。

---

### Task 1: Rust — `Timbre` 型と `@` のパース

**Files:**
- Modify: `src-tauri/src/domain/mml.rs`(`Timbre` を追加、`NoteEvent` に `timbre` を足す)
- Modify: `src-tauri/src/domain/mod.rs:33`(`Timbre` を再エクスポート)
- Modify: `src-tauri/src/mml.rs`(`Op::Timbre`、`Cursor::timbre_name`、`parse_track` の `'@'` 分岐、`TrackState.timbre`、`run_track`、テスト)
- Modify: `src-tauri/src/lib.rs:484` 付近(バインディングのテストに `Timbre` の assert を足す)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Produces: `crate::domain::Timbre`(`Fm, Square, Pulse, Triangle, Saw, Sine, Organ, Epiano, Bell, Am, Lead, Bass, Drum, Noise`)。`Timbre::ALL: [Timbre; 14]`、`Timbre::from_number(u32) -> Option<Timbre>`、`Timbre::name(self) -> &'static str`。`NoteEvent.timbre: Timbre`(TS では `timbre: Timbre`、`Timbre` は `"fm" | "square" | …` の文字列ユニオン)。
- Consumes: 既存の `parse` / `run_track` / `Cursor`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/mml.rs` の `mod tests` の先頭の `use crate::domain::{MmlError, MmlScore};` を `use crate::domain::{MmlError, MmlScore, Timbre};` に変え、`mod tests` の末尾(`validate_sound_choice_checks_only_mml` の後)に次を追加する。

```rust
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
```

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認する**

Run: `cd src-tauri && cargo test --lib mml:: 2>&1 | tail -20`
Expected: `Timbre` が未定義でコンパイルエラー。

- [ ] **Step 3: `Timbre` 型を追加する**

`src-tauri/src/domain/mml.rs` の `NoteEvent` の直前に追加する。

```rust
/// 音色(Issue #494)。MML の `@<番号>` / `@<名前>` で指定する。番号は `ALL` の並び順(0 始まり)。
/// 番号と名前は、フロントの `lib/mmlTimbres.ts` と同じにすること(両側のテストが同じ表を持つ)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Timbre {
    Fm,
    Square,
    Pulse,
    Triangle,
    Saw,
    Sine,
    Organ,
    Epiano,
    Bell,
    Am,
    Lead,
    Bass,
    Drum,
    Noise,
}

impl Timbre {
    /// 番号順の一覧。
    pub const ALL: [Timbre; 14] = [
        Timbre::Fm,
        Timbre::Square,
        Timbre::Pulse,
        Timbre::Triangle,
        Timbre::Saw,
        Timbre::Sine,
        Timbre::Organ,
        Timbre::Epiano,
        Timbre::Bell,
        Timbre::Am,
        Timbre::Lead,
        Timbre::Bass,
        Timbre::Drum,
        Timbre::Noise,
    ];

    pub fn from_number(n: u32) -> Option<Timbre> {
        Self::ALL.get(n as usize).copied()
    }

    /// MML の `@<名前>` で使う名前(小文字)。serde の出力と同じ。
    pub fn name(self) -> &'static str {
        match self {
            Timbre::Fm => "fm",
            Timbre::Square => "square",
            Timbre::Pulse => "pulse",
            Timbre::Triangle => "triangle",
            Timbre::Saw => "saw",
            Timbre::Sine => "sine",
            Timbre::Organ => "organ",
            Timbre::Epiano => "epiano",
            Timbre::Bell => "bell",
            Timbre::Am => "am",
            Timbre::Lead => "lead",
            Timbre::Bass => "bass",
            Timbre::Drum => "drum",
            Timbre::Noise => "noise",
        }
    }
}
```

`NoteEvent` の末尾(`velocity` の後)にフィールドを足す。

```rust
    /// 音色(`@` で指定。指定が無ければ `fm`)
    pub timbre: Timbre,
```

`src-tauri/src/domain/mod.rs:33` を `pub use mml::{MmlError, MmlParseResult, MmlScore, NoteEvent, Timbre};` にする。

- [ ] **Step 4: パーサーを実装する**

`src-tauri/src/mml.rs`:

1. `use crate::domain::{MmlError, MmlScore, NoteEvent};` を `use crate::domain::{MmlError, MmlScore, NoteEvent, Timbre};` にする。
2. `enum Op` の末尾(`Volume(u8),` の後)に `Timbre(Timbre),` を足す。
3. `impl Cursor<'_>` の `number` の後に追加する。

```rust
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
```

4. `parse_track` の `'v' => { … }` 分岐の後(`'[' =>` の前)に追加する。

```rust
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
```

5. `struct TrackState` に `timbre: Timbre,` を足し、`run_track` の初期化に `timbre: Timbre::Fm,` を足す。
6. `run_track` の `Op::Volume(n) => …` の後に `Op::Timbre(t) => st.timbre = *t,` を足す。
7. `notes.push(NoteEvent { … })` に `timbre: st.timbre,` を足す(タイで連結する分岐は何も変えない = 先頭の音の音色のまま)。

- [ ] **Step 5: バインディングのテストに assert を足す**

`src-tauri/src/lib.rs` の `assert!(ts.contains("\"parsed\"") && …)` の行(484 行付近)の直後に追加する。

```rust
        assert!(ts.contains("export type Timbre"), "missing Timbre type");
        assert!(ts.contains("\"epiano\""), "Timbre variants must be camelCase strings");
```

- [ ] **Step 6: テストを実行して通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib mml:: 2>&1 | tail -15`
Expected: 全て PASS(既存のテストも含む。`@` を「不明な文字」として期待する既存テストがあれば、`@` 以外の文字に直す)。

Run: `cd src-tauri && cargo test generates_frontend_bindings 2>&1 | tail -5`
Expected: PASS。`git diff frontend/src/bindings/tauri.gen.ts` に `export type Timbre = …` と `NoteEvent` の `timbre: Timbre` が出る。

Run: `cd src-tauri && cargo test 2>&1 | grep "test result"` と `cd frontend && pnpm check 2>&1 | tail -5`
Expected: `cargo test` は failed 0。`pnpm check` は 0 errors(既存の警告 1 件のみ)。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/domain/mml.rs src-tauri/src/domain/mod.rs src-tauri/src/mml.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: MMLに音色指定(@)を追加しNoteEventに音色を持たせる(Issue #494)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: フロント — 音色プリセット表 `lib/mmlTimbres.ts`

**Files:**
- Create: `frontend/src/lib/mmlTimbres.ts`
- Create: `frontend/src/lib/mmlTimbres.test.ts`
- Modify: `frontend/src/lib/mmlRender.ts`(`TAIL_SEC` を `export` するだけ)

**Interfaces:**
- Consumes: `Timbre`(`../bindings/tauri.gen`)。
- Produces:
  - `type ToneModule = typeof import("tone")`
  - `interface Voice { trigger(hz: number, durationSec: number, startSec: number, velocity: number): void }`
  - `interface TimbrePreset { number: number; name: Timbre; gainDb: number; releaseSec: number; perNote?: boolean; create(Tone: ToneModule, volumeDb: number): Voice }`
  - `const TIMBRES: Record<Timbre, TimbrePreset>`、`const TIMBRE_LIST: TimbrePreset[]`(番号順)。
  - `create` は `Tone.Offline` のコールバック内で呼ばれ、出力先(`toDestination()`)と音量(`volumeDb`)を設定済みの `Voice` を返す。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlRender.ts` の `const TAIL_SEC = 0.6;` を `export const TAIL_SEC = 0.6;` にする。`frontend/src/lib/mmlTimbres.test.ts` を作る。

```ts
import { describe, expect, it } from "vitest";
import type { Timbre } from "../bindings/tauri.gen";
import { TAIL_SEC } from "./mmlRender";
import { TIMBRES, TIMBRE_LIST } from "./mmlTimbres";

// Rust 側(domain/mml.rs の Timbre::ALL と mml.rs の TIMBRE_TABLE)と同じ並び。変えるときは両方を直す。
const EXPECTED: [number, Timbre][] = [
  [0, "fm"],
  [1, "square"],
  [2, "pulse"],
  [3, "triangle"],
  [4, "saw"],
  [5, "sine"],
  [6, "organ"],
  [7, "epiano"],
  [8, "bell"],
  [9, "am"],
  [10, "lead"],
  [11, "bass"],
  [12, "drum"],
  [13, "noise"],
];

describe("TIMBRES", () => {
  it("番号と名前が Rust 側の表と一致する", () => {
    expect(TIMBRE_LIST.map((p) => [p.number, p.name])).toEqual(EXPECTED);
  });

  it("キーと name が一致し、全 Timbre にプリセットがある", () => {
    for (const [, name] of EXPECTED) expect(TIMBRES[name].name).toBe(name);
    expect(Object.keys(TIMBRES).sort()).toEqual(EXPECTED.map(([, n]) => n).sort());
  });

  it("全プリセットのリリースは TAIL_SEC 以下で、gainDb は有限の妥当な範囲", () => {
    for (const p of TIMBRE_LIST) {
      expect(p.releaseSec, p.name).toBeGreaterThan(0);
      expect(p.releaseSec, p.name).toBeLessThanOrEqual(TAIL_SEC);
      expect(Number.isFinite(p.gainDb), p.name).toBe(true);
      expect(p.gainDb, p.name).toBeGreaterThanOrEqual(-30);
      expect(p.gainDb, p.name).toBeLessThanOrEqual(6);
    }
  });

  it("fm は現行の音(補正 0 dB)のまま", () => {
    expect(TIMBRES.fm.gainDb).toBe(0);
    expect(TIMBRES.fm.releaseSec).toBe(0.1);
  });

  it("単音用の音源だけが perNote である", () => {
    expect(TIMBRE_LIST.filter((p) => p.perNote).map((p) => p.name)).toEqual(["noise"]);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/mmlTimbres.test.ts`
Expected: `./mmlTimbres` が見つからず FAIL。

- [ ] **Step 3: `mmlTimbres.ts` を実装する**

`gainDb` は spike の実測からの**初期値**で、Task 4 で実描画を測って確定する。型エラー(Tone.js のオプション型が厳しい箇所)は、`as never` のような広い逃げではなく、そのプリセットのオプションだけに `satisfies` や個別の型注釈を当てて直す。

```ts
// MML の音色(@番号 / @名前)ごとの音源の作り方(Issue #494)。
// 音色の番号と名前は Rust の Timbre::ALL(src-tauri/src/domain/mml.rs)と同じにする。
// gainDb は、C4 の単音の聞こえる大きさを fm(現行の音)にそろえる補正。実描画で測って決めている(docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md)。
import type { Timbre } from "../bindings/tauri.gen";

export type ToneModule = typeof import("tone");

/** Offline コンテキスト内に作った音源 1 つ。 */
export interface Voice {
  trigger(hz: number, durationSec: number, startSec: number, velocity: number): void;
}

export interface TimbrePreset {
  /** MML の `@<番号>` */
  number: number;
  /** MML の `@<名前>`(`Timbre` と同じ文字列) */
  name: Timbre;
  /** 聞こえる大きさをそろえる補正(dB) */
  gainDb: number;
  /** リリース時間(秒)。`TAIL_SEC` 以下にする */
  releaseSec: number;
  /** true なら 1 音ごとに音源を作る(単音用で、同じ音源に時刻の重なる音を鳴らせない) */
  perNote?: boolean;
  /** `Tone.Offline` のコールバック内で呼ぶ。出力先と音量(`volumeDb`)を設定済みの音源を返す。 */
  create(Tone: ToneModule, volumeDb: number): Voice;
}

interface PolyLike {
  volume: { value: number };
  toDestination(): unknown;
  triggerAttackRelease(note: number, duration: number, time: number, velocity: number): unknown;
}

function poly(synth: PolyLike, volumeDb: number): Voice {
  synth.toDestination();
  synth.volume.value = volumeDb;
  return { trigger: (hz, d, t, v) => synth.triggerAttackRelease(hz, d, t, v) };
}

const BASIC_ENV = { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.1 };
const basic = (oscillator: Record<string, unknown>) => (Tone: ToneModule, db: number) =>
  poly(new Tone.PolySynth(Tone.Synth, { oscillator, envelope: BASIC_ENV } as never), db);

export const TIMBRES: Record<Timbre, TimbrePreset> = {
  fm: {
    number: 0,
    name: "fm",
    gainDb: 0,
    releaseSec: 0.1,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 3,
          modulationIndex: 8,
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.4, release: 0.1 },
        }),
        db,
      ),
  },
  square: { number: 1, name: "square", gainDb: -10.2, releaseSec: 0.1, create: basic({ type: "square" }) },
  pulse: { number: 2, name: "pulse", gainDb: -11.6, releaseSec: 0.1, create: basic({ type: "pulse", width: 0.25 }) },
  triangle: { number: 3, name: "triangle", gainDb: -6.9, releaseSec: 0.1, create: basic({ type: "triangle" }) },
  saw: { number: 4, name: "saw", gainDb: -5.3, releaseSec: 0.1, create: basic({ type: "sawtooth" }) },
  sine: { number: 5, name: "sine", gainDb: -8.7, releaseSec: 0.1, create: basic({ type: "sine" }) },
  organ: {
    number: 6,
    name: "organ",
    gainDb: -14.3,
    releaseSec: 0.05,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.Synth, {
          oscillator: { type: "custom", partials: [1, 0.6, 0.4, 0.25, 0.15] },
          envelope: { attack: 0.01, decay: 0.05, sustain: 1, release: 0.05 },
        } as never),
        db,
      ),
  },
  epiano: {
    number: 7,
    name: "epiano",
    gainDb: 0,
    releaseSec: 0.3,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 2,
          modulationIndex: 3,
          envelope: { attack: 0.005, decay: 0.6, sustain: 0.2, release: 0.3 },
          modulationEnvelope: { attack: 0.005, decay: 0.3, sustain: 0.3, release: 0.3 },
        }),
        db,
      ),
  },
  bell: {
    number: 8,
    name: "bell",
    gainDb: 0,
    releaseSec: 0.5,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.FMSynth, {
          harmonicity: 5.07,
          modulationIndex: 12,
          envelope: { attack: 0.001, decay: 0.8, sustain: 0.05, release: 0.5 },
          modulationEnvelope: { attack: 0.001, decay: 0.4, sustain: 0.1, release: 0.4 },
        }),
        db,
      ),
  },
  am: {
    number: 9,
    name: "am",
    gainDb: -1.4,
    releaseSec: 0.2,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.AMSynth, {
          envelope: { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.2 },
          modulationEnvelope: { attack: 0.005, decay: 0.1, sustain: 0.5, release: 0.2 },
        }),
        db,
      ),
  },
  lead: {
    number: 10,
    name: "lead",
    gainDb: -20.3,
    releaseSec: 0.15,
    create: (Tone, db) => {
      const voice = { envelope: { attack: 0.01, decay: 0.1, sustain: 0.6, release: 0.15 } };
      return poly(
        new Tone.PolySynth(Tone.DuoSynth, {
          harmonicity: 1,
          vibratoAmount: 0.3,
          vibratoRate: 5,
          voice0: voice,
          voice1: voice,
        } as never),
        db,
      );
    },
  },
  bass: {
    number: 11,
    name: "bass",
    gainDb: -14.5,
    releaseSec: 0.2,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.MonoSynth, {
          oscillator: { type: "sawtooth" },
          filter: { Q: 1, type: "lowpass", rolloff: -24 },
          envelope: { attack: 0.01, decay: 0.2, sustain: 0.5, release: 0.2 },
          filterEnvelope: { attack: 0.01, decay: 0.2, sustain: 0.5, release: 0.2, baseFrequency: 200, octaves: 3 },
        } as never),
        db,
      ),
  },
  drum: {
    number: 12,
    name: "drum",
    gainDb: -6.8,
    releaseSec: 0.3,
    create: (Tone, db) =>
      poly(
        new Tone.PolySynth(Tone.MembraneSynth, {
          envelope: { attack: 0.001, decay: 0.3, sustain: 0, release: 0.3 },
        } as never),
        db,
      ),
  },
  noise: {
    number: 13,
    name: "noise",
    gainDb: -4.5,
    releaseSec: 0.05,
    perNote: true,
    create: (Tone, db) => {
      // NoiseSynth は単音用で PolySynth に包めない。音程は無視する。
      const synth = new Tone.NoiseSynth({
        noise: { type: "white" },
        envelope: { attack: 0.001, decay: 0.15, sustain: 0, release: 0.05 },
      }).toDestination();
      synth.volume.value = db;
      return { trigger: (_hz, d, t, v) => void synth.triggerAttackRelease(d, t, v) };
    },
  },
};

/** 番号順の一覧(ヘルプ表示用)。 */
export const TIMBRE_LIST: TimbrePreset[] = Object.values(TIMBRES).sort((a, b) => a.number - b.number);
```

- [ ] **Step 4: テストと型検査を実行する**

Run: `cd frontend && npx vitest run src/lib/mmlTimbres.test.ts`
Expected: 全て PASS。

Run: `cd frontend && pnpm check 2>&1 | tail -8`
Expected: 0 errors。`as never` の箇所は最小限にとどめ、直せるものは型を合わせる(`as never` が残る場合は、どのオプションが型に合わないかをコメントに一言書く)。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/mmlTimbres.ts frontend/src/lib/mmlTimbres.test.ts frontend/src/lib/mmlRender.ts
git commit -m "feat: MMLの音色プリセット表を追加(Issue #494)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: フロント — `renderScore` を音色ごとの音源に切り替える

**Files:**
- Modify: `frontend/src/lib/mmlRender.ts`
- Create: `frontend/src/lib/mmlRender.test.ts`

**Interfaces:**
- Consumes: Task 2 の `TIMBRES` / `Voice`、`NoteEvent.timbre`(Task 1)。
- Produces: `renderScore(score: MmlScore): Promise<string>`(シグネチャは変えない)。`TAIL_SEC` は export 済み。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlRender.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MmlScore, NoteEvent, Timbre } from "../bindings/tauri.gen";

type Call = { hz: number; d: number; t: number; v: number };
const created: { timbre: string; volumeDb: number; calls: Call[] }[] = [];

vi.mock("./mmlTimbres", () => {
  const mk = (name: string, gainDb: number, perNote = false) => ({
    number: 0,
    name,
    gainDb,
    releaseSec: 0.1,
    perNote,
    create: (_Tone: unknown, volumeDb: number) => {
      const rec = { timbre: name, volumeDb, calls: [] as Call[] };
      created.push(rec);
      return { trigger: (hz: number, d: number, t: number, v: number) => rec.calls.push({ hz, d, t, v }) };
    },
  });
  return { TIMBRES: { fm: mk("fm", 0), square: mk("square", -10), noise: mk("noise", -4.5, true) } };
});

const offline = vi.fn(async (cb: () => void, _dur: number, _ch: number, sr: number) => {
  cb();
  return { get: () => ({ sampleRate: sr, getChannelData: () => new Float32Array([0, 0.5, -0.5]) }) };
});
vi.mock("tone", () => ({
  Offline: offline,
  Frequency: (midi: number) => ({ toFrequency: () => midi * 10 }),
  gainToDb: (g: number) => 20 * Math.log10(g),
}));

const { renderScore, TAIL_SEC } = await import("./mmlRender");

const note = (track: number, timbre: Timbre, startSec = 0, midi = 60): NoteEvent => ({
  track,
  startSec,
  durationSec: 0.25,
  midi,
  velocity: 0.5,
  timbre,
});
const score = (notes: NoteEvent[], trackCount = 1, totalSec = 1): MmlScore => ({ notes, totalSec, trackCount });

beforeEach(() => {
  created.length = 0;
  offline.mockClear();
});

describe("renderScore", () => {
  it("WAV の data: URL を返し、長さは totalSec + TAIL_SEC(モノラル・22.05kHz)", async () => {
    const url = await renderScore(score([note(0, "fm")], 1, 2));
    expect(url.startsWith("data:audio/wav;base64,")).toBe(true);
    expect(offline).toHaveBeenCalledWith(expect.any(Function), 2 + TAIL_SEC, 1, 22050);
  });

  it("同じ (トラック, 音色) の音は 1 つの音源で鳴らす", async () => {
    await renderScore(score([note(0, "fm", 0), note(0, "fm", 0.5), note(0, "fm", 1)]));
    expect(created).toHaveLength(1);
    expect(created[0].calls.map((c) => c.t)).toEqual([0, 0.5, 1]);
    expect(created[0].calls[0]).toEqual({ hz: 600, d: 0.25, t: 0, v: 0.5 });
  });

  it("別の音色・別のトラックは別の音源になる", async () => {
    await renderScore(score([note(0, "fm"), note(0, "square", 0.5), note(1, "fm"), note(1, "fm", 0.5)], 2));
    expect(created.map((c) => c.timbre).sort()).toEqual(["fm", "fm", "square"]);
  });

  it("perNote の音色は 1 音ごとに音源を作る", async () => {
    await renderScore(score([note(0, "noise", 0), note(0, "noise", 0.5), note(0, "noise", 1)]));
    expect(created).toHaveLength(3);
    expect(created.every((c) => c.calls.length === 1)).toBe(true);
  });

  it("音量は gainDb に、複数トラックのときだけ 1/トラック数 の減衰を足す", async () => {
    await renderScore(score([note(0, "square")], 1));
    expect(created[0].volumeDb).toBe(-10);

    created.length = 0;
    await renderScore(score([note(0, "square"), note(1, "fm")], 2));
    const byName = Object.fromEntries(created.map((c) => [c.timbre, c.volumeDb]));
    expect(byName.square).toBeCloseTo(-10 + 20 * Math.log10(1 / 2), 6);
    expect(byName.fm).toBeCloseTo(20 * Math.log10(1 / 2), 6);
  });

  it("start/duration/velocity が null(非有限値)なら 0 として鳴らす", async () => {
    const n = { ...note(0, "fm"), startSec: null, durationSec: null, velocity: null } as unknown as NoteEvent;
    await renderScore(score([n]));
    expect(created[0].calls[0]).toMatchObject({ d: 0, t: 0, v: 0 });
  });

  it("レンダリング結果が空なら例外にする", async () => {
    offline.mockResolvedValueOnce({ get: () => undefined } as never);
    await expect(renderScore(score([note(0, "fm")]))).rejects.toThrow("空");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/mmlRender.test.ts`
Expected: FAIL(今の `renderScore` は `./mmlTimbres` を使わないため、音源の記録が空)。

- [ ] **Step 3: `renderScore` を実装する**

`frontend/src/lib/mmlRender.ts` 全体を次にする。

```ts
// MmlScore を Tone.js の Tone.Offline で WAV にする(Issue #75、音色は Issue #494)。
// ここでは音を出さない。出力は data: URL で、再生は Rust(play_notify_sound)が行う。
// Tone.js は重いので遅延 import し、通常の起動コストに乗せない。
import type { MmlScore, Timbre } from "../bindings/tauri.gen";
import { TIMBRES, type Voice } from "./mmlTimbres";
import { encodeWavMono16, wavToDataUrl } from "./wav";

const SAMPLE_RATE = 22050;
/** 音色の余韻(リリース)が切れないように足す秒数。全プリセットの releaseSec はこれ以下にする */
export const TAIL_SEC = 0.6;

export async function renderScore(score: MmlScore): Promise<string> {
  const Tone = await import("tone");
  const buffer = await Tone.Offline(
    () => {
      // 複数トラックを足すと 1.0 を超えて WAV 化で頭打ちになるので、トラック数で割って揃える(単一トラックは補正なし)。
      const trackDb = score.trackCount > 1 ? Tone.gainToDb(1 / score.trackCount) : 0;
      // 音源は (トラック, 音色) ごとに最初に使うときに作る。
      const voices = new Map<string, Voice>();
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
      for (const n of score.notes) {
        const hz = Tone.Frequency(n.midi, "midi").toFrequency();
        voiceFor(n.track, n.timbre).trigger(hz, n.durationSec ?? 0, n.startSec ?? 0, n.velocity ?? 0);
      }
    },
    (score.totalSec ?? 0) + TAIL_SEC,
    1,
    SAMPLE_RATE,
  );
  const audio = buffer.get();
  if (!audio) throw new Error("レンダリング結果が空でした");
  return wavToDataUrl(encodeWavMono16([audio.getChannelData(0)], audio.sampleRate));
}
```

- [ ] **Step 4: テストと型検査を実行する**

Run: `cd frontend && npx vitest run src/lib/mmlRender.test.ts src/lib/mmlTimbres.test.ts src/lib/soundChoice.test.ts`
Expected: 全て PASS。

Run: `cd frontend && pnpm check 2>&1 | tail -5` → 0 errors。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/mmlRender.ts frontend/src/lib/mmlRender.test.ts
git commit -m "feat: MMLレンダラーを音色ごとの音源に切り替える(Issue #494)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 実描画での校正と確認(WebKitGTK / Xvfb)

実際の `renderScore` と `mmlTimbres.ts` を WebKitGTK で動かし、14 種の描画の成否・音量・ピークを測って `gainDb` を確定する。ハーネスはスクラッチパッド(`$SCRATCH`)に置き、リポジトリには入れない(コミットするのは `mmlTimbres.ts` の `gainDb` の更新とスペックの校正結果だけ)。

**Files:**
- Create(コミットしない): `$SCRATCH/calib/entry.ts`、`vite.config.ts`、`calib.html`、`run.py`、`run-calib.sh`
- Modify: `frontend/src/lib/mmlTimbres.ts`(`gainDb` の値のみ)
- Modify: `docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md`(校正結果の節を足す)

**Interfaces:** なし(測定のみ)。`$SCRATCH` は、セッションのスクラッチパッドか、`mktemp -d` で作る作業ディレクトリ。

**合格基準**(全プリセット):
- 描画が例外にならず、サンプルに NaN がない(`bad == 0`)。
- C4(midi 60、長さ 0.3 秒、velocity 10/15)の単音で、先頭 0.5 秒の RMS が **`fm` の RMS ± 1.0 dB 以内**(`fm` は同じ実行で測った値。0 dB 補正のまま)。
- C4 の単音のピークが **0.36 以下**。この 2 つが両立しないプリセット(`noise` など、RMS に対してピークが高いもの)は、ピーク上限を優先して `gainDb` を下げ、RMS の差を結果に記録する(スペックの決め方どおり。不合格にはしない)。
- midi 36, 48, 60, 72, 84, 96 の単音のピークが全て 0.001 以上(無音でない)。`bass` の高音域は小さくてよいが 0.001 以上であること。
- 音色が違う 4 トラックを同時に鳴らした WAV(C4 を `lead` `square` `saw` `noise` で同時刻)のピークが 0.99 未満。
- 200 音(4 トラック × 50 音、0.2 秒間隔)の描画時間を `noise` と `lead` について記録する(基準: 10 秒未満)。

- [ ] **Step 1: ハーネスを作る**

`$SCRATCH/calib/entry.ts`(`<REPO>` はリポジトリの絶対パス):

```ts
import { renderScore } from "<REPO>/frontend/src/lib/mmlRender";
import { TIMBRE_LIST } from "<REPO>/frontend/src/lib/mmlTimbres";
import type { MmlScore, NoteEvent, Timbre } from "<REPO>/frontend/src/bindings/tauri.gen";

const SR = 22050;

function decodeWav(dataUrl: string): Float32Array {
  const bin = atob(dataUrl.slice(dataUrl.indexOf(",") + 1));
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const pcm = new Int16Array(bytes.buffer, 44, (bytes.length - 44) >> 1);
  return Float32Array.from(pcm, (x) => x / 0x8000);
}
function stats(d: Float32Array, from = 0, to = d.length) {
  let peak = 0, sq = 0, bad = 0;
  for (let i = from; i < Math.min(to, d.length); i++) {
    const x = d[i];
    if (!Number.isFinite(x)) { bad++; continue; }
    peak = Math.max(peak, Math.abs(x)); sq += x * x;
  }
  const n = Math.max(1, Math.min(to, d.length) - from);
  return { peak: +peak.toFixed(4), rmsDb: +(20 * Math.log10(Math.sqrt(sq / n) + 1e-12)).toFixed(1), bad };
}
const note = (timbre: Timbre, midi: number, startSec = 0, track = 0, durationSec = 0.3): NoteEvent => ({
  track, startSec, durationSec, midi, velocity: 10 / 15, timbre,
});
const score = (notes: NoteEvent[], trackCount = 1, totalSec = 0.3): MmlScore => ({ notes, totalSec, trackCount });
const render = async (s: MmlScore) => decodeWav(await renderScore(s));

(window as unknown as Record<string, unknown>).calibrate = async () => {
  const out: Record<string, unknown> = {};
  const c4: Record<string, ReturnType<typeof stats>> = {};
  for (const p of TIMBRE_LIST) {
    const r: Record<string, unknown> = {};
    try {
      c4[p.name] = stats(await render(score([note(p.name, 60)])), 0, Math.floor(0.5 * SR));
      r.c4 = c4[p.name];
      r.pitchPeaks = {};
      for (const midi of [36, 48, 60, 72, 84, 96]) {
        (r.pitchPeaks as Record<number, number>)[midi] = stats(await render(score([note(p.name, midi)]))).peak;
      }
    } catch (e) { r.error = String((e as Error)?.stack ?? e); }
    out[p.name] = r;
  }
  const fm = c4.fm;
  for (const p of TIMBRE_LIST) {
    const r = out[p.name] as Record<string, unknown>;
    const s = c4[p.name];
    if (s && fm) {
      r.deltaRmsDb = +(s.rmsDb - fm.rmsDb).toFixed(1);
      r.suggestedGainDb = +(p.gainDb + (fm.rmsDb - s.rmsDb)).toFixed(1); // 次の gainDb の目安(ピーク上限は別に見る)
    }
  }
  // 4 トラック同時(音色違い)
  try {
    const names: Timbre[] = ["lead", "square", "saw", "noise"];
    out.mix4 = stats(await render(score(names.map((n, i) => note(n, 60, 0, i)), 4)));
  } catch (e) { out.mix4 = { error: String((e as Error)?.stack ?? e) }; }
  // 200 音の描画時間
  const stress: Record<string, number> = {};
  for (const name of ["noise", "lead", "fm"] as Timbre[]) {
    const notes: NoteEvent[] = [];
    for (let tr = 0; tr < 4; tr++) for (let i = 0; i < 50; i++) notes.push(note(name, 57 + (i % 12), i * 0.2, tr, 0.15));
    const t0 = performance.now();
    try { await render(score(notes, 4, 10)); stress[name] = Math.round(performance.now() - t0); }
    catch (e) { stress[name] = -1; }
  }
  out.stressMs = stress;
  return out;
};
```

`$SCRATCH/calib/vite.config.ts`:

```ts
import { defineConfig } from "vite";
export default defineConfig({
  root: "<REPO>/frontend",
  logLevel: "warn",
  publicDir: false,
  build: {
    outDir: "<SCRATCH>/calib/dist",
    emptyOutDir: true,
    minify: false,
    lib: { entry: "<SCRATCH>/calib/entry.ts", formats: ["iife"], name: "Calib", fileName: () => "calib.js" },
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
  resolve: { alias: { tone: "<REPO>/frontend/node_modules/tone" } },
});
```

`$SCRATCH/calib/calib.html`:

```html
<!doctype html><meta charset="utf-8"><title>calib</title>
<script src="calib.js"></script>
<script>
  window.addEventListener("load", async () => {
    let res;
    try { res = await window.calibrate(); } catch (e) { res = { error: String(e && e.stack || e) }; }
    window.webkit.messageHandlers.out.postMessage(JSON.stringify(res));
  });
</script>
```

`$SCRATCH/calib/run.py`(WebKitGTK でページを開き、結果の JSON を標準出力に出す):

```python
import json
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

win = Gtk.Window()
view = WebKit2.WebView()
win.add(view)
win.set_default_size(400, 300)
win.show_all()
manager = view.get_user_content_manager()
manager.register_script_message_handler("out")


def on_msg(_m, result):
    print(json.dumps(json.loads(result.get_js_value().to_string()), ensure_ascii=False, indent=2))
    Gtk.main_quit()


manager.connect("script-message-received::out", on_msg)
GLib.timeout_add_seconds(int(sys.argv[1]), lambda: (print("TIMEOUT: no result from page"), Gtk.main_quit()))
view.load_uri("http://127.0.0.1:8765/calib.html")
Gtk.main()
```

`$SCRATCH/calib/run-calib.sh`(ビルド → http 配信 → Xvfb で実行 → PID を指定して後始末。ページは `http://127.0.0.1` 起点で読むこと。`file://` 起点だと AudioWorklet 系が CORS で拒否される):

```bash
#!/usr/bin/env bash
set -u
S="$(cd "$(dirname "$0")" && pwd)"
REPO=<REPO>
(cd "$REPO/frontend" && pnpm exec vite build --config "$S/vite.config.ts") || exit 1
cp "$S/calib.html" "$S/dist/"
(cd "$S/dist" && python3 -m http.server 8765 --bind 127.0.0.1 >/dev/null 2>&1 & echo $! > "$S/http.pid")
(Xvfb :97 -screen 0 800x600x24 >/dev/null 2>&1 & echo $! > "$S/xvfb.pid")
sleep 1
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=:97 timeout 400 python3 "$S/run.py" 360 2>/dev/null
kill "$(cat "$S/xvfb.pid")" "$(cat "$S/http.pid")"
```

(`<REPO>` / `<SCRATCH>` は実際の絶対パスに置き換える。`pkill` / `killall` は使わない。)

- [ ] **Step 2: 初回の測定**

Run: `bash $SCRATCH/calib/run-calib.sh > $SCRATCH/calib/result1.json`
Expected: JSON が出る。TIMEOUT や `error` が出たら、そのプリセットの `create` を直す(Tone.js のオプション名の誤りなど)。直せない音源は、スペックと合意した範囲を超えるので、止めてユーザーに報告する。

- [ ] **Step 3: `gainDb` を調整して合格基準を満たすまで繰り返す**

各プリセットの `deltaRmsDb` を 0 に近づけるため、`mmlTimbres.ts` の `gainDb` を `suggestedGainDb` に更新する。ただし C4 のピークが 0.36 を超えるプリセットは、ピークが 0.36 以下になるまで `gainDb` を下げる(RMS が −1 dB より下がる場合は、その旨を記録し、`fm` との差が最小になる値を選ぶ)。`fm` は 0 dB のまま変えない。更新後に `run-calib.sh` を再実行する(最大 3 周)。ピーク上限を優先した場合を除いて合格基準を満たせないプリセットがあれば、止めてユーザーに報告する。

- [ ] **Step 4: 校正結果をスペックに記録する**

`docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md` の「spike の結果(2026-10-16)」の前に、次の節を足す。

```markdown
## 校正結果(<実施日>)

実際の `renderScore` を WebKitGTK 2.54.1(Xvfb、`http://127.0.0.1` 起点)で実行して測った。`gainDb` の初期値(上の表)を、この結果で次のように確定した。

| 音色 | gainDb | C4 の RMS(fm との差 dB) | C4 のピーク |
|---|---|---|---|
| …14 行 … | | | |

- 4 トラック同時(lead / square / saw / noise)のピーク: <値>
- 200 音の描画時間: noise <値> ms、lead <値> ms、fm <値> ms
- <基準を満たせなかった項目があれば、その内容と判断>
```

(表は測定値で埋める。値の欠けた行や「…」を残さない。)

- [ ] **Step 5: 全体のテストを実行する**

Run: `cd frontend && npx vitest run src/lib/mmlTimbres.test.ts src/lib/mmlRender.test.ts` と `pnpm check`
Expected: PASS(`gainDb` の範囲 −30〜6 に収まっていること)。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/mmlTimbres.ts docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md
git commit -m "feat: MMLの音色の音量補正を実描画で校正する(Issue #494)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: UI のヘルプとユーザーガイド

**Files:**
- Modify: `frontend/src/ui/settings/SoundChoicePicker.svelte`(「記法」ヘルプ)
- Modify: `frontend/src/ui/settings/SoundChoicePicker.test.ts`
- Modify: `docs/guide/user-guide.md`(「MML で通知音を作る」)
- Modify: `docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md`(実コードに合わせた補足 2 点)

**Interfaces:**
- Consumes: `TIMBRE_LIST`(Task 2)。

- [ ] **Step 1: 失敗するテストを書く**

`SoundChoicePicker.test.ts` の末尾の `describe` ブロックの中に追加する(`import` はファイル先頭の既存の並びに `TIMBRE_LIST` を足す: `const { TIMBRE_LIST } = await import("../../lib/mmlTimbres");` を `SoundChoicePicker` の import の下に置く)。

```ts
  it("記法ヘルプに音色の一覧(番号と名前)が出る", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-timbres").textContent ?? "";
    expect(TIMBRE_LIST).toHaveLength(14);
    for (const p of TIMBRE_LIST) expect(text).toContain(`@${p.number} ${p.name}`);
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/settings/SoundChoicePicker.test.ts`
Expected: `mml-timbres` が見つからず FAIL。

- [ ] **Step 3: ヘルプを実装する**

`SoundChoicePicker.svelte` の `<script>` に `import { TIMBRE_LIST } from "../../lib/mmlTimbres";` を足す(既存の import の並びに合わせる)。「記法」の `<pre>` を次のように直す(トラックの行を変え、音色の行を足す)。

```svelte
    <pre class="m-0 mt-1 whitespace-pre-wrap font-mono">音符 c d e f g a b（# や + で半音上げ、- で半音下げ）
長さ 音符の後ろに 1 2 4 8 16 32 64、付点は .（例: c8. ）
休符 r4  タイ c4&c8
o4 オクターブ指定  &gt; で上げる  &lt; で下げる
l8 既定の長さ  t120 テンポ  v10 音量(0〜15)
@3 または @bell 音色（下の一覧。何も書かなければ @0 fm）
[cde]3 繰り返し(2〜99回、入れ子は不可)
; でトラックを分ける(最大4つ。テンポ・音色はトラックごとに書く)
全体で 1000 文字・10 秒まで</pre>
    <p class="m-0 mt-1 font-mono" data-testid="mml-timbres">音色: {#each TIMBRE_LIST as p, i (p.number)}{i > 0 ? " / " : ""}@{p.number} {p.name}{/each}</p>
```

- [ ] **Step 4: ユーザーガイドを直す**

`docs/guide/user-guide.md` の「MML で通知音を作る」の表に、`v10` の行の後へ次の行を足す。

```markdown
| `@3`、`@bell` | 音色の指定（番号か名前。下の表を参照）。何も書かなければ `@0`（`fm`）。以降の音がその音色になる |
```

表の直後の段落の「音色は全トラックで共通です。」を削り、「トラックごとにテンポ・オクターブ・長さ・音量は独立していて」を「トラックごとにテンポ・オクターブ・長さ・音量・音色は独立していて」に直す。段落の後(「保存した MML は、」の前)に次を足す。

```markdown
音色は次の 14 種類です。`@3c` のように番号の直後に音符を続けても、`@bell c` のように名前で書いても使えます。トラックの途中で何度でも切り替えられます。

| 番号 | 名前 | 音の作り方 |
|---|---|---|
| 0 | `fm` | FM 音源（既定。音色指定がない MML はこの音で鳴ります） |
| 1〜5 | `square` `pulse` `triangle` `saw` `sine` | 矩形波・パルス波(幅 25%)・三角波・ノコギリ波・サイン波 |
| 6 | `organ` | 倍音を重ねたオルガン風 |
| 7 | `epiano` | FM 音源の電子ピアノ風 |
| 8 | `bell` | FM 音源の鐘風 |
| 9 | `am` | AM 音源 |
| 10 | `lead` | ビブラートつきのリード |
| 11 | `bass` | ローパスをかけたベース。低い音向きで、C6 より高い音は小さくなります |
| 12 | `drum` | ドラム風（音程は下がっていきます） |
| 13 | `noise` | ノイズ（音程はなく、ハイハットやスネアのような音） |
```

- [ ] **Step 5: スペックの補足を直す**

`docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md`:
- 「3. Rust」の互換の項の「既存のテストは、期待値に `timbre: Timbre::Fm` を足す。」を、「既存のテストは `NoteEvent` を直接組み立てていないので、期待値の変更は要らない。」に直す。
- 「3. Rust」の `match_name` / 「名前の一覧を持たせる」を、実装に合わせて直す(`domain/mml.rs` に `Timbre::ALL` / `from_number` / `name`、`mml.rs` の `Cursor::timbre_name` が表の名前に一致して読み進める)。
- 「2. MML の文法」の状態の項に、「タイ(`&`)でつないだ音は、先頭の音の音色のまま(間に `@` があっても連結でき、先頭の音のベロシティを保つ既存の扱いと同じ)。」を足す。

- [ ] **Step 6: テストと型検査を実行する**

Run: `cd frontend && npx vitest run src/ui/settings/SoundChoicePicker.test.ts && pnpm check 2>&1 | tail -5`
Expected: PASS、0 errors。`rounded-[…]` などの即値を作っていないこと。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md
git commit -m "docs: MMLの音色指定のヘルプとユーザーガイドを追加(Issue #494)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 統合確認(実アプリ・Android・全テスト)

コードの変更はしない(不具合が見つかった場合だけ、その修正をコミットする)。確認結果をスペックの「実装後の確認」に追記する。

**Files:**
- Modify: `docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md`(「実装後の確認」を足す)

- [ ] **Step 1: 全体のテスト**

Run: `cd src-tauri && cargo test 2>&1 | grep "test result"`、`cd frontend && pnpm test 2>&1 | tail -6`、`pnpm check 2>&1 | tail -5`
Expected: `cargo test` は failed 0、`pnpm test` は全 PASS、`pnpm check` は 0 errors(既存の警告 1 件のみ)。数字をスペックに書く。

- [ ] **Step 2: 実アプリ(Xvfb・隔離)での確認**

#75 の確認と同じ手順(`docs/superpowers/specs/2026-10-15-issue-75-mml-ringtone-design.md` の「実装後の確認」の「気づいたこと」を参照: 隔離起動のデバッグビルドは `frontend/dist` を埋め込むので、`pnpm build` で dist を作り直してから `cd src-tauri && cargo build` する)。`WAYLAND_DISPLAY` を unset、`GDK_BACKEND=x11`、`dbus-run-session`、短い `/tmp` 直下の `XDG_*` で、実画面に出ないようにして起動する。確認すること:

1. 設定→通知で、MML に `@bell o5 l8 cdefgab>c` を入力 → 「再生時間 …秒」が出る。`@14 c` → 「2 文字目: 音色は 0〜13 です」、保存と試聴が無効。`@xyz c` → 「2 文字目: 不明な音色です」。
2. 「記法」ヘルプに音色の一覧(`@0 fm / @1 square / … / @13 noise`)が出る。
3. 試聴: `window.fetch` の `ipc://` を包んで `play_notify_sound` の引数を記録し、`data:audio/wav;base64,…` が渡る(`beep` に落ちない)ことを、`@` つきの MML 3 種類(`@lead …`、`@drum …`、`@noise …`)で確認する。stderr とログに WARN / ERROR がない。
4. 保存と復元: `@3 t120 o5 c d e`(など)を保存 → `settings.json` の `notify.soundChoice` が `mml:…` で入り、アプリを再起動して設定→通知を開くと入力欄に復元される。
5. 互換: `@` を含まない `t120 o5 l8 cdefgab>c` を試聴 → `data:` URL の長さが #75 の確認時(152,975 文字)と同じ。

起動した Xvfb・アプリ・http サーバーなどは、正確な PID だけを `kill` して後始末する。

- [ ] **Step 3: Android エミュレーターでの確認**

#75 の検証ゲートと同じ手順(AVD `tsumugi-verify`、ヘッドレス起動。`$SCRATCH` に同じ形の結果受け口のサーバーを置く)。Task 4 のハーネス(`calib.js` と `calib.html`)を、エミュレーターの Chrome から `http://10.0.2.2:<port>/calib.html` で開く(結果を POST で受け取れるよう、`calib.html` の `postMessage` 行を `fetch("/result", { method: "POST", body: JSON.stringify(res) })` に置き換えた版を使う)。確認すること: 14 種全てで `error` がなく `bad == 0`、C4 のピークが 0 でない。特に、現行の FM 音源が使っていない `lead`(`DuoSynth`)、`bass`(`MonoSynth`)、`drum`(`MembraneSynth`)、`noise`(`NoiseSynth`)。エミュレーターの停止と後始末は、起動した PID だけを対象にする。

- [ ] **Step 4: 結果をスペックに記録する**

スペックの末尾(「スコープ外」の後)に「実装後の確認」を足す。確認できたこと(日付、HEAD、Task 6 の各結果の数字)と、確認できなかったことを正直に書く。確認できなかったこととして最低限、次を書く(実行して確認できた項目は外す): 聞こえ方(作業者は音を聞けない)、複数トラックで音色が違うときの音量バランスの聞き比べ、実際の Android WebView、macOS / Windows。

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-10-16-issue-494-mml-timbre-design.md
git commit -m "docs: MML音色の実装後の確認結果を追記(Issue #494)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
