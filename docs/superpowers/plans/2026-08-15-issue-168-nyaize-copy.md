# nyaize前の文字列をコピーできるようにする（Issue #168） Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノート本文がにゃん語化(`nyaize()`)されて表示されているとき、ユーザがそのテキストを選択してコピーすると、にゃん語化"前"の元の文字列がクリップボードに入るようにする。

**Architecture:** `MfmNode.svelte` の text/ruby 描画箇所で、にゃん語化適用時のみ `data-original-text` 属性を持つ `<span>` で実テキストをラップする。新規モジュール `nyaizeCopy.ts` が、コピー時の選択範囲(`Range`)をDOM上で走査し、`data-original-text` を持つ祖先があれば編集距離ベースの文字対応表で元テキストの部分文字列に差し替えて `clipboardData` に書き込む。`NoteCard.svelte` の CW/本文の2箇所に `oncopy` ハンドラとして接続する。

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest + @testing-library/svelte（既存のフロントエンドテスト基盤をそのまま使用。新規依存追加なし）。

**Spec:** `docs/superpowers/specs/2026-08-15-issue-168-nyaize-copy-design.md`

## Global Constraints

- 表示上の見た目・にゃん語化ロジック(`nyaize()`)自体は変更しない。既存の `nyaize.test.ts` / `Mfm.test.ts` は無変更のまま通ること。
- nyaize対象外のノード（link/quote/plainの中身、mention、hashtag、code等、`disableNyaize`扱い）は元々表示文字列=元文字列なので追加対応不要——`data-original-text` を付けない。
- 選択範囲がコンテナ外にまたがる場合はブラウザのデフォルトコピー動作にフォールバックする（`preventDefault()`しない）。
- コミットメッセージは主語行のみ（本文なし）。

---

## Task 1: 文字対応表ユーティリティ (`buildNyaizeCharMap` / `mapNyaizedRangeToOriginal`)

**Files:**
- Create: `frontend/src/lib/nyaizeCopy.ts`
- Test: `frontend/src/lib/nyaizeCopy.test.ts`

**Interfaces:**
- Produces:
  - `buildNyaizeCharMap(original: string, nyaized: string): number[]` — 長さ `nyaized.length` の配列。`map[i]` は `nyaized[i]` に対応する `original` 側のインデックス（単調非減少）。
  - `mapNyaizedRangeToOriginal(map: number[], original: string, start: number, end: number): string` — `nyaized` 側の半開区間 `[start, end)` を `map` 経由で `original` の対応部分文字列に変換して返す。`start >= end` の場合は空文字列。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/nyaizeCopy.test.ts` を新規作成:

```ts
import { describe, expect, it } from "vitest";
import { buildNyaizeCharMap, mapNyaizedRangeToOriginal } from "./nyaizeCopy";
import { nyaize } from "./nyaize";

describe("buildNyaizeCharMap / mapNyaizedRangeToOriginal", () => {
  it("maps a 1-to-2 char expansion (な -> にゃ) back to the original", () => {
    const original = "こんな感じ";
    const nyaized = nyaize(original); // "こんにゃ感じ"
    const map = buildNyaizeCharMap(original, nyaized);

    // 全選択は元テキスト全体に戻る
    expect(mapNyaizedRangeToOriginal(map, original, 0, nyaized.length)).toBe(original);

    // "にゃ" だけを選択しても、対応する元の1文字 "な" を含む区間に戻る
    const nyaIndex = nyaized.indexOf("にゃ");
    const partial = mapNyaizedRangeToOriginal(map, original, nyaIndex, nyaIndex + 2);
    expect(partial).toBe("な");
  });

  it("maps unchanged text 1:1", () => {
    const original = "hello world";
    const nyaized = nyaize(original); // 変化なし
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 0, 5)).toBe("hello");
  });

  it("maps a trailing insertion (다 -> 다냥) back to the original char", () => {
    const original = "간다";
    const nyaized = nyaize(original); // "간다냥"
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 0, nyaized.length)).toBe(original);
  });

  it("returns an empty string for an empty or inverted range", () => {
    const original = "test";
    const nyaized = nyaize(original);
    const map = buildNyaizeCharMap(original, nyaized);
    expect(mapNyaizedRangeToOriginal(map, original, 2, 2)).toBe("");
    expect(mapNyaizedRangeToOriginal(map, original, 3, 1)).toBe("");
  });

  it("handles empty strings", () => {
    const map = buildNyaizeCharMap("", "");
    expect(mapNyaizedRangeToOriginal(map, "", 0, 0)).toBe("");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/lib/nyaizeCopy.test.ts`
Expected: FAIL（`nyaizeCopy.ts` が存在しない/exportが無い）

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/nyaizeCopy.ts` を新規作成:

```ts
// nyaize()で変換された文字列から、変換前の元の文字列を復元するためのユーティリティ。
// nyaize()は原文字の削除・並べ替えを行わない（1文字->複数文字展開、末尾への文字追加のみ）ため、
// 編集距離DP（挿入コストのみ低い非対称コスト）で十分実用的な対応表が得られる。

/**
 * nyaized の各文字が original の何文字目に対応するかを表す配列を構築する。
 * 戻り値の長さは nyaized.length。値は単調非減少。
 */
export function buildNyaizeCharMap(original: string, nyaized: string): number[] {
  const n = original.length;
  const m = nyaized.length;

  if (m === 0) return [];
  if (n === 0) return new Array(m).fill(0);

  // dp[i][j] = original[0..i) と nyaized[0..j) を対応付けるための最小コスト。
  // 操作: match/substitute(original[i-1] <-> nyaized[j-1], コスト0 or 1) /
  //       insert(nyaized[j-1]だけ消費, original側を消費しない, コスト1) /
  //       delete(original[i-1]だけ消費, コスト2 = 極力避ける)
  const INSERT_COST = 1;
  const DELETE_COST = 2;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = 1; i <= n; i++) dp[i][0] = i * DELETE_COST;
  for (let j = 1; j <= m; j++) dp[0][j] = j * INSERT_COST;

  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      const subCost = original[i - 1] === nyaized[j - 1] ? 0 : 1;
      dp[i][j] = Math.min(
        dp[i - 1][j - 1] + subCost, // match/substitute
        dp[i][j - 1] + INSERT_COST, // insert (originalを消費しない)
        dp[i - 1][j] + DELETE_COST, // delete (nyaizedを消費しない)
      );
    }
  }

  // バックトレースして、各 nyaized インデックスが対応する original インデックスを求める。
  const map = new Array<number>(m);
  let i = n;
  let j = m;
  while (j > 0) {
    const subCost = i > 0 && original[i - 1] === nyaized[j - 1] ? 0 : 1;
    if (i > 0 && dp[i][j] === dp[i - 1][j - 1] + subCost) {
      i -= 1;
      j -= 1;
      map[j] = i;
    } else if (dp[i][j] === dp[i][j - 1] + INSERT_COST) {
      j -= 1;
      // 挿入された文字は直前(まだ消費していない)の original インデックスに対応付ける。
      map[j] = i > 0 ? i - 1 : 0;
    } else {
      i -= 1;
    }
  }

  // 単調性の保証（挿入が先頭に来た場合など、負値/未設定を0側に丸める）。
  for (let k = 0; k < m; k++) {
    if (map[k] === undefined) map[k] = 0;
    if (map[k] < 0) map[k] = 0;
  }
  return map;
}

/**
 * nyaized 側の半開区間 [start, end) を、map 経由で original の対応部分文字列に変換する。
 */
export function mapNyaizedRangeToOriginal(
  map: number[],
  original: string,
  start: number,
  end: number,
): string {
  if (start >= end || start < 0 || end > map.length) return "";
  const origStart = map[start];
  const origEndExclusive = map[end - 1] + 1;
  return original.slice(origStart, Math.max(origStart, origEndExclusive));
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/lib/nyaizeCopy.test.ts`
Expected: PASS（全5ケース）

- [ ] **Step 5: コミット**

```bash
cd frontend && pnpm vitest run src/lib/nyaizeCopy.test.ts && cd ..
git add frontend/src/lib/nyaizeCopy.ts frontend/src/lib/nyaizeCopy.test.ts
git commit -m "feat: nyaize文字対応表ユーティリティを追加"
```

---

## Task 2: copyイベントハンドラ (`handleNyaizeCopy`)

**Files:**
- Modify: `frontend/src/lib/nyaizeCopy.ts`
- Test: `frontend/src/lib/nyaizeCopy.test.ts`

**Interfaces:**
- Consumes: `buildNyaizeCharMap`, `mapNyaizedRangeToOriginal`（Task 1で定義、同一ファイル内）
- Produces: `handleNyaizeCopy(event: ClipboardEvent): void` — `copy` イベントリスナーとして直接使う関数。`event.currentTarget` をコンテナ要素として扱う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/nyaizeCopy.test.ts` に追記（既存の `describe("buildNyaizeCharMap...")` の下に追加）:

```ts
import { fireEvent } from "@testing-library/dom";

describe("handleNyaizeCopy", () => {
  function setDataAttrSpan(text: string, original: string): HTMLSpanElement {
    const span = document.createElement("span");
    span.dataset.originalText = original;
    span.textContent = text;
    return span;
  }

  function fireCopyAndCapture(container: HTMLElement, selectStart: Node, selectEnd: Node): string {
    const range = document.createRange();
    range.setStart(selectStart, 0);
    range.setEnd(selectEnd, (selectEnd.textContent ?? "").length);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    let captured = "";
    const clipboardData = {
      setData: (_type: string, value: string) => {
        captured = value;
      },
    };
    const event = new Event("copy", { bubbles: true, cancelable: true }) as ClipboardEvent;
    Object.defineProperty(event, "clipboardData", { value: clipboardData });
    Object.defineProperty(event, "currentTarget", { value: container });
    fireEvent(container, event);
    return captured;
  }

  it("replaces the copied text with the original (pre-nyaize) text", () => {
    const container = document.createElement("div");
    const span = setDataAttrSpan("こんにゃ感じ", "こんな感じ");
    container.appendChild(span);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const textNode = span.firstChild!;
    const captured = fireCopyAndCapture(container, textNode, textNode);

    expect(captured).toBe("こんな感じ");
    document.body.removeChild(container);
  });

  it("passes through text that has no data-original-text ancestor (nyaize対象外)", () => {
    const container = document.createElement("div");
    const span = document.createElement("span");
    span.textContent = "plain text";
    container.appendChild(span);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const textNode = span.firstChild!;
    const captured = fireCopyAndCapture(container, textNode, textNode);

    expect(captured).toBe("plain text");
    document.body.removeChild(container);
  });

  it("converts <br> elements crossed by the selection into newlines", () => {
    const container = document.createElement("div");
    const span1 = setDataAttrSpan("こんにゃ", "こんな");
    container.appendChild(span1);
    container.appendChild(document.createElement("br"));
    const span2 = setDataAttrSpan("感じ", "感じ");
    container.appendChild(span2);
    document.body.appendChild(container);

    container.addEventListener("copy", handleNyaizeCopy);
    const start = span1.firstChild!;
    const end = span2.firstChild!;
    const captured = fireCopyAndCapture(container, start, end);

    expect(captured).toBe("こんな\n感じ");
    document.body.removeChild(container);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/lib/nyaizeCopy.test.ts`
Expected: FAIL（`handleNyaizeCopy` が存在しない）

- [ ] **Step 3: 実装する**

`frontend/src/lib/nyaizeCopy.ts` の末尾に追記:

```ts
function collectRangePieces(container: HTMLElement, range: Range): string[] {
  const pieces: string[] = [];
  const walker = document.createTreeWalker(
    container,
    NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT,
    {
      acceptNode(node) {
        if (node.nodeType === Node.ELEMENT_NODE && (node as Element).tagName !== "BR") {
          return NodeFilter.FILTER_SKIP;
        }
        return range.intersectsNode(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
      },
    },
  );

  let node = walker.nextNode();
  while (node) {
    if (node.nodeType === Node.ELEMENT_NODE) {
      pieces.push("\n");
    } else {
      const textNode = node as Text;
      const full = textNode.textContent ?? "";
      const start = textNode === range.startContainer ? range.startOffset : 0;
      const end = textNode === range.endContainer ? range.endOffset : full.length;

      const originalAncestor = (textNode.parentElement)?.closest<HTMLElement>("[data-original-text]");
      if (originalAncestor && originalAncestor.contains(textNode)) {
        const original = originalAncestor.dataset.originalText ?? "";
        const map = buildNyaizeCharMap(original, full);
        pieces.push(mapNyaizedRangeToOriginal(map, original, start, end));
      } else {
        pieces.push(full.slice(start, end));
      }
    }
    node = walker.nextNode();
  }
  return pieces;
}

/**
 * copyイベントを横取りし、nyaize済みテキストの選択範囲を元の（nyaize前の）文字列に
 * 差し替えてクリップボードに書き込む。選択がコンテナ外にまたがる場合は何もしない
 * （ブラウザデフォルトのコピー動作にフォールバックする）。
 */
export function handleNyaizeCopy(event: ClipboardEvent): void {
  const container = event.currentTarget as HTMLElement | null;
  if (!container) return;

  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return;

  const range = selection.getRangeAt(0);
  if (!container.contains(range.startContainer) || !container.contains(range.endContainer)) {
    return;
  }

  const pieces = collectRangePieces(container, range);
  event.clipboardData?.setData("text/plain", pieces.join(""));
  event.preventDefault();
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/lib/nyaizeCopy.test.ts`
Expected: PASS（Task 1の5ケース + Task 2の3ケース、計8ケース）

- [ ] **Step 5: コミット**

```bash
cd frontend && pnpm vitest run src/lib/nyaizeCopy.test.ts && cd ..
git add frontend/src/lib/nyaizeCopy.ts frontend/src/lib/nyaizeCopy.test.ts
git commit -m "feat: nyaizeコピーイベントハンドラを追加"
```

---

## Task 3: `MfmNode.svelte` で元テキストをDOMに保持する

**Files:**
- Modify: `frontend/src/render/MfmNode.svelte:62-64`（text分岐）, `frontend/src/render/MfmNode.svelte:48-51,80`（ruby base/rt）
- Test: `frontend/src/render/Mfm.test.ts`

**Interfaces:**
- Consumes: なし（既存の `nyaize()` のみ）
- Produces: `shouldNyaize` が true の text/ruby ノードは `<span data-original-text="...">` でラップされる、という DOM 契約（Task 4 の `NoteCard.svelte` 連携が前提とする）。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/render/Mfm.test.ts` に、既存の `describe("Mfm", ...)` ブロック内へ追記:

```ts
  it("wraps nyaized text in a span with data-original-text", () => {
    const { container } = render(Mfm, { props: { text: "こんな", nyaize: true } });
    const span = container.querySelector("span[data-original-text]");
    expect(span?.dataset.originalText).toBe("こんな");
    expect(span?.textContent).toBe("こんにゃ");
  });

  it("does not add data-original-text when nyaize is false", () => {
    const { container } = render(Mfm, { props: { text: "こんな", nyaize: false } });
    expect(container.querySelector("span[data-original-text]")).toBeNull();
  });

  it("does not add data-original-text inside a link label (disableNyaize)", () => {
    const { container } = render(Mfm, {
      props: { text: "[こんな](https://example.com)", nyaize: true },
    });
    expect(container.querySelector("span[data-original-text]")).toBeNull();
  });
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm vitest run src/render/Mfm.test.ts`
Expected: FAIL（`data-original-text` を持つ要素が無い）

- [ ] **Step 3: `MfmNode.svelte` を修正する**

`frontend/src/render/MfmNode.svelte:62-64` を置き換え:

```svelte
{#if node.type === "text"}
  {@const original = String(p.text ?? "")}
  {@const text = shouldNyaize ? nyaize(original) : original}
  {#if shouldNyaize}
    <span data-original-text={original}>{#each text.split("\n") as line, i}{#if i > 0}<br />{/if}{line}{/each}</span>
  {:else}
    {#each text.split("\n") as line, i}{#if i > 0}<br />{/if}{line}{/each}
  {/if}
```

`frontend/src/render/MfmNode.svelte:80` の ruby 描画部分を、`rubyBaseText`/`rubyRt` に元テキストも持たせる形に変更する。まず `<script>` 部（48-51行目）を置き換え:

```ts
  const rubyBaseOriginal = $derived(ruby?.baseText);
  const rubyBaseText = $derived(
    ruby?.baseText !== undefined ? (shouldNyaize ? nyaize(ruby.baseText) : ruby.baseText) : undefined,
  );
  const rubyRtOriginal = $derived(ruby?.rt ?? "");
  const rubyRt = $derived(ruby ? (shouldNyaize ? nyaize(ruby.rt) : ruby.rt) : "");
```

次に80行目のテンプレートを置き換え:

```svelte
    <ruby>{#if rubyBaseText !== undefined}{#if shouldNyaize}<span data-original-text={rubyBaseOriginal}>{rubyBaseText}</span>{:else}{rubyBaseText}{/if}{:else}{#each ruby.base as c}<Self node={c} {emojis} nyaize={shouldNyaize} />{/each}{/if}<rt>{#if shouldNyaize}<span data-original-text={rubyRtOriginal}>{rubyRt}</span>{:else}{rubyRt}{/if}</rt></ruby>
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm vitest run src/render/Mfm.test.ts`
Expected: PASS（既存ケース含め全件）

- [ ] **Step 5: コミット**

```bash
cd frontend && pnpm vitest run src/render/Mfm.test.ts && cd ..
git add frontend/src/render/MfmNode.svelte frontend/src/render/Mfm.test.ts
git commit -m "feat: nyaize済みテキストに元テキストをdata属性で保持"
```

---

## Task 4: `NoteCard.svelte` に copy ハンドラを接続する

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte:329,338`

**Interfaces:**
- Consumes: `handleNyaizeCopy(event: ClipboardEvent): void`（Task 2、`frontend/src/lib/nyaizeCopy.ts` からimport）

- [ ] **Step 1: import を追加する**

`frontend/src/ui/NoteCard.svelte` の既存 import 群（先頭付近、他の `../lib/` からのimportと並べる）に追加:

```ts
  import { handleNyaizeCopy } from "../lib/nyaizeCopy";
```

- [ ] **Step 2: CWテキストと本文テキストに `oncopy` を追加する**

`frontend/src/ui/NoteCard.svelte:329` を置き換え:

```svelte
          <span class="text-sm [-webkit-user-select:text] select-text" oncopy={handleNyaizeCopy}><Mfm text={inner.cw} emojis={emojiMap} nyaize={inner.user.isCat} /></span>
```

`frontend/src/ui/NoteCard.svelte:338` を置き換え:

```svelte
          <div class="mt-px whitespace-pre-wrap break-words text-sm leading-[1.42] [-webkit-user-select:text] select-text" oncopy={handleNyaizeCopy}><Mfm text={inner.text} emojis={emojiMap} nyaize={inner.user.isCat} /></div>
```

- [ ] **Step 3: フロントエンド全体のチェックを実行**

Run: `cd frontend && pnpm check`
Expected: エラーなし（svelte-check + tsc）

- [ ] **Step 4: 既存テストスイート全体を実行**

Run: `cd frontend && pnpm vitest run`
Expected: 全件PASS（既存分含む）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte
git commit -m "feat: ノートカードにnyaizeコピーハンドラを接続"
```

---

## Task 5: 手動確認 + PR作成

**Files:** なし（コード変更なし）

- [ ] **Step 1: `cargo tauri dev` で実機確認**

`isCat` なアカウントを持つ、またはテスト用に一時的に `inner.user.isCat` を強制 `true` にして、にゃん語化された投稿本文を選択→コピーし、他のテキストエディタに貼り付けて元の文字列（にゃん語化前）になっていることを目視確認する。確認後、デバッグ用の強制trueは元に戻す（コミットしない）。

- [ ] **Step 2: `git push` して PR を作成**

```bash
git push -u origin feat/issue-168-nyaize-copy
gh pr create --title "feat: nyaize前の文字列をコピーできるようにする" --body "$(cat <<'EOF'
Fixes #168

## 概要
にゃん語化(`nyaize()`)された投稿本文を選択してコピーした際、にゃん語化前の元の文字列がクリップボードに入るようにした。

## 実装
- `MfmNode.svelte`: nyaize適用時のテキスト/ruby描画を `data-original-text` 属性付きの `<span>` でラップ
- `frontend/src/lib/nyaizeCopy.ts`（新規）: 編集距離ベースの文字対応表と、`copy` イベントで選択範囲を元テキストに差し替えるハンドラ
- `NoteCard.svelte`: CW・本文テキストのコンテナに `oncopy` ハンドラを接続

## テスト
- `pnpm vitest run` 全件PASS
- `pnpm check` エラーなし
EOF
)"
```

- [ ] **Step 3: PR番号を確認し、ユーザーに報告する**

Run: `gh pr view --json url,number -q '.url'`

## Self-Review Notes

- **Spec coverage:** spec §1(元テキスト保持)→Task 3、§2(文字対応表)→Task 1、§3(copyハンドラ)→Task 2、§4(呼び出し側)→Task 4。テスト方針(spec「テスト」節)→各Taskのテストで反映。
- **Type consistency:** `buildNyaizeCharMap(original, nyaized): number[]` と `mapNyaizedRangeToOriginal(map, original, start, end): string` はTask 1で定義した名前・シグネチャのままTask 2で使用。`handleNyaizeCopy(event: ClipboardEvent): void` はTask 2定義のままTask 4でimportして使用。`data-original-text` 属性名はTask 3・spec・Task 2のセレクタ (`[data-original-text]`) で統一。
