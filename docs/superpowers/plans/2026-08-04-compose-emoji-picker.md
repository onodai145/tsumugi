# 投稿欄への絵文字ピッカー追加 (Issue #20) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 投稿欄 (`ComposeBar.svelte`) に絵文字ピッカーボタンを追加し、既存の `ReactionPicker.svelte` から絵文字をブラウズして本文に挿入できるようにする。

**Architecture:** 既存の `ReactionPicker.svelte` をそのまま再利用し、`ComposeBar.svelte` の添付メニュー (`showAttachMenu`) と同じ portal + オーバーレイのポップオーバーパターンで表示する。`ReactionPicker.onpick` が返すリアクションキー形式 (`:name@.:` / Unicode文字) を、投稿本文向けのMFMショートコード形式 (`:name:` / Unicode文字) に変換する小さな純関数を先に実装し、テストで固める。

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest + @testing-library/svelte

## Global Constraints

- 新規コンポーネントは作らない。既存の `ReactionPicker.svelte` をそのまま再利用する（spec）。
- 変更対象は `frontend/src/ui/ComposeBar.svelte` と `frontend/src/lib/emojiKey.ts` のみ（spec: 他の絵文字ピッカー呼び出し箇所には手を入れない）。
- 絵文字選択後もピッカーは自動で閉じない（ユーザー承認済みの挙動）。
- 挿入位置は現在のカーソル位置 (`cursorPos`)。選択範囲の置換はしない、常に単純挿入。
- コミットメッセージは件名のみ（本文・箇条書きなし）。

---

### Task 1: リアクションキー→挿入テキスト変換ヘルパー

**Files:**
- Modify: `frontend/src/lib/emojiKey.ts`
- Test: `frontend/src/lib/emojiKey.test.ts`

**Interfaces:**
- Consumes: `isCustomEmojiKey(key: string): boolean`, `parseCustomEmojiPinKey(key: string): { name: string; host: string | null }`（同ファイル内の既存関数）
- Produces: `emojiKeyToInsertText(key: string): string` — Task 2 の `ComposeBar.svelte` から呼び出す。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/emojiKey.test.ts` の末尾に追記:

```ts
describe("emojiKeyToInsertText", () => {
  it("converts a custom emoji key into a plain :name: shortcode", () => {
    expect(emojiKeyToInsertText(":blob_cat@.:")).toBe(":blob_cat:");
  });

  it("strips the host from a custom emoji key with an explicit host", () => {
    expect(emojiKeyToInsertText(":blob_cat@misskey.io:")).toBe(":blob_cat:");
  });

  it("returns a plain unicode emoji unchanged", () => {
    expect(emojiKeyToInsertText("😺")).toBe("😺");
  });
});
```

ファイル先頭の import 文を更新:

```ts
import {
  customEmojiKey,
  customEmojiPinKey,
  emojiKeyToInsertText,
  isCustomEmojiKey,
  parseCustomEmojiPinKey,
} from "./emojiKey";
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/emojiKey.test.ts`
Expected: FAIL — `emojiKeyToInsertText is not a function` (or similar import error)

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/emojiKey.ts` の末尾に追記:

```ts
// ReactionPicker.onpick が返すリアクションキー形式(Unicode文字 or ":name@host:")を、
// 投稿本文へ挿入するMFMショートコード形式に変換する。カスタム絵文字はhost部分を捨てて
// ":name:" にする(投稿本文中のショートコードにhostは含めない)。Unicodeはそのまま返す。
export function emojiKeyToInsertText(key: string): string {
  if (!isCustomEmojiKey(key)) return key;
  const { name } = parseCustomEmojiPinKey(key);
  return `:${name}:`;
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm vitest run src/lib/emojiKey.test.ts`
Expected: PASS（全ケース）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/emojiKey.ts frontend/src/lib/emojiKey.test.ts
git commit -m "feat: リアクションキーを投稿欄挿入テキストに変換するヘルパーを追加"
```

---

### Task 2: ComposeBarへの絵文字ピッカーボタン追加

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: `emojiKeyToInsertText(key: string): string`（Task 1）, `ReactionPicker` component (`props: { accountId: string; onpick: (reaction: string) => void; showPinned?: boolean }`, from `frontend/src/input/ReactionPicker.svelte`), `portal` action (`frontend/src/lib/portal.ts`)
- Produces: なし（末端タスク）

- [ ] **Step 1: import を追加する**

`frontend/src/ui/ComposeBar.svelte` の import ブロックを変更:

```ts
  import { ImagePlus, SmilePlus, X } from "@lucide/svelte";
```
（既存の `import { ImagePlus, X } from "@lucide/svelte";` を上記に置き換え）

さらに以下を追加:

```ts
  import ReactionPicker from "../input/ReactionPicker.svelte";
  import { emojiKeyToInsertText } from "../lib/emojiKey";
```

- [ ] **Step 2: ピッカーの開閉状態と位置を管理する state を追加する**

`showDrivePicker` の宣言 (`let showDrivePicker = $state(false);`) の直後に追加:

```ts
  let showEmojiPicker = $state(false);
  let emojiPickerTrigger = $state<HTMLElement | undefined>(undefined);
  let emojiPickerPos = $state<{ left: number; top: number } | null>(null);

  function toggleEmojiPicker() {
    if (showEmojiPicker) {
      showEmojiPicker = false;
      return;
    }
    const r = emojiPickerTrigger?.getBoundingClientRect();
    if (r) emojiPickerPos = { left: r.left, top: r.bottom + 4 };
    showEmojiPicker = true;
  }
```

- [ ] **Step 3: 選択した絵文字をカーソル位置へ挿入する関数を追加する**

`syncCursor` 関数の直後（`onTextareaInput` の前）に追加:

```ts
  async function insertEmoji(reactionKey: string) {
    const insertText = emojiKeyToInsertText(reactionKey);
    const pos = cursorPos;
    text = text.slice(0, pos) + insertText + text.slice(pos);
    const newPos = pos + insertText.length;
    suppressAt = newPos;
    await tick();
    textarea?.setSelectionRange(newPos, newPos);
    textarea?.focus();
    cursorPos = newPos;
  }
```

（`tick` は既に `svelte` から import 済み、`confirmCompletion` と同じパターンを踏襲）

- [ ] **Step 4: ツールバーに絵文字ピッカーボタンとポップオーバーを追加する**

`.tools.left` 内、`CW` ボタンの直前（画像添付ボタンの直後）にボタンを追加。該当箇所（現状）:

```svelte
      <button
        class="icon"
        title="画像を添付"
        bind:this={attachTrigger}
        onclick={toggleAttachMenu}
        disabled={busy}
      ><ImagePlus size={16} /></button>
      <button class="mini" class:active={useCw} onclick={() => (useCw = !useCw)}>CW</button>
```

これを次のように変更:

```svelte
      <button
        class="icon"
        title="画像を添付"
        bind:this={attachTrigger}
        onclick={toggleAttachMenu}
        disabled={busy}
      ><ImagePlus size={16} /></button>
      <button
        class="icon"
        class:active={showEmojiPicker}
        title="絵文字を挿入"
        bind:this={emojiPickerTrigger}
        onclick={toggleEmojiPicker}
        disabled={busy || !accountId}
      ><SmilePlus size={16} /></button>
      <button class="mini" class:active={useCw} onclick={() => (useCw = !useCw)}>CW</button>
```

ポップオーバー自体は、既存の `{#if showAttachMenu && attachMenuPos}` ブロックの直後（`{#if showDrivePicker && accountId}` の前）に追加:

```svelte
{#if showEmojiPicker && emojiPickerPos && accountId}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="attach-overlay" use:portal onclick={() => (showEmojiPicker = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="emoji-picker-pop"
      style={`left:${emojiPickerPos.left}px;top:${emojiPickerPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="presentation"
    >
      <ReactionPicker accountId={accountId} onpick={insertEmoji} />
    </div>
  </div>
{/if}
```

- [ ] **Step 5: ポップオーバーの位置指定用CSSを追加する**

`<style>` ブロック内、`.attach-menu` のスタイル定義の直後に追加:

```css
  .emoji-picker-pop {
    position: fixed;
  }
```

（`.icon.active` のスタイルは既存の `.mini.active` を流用せず、`.icon` に `active` 修飾がまだ無いため、`.icon` の定義の直後に追加）

```css
  .icon.active {
    border-color: var(--accent);
    color: var(--accent);
  }
```

- [ ] **Step 6: 型チェックを通す**

Run: `cd frontend && pnpm check`
Expected: エラーなし（既存の警告のみ許容）

- [ ] **Step 7: 動作確認（手動）**

Run: `cargo tauri dev`

確認項目:
- 投稿欄ツールバーに絵文字ボタンが表示される
- クリックでピッカーが開き、ピン留め/最近使った/カスタム絵文字/Unicode絵文字が表示される
- カスタム絵文字を選ぶと本文に `:name:` が挿入され、ピッカーは開いたままである
- Unicode絵文字を選ぶと本文にその文字が挿入され、ピッカーは開いたままである
- 続けて2つ目の絵文字を選ぶと、直前に挿入した位置の直後に追加される（先頭に戻らない）
- ピッカーの外側をクリックすると閉じる
- ボタンを再クリックすると閉じる
- アカウント未選択時はボタンが disabled になる
- モバイル投稿モーダル（`expanded=true` で使われる箇所）でも同様に動作する

- [ ] **Step 8: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: 投稿欄に絵文字ピッカーを追加"
```
