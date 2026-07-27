# エラーモーダル化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ComposeBar.svelte` の投稿・添付エラーを、目立たない赤字/ツールチップ表示から、明示的に閉じられるモーダルダイアログ表示に変える（Issue #123）。

**Architecture:** `AddColumnModal.svelte` が持つ overlay/modal CSS パターンを汎用コンポーネント `Modal.svelte` として切り出し、`ComposeBar.svelte` の `err` state 表示をそのモーダルに置き換える。`AddColumnModal.svelte` 自体はスコープ外で変更しない。

**Tech Stack:** Svelte 5 (runes, snippet), TypeScript, `@lucide/svelte`（Xアイコン）。フロントエンドに単体テストランナーは無いため（`pnpm check` は svelte-check + tsc のみ）、検証は型チェックと `cargo tauri dev` での手動確認で行う。

## Global Constraints

- 会話・コミットメッセージは日本語（プロジェクト方針）。
- コミットメッセージはサブジェクト行のみ、本文なし（ユーザーのグローバル設定）。
- 編集前に必ずフィーチャーブランチを切る。今回は既に `feat/error-modal-123` ブランチ上で作業する（追加のブランチ作成は不要）。
- `AddColumnModal.svelte` は変更しない（スコープ外）。
- モーダルの閉じるボタンのラベルは「わかった」（ユーザー指定）。
- 型チェックは `cd frontend && pnpm check` で通すこと。

---

### Task 1: 汎用 `Modal.svelte` コンポーネントを作成する

**Files:**
- Create: `frontend/src/ui/Modal.svelte`

**Interfaces:**
- Consumes: なし（新規コンポーネント）。
- Produces: `Modal.svelte` のprops — `title: string`、`onclose: () => void`、`children: Snippet`。呼び出し側は `<Modal title="..." onclose={...}>{#snippet children()}...{/snippet}</Modal>` の形で使う（Task 2 で使用）。

- [ ] **Step 1: `Modal.svelte` を作成する**

`AddColumnModal.svelte` の overlay/modal パターン（337-343行目、560-573行目のCSS）を汎用化する。

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";

  let { title, onclose, children }: { title: string; onclose: () => void; children: Snippet } =
    $props();
</script>

<div class="overlay" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>{title}</span>
      <button class="x" onclick={onclose}><X size={16} /></button>
    </header>
    {@render children()}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: start center;
    padding-top: 8vh;
    z-index: 50;
  }
  .modal {
    width: min(480px, 92vw);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    margin-bottom: 12px;
  }
  .x {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
</style>
```

- [ ] **Step 2: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし（`Modal.svelte` はまだどこからも import されていないため、未使用による警告は出ない）。

- [ ] **Step 3: コミットする**

```bash
git add frontend/src/ui/Modal.svelte
git commit -m "feat: 汎用Modalコンポーネントを追加"
```

---

### Task 2: `ComposeBar.svelte` のエラー表示をモーダルに置き換える

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`

**Interfaces:**
- Consumes: Task 1 で作成した `Modal`（`title`, `onclose`, `children` snippet の props）。
- Produces: なし（末端のUI変更）。

- [ ] **Step 1: `Modal` を import する**

`frontend/src/ui/ComposeBar.svelte` の9行目付近、既存の import 群に追加する。

```ts
import Modal from "./Modal.svelte";
```

- [ ] **Step 2: ツールバー内の `!` アイコン表示を削除する**

`frontend/src/ui/ComposeBar.svelte:424` の以下の行を削除する。

```svelte
      {#if err}<span class="err" title={err}>!</span>{/if}
```

- [ ] **Step 3: 添付サムネイルの `!` バッジからツールチップ依存を外す**

`frontend/src/ui/ComposeBar.svelte:350` を以下に変更する（`title` 属性を削除し、位置マーカーとしてのみ残す）。

Before:
```svelte
            <span class="thumb-status error" title={err ?? "アップロードに失敗しました"}>!</span>
```

After:
```svelte
            <span class="thumb-status error">!</span>
```

- [ ] **Step 4: エラーモーダルを追加する**

`frontend/src/ui/ComposeBar.svelte` の431行目（`</div>` で `.composewrap` が閉じた直後、433行目の `{#if showAttachMenu ...}` の前）に以下を追加する。

```svelte
{#if err}
  <Modal title="エラー" onclose={() => (err = null)}>
    {#snippet children()}
      <p class="err-body">{err}</p>
      <div class="err-actions">
        <button class="err-ok" onclick={() => (err = null)}>わかった</button>
      </div>
    {/snippet}
  </Modal>
{/if}
```

- [ ] **Step 5: モーダル本文/ボタンのスタイルを追加する**

`frontend/src/ui/ComposeBar.svelte` の `<style>` ブロック内、既存の `.err` ルール（745-749行目）の直後に追加する。

```css
  .err-body {
    color: var(--text);
    font-size: 0.9rem;
    margin: 0 0 14px;
    word-break: break-word;
    white-space: pre-wrap;
  }
  .err-actions {
    display: flex;
    justify-content: flex-end;
  }
  .err-ok {
    padding: 8px 20px;
    border: none;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
```

既存の `.err` ルール（トグルボタン以外では使われなくなるツールバー用スタイル）は、Step 2 でその利用箇所を削除したため未使用になる。同じ `<style>` ブロック内で `.err` を検索し、他に使用箇所がなければ削除する。

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 7: `cargo tauri dev` で手動確認する**

Run: `cargo tauri dev`

確認項目:
1. アカウント未選択のまま投稿ボタンを押す → 「エラー」モーダルが開き、本文に「アカウントを選択してください」と表示される。
2. モーダル下部の「わかった」ボタンを押す → モーダルが閉じる。
3. 別途エラーを発生させた後、Escapeキー、または背景クリックでもモーダルが閉じることを確認する。
4. 添付ファイルのアップロードを失敗させる（存在しないファイルパス等は難しいため、ネットワーク切断や無効なアカウントでのアップロード試行などで代替可）→ サムネイル上に赤い `!` バッジが表示され、かつ「エラー」モーダルにメッセージが出ることを確認する。
5. ツールバーに元の赤い `!` アイコンが表示されないことを確認する。

- [ ] **Step 8: コミットする**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: 投稿エラーをモーダル表示に変更"
```

---

## Self-Review Notes

- **Spec coverage:** Modal.svelte 抽出（Task 1）、ツールバーの`!`アイコン削除・添付バッジのツールチップ除去・モーダル追加（Task 2 Step 2-5）、「わかった」ボタン（Task 2 Step 4）、既存の `err = null` リセット箇所は変更不要なので触れていない（spec通り維持）、手動確認（Task 2 Step 7）— すべて spec の各項目に対応するタスクがある。
- **Placeholder scan:** 全ステップに具体的なコード・コマンド・期待結果を記載済み。
- **Type consistency:** `Modal` の props (`title`, `onclose`, `children`) は Task 1 の定義と Task 2 の使用箇所で一致している。`err` は既存の `let err = $state<string | null>(null)` をそのまま使う。
