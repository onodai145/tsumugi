# MML 入力欄のエディタ化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定の通知音「MML（打ち込み）」の入力欄を CodeMirror 6 のエディタにし、構文ハイライト・エラー下線・エラー位置への移動・使用量の表示・a11y を入れる。

**Architecture:** 字句解析(`mmlLexer`)・使用量(`mmlUsage`)・位置変換(`mmlPosition`)は DOM に依存しない純関数にする。CodeMirror 固有の拡張(ハイライト、下線、テーマ)は `mmlEditorExtensions.ts`、Svelte への包みは `MmlEditor.svelte` に分ける。`SoundChoicePicker.svelte` は `MmlEditor` を動的 `import()` で読み込み、既存の 300ms デバウンスの `parseMml` の仕組みはそのまま使う。

**Tech Stack:** Svelte 5(runes)、CodeMirror 6(`@codemirror/state` / `@codemirror/view` / `@codemirror/commands`)、Vitest + jsdom、Tailwind。

**Spec:** `docs/superpowers/specs/2026-10-17-issue-500-mml-editor-design.md`

## Global Constraints

- 依存の追加は `@codemirror/state` / `@codemirror/view` / `@codemirror/commands` の 3 つだけ。`@codemirror/language` と `@codemirror/lint` は入れない。
- 新しいテーマトークン(CSS 変数)は作らない。配色は既存トークン(`--text` / `--text-dim` / `--accent` / `--danger` / `--warning`)の `color-mix` と太字で区別する。`--success` / `--info` は流用しない(style-guide §10)。
- 上限は 5000 文字・3000 音・4 トラック・90 秒。`lib/mmlLimits.ts` に置き、Rust の `MAX_*` と同じ値にする。
- `SoundChoicePicker` の props(`value` / `allowInherit` / `onvalidity`)、保存形式(`mml:<テキスト>`)、状態行(`data-testid="mml-status"`)の既存の文言は変えない。
- `MmlEditor` は動的 `import()` で読み込む(設定で MML モードを選ぶまでバンドルに乗せない)。
- 高さは最小 3 行で内容に応じて伸びる。内側スクロールなし。`resize-y` は使わない。
- Rust 側は挙動を変えない。`mml.rs` にテストを 1 つ足すだけ。
- 新しい UI の値は `docs/design/style-guide.md` に沿う(`rounded-[Npx]` などの即値を作らない)。フォーカス表示は `focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50`。
- コミットメッセージは件名のみ(本文・箇条書きなし)。ハーネスの指示どおり末尾に `Co-Authored-By` トレーラーを付ける。`--no-verify` は使わない。`git commit` が失敗・タイムアウトしたら止めてユーザーに報告する(再試行しない)。
- 作業ブランチは `feat/issue-500-mml-editor`(作成済み)。`main` には触らない。
- 以降のコマンドは作業ディレクトリが変わっても動くよう、絶対パスで書く。リポジトリルートは `/home/onodai145/repos/github.com/onodai145/tsumugi`。
- 単一のフロントテストは `cd <root>/frontend && npx vitest run <path>`(`pnpm test -- <path>` はパスを無視して全件走る)。

## Review Focus

仕様が黙っているが、使う人が踏みやすい入力と条件。それぞれ、持ち主のタスクにテストを足してある。

1. 再生時間がちょうど 90 秒で、浮動小数点の誤差で 90.0000004 になる MML。パーサーは通す(`TIME_EPS = 1e-6`)ので、使用量の表示を「超過」にしてはいけない。(Task 1)
2. 絵文字などサロゲートペアを含む MML。Rust の文字位置(Unicode スカラ値の添字)と JS の添字(UTF-16)がずれても、下線とカーソル移動が正しい文字に付く。(Task 2)
3. エラー位置が文書の末尾(5000 字超過は `position = 5000`)や、空文書。下線の設定で例外を出さず、末尾の 1 文字に付けるか、何も付けない。(Task 2、Task 4)
4. エラー表示のまま編集した直後。位置がずれた古い下線を残さない(次の解析結果が来る前に消す)。(Task 6)
5. 大文字・空白・改行を含む MML。パーサーは空白を除いて小文字化して読むので、色分けも同じ前提で読む。(Task 3)
6. 同じ画面に `SoundChoicePicker` が 2 つある(`AddColumnModal`)。`aria-describedby` の id が衝突しない。(Task 6)

---

### Task 1: 上限の定数と使用量の計算

**Files:**
- Create: `frontend/src/lib/mmlLimits.ts`
- Create: `frontend/src/lib/mmlLimits.test.ts`
- Create: `frontend/src/lib/mmlUsage.ts`
- Create: `frontend/src/lib/mmlUsage.test.ts`
- Modify: `src-tauri/src/mml.rs`(`mod tests` にテストを 1 つ足す)

**Interfaces:**
- Produces(`mmlLimits.ts`): `MML_LIMITS: { readonly chars: 5000; readonly notes: 3000; readonly tracks: 4; readonly sec: 90 }`、`NEAR_RATIO = 0.9`、`SEC_EPS = 1e-6`
- Produces(`mmlUsage.ts`):
  - `type UsageLevel = "normal" | "near" | "over"`
  - `type UsageKey = "chars" | "notes" | "tracks" | "sec"`
  - `interface UsageItem { key: UsageKey; label: string; value: number | null; limit: number; level: UsageLevel }`
  - `interface ParsedUsage { noteCount: number; totalSec: number }`
  - `countChars(text: string): number`、`countTracks(text: string): number`
  - `usageLevel(value: number | null, limit: number): UsageLevel`
  - `computeUsage(text: string, parsed: ParsedUsage | null): UsageItem[]`(順序は chars, notes, tracks, sec)
  - `formatUsage(item: UsageItem): string`

- [ ] **Step 1: 上限の表のテストを書く**

`frontend/src/lib/mmlLimits.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { MML_LIMITS, NEAR_RATIO } from "./mmlLimits";

describe("MML_LIMITS", () => {
  // src-tauri/src/mml.rs の MAX_TEXT_CHARS / MAX_NOTES / MAX_TRACKS / MAX_TOTAL_SEC と同じ値。
  // Rust 側のテスト(limits_match_frontend_table)も同じ表を持つ。片方だけ変えると、どちらかのテストが落ちる。
  it("Rust のパーサーの上限と同じ値", () => {
    expect(MML_LIMITS).toEqual({ chars: 5000, notes: 3000, tracks: 4, sec: 90 });
  });

  it("上限の 9 割から「近い」とみなす", () => {
    expect(NEAR_RATIO).toBe(0.9);
  });
});
```

- [ ] **Step 2: 使用量のテストを書く**

`frontend/src/lib/mmlUsage.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { computeUsage, countChars, countTracks, formatUsage, usageLevel } from "./mmlUsage";

describe("countChars", () => {
  it("Rust の chars().count() と同じく、サロゲートペアを 1 文字と数える", () => {
    expect(countChars("c4")).toBe(2);
    expect(countChars("😀c")).toBe(2);
    expect(countChars("")).toBe(0);
  });

  it("空白と改行も数える(パーサーは空白を含めた文字数で上限を見る)", () => {
    expect(countChars("c4 d4\ne4")).toBe(8);
  });
});

describe("countTracks", () => {
  it("; で分けた、空白以外を含む区間の数", () => {
    expect(countTracks("c4;d4")).toBe(2);
    expect(countTracks("c4;;d4;")).toBe(2);
    expect(countTracks(" ; \n ;")).toBe(0);
    expect(countTracks("")).toBe(0);
    expect(countTracks("c4")).toBe(1);
  });
});

describe("usageLevel", () => {
  it("9 割未満は normal、9 割以上で上限ちょうどまでは near、超えたら over", () => {
    expect(usageLevel(4499, 5000)).toBe("normal");
    expect(usageLevel(4500, 5000)).toBe("near");
    expect(usageLevel(5000, 5000)).toBe("near");
    expect(usageLevel(5001, 5000)).toBe("over");
  });

  it("値が無い(null)なら normal", () => {
    expect(usageLevel(null, 5000)).toBe("normal");
  });

  it("再生時間がちょうど 90 秒で、浮動小数点の誤差で超えても over にしない(パーサーは 1e-6 の許容幅で通す)", () => {
    expect(usageLevel(90.0000004, 90)).toBe("near");
    expect(usageLevel(90.00001, 90)).toBe("over");
  });
});

describe("computeUsage", () => {
  it("成功した解析があれば、音数と再生時間も入る", () => {
    const items = computeUsage("c4;d4", { noteCount: 2700, totalSec: 12.34 });
    expect(items.map((i) => [i.key, i.value, i.limit, i.level])).toEqual([
      ["chars", 5, 5000, "normal"],
      ["notes", 2700, 3000, "near"],
      ["tracks", 2, 4, "normal"],
      ["sec", 12.34, 90, "normal"],
    ]);
  });

  it("解析の結果が無ければ、音数と再生時間は null", () => {
    const items = computeUsage("c4", null);
    expect(items.find((i) => i.key === "notes")?.value).toBeNull();
    expect(items.find((i) => i.key === "sec")?.value).toBeNull();
    expect(items.find((i) => i.key === "chars")?.value).toBe(2);
  });
});

describe("formatUsage", () => {
  it("項目ごとの表記にする", () => {
    const items = computeUsage("c4;d4", { noteCount: 120, totalSec: 12.34 });
    expect(items.map(formatUsage)).toEqual(["文字 5/5000", "音 120/3000", "トラック 2/4", "再生 12.3/90 秒"]);
  });

  it("値が無いときは — を出す", () => {
    const items = computeUsage("c4", null);
    expect(items.map(formatUsage)).toEqual(["文字 2/5000", "音 —/3000", "トラック 1/4", "再生 —/90 秒"]);
  });
});
```

- [ ] **Step 3: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLimits.test.ts src/lib/mmlUsage.test.ts`
Expected: FAIL(`Failed to resolve import "./mmlLimits"` など)

- [ ] **Step 4: 実装する**

`frontend/src/lib/mmlLimits.ts`:

```ts
// MML の上限。src-tauri/src/mml.rs の MAX_TEXT_CHARS / MAX_NOTES / MAX_TRACKS / MAX_TOTAL_SEC と同じ値にする。
// 両側のテストが同じ表を持つ(mmlLimits.test.ts と mml.rs の limits_match_frontend_table)。
export const MML_LIMITS = { chars: 5000, notes: 3000, tracks: 4, sec: 90 } as const;

/** 上限のこの割合以上になったら「近い」と知らせる */
export const NEAR_RATIO = 0.9;

/** 再生時間の浮動小数点誤差の許容幅。Rust の TIME_EPS と同じ */
export const SEC_EPS = 1e-6;
```

`frontend/src/lib/mmlUsage.ts`:

```ts
// MML の使用量(文字数・音数・トラック数・再生時間)と、上限に対する段階。
import { MML_LIMITS, NEAR_RATIO, SEC_EPS } from "./mmlLimits";

export type UsageLevel = "normal" | "near" | "over";
export type UsageKey = "chars" | "notes" | "tracks" | "sec";

export interface UsageItem {
  key: UsageKey;
  label: string;
  /** 分からない(解析が通っていない)ときは null */
  value: number | null;
  limit: number;
  level: UsageLevel;
}

/** `parseMml` が成功したときの値 */
export interface ParsedUsage {
  noteCount: number;
  totalSec: number;
}

/** Rust の `chars().count()` と同じ数え方(サロゲートペアは 1 文字) */
export function countChars(text: string): number {
  return [...text].length;
}

/** パーサーと同じく、`;` で分けた区間のうち空白以外を含むものの数 */
export function countTracks(text: string): number {
  return text.split(";").filter((seg) => seg.trim() !== "").length;
}

export function usageLevel(value: number | null, limit: number): UsageLevel {
  if (value === null) return "normal";
  if (value > limit + SEC_EPS) return "over";
  if (value >= limit * NEAR_RATIO) return "near";
  return "normal";
}

export function computeUsage(text: string, parsed: ParsedUsage | null): UsageItem[] {
  const item = (key: UsageKey, label: string, value: number | null): UsageItem => ({
    key,
    label,
    value,
    limit: MML_LIMITS[key],
    level: usageLevel(value, MML_LIMITS[key]),
  });
  return [
    item("chars", "文字", countChars(text)),
    item("notes", "音", parsed ? parsed.noteCount : null),
    item("tracks", "トラック", countTracks(text)),
    item("sec", "再生", parsed ? parsed.totalSec : null),
  ];
}

export function formatUsage(item: UsageItem): string {
  const value = item.value === null ? "—" : item.key === "sec" ? item.value.toFixed(1) : String(item.value);
  const unit = item.key === "sec" ? " 秒" : "";
  return `${item.label} ${value}/${item.limit}${unit}`;
}
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLimits.test.ts src/lib/mmlUsage.test.ts`
Expected: PASS(全件)

- [ ] **Step 6: Rust 側に、同じ表を固定するテストを足す**

`src-tauri/src/mml.rs` の `mod tests` 内、`fn approx` の直後に足す。

old:

```rust
    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
```

new:

```rust
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
```

- [ ] **Step 7: Rust のテストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml::tests::limits_match_frontend_table`
Expected: `test result: ok. 1 passed`

- [ ] **Step 8: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/mmlLimits.ts frontend/src/lib/mmlLimits.test.ts frontend/src/lib/mmlUsage.ts frontend/src/lib/mmlUsage.test.ts src-tauri/src/mml.rs
git commit -m "feat: MMLの使用量の計算と上限の表を追加(Issue #500)"
```

---

### Task 2: エラー位置の変換

**Files:**
- Create: `frontend/src/lib/mmlPosition.ts`
- Create: `frontend/src/lib/mmlPosition.test.ts`

**Interfaces:**
- Produces:
  - `charPosToIndex(text: string, pos: number): number` — Rust の文字位置(Unicode スカラ値の添字)を、`text` の添字(UTF-16)にする。`pos` が文字数以上なら `text.length`。
  - `errorRange(text: string, pos: number): { from: number; to: number } | null` — 下線を引く範囲。空文書は `null`。`pos` が文字数以上なら最後の 1 文字。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlPosition.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { charPosToIndex, errorRange } from "./mmlPosition";

describe("charPosToIndex", () => {
  it("ASCII だけなら、そのまま", () => {
    expect(charPosToIndex("c4 x", 3)).toBe(3);
  });

  it("サロゲートペアは Rust では 1 文字、JS では 2 単位", () => {
    // 「😀」は Rust の 1 文字目。2 文字目(c)は JS の添字では 2
    expect(charPosToIndex("😀cx", 1)).toBe(2);
    expect(charPosToIndex("😀cx", 2)).toBe(3);
  });

  it("文字数以上なら、末尾", () => {
    expect(charPosToIndex("c4", 2)).toBe(2);
    expect(charPosToIndex("c4", 99)).toBe(2);
    expect(charPosToIndex("😀", 5)).toBe(2);
    expect(charPosToIndex("", 0)).toBe(0);
  });
});

describe("errorRange", () => {
  it("その位置の 1 文字", () => {
    expect(errorRange("c4 x", 3)).toEqual({ from: 3, to: 4 });
  });

  it("手前にサロゲートペアがあっても、正しい文字に付く", () => {
    expect(errorRange("😀cx", 2)).toEqual({ from: 3, to: 4 });
  });

  it("エラー位置がサロゲートペアなら、2 単位ぶん", () => {
    expect(errorRange("c😀", 1)).toEqual({ from: 1, to: 3 });
  });

  it("文字数以上(5000 字超過のエラーは position = 文書の長さ)なら、最後の 1 文字", () => {
    expect(errorRange("ab", 2)).toEqual({ from: 1, to: 2 });
    expect(errorRange("ab", 5000)).toEqual({ from: 1, to: 2 });
    expect(errorRange("a😀", 5000)).toEqual({ from: 1, to: 3 });
  });

  it("空文書には引かない", () => {
    expect(errorRange("", 0)).toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlPosition.test.ts`
Expected: FAIL(`Failed to resolve import "./mmlPosition"`)

- [ ] **Step 3: 実装する**

`frontend/src/lib/mmlPosition.ts`:

```ts
// `parseMml` のエラー位置を、エディタの文字列添字にする。
// Rust の `position` は Unicode スカラ値の添字。JS の文字列は UTF-16 なので、サロゲートペアがあるとずれる。

/** Rust の文字位置を、`text` の添字(UTF-16)にする。文字数以上なら `text.length` */
export function charPosToIndex(text: string, pos: number): number {
  let index = 0;
  let n = 0;
  for (const ch of text) {
    if (n >= pos) break;
    index += ch.length;
    n++;
  }
  return index;
}

/** エラー下線の範囲。空文書は null。位置が文字数以上なら最後の 1 文字 */
export function errorRange(text: string, pos: number): { from: number; to: number } | null {
  const chars = [...text];
  if (chars.length === 0) return null;
  const i = Math.max(0, Math.min(pos, chars.length - 1));
  const from = charPosToIndex(text, i);
  return { from, to: from + chars[i].length };
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlPosition.test.ts`
Expected: PASS(全件)

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/mmlPosition.ts frontend/src/lib/mmlPosition.test.ts
git commit -m "feat: MMLのエラー位置をエディタの添字に直す関数を追加(Issue #500)"
```

---

### Task 3: 字句解析

**Files:**
- Create: `frontend/src/lib/mmlLexer.ts`
- Create: `frontend/src/lib/mmlLexer.test.ts`

**Interfaces:**
- Consumes: `TIMBRE_LIST`(`lib/mmlTimbres.ts`。各要素の `name` が `@名前` の名前)
- Produces:
  - `type MmlTokenKind = "note" | "rest" | "command" | "timbre" | "number" | "repeat" | "separator" | "unknown"`
  - `interface MmlToken { from: number; to: number; kind: MmlTokenKind }` — `from`/`to` は `text` の添字(UTF-16、`to` は含まない)
  - `lexMml(text: string): MmlToken[]` — 位置の昇順で、重ならない。構文が誤っていても例外を出さず、最後まで分類する。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mmlLexer.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { lexMml } from "./mmlLexer";

/** [トークンの文字列, 種別] の列にして比べやすくする */
function lex(text: string): [string, string][] {
  return lexMml(text).map((t) => [text.slice(t.from, t.to), t.kind]);
}

describe("lexMml", () => {
  it("コマンドと数字", () => {
    expect(lex("t120 o5 l8")).toEqual([
      ["t", "command"],
      ["120", "number"],
      ["o", "command"],
      ["5", "number"],
      ["l", "command"],
      ["8", "number"],
    ]);
  });

  it("音符は音名と半音記号まで、長さと付点は number", () => {
    expect(lex("c#4. d-8 e+")).toEqual([
      ["c#", "note"],
      ["4.", "number"],
      ["d-", "note"],
      ["8", "number"],
      ["e+", "note"],
    ]);
  });

  it("休符とタイは rest", () => {
    expect(lex("r4&c8")).toEqual([
      ["r", "rest"],
      ["4", "number"],
      ["&", "rest"],
      ["c", "note"],
      ["8", "number"],
    ]);
  });

  it("オクターブの上げ下げは command", () => {
    expect(lex(">c<")).toEqual([
      [">", "command"],
      ["c", "note"],
      ["<", "command"],
    ]);
  });

  it("音色は @番号 と @名前", () => {
    expect(lex("@3c")).toEqual([
      ["@3", "timbre"],
      ["c", "note"],
    ]);
    expect(lex("@bell c")).toEqual([
      ["@bell", "timbre"],
      ["c", "note"],
    ]);
  });

  it("音色名は最長一致で読む(@epiano は e ではなく音色)", () => {
    expect(lex("@epiano")).toEqual([["@epiano", "timbre"]]);
  });

  it("繰り返しは [ と、] とその直後の回数", () => {
    expect(lex("[cde]3")).toEqual([
      ["[", "repeat"],
      ["c", "note"],
      ["d", "note"],
      ["e", "note"],
      ["]3", "repeat"],
    ]);
  });

  it("トラック区切り", () => {
    expect(lex("c;e")).toEqual([
      ["c", "note"],
      [";", "separator"],
      ["e", "note"],
    ]);
  });

  it("構文が誤っていても例外を出さず、最後まで分類する(解析は parseMml の仕事)", () => {
    expect(lex("c4 x")).toEqual([
      ["c", "note"],
      ["4", "number"],
      ["x", "unknown"],
    ]);
    expect(lex("@")).toEqual([["@", "timbre"]]);
    expect(lexMml("@zzz").map((t) => t.kind)).toEqual(["timbre", "unknown", "unknown", "unknown"]);
    expect(lexMml("]]").map((t) => t.kind)).toEqual(["repeat", "repeat"]);
  });

  it("大文字・空白・改行があっても、パーサーと同じく読む(空白は除き、小文字化する)", () => {
    expect(lex("C4\nD4")).toEqual([
      ["C", "note"],
      ["4", "number"],
      ["D", "note"],
      ["4", "number"],
    ]);
    expect(lex("T 120")).toEqual([
      ["T", "command"],
      ["120", "number"],
    ]);
    expect(lex("@BELLc")).toEqual([
      ["@BELL", "timbre"],
      ["c", "note"],
    ]);
  });

  it("空白をまたぐトークンは、空白を含む範囲になる", () => {
    const [t] = lexMml("@ bell");
    expect(t).toEqual({ from: 0, to: 6, kind: "timbre" });
  });

  it("サロゲートペアは 1 つの unknown で、後続の位置は UTF-16 の添字", () => {
    expect(lexMml("😀c")).toEqual([
      { from: 0, to: 2, kind: "unknown" },
      { from: 2, to: 3, kind: "note" },
    ]);
  });

  it("空文字列は空", () => {
    expect(lexMml("")).toEqual([]);
    expect(lexMml("  \n ")).toEqual([]);
  });

  it("位置は昇順で重ならない", () => {
    const tokens = lexMml("t120 o5 l8 [c#4.d-8]3 r4&e @bell g; >a<b @3 x");
    for (let i = 1; i < tokens.length; i++) expect(tokens[i].from).toBeGreaterThanOrEqual(tokens[i - 1].to);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts`
Expected: FAIL(`Failed to resolve import "./mmlLexer"`)

- [ ] **Step 3: 実装する**

`frontend/src/lib/mmlLexer.ts`:

```ts
// MML の寛容な字句解析(Issue #500)。色分けのためだけに使い、構文は検証しない(検証は Rust の parse_mml)。
// パーサーと同じく、空白を除いて ASCII だけ小文字化して読む。範囲は元のテキストの添字(UTF-16)。
import { TIMBRE_LIST } from "./mmlTimbres";

export type MmlTokenKind = "note" | "rest" | "command" | "timbre" | "number" | "repeat" | "separator" | "unknown";

export interface MmlToken {
  from: number;
  to: number;
  kind: MmlTokenKind;
}

interface Sym {
  ch: string;
  from: number;
  to: number;
}

const NOTES = new Set(["a", "b", "c", "d", "e", "f", "g"]);
const NUMBERED_COMMANDS = new Set(["o", "l", "t", "v"]);
const TIMBRE_NAMES: string[] = TIMBRE_LIST.map((p) => p.name);

function asciiLower(ch: string): string {
  return ch >= "A" && ch <= "Z" ? ch.toLowerCase() : ch;
}

function isDigit(ch: string): boolean {
  return ch.length === 1 && ch >= "0" && ch <= "9";
}

function isAccidental(ch: string): boolean {
  return ch === "#" || ch === "+" || ch === "-";
}

/** 空白を除いた (小文字化した文字, 元の範囲) の列 */
function symbols(text: string): Sym[] {
  const out: Sym[] = [];
  let at = 0;
  for (const ch of text) {
    const from = at;
    at += ch.length;
    if (!/\s/u.test(ch)) out.push({ ch: asciiLower(ch), from, to: at });
  }
  return out;
}

export function lexMml(text: string): MmlToken[] {
  const syms = symbols(text);
  const out: MmlToken[] = [];
  const at = (i: number): string => (i < syms.length ? syms[i].ch : "");
  const push = (start: number, end: number, kind: MmlTokenKind): void => {
    out.push({ from: syms[start].from, to: syms[end - 1].to, kind });
  };
  const digitsEnd = (i: number): number => {
    let j = i;
    while (isDigit(at(j))) j++;
    return j;
  };
  /** 音符・休符の後ろの長さと付点を number として積み、次の位置を返す */
  const lengthAndDots = (i: number): number => {
    let j = digitsEnd(i);
    while (at(j) === ".") j++;
    if (j > i) push(i, j, "number");
    return j;
  };
  /** i から始まる音色名の長さ(記号数)。無ければ 0。最長一致 */
  const timbreNameLen = (i: number): number => {
    let best = 0;
    for (const name of TIMBRE_NAMES) {
      if (name.length > best && i + name.length <= syms.length && [...name].every((c, k) => syms[i + k].ch === c)) {
        best = name.length;
      }
    }
    return best;
  };

  let i = 0;
  while (i < syms.length) {
    const ch = syms[i].ch;
    if (NOTES.has(ch)) {
      let j = i + 1;
      if (isAccidental(at(j))) j++;
      push(i, j, "note");
      i = lengthAndDots(j);
    } else if (ch === "r") {
      push(i, i + 1, "rest");
      i = lengthAndDots(i + 1);
    } else if (ch === "&") {
      push(i, i + 1, "rest");
      i++;
    } else if (NUMBERED_COMMANDS.has(ch)) {
      push(i, i + 1, "command");
      const j = digitsEnd(i + 1);
      if (j > i + 1) push(i + 1, j, "number");
      i = j;
    } else if (ch === "<" || ch === ">") {
      push(i, i + 1, "command");
      i++;
    } else if (ch === "@") {
      let j = digitsEnd(i + 1);
      if (j === i + 1) j = i + 1 + timbreNameLen(i + 1);
      push(i, j, "timbre");
      i = j;
    } else if (ch === "[") {
      push(i, i + 1, "repeat");
      i++;
    } else if (ch === "]") {
      const j = digitsEnd(i + 1);
      push(i, j, "repeat");
      i = j;
    } else if (ch === ";") {
      push(i, i + 1, "separator");
      i++;
    } else {
      push(i, i + 1, "unknown");
      i++;
    }
  }
  return out;
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/mmlLexer.test.ts`
Expected: PASS(全件)。落ちたら、テストの期待ではなく、パーサー(`src-tauri/src/mml.rs` の `parse_track`)の規則と照らして実装を直す。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/mmlLexer.ts frontend/src/lib/mmlLexer.test.ts
git commit -m "feat: MMLの色分け用の字句解析を追加(Issue #500)"
```

---

### Task 4: CodeMirror の依存と拡張(ハイライト・下線・テーマ)

**Files:**
- Modify: `frontend/package.json`、`frontend/pnpm-lock.yaml`(`pnpm add` が更新する)
- Modify: `frontend/vite.config.ts`(`optimizeDeps.include`)
- Modify: `frontend/src/test-setup.ts`(jsdom に足りない Range の API)
- Create: `frontend/src/ui/settings/mmlEditorExtensions.ts`
- Create: `frontend/src/ui/settings/mmlEditorExtensions.test.ts`

**Interfaces:**
- Consumes: `lexMml`、`MmlTokenKind`(Task 3)
- Produces(`mmlEditorExtensions.ts`):
  - `mmlHighlight: Extension` — 文書全体を字句解析して `Decoration.mark` を付ける。クラスは `mml-rest` / `mml-command` / `mml-timbre` / `mml-number` / `mml-repeat` / `mml-separator`(`note` と `unknown` には付けない)
  - `errorField: StateField<DecorationSet>` と `setErrorRange: StateEffect<{ from: number; to: number } | null>` — 下線(クラス `mml-error`)。文書の編集に追従してずれる。範囲は文書の長さに丸め、空になったら消す
  - `mmlTheme: Extension` — 見た目

- [ ] **Step 1: 依存を足す**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm add @codemirror/state @codemirror/view @codemirror/commands`
Expected: `package.json` の `dependencies` に 3 つ入り、`pnpm-lock.yaml` が更新される。

- [ ] **Step 2: vite の事前バンドルに足す**

`frontend/vite.config.ts` の `optimizeDeps.include` に足す(動的 import の依存は、実行時の再最適化でフルリロードを起こして `vite-plugin-svelte` の仮想 CSS を壊すため。`tone` と同じ扱い)。

old:

```ts
      "mfm-js",
      "tone",
    ],
```

new:

```ts
      "mfm-js",
      "tone",
      "@codemirror/state",
      "@codemirror/view",
      "@codemirror/commands",
    ],
```

- [ ] **Step 3: jsdom に Range の矩形 API を足す**

CodeMirror は測定で `Range.getClientRects` / `getBoundingClientRect` を呼ぶが、jsdom には無い。`frontend/src/test-setup.ts` の末尾に足す。

```ts

// CodeMirror が測定に使う Range の矩形 API。jsdom には無いので、何も返さない実装で補う。
// (他にも足りない API がテストで出たら、ここに必要なものだけ足す。)
if (!Range.prototype.getClientRects) {
  Range.prototype.getClientRects = () =>
    ({ length: 0, item: () => null, [Symbol.iterator]: function* () {} }) as unknown as DOMRectList;
}
if (!Range.prototype.getBoundingClientRect) {
  Range.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0, toJSON: () => ({}) }) as DOMRect;
}
```

- [ ] **Step 4: 失敗するテストを書く**

`frontend/src/ui/settings/mmlEditorExtensions.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { errorField, mmlHighlight, setErrorRange } from "./mmlEditorExtensions";

const views: EditorView[] = [];

function mount(doc: string): { view: EditorView; texts: (cls: string) => string[] } {
  const parent = document.createElement("div");
  document.body.appendChild(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({ doc, extensions: [mmlHighlight, errorField] }),
  });
  views.push(view);
  return {
    view,
    texts: (cls) => [...parent.querySelectorAll(`.${cls}`)].map((e) => e.textContent ?? ""),
  };
}

afterEach(() => {
  while (views.length) views.pop()!.destroy();
  document.body.innerHTML = "";
});

describe("mmlHighlight", () => {
  it("コマンドと数字に色のクラスを付ける", () => {
    const { texts } = mount("t120 o5 c4");
    expect(texts("mml-command")).toEqual(["t", "o"]);
    expect(texts("mml-number")).toEqual(["120", "5", "4"]);
  });

  it("音色・繰り返し・区切り・休符", () => {
    const { texts } = mount("@bell c [d]2 r ; e");
    expect(texts("mml-timbre")).toEqual(["@bell"]);
    expect(texts("mml-repeat")).toEqual(["[", "]2"]);
    expect(texts("mml-rest")).toEqual(["r"]);
    expect(texts("mml-separator")).toEqual([";"]);
  });

  it("構文が誤っていても例外を出さず、描画できる", () => {
    expect(() => mount("c4 x @zzz ]]")).not.toThrow();
  });

  it("編集すると色分けも付け直される", () => {
    const { view, texts } = mount("c4");
    view.dispatch({ changes: { from: 2, insert: " t120" } });
    expect(texts("mml-command")).toEqual(["t"]);
    expect(texts("mml-number")).toEqual(["4", "120"]);
  });
});

describe("errorField", () => {
  it("setErrorRange で範囲に下線のクラスを付け、null で消す", () => {
    const { view, texts } = mount("c4 x");
    view.dispatch({ effects: setErrorRange.of({ from: 3, to: 4 }) });
    expect(texts("mml-error")).toEqual(["x"]);
    view.dispatch({ effects: setErrorRange.of(null) });
    expect(texts("mml-error")).toEqual([]);
  });

  it("手前を編集すると、下線は同じ文字に付いたままずれる", () => {
    const { view, texts } = mount("c4 x");
    view.dispatch({ effects: setErrorRange.of({ from: 3, to: 4 }) });
    view.dispatch({ changes: { from: 0, insert: "d" } });
    expect(texts("mml-error")).toEqual(["x"]);
  });

  it("範囲が文書の長さを超えていたら丸める。全く外なら付けない(例外を出さない)", () => {
    const { view, texts } = mount("c4 x");
    view.dispatch({ effects: setErrorRange.of({ from: 3, to: 99 }) });
    expect(texts("mml-error")).toEqual(["x"]);
    view.dispatch({ effects: setErrorRange.of({ from: 10, to: 12 }) });
    expect(texts("mml-error")).toEqual([]);
  });

  it("空文書に範囲を設定しても例外を出さず、何も付けない", () => {
    const { view, texts } = mount("");
    expect(() => view.dispatch({ effects: setErrorRange.of({ from: 0, to: 1 }) })).not.toThrow();
    expect(texts("mml-error")).toEqual([]);
  });
});
```

- [ ] **Step 5: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/mmlEditorExtensions.test.ts`
Expected: FAIL(`Failed to resolve import "./mmlEditorExtensions"`)

- [ ] **Step 6: 実装する**

`frontend/src/ui/settings/mmlEditorExtensions.ts`:

```ts
// MML エディタの CodeMirror 拡張(Issue #500)。色分け・エラー下線・見た目。
// 色は既存のテーマトークンだけを使う(カスタムテーマの対応を増やさない)。色だけでなく太字でも区別する。
import { RangeSetBuilder, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { lexMml, type MmlTokenKind } from "../../lib/mmlLexer";

const MARKS: Record<MmlTokenKind, Decoration | null> = {
  note: null,
  unknown: null,
  rest: Decoration.mark({ class: "mml-rest" }),
  command: Decoration.mark({ class: "mml-command" }),
  timbre: Decoration.mark({ class: "mml-timbre" }),
  number: Decoration.mark({ class: "mml-number" }),
  repeat: Decoration.mark({ class: "mml-repeat" }),
  separator: Decoration.mark({ class: "mml-separator" }),
};

function buildDecorations(doc: string): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const token of lexMml(doc)) {
    const mark = MARKS[token.kind];
    if (mark) builder.add(token.from, token.to, mark);
  }
  return builder.finish();
}

/** 文書全体を字句解析して色分けする。上限が 5000 文字なので、編集のたびに作り直して足りる */
export const mmlHighlight: Extension = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view.state.doc.toString());
    }
    update(update: ViewUpdate) {
      if (update.docChanged) this.decorations = buildDecorations(update.state.doc.toString());
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

const errorMark = Decoration.mark({ class: "mml-error" });

/** エラー下線の範囲を設定する。null で消す */
export const setErrorRange = StateEffect.define<{ from: number; to: number } | null>();

export const errorField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(decorations, tr) {
    let next = decorations.map(tr.changes);
    for (const effect of tr.effects) {
      if (!effect.is(setErrorRange)) continue;
      const range = effect.value;
      if (!range) {
        next = Decoration.none;
        continue;
      }
      const from = Math.max(0, range.from);
      const to = Math.min(range.to, tr.state.doc.length);
      next = from < to ? Decoration.set([errorMark.range(from, to)]) : Decoration.none;
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** 枠(角丸・枠線・フォーカスリング)は MmlEditor.svelte のホスト要素が持つ。ここは中身だけ */
export const mmlTheme: Extension = EditorView.theme({
  "&": { fontSize: "0.75rem", color: "var(--text)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace)",
    lineHeight: "1.5",
  },
  ".cm-content": { padding: "0.5rem 0.625rem", minHeight: "calc(4.5em + 1rem)", caretColor: "var(--text)" },
  ".cm-line": { padding: "0" },
  ".cm-placeholder": { color: "var(--text-dim)" },
  ".mml-rest": { color: "var(--text-dim)" },
  ".mml-command": { color: "var(--accent)" },
  ".mml-timbre": { color: "color-mix(in srgb, var(--accent) 50%, var(--text))", fontWeight: "700" },
  ".mml-number": { color: "color-mix(in srgb, var(--accent) 35%, var(--text))" },
  ".mml-repeat": { fontWeight: "700" },
  ".mml-separator": { color: "var(--text-dim)", fontWeight: "700" },
  ".mml-error": { textDecoration: "underline wavy var(--danger)", textUnderlineOffset: "2px" },
});
```

- [ ] **Step 7: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/mmlEditorExtensions.test.ts`
Expected: PASS(全件)。`getClientRects` などの未実装で落ちる場合は、Step 3 のスタブに必要なものだけ足す。同じ種別の隣り合う装飾は CodeMirror が 1 つの要素にまとめることがあるので、まとまって失敗したらテスト文字列の並びを見直す(実装の期待は変えない)。

- [ ] **Step 8: 型検査を通す**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check`
Expected: エラー 0

- [ ] **Step 9: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/package.json frontend/pnpm-lock.yaml frontend/vite.config.ts frontend/src/test-setup.ts frontend/src/ui/settings/mmlEditorExtensions.ts frontend/src/ui/settings/mmlEditorExtensions.test.ts
git commit -m "feat: MMLエディタのCodeMirror拡張(色分け・エラー下線)を追加(Issue #500)"
```

---

### Task 5: MmlEditor コンポーネント

**Files:**
- Create: `frontend/src/ui/settings/MmlEditor.svelte`
- Create: `frontend/src/ui/settings/MmlEditor.test.ts`

**Interfaces:**
- Consumes: `mmlHighlight` / `errorField` / `setErrorRange` / `mmlTheme`(Task 4)、`errorRange` / `charPosToIndex`(Task 2)
- Produces(`MmlEditor.svelte`):
  - props: `value: string`(`$bindable`)、`errorPosition?: number | null`(Rust の文字位置。`null` で下線なし)、`ariaLabel?: string`(既定 `"MML"`)、`describedBy?: string`、`invalid?: boolean`
  - `export function focusAt(pos: number): void` — Rust の文字位置にカーソルを置き、見える位置までスクロールしてフォーカスする
  - ホスト要素に `data-testid="mml-input"`(`EditorView.findFromDOM(host)` で `EditorView` が取れる)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/settings/MmlEditor.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { EditorView } from "@codemirror/view";
import MmlEditor from "./MmlEditor.svelte";

afterEach(() => cleanup());

function viewOf(el: HTMLElement): EditorView {
  const view = EditorView.findFromDOM(el);
  if (!view) throw new Error("EditorView が見つからない");
  return view;
}

describe("MmlEditor", () => {
  it("value が文書に入る", () => {
    const { getByTestId } = render(MmlEditor, { value: "t120 o5 c4" });
    expect(viewOf(getByTestId("mml-input")).state.doc.toString()).toBe("t120 o5 c4");
  });

  it("外から value を変えると、文書も変わる", async () => {
    const { getByTestId, rerender } = render(MmlEditor, { value: "c4" });
    await rerender({ value: "d8 e8" });
    expect(viewOf(getByTestId("mml-input")).state.doc.toString()).toBe("d8 e8");
  });

  it("色分けのクラスが付く", () => {
    const { container } = render(MmlEditor, { value: "t120 c4" });
    expect(container.querySelector(".mml-command")?.textContent).toBe("t");
  });

  it("errorPosition の位置に下線が付き、null にすると消える", async () => {
    const { container, rerender } = render(MmlEditor, { value: "c4 x", errorPosition: 3 });
    expect(container.querySelector(".mml-error")?.textContent).toBe("x");
    await rerender({ value: "c4 x", errorPosition: null });
    expect(container.querySelector(".mml-error")).toBeNull();
  });

  it("手前にサロゲートペアがあっても、Rust の文字位置で正しい文字に下線が付く", () => {
    const { container } = render(MmlEditor, { value: "😀cx", errorPosition: 2 });
    expect(container.querySelector(".mml-error")?.textContent).toBe("x");
  });

  it("errorPosition が文書の長さ以上でも例外を出さず、最後の 1 文字に付く", () => {
    const { container } = render(MmlEditor, { value: "c4", errorPosition: 5000 });
    expect(container.querySelector(".mml-error")?.textContent).toBe("4");
  });

  it("空の文書では、下線を付けない", () => {
    const { container } = render(MmlEditor, { value: "", errorPosition: 0 });
    expect(container.querySelector(".mml-error")).toBeNull();
  });

  it("focusAt は Rust の文字位置にカーソルを置く", () => {
    const { component, getByTestId } = render(MmlEditor, { value: "😀cx" });
    component.focusAt(2);
    expect(viewOf(getByTestId("mml-input")).state.selection.main.head).toBe(3);
  });

  it("focusAt が文書の長さ以上なら、末尾に置く", () => {
    const { component, getByTestId } = render(MmlEditor, { value: "c4" });
    component.focusAt(5000);
    expect(viewOf(getByTestId("mml-input")).state.selection.main.head).toBe(2);
  });

  it("a11y: 名前・複数行・ソフトキーボードの自動修正の抑止が付く", () => {
    const { container } = render(MmlEditor, { value: "c4", describedBy: "st us" });
    const content = container.querySelector(".cm-content")!;
    expect(content.getAttribute("role")).toBe("textbox");
    expect(content.getAttribute("aria-multiline")).toBe("true");
    expect(content.getAttribute("aria-label")).toBe("MML");
    expect(content.getAttribute("aria-describedby")).toBe("st us");
    expect(content.getAttribute("aria-invalid")).toBeNull();
    expect(content.getAttribute("autocapitalize")).toBe("off");
    expect(content.getAttribute("autocorrect")).toBe("off");
    expect(content.getAttribute("spellcheck")).toBe("false");
  });

  it("a11y: invalid のとき aria-invalid が付き、戻すと外れる", async () => {
    const { container, rerender } = render(MmlEditor, { value: "c4 x", invalid: true });
    const content = container.querySelector(".cm-content")!;
    expect(content.getAttribute("aria-invalid")).toBe("true");
    await rerender({ value: "c4 x", invalid: false });
    expect(content.getAttribute("aria-invalid")).toBeNull();
  });

  it("空のとき、プレースホルダーを出す", () => {
    const { container } = render(MmlEditor, { value: "" });
    expect(container.querySelector(".cm-placeholder")?.textContent).toBe("t120 o5 l8 cdefgab>c");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/MmlEditor.test.ts`
Expected: FAIL(`Failed to resolve import "./MmlEditor.svelte"`)

- [ ] **Step 3: 実装する**

`frontend/src/ui/settings/MmlEditor.svelte`:

```svelte
<script lang="ts">
  // MML 入力欄(Issue #500)。CodeMirror 6 を包み、色分け・エラー下線・カーソル移動を持つ。
  // 構文の検証はしない(親が parseMml の結果を errorPosition で渡す)。
  import { onMount } from "svelte";
  import { Compartment, EditorState } from "@codemirror/state";
  import { EditorView, keymap, placeholder } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
  import { charPosToIndex, errorRange } from "../../lib/mmlPosition";
  import { errorField, mmlHighlight, mmlTheme, setErrorRange } from "./mmlEditorExtensions";

  let {
    value = $bindable(""),
    errorPosition = null,
    ariaLabel = "MML",
    describedBy,
    invalid = false,
  }: {
    value: string;
    /// parse_mml が返した、エラーの文字位置(Rust の添字)。null なら下線なし
    errorPosition?: number | null;
    ariaLabel?: string;
    describedBy?: string;
    invalid?: boolean;
  } = $props();

  let host = $state<HTMLDivElement>();
  let view = $state.raw<EditorView>();
  const a11y = new Compartment();

  function a11yExtension() {
    return EditorView.contentAttributes.of({
      role: "textbox",
      "aria-multiline": "true",
      "aria-label": ariaLabel,
      autocapitalize: "off",
      autocorrect: "off",
      spellcheck: "false",
      ...(describedBy ? { "aria-describedby": describedBy } : {}),
      ...(invalid ? { "aria-invalid": "true" } : {}),
    });
  }

  onMount(() => {
    if (!host) return;
    const v = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          placeholder("t120 o5 l8 cdefgab>c"),
          mmlHighlight,
          errorField,
          mmlTheme,
          a11y.of(a11yExtension()),
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) return;
            const text = update.state.doc.toString();
            if (text !== value) value = text;
          }),
        ],
      }),
    });
    view = v;
    return () => v.destroy();
  });

  // 外から value が変わったら、文書に反映する(エディタ側の変更で value が変わったときは何もしない)
  $effect(() => {
    const v = value;
    if (view && v !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
    }
  });

  $effect(() => {
    const pos = errorPosition;
    if (!view) return;
    const range = pos === null ? null : errorRange(view.state.doc.toString(), pos);
    view.dispatch({ effects: setErrorRange.of(range) });
  });

  $effect(() => {
    if (!view) return;
    view.dispatch({ effects: a11y.reconfigure(a11yExtension()) });
  });

  /** Rust の文字位置にカーソルを置いて、見える位置までスクロールし、フォーカスする */
  export function focusAt(pos: number): void {
    if (!view) return;
    const anchor = charPosToIndex(view.state.doc.toString(), pos);
    view.dispatch({ selection: { anchor }, effects: EditorView.scrollIntoView(anchor, { y: "center" }) });
    view.focus();
  }
</script>

<div
  bind:this={host}
  class="cursor-text rounded-lg border border-border bg-muted text-foreground focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50"
  data-testid="mml-input"
></div>
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/MmlEditor.test.ts`
Expected: PASS(全件)。`rerender` 後の反映が非同期で落ちる場合は、`await vi.waitFor(...)` で待つ形にする(`$effect` は描画の後に走る)。

- [ ] **Step 5: 型検査を通す**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check`
Expected: エラー 0(`component.focusAt` の型が合わないときは、テストで `component as unknown as { focusAt(pos: number): void }` にする)

- [ ] **Step 6: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/settings/MmlEditor.svelte frontend/src/ui/settings/MmlEditor.test.ts
git commit -m "feat: CodeMirrorを使うMML入力欄のコンポーネントを追加(Issue #500)"
```

---

### Task 6: SoundChoicePicker への組み込み

**Files:**
- Modify: `frontend/src/ui/settings/SoundChoicePicker.svelte`
- Modify: `frontend/src/ui/settings/SoundChoicePicker.test.ts`(`<textarea>` 前提を書き換え、新しい挙動のテストを足す)

**Interfaces:**
- Consumes: `MmlEditor`(Task 5。動的 import)、`computeUsage` / `formatUsage` / `ParsedUsage`(Task 1)
- Produces: 変更なし(props `value` / `allowInherit` / `onvalidity` は維持)。新しい `data-testid`: `mml-error-jump`(エラー位置へ移動するボタン)、`mml-usage`(使用量の行)、`mml-usage-chars` / `mml-usage-notes` / `mml-usage-tracks` / `mml-usage-sec`(各項目。`data-level` に `normal` / `near` / `over`)

- [ ] **Step 1: テストを書き換える**

`frontend/src/ui/settings/SoundChoicePicker.test.ts` を、次の内容で置き換える(既存の挙動のテストは `<textarea>` を CodeMirror に合わせて残し、新しい挙動を足す)。

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { EditorView } from "@codemirror/view";

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
const { TIMBRE_LIST } = await import("../../lib/mmlTimbres");

const okScore = { notes: [], totalSec: 2.5, trackCount: 1 };

/** MmlEditor は動的 import なので、現れるまで待ってから EditorView を取る */
async function viewOf(findByTestId: (id: string) => Promise<HTMLElement>): Promise<EditorView> {
  const view = EditorView.findFromDOM(await findByTestId("mml-input"));
  if (!view) throw new Error("EditorView が見つからない");
  return view;
}

function setDoc(view: EditorView, text: string) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

beforeEach(() => {
  vi.spyOn(commands, "parseMml").mockResolvedValue({ status: "parsed", score: okScore });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SoundChoicePicker の MML モード", () => {
  it("保存済みの mml: を開くと MML モードで、入力欄に本文が入る", async () => {
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:t120 o5 c4" });
    expect((await viewOf(findByTestId)).state.doc.toString()).toBe("t120 o5 c4");
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
    expect(getByTestId("mml-status").textContent).toContain("4 文字目: 不明な文字");
    expect(onvalidity).toHaveBeenLastCalledWith(true);
  });

  it("parse_mml の IPC が失敗したら、位置なしのメッセージを出し invalid=true のままにする(未処理の reject はテスト失敗になる)", async () => {
    vi.spyOn(commands, "parseMml").mockRejectedValue(new Error("ipc down"));
    const onvalidity = vi.fn();
    const { getByTestId, queryByTestId } = render(SoundChoicePicker, { value: "mml:c4", onvalidity });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("ipc down"));
    const text = getByTestId("mml-status").textContent ?? "";
    expect(text).toContain("構文を確認できませんでした");
    expect(text).not.toContain("文字目");
    expect(text).not.toContain("確認中");
    expect(queryByTestId("mml-error-jump")).toBeNull();
    expect(onvalidity).toHaveBeenLastCalledWith(true);
  });

  it("本文が空なら parse_mml を呼ばずに invalid=true にする", async () => {
    const onvalidity = vi.fn();
    render(SoundChoicePicker, { value: "mml:", onvalidity });
    await vi.waitFor(() => expect(onvalidity).toHaveBeenLastCalledWith(true));
    expect(commands.parseMml).not.toHaveBeenCalled();
  });

  it("入力すると 300ms 待って、最後のテキストだけをパースする", async () => {
    const { findByTestId } = render(SoundChoicePicker, { value: "mml:c" });
    await vi.waitFor(() => expect(commands.parseMml).toHaveBeenCalledTimes(1));
    const view = await viewOf(findByTestId);
    setDoc(view, "cd");
    setDoc(view, "cde");
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

  it("記法ヘルプに音色の一覧(番号と名前)が出る", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    const text = getByTestId("mml-timbres").textContent ?? "";
    expect(TIMBRE_LIST).toHaveLength(14);
    for (const p of TIMBRE_LIST) expect(text).toContain(`@${p.number} ${p.name}`);
  });
});

describe("SoundChoicePicker のエラー表示(Issue #500)", () => {
  const invalid = { status: "invalid", error: { position: 3, message: "不明な文字 `x` です" } } as const;

  it("エラー位置に下線が付き、aria-invalid になる", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue(invalid);
    const { container, findByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    await viewOf(findByTestId);
    await vi.waitFor(() => expect(container.querySelector(".mml-error")?.textContent).toBe("x"));
    expect(container.querySelector(".cm-content")?.getAttribute("aria-invalid")).toBe("true");
  });

  it("「N 文字目」を押すと、その位置にカーソルが移る", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue(invalid);
    const { findByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    const view = await viewOf(findByTestId);
    const jump = await findByTestId("mml-error-jump");
    expect(jump.textContent).toBe("4 文字目");
    await fireEvent.click(jump);
    expect(view.state.selection.main.head).toBe(3);
  });

  it("エラー表示のまま編集したら、次の解析結果を待たずに古い下線を消す", async () => {
    const parse = vi.spyOn(commands, "parseMml");
    parse.mockResolvedValueOnce(invalid);
    parse.mockReturnValue(new Promise<never>(() => {})); // 2 回目以降は返ってこない(解析中のまま)
    const { container, findByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    const view = await viewOf(findByTestId);
    await vi.waitFor(() => expect(container.querySelector(".mml-error")).not.toBeNull());
    setDoc(view, "c4 xy");
    await vi.waitFor(() => expect(container.querySelector(".mml-error")).toBeNull());
  });

  it("同じ画面に 2 つあっても、aria-describedby の id が衝突しない", async () => {
    const a = render(SoundChoicePicker, { value: "mml:c4" });
    const b = render(SoundChoicePicker, { value: "mml:d4" });
    await viewOf(a.findByTestId);
    await viewOf(b.findByTestId);
    const idsA = a.container.querySelector(".cm-content")!.getAttribute("aria-describedby")!.split(" ");
    const idsB = b.container.querySelector(".cm-content")!.getAttribute("aria-describedby")!.split(" ");
    expect(idsA).toHaveLength(2);
    for (const id of idsA) expect(idsB).not.toContain(id);
    for (const id of idsA) expect(a.container.querySelector(`#${CSS.escape(id)}`)).not.toBeNull();
  });
});

describe("SoundChoicePicker の使用量(Issue #500)", () => {
  it("文字数・音数・トラック数・再生時間を上限と並べて出し、9 割以上は near にする", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue({
      status: "parsed",
      score: { notes: new Array(2700).fill(null), totalSec: 12.34, trackCount: 2 } as never,
    });
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4;d4" });
    await vi.waitFor(() => expect(getByTestId("mml-usage-notes").textContent).toBe("音 2700/3000"));
    expect(getByTestId("mml-usage-chars").textContent).toBe("文字 5/5000");
    expect(getByTestId("mml-usage-tracks").textContent).toBe("トラック 2/4");
    expect(getByTestId("mml-usage-sec").textContent).toBe("再生 12.3/90 秒");
    expect(getByTestId("mml-usage-notes").getAttribute("data-level")).toBe("near");
    expect(getByTestId("mml-usage-chars").getAttribute("data-level")).toBe("normal");
  });

  it("構文エラーのときは、音数と再生時間を — にする(文字数とトラック数は出す)", async () => {
    vi.spyOn(commands, "parseMml").mockResolvedValue({
      status: "invalid",
      error: { position: 3, message: "不明な文字 `x` です" },
    });
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:c4 x" });
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("文字目"));
    expect(getByTestId("mml-usage-notes").textContent).toBe("音 —/3000");
    expect(getByTestId("mml-usage-sec").textContent).toBe("再生 —/90 秒");
    expect(getByTestId("mml-usage-chars").textContent).toBe("文字 4/5000");
  });

  it("5000 文字を超えたら chars が over になる", async () => {
    const { getByTestId } = render(SoundChoicePicker, { value: "mml:" + "c".repeat(5001) });
    expect(getByTestId("mml-usage-chars").getAttribute("data-level")).toBe("over");
  });

  it("解析中は前回の音数と再生時間を保つ(300ms ごとに — へ点滅させない)", async () => {
    const parse = vi.spyOn(commands, "parseMml");
    parse.mockResolvedValueOnce({
      status: "parsed",
      score: { notes: new Array(10).fill(null), totalSec: 3, trackCount: 1 } as never,
    });
    parse.mockReturnValue(new Promise<never>(() => {})); // 2 回目以降は解析中のまま
    const { findByTestId, getByTestId } = render(SoundChoicePicker, { value: "mml:c4" });
    await vi.waitFor(() => expect(getByTestId("mml-usage-notes").textContent).toBe("音 10/3000"));
    setDoc(await viewOf(findByTestId), "c4d4");
    await vi.waitFor(() => expect(getByTestId("mml-status").textContent).toContain("確認中"));
    expect(getByTestId("mml-usage-notes").textContent).toBe("音 10/3000");
    expect(getByTestId("mml-usage-chars").textContent).toBe("文字 4/5000");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/SoundChoicePicker.test.ts`
Expected: FAIL(`mml-input` が `EditorView` ではない、`mml-usage-*` が無い、など)

- [ ] **Step 3: SoundChoicePicker.svelte を変更する(script)**

(a) import を足す。

old:

```ts
  import { TIMBRE_LIST } from "../../lib/mmlTimbres";
```

new:

```ts
  import { TIMBRE_LIST } from "../../lib/mmlTimbres";
  import { computeUsage, formatUsage, type ParsedUsage } from "../../lib/mmlUsage";
```

(b) `error` の状態に、解析した時点の文字列を持たせる。

old:

```ts
    | { kind: "error"; position: number; message: string }
```

new:

```ts
    // text は、そのエラーを返した解析の入力。編集後の古い位置に下線を引かないために持つ
    | { kind: "error"; position: number; message: string; text: string }
```

(c) 状態を足す。

old:

```ts
  let status = $state<MmlStatus>({ kind: "idle" });
```

new:

```ts
  let status = $state<MmlStatus>({ kind: "idle" });
  // 直近に構文が通ったときの音数と再生時間。解析中は点滅を避けて、これを出し続ける
  let lastOk = $state<ParsedUsage | null>(null);
  let editor = $state<{ focusAt(pos: number): void }>();
  // 同じ画面に複数あっても id が衝突しないように
  const uid = $props.id();
```

(d) 解析の `$effect` を、`lastOk` と `text` を更新する形にする。

old:

```ts
    if (!text.trim()) {
      status = { kind: "error", position: 0, message: "MML を入力してください" };
      return;
    }
```

new:

```ts
    if (!text.trim()) {
      lastOk = null;
      status = { kind: "error", position: 0, message: "MML を入力してください", text };
      return;
    }
```

old:

```ts
          const r = await commands.parseMml(text);
          if (text !== mmlText) return;
          status =
            r.status === "parsed"
              ? { kind: "ok", totalSec: r.score.totalSec ?? 0 }
              : { kind: "error", position: r.error.position, message: r.error.message };
        } catch (e) {
          if (text !== mmlText) return;
          status = { kind: "failed", message: String(e) };
        }
```

new:

```ts
          const r = await commands.parseMml(text);
          if (text !== mmlText) return;
          if (r.status === "parsed") {
            const totalSec = r.score.totalSec ?? 0;
            lastOk = { noteCount: r.score.notes.length, totalSec };
            status = { kind: "ok", totalSec };
          } else {
            lastOk = null;
            status = { kind: "error", position: r.error.position, message: r.error.message, text };
          }
        } catch (e) {
          if (text !== mmlText) return;
          lastOk = null;
          status = { kind: "failed", message: String(e) };
        }
```

(e) 派生値と移動の関数を足す(`onMount(() => () => onvalidity?.(false));` の直後)。

old:

```ts
  onMount(() => () => onvalidity?.(false));
```

new:

```ts
  onMount(() => () => onvalidity?.(false));

  // 編集したら status は pending に変わるので、位置のずれた古い下線は残らない。text の一致は念のため
  const errorPosition = $derived(status.kind === "error" && status.text === mmlText ? status.position : null);
  const usage = $derived(computeUsage(mmlText, status.kind === "ok" || status.kind === "pending" ? lastOk : null));

  function jumpToError() {
    if (errorPosition !== null) editor?.focusAt(errorPosition);
  }
```

- [ ] **Step 4: SoundChoicePicker.svelte を変更する(markup)**

old(`<textarea>` から状態行の `</p>` まで):

```svelte
  <textarea
    class="resize-y rounded-lg border border-border bg-muted px-2.5 py-2 font-mono text-xs text-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
    rows="3"
    spellcheck="false"
    placeholder="t120 o5 l8 cdefgab>c"
    bind:value={mmlText}
    data-testid="mml-input"
  ></textarea>
  <p class="m-0 text-xs {status.kind === 'error' || status.kind === 'failed' ? 'text-destructive' : 'text-muted-foreground'}" data-testid="mml-status">
    {#if status.kind === "ok"}再生時間 {status.totalSec.toFixed(1)} 秒
    {:else if status.kind === "error"}{status.position + 1} 文字目: {status.message}
    {:else if status.kind === "failed"}構文を確認できませんでした: {status.message}
    {:else}確認中…{/if}
  </p>
```

new:

```svelte
  {#await import("./MmlEditor.svelte")}
    <div class="h-20 rounded-lg border border-border bg-muted"></div>
  {:then { default: MmlEditor }}
    <MmlEditor
      bind:this={editor}
      bind:value={mmlText}
      {errorPosition}
      describedBy="{uid}-status {uid}-usage"
      invalid={status.kind === "error" || status.kind === "failed"}
    />
  {/await}
  <p id="{uid}-status" class="m-0 text-xs {status.kind === 'error' || status.kind === 'failed' ? 'text-destructive' : 'text-muted-foreground'}" data-testid="mml-status">
    {#if status.kind === "ok"}再生時間 {status.totalSec.toFixed(1)} 秒
    {:else if status.kind === "error"}<button
        type="button"
        class="cursor-pointer rounded-md border-0 bg-transparent p-0 text-xs text-destructive underline focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
        onclick={jumpToError}
        data-testid="mml-error-jump">{status.position + 1} 文字目</button>: {status.message}
    {:else if status.kind === "failed"}構文を確認できませんでした: {status.message}
    {:else}確認中…{/if}
  </p>
  <p id="{uid}-usage" class="m-0 flex flex-wrap gap-x-3 text-xs text-muted-foreground" data-testid="mml-usage">
    {#each usage as u (u.key)}
      <span
        data-testid="mml-usage-{u.key}"
        data-level={u.level}
        class={{ "font-bold": u.level !== "normal", "text-[var(--warning)]": u.level === "near", "text-destructive": u.level === "over" }}
      >{formatUsage(u)}</span>
    {/each}
  </p>
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/SoundChoicePicker.test.ts`
Expected: PASS(全件)

- [ ] **Step 6: 他のテストと型検査を通す**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check && pnpm test`
Expected: エラー 0、全テスト PASS(`NotifySection.test.ts` と `AddColumnModal.test.ts` も含む。MML の入力欄に触れて落ちるものがあれば、`<textarea>` 前提を `EditorView` に合わせて直す)

- [ ] **Step 7: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/settings/SoundChoicePicker.svelte frontend/src/ui/settings/SoundChoicePicker.test.ts
git commit -m "feat: MML入力欄をエディタにして、エラー下線と使用量を出す(Issue #500)"
```

---

### Task 7: ガイド、バンドル、実画面の確認

**Files:**
- Modify: `docs/guide/user-guide.md`(「MML で通知音を作る」の説明)
- Modify: `docs/superpowers/specs/2026-10-17-issue-500-mml-editor-design.md`(実装後の確認結果を追記)

**Interfaces:** なし(ドキュメントと確認)

- [ ] **Step 1: ユーザーガイドを直す**

`docs/guide/user-guide.md` の、次の 1 文を置き換える。

old:

```
入力欄の下に再生時間が出れば構文は正しく、「試聴」で鳴らせます。書き方が誤っているときは、何文字目が原因かが表示され、直すまで保存できません。
```

new:

```
音符・休符・コマンド・音色・数字は色分けされます。入力欄の下に再生時間が出れば構文は正しく、「試聴」で鳴らせます。書き方が誤っているときは、原因の文字に波線が引かれ、「N 文字目」を押すとその位置にカーソルが移ります。直すまで保存できません。その下に、文字数・音の数・トラック数・再生時間が上限と並んで出ます(上限の 9 割を超えると太字と色で知らせ、超えると赤くなります)。音の数と再生時間は、構文が通っているときだけ出ます。
```

- [ ] **Step 2: 全体の検証を走らせる**

Run:
```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check && pnpm test
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib mml
```
Expected: エラー 0、フロント全テスト PASS、`mml` のテスト PASS

- [ ] **Step 3: バンドルの増分を測る**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm exec vite build 2>&1 | tail -30`
Expected: `MmlEditor` の chunk が独立して出る(動的 import の確認)。その chunk の大きさ(gzip)と、メインの chunk が増えていないこと(増えた場合は、どの import が引き込んだか調べる)を記録する。

- [ ] **Step 4: 実画面を確認する(自動テストでは済ませない)**

手順は `e2e/README.md` と、リポジトリの `CLAUDE.md`(Xvfb と隔離した D-Bus、隔離インスタンスの XDG パスは短く、`dbus-run-session`、プロセスの掃除は正確な PID のみ)に従う。ユーザーの実画面(Wayland)に漏らさないため、`DISPLAY` だけでなく `WAYLAND_DISPLAY` も unset する。起動した `cargo tauri dev` は、終わる前に自分で止める。次を確かめ、結果を Step 6 に書く。

- 設定→通知と、タブ追加(`AddColumnModal`)の両方で、MML モードの入力欄が表示される。
- 入力した文字が色分けされる。ライト・ダークの両方のテーマで読める。
- 構文エラーの文字に波線が付く位置が、正しい文字である。「N 文字目」を押すと、そこにカーソルが移る。
- 日本語 IME を有効にしても、入力が壊れない(MML は ASCII だが、全角を打ってエラーになること)。
- Escape と Enter が、モーダルの操作(閉じる・確定)と衝突しない。
- 5000 文字を超えるとき、使用量の文字数が赤くなり、エラーが出る。
- スクリーンリーダー向けの属性が付いている(`document.querySelector('.cm-content')` の属性)。
- Tab キーが入力欄に取り込まれず、フォーカスが次の要素へ移る(Shift+Tab で戻れる)。

- [ ] **Step 5: Android のエミュレーターで確認する**

`adb shell input tap` の本物のタップで入力欄を押す(CDP の `.click()` はユーザー操作扱いにならず、キーボードが出ない)。次を確かめ、結果を Step 6 に書く。

- ソフトキーボードが出て、音符が大文字にならず、自動修正が入らない。
- `--keyboard-inset` が効き、入力中の行がキーボードに隠れない(入力欄が下にあるモーダルでも)。
- 長い MML で、外側のモーダルだけがスクロールする(二重スクロールにならない)。
- 「N 文字目」のボタンを押して、カーソルが移りスクロールする。

入力が壊れる、またはキャレットがずれる場合は、そこで止める。spec の「リスクと撤退条件」のとおり、自前の `<textarea>` 重ね描きの再検討を提案し、spec を更新する(この計画の続きは実行しない)。

- [ ] **Step 6: 確認結果を spec に追記する**

`docs/superpowers/specs/2026-10-17-issue-500-mml-editor-design.md` の末尾に、`## 実装後の確認結果` を足し、Step 3〜5 で測った・確かめた内容(バンドルの増分、各環境で確認した項目と結果、確認できなかった項目)を、事実だけ書く。確認していないことは「未確認」と書く。

- [ ] **Step 7: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add docs/guide/user-guide.md docs/superpowers/specs/2026-10-17-issue-500-mml-editor-design.md
git commit -m "docs: MML入力欄のエディタ化をガイドに反映し、確認結果を追記(Issue #500)"
```

- [ ] **Step 8: push と PR は、ユーザーに確認してから行う**

push は外に出る操作なので、ユーザーの指示を待つ。指示があったら、`gh pr create` で `.github/pull_request_template.md` の構成に沿った本文を書く。PR 本文は **`Refs #500`** とし、`Fixes` / `Closes` は使わない(音色・サンプルの挿入、生成中の表示と中断、入力補助が残っていて、#500 はクローズしない)。末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。マージは `gh pr merge --merge`(squash しない)。push 後に CI を待たない(ユーザーが見る)。
