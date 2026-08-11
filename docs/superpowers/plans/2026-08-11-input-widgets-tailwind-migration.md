# 入力系ウィジェットTailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の第6バッチとして、`TqlCompletionField.svelte`/`CompletionPopover.svelte`/`Sparkle.svelte`の手書きCSSをTailwindユーティリティクラスへ移行する。

**Architecture:** 各ファイルの`<style>`ブロックをTailwindユーティリティクラスへ置き換える。`Sparkle.svelte`のみ、動的な`--size`カスタムプロパティに依存する`@keyframes`アニメーションをTailwind非対応のため`<style>`に残す(既存バッチの`color-mix()`パターンと同じ扱い)。条件付きクラスの衝突(`invalid`/`selected`)は「1つの完全なクラス文字列を選ぶ三項演算子」で解消する。`<script>`ロジックは一切変更しない。

**Tech Stack:** Tailwind CSS v4、既存の`@theme`トークンブリッジ

## Global Constraints

- 各ファイルの`<script>`ブロックのロジックは一切変更しない
- surfaceカラーのマッピング規約: `--surface-1`→`bg-background`、`--surface-2`→`bg-muted`、`--text`→`text-foreground`、`--text-dim`→`text-muted-foreground`、`--accent`→`text-primary`、`--border`→`border-border`、`--danger`→`border-destructive`
- **条件付きクラスは必ず「1つの完全なクラス文字列を選ぶ三項演算子」の形にする。`class:`ディレクティブや`class={[...]}`配列で同じCSSプロパティを設定する複数クラスを個別にON/OFFする書き方は禁止**(#176/#178/#180で見つかった実バグと同じパターン)
- ピクセル値がTailwindの標準スペーシングスケールに正確に乗らない場合はアービトラリ値(`px-[9px]`等)を使う
- フォントファミリーの`inherit`は`font-[inherit]`、複数フォントのリストは`font-[ui-monospace,"Cascadia_Code","SF_Mono",monospace]`のようにアービトラリ値で表現する(カンマ区切りの複数フォント名はTailwindのアービトラリ`font-[...]`構文でそのまま渡せる。スペースは`_`に置換)
- `z-index`の意図を説明するコメント(`CompletionPopover.svelte`のz-[1010]について)はTailwind化後もそのままHTMLコメントとして残す

---

### Task 1: `TqlCompletionField.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/input/TqlCompletionField.svelte`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元(`AddColumnModal.svelte`)からの`mode`/`value`/`placeholder`/`rows`/`invalid`/`oninput`/`lists`/`antennas`/`channels` propsは変更しない

- [ ] **Step 1: `<textarea>`のクラスを置き換え(173〜197行目)**

```svelte
{#if mode === "query"}
  <textarea
    class={invalid
      ? 'rounded-lg border border-destructive bg-muted px-2.5 py-2 font-[ui-monospace,"Cascadia_Code","SF_Mono",monospace] text-[0.82rem] text-foreground resize-y'
      : 'rounded-lg border border-border bg-muted px-2.5 py-2 font-[ui-monospace,"Cascadia_Code","SF_Mono",monospace] text-[0.82rem] text-foreground resize-y'}
    {rows}
    {placeholder}
    bind:value
    bind:this={el}
    onkeydown={onKeydown}
    onkeyup={syncCursor}
    onclick={syncCursor}
    oninput={onInputHandler}
    oncompositionstart={() => (composing = true)}
    oncompositionend={() => {
      composing = false;
      syncCursor();
    }}
    onfocus={() => {
      focused = true;
      syncCursor();
    }}
    onblur={() => {
      focused = false;
      suppressAt = cursorPos;
    }}
  ></textarea>
{:else}
  <input
    class={invalid
      ? "rounded-lg border border-destructive bg-muted px-2.5 py-2 font-[inherit] text-foreground"
      : "rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"}
    {placeholder}
    bind:value
    bind:this={el}
    onkeydown={onKeydown}
    onkeyup={syncCursor}
    onclick={syncCursor}
    oninput={onInputHandler}
    oncompositionstart={() => (composing = true)}
    oncompositionend={() => {
      composing = false;
      syncCursor();
    }}
    onfocus={() => {
      focused = true;
      syncCursor();
    }}
    onblur={() => {
      focused = false;
      suppressAt = cursorPos;
    }}
  />
{/if}
```

補足: `resize: vertical`は`<textarea>`のみに元々付いていたプロパティのため`resize-y`は`<textarea>`側のクラス文字列にのみ含め、`<input>`側には付けない(元CSSの`textarea`/`input`セレクタが別々だった構造をそのまま維持)。

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(234〜257行目)を削除する。`color-mix()`等の変換不能パターンは含まれていないため不要になる。`{#if popoverOpen && popoverPos}...{/if}`ブロック(224〜232行目)は変更しない。

- [ ] **Step 3: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 4: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: このファイルにはテストが無いが、既存テスト(246/246)が壊れていないことを確認する

- [ ] **Step 5: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 6: Commit**

```bash
git add frontend/src/input/TqlCompletionField.svelte
git commit -m "style: TqlCompletionField.svelteをTailwindクラスに移行"
```

---

### Task 2: `CompletionPopover.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/CompletionPopover.svelte`
- Modify: `frontend/src/ui/CompletionPopover.test.ts`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元(`TqlCompletionField.svelte`/MFM補完系)からの`items`/`selectedIndex`/`left`/`top`/`onpick` propsは変更しない

- [ ] **Step 1: マークアップを置き換え(26〜51行目)**

```svelte
<!-- Modal.svelte/ConfirmDialog.svelte(z-[1000])より前面に出す必要がある。
     AddColumnModal(唯一のTqlCompletionField呼び出し元)が共通Modalを使うようになったため。 -->
<div
  class="fixed z-[1010] flex max-h-[260px] min-w-[160px] max-w-[min(320px,90vw)] flex-col overflow-y-auto rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
  data-testid="completion-popover"
  use:portal
  style={`left:${left}px;top:${top}px`}
  role="listbox"
>
  {#each items as item, i (item.key)}
    <button
      type="button"
      class={i === selectedIndex
        ? "flex w-full items-center gap-1.5 rounded-md bg-muted px-2 py-[5px] text-left font-[inherit] text-[0.82rem] text-primary"
        : "flex w-full items-center gap-1.5 rounded-md px-2 py-[5px] text-left font-[inherit] text-[0.82rem] text-foreground"}
      role="option"
      aria-selected={i === selectedIndex}
      bind:this={itemEls[i]}
      onmousedown={(e) => {
        // click ではなく mousedown を使い、かつ preventDefault することで
        // textarea の blur を発生させずに確定できるようにする(blurが先に走ると
        // ポップアップが閉じてクリックが空振りする)。
        e.preventDefault();
        onpick(i);
      }}
    >
      {#if item.thumbnail?.type === "custom" || item.thumbnail?.type === "avatar"}
        <img class="h-[18px] w-[18px] flex-none object-contain" src={item.thumbnail.url} alt={item.label} />
      {:else if item.thumbnail?.type === "unicode"}
        <span class="inline-flex h-[18px] w-[18px] flex-none items-center justify-center text-base">{item.thumbnail.char}</span>
      {/if}
      <span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{item.label}</span>
    </button>
  {/each}
</div>
```

- [ ] **Step 2: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(53〜109行目)を削除する。`color-mix()`は使われておらず(`.completion-item.selected`は素の`var(--surface-2)`/`var(--accent)`参照)不要になる。

- [ ] **Step 3: `CompletionPopover.test.ts`のセレクタを更新**

```diff
--- a/frontend/src/ui/CompletionPopover.test.ts
+++ b/frontend/src/ui/CompletionPopover.test.ts
@@
     const { baseElement } = render(CompletionPopover, {
       props: { items: [textItem], selectedIndex: 0, left: 42, top: 99, onpick: () => {} },
     });
-    const el = baseElement.querySelector(".completion-popover") as HTMLElement;
+    const el = baseElement.querySelector('[data-testid="completion-popover"]') as HTMLElement;
     expect(el.style.left).toBe("42px");
     expect(el.style.top).toBe("99px");
   });
```

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: `CompletionPopover.test.ts`の全テストが通る

- [ ] **Step 6: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/CompletionPopover.svelte frontend/src/ui/CompletionPopover.test.ts
git commit -m "style: CompletionPopover.svelteをTailwindクラスに移行"
```

---

### Task 3: `Sparkle.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/render/Sparkle.svelte`
- Modify: `frontend/src/render/Sparkle.test.ts`

**Interfaces:**
- Consumes: なし
- Produces: 見た目・挙動は現状維持。呼び出し元(`MfmNode.svelte`、`$[sparkle]`装飾時)からの`children` snippetは変更しない

- [ ] **Step 1: マークアップを置き換え(67〜82行目)**

```svelte
<span class="relative inline-block" bind:this={host}>
  {@render children()}
  {#if !reduced}
    <span class="pointer-events-none absolute inset-0 overflow-visible" data-testid="sparkle-layer" aria-hidden="true">
      {#each particles as p (p.id)}
        <svg
          class="particle"
          viewBox="0 0 64 64"
          style={`left:${p.x}px;top:${p.y}px;--size:${p.size};animation-duration:${p.duration}ms;fill:${p.color}`}
        >
          <path d={STAR} />
        </svg>
      {/each}
    </span>
  {/if}
</span>
```

- [ ] **Step 2: `<style>`ブロックを縮小**

`<style>...</style>`ブロック全体(84〜117行目)を以下に置き換える(`.particle`と`@keyframes`のみ残す。`.mfm-sparkle`/`.layer`は削除):

```svelte
<style>
  .particle {
    position: absolute;
    width: 64px;
    height: 64px;
    /* 64px 箱の中心を left/top の座標に合わせる（transform はアニメが使うため margin で補正） */
    margin: -32px 0 0 -32px;
    transform: scale(0);
    animation-name: mfm-sparkle-particle;
    animation-timing-function: linear;
    animation-iteration-count: 1;
  }
  @keyframes mfm-sparkle-particle {
    0% {
      transform: rotate(0deg) scale(0);
    }
    50% {
      transform: rotate(180deg) scale(var(--size));
    }
    100% {
      transform: rotate(360deg) scale(0);
    }
  }
</style>
```

- [ ] **Step 3: `Sparkle.test.ts`のセレクタを更新**

```diff
--- a/frontend/src/render/Sparkle.test.ts
+++ b/frontend/src/render/Sparkle.test.ts
@@
   it("does not render the particle layer when reduced motion is preferred", () => {
     mockMatchMedia(true);
     const { container } = render(Sparkle, { props: { children: textSnippet("hi") } });
-    expect(container.querySelector(".layer")).toBeNull();
+    expect(container.querySelector('[data-testid="sparkle-layer"]')).toBeNull();
   });

   it("renders the particle layer when reduced motion is not preferred", () => {
     mockMatchMedia(false);
     const { container } = render(Sparkle, { props: { children: textSnippet("hi") } });
-    expect(container.querySelector(".layer")).not.toBeNull();
+    expect(container.querySelector('[data-testid="sparkle-layer"]')).not.toBeNull();
   });
```

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: `Sparkle.test.ts`の全テストが通る

- [ ] **Step 6: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 7: Commit**

```bash
git add frontend/src/render/Sparkle.svelte frontend/src/render/Sparkle.test.ts
git commit -m "style: Sparkle.svelteをTailwindクラスに移行"
```

---

### 手動確認(全タスク完了後)

リポジトリルートから`cargo tauri dev`を起動し、以下を確認する:
- カラム/タブ追加モーダルのエキスパート(TQL)モードで、`list(`等の入力途中に補完ポップアップが表示・選択・確定できること
- 無効なTQL入力時に枠線が赤系(destructive)に変わること
- MFMの`$[sparkle]`装飾(✨演出)がタイムライン上のノートで従来通りアニメーション表示されること
- ライト/ダーク両テーマ
