# フロントエンド単体テスト基盤の導入（Vitest） Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `frontend/` にVitestを導入し、`frontend/src/lib/` 配下の純粋ロジック7ファイルに単体テストを追加し、CIで自動実行されるようにする。

**Architecture:** 既存の `frontend/vite.config.ts` にVitestの `test` ブロックを追加し（別ファイルの `vitest.config.ts` は作らない）、jsdom環境で動かす。テスト対象は既存実装（変更なし）なので、各タスクは「特性テスト（characterization test）を書く→現行実装に対して green で通ることを確認する」という順序で進める（新規実装のRed-Green-Refactorではない）。最後にCIワークフローへ組み込む。

**Tech Stack:** Vitest, jsdom, pnpm, TypeScript（既存の `frontend/tsconfig.app.json` の型設定をそのまま使う）

## Global Constraints

- テストランナーはVitest、環境は `jsdom`（`docs/superpowers/specs/2026-07-29-frontend-unit-test-vitest-design.md` より）
- 設定は `frontend/vite.config.ts` の `test` ブロックに追加する。別ファイル `vitest.config.ts` は作らない
- テストファイルは対象ファイルと同ディレクトリに `*.test.ts` 命名で配置する
- カバレッジ計測ツールはこのspecの対象外。導入しない
- 対象ファイルは既存実装のまま変更しない（テスト追加のみ）
- 全てのコマンドは `frontend/` ディレクトリで実行する

---

### Task 1: Vitest基盤の導入 + `time.ts` テスト

**Files:**
- Modify: `frontend/package.json`
- Modify: `frontend/vite.config.ts`
- Test: `frontend/src/lib/time.test.ts`

**Interfaces:**
- Consumes: `relativeTime(epochSec: number): string`（`frontend/src/lib/time.ts` の既存export、変更なし）
- Produces: `pnpm test` コマンド（`vitest run` を実行）。以降の全タスクがこれを使う

- [ ] **Step 1: Vitestとjsdomをインストール**

```bash
cd frontend && pnpm add -D vitest jsdom
```

これにより `frontend/package.json` の `devDependencies` と `frontend/pnpm-lock.yaml` が更新される。

- [ ] **Step 2: `test` スクリプトを追加**

`frontend/package.json` の `scripts` に以下を追加する（`"check"` の次の行）:

```json
    "check": "svelte-check --tsconfig ./tsconfig.app.json && tsc -p tsconfig.node.json",
    "test": "vitest run"
```

- [ ] **Step 3: `frontend/vite.config.ts` にVitestの `test` ブロックを追加**

ファイル先頭（`import { defineConfig } from "vite";` の前）に、`tsc -p tsconfig.node.json` がVitestの `test` オプションの型を解決できるよう三重スラッシュ参照を追加する:

```ts
/// <reference types="vitest/config" />
```

`export default defineConfig({` の中、`server: { ... }` ブロックの後ろに以下を追加する:

```ts
  test: {
    environment: "jsdom",
  },
```

- [ ] **Step 4: `frontend/src/lib/time.test.ts` を作成**

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { relativeTime } from "./time";

describe("relativeTime", () => {
  const NOW = new Date("2026-07-29T12:00:00Z").getTime();

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns an empty string for epochSec 0", () => {
    expect(relativeTime(0)).toBe("");
  });

  it("returns seconds for under a minute", () => {
    const epochSec = NOW / 1000 - 30;
    expect(relativeTime(epochSec)).toBe("30s");
  });

  it("returns minutes for under an hour", () => {
    const epochSec = NOW / 1000 - 5 * 60;
    expect(relativeTime(epochSec)).toBe("5m");
  });

  it("returns hours for under a day", () => {
    const epochSec = NOW / 1000 - 3 * 3600;
    expect(relativeTime(epochSec)).toBe("3h");
  });

  it("returns days for under a week", () => {
    const epochSec = NOW / 1000 - 2 * 86400;
    expect(relativeTime(epochSec)).toBe("2d");
  });

  it("returns a localized date for a week or more", () => {
    const epochSec = NOW / 1000 - 8 * 86400;
    expect(relativeTime(epochSec)).toBe(new Date(epochSec * 1000).toLocaleDateString());
  });
});
```

- [ ] **Step 5: テストを実行して通ることを確認**

Run: `pnpm test`
Expected: `Test Files  1 passed (1)` / `Tests  6 passed (6)`（`time.test.ts` のみが対象なのでこの時点でファイル数は1）

- [ ] **Step 6: コミット**

```bash
git add package.json pnpm-lock.yaml vite.config.ts src/lib/time.test.ts
git commit -m "test: Vitest基盤を導入しtime.tsのテストを追加"
```

---

### Task 2: `nyaize.ts` テスト

**Files:**
- Test: `frontend/src/lib/nyaize.test.ts`

**Interfaces:**
- Consumes: `nyaize(text: string): string`（`frontend/src/lib/nyaize.ts` の既存export）

- [ ] **Step 1: `frontend/src/lib/nyaize.test.ts` を作成**

```ts
import { describe, expect, it } from "vitest";
import { nyaize } from "./nyaize";

describe("nyaize", () => {
  it("converts ja-JP な to にゃ", () => {
    expect(nyaize("こんな感じ")).toBe("こんにゃ感じ");
  });

  it("converts katakana ナ to ニャ", () => {
    expect(nyaize("バナナ")).toBe("バニャニャ");
  });

  it("converts lowercase 'na' preceded by n to 'nya'", () => {
    expect(nyaize("banana")).toBe("banyanya");
  });

  it("preserves case when converting 'NA' to 'NYA'", () => {
    expect(nyaize("BANANA")).toBe("BANYANYA");
  });

  it("converts 'morning' to 'mornyan'", () => {
    expect(nyaize("morning")).toBe("mornyan");
  });

  it("converts 'everyone' to 'everynyan'", () => {
    expect(nyaize("everyone")).toBe("everynyan");
  });

  it("returns the input unchanged when nothing matches", () => {
    expect(nyaize("hello world")).toBe("hello world");
  });

  it("returns an empty string for empty input", () => {
    expect(nyaize("")).toBe("");
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/lib/nyaize.test.ts`
Expected: `Tests  8 passed (8)`

- [ ] **Step 3: コミット**

```bash
git add src/lib/nyaize.test.ts
git commit -m "test: nyaize.tsのテストを追加"
```

---

### Task 3: `backgroundFitMode.ts` テスト

**Files:**
- Test: `frontend/src/lib/backgroundFitMode.test.ts`

**Interfaces:**
- Consumes: `BACKGROUND_FIT_MODE_CSS: Record<string, [string, string]>`, `BACKGROUND_FIT_MODE_OPTIONS: { value: BackgroundFitMode; label: string }[]`（`frontend/src/lib/backgroundFitMode.ts` の既存export）

- [ ] **Step 1: `frontend/src/lib/backgroundFitMode.test.ts` を作成**

```ts
import { describe, expect, it } from "vitest";
import { BACKGROUND_FIT_MODE_CSS, BACKGROUND_FIT_MODE_OPTIONS } from "./backgroundFitMode";

describe("BACKGROUND_FIT_MODE_CSS", () => {
  it("maps cover to background-size cover / no-repeat", () => {
    expect(BACKGROUND_FIT_MODE_CSS.cover).toEqual(["cover", "no-repeat"]);
  });

  it("maps fill to 100% 100% / no-repeat", () => {
    expect(BACKGROUND_FIT_MODE_CSS.fill).toEqual(["100% 100%", "no-repeat"]);
  });

  it("maps tile to auto / repeat", () => {
    expect(BACKGROUND_FIT_MODE_CSS.tile).toEqual(["auto", "repeat"]);
  });

  it("has a CSS entry for every option value", () => {
    for (const { value } of BACKGROUND_FIT_MODE_OPTIONS) {
      expect(BACKGROUND_FIT_MODE_CSS[value]).toBeDefined();
    }
  });
});

describe("BACKGROUND_FIT_MODE_OPTIONS", () => {
  it("has exactly 4 options", () => {
    expect(BACKGROUND_FIT_MODE_OPTIONS).toHaveLength(4);
  });

  it("has unique values", () => {
    const values = BACKGROUND_FIT_MODE_OPTIONS.map((o) => o.value);
    expect(new Set(values).size).toBe(values.length);
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/lib/backgroundFitMode.test.ts`
Expected: `Tests  6 passed (6)`

- [ ] **Step 3: コミット**

```bash
git add src/lib/backgroundFitMode.test.ts
git commit -m "test: backgroundFitMode.tsのテストを追加"
```

---

### Task 4: `backgroundPosition.ts` テスト

**Files:**
- Test: `frontend/src/lib/backgroundPosition.test.ts`

**Interfaces:**
- Consumes: `BACKGROUND_POSITION_CSS: Record<string, string>`, `BACKGROUND_POSITION_GRID: BackgroundPosition[]`（`frontend/src/lib/backgroundPosition.ts` の既存export）

- [ ] **Step 1: `frontend/src/lib/backgroundPosition.test.ts` を作成**

```ts
import { describe, expect, it } from "vitest";
import { BACKGROUND_POSITION_CSS, BACKGROUND_POSITION_GRID } from "./backgroundPosition";

describe("BACKGROUND_POSITION_CSS", () => {
  it("maps center to center center", () => {
    expect(BACKGROUND_POSITION_CSS.center).toBe("center center");
  });

  it("maps top-left to left top", () => {
    expect(BACKGROUND_POSITION_CSS["top-left"]).toBe("left top");
  });

  it("maps bottom-right to right bottom", () => {
    expect(BACKGROUND_POSITION_CSS["bottom-right"]).toBe("right bottom");
  });

  it("has a CSS value for every grid position", () => {
    for (const pos of BACKGROUND_POSITION_GRID) {
      expect(BACKGROUND_POSITION_CSS[pos]).toBeDefined();
    }
  });
});

describe("BACKGROUND_POSITION_GRID", () => {
  it("has 9 positions in row-major order", () => {
    expect(BACKGROUND_POSITION_GRID).toEqual([
      "top-left",
      "top",
      "top-right",
      "left",
      "center",
      "right",
      "bottom-left",
      "bottom",
      "bottom-right",
    ]);
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/lib/backgroundPosition.test.ts`
Expected: `Tests  5 passed (5)`

- [ ] **Step 3: コミット**

```bash
git add src/lib/backgroundPosition.test.ts
git commit -m "test: backgroundPosition.tsのテストを追加"
```

---

### Task 5: `emojiKey.ts` テスト

**Files:**
- Test: `frontend/src/lib/emojiKey.test.ts`

**Interfaces:**
- Consumes: `isCustomEmojiKey(key: string): boolean`, `customEmojiKey(name: string): string`, `customEmojiPinKey(name: string, host: string): string`, `parseCustomEmojiPinKey(key: string): { name: string; host: string | null }`（`frontend/src/lib/emojiKey.ts` の既存export）

- [ ] **Step 1: `frontend/src/lib/emojiKey.test.ts` を作成**

```ts
import { describe, expect, it } from "vitest";
import { customEmojiKey, customEmojiPinKey, isCustomEmojiKey, parseCustomEmojiPinKey } from "./emojiKey";

describe("isCustomEmojiKey", () => {
  it("returns true for a custom emoji key", () => {
    expect(isCustomEmojiKey(":blob_cat:")).toBe(true);
  });

  it("returns false for a plain unicode emoji", () => {
    expect(isCustomEmojiKey("😺")).toBe(false);
  });

  it("returns false for a lone colon", () => {
    expect(isCustomEmojiKey(":")).toBe(false);
  });

  it("returns false for an empty name between colons", () => {
    expect(isCustomEmojiKey("::")).toBe(false);
  });
});

describe("customEmojiKey", () => {
  it("wraps the name in colons", () => {
    expect(customEmojiKey("blob_cat")).toBe(":blob_cat:");
  });
});

describe("customEmojiPinKey", () => {
  it("wraps name and host in colons with an @ separator", () => {
    expect(customEmojiPinKey("blob_cat", "misskey.io")).toBe(":blob_cat@misskey.io:");
  });
});

describe("parseCustomEmojiPinKey", () => {
  it("splits name and host", () => {
    expect(parseCustomEmojiPinKey(":blob_cat@misskey.io:")).toEqual({
      name: "blob_cat",
      host: "misskey.io",
    });
  });

  it("returns a null host for keys without an @", () => {
    expect(parseCustomEmojiPinKey(":blob_cat:")).toEqual({ name: "blob_cat", host: null });
  });

  it("splits on the last @ when the name itself contains one", () => {
    expect(parseCustomEmojiPinKey(":weird@name@misskey.io:")).toEqual({
      name: "weird@name",
      host: "misskey.io",
    });
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/lib/emojiKey.test.ts`
Expected: `Tests  9 passed (9)`

- [ ] **Step 3: コミット**

```bash
git add src/lib/emojiKey.test.ts
git commit -m "test: emojiKey.tsのテストを追加"
```

---

### Task 6: `keymap.ts` テスト

**Files:**
- Test: `frontend/src/lib/keymap.test.ts`

**Interfaces:**
- Consumes: `ACTIONS`, `eventToChord(e: KeyboardEvent): string`, `defaultKeymap(): Map<string, KeyAction>`, `effectiveChord(action: KeyAction, overrides: Record<string, string>): string`, `buildKeymap(overrides: Record<string, string>): Map<string, KeyAction>`, `prettyChord(chord: string): string`, `isModifierOnly(e: KeyboardEvent): boolean`（`frontend/src/lib/keymap.ts` の既存export）

- [ ] **Step 1: `frontend/src/lib/keymap.test.ts` を作成**

```ts
import { describe, expect, it } from "vitest";
import {
  ACTIONS,
  buildKeymap,
  defaultKeymap,
  effectiveChord,
  eventToChord,
  isModifierOnly,
  prettyChord,
} from "./keymap";

function key(
  k: string,
  mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {},
): KeyboardEvent {
  return new KeyboardEvent("keydown", { key: k, ...mods });
}

describe("eventToChord", () => {
  it("returns a lowercase single key with no modifiers", () => {
    expect(eventToChord(key("J"))).toBe("j");
  });

  it("prefixes modifiers in ctrl/meta/alt/shift order", () => {
    expect(eventToChord(key("Enter", { ctrlKey: true, shiftKey: true }))).toBe("ctrl+shift+Enter");
  });

  it("normalizes space to 'space'", () => {
    expect(eventToChord(key(" "))).toBe("space");
  });

  it("keeps multi-character key names as-is", () => {
    expect(eventToChord(key("ArrowUp"))).toBe("ArrowUp");
  });
});

describe("defaultKeymap", () => {
  it("maps every default chord to its action", () => {
    const m = defaultKeymap();
    expect(m.get("j")).toBe("note.next");
    expect(m.get("n")).toBe("compose.new");
    expect(m.size).toBe(ACTIONS.length);
  });
});

describe("effectiveChord", () => {
  it("returns the default chord when there is no override", () => {
    expect(effectiveChord("note.next", {})).toBe("j");
  });

  it("returns the overridden chord when present", () => {
    expect(effectiveChord("note.next", { "note.next": "shift+j" })).toBe("shift+j");
  });
});

describe("buildKeymap", () => {
  it("applies overrides to the resulting chord map", () => {
    const m = buildKeymap({ "note.next": "shift+j" });
    expect(m.get("shift+j")).toBe("note.next");
    expect(m.get("j")).toBeUndefined();
  });

  it("falls back to defaults for actions without overrides", () => {
    const m = buildKeymap({ "note.next": "shift+j" });
    expect(m.get("k")).toBe("note.prev");
  });
});

describe("prettyChord", () => {
  it("formats a single letter chord", () => {
    expect(prettyChord("j")).toBe("J");
  });

  it("formats modifiers with their display labels", () => {
    expect(prettyChord("ctrl+shift+Enter")).toBe("Ctrl + Shift + Enter");
  });

  it("formats meta as the command symbol", () => {
    expect(prettyChord("meta+k")).toBe("⌘ + K");
  });

  it("formats space as the word 'Space'", () => {
    expect(prettyChord("space")).toBe("Space");
  });
});

describe("isModifierOnly", () => {
  it("returns true when only Shift is pressed", () => {
    expect(isModifierOnly(key("Shift"))).toBe(true);
  });

  it("returns true when only Control is pressed", () => {
    expect(isModifierOnly(key("Control"))).toBe(true);
  });

  it("returns false for a regular key", () => {
    expect(isModifierOnly(key("j"))).toBe(false);
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/lib/keymap.test.ts`
Expected: `Tests  16 passed (16)`

- [ ] **Step 3: コミット**

```bash
git add src/lib/keymap.test.ts
git commit -m "test: keymap.tsのテストを追加"
```

---

### Task 7: `mfm.ts` テスト

**Files:**
- Test: `frontend/src/lib/mfm.test.ts`

**Interfaces:**
- Consumes: `isKnownFn(name: string): boolean`, `mfmFn(name: string, args?: Record<string, string | boolean>): { class: string; style: string }`（`frontend/src/lib/mfm.ts` の既存export）
- Note: `mfmFn` は内部で `window.matchMedia("(prefers-reduced-motion: reduce)")` を呼ぶ。jsdomは `matchMedia` を実装していないため、テスト側で `vi.stubGlobal("matchMedia", ...)` によりモックする必要がある

- [ ] **Step 1: `frontend/src/lib/mfm.test.ts` を作成**

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { isKnownFn, mfmFn } from "./mfm";

function mockMatchMedia(matches: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches,
    media: query,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
}

describe("isKnownFn", () => {
  it("returns true for a known function name", () => {
    expect(isKnownFn("tada")).toBe(true);
  });

  it("returns false for an unknown function name", () => {
    expect(isKnownFn("nonexistent")).toBe(false);
  });
});

describe("mfmFn", () => {
  beforeEach(() => {
    mockMatchMedia(false);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns empty class/style for an unknown function", () => {
    expect(mfmFn("nonexistent")).toEqual({ class: "", style: "" });
  });

  it("applies font-size scaling for x2/x3/x4", () => {
    expect(mfmFn("x2").style).toBe("font-size:2em");
    expect(mfmFn("x3").style).toBe("font-size:3em");
    expect(mfmFn("x4").style).toBe("font-size:4em");
  });

  it("builds a tada animation with default timing", () => {
    const result = mfmFn("tada");
    expect(result.style).toBe(
      "font-size:150%;animation:mfm-tada 1s linear infinite both;animation-delay:0s",
    );
  });

  it("respects custom speed/delay args for tada", () => {
    const result = mfmFn("tada", { speed: "2s", delay: "0.5s" });
    expect(result.style).toBe(
      "font-size:150%;animation:mfm-tada 2s linear infinite both;animation-delay:0.5s",
    );
  });

  it("ignores invalid time args and falls back to defaults", () => {
    const result = mfmFn("tada", { speed: "not-a-time" });
    expect(result.style).toBe(
      "font-size:150%;animation:mfm-tada 1s linear infinite both;animation-delay:0s",
    );
  });

  it("suppresses the animation when reduced motion is preferred", () => {
    mockMatchMedia(true);
    const result = mfmFn("jelly");
    expect(result.style).toBe("");
  });

  it("still applies static styling under reduced motion", () => {
    mockMatchMedia(true);
    const result = mfmFn("tada");
    expect(result.style).toBe("font-size:150%;");
  });

  it("validates hex colors for fg, falling back to red", () => {
    expect(mfmFn("fg", { color: "0f0" }).style).toBe("color:#0f0;overflow-wrap:anywhere");
    expect(mfmFn("fg", { color: "not-a-color" }).style).toBe("color:#f00;overflow-wrap:anywhere");
  });

  it("applies blur as a class, not an inline style", () => {
    expect(mfmFn("blur")).toEqual({ class: "mfm-blur", style: "" });
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/lib/mfm.test.ts`
Expected: `Tests  11 passed (11)`

- [ ] **Step 3: コミット**

```bash
git add src/lib/mfm.test.ts
git commit -m "test: mfm.tsのテストを追加"
```

---

### Task 8: CI統合

**Files:**
- Modify: `.github/workflows/test.yml`

**Interfaces:**
- Consumes: `pnpm test`（Task 1で追加した `frontend/package.json` の `scripts.test`）

- [ ] **Step 1: `frontend-check` ジョブに `pnpm test` ステップを追加**

`.github/workflows/test.yml` の `frontend-check` ジョブ内、`svelte-check` ステップの後ろに追加する:

```yaml
      - name: svelte-check
        working-directory: frontend
        run: pnpm check

      - name: vitest
        working-directory: frontend
        run: pnpm test
```

- [ ] **Step 2: ローカルでフルテストスイートが通ることを確認**

Run: `cd frontend && pnpm test`
Expected: `Test Files  7 passed (7)` / `Tests  61 passed (61)`（Task 1〜7で追加した7ファイル・61ケース全て）

- [ ] **Step 3: `pnpm check` に既存の型エラーが生じていないことを確認**

Run: `cd frontend && pnpm check`
Expected: エラーなしで終了

- [ ] **Step 4: コミット**

```bash
git add .github/workflows/test.yml
git commit -m "ci: frontend-checkジョブにvitestを追加"
```
