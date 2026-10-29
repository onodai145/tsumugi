# MML ドラムの割り当て `@drumkit` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MML に新しい音色 `@14 drumkit` を足し、GM(General MIDI)のドラムと同じ音の高さで、キック・スネア・ハイハットなどを打ち分けて鳴らせるようにする。

**Architecture:** Rust は `Timbre::Drumkit` と音色名の最長一致だけを足す(`NoteEvent` の形は変えない)。フロントは、GM の番号 → 楽器の表と楽器ごとの音の作り方を、Tone.js に触れない定数ファイル `mmlDrumkit.ts` に置き、`mmlTimbres.ts` の `drumkit` 音源がそれを読んで、楽器ごとに `Tone.js` の音源を作る。音源の `trigger` に、ノート番号 `midi` を 5 つ目の引数として足す。

**Tech Stack:** Rust(`mml.rs`、`domain/mml.rs`)、Svelte 5 + TypeScript + Vitest、Tone.js 15.1.22(`MembraneSynth` / `NoiseSynth` / `MetalSynth` / `Filter` / `Volume`)。

**Spec:** `docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md`(承認済み。矛盾したらこちらが優先)

## Global Constraints

- 全ユーザー向けの文言(エラー、ヘルプ、ガイド、コミット以外の説明)は日本語。
- 音色の番号と名前: `@14` / `drumkit`。Rust の `Timbre::ALL` とフロントの `TIMBRES` / `EXPECTED` は同じ並び。
- `NoteEvent` / `MmlScore` / `MmlError` の形は変えない。`frontend/src/bindings/tauri.gen.ts` は手で編集せず、`cargo test` の `generates_frontend_bindings` で再生成する(`Timbre` の和に `"drumkit"` が増える)。
- `@12 drum` と `@13 noise` の音と、`@` を含まない MML の結果は変えない。
- 割り当てのない高さは、鳴らさない(エラーにしない)。割り当ては GM のノート番号(`midi = 12 × (オクターブ + 1) + 半音`、`o2 c` = 36):
  - キック 35, 36 / サイドスティック 37 / スネア 38, 40 / ハンドクラップ 39
  - タム 41(最低)・43・45・47・48・50(最高)
  - クローズドハイハット 42, 44 / オープンハイハット 46
  - クラッシュ 49, 57 / ライド 51, 59 / ライドベル 53 / カウベル 56
- 楽器の固定の長さ(最後の打ち出しの時刻 + 減衰 + リリース)は `TAIL_SEC`(0.6 秒)以下。1 打のピークは 0.36 以下。
- 書いた音の長さ(と `q`)は無視する。音量 `v` は 1 打ごとのベロシティとして効く。
- コミットメッセージは 1 行だけ(本文なし)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別に付ける。`git commit` が失敗・タイムアウトしたら止めて報告し、再試行や `--no-gpg-sign` / `--no-verify` はしない。
- `rustfmt --check --edition 2021 src/mml.rs` の差分の数は 5 のまま(`main` と同じ)。
- 音が実際に鳴ることは確認できない(音声デバイスなし)。そのことを検証の記録に書く。実画面の確認は隔離した Xvfb で行い、起動したプロセスは、環境変数のトークンで特定した PID だけを止める(`pkill` / `killall` は使わない)。

## Review Focus

仕様が黙っているが、使う人が踏みやすい入力と条件。それぞれ、持ち主のタスクにテストを足してある。

1. 音色名の最長一致: `@drum c` は `drum`、`@drumkit c` と `@drumkitc4` は `drumkit`、`@drumk c`(= `@drumkc`)は `k` を指す構文エラー。大文字 `@DrumKit`、`@14`、`@15`(範囲外のエラー文言が `0〜14`)。接頭辞の組が `drum` と `drumkit` だけであること。(Task 1)
2. 割り当ての境界: GM の表にある全番号が楽器に対応し、表にない 0〜127 は `null`(特に 34 以下、52、54、55、58、60〜81、82 以上)。タムの 6 段が互いに別の楽器であること。(Task 2)
3. 割り当てのない高さは、音源を作らず鳴らさない(無音で、エラーにならない)。休符と同じ長さを占めるだけであること。(Task 2)
4. 和音 `'cf+'`(キックとハイハットの同時)と、`drumkit` を複数トラックで使うとき、同じトラックの `drumkit` の音源を使い回すこと(トラックごとに別の音源)。(Task 2)
5. 長さ・`q`・タイ: 書いた長さが違っても、同じ楽器は同じ長さで鳴ること。(Task 2)
6. 固定の長さが `TAIL_SEC` 以下で、最後の音の余韻が切れないこと。1 打のピークが 0.36 以下。(Task 2 の定数テスト、Task 4 の実測)
7. 3000 音すべてを `drumkit` で鳴らしたときの描画時間(1 打ごとに音源を作る楽器がある)。15 種すべてが実際の WebKitGTK で、エラー・NaN なしで描画できること(`MetalSynth` がオフライン描画で動くこと)。(Task 4)

---

### Task 1: Rust — `Timbre::Drumkit` と音色名の最長一致

**Files:**
- Modify: `src-tauri/src/domain/mml.rs`(`Timbre`、`ALL`、`name`)
- Modify: `src-tauri/src/mml.rs`(`timbre_name` と、`mod tests` の表・テスト)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`(`cargo test` が書き換える。手で編集しない)

**Interfaces:**
- Consumes: 既存の `Timbre::ALL` / `from_number` / `name()`、`timbre_name`(`mml.rs:135` 付近)、テストの `timbres(text)`、`must_parse`、`must_fail`、`TIMBRE_TABLE`。
- Produces: `Timbre::Drumkit`(番号 14、名前 `"drumkit"`)。TS の `Timbre` 型に `"drumkit"` が増える(Task 2 が使う)。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/mml.rs` の `mod tests` を 3 か所編集する。

(a) `TIMBRE_TABLE` の型の長さと最後の行を置き換える。

置き換える前:
```rust
    const TIMBRE_TABLE: [(u32, &str, Timbre); 14] = [
```
置き換えた後:
```rust
    const TIMBRE_TABLE: [(u32, &str, Timbre); 15] = [
```

置き換える前:
```rust
        (13, "noise", Timbre::Noise),
    ];
```
置き換えた後:
```rust
        (13, "noise", Timbre::Noise),
        (14, "drumkit", Timbre::Drumkit),
    ];
```

(b) テスト `timbre_names_are_not_prefixes_of_each_other` を丸ごと置き換える。

置き換える前:
```rust
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
```
置き換えた後:
```rust
    #[test]
    fn only_drum_is_a_prefix_of_another_timbre_name() {
        // 音色名の読み取りは最長一致。接頭辞の組は drum と drumkit だけで、残りの `kit` は命令の先頭にならない
        let mut pairs = Vec::new();
        for a in Timbre::ALL {
            for b in Timbre::ALL {
                if a != b && b.name().starts_with(a.name()) {
                    pairs.push((a.name(), b.name()));
                }
            }
        }
        assert_eq!(pairs, vec![("drum", "drumkit")]);
    }

    #[test]
    fn timbre_name_uses_the_longest_match() {
        assert_eq!(timbres("@drum c"), vec![Timbre::Drum]);
        assert_eq!(timbres("@drumkit c"), vec![Timbre::Drumkit]);
        assert_eq!(timbres("@DrumKit C"), vec![Timbre::Drumkit]);
        assert_eq!(timbres("@drumkitc4"), vec![Timbre::Drumkit]);
        assert_eq!(timbres("@drum t60 c"), vec![Timbre::Drum]);
        assert_eq!(timbres("@14 c"), vec![Timbre::Drumkit]);
        // `@drumk` は drumkit にならず、drum の後ろの `k`(不明な文字)を指す
        assert_eq!(must_fail("@drumk c").position, 5);
    }
```

(c) `timbre_errors_report_where_to_look` の `@14` を `@15` にする。

置き換える前:
```rust
        assert_eq!(must_fail("@14 c").position, 1); // 範囲外は数字の先頭
        assert!(must_fail("@14 c").message.contains("0〜13"));
```
置き換えた後:
```rust
        assert_eq!(must_fail("@15 c").position, 1); // 範囲外は数字の先頭
        assert!(must_fail("@15 c").message.contains("0〜14"));
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml:: 2>&1 | grep -E "^error|cannot find|no variant|test result" | head`
Expected: `no variant or associated item named 'Drumkit'` のコンパイルエラー(テストはまだ動かない)。

- [ ] **Step 3: 実装する**

`src-tauri/src/domain/mml.rs` を編集する。

- `enum Timbre` の `Noise,` の次に `Drumkit,` を足す。
- `pub const ALL: [Timbre; 14]` を `[Timbre; 15]` にし、`Timbre::Noise,` の次に `Timbre::Drumkit,` を足す。
- `name()` の `Timbre::Noise => "noise",` の次に `Timbre::Drumkit => "drumkit",` を足す。

`src-tauri/src/mml.rs` の `timbre_name` を置き換える。

置き換える前:
```rust
    /// 先頭が音色名と一致するなら、その名前の分だけ読み進めて返す。
    /// どの名前も他の名前の接頭辞ではない(テストで確認)ので、一致は 1 つに決まる。
    fn timbre_name(&mut self) -> Option<Timbre> {
        let rest = &self.syms[self.i..];
        let t = Timbre::ALL.into_iter().find(|t| {
            let name = t.name();
            rest.len() >= name.len() && name.chars().zip(rest).all(|(a, (b, _))| a == *b)
        })?;
```
置き換えた後:
```rust
    /// 先頭が音色名と一致するなら、その名前の分だけ読み進めて返す。複数の名前が一致するときは、最も長いものを選ぶ
    /// (`drum` は `drumkit` の接頭辞。接頭辞の組は drum と drumkit だけで、テストで確認する)。
    fn timbre_name(&mut self) -> Option<Timbre> {
        let rest = &self.syms[self.i..];
        let t = Timbre::ALL
            .into_iter()
            .filter(|t| {
                let name = t.name();
                rest.len() >= name.len() && name.chars().zip(rest).all(|(a, (b, _))| a == *b)
            })
            .max_by_key(|t| t.name().len())?;
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "FAILED|panicked|test result: .* [1-9][0-9]* passed"; rustfmt --check --edition 2021 src/mml.rs 2>&1 | grep -c '^Diff in'; cd .. && git status --short`
Expected: すべて PASS(失敗 0)。rustfmt の差分は `5`。`git status` に `src-tauri/src/domain/mml.rs`・`src-tauri/src/mml.rs` と、再生成された `frontend/src/bindings/tauri.gen.ts`(`"drumkit"` が増えただけ)が出る。`tauri.gen.ts` の差分は `git diff frontend/src/bindings/tauri.gen.ts` で、`Timbre` の和に `| "drumkit"` が増えただけであることを確認する。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add src-tauri/src/domain/mml.rs src-tauri/src/mml.rs frontend/src/bindings/tauri.gen.ts && git commit -m "feat: MML に音色 @14 drumkit を追加し、音色名を最長一致で読む(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: フロント — GM の割り当て表、`drumkit` 音源、`midi` の受け渡し

**Files:**
- Create: `frontend/src/lib/mmlDrumkit.ts`、`frontend/src/lib/mmlDrumkit.test.ts`
- Modify: `frontend/src/lib/mmlTimbres.ts`、`frontend/src/lib/mmlTimbres.test.ts`
- Modify: `frontend/src/lib/mmlRender.ts`、`frontend/src/lib/mmlRender.test.ts`
- Modify: `frontend/src/ui/settings/SoundChoicePicker.test.ts`(音色の数 14 → 15)

**Interfaces:**
- Consumes: Task 1 の TS 型 `Timbre`(`"drumkit"` を含む)、既存の `Voice`・`TimbrePreset`・`ToneModule`・`TIMBRES`・`TIMBRE_LIST`・`TAIL_SEC`。
- Produces:
  - `mmlDrumkit.ts`: `type DrumPiece`(16 種)、`DRUM_MAP`、`drumPieceFor(midi: number): DrumPiece | null`、`PIECE_SPECS: Record<DrumPiece, PieceSpec>`、`pieceLengthSec(spec: PieceSpec): number`。
  - `Voice.trigger(hz, durationSec, startSec, velocity, midi?: number)`(5 つ目の引数は、`drumkit` だけが使う)。
  - `TIMBRES.drumkit`(番号 14)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlDrumkit.test.ts` を新規作成する:

```ts
import { describe, expect, it } from "vitest";
import { DRUM_MAP, PIECE_SPECS, drumPieceFor, pieceLengthSec, type DrumPiece } from "./mmlDrumkit";
import { TAIL_SEC } from "./mmlRender";

// GM のパーカッションマップ(docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md の表)
const EXPECTED: [number, DrumPiece][] = [
  [35, "kick"],
  [36, "kick"],
  [37, "sidestick"],
  [38, "snare"],
  [39, "clap"],
  [40, "snare"],
  [41, "tomLowFloor"],
  [42, "hihatClosed"],
  [43, "tomHighFloor"],
  [44, "hihatClosed"],
  [45, "tomLow"],
  [46, "hihatOpen"],
  [47, "tomLowMid"],
  [48, "tomHiMid"],
  [49, "crash"],
  [50, "tomHigh"],
  [51, "ride"],
  [53, "rideBell"],
  [56, "cowbell"],
  [57, "crash"],
  [59, "ride"],
];

describe("DRUM_MAP / drumPieceFor", () => {
  it("GM の番号が、仕様の表の楽器に対応する", () => {
    for (const [midi, piece] of EXPECTED) expect(drumPieceFor(midi), `midi ${midi}`).toBe(piece);
    expect(Object.keys(DRUM_MAP)).toHaveLength(EXPECTED.length);
  });

  it("表にない高さは null(0〜127 のうち、表にある番号以外すべて)", () => {
    const mapped = new Set(EXPECTED.map(([m]) => m));
    for (let midi = 0; midi <= 127; midi++) {
      if (!mapped.has(midi)) expect(drumPieceFor(midi), `midi ${midi}`).toBeNull();
    }
    for (const midi of [34, 52, 54, 55, 58, 60, 81, 82]) expect(drumPieceFor(midi)).toBeNull();
  });

  it("タムの 6 段は、互いに別の楽器で、高さが低い順に並ぶ", () => {
    const toms = [41, 43, 45, 47, 48, 50].map((m) => drumPieceFor(m)!);
    expect(new Set(toms).size).toBe(6);
    const hz = toms.map((p) => {
      const spec = PIECE_SPECS[p];
      if (spec.kind !== "membrane") throw new Error(`${p} は膜の音源ではない`);
      return spec.hz;
    });
    expect([...hz].sort((a, b) => a - b)).toEqual(hz);
  });
});

describe("PIECE_SPECS", () => {
  it("割り当てた全楽器に仕様があり、仕様のある楽器はすべて割り当てられている", () => {
    const used = new Set(Object.values(DRUM_MAP));
    expect(Object.keys(PIECE_SPECS).sort()).toEqual([...used].sort());
  });

  it("固定の長さ(最後の打ち出し + 減衰 + リリース)は TAIL_SEC 以下", () => {
    for (const [piece, spec] of Object.entries(PIECE_SPECS)) {
      expect(pieceLengthSec(spec), piece).toBeGreaterThan(0);
      expect(pieceLengthSec(spec), piece).toBeLessThanOrEqual(TAIL_SEC);
    }
  });

  it("trimDb は有限で、極端でない", () => {
    for (const [piece, spec] of Object.entries(PIECE_SPECS)) {
      expect(Number.isFinite(spec.trimDb), piece).toBe(true);
      expect(Math.abs(spec.trimDb), piece).toBeLessThanOrEqual(24);
    }
  });
});
```

`frontend/src/lib/mmlTimbres.test.ts` を編集する。

(a) `EXPECTED` の最後に `[14, "drumkit"],` を足す。

置き換える前:
```ts
  [13, "noise"],
];
```
置き換えた後:
```ts
  [13, "noise"],
  [14, "drumkit"],
];
```

(b) ファイルの先頭の import に `vi` と型を足し、最後の `describe("TIMBRES", ...)` の**後ろ**に、次を追加する。

先頭の import を置き換える:

置き換える前:
```ts
import { describe, expect, it } from "vitest";
import type { Timbre } from "../bindings/tauri.gen";
import { TAIL_SEC } from "./mmlRender";
import { TIMBRES, TIMBRE_LIST } from "./mmlTimbres";
```
置き換えた後:
```ts
import { describe, expect, it } from "vitest";
import type { Timbre } from "../bindings/tauri.gen";
import { TAIL_SEC } from "./mmlRender";
import { TIMBRES, TIMBRE_LIST, type ToneModule } from "./mmlTimbres";
```

ファイルの最後に追加する:
```ts

/** drumkit の音源の作り方を確かめるための、Tone.js の代役。作ったものと、鳴らした呼び出しを記録する */
function fakeTone() {
  const made: { cls: string; args: unknown[]; node: FakeNode }[] = [];
  const hits: { cls: string; args: unknown[]; node: FakeNode }[] = [];
  class FakeNode {
    volume = { value: 0 };
    connected: unknown[] = [];
    constructor(
      public cls: string,
      public args: unknown[],
    ) {
      made.push({ cls, args, node: this });
    }
    connect(to: unknown) {
      this.connected.push(to);
      return this;
    }
    toDestination() {
      return this;
    }
    triggerAttackRelease(...args: unknown[]) {
      hits.push({ cls: this.cls, args, node: this });
      return this;
    }
  }
  const cls = (name: string) =>
    class extends FakeNode {
      constructor(...args: unknown[]) {
        super(name, args);
      }
    };
  const Tone = {
    Volume: cls("Volume"),
    PolySynth: cls("PolySynth"),
    MembraneSynth: cls("MembraneSynth"),
    NoiseSynth: cls("NoiseSynth"),
    MetalSynth: cls("MetalSynth"),
    Filter: cls("Filter"),
  } as unknown as ToneModule;
  return { Tone, made, hits };
}

describe("drumkit の音源", () => {
  it("drumkit は単音用ではない(1 トラックに 1 つ作って使い回す)", () => {
    expect(TIMBRES.drumkit.perNote).toBeUndefined();
    expect(TIMBRES.drumkit.number).toBe(14);
  });

  it("割り当てのない高さは、音源を作らず、鳴らさない", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    const before = hits.length;
    for (const midi of [0, 34, 52, 60, 81, 127]) voice.trigger(440, 0.5, 0, 0.8, midi);
    voice.trigger(440, 0.5, 0, 0.8); // midi が無いときも鳴らさない
    expect(hits).toHaveLength(before);
  });

  it("GM の番号の楽器で鳴り、書いた長さは無視して、楽器ごとの固定の長さで鳴らす", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    voice.trigger(440, 0.01, 1.0, 0.7, 36); // キック
    voice.trigger(440, 5, 2.0, 0.7, 36);
    expect(hits).toHaveLength(2);
    expect(hits[0].args[1]).toBe(hits[1].args[1]); // 長さは同じ
    expect(hits[0].args[2]).toBe(1.0); // 開始時刻
    expect(hits[1].args[2]).toBe(2.0);
    expect(hits[0].args[3]).toBe(0.7); // ベロシティ
    expect(hits[0].cls).toBe("PolySynth");
  });

  it("同じ楽器の膜の音源は使い回し、別の楽器は別の音源にする", () => {
    const { Tone, made } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    voice.trigger(440, 0.1, 0, 0.8, 36);
    const polys = () => made.filter((m) => m.cls === "PolySynth").length;
    expect(polys()).toBe(1);
    voice.trigger(440, 0.1, 0.5, 0.8, 35); // 同じキック
    expect(polys()).toBe(1);
    voice.trigger(440, 0.1, 1, 0.8, 41); // タム
    expect(polys()).toBe(2);
  });

  it("ノイズと金属の楽器は、1 打ごとに音源を作る", () => {
    const { Tone, made } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    const count = (cls: string) => made.filter((m) => m.cls === cls).length;
    voice.trigger(440, 0.1, 0, 0.8, 42);
    voice.trigger(440, 0.1, 0.1, 0.8, 42);
    expect(count("NoiseSynth")).toBe(2);
    voice.trigger(440, 0.1, 0.2, 0.8, 49);
    voice.trigger(440, 0.1, 0.3, 0.8, 57);
    expect(count("MetalSynth")).toBe(2);
  });

  it("ハンドクラップは短い間隔で 3 回叩く", () => {
    const { Tone, hits } = fakeTone();
    const voice = TIMBRES.drumkit.create(Tone, -3);
    voice.trigger(440, 0.1, 1.0, 0.8, 39);
    const times = hits.map((h) => h.args[1] as number);
    expect(times).toHaveLength(3);
    expect(times[0]).toBe(1.0);
    expect(times[1]).toBeGreaterThan(times[0]);
    expect(times[2]).toBeGreaterThan(times[1]);
  });

  it("出力の音量は、渡した volumeDb になる", () => {
    const { Tone, made } = fakeTone();
    TIMBRES.drumkit.create(Tone, -7.5);
    const volume = made.find((m) => m.cls === "Volume")!;
    expect(volume.args[0]).toBe(-7.5);
  });
});
```

`frontend/src/lib/mmlRender.test.ts` を編集する。

(a) モックの `trigger` が `midi` も記録するようにする。

置き換える前:
```ts
type Call = { hz: number; d: number; t: number; v: number };
```
置き換えた後:
```ts
type Call = { hz: number; d: number; t: number; v: number; midi?: number };
```

置き換える前:
```ts
      return { trigger: (hz: number, d: number, t: number, v: number) => rec.calls.push({ hz, d, t, v }) };
```
置き換えた後:
```ts
      return {
        trigger: (hz: number, d: number, t: number, v: number, midi?: number) => rec.calls.push({ hz, d, t, v, midi }),
      };
```

(b) 既存の期待値に `midi: 60` を足す(`note()` の既定の midi は 60)。

置き換える前:
```ts
    expect(created[0].calls[0]).toEqual({ hz: 600, d: 0.25, t: 0, v: 0.5 });
```
置き換えた後:
```ts
    expect(created[0].calls[0]).toEqual({ hz: 600, d: 0.25, t: 0, v: 0.5, midi: 60 });
```

(c) `describe("renderScore", ...)` の中、`"perNote の音色は 1 音ごとに音源を作る"` の `it` の次に追加する:
```ts

  it("音源の trigger に、ノート番号 midi も渡す", async () => {
    await renderScore(score([note(0, "fm", 0, 36), note(0, "fm", 0.5, 42)]));
    expect(created[0].calls.map((c) => c.midi)).toEqual([36, 42]);
  });
```

`frontend/src/ui/settings/SoundChoicePicker.test.ts` の音色の数を直す。

置き換える前:
```ts
    expect(TIMBRE_LIST).toHaveLength(14);
```
置き換えた後:
```ts
    expect(TIMBRE_LIST).toHaveLength(15);
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlDrumkit.test.ts src/lib/mmlTimbres.test.ts src/lib/mmlRender.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | grep -E "×|FAIL|Test Files|Tests |Error" | head -20`
Expected: `mmlDrumkit.test.ts` は import 失敗(ファイルが無い)。`mmlTimbres.test.ts` の drumkit のテストと `EXPECTED`、`mmlRender.test.ts` の `midi`、`SoundChoicePicker.test.ts` の 15 が FAIL。`pnpm check` は `TIMBRES` に `drumkit` が無いと型エラーにもなる(次のステップで直す)。

- [ ] **Step 3: 実装する**

(a) `frontend/src/lib/mmlDrumkit.ts` を新規作成する:

```ts
// MML のドラム音色 `@drumkit`(Issue #499)の、GM のノート番号 → 楽器の割り当てと、楽器ごとの音の作り方の定数。
// Tone.js には触れない(音源を作るのは mmlTimbres.ts)。割り当ては General MIDI のパーカッションマップ
// (docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md)。`midi = 12 × (オクターブ + 1) + 半音` で、`o2 c` = 36。
// 表にない高さは鳴らさない。音の細かい作り(減衰・フィルタ・周波数)と trimDb は、実機で描画して測って決めた値。

export type DrumPiece =
  | "kick"
  | "sidestick"
  | "snare"
  | "clap"
  | "tomLowFloor"
  | "tomHighFloor"
  | "tomLow"
  | "tomLowMid"
  | "tomHiMid"
  | "tomHigh"
  | "hihatClosed"
  | "hihatOpen"
  | "crash"
  | "ride"
  | "rideBell"
  | "cowbell";

/** GM のノート番号 → 楽器。表にない番号は鳴らさない */
export const DRUM_MAP: Readonly<Record<number, DrumPiece>> = {
  35: "kick",
  36: "kick",
  37: "sidestick",
  38: "snare",
  39: "clap",
  40: "snare",
  41: "tomLowFloor",
  42: "hihatClosed",
  43: "tomHighFloor",
  44: "hihatClosed",
  45: "tomLow",
  46: "hihatOpen",
  47: "tomLowMid",
  48: "tomHiMid",
  49: "crash",
  50: "tomHigh",
  51: "ride",
  53: "rideBell",
  56: "cowbell",
  57: "crash",
  59: "ride",
};

export function drumPieceFor(midi: number): DrumPiece | null {
  return Object.hasOwn(DRUM_MAP, midi) ? DRUM_MAP[midi] : null;
}

/** 膜の音(キック・タム)。`hz` は叩く高さで、`pitchDecay` と `octaves` で下降の幅を決める */
export interface MembraneSpec {
  kind: "membrane";
  hz: number;
  pitchDecay: number;
  octaves: number;
  decay: number;
  release: number;
  trimDb: number;
}

/** ノイズの音(スネア・クラップ・ハイハット・サイドスティック)。`bursts` は、叩く時刻のずれ(秒)。`body` は重ねる膜の音 */
export interface NoiseSpec {
  kind: "noise";
  filter: "highpass" | "bandpass";
  cutoff: number;
  bursts: number[];
  decay: number;
  release: number;
  trimDb: number;
  body?: MembraneSpec;
}

/** 金属の音(シンバル・ベル・カウベル) */
export interface MetalSpec {
  kind: "metal";
  hz: number;
  harmonicity: number;
  modulationIndex: number;
  resonance: number;
  octaves: number;
  decay: number;
  release: number;
  trimDb: number;
}

export type PieceSpec = MembraneSpec | NoiseSpec | MetalSpec;

const tom = (hz: number): MembraneSpec => ({
  kind: "membrane",
  hz,
  pitchDecay: 0.06,
  octaves: 2,
  decay: 0.25,
  release: 0.15,
  trimDb: 0,
});

export const PIECE_SPECS: Record<DrumPiece, PieceSpec> = {
  kick: { kind: "membrane", hz: 55, pitchDecay: 0.05, octaves: 4, decay: 0.3, release: 0.2, trimDb: 0 },
  sidestick: { kind: "noise", filter: "bandpass", cutoff: 2200, bursts: [0], decay: 0.03, release: 0.02, trimDb: 0 },
  snare: {
    kind: "noise",
    filter: "highpass",
    cutoff: 1800,
    bursts: [0],
    decay: 0.16,
    release: 0.05,
    trimDb: 0,
    body: { kind: "membrane", hz: 190, pitchDecay: 0.02, octaves: 2, decay: 0.1, release: 0.05, trimDb: 0 },
  },
  clap: { kind: "noise", filter: "bandpass", cutoff: 1500, bursts: [0, 0.012, 0.024], decay: 0.08, release: 0.05, trimDb: 0 },
  tomLowFloor: tom(70),
  tomHighFloor: tom(85),
  tomLow: tom(105),
  tomLowMid: tom(130),
  tomHiMid: tom(160),
  tomHigh: tom(200),
  hihatClosed: { kind: "noise", filter: "highpass", cutoff: 7000, bursts: [0], decay: 0.04, release: 0.03, trimDb: 0 },
  hihatOpen: { kind: "noise", filter: "highpass", cutoff: 7000, bursts: [0], decay: 0.3, release: 0.2, trimDb: 0 },
  crash: { kind: "metal", hz: 300, harmonicity: 5.1, modulationIndex: 32, resonance: 4000, octaves: 1.5, decay: 0.4, release: 0.15, trimDb: 0 },
  ride: { kind: "metal", hz: 400, harmonicity: 5.1, modulationIndex: 20, resonance: 5000, octaves: 1.5, decay: 0.3, release: 0.15, trimDb: 0 },
  rideBell: { kind: "metal", hz: 520, harmonicity: 3.1, modulationIndex: 12, resonance: 5500, octaves: 1, decay: 0.25, release: 0.1, trimDb: 0 },
  cowbell: { kind: "metal", hz: 560, harmonicity: 2.5, modulationIndex: 8, resonance: 800, octaves: 1, decay: 0.12, release: 0.05, trimDb: 0 },
};

/** 1 打の固定の長さ(最後の打ち出しの時刻 + 減衰 + リリース)。`TAIL_SEC` 以下にする */
export function pieceLengthSec(spec: PieceSpec): number {
  const lastBurst = spec.kind === "noise" ? Math.max(...spec.bursts) : 0;
  return lastBurst + spec.decay + spec.release;
}
```

(b) `frontend/src/lib/mmlTimbres.ts` を編集する。

- `Voice` を置き換える。

置き換える前:
```ts
export interface Voice {
  trigger(hz: number, durationSec: number, startSec: number, velocity: number): void;
}
```
置き換えた後:
```ts
export interface Voice {
  /** `midi` はノート番号。打楽器の `drumkit` だけが使う(音の高さで楽器を選ぶ) */
  trigger(hz: number, durationSec: number, startSec: number, velocity: number, midi?: number): void;
}
```

- import に `import { PIECE_SPECS, drumPieceFor, type DrumPiece, type MembraneSpec } from "./mmlDrumkit";` を足す(`import type { Timbre } ...` の次の行)。

- `const BASIC_ENV = ...` の**前**に、次の関数を足す:

```ts
/** `@drumkit`。音の高さ(midi)から GM の楽器を選んで鳴らす。割り当てのない高さは鳴らさない。書いた長さは無視する */
function drumkit(Tone: ToneModule, volumeDb: number): Voice {
  const out = new Tone.Volume(volumeDb).toDestination();
  // 膜の音源(キック・タム・スネアの胴)は、楽器ごとに 1 つ作って使い回す。ノイズと金属は単音用なので 1 打ごとに作る
  const membranes = new Map<DrumPiece, PolyLike & { connect(to: unknown): unknown }>();
  const membraneFor = (piece: DrumPiece, spec: MembraneSpec) => {
    let synth = membranes.get(piece);
    if (!synth) {
      synth = new Tone.PolySynth(Tone.MembraneSynth, {
        pitchDecay: spec.pitchDecay,
        octaves: spec.octaves,
        envelope: { attack: 0.001, decay: spec.decay, sustain: 0, release: spec.release },
      });
      synth.volume.value = spec.trimDb;
      synth.connect(out);
      membranes.set(piece, synth);
    }
    return synth;
  };
  return {
    trigger(_hz, _d, t, v, midi) {
      const piece = midi === undefined ? null : drumPieceFor(midi);
      if (!piece) return;
      const spec = PIECE_SPECS[piece];
      if (spec.kind === "membrane") {
        membraneFor(piece, spec).triggerAttackRelease(spec.hz, spec.decay, t, v);
      } else if (spec.kind === "noise") {
        for (const offset of spec.bursts) {
          const noise = new Tone.NoiseSynth({
            noise: { type: "white" },
            envelope: { attack: 0.001, decay: spec.decay, sustain: 0, release: spec.release },
          });
          const filter = new Tone.Filter(spec.cutoff, spec.filter);
          noise.volume.value = spec.trimDb;
          noise.connect(filter);
          filter.connect(out);
          noise.triggerAttackRelease(spec.decay, t + offset, v);
        }
        if (spec.body) membraneFor(piece, spec.body).triggerAttackRelease(spec.body.hz, spec.body.decay, t, v);
      } else {
        const metal = new Tone.MetalSynth({
          harmonicity: spec.harmonicity,
          modulationIndex: spec.modulationIndex,
          octaves: spec.octaves,
          resonance: spec.resonance,
          envelope: { attack: 0.001, decay: spec.decay, release: spec.release },
        });
        metal.volume.value = spec.trimDb;
        metal.connect(out);
        metal.triggerAttackRelease(spec.hz, spec.decay, t, v);
      }
    },
  };
}

```

- `TIMBRES` の最後(`noise: { ... },` の次、`};` の前)に追加する:

```ts
  drumkit: {
    number: 14,
    name: "drumkit",
    gainDb: 0,
    releaseSec: 0.2,
    create: (Tone, db) => drumkit(Tone, db),
  },
```

(c) `frontend/src/lib/mmlRender.ts` の `trigger` の呼び出しに `n.midi` を足す。

置き換える前:
```ts
        voiceFor(n.track, n.timbre).trigger(hz, n.durationSec ?? 0, n.startSec ?? 0, n.velocity ?? 0);
```
置き換えた後:
```ts
        voiceFor(n.track, n.timbre).trigger(hz, n.durationSec ?? 0, n.startSec ?? 0, n.velocity ?? 0, n.midi);
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlDrumkit.test.ts src/lib/mmlTimbres.test.ts src/lib/mmlRender.test.ts src/ui/settings/SoundChoicePicker.test.ts 2>&1 | grep -E "×|FAIL|Test Files|Tests |Error"`
Expected: すべて PASS。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -3`
Expected: 0 errors(既存の `UrlPreviewCard.svelte` の警告 1 件だけは出てよい)。型エラーが出たら、`PolyLike` の形(`connect` を持たない)や `Tone.Filter` の第 2 引数の型を、`node_modules/tone/build/esm/` の `.d.ts` に合わせて直す(挙動は変えない)。直した内容は台帳に Ruling として書く。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlDrumkit.ts frontend/src/lib/mmlDrumkit.test.ts frontend/src/lib/mmlTimbres.ts frontend/src/lib/mmlTimbres.test.ts frontend/src/lib/mmlRender.ts frontend/src/lib/mmlRender.test.ts frontend/src/ui/settings/SoundChoicePicker.test.ts && git commit -m "feat: MML の drumkit を、GM の割り当てで楽器ごとに鳴らす(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ヘルプとユーザーガイド

**Files:**
- Modify: `frontend/src/ui/settings/SoundChoicePicker.svelte`(「記法」ヘルプ)
- Modify: `frontend/src/ui/settings/SoundChoicePicker.test.ts`
- Modify: `docs/guide/user-guide.md`

**Interfaces:**
- Consumes: `data-testid="mml-syntax-help"` の `<pre>`、ガイドの音色の表(`| 13 | \`noise\` | … |` の行)。
- Produces: なし(表示のみ)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/settings/SoundChoicePicker.test.ts` の、「記法ヘルプに、マクロ $riff の書き方と、名前の区切りの規則が出る」の `it` の次に追加する:

```ts

  it("記法ヘルプに、@drumkit の割り当て(GM の高さ)と、鳴らない高さの扱いが出る", () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-syntax-help").textContent ?? "";
    expect(text).toContain("@drumkit");
    expect(text).toContain("o2 c キック");
    expect(text).toContain("o2 d スネア");
    expect(text).toContain("割り当てのない高さは鳴らない");
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/SoundChoicePicker.test.ts 2>&1 | grep -E "×|Test Files|Tests "`
Expected: 追加した `it` が FAIL。

- [ ] **Step 3: 実装する**

(a) `frontend/src/ui/settings/SoundChoicePicker.svelte` の「記法」の `<pre>` の、`@3 または @bell 音色…` の行の**次**に、1 行足す:

```
@drumkit ドラム(音の高さで楽器を打ち分ける。GM と同じ高さ。o2 c キック、o2 d スネア、o2 f+ クローズドハイハット、o2 a+ オープンハイハット、o3 c+ クラッシュ、タムは o2 f g a b と o3 c d。書いた長さは無視。割り当てのない高さは鳴らない。表はガイド)
```

(b) `docs/guide/user-guide.md` を編集する(置き換え前の文字列がちょうど 1 回出ることを確かめながら置き換える)。

1. 音色の表の `noise` の行の次に、1 行足す:

```markdown
| 14 | `drumkit` | ドラム(音の高さでキック・スネア・ハイハットなどを打ち分けます。下の表を参照) |
```

2. 音色の表と、その次の段落(`保存した MML は、設定→通知の…`)の間に、次の節を足す:

```markdown

**`@drumkit` の割り当て**: General MIDI(GM)のドラムと同じ高さで鳴ります(`o2 c` が 36 番)。書いた音の長さ(と `q`)は無視して、楽器ごとの決まった長さで鳴ります。音量 `v` は効きます。割り当てのない高さの音は鳴りません(エラーにもなりません)。`'cf+'` のように和音で書くと、キックとハイハットが同時に鳴ります。

| 楽器 | 書き方 |
|---|---|
| キック | `o2 c`(36)、`o1 b`(35) |
| サイドスティック | `o2 c+`(37) |
| スネア | `o2 d`(38)、`o2 e`(40) |
| ハンドクラップ | `o2 d+`(39) |
| タム(低い順) | `o2 f`(41)、`o2 g`(43)、`o2 a`(45)、`o2 b`(47)、`o3 c`(48)、`o3 d`(50) |
| クローズドハイハット | `o2 f+`(42)、`o2 g+`(44) |
| オープンハイハット | `o2 a+`(46) |
| クラッシュシンバル | `o3 c+`(49)、`o3 a`(57) |
| ライドシンバル | `o3 d+`(51)、`o3 b`(59) |
| ライドベル | `o3 f`(53) |
| カウベル | `o3 g+`(56) |

例: `@drumkit t120 o2 [c8 f+8 d8 f+8]4`(キック・ハイハット・スネア・ハイハットの 4 拍)。
```

3. 「音色は全部で 14 種」のような、種類の数を書いた箇所があれば 15 にする(`grep -n "14 種\|14種" docs/guide/user-guide.md` で探す。無ければそのまま)。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings 2>&1 | grep -E "×|Test Files|Tests "; pnpm check 2>&1 | tail -2`
Expected: すべて PASS、0 errors。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts docs/guide/user-guide.md && git commit -m "docs: MML の drumkit の割り当てを、ヘルプとユーザーガイドに追記(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 実機の確認と音量の校正(隔離した Xvfb)

**Files:**
- Modify: `frontend/src/lib/mmlTimbres.ts`(`gainDb`)、`frontend/src/lib/mmlDrumkit.ts`(各楽器の `trimDb`、音作りの定数。実測で直す場合のみ)
- Modify: `docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md`(末尾に「実装後の確認」を追記)

**Interfaces:**
- Consumes: Task 1〜3 の成果。`frontend/dist` と `src-tauri/target/debug/tsumugi` は、必ず作り直してから確認する(`pnpm build` → `cargo build`。直接ビルドしたデバッグバイナリは `frontend/dist` を埋め込む)。
- Produces: 校正した `gainDb` / `trimDb`、spec の記録。

- [ ] **Step 1: 自動テストをすべて走らせる**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | grep -E "test result|FAILED|panicked" | head
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -3
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm test 2>&1 | grep -E "Test Files|Tests "
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short
```
Expected: `cargo test` は失敗 0。`pnpm check` は 0 errors。`pnpm test` は全件 PASS。`git status` が空(`tauri.gen.ts` を含め、未コミットの変更なし)。

- [ ] **Step 2: 実アプリを隔離して起動する**

メモリの `feedback-dev-server-verification-must-use-virtual-display.md` / `tauri-single-instance-blocks-xdg-isolated-verification.md` / `reference-isolated-instance-needs-short-xdg-path.md` / `debug-bridge-does-not-support-await.md` を先に読む。これまでの PR(`docs/superpowers/specs/2026-10-20-issue-499-mml-macro-design.md` の「実装後の確認」)と同じ方法で行う:

- ビルド: `cd frontend && pnpm build` のあとに `cd src-tauri && cargo build`。
- 起動: `Xvfb :<空き番号>`(`ls /tmp/.X11-unix` で空きを選ぶ)、`dbus-run-session`、`XDG_*` を `/tmp` 直下の短いパス(例: `/tmp/tsv499k`)、`WAYLAND_DISPLAY` は unset、`GDK_BACKEND=x11`、`target/debug/tsumugi` を直接起動。起動した全プロセスに、一意なトークン(`TSUMUGI_VERIFY_TOKEN`)を環境変数で付ける。
- アカウント: 起動前に `$XDG_CONFIG_HOME/com.onodai.tsumugi/settings.json` へ、ダミーのアカウントだけを書く(`{"accounts":[{"id":"00000000-0000-4000-8000-000000000499","host":"misskey.invalid","username":"verify","userId":"verifyuser","displayName":"verify","avatarUrl":null}]}`)。
- 起動後、アプリの起動が終わる(`data-testid="app-menu-trigger"` が出る)のを待つ。数分かかる。
- 画面: メニュー → 設定 → 通知 → 「通知音を鳴らす」ON → 種類「MML（打ち込み）」。入力は `xdotool type`。bridge で `fetch` の `ipc://` を包み、`play_notify_sound` に渡る `data:audio/wav;base64,…` を記録する(記録した値は、bridge から読み出して Python でデコードする)。

- [ ] **Step 3: 15 種すべてが描画できること、楽器ごとの音量を測る**

1. 15 種すべて(`@0 c` 〜 `@14 o2 c`)について、「試聴」を押して WAV を記録し、Python(`python3 -I`、標準ライブラリの `base64` / `struct` / `math`)で、ピークと RMS を計算する。**エラーなし・NaN なし・ピーク 0.36 以下**を確かめる。`fm` の `@0 c`(C4 の単音)の RMS を基準にする。
2. `drumkit` の楽器ごと(DRUM_MAP の 16 楽器。例: `@drumkit o2 c4`、`@drumkit o2 d4`、…)の 1 打の RMS とピークを測る。1 打ごとの長さが `TAIL_SEC` の範囲に収まり、末尾が切れていないこと(WAV の最後の 0.05 秒の最大値が小さいこと)も見る。
3. `drumkit` 全体の `gainDb` を、キックなど代表的な楽器の RMS が `fm` の RMS とおおむね揃う値に校正する。楽器どうしの RMS が大きく違う(±3 dB を超える)ものは、`mmlDrumkit.ts` の該当楽器の `trimDb` で揃える。**ピークが 0.36 を超える楽器は、`trimDb` を下げて 0.36 以下にする。**
4. 校正した値を `mmlTimbres.ts` の `drumkit.gainDb` と `mmlDrumkit.ts` の `trimDb` に反映し、`pnpm build` → `cargo build` → 再測定して、基準に収まることを確かめる。`mmlTimbres.test.ts` の `gainDb` の範囲(−30〜6)に収まること。

校正の結果(楽器ごとの RMS・ピーク、変更前後の `gainDb` / `trimDb`)は、Step 6 の spec の記録に残す。

- [ ] **Step 4: 割り当てと、3000 音の描画時間を確かめる**

1. 割り当てのない高さ(`@drumkit o4 c4` や `@drumkit o6 c4` など)の WAV が、無音(ピーク 0)であること。割り当てのある高さ(`@drumkit o2 c4`)は、無音でないこと。
2. 和音 `@drumkit 'o2 cf+'4`(キックとハイハットの同時)の WAV が、単独のキックより大きいこと(2 音が重なる)ことを確かめる。
3. 描画時間: `@drumkit t240 l64 o2 [[c f+ d f+]99]7`(2772 音。`totalSec` は 90 秒以内)を入力し、「試聴」を押してから `play_notify_sound` が呼ばれるまでの時間を測る。UI の「生成中… N 秒」の最大値と、bridge で測った経過時間を、spec に書く。遅すぎる(体感で 10 秒を超える)ときは止めて、使い回す楽器を増やす方針に切り替えてユーザーに相談する。
4. トラック 4 本(`;` で分けて、それぞれ別の高さの `@drumkit`)でも、エラーなく描画できること。

- [ ] **Step 5: UI の表示を確かめる**

- 「記法」を開くと、`@drumkit ドラム(…)` の行が出る。
- 音色のボタンに `@14 drumkit` が出て、押すとカーソル位置に `@drumkit` が入る。
- 色分け: `@drumkit` が、他の音色と同じ色(timbre)になる(`@drum` と `@drumkit` の読み分けも、DOM のクラスで確かめる)。
- スクリーンショットを撮って、自分で見る。

確認が終わったら、自分で起動した Xvfb / `dbus-run-session` / アプリのプロセスを、トークンで特定した PID を指定して止める(`pkill` / `killall` は使わない)。`/tmp` に作った隔離ホームと、Xvfb のロックファイルも片付ける。ユーザー自身の `cargo tauri dev` のプロセスには触れない。

- [ ] **Step 6: 結果を spec の末尾に記録する**

`docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md` の末尾に、次の節を追記する(実際に確認したことと、確認できなかったことを、事実のとおりに書く。数値と所要時間はこの手順で得た実測値を入れる。括弧書きのまま残さない):

```markdown
## 実装後の確認

(確認した日付・ブランチ・HEAD のコミット、隔離インスタンスの起動方法を書く。)

確認できたこと:
- 自動テスト: `cargo test` / `pnpm check` / `pnpm test` の結果(件数)。`rustfmt --check src/mml.rs` の差分の数(`main` と同じ 5 かどうか)。
- 15 種すべての描画: エラー・NaN の有無、ピークの最大値。
- `drumkit` の楽器ごとの 1 打の RMS とピーク、校正した `gainDb` / `trimDb`(変更前後)。`fm` の C4 の RMS を基準にした dB。
- 割り当てのない高さが無音になること、和音で 2 音が重なること。
- 3000 音近くを `drumkit` で鳴らしたときの描画時間。
- UI: 「記法」の行、音色ボタン `@14 drumkit`、色分け。

確認できなかったこと:
- 実際に音が鳴ること、音作り(各楽器の音の質、楽器どうしの音量バランス)の良し悪し。音声デバイスが無いので、波形の数値(RMS・ピーク)までを確認した。聴いた人の評価で、後から直す前提。
- Android の実機・エミュレーター、macOS / Windows。
```

- [ ] **Step 7: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && git add frontend/src/lib/mmlTimbres.ts frontend/src/lib/mmlDrumkit.ts docs/superpowers/specs/2026-10-30-issue-499-mml-drumkit-design.md && git commit -m "docs: MML の drumkit の音量を校正し、実装後の確認を記録(Issue #499)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: ブランチの扱いを決める(ユーザーに確認する)**

`superpowers:finishing-a-development-branch` に従う。PR を作る場合は、`.github/pull_request_template.md` の構成に手で合わせ、本文に `Refs #499` と書く(`Fixes` / `Closes` は書かない。#499 の候補が未着手で残るため)。`gh pr merge` は `--merge`(squash にしない。必須チェックの `e2e` が実行中だとブロックされるので、その場合は待たずにユーザーに伝える)。push 後に CI を待つループは作らない(ユーザーが見る)。#499 のチェックボックス(「ドラムの割り当て」)を更新するかは、ユーザーに確認してから行う。作業ブランチの削除も、ユーザーが頼んだときだけ行う。
