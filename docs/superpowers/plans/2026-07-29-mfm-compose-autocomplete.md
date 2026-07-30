# MFM補完(ComposeBar) Phase1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ComposeBar.svelte` の本文textareaで、`:emoji:`(カスタム+Unicode)・`$[fnName ...]`・`$[fnName.argName ...]`・`$[border.style=...]` をキーボード操作可能なポップアップで補完できるようにする。

**Architecture:** トリガー検出・候補マッチング・置換計算はすべて `frontend/src/lib/mfmCompletion.ts` の純粋関数(DOM非依存)に実装し、Vitestで網羅的にテストする。UIは新規 `CompletionPopover.svelte`(候補描画+クリック確定のみを担う無状態コンポーネント)と、状態・キー操作ルーティング・textarea内キャレット座標計算を持つ `ComposeBar.svelte` 側の配線に分離する。

**Tech Stack:** Svelte 5(runes)、TypeScript、Vitest、`@testing-library/svelte`。新規外部依存の追加なし(既存の `@misskey-dev/emoji-data` 由来データ・既存 `lib/mfm.ts` を再利用)。

## Global Constraints

- 対象は `ComposeBar.svelte` の本文textareaのみ。CW欄・他のtextarea(TQLフィルタ入力等)は対象外。
- メンション(`@user`)・ハッシュタグ(`#tag`)補完はPhase1のスコープ外(検索APIが未実装のため)。
- 引数値の自由入力項目(色・時間・数値)は補完しない。列挙値補完は `border.style` のみ。
- 候補は前方一致(starts-with)・最大10件。絵文字はカスタム優先→Unicode、名前順。
- 確定操作: `↑`/`↓` で選択移動、`Tab`/`Enter` で確定、`Escape` で閉じる。`Ctrl+Enter`(投稿)は常に最優先。
- 新規npm依存パッケージを追加しない。
- 設計書: `docs/superpowers/specs/2026-07-29-mfm-compose-autocomplete-design.md`(このプランの元設計)。

---

## Task 1: `lib/mfm.ts` — 引数スキーマ (`FN_ARGS`) の追加とエクスポート

**Files:**
- Modify: `frontend/src/lib/mfm.ts`
- Test: `frontend/src/lib/mfm.test.ts`(既存ファイルに追記)

**Interfaces:**
- Produces: `export const KNOWN_FN: Set<string>`(既存の非export定数をexportに変更)、`export interface MfmArgSpec { name: string; hasValue: boolean; enum?: string[] }`、`export const FN_ARGS: Record<string, MfmArgSpec[]>`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mfm.test.ts` の末尾に追記:

```ts
import { FN_ARGS, KNOWN_FN } from "./mfm";

describe("FN_ARGS", () => {
  it("has an entry for every known fn name", () => {
    for (const name of KNOWN_FN) {
      expect(FN_ARGS[name]).toBeDefined();
    }
  });

  it("marks tada's speed/delay as value args", () => {
    expect(FN_ARGS.tada).toEqual([
      { name: "speed", hasValue: true },
      { name: "delay", hasValue: true },
    ]);
  });

  it("marks flip's h/v as flag args (no value)", () => {
    expect(FN_ARGS.flip).toEqual([
      { name: "h", hasValue: false },
      { name: "v", hasValue: false },
    ]);
  });

  it("gives border.style a closed enum matching the CSS border-style keywords mfmFn accepts", () => {
    const style = FN_ARGS.border.find((a) => a.name === "style");
    expect(style?.enum).toEqual([
      "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset",
    ]);
  });

  it("does not give border.color an enum (free-form hex input)", () => {
    const color = FN_ARGS.border.find((a) => a.name === "color");
    expect(color?.enum).toBeUndefined();
  });

  it("gives x2/x3/x4/blur an empty arg list", () => {
    expect(FN_ARGS.x2).toEqual([]);
    expect(FN_ARGS.x3).toEqual([]);
    expect(FN_ARGS.x4).toEqual([]);
    expect(FN_ARGS.blur).toEqual([]);
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfm.test.ts`
Expected: FAIL — `FN_ARGS`/`KNOWN_FN` が存在しない、または不一致

- [ ] **Step 3: 実装する**

`frontend/src/lib/mfm.ts` の `const KNOWN_FN = new Set([...])` を `export const KNOWN_FN = new Set([...])` に変更(中身は変えない)。

同ファイルの `KNOWN_FN` 定義の直後、`isKnownFn` の前後どちらでもよいが `BORDER_STYLES` 定義より後ろに追記:

```ts
export interface MfmArgSpec {
  name: string;
  hasValue: boolean;
  enum?: string[];
}

export const FN_ARGS: Record<string, MfmArgSpec[]> = {
  tada: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  jelly: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  twitch: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  shake: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  jump: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  bounce: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  rainbow: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  spin: [
    { name: "speed", hasValue: true },
    { name: "delay", hasValue: true },
    { name: "x", hasValue: false },
    { name: "y", hasValue: false },
    { name: "left", hasValue: false },
    { name: "alternate", hasValue: false },
  ],
  flip: [{ name: "h", hasValue: false }, { name: "v", hasValue: false }],
  x2: [],
  x3: [],
  x4: [],
  blur: [],
  font: [
    { name: "serif", hasValue: false },
    { name: "monospace", hasValue: false },
    { name: "cursive", hasValue: false },
    { name: "fantasy", hasValue: false },
    { name: "emoji", hasValue: false },
    { name: "math", hasValue: false },
  ],
  rotate: [{ name: "deg", hasValue: true }],
  position: [{ name: "x", hasValue: true }, { name: "y", hasValue: true }],
  scale: [{ name: "x", hasValue: true }, { name: "y", hasValue: true }],
  fg: [{ name: "color", hasValue: true }],
  bg: [{ name: "color", hasValue: true }],
  border: [
    { name: "color", hasValue: true },
    {
      name: "style",
      hasValue: true,
      enum: ["hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset"],
    },
    { name: "width", hasValue: true },
    { name: "radius", hasValue: true },
    { name: "noclip", hasValue: false },
  ],
};
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfm.test.ts`
Expected: PASS(既存テストも含めて全件通過)

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mfm.ts frontend/src/lib/mfm.test.ts
git commit -m "feat: MFM関数の引数スキーマ(FN_ARGS)を追加"
```

---

## Task 2: `lib/mfmCompletion.ts` — トリガー検出・マッチング・置換計算

**Files:**
- Create: `frontend/src/lib/mfmCompletion.ts`
- Test: `frontend/src/lib/mfmCompletion.test.ts`

**Interfaces:**
- Consumes: `KNOWN_FN`, `FN_ARGS`, `MfmArgSpec` from `./mfm`(Task 1)。`UNICODE_EMOJIS` from `./unicodeEmojiList`(`{char: string; name: string; category: number}[]`)。`EmojiDef` type from `../bindings/tauri.gen`(`{name, host, url, category, aliases}`)。
- Produces:
  - `export type Trigger = { kind: "emoji"; query: string; start: number; end: number } | { kind: "fnName"; query: string; start: number; end: number } | { kind: "argName"; fnName: string; query: string; start: number; end: number } | { kind: "argValue"; fnName: string; argName: string; query: string; start: number; end: number }`
  - `export function detectTrigger(text: string, cursor: number): Trigger | null`
  - `export interface EmojiMatch { key: string; kind: "custom" | "unicode"; name: string; url?: string; char?: string }`
  - `export function matchEmojis(query: string, customEmojis: EmojiDef[]): EmojiMatch[]`
  - `export function matchFnNames(query: string): string[]`
  - `export function matchArgNames(fnName: string, query: string): MfmArgSpec[]`
  - `export function matchArgValues(fnName: string, argName: string, query: string): string[]`
  - `export interface CompletionThumbnail { type: "custom" | "unicode"; url?: string; char?: string }`
  - `export interface CompletionItem { key: string; label: string; insertText: string; thumbnail?: CompletionThumbnail }`
  - `export function buildCompletionItems(trigger: Trigger, customEmojis: EmojiDef[]): CompletionItem[]`
  - `export function applyCompletion(text: string, trigger: Trigger, item: CompletionItem): { text: string; cursor: number }`

### Cycle A: `detectTrigger`

- [ ] **Step 1: 失敗するテストを書く**

Create `frontend/src/lib/mfmCompletion.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { detectTrigger } from "./mfmCompletion";

describe("detectTrigger", () => {
  it("returns null when there is no trigger character", () => {
    expect(detectTrigger("hello world", 11)).toBeNull();
  });

  it("detects an emoji trigger at the start of the text", () => {
    expect(detectTrigger(":sm", 3)).toEqual({ kind: "emoji", query: "sm", start: 0, end: 3 });
  });

  it("detects an emoji trigger after whitespace", () => {
    expect(detectTrigger("hello :sm", 9)).toEqual({ kind: "emoji", query: "sm", start: 6, end: 9 });
  });

  it("does not treat a colon glued to a word as an emoji trigger", () => {
    // "http:" のように直前が英数字の ":" は、その時点ではトリガーにしない
    // (直前が行頭/空白/開き括弧類の ":" だけをトリガーとみなす)
    expect(detectTrigger("http:", 5)).toBeNull();
  });

  it("detects an fn name trigger right after $[", () => {
    expect(detectTrigger("$[ta", 4)).toEqual({ kind: "fnName", query: "ta", start: 2, end: 4 });
  });

  it("detects an fn name trigger with an empty query", () => {
    expect(detectTrigger("$[", 2)).toEqual({ kind: "fnName", query: "", start: 2, end: 2 });
  });

  it("does not detect an fn trigger once a $[...] has already been closed", () => {
    expect(detectTrigger("$[tada hi] world:", 17)).toBeNull();
  });

  it("stops fn-name detection once whitespace has been typed, falling back to no trigger", () => {
    expect(detectTrigger("$[tada hi", 9)).toBeNull();
  });

  it("still detects an emoji trigger inside an fn's content (after whitespace)", () => {
    expect(detectTrigger("$[tada hi :sm", 13)).toEqual({
      kind: "emoji", query: "sm", start: 10, end: 13,
    });
  });

  it("detects an arg-name trigger right after the dot", () => {
    expect(detectTrigger("$[tada.spee", 11)).toEqual({
      kind: "argName", fnName: "tada", query: "spee", start: 7, end: 11,
    });
  });

  it("detects an arg-name trigger for the second argument after a comma", () => {
    expect(detectTrigger("$[tada.speed=1s,de", 18)).toEqual({
      kind: "argName", fnName: "tada", query: "de", start: 16, end: 18,
    });
  });

  it("detects an arg-value trigger for border.style", () => {
    expect(detectTrigger("$[border.style=so", 18)).toEqual({
      kind: "argValue", fnName: "border", argName: "style", query: "so", start: 15, end: 18,
    });
  });

  it("does not detect an arg-value trigger for an arg without an enum (e.g. border.color)", () => {
    expect(detectTrigger("$[border.color=f", 17)).toBeNull();
  });

  it("does not detect an arg-value trigger for an unknown fn name", () => {
    expect(detectTrigger("$[nonexistent.style=so", 23)).toBeNull();
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: FAIL — `./mfmCompletion` が存在しない

- [ ] **Step 3: 実装する**

Create `frontend/src/lib/mfmCompletion.ts`:

```ts
import type { EmojiDef } from "../bindings/tauri.gen";
import { FN_ARGS, KNOWN_FN, type MfmArgSpec } from "./mfm";
import { UNICODE_EMOJIS } from "./unicodeEmojiList";

export type Trigger =
  | { kind: "emoji"; query: string; start: number; end: number }
  | { kind: "fnName"; query: string; start: number; end: number }
  | { kind: "argName"; fnName: string; query: string; start: number; end: number }
  | { kind: "argValue"; fnName: string; argName: string; query: string; start: number; end: number };

const IDENT = /^[a-zA-Z0-9_]*$/;
// 直前が行頭または空白/開き括弧類のときだけ ":" を絵文字トリガーの開始とみなす
// (英数字に直接くっついた ":"(例: "http:")を誤検出しないため)。
const EMOJI_TRIGGER = /(?:^|[\s([{"'>])(:[a-zA-Z0-9_+-]*)$/;

function detectFnTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const openIdx = head.lastIndexOf("$[");
  if (openIdx === -1) return null;
  const seg = head.slice(openIdx + 2);
  if (seg.includes("]")) return null; // 直近の $[ はすでに閉じている
  if (/\s/.test(seg)) return null; // 本文コンテンツに入っている(呼び出し側が絵文字トリガーを別途評価する)

  const dotIdx = seg.indexOf(".");
  if (dotIdx === -1) {
    if (!IDENT.test(seg)) return null;
    return { kind: "fnName", query: seg, start: openIdx + 2, end: cursor };
  }

  const fnName = seg.slice(0, dotIdx);
  const argsStr = seg.slice(dotIdx + 1);
  const lastComma = argsStr.lastIndexOf(",");
  const argSeg = lastComma === -1 ? argsStr : argsStr.slice(lastComma + 1);
  const argSegStart = openIdx + 2 + dotIdx + 1 + (lastComma === -1 ? 0 : lastComma + 1);

  const eqIdx = argSeg.indexOf("=");
  if (eqIdx === -1) {
    if (!IDENT.test(argSeg)) return null;
    return { kind: "argName", fnName, query: argSeg, start: argSegStart, end: cursor };
  }

  const argName = argSeg.slice(0, eqIdx);
  const valueQuery = argSeg.slice(eqIdx + 1);
  if (!IDENT.test(valueQuery)) return null;
  const spec = FN_ARGS[fnName]?.find((a) => a.name === argName);
  if (!spec?.enum) return null; // 列挙値を持つ引数のみ値補完する
  const valueStart = argSegStart + eqIdx + 1;
  return { kind: "argValue", fnName, argName, query: valueQuery, start: valueStart, end: cursor };
}

function detectEmojiTrigger(text: string, cursor: number): Trigger | null {
  const head = text.slice(0, cursor);
  const m = head.match(EMOJI_TRIGGER);
  if (!m) return null;
  const matched = m[1]; // ":query" (先頭の境界文字は含まない)
  return { kind: "emoji", query: matched.slice(1), start: cursor - matched.length, end: cursor };
}

export function detectTrigger(text: string, cursor: number): Trigger | null {
  return detectFnTrigger(text, cursor) ?? detectEmojiTrigger(text, cursor);
}
```

`KNOWN_FN`/`MfmArgSpec` は現時点では未使用に見えるが、Cycle B で使用するため import はそのままにする(未使用エラーが出る場合はCycle Bまで一時的に `// eslint-disable` 等は不要 — TypeScriptのnoUnusedLocalsで落ちる場合のみCycle Bの内容を同時に書き進めて解消する)。

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: PASS(このCycleの13ケース全通過。`KNOWN_FN`/`MfmArgSpec` が未使用でビルドエラーになる場合は先にCycle Bまで実装してから再実行してよい)

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mfmCompletion.ts frontend/src/lib/mfmCompletion.test.ts
git commit -m "feat: MFM補完のトリガー検出(detectTrigger)を実装"
```

### Cycle B: マッチング関数

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mfmCompletion.test.ts` に追記:

```ts
import { matchArgNames, matchArgValues, matchEmojis, matchFnNames } from "./mfmCompletion";
import type { EmojiDef } from "../bindings/tauri.gen";

function emoji(name: string, aliases: string[] = []): EmojiDef {
  return { name, host: null, url: `https://example.com/${name}.png`, category: null, aliases };
}

describe("matchEmojis", () => {
  it("matches custom emoji by name prefix, case-insensitively", () => {
    const custom = [emoji("Smile_cat"), emoji("smoke"), emoji("wave")];
    const result = matchEmojis("sm", custom);
    expect(result.map((r) => r.name)).toEqual(["Smile_cat", "smoke"]);
    expect(result.every((r) => r.kind === "custom")).toBe(true);
  });

  it("matches custom emoji by alias prefix too", () => {
    const custom = [emoji("neko", ["cat_face"])];
    expect(matchEmojis("cat", custom).map((r) => r.name)).toEqual(["neko"]);
  });

  it("ranks custom emoji ahead of unicode emoji for the same query", () => {
    const custom = [emoji("smile")];
    const result = matchEmojis("smi", custom);
    expect(result[0]).toEqual({ key: "custom:smile", kind: "custom", name: "smile", url: custom[0].url });
  });

  it("falls back to unicode emoji shortcodes when no custom emoji matches", () => {
    const result = matchEmojis("grin", []);
    expect(result.length).toBeGreaterThan(0);
    expect(result.every((r) => r.kind === "unicode")).toBe(true);
    expect(result[0].char).toBeTruthy();
  });

  it("caps the total at 10 matches", () => {
    const custom = Array.from({ length: 20 }, (_, i) => emoji(`smile_${i}`));
    expect(matchEmojis("smile", custom)).toHaveLength(10);
  });

  it("returns everything up to the limit for an empty query", () => {
    const custom = [emoji("a"), emoji("b")];
    expect(matchEmojis("", custom).length).toBeGreaterThan(0);
  });
});

describe("matchFnNames", () => {
  it("matches known fn names by prefix, sorted", () => {
    expect(matchFnNames("s")).toEqual(["scale", "shake", "spin"]);
  });

  it("returns an empty array for no match", () => {
    expect(matchFnNames("zzz")).toEqual([]);
  });
});

describe("matchArgNames", () => {
  it("matches an fn's arg specs by name prefix", () => {
    expect(matchArgNames("border", "s")).toEqual([
      {
        name: "style", hasValue: true,
        enum: ["hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset"],
      },
    ]);
  });

  it("returns an empty array for an unknown fn", () => {
    expect(matchArgNames("nonexistent", "s")).toEqual([]);
  });
});

describe("matchArgValues", () => {
  it("matches border.style's enum by prefix", () => {
    expect(matchArgValues("border", "style", "d")).toEqual(["dotted", "dashed", "double"]);
  });

  it("returns an empty array for an arg with no enum", () => {
    expect(matchArgValues("border", "color", "f")).toEqual([]);
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: FAIL — `matchEmojis`/`matchFnNames`/`matchArgNames`/`matchArgValues` が存在しない

- [ ] **Step 3: 実装する**

`frontend/src/lib/mfmCompletion.ts` に追記(ファイル末尾):

```ts
const MAX_MATCHES = 10;

function startsWithCI(name: string, query: string): boolean {
  return name.toLowerCase().startsWith(query.toLowerCase());
}

export interface EmojiMatch {
  key: string;
  kind: "custom" | "unicode";
  name: string;
  url?: string;
  char?: string;
}

export function matchEmojis(query: string, customEmojis: EmojiDef[]): EmojiMatch[] {
  const custom: EmojiMatch[] = customEmojis
    .filter((e) => startsWithCI(e.name, query) || e.aliases.some((a) => startsWithCI(a, query)))
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((e) => ({ key: `custom:${e.name}`, kind: "custom", name: e.name, url: e.url }));

  const remaining = MAX_MATCHES - custom.length;
  const unicode: EmojiMatch[] =
    remaining > 0
      ? UNICODE_EMOJIS.filter((e) => startsWithCI(e.name, query))
          .sort((a, b) => a.name.localeCompare(b.name))
          .slice(0, remaining)
          .map((e) => ({ key: `unicode:${e.name}`, kind: "unicode", name: e.name, char: e.char }))
      : [];

  return [...custom.slice(0, MAX_MATCHES), ...unicode];
}

export function matchFnNames(query: string): string[] {
  return [...KNOWN_FN]
    .filter((name) => startsWithCI(name, query))
    .sort((a, b) => a.localeCompare(b))
    .slice(0, MAX_MATCHES);
}

export function matchArgNames(fnName: string, query: string): MfmArgSpec[] {
  const specs = FN_ARGS[fnName] ?? [];
  return specs.filter((s) => startsWithCI(s.name, query)).slice(0, MAX_MATCHES);
}

export function matchArgValues(fnName: string, argName: string, query: string): string[] {
  const spec = (FN_ARGS[fnName] ?? []).find((s) => s.name === argName);
  const values = spec?.enum ?? [];
  return values.filter((v) => startsWithCI(v, query)).slice(0, MAX_MATCHES);
}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: PASS(Cycle A + Cycle Bの全ケース)

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mfmCompletion.ts frontend/src/lib/mfmCompletion.test.ts
git commit -m "feat: MFM補完の候補マッチング(match*)を実装"
```

### Cycle C: 候補の組み立てと置換計算

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mfmCompletion.test.ts` に追記:

```ts
import { applyCompletion, buildCompletionItems, type CompletionItem } from "./mfmCompletion";

describe("buildCompletionItems", () => {
  it("builds emoji items with :name: insert text and a thumbnail", () => {
    const custom = [emoji("neko")];
    const trigger = { kind: "emoji", query: "ne", start: 0, end: 3 } as const;
    expect(buildCompletionItems(trigger, custom)).toEqual([
      { key: "custom:neko", label: "neko", insertText: ":neko:", thumbnail: { type: "custom", url: custom[0].url } },
    ]);
  });

  it("builds fnName items with the bare name as insert text", () => {
    const trigger = { kind: "fnName", query: "tad", start: 2, end: 5 } as const;
    expect(buildCompletionItems(trigger, [])).toEqual([
      { key: "tada", label: "tada", insertText: "tada" },
    ]);
  });

  it("builds argName items, appending '=' for value args but not for flags", () => {
    const trigger = { kind: "argName", fnName: "spin", query: "", start: 0, end: 0 } as const;
    const items = buildCompletionItems(trigger, []);
    expect(items.find((i) => i.key === "speed")).toEqual({ key: "speed", label: "speed=", insertText: "speed=" });
    expect(items.find((i) => i.key === "x")).toEqual({ key: "x", label: "x", insertText: "x" });
  });

  it("builds argValue items with the bare enum value as insert text", () => {
    const trigger = { kind: "argValue", fnName: "border", argName: "style", query: "so", start: 0, end: 2 } as const;
    expect(buildCompletionItems(trigger, [])).toEqual([
      { key: "solid", label: "solid", insertText: "solid" },
    ]);
  });
});

describe("applyCompletion", () => {
  it("splices the insert text into the trigger's range and places the cursor after it", () => {
    const item: CompletionItem = { key: "neko", label: "neko", insertText: ":neko:" };
    const trigger = { kind: "emoji", query: "ne", start: 6, end: 9 } as const;
    const result = applyCompletion("hello :ne", trigger, item);
    expect(result).toEqual({ text: "hello :neko:", cursor: 12 });
  });

  it("keeps text after the trigger end intact", () => {
    const item: CompletionItem = { key: "tada", label: "tada", insertText: "tada" };
    const trigger = { kind: "fnName", query: "ta", start: 2, end: 4 } as const;
    const result = applyCompletion("$[ta hi]", trigger, item);
    expect(result).toEqual({ text: "$[tada hi]", cursor: 6 });
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: FAIL — `buildCompletionItems`/`applyCompletion` が存在しない

- [ ] **Step 3: 実装する**

`frontend/src/lib/mfmCompletion.ts` に追記(ファイル末尾):

```ts
export interface CompletionThumbnail {
  type: "custom" | "unicode";
  url?: string;
  char?: string;
}

export interface CompletionItem {
  key: string;
  label: string;
  insertText: string;
  thumbnail?: CompletionThumbnail;
}

export function buildCompletionItems(trigger: Trigger, customEmojis: EmojiDef[]): CompletionItem[] {
  switch (trigger.kind) {
    case "emoji":
      return matchEmojis(trigger.query, customEmojis).map((m) => ({
        key: m.key,
        label: m.name,
        insertText: `:${m.name}:`,
        thumbnail:
          m.kind === "custom" ? { type: "custom" as const, url: m.url } : { type: "unicode" as const, char: m.char },
      }));
    case "fnName":
      return matchFnNames(trigger.query).map((name) => ({ key: name, label: name, insertText: name }));
    case "argName":
      return matchArgNames(trigger.fnName, trigger.query).map((spec) => ({
        key: spec.name,
        label: spec.hasValue ? `${spec.name}=` : spec.name,
        insertText: spec.hasValue ? `${spec.name}=` : spec.name,
      }));
    case "argValue":
      return matchArgValues(trigger.fnName, trigger.argName, trigger.query).map((value) => ({
        key: value,
        label: value,
        insertText: value,
      }));
  }
}

export function applyCompletion(
  text: string,
  trigger: Trigger,
  item: CompletionItem,
): { text: string; cursor: number } {
  const next = text.slice(0, trigger.start) + item.insertText + text.slice(trigger.end);
  return { text: next, cursor: trigger.start + item.insertText.length };
}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/mfmCompletion.test.ts`
Expected: PASS(Cycle A〜Cの全ケース)

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mfmCompletion.ts frontend/src/lib/mfmCompletion.test.ts
git commit -m "feat: MFM補完の候補組み立て(buildCompletionItems)と置換計算(applyCompletion)を実装"
```

---

## Task 3: `lib/caretPosition.ts` — textarea内のキャレット座標計算

**Files:**
- Create: `frontend/src/lib/caretPosition.ts`
- Test: `frontend/src/lib/caretPosition.test.ts`

**Interfaces:**
- Produces: `export interface CaretCoordinates { left: number; top: number; height: number }`、`export function getCaretCoordinates(el: HTMLTextAreaElement, position: number): CaretCoordinates`

**補足:** jsdomにはレイアウトエンジンが無く `offsetLeft`/`offsetTop` は常に0を返すため、ここでは「例外を投げず数値を返す」「呼び出し後にDOMへミラー要素を残さない」ことのみをテストする。実際のピクセル精度はブラウザでの手動確認(Task 5)で検証する。

- [ ] **Step 1: 失敗するテストを書く**

Create `frontend/src/lib/caretPosition.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { getCaretCoordinates } from "./caretPosition";

function makeTextarea(value: string): HTMLTextAreaElement {
  const el = document.createElement("textarea");
  el.value = value;
  document.body.appendChild(el);
  return el;
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("getCaretCoordinates", () => {
  it("returns numeric left/top/height without throwing", () => {
    const el = makeTextarea("hello world");
    const coords = getCaretCoordinates(el, 5);
    expect(typeof coords.left).toBe("number");
    expect(typeof coords.top).toBe("number");
    expect(typeof coords.height).toBe("number");
    expect(Number.isNaN(coords.left)).toBe(false);
  });

  it("does not leave a mirror element behind in the DOM", () => {
    const el = makeTextarea("hello world");
    getCaretCoordinates(el, 3);
    expect(document.getElementById("mfm-completion-caret-mirror")).toBeNull();
  });

  it("handles a position at the very end of the text", () => {
    const el = makeTextarea("hi");
    expect(() => getCaretCoordinates(el, 2)).not.toThrow();
  });

  it("handles an empty textarea", () => {
    const el = makeTextarea("");
    expect(() => getCaretCoordinates(el, 0)).not.toThrow();
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/caretPosition.test.ts`
Expected: FAIL — `./caretPosition` が存在しない

- [ ] **Step 3: 実装する**

Create `frontend/src/lib/caretPosition.ts`:

```ts
// textarea内の指定文字位置のキャレット座標(textarea左上を原点とするpx)を返す。
// ミラーdiv方式: textareaと同じスタイルを与えた非表示divへキャレット位置までの
// テキストを流し込み、末尾に置いたマーカー要素の offsetLeft/offsetTop を読む定番の手法。
const MIRRORED_PROPERTIES = [
  "boxSizing", "width", "height", "overflowX", "overflowY",
  "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth", "borderStyle",
  "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
  "fontStyle", "fontVariant", "fontWeight", "fontSize", "lineHeight", "fontFamily",
  "textAlign", "textTransform", "textIndent", "textDecoration",
  "letterSpacing", "wordSpacing", "tabSize", "whiteSpace", "wordWrap", "wordBreak",
] as const satisfies readonly (keyof CSSStyleDeclaration)[];

export interface CaretCoordinates {
  left: number;
  top: number;
  height: number;
}

export function getCaretCoordinates(el: HTMLTextAreaElement, position: number): CaretCoordinates {
  const div = document.createElement("div");
  div.id = "mfm-completion-caret-mirror";
  document.body.appendChild(div);

  const style = div.style;
  const computed = window.getComputedStyle(el);

  style.position = "absolute";
  style.visibility = "hidden";
  style.top = "0";
  style.left = "-9999px";
  style.whiteSpace = "pre-wrap";
  style.wordWrap = "break-word";

  for (const prop of MIRRORED_PROPERTIES) {
    const value = computed[prop as keyof CSSStyleDeclaration];
    if (typeof value === "string") {
      (style as unknown as Record<string, string>)[prop] = value;
    }
  }
  style.width = computed.width;

  div.textContent = el.value.slice(0, position);
  const marker = document.createElement("span");
  marker.textContent = el.value.slice(position) || ".";
  div.appendChild(marker);

  const coords: CaretCoordinates = {
    left: marker.offsetLeft - el.scrollLeft,
    top: marker.offsetTop - el.scrollTop,
    height: marker.offsetHeight,
  };

  document.body.removeChild(div);
  return coords;
}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/caretPosition.test.ts`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/caretPosition.ts frontend/src/lib/caretPosition.test.ts
git commit -m "feat: textarea内キャレット座標計算(getCaretCoordinates)を追加"
```

---

## Task 4: `lib/portal.ts` — ポータル用Svelteアクションの共通化

`ComposeBar.svelte` は添付メニュー(`showAttachMenu`)表示のために、要素を `document.body` 直下へ移動するローカル関数 `attachPortal` をすでに持っている(`frontend/src/ui/ComposeBar.svelte:83-86`)。新しい `CompletionPopover.svelte` も同じ理由(固定位置での重ね表示)で同じ仕組みが要るため、共通ユーティリティへ切り出して両方から使う。

**Files:**
- Create: `frontend/src/lib/portal.ts`
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Produces: `export function portal(node: HTMLElement): { destroy: () => void }`

**補足:** 3行のDOM移動ロジックであり、元の `attachPortal` にも専用テストは無い(既存の添付メニューは手動確認のみでカバーされている)。ここでも既存の方針に合わせ、専用の自動テストは追加しない。

- [ ] **Step 1: 実装する**

Create `frontend/src/lib/portal.ts`:

```ts
// 要素を document.body 直下へ移動するSvelteアクション。
// 固定配置のポップアップ/メニューを、祖先要素の overflow:hidden や
// position:relative の影響を受けずに画面へ重ねて表示するために使う。
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy: () => node.remove(),
  };
}
```

- [ ] **Step 2: `ComposeBar.svelte` を切り替える**

`frontend/src/ui/ComposeBar.svelte` の import群に追加:

```ts
import { portal } from "../lib/portal";
```

既存のローカル関数を削除:

```ts
  function attachPortal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
```

テンプレート内の `use:attachPortal` を `use:portal` に置換(1箇所、添付メニューの `<div class="attach-overlay" use:attachPortal ...>`)。

- [ ] **Step 3: 型チェックで確認する**

Run: `cd frontend && pnpm check`
Expected: エラー無し(未使用importや未解決の `attachPortal` 参照が残っていないこと)

- [ ] **Step 4: コミット**

```bash
git add frontend/src/lib/portal.ts frontend/src/ui/ComposeBar.svelte
git commit -m "refactor: ポータル用アクションをlib/portal.tsへ切り出す"
```

---

## Task 5: `CompletionPopover.svelte` — 候補ポップアップUI

**Files:**
- Create: `frontend/src/ui/CompletionPopover.svelte`
- Test: `frontend/src/ui/CompletionPopover.test.ts`

**Interfaces:**
- Consumes: `CompletionItem` type from `../lib/mfmCompletion`(Task 2)。`portal` from `../lib/portal`(Task 4)。
- Produces: Svelte component `CompletionPopover` with props `{ items: CompletionItem[]; selectedIndex: number; left: number; top: number; onpick: (index: number) => void }`

- [ ] **Step 1: 失敗するテストを書く**

Create `frontend/src/ui/CompletionPopover.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import CompletionPopover from "./CompletionPopover.svelte";
import type { CompletionItem } from "../lib/mfmCompletion";

afterEach(() => cleanup());

const emojiItem: CompletionItem = {
  key: "custom:neko",
  label: "neko",
  insertText: ":neko:",
  thumbnail: { type: "custom", url: "https://example.com/neko.png" },
};
const unicodeItem: CompletionItem = {
  key: "unicode:grin",
  label: "grin",
  insertText: ":grin:",
  thumbnail: { type: "unicode", char: "😁" },
};
const textItem: CompletionItem = { key: "tada", label: "tada", insertText: "tada" };

describe("CompletionPopover", () => {
  it("renders one row per item with its label", () => {
    const { getByText } = render(CompletionPopover, {
      props: { items: [emojiItem, textItem], selectedIndex: 0, left: 10, top: 20, onpick: () => {} },
    });
    expect(getByText("neko")).toBeTruthy();
    expect(getByText("tada")).toBeTruthy();
  });

  it("renders a thumbnail image for a custom emoji item", () => {
    const { getByRole } = render(CompletionPopover, {
      props: { items: [emojiItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
    });
    expect(getByRole("img").getAttribute("src")).toBe("https://example.com/neko.png");
  });

  it("renders the raw character for a unicode emoji item (no image)", () => {
    const { getByText, queryByRole } = render(CompletionPopover, {
      props: { items: [unicodeItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
    });
    expect(getByText("😁")).toBeTruthy();
    expect(queryByRole("img")).toBeNull();
  });

  it("marks the item at selectedIndex as selected", () => {
    const { getAllByRole } = render(CompletionPopover, {
      props: { items: [emojiItem, textItem], selectedIndex: 1, left: 0, top: 0, onpick: () => {} },
    });
    const options = getAllByRole("option");
    expect(options[0].getAttribute("aria-selected")).toBe("false");
    expect(options[1].getAttribute("aria-selected")).toBe("true");
  });

  it("calls onpick with the clicked item's index", async () => {
    const onpick = vi.fn();
    const { getAllByRole } = render(CompletionPopover, {
      props: { items: [emojiItem, textItem], selectedIndex: 0, left: 0, top: 0, onpick },
    });
    await fireEvent.mouseDown(getAllByRole("option")[1]);
    expect(onpick).toHaveBeenCalledWith(1);
  });

  it("prevents the default mousedown action so the textarea never loses focus", async () => {
    const { getAllByRole } = render(CompletionPopover, {
      props: { items: [emojiItem], selectedIndex: 0, left: 0, top: 0, onpick: () => {} },
    });
    const event = await fireEvent.mouseDown(getAllByRole("option")[0]);
    expect(event).toBe(false); // fireEventはpreventDefaultされたイベントでfalseを返す
  });

  it("positions itself using the left/top props", () => {
    // portal(→lib/portal.ts)がルート要素を document.body 直下へ移動するため、
    // render() の container ではなく baseElement(既定で document.body)側から探す。
    const { baseElement } = render(CompletionPopover, {
      props: { items: [textItem], selectedIndex: 0, left: 42, top: 99, onpick: () => {} },
    });
    const el = baseElement.querySelector(".completion-popover") as HTMLElement;
    expect(el.style.left).toBe("42px");
    expect(el.style.top).toBe("99px");
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/CompletionPopover.test.ts`
Expected: FAIL — `CompletionPopover.svelte` が存在しない

- [ ] **Step 3: 実装する**

Create `frontend/src/ui/CompletionPopover.svelte`:

```svelte
<script lang="ts">
  import type { CompletionItem } from "../lib/mfmCompletion";
  import { portal } from "../lib/portal";

  let {
    items,
    selectedIndex,
    left,
    top,
    onpick,
  }: {
    items: CompletionItem[];
    selectedIndex: number;
    left: number;
    top: number;
    onpick: (index: number) => void;
  } = $props();
</script>

<div class="completion-popover" use:portal style={`left:${left}px;top:${top}px`} role="listbox">
  {#each items as item, i (item.key)}
    <button
      type="button"
      class="completion-item"
      class:selected={i === selectedIndex}
      role="option"
      aria-selected={i === selectedIndex}
      onmousedown={(e) => {
        // click ではなく mousedown を使い、かつ preventDefault することで
        // textarea の blur を発生させずに確定できるようにする(blurが先に走ると
        // ポップアップが閉じてクリックが空振りする)。
        e.preventDefault();
        onpick(i);
      }}
    >
      {#if item.thumbnail?.type === "custom"}
        <img class="completion-thumb" src={item.thumbnail.url} alt="" />
      {:else if item.thumbnail?.type === "unicode"}
        <span class="completion-thumb completion-thumb-unicode">{item.thumbnail.char}</span>
      {/if}
      <span class="completion-label">{item.label}</span>
    </button>
  {/each}
</div>

<style>
  .completion-popover {
    position: fixed;
    z-index: 60;
    display: flex;
    flex-direction: column;
    max-height: 260px;
    overflow-y: auto;
    min-width: 160px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 4px;
  }
  .completion-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-size: 0.82rem;
  }
  .completion-item.selected {
    background: var(--surface-2);
    color: var(--accent);
  }
  .completion-thumb {
    flex: none;
    width: 18px;
    height: 18px;
    object-fit: contain;
  }
  .completion-thumb-unicode {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 1rem;
  }
  .completion-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/ui/CompletionPopover.test.ts`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/CompletionPopover.svelte frontend/src/ui/CompletionPopover.test.ts
git commit -m "feat: MFM補完候補ポップアップ(CompletionPopover)を追加"
```

---

## Task 6: `ComposeBar.svelte` への配線

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: `detectTrigger`, `buildCompletionItems`, `applyCompletion`, `Trigger`, `CompletionItem` from `../lib/mfmCompletion`(Task 2)。`getCaretCoordinates` from `../lib/caretPosition`(Task 3)。`CompletionPopover` from `./CompletionPopover.svelte`(Task 5)。`app.loadEmojis(accountId)`/`app.emojis`(既存、`frontend/src/lib/store.svelte.ts:1381-1409`)。

自動テストではなく手動確認(Step末尾)で検証する。理由: `ComposeBar.svelte` は `app`(アカウント・IPC・投稿状態を持つグローバルストア)や `@tauri-apps/plugin-dialog` に直接依存しており、これらを丸ごとモックするコストは配線ロジック(数十行のイベントハンドラ)に対して不釣り合いに大きい。判定・マッチング・置換のロジックはすべてTask 2で純粋関数として網羅テスト済みなので、ここでの残作業はDOM配線のみである。プロジェクトのUI変更方針(`CLAUDE.md`)に従い、`cargo tauri dev` 上での実機確認をもって検証とする。

- [ ] **Step 1: importを追加する**

`frontend/src/ui/ComposeBar.svelte` の先頭import群に追加:

```ts
import { tick } from "svelte";
import CompletionPopover from "./CompletionPopover.svelte";
import { applyCompletion, buildCompletionItems, detectTrigger, type CompletionItem, type Trigger } from "../lib/mfmCompletion";
import { getCaretCoordinates } from "../lib/caretPosition";
```

- [ ] **Step 2: 補完用の状態を追加する**

`let textarea = $state<HTMLTextAreaElement | undefined>(undefined);` の直後に追加:

```ts
  let cursorPos = $state(0);
  let suppressAt = $state<number | null>(null);
  let composing = $state(false);
  let selectedIndex = $state(0);
```

- [ ] **Step 3: 絵文字データのロードを保証する**

既存の `$effect(() => { if (!accountTouched) accountId = app.defaultAccountId(); });` の直後に追加:

```ts
  // 補完ポップアップで使うカスタム絵文字を先読みする(ReactionPickerと同じパターン)。
  $effect(() => {
    if (accountId) app.loadEmojis(accountId).catch(() => {});
  });
```

- [ ] **Step 4: トリガー・候補・位置の派生値を追加する**

`compact` の `$derived` ブロックの直後に追加:

```ts
  const customEmojiList = $derived(accountId ? (app.emojis[accountId] ?? []) : []);
  const trigger = $derived<Trigger | null>(
    composing || cursorPos === suppressAt ? null : detectTrigger(text, cursorPos),
  );
  const candidates = $derived<CompletionItem[]>(trigger ? buildCompletionItems(trigger, customEmojiList) : []);
  const popoverOpen = $derived(trigger !== null && candidates.length > 0);

  // クエリが変わって候補集合が変わるたびに選択位置を先頭へ戻す
  $effect(() => {
    trigger;
    selectedIndex = 0;
  });

  let popoverPos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!popoverOpen || !trigger || !textarea) {
      popoverPos = null;
      return;
    }
    const rect = textarea.getBoundingClientRect();
    const caret = getCaretCoordinates(textarea, trigger.start);
    popoverPos = { left: rect.left + caret.left, top: rect.top + caret.top + caret.height };
  });
```

- [ ] **Step 5: カーソル同期・確定処理を追加する**

`cancelContext` 関数の直前に追加:

```ts
  function syncCursor() {
    const pos = textarea?.selectionStart ?? 0;
    if (pos !== cursorPos) suppressAt = null;
    cursorPos = pos;
  }

  function onTextareaInput() {
    syncCursor();
    suppressAt = null;
  }

  async function confirmCompletion(index: number) {
    const t = trigger;
    const item = candidates[index];
    if (!t || !item) return;
    const result = applyCompletion(text, t, item);
    text = result.text;
    suppressAt = result.cursor;
    await tick();
    textarea?.setSelectionRange(result.cursor, result.cursor);
    textarea?.focus();
    cursorPos = result.cursor;
  }
```

- [ ] **Step 6: `onKey` を拡張してポップアップ操作をルーティングする**

既存の `onKey` を置き換える:

```ts
  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      if (busy) return;
      submit();
      return;
    }
    if (popoverOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        selectedIndex = (selectedIndex + 1) % candidates.length;
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        selectedIndex = (selectedIndex - 1 + candidates.length) % candidates.length;
        return;
      }
      if (e.key === "Tab" || e.key === "Enter") {
        e.preventDefault();
        confirmCompletion(selectedIndex);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        suppressAt = cursorPos; // ポップアップだけ閉じる(返信/引用のキャンセルは行わない)
        return;
      }
    }
    if (e.key === "Escape" && (replyTo || quoteOf)) {
      e.preventDefault();
      cancelContext();
    }
  }
```

- [ ] **Step 7: テンプレートのtextareaにイベントを追加する**

既存の `<textarea>` ブロックを置き換える:

```svelte
  <textarea
    class="text"
    class:compact
    class:expanded
    rows={expanded ? 4 : 1}
    placeholder="いまどうしてる？（Ctrl+Enter で投稿）"
    bind:value={text}
    bind:this={textarea}
    onkeydown={onKey}
    onkeyup={syncCursor}
    onclick={syncCursor}
    oninput={onTextareaInput}
    oncompositionstart={() => (composing = true)}
    oncompositionend={() => {
      composing = false;
      syncCursor();
    }}
    onfocus={() => (focused = true)}
    onblur={() => {
      focused = false;
      suppressAt = cursorPos;
    }}
    onpaste={handlePaste}
  ></textarea>

  {#if popoverOpen && popoverPos}
    <CompletionPopover
      items={candidates}
      {selectedIndex}
      left={popoverPos.left}
      top={popoverPos.top}
      onpick={confirmCompletion}
    />
  {/if}
```

- [ ] **Step 8: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無し

- [ ] **Step 9: 単体テストを一括実行する(回帰確認)**

Run: `cd frontend && pnpm vitest run`
Expected: 既存分含め全PASS

- [ ] **Step 10: 手動確認する**

```bash
cargo tauri dev
```

ComposeBarの本文欄で以下を確認する:

1. `:sm` と入力 → カスタム絵文字とUnicode絵文字(😊等)が候補に出る。カスタムが先、Unicodeが後。
2. `↓`/`↑` で選択が動く。`Enter` で確定すると `:name:` が挿入され、カーソルがその直後に来る。続けて別の文字を打っても即座に再トリガーしない(直後の再ポップアップが出ない)こと。
3. `$[ta` と入力 → `tada` が候補に出る。確定すると `$[tada` になり、`]` は自動挿入されない。
4. `$[tada.` と入力 → `speed`/`delay` が候補に出る。`speed` を確定すると `$[tada.speed=` になりカーソルが `=` の直後。
5. `$[border.style=so` と入力 → `solid` が候補に出て、確定で `so` が `solid` に置き換わる。
6. `$[tada hi` のように空白を打った後は `$[` 系の補完が出ない(絵文字トリガーだけは効く)こと。
7. `Escape` でポップアップが閉じる。返信/引用中に `Escape` を押した場合、ポップアップが無ければ返信/引用のキャンセルが従来通り動く。
8. `Ctrl+Enter` はポップアップ表示中でも投稿として動作する(補完確定に奪われない)。
9. 日本語IME変換中(例えば「かお」→変換候補表示)は補完ポップアップが割り込まない。
10. ポップアップの候補をマウスクリックしても確定でき、textareaのフォーカスが外れない。

問題が見つかった場合はこのタスク内で修正し、再度手動確認する。

- [ ] **Step 11: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: ComposeBarにMFM補完ポップアップを配線"
```

---

## Self-Review Notes

- **Spec coverage:** 絵文字/fn名/引数名/引数値の4トリガー(Task 2)、UI+キー操作(Task 5, 6)、キャレット追従(Task 3)、適用範囲=ComposeBar本文のみ(Task 6で他のtextareaに触れていない)、IME対応・ネスト`$[`非対応の既知制約(Task 6 Step 4-6のロジックと手動確認Step 10-6, 10-9)を各タスクでカバー済み。メンション/ハッシュタグは意図的にスコープ外(Global Constraints明記)。
- **Placeholder scan:** 各Stepのコードはすべて完全な実装/テストコードで、TODOや「後で実装」は無い。
- **Type consistency:** `Trigger`/`CompletionItem`/`MfmArgSpec` の型はTask 1→2→5→6の順に一貫して同じ形状で使い回している(Task 6のimportで再掲・確認済み)。
