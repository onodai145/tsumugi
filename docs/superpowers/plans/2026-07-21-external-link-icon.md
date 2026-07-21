# 外部リンクアイコン付与 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノート本文の MFM `url`/`link` リンクに外部リンクアイコンを付与し、外部へ遷移することを視覚的に明示する(Issue #97)。

**Architecture:** `frontend/src/render/MfmNode.svelte` の `url`/`link` ノード描画に `@lucide/svelte` の `ExternalLink` アイコンを追記し、`frontend/src/app.css` に最小限のレイアウト用スタイルを追加する。ロジック分岐・Rust側の変更は無い。

**Tech Stack:** Svelte 5 (runes), `@lucide/svelte`, plain CSS (`frontend/src/app.css`)。

## Global Constraints

- 対象ノードは MFM `url` ノードと `link` ノードのみ。`mention`・`hashtag` ノードは変更しない。
- `docs/superpowers/specs/2026-07-21-external-link-icon-design.md` の設計に従う。

---

### Task 1: MfmNode.svelte に外部リンクアイコンを追加

**Files:**
- Modify: `frontend/src/render/MfmNode.svelte:1-9` (import追加)
- Modify: `frontend/src/render/MfmNode.svelte:86-90` (`url`/`link` ノード描画)
- Modify: `frontend/src/app.css:110`付近 (`.mfm-link`/`.mfm-link-icon` スタイル追加)

**Interfaces:**
- Consumes: 既存の `MfmNode` 型、`p.url`(リンク先URL)、`children`(link ノードのラベル部分)。
- Produces: `.mfm-link` を `<a>` に付与したまま、末尾に `<ExternalLink class="mfm-link-icon" size={12} />` を追加する。他ファイルからの参照はない。

このコンポーネントはロジック分岐を持たない純粋な表示変更のため、Rust側のユニットテストや自動テストは対象外。`pnpm check` の型チェックと `cargo tauri dev` での目視確認で検証する。

- [ ] **Step 1: import 文に `ExternalLink` を追加**

`frontend/src/render/MfmNode.svelte` の先頭 import 群に以下を追加する(既存の他 import の直後、6行目付近):

```svelte
  import { ExternalLink } from "@lucide/svelte";
```

- [ ] **Step 2: `url` ノードの描画にアイコンを追加**

`frontend/src/render/MfmNode.svelte:87` の既存行:

```svelte
  <a class="mfm-link" href={p.url} target="_blank" rel="noreferrer noopener">{p.url}</a>
```

を以下に置き換える:

```svelte
  <a class="mfm-link" href={p.url} target="_blank" rel="noreferrer noopener">{p.url}<ExternalLink class="mfm-link-icon" size={12} /></a>
```

- [ ] **Step 3: `link` ノードの描画にアイコンを追加**

`frontend/src/render/MfmNode.svelte:90` の既存行:

```svelte
  <a class="mfm-link" href={p.url} target="_blank" rel="noreferrer noopener">{#each children as c}<Self node={c} {emojis} />{/each}</a>
```

を以下に置き換える:

```svelte
  <a class="mfm-link" href={p.url} target="_blank" rel="noreferrer noopener">{#each children as c}<Self node={c} {emojis} />{/each}<ExternalLink class="mfm-link-icon" size={12} /></a>
```

- [ ] **Step 4: app.css に `.mfm-link`/`.mfm-link-icon` スタイルを追加**

`frontend/src/app.css` の `.mfm-mention {` ブロック(110行目)の直前に以下を挿入する:

```css
.mfm-link {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
.mfm-link-icon {
  flex-shrink: 0;
  opacity: 0.7;
}
```

- [ ] **Step 5: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: エラーなしで終了する。

- [ ] **Step 6: 実機で目視確認**

Run: `cargo tauri dev`

URLまたは `?[label](url)` 形式のリンクを含むノートをタイムラインに表示し、リンク末尾に外部リンクアイコンが表示されること、クリックで正しいURLに遷移することを確認する。

- [ ] **Step 7: コミット**

```bash
git add frontend/src/render/MfmNode.svelte frontend/src/app.css
git commit -m "feat: ノート内リンクに外部リンクアイコンを付与"
```
