# Svelteコンポーネント単体テストの導入（Testing Library） Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `@testing-library/svelte` を導入し、`frontend/src/render/` の `CustomEmoji.svelte` / `Sparkle.svelte` / `Mfm.svelte` の3コンポーネントに描画テストを追加する。

**Architecture:** `frontend/vite.config.ts` に `resolve.conditions: ["browser"]`（Vitest実行時のみ）を追加し、Svelte 5の `mount()` がテスト環境で使えるようにする。`Mfm.svelte` は内部で `MfmNode.svelte` → `UnicodeEmoji.svelte` → `store.svelte.ts` → `platform.ts` を静的importで連鎖的に読み込むため、テストファイル内で `@tauri-apps/plugin-os` を最小限モックする。既存実装（変更なし）に対する特性テストなので、#130と同様「テストを書く→現行実装に対してgreenで通ることを確認する」の順で進める。

**Tech Stack:** Vitest, `@testing-library/svelte`, jsdom, TypeScript

## Global Constraints

- テストファイルは対象ファイルと同ディレクトリに `*.test.ts` 命名で配置する（`docs/superpowers/specs/2026-07-29-svelte-component-testing-library-design.md` より）
- `@testing-library/jest-dom` は導入しない（素の `expect` で書けるため。YAGNI）
- `MfmNode.svelte` 単体のテストファイルは作らない。全ノード種別は `Mfm.svelte` 経由でテストする
- `unicodeEmoji` / `blockCode` ノード種別（`UnicodeEmoji.svelte` / `CodeBlock.svelte` 経由）は描画結果の検証対象外
- カバレッジ計測はこのspecの対象外
- 対象ファイルは既存実装のまま変更しない（テスト追加のみ）
- 全てのコマンドは `frontend/` ディレクトリで実行する

---

### Task 1: Testing Libraryの導入 + `resolve.conditions`設定 + `CustomEmoji.svelte` テスト

**Files:**
- Modify: `frontend/package.json`
- Modify: `frontend/vite.config.ts`
- Test: `frontend/src/render/CustomEmoji.test.ts`

**Interfaces:**
- Consumes: `CustomEmoji.svelte` の props `{ name: string; url?: string; showTitle?: boolean }`（`frontend/src/render/CustomEmoji.svelte` の既存実装、変更なし）
- Produces: `@testing-library/svelte` の `render`/`cleanup` が使える状態。以降の全タスクがこれを使う

- [ ] **Step 1: `@testing-library/svelte` をインストール**

```bash
cd frontend && pnpm add -D @testing-library/svelte
```

- [ ] **Step 2: `frontend/vite.config.ts` に `resolve.conditions` を追加**

Vitestの既定解決では `svelte` パッケージの `exports` 条件がサーバービルド側に倒れ、`@testing-library/svelte` の `render()` が内部で呼ぶ `mount()` が `lifecycle_function_unavailable`（`mount(...) is not available on the server`）で失敗する。`export default defineConfig({` の末尾（`test: { ... }` の後）に追加する:

```ts
  // vitest実行時、Svelteパッケージがサーバー向けビルドに解決され
  // mount()が使えなくなる(lifecycle_function_unavailable)ため、
  // テスト時のみ browser 条件で解決させる。
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
```

- [ ] **Step 3: `frontend/src/render/CustomEmoji.test.ts` を作成**

```ts
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import CustomEmoji from "./CustomEmoji.svelte";

afterEach(() => cleanup());

describe("CustomEmoji", () => {
  it("renders an img with alt/title when a url is provided", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png" },
    });
    const img = getByRole("img");
    expect(img.getAttribute("src")).toBe("https://example.com/e.png");
    expect(img.getAttribute("alt")).toBe(":blob_cat:");
    expect(img.getAttribute("title")).toBe(":blob_cat:");
  });

  it("omits the title attribute when showTitle is false", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png", showTitle: false },
    });
    expect(getByRole("img").hasAttribute("title")).toBe(false);
  });

  it("renders fallback text when no url is provided", () => {
    const { getByText, queryByRole } = render(CustomEmoji, { props: { name: "blob_cat" } });
    expect(getByText(":blob_cat:")).toBeTruthy();
    expect(queryByRole("img")).toBeNull();
  });
});
```

- [ ] **Step 4: テストを実行して通ることを確認**

Run: `pnpm test -- src/render/CustomEmoji.test.ts`
Expected: `Tests  3 passed (3)`

- [ ] **Step 5: コミット**

```bash
git add package.json pnpm-lock.yaml vite.config.ts src/render/CustomEmoji.test.ts
git commit -m "test: Testing Libraryを導入しCustomEmoji.svelteのテストを追加"
```

---

### Task 2: `Sparkle.svelte` テスト

**Files:**
- Test: `frontend/src/render/Sparkle.test.ts`

**Interfaces:**
- Consumes: `Sparkle.svelte` の props `{ children: Snippet }`（`frontend/src/render/Sparkle.svelte` の既存実装、変更なし）
- Note: `Sparkle.svelte` は `window.matchMedia("(prefers-reduced-motion: reduce)")` を直接呼ぶ。jsdomは未実装なので `vi.stubGlobal("matchMedia", ...)` でモックする（#130の `mfm.test.ts` と同じパターン）
- Note: Svelte 5のsnippet propは `svelte` パッケージの `createRawSnippet()` で組み立ててテストに渡す

- [ ] **Step 1: `frontend/src/render/Sparkle.test.ts` を作成**

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import Sparkle from "./Sparkle.svelte";

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

function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span>${text}</span>`,
  }));
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Sparkle", () => {
  it("renders its children", () => {
    mockMatchMedia(false);
    const { getByText } = render(Sparkle, { props: { children: textSnippet("hi") } });
    expect(getByText("hi")).toBeTruthy();
  });

  it("does not render the particle layer when reduced motion is preferred", () => {
    mockMatchMedia(true);
    const { container } = render(Sparkle, { props: { children: textSnippet("hi") } });
    expect(container.querySelector(".layer")).toBeNull();
  });

  it("renders the particle layer when reduced motion is not preferred", () => {
    mockMatchMedia(false);
    const { container } = render(Sparkle, { props: { children: textSnippet("hi") } });
    expect(container.querySelector(".layer")).not.toBeNull();
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/render/Sparkle.test.ts`
Expected: `Tests  3 passed (3)`

- [ ] **Step 3: コミット**

```bash
git add src/render/Sparkle.test.ts
git commit -m "test: Sparkle.svelteのテストを追加"
```

---

### Task 3: `Mfm.svelte` テスト

**Files:**
- Test: `frontend/src/render/Mfm.test.ts`

**Interfaces:**
- Consumes: `Mfm.svelte` の props `{ text: string; emojis?: Record<string, string>; simple?: boolean; nyaize?: boolean }`（`frontend/src/render/Mfm.svelte` の既存実装、変更なし）。内部の `MfmNode.svelte`（全ノード種別の描画分岐）は変更なしのまま、この公開コンポーネント経由でテストする
- Note: `MfmNode.svelte` は `UnicodeEmoji.svelte`/`CodeBlock.svelte` を静的importしており、それらが `../lib/store.svelte` 経由で `../lib/platform.ts` の `platform()` をモジュール評価時に同期呼び出しする。未モックだと `Cannot read properties of undefined (reading 'platform')` で即例外になるため、テストファイル先頭で `vi.mock("@tauri-apps/plugin-os", ...)` する
- Note: `MfmNode.svelte` はどの `fn` ノードでも内部で `mfmFn()` を呼び、`mfmFn()` は常に `window.matchMedia` で prefers-reduced-motion を判定する。`$[...]` 構文を含むテストケース全般で `matchMedia` のスタブが必要（`beforeEach` で毎回スタブする）
- Note: 初回import時、`CodeBlock.svelte` が使う `../lib/shiki`（Shikiシンタックスハイライタ）まで静的importの評価対象に入るため、このテストファイルは実行に約10秒前後かかる。機能的には問題ないため許容する

- [ ] **Step 1: `frontend/src/render/Mfm.test.ts` を作成**

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import Mfm from "./Mfm.svelte";

// MfmNode.svelte は UnicodeEmoji.svelte / CodeBlock.svelte を静的importしており、
// それらが ../lib/store.svelte 経由で ../lib/platform.ts の platform() を
// モジュール評価時に同期呼び出しする。Tauriランタイム外(jsdom)では未モックだと
// 即座に例外になるため最小限モックする。
vi.mock("@tauri-apps/plugin-os", () => ({
  platform: () => "linux",
}));

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

// MfmNode.svelte はどの fn ノードでも mfmFn() を呼び、mfmFn() は常に
// prefers-reduced-motion を window.matchMedia で判定する。jsdomは未実装なので
// $[...] を含むケース全般でスタブが要る。
beforeEach(() => {
  mockMatchMedia(false);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Mfm", () => {
  it("renders bold/italic/strike", () => {
    expect(render(Mfm, { props: { text: "**bold**" } }).container.querySelector("b")?.textContent).toBe(
      "bold",
    );
    expect(render(Mfm, { props: { text: "*italic*" } }).container.querySelector("i")?.textContent).toBe(
      "italic",
    );
    expect(render(Mfm, { props: { text: "~~strike~~" } }).container.querySelector("s")?.textContent).toBe(
      "strike",
    );
  });

  it("renders small/center", () => {
    expect(
      render(Mfm, { props: { text: "<small>small</small>" } }).container.querySelector("small")
        ?.textContent,
    ).toBe("small");
    const center = render(Mfm, { props: { text: "<center>hi</center>" } }).container.querySelector("div");
    expect(center?.textContent).toBe("hi");
    expect(center?.style.textAlign).toBe("center");
  });

  it("renders a quote as a blockquote", () => {
    const { container } = render(Mfm, { props: { text: "> quoted" } });
    expect(container.querySelector("blockquote.mfm-quote")?.textContent).toBe("quoted");
  });

  it("renders a link with its label and an external-link icon", () => {
    const { container } = render(Mfm, { props: { text: "[label](https://example.com)" } });
    const a = container.querySelector("a.mfm-link");
    expect(a?.getAttribute("href")).toBe("https://example.com");
    expect(a?.textContent).toBe("label");
    expect(a?.querySelector("svg")).toBeTruthy();
  });

  it("renders a bare url as a link showing the url itself", () => {
    const { container } = render(Mfm, { props: { text: "https://example.com/path" } });
    const a = container.querySelector("a.mfm-link");
    expect(a?.getAttribute("href")).toBe("https://example.com/path");
    expect(a?.textContent).toBe("https://example.com/path");
  });

  it("renders a mention", () => {
    const { container } = render(Mfm, { props: { text: "@alice@example.com" } });
    expect(container.querySelector("span.mfm-mention")?.textContent).toBe("@alice@example.com");
  });

  it("renders a hashtag", () => {
    const { container } = render(Mfm, { props: { text: "#tag" } });
    expect(container.querySelector("span.mfm-hashtag")?.textContent).toBe("#tag");
  });

  it("renders a custom emoji image when a url is supplied", () => {
    const { getByRole } = render(Mfm, {
      props: { text: ":blob_cat:", emojis: { blob_cat: "https://example.com/e.png" } },
    });
    expect(getByRole("img").getAttribute("src")).toBe("https://example.com/e.png");
  });

  it("falls back to :name: text when the emoji is unknown", () => {
    const { container } = render(Mfm, { props: { text: ":blob_cat:" } });
    expect(container.querySelector(".custom-emoji-fallback")?.textContent).toBe(":blob_cat:");
  });

  it("renders inline code", () => {
    const { container } = render(Mfm, { props: { text: "`code`" } });
    expect(container.querySelector("code.mfm-code")?.textContent).toBe("code");
  });

  it("applies a known fn's styling (x2 doubles the font size)", () => {
    const { container } = render(Mfm, { props: { text: "$[x2 hi]" } });
    const span = container.querySelector("span");
    expect(span?.textContent).toBe("hi");
    expect(span?.style.fontSize).toBe("2em");
  });

  it("shows an unknown fn literally instead of dropping it", () => {
    const { container } = render(Mfm, { props: { text: "$[nonexistent hi]" } });
    expect(container.textContent).toBe("$[nonexistent hi]");
  });

  it("renders $[ruby base rt] as a <ruby> element with a reading", () => {
    const { container } = render(Mfm, { props: { text: "$[ruby base rt]" } });
    const ruby = container.querySelector("ruby");
    expect(ruby?.querySelector("rt")?.textContent).toBe("rt");
    expect(ruby?.textContent).toBe("basert");
  });

  it("renders $[unixtime ...] as a localized date/time with a matching title", () => {
    const { container } = render(Mfm, { props: { text: "$[unixtime 1700000000]" } });
    const el = container.querySelector(".mfm-unixtime");
    const expected = new Date(1700000000 * 1000).toLocaleString();
    expect(el?.getAttribute("title")).toBe(expected);
    expect(el?.textContent).toBe(`🕛 ${expected}`);
  });

  it("wraps $[sparkle ...] content in the Sparkle component", () => {
    const { container } = render(Mfm, { props: { text: "$[sparkle hi]" } });
    const sparkle = container.querySelector(".mfm-sparkle");
    expect(sparkle?.textContent?.trim()).toBe("hi");
  });

  it("renders $[clickable ...] content without a wrapper element", () => {
    const { container } = render(Mfm, { props: { text: "$[clickable hi]" } });
    expect(container.textContent).toBe("hi");
    expect(container.querySelector("span, a, b")).toBeNull();
  });

  it("renders <plain>...</plain> content as literal text, not parsed MFM", () => {
    const { container } = render(Mfm, { props: { text: "<plain>**not bold**</plain>" } });
    expect(container.textContent).toBe("**not bold**");
    expect(container.querySelector("b")).toBeNull();
  });

  it("nyaizes text nodes when nyaize is true", () => {
    const { container } = render(Mfm, { props: { text: "こんな", nyaize: true } });
    expect(container.textContent).toBe("こんにゃ");
  });

  it("does not nyaize when nyaize is false", () => {
    const { container } = render(Mfm, { props: { text: "こんな", nyaize: false } });
    expect(container.textContent).toBe("こんな");
  });

  it("does not nyaize a link label even when nyaize is true (disableNyaize)", () => {
    const { container } = render(Mfm, {
      props: { text: "[こんな](https://example.com)", nyaize: true },
    });
    expect(container.querySelector("a")?.textContent).toBe("こんな");
  });

  it("does not nyaize inside a quote even when nyaize is true (disableNyaize)", () => {
    const { container } = render(Mfm, { props: { text: "> こんな", nyaize: true } });
    expect(container.querySelector("blockquote")?.textContent).toBe("こんな");
  });

  it("renders nothing for empty text", () => {
    const { container } = render(Mfm, { props: { text: "" } });
    expect(container.textContent).toBe("");
  });
});
```

- [ ] **Step 2: テストを実行して通ることを確認**

Run: `pnpm test -- src/render/Mfm.test.ts`
Expected: `Tests  22 passed (22)`（初回importに約10秒前後かかる）

- [ ] **Step 3: コミット**

```bash
git add src/render/Mfm.test.ts
git commit -m "test: Mfm.svelteのテストを追加"
```

---

### Task 4: 全体確認

**Files:**
- (変更なし。検証のみ)

- [ ] **Step 1: フルテストスイートを実行**

Run: `cd frontend && pnpm test`
Expected: `Test Files  10 passed (10)` / `Tests  89 passed (89)`（#130の61件 + Task1〜3の28件）

- [ ] **Step 2: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: エラーなしで終了（既存の3件のsvelte-check warningのみ残る）

追加のコミットは不要（Task 1〜3のコミットのみ）。CIは#130で追加済みの `pnpm test` ステップがそのまま新規ファイルを拾う。
