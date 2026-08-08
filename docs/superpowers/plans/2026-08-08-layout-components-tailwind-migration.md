# レイアウト系コンポーネントのTailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の第1バッチとして、`Column.svelte`/`Pane.svelte`/`Backstage.svelte`の手書きCSSをTailwindユーティリティクラスへ置き換え、shadcn-svelteのButtonプリミティブを初めて導入する。

**Architecture:** 各コンポーネントの`<style>`ブロックを、静的なスタイル(padding/border/color/flexなど)はTailwindユーティリティクラスに、JS計算値によるインラインstyle(幅リサイズ等)と`color-mix()`によるアクセント/不透明度表現は最小化した`<style>`ブロックに残す形で置き換える。Buttonプリミティブは`shadcn-svelte add button`で追加し、参照するCSS変数が既存の`@theme`ブリッジ(Issue #170で導入済み)と整合することを確認済み(下記Task 1参照)。

**Tech Stack:** Tailwind CSS v4, shadcn-svelte(Button primitive), 既存の`@theme`トークンブリッジ

## Global Constraints

- ドラッグ&ドロップ、リサイズ、スクロール検知(`onScroll`)等の既存の振る舞い・イベントハンドラは一切変更しない
- `color-mix(in srgb, var(--surface-*) var(--column-opacity, 100%), transparent)`のような、ユーザー設定`--column-opacity`に依存する背景表現はTailwindユーティリティに変換せず、`<style>`に残す(他バッチにも共通する既知の例外)
- Rust側・`theme.ts`・`@theme`ブリッジ(`frontend/src/app.css`)は変更しない
- `data-state`/`data-level`属性によるスタイル分岐は、可能な限りTailwindの`data-[state=...]:`アービトラリバリアントに置き換える
- surface色のTailwindクラスへのマッピングは全タスクで統一する: `--surface-1` → `bg-background`/`text-background`等、`--surface-2` → `bg-card`、`--surface-3` → `bg-popover`(いずれも`@theme`ブリッジで同じ実値になる)
- `--text` → `text-foreground`、`--text-dim` → `text-muted-foreground`、`--accent` → `bg-primary`/`text-primary`/`border-primary`等、`--danger` → `bg-destructive`/`text-destructive`、`--border` → `border-border`。`--success`/`--warning`/`--info`は`@theme`ブリッジに含まれないため、Tailwindのアービトラリ値記法(例: `text-[var(--success)]`)を使う

---

### Task 1: shadcn Buttonプリミティブの導入と検証

**Files:**
- Create: `frontend/src/lib/components/ui/button/button.svelte`(shadcn-svelte CLI生成)
- Create: `frontend/src/lib/components/ui/button/index.ts`(shadcn-svelte CLI生成)
- Test: 手動検証(下記Step参照)

**Interfaces:**
- Consumes: 既存の`@theme`ブリッジ(`frontend/src/app.css`、Issue #170で導入済み)、`frontend/src/lib/utils.ts`の`cn()`
- Produces: `import { Button } from "$lib/components/ui/button";` で使えるButtonコンポーネント。Props: `variant?: "default" | "outline" | "secondary" | "ghost" | "destructive" | "link"`、`size?: "default" | "xs" | "sm" | "lg" | "icon" | "icon-xs" | "icon-sm" | "icon-lg"`、`class?: string`(他のHTMLButtonAttributes/HTMLAnchorAttributesも受け付ける)。Task 3・Task 4がこれを使う

- [ ] **Step 1: shadcn-svelte CLIでButtonを追加**

```bash
cd frontend
pnpm dlx shadcn-svelte@latest add button --yes
```

- [ ] **Step 2: 生成されたコンポーネントが参照するCSS変数を確認**

`frontend/src/lib/components/ui/button/button.svelte`を開き、`buttonVariants`(`tv()`呼び出し)内で使われている`bg-*`/`text-*`/`border-*`/`ring-*`クラス名を全て書き出し、それぞれが`frontend/src/app.css`の`@theme`ブロック(Issue #170で追加済み、`--color-background`/`--color-primary`/`--color-secondary`/`--color-muted`/`--color-destructive`/`--color-border`/`--color-input`/`--color-ring`等)でカバーされていることを確認する。

期待される結果(この計画作成時点で事前検証済み): 全て`@theme`ブリッジでカバーされており、`app.css`の変更は不要。`rounded-[min(var(--radius-md),8px)]`のような`--radius-md`参照は、`@import "tailwindcss/theme.css"`が提供するTailwind本体のデフォルトトークン(`--radius-md: 0.375rem`)がそのまま使われるため問題ない。

もし事前検証と異なり、`@theme`ブリッジでカバーされていないCSS変数参照(例: `--color-chart-1`等、shadcn CLIのデフォルトプリセット由来のトークン)が見つかった場合は、そのユーティリティクラスを実際に使う予定が無ければ無視してよい(Tailwind v4は未使用の`@theme`変数参照をツリーシェイクするため、使われなければビルドに影響しない)。Task 3・Task 4で実際に使うクラス(`bg-primary`/`text-primary-foreground`/`bg-secondary`/`text-secondary-foreground`/`border-border`/`bg-background`等)が正しく解決されることだけを確認すればよい。

- [ ] **Step 3: `frontend/src/app.css`に意図しない変更が無いことを確認**

Run: `cd frontend && git diff --stat -- src/app.css`
Expected: 出力が空(Issue #170のTask 2で起きたCLI事故のように、`app.css`が意図せず上書きされていないことを確認する)

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/components frontend/package.json frontend/pnpm-lock.yaml
git commit -m "feat: shadcn-svelteのButtonプリミティブを追加"
```

---

### Task 2: `Pane.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/Pane.svelte`

**Interfaces:**
- Consumes: なし(Task 1と独立して実施可能)
- Produces: 見た目・挙動は現状維持。`<style>`ブロックは完全に削除される

現在の`Pane.svelte`は以下の内容(全文、56〜90行目が`<style>`ブロック):

```svelte
{#if node.type === "leaf"}
  {@const group = app.groups.find((g) => g.id === node.groupId)}
  {#if group}
    <Column {group} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {stretch} />
  {/if}
{:else if node.direction === "row"}
  <div class="row">
    {#each node.children as child (child.node.id)}
      {#if child.node.type === "leaf"}
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
      {:else}
        <div class="row-item" style={child.auto ? "flex:1 1 0;min-width:220px" : `flex:0 0 ${child.size}px`}>
          <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <div class="col">
    {#each node.children as child (child.node.id)}
      <div class="col-item" style={child.auto ? "flex:1 1 0" : `flex:0 0 ${child.size}%`}>
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} stretch={true} />
      </div>
    {/each}
  </div>
{/if}

<style>
  .row {
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    overflow-x: auto;
  }
  .col {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    height: 100%;
    min-height: 0;
  }
  .col-item {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
  .row-item {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    min-width: 0;
  }
</style>
```

- [ ] **Step 1: マークアップと`<style>`ブロックを置き換え**

`<script>`ブロックは変更しない。`{#if node.type === "leaf"}`以降を以下に置き換える(`<style>`ブロックは削除):

```svelte
{#if node.type === "leaf"}
  {@const group = app.groups.find((g) => g.id === node.groupId)}
  {#if group}
    <Column {group} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {stretch} />
  {/if}
{:else if node.direction === "row"}
  <!-- flex-auto(flex:1 1 auto)は親(.columns、flex-direction:row)のflex子として残り幅
       いっぱいに広がるために必須。flex-1(flex:1 1 0)だとコンテンツ幅にshrink-to-fitし、
       内部のauto幅Columnがビューポート幅ではなく縮んだ幅を基準に均等割りしようとして破綻する
       (ウィンドウ幅を変えるたびbroken widthが変わって見えるのはこれが原因)。 -->
  <div class="flex flex-auto min-w-0 h-full overflow-x-auto">
    {#each node.children as child (child.node.id)}
      {#if child.node.type === "leaf"}
        <!-- Leafの幅は今まで通りColumn.svelte側(ColumnGroup.width/auto)が決める。
             child.size/autoは(このSliceでは)Leafの実際の幅とは同期していないため、
             ここでラップして使うと二重管理・食い違いの原因になる。 -->
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
      {:else}
        <!-- ネストしたSplit(例: 下に分割された塊)にはColumn.svelteに相当する幅指定元が
             無いため、PaneChild.size/autoをそのままflex指定に使う。 -->
        <div
          class="flex flex-col h-full min-h-0 min-w-0"
          style={child.auto ? "flex:1 1 0;min-width:220px" : `flex:0 0 ${child.size}px`}
        >
          <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <div class="flex flex-col flex-auto h-full min-h-0">
    {#each node.children as child (child.node.id)}
      <div class="flex flex-col min-h-0 min-w-0" style={child.auto ? "flex:1 1 0" : `flex:0 0 ${child.size}%`}>
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} stretch={true} />
      </div>
    {/each}
  </div>
{/if}
```

- [ ] **Step 2: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 3: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/Pane.svelte
git commit -m "style: Pane.svelteをTailwindクラスに移行"
```

---

### Task 3: `Backstage.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/Backstage.svelte`

**Interfaces:**
- Consumes: Task 1の`Button`(`$lib/components/ui/button`)
- Produces: 見た目・挙動は現状維持(トグル/クリア/再認証ボタンはButtonプリミティブ使用に変わるが、視覚的には既存とほぼ同じになるようクラスで調整する)

- [ ] **Step 1: `<script>`にButtonのimportを追加**

`frontend/src/ui/Backstage.svelte`の`<script>`ブロック冒頭の import 群に追加:

```ts
import { Button } from "$lib/components/ui/button";
```

- [ ] **Step 2: マークアップをTailwindクラスに置き換え**

45〜90行目のマークアップ全体を以下に置き換える(`<script>`ブロックのロジックは一切変更しない):

```svelte
<div class="flex flex-col flex-none border-t border-border bg-card" class:open>
  {#if open}
    <div class="h-[min(38vh,320px)] overflow-y-auto border-b border-border bg-background font-mono text-[0.76rem]">
      {#if app.logs.length === 0}
        <div class="p-3.5 text-center text-muted-foreground">ログはまだありません</div>
      {:else}
        {#each app.logs as l (l.id)}
          {@const Ic = icon[l.level]}
          <div class="flex items-baseline gap-2 px-2.5 py-0.5 hover:bg-card" data-level={l.level}>
            <span
              class={[
                "inline-flex flex-none",
                {
                  "text-[var(--success)]": l.level === "success",
                  "text-[var(--warning)]": l.level === "warn",
                  "text-destructive": l.level === "error",
                  "text-muted-foreground": l.level === "info",
                },
              ]}
            ><Ic size={12} /></span>
            <span class="flex-none text-muted-foreground">{hhmmss(l.at)}</span>
            <span class="flex-1 break-words">{l.text}</span>
            {#if l.reauthAccountId}
              <Button variant="outline" size="xs" onclick={() => onReauth(l.reauthAccountId!)}>再認証</Button>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <div
    class="flex min-h-6 items-center gap-2 pt-[3px] pr-[max(8px,env(safe-area-inset-right))] pb-[max(3px,env(safe-area-inset-bottom))] pl-[max(8px,env(safe-area-inset-left))] text-[0.76rem]"
  >
    <Button variant="outline" size="xs" onclick={() => (open = !open)} title="操作ログ (Backstage)">
      {#if open}<ChevronDown size={13} />{:else}<ChevronUp size={13} />{/if} ログ
      {#if errorCount > 0}<span class="rounded-lg bg-destructive px-[5px] text-[0.68rem] leading-[1.4] text-white">{errorCount}</span>{/if}
    </Button>
    <div class="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden whitespace-nowrap" data-level={latest?.level ?? "info"}>
      {#if latest}
        {@const LatestIc = icon[latest.level]}
        <span
          class={[
            "inline-flex flex-none",
            {
              "text-[var(--success)]": latest.level === "success",
              "text-[var(--warning)]": latest.level === "warn",
              "text-destructive": latest.level === "error",
              "text-muted-foreground": latest.level === "info",
            },
          ]}
        ><LatestIc size={12} /></span>
        <span class="flex-none text-muted-foreground">{hhmmss(latest.at)}</span>
        <span class="overflow-hidden text-ellipsis">{latest.text}</span>
      {:else}
        <span class="overflow-hidden text-ellipsis text-muted-foreground">操作すると、ここに履歴が表示されます</span>
      {/if}
    </div>
    {#if open && app.logs.length > 0}
      <Button variant="ghost" size="xs" class="flex-none text-muted-foreground" onclick={() => app.clearLogs()}>クリア</Button>
    {/if}
    <div
      class="flex flex-none items-center gap-2.5 whitespace-nowrap text-muted-foreground [font-variant-numeric:tabular-nums]"
      title="DB件数 / 流速(件・分) / 起動からの経過時間"
    >
      <span class="inline-flex items-center gap-0.5"><Database size={12} />{app.noteCount.toLocaleString()}件</span>
      <span class="inline-flex items-center gap-0.5"><Activity size={12} />{app.noteRatePerMin}件/分</span>
      <span class="inline-flex items-center gap-0.5"><Clock size={12} />{elapsed}</span>
    </div>
  </div>
</div>
```

- [ ] **Step 3: `<style>`ブロックを削除**

92〜237行目の`<style>`ブロック全体を削除する。

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 6: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、以下を確認する:
- ログ開閉ボタンの見た目・クリック動作
- エラーバッジ(赤背景・件数)の表示
- ログレベルごとのアイコン色(info=灰, success=緑, warn=黄, error=赤)
- 再認証ボタン・クリアボタンの見た目・クリック動作
- 元の見た目(角丸4px、枠線、ホバー時の枠線ハイライト等)からの見た目の差異が気になる場合は、Buttonの`variant`/`size`/`class`調整で近づける

この目視確認の結果、明らかな見た目のズレがあれば、このタスクの中でクラス調整を行ってから次に進める。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/Backstage.svelte
git commit -m "style: Backstage.svelteをTailwindクラス+Buttonプリミティブに移行"
```

---

### Task 4: `Column.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 1の`Button`(`$lib/components/ui/button`)
- Produces: 見た目・挙動は現状維持。ドラッグ&ドロップ・リサイズのイベントハンドラ(`onResizeDown`/`onResizeMove`/`onResizeUp`/`ondragover`等)は一切変更しない

- [ ] **Step 1: `<script>`にButtonのimportを追加**

`frontend/src/ui/Column.svelte`の`<script>`ブロック冒頭のimport群に追加:

```ts
import { Button } from "$lib/components/ui/button";
```

- [ ] **Step 2: マークアップをTailwindクラスに置き換え**

58〜165行目のマークアップを以下に置き換える(`<script>`ブロックのロジック・イベントハンドラは一切変更しない)。`.column`/`.tabbar`の背景(`color-mix`)とフォーカス時のアクセント枠線は`<style>`に残す(下記Step 3):

```svelte
<section
  class="group relative flex flex-none flex-col h-full border-r border-border col-bg"
  style={stretch ? "flex:1 1 0;min-width:0" : group.auto ? "flex:1 1 0;min-width:220px" : `width:${group.width}px`}
  class:opacity-55={app.draggingGroupId === group.id}
  class:focused={app.focusedGroupId === group.id}
  ondragover={(e) => {
    e.preventDefault();
    app.dragOverGroup(group.id);
  }}
  role="group"
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="tabbar-bg flex min-h-[26px] items-stretch gap-px overflow-x-auto border-b border-border border-t-2"
    ondragover={(e) => {
      if (app.draggingTabId) {
        e.preventDefault();
        app.dragOverTabBarEnd(group.id);
      }
    }}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span
      class="flex w-[26px] flex-none cursor-grab select-none items-center justify-center text-muted-foreground active:cursor-grabbing"
      draggable="true"
      ondragstart={(e) => {
        e.dataTransfer?.setData("text/plain", group.id);
        app.startDragGroup(group.id);
      }}
      ondragend={() => app.endDragGroup()}
      ondblclick={() => onEditGroup(group.id)}
      title="ドラッグでカラムを並べ替え（ダブルクリックでカラム設定）"
    ><GripVertical size={14} /></span>

    {#each group.tabs as t (t.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class={[
          "flex cursor-grab items-center active:cursor-grabbing",
          {
            "shadow-[inset_0_-2px_0_var(--color-primary)]": t.id === group.activeTabId,
            "opacity-65": t.id !== group.activeTabId,
            "opacity-40": app.draggingTabId === t.id,
          },
        ]}
        draggable="true"
        ondragstart={(e) => {
          e.dataTransfer?.setData("text/plain", t.id);
          e.stopPropagation();
          app.startDragTab(t.id);
        }}
        ondragend={() => app.endDragTab()}
        ondragover={(e) => {
          if (app.draggingTabId) {
            e.preventDefault();
            e.stopPropagation();
            app.dragOverTab(group.id, t.id);
          }
        }}
      >
        <button
          class="flex items-center gap-1 whitespace-nowrap border-none bg-transparent px-1.5 py-0.5 text-[0.76rem] text-foreground"
          onclick={() => app.setActiveTab(group.id, t.id)}
          ondblclick={() => onEditTab(t)}
          title={`${tabName(t)}（ダブルクリックで編集）`}
        >
          <span
            class={[
              "h-1.5 w-1.5 flex-none rounded-full bg-muted-foreground",
              {
                "bg-[var(--success)]": t.state === "connected",
                "bg-[var(--warning)]": t.state === "connecting" || t.state === "reconnecting",
                "bg-destructive": t.state === "error",
              },
            ]}
            data-state={t.state}
          ></span>{tabName(t)}
        </button>
        <button
          class="inline-flex border-none bg-transparent py-0 pr-0 pl-1 text-muted-foreground"
          class:hidden={t.id !== group.activeTabId}
          title="タブを閉じる"
          onclick={() => app.closeTab(t.id)}
        ><X size={12} /></button>
      </div>
    {/each}

    <Button variant="ghost" size="icon-xs" class="text-muted-foreground" title="タブを追加" onclick={() => onAddTab(group.id)}>＋</Button>
    <Button variant="ghost" size="icon-xs" class="text-muted-foreground" title="下に分割" onclick={() => onSplitDown(group.id)}>⬓</Button>
  </div>

  {#if activeTab}
    <div class="flex-1 overflow-y-auto" onscroll={onScroll}>
      {#if isNotif}
        {#each activeTab.notifications as n (n.id)}
          <NotificationCard notification={n} accountId={activeTab.accountId} />
        {/each}
        {#if activeTab.notifications.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-[0.82rem] text-muted-foreground">まだ通知がありません</div>
        {/if}
      {:else}
        {#each activeTab.notes as note (note.id)}
          <NoteCard
            {note}
            accountId={activeTab.accountId}
            tabId={activeTab.id}
            selected={note.id === activeTab.selectedNoteId}
          />
        {/each}
        {#if activeTab.notes.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-[0.82rem] text-muted-foreground">まだノートがありません</div>
        {/if}
      {/if}
      {#if activeTab.loadingMore}<div class="p-3.5 text-center text-[0.82rem] text-muted-foreground">読み込み中…</div>{/if}
    </div>
  {/if}

  {#if !stretch && !group.auto}
    <div
      class="absolute right-[-3px] top-0 h-full w-1.5 cursor-col-resize hover:bg-[color-mix(in_srgb,var(--color-primary)_40%,transparent)]"
      style="z-index:5"
      onpointerdown={onResizeDown}
      onpointermove={onResizeMove}
      onpointerup={onResizeUp}
      role="separator"
      aria-label="幅を変更"
    ></div>
  {/if}
</section>
```

補足:
- `.tab-close`の表示/非表示は元CSSでは`.tab:not(.active) .tab-close { display: none; }`だったが、Tailwindでは親要素の状態を子の`display`に反映させる簡単な方法が無いため、`class:hidden={t.id !== group.activeTabId}`で同等の挙動にしている(非activeタブでは閉じるボタンを隠す)。
- タブのアクティブ下線は元々`box-shadow: inset 0 -2px 0 var(--accent)`だったため、Tailwindのアービトラリ`shadow-[...]`記法で`var(--color-primary)`(`@theme`ブリッジ経由で`--accent`と同値)を参照している。
- `z-index:5`はTailwindの`z-5`が標準スケールに無い値のため、styleに残している(`z-[5]`という書き方も可能だが、他の`style`属性と合わせて可読性のためstyleにまとめた)。

- [ ] **Step 3: `<style>`ブロックを縮小**

167〜302行目の`<style>`ブロック全体を、`color-mix`によるアクセント/不透明度表現だけを残した以下の内容に置き換える:

```svelte
<style>
  /* 背景画像設定時にカラムを透けさせるための不透明度(--column-opacity)。Tailwindに
     color-mix()のユーティリティが無いため、この2つの背景だけはCSSに残す。 */
  .col-bg {
    background: color-mix(in srgb, var(--surface-1) var(--column-opacity, 100%), transparent);
  }
  .tabbar-bg {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
    border-top-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  /* キーボードフォーカス中のカラムは上端をはっきり表示。.group/.focusedは同一コンポーネント
     テンプレート内の要素なのでSvelteのスコープ付きCSSがそのまま効く(:global()不要)。 */
  .group.focused .tabbar-bg {
    border-top-color: var(--accent);
  }
</style>
```

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 6: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、以下を確認する:
- カラムのタブ切替、タブのドラッグ並び替え、タブ追加(＋)/下分割(⬓)ボタン、タブを閉じる(×)ボタン(非アクティブタブでは非表示のまま)
- カラム幅のドラッグリサイズ、`auto`カラムの均等割り、リサイズハンドルのホバー時ハイライト
- 設定→表示のカラム不透明度を変更し、`.col-bg`/`.tabbar-bg`の背景透過が反映されること
- フォーカス中カラムのタブバー上端が`--accent`色ではっきり表示されること(フォーカスしていないカラムは45%の薄いアクセント色のまま)
- タブの接続状態ドット(connected=緑/connecting・reconnecting=黄/error=赤/未接続=灰)の色分け
- ドラッグ中のカラムの半透明表示(opacity-55)、ドラッグ中タブの半透明表示(opacity-40)
- 明らかな見た目のズレがあれば、このタスクの中でクラス調整を行う

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/Column.svelte
git commit -m "style: Column.svelteをTailwindクラス+Buttonプリミティブに移行"
```
