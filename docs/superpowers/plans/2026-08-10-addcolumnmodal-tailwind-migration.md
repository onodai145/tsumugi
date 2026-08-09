# AddColumnModal.svelte Tailwind移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #174の第4バッチとして、`AddColumnModal.svelte`の手書きCSS(overlay/modal/header自前実装)を共通`Modal.svelte`+Tailwindユーティリティクラスへ移行する。あわせて第3バッチで見送った`ConfirmDialog.svelte`の余白バグを直す。

**Architecture:** `AddColumnModal.svelte`のoverlay/modal/header/portalを共通`Modal.svelte`に置き換え、フィールド群の`<style>`をトークンブリッジ準拠のTailwindユーティリティクラスに変換する。「簡単/エキスパート」切替ボタンは単一クラス選択の三項演算子で実装し、過去バッチで踏んだクラス衝突バグを回避する。送信ボタン・小型ボタン群はshadcn Buttonプリミティブに置き換える。データ取得ロジック・バリデーション・`submit()`等の`<script>`ロジックは一切変更しない。

**Tech Stack:** Tailwind CSS v4、既存の`@theme`トークンブリッジ、shadcn-svelte Buttonプリミティブ(`$lib/components/ui/button`、新規追加なし)、共通`Modal.svelte`(第2バッチで整備済み)

## Global Constraints

- `<script>`ブロックのロジック(`buildKind()`/`buildFilter()`/`sourceDsl()`/`guidedToTql()`/`switchToExpert()`/`submit()`/各`$effect`等)は一切変更しない
- surfaceカラーのマッピング規約: `--surface-1`→`bg-background`、`--surface-2`→`bg-muted`、`--surface-3`→`bg-accent`、`--text`→`text-foreground`、`--text-dim`→`text-muted-foreground`、`--accent`→`bg-primary`/`text-primary-foreground`、`--danger`→`text-destructive`、`--border`→`border-border`
- **条件付きクラスは必ず「1つの完全なクラス文字列を選ぶ三項演算子」の形にする。「同じCSSプロパティを設定する複数のクラスを`class:`ディレクティブや`class={[...]}`配列で個別にON/OFFする」書き方は禁止**(#176の最終レビューで見つかったバグと同じ理由。生成後CSSのアルファベット順で優先順位が決まり意図通りに上書きされないため)
- Preflight除外環境でのUAデフォルトmargin対策として、`<p>`要素には常に`mt-0`または`mb-0`など、元CSSで暗黙にゼロだった側のmarginを明示的に指定する(#178の最終レビューで見つかったバグの再発防止)
- Rust側・`theme.ts`・`@theme`ブリッジ(`frontend/src/app.css`)は変更しない
- Buttonプリミティブは既存のものをそのまま使う(shadcn-svelte CLIの再実行は不要)
- `Dropdown.svelte`/`AccountSelect.svelte`/`input/TqlCompletionField.svelte`は対象外、変更しない

---

### Task 1: `ConfirmDialog.svelte`の余白バグ修正

**Files:**
- Modify: `frontend/src/ui/ConfirmDialog.svelte:45`

**Interfaces:**
- Consumes: なし(既存の`Button`をそのまま使用)
- Produces: 見た目・挙動は現状維持(marginの実効値のみ修正)

- [ ] **Step 1: `<p>`要素に`mt-0`を追加**

`frontend/src/ui/ConfirmDialog.svelte`の45行目を以下のように変更する:

```diff
- <p class="mb-4 whitespace-pre-wrap text-[0.85rem] text-foreground">{message}</p>
+ <p class="mb-4 mt-0 whitespace-pre-wrap text-[0.85rem] text-foreground">{message}</p>
```

- [ ] **Step 2: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 3: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、確認ダイアログ(例: カラム削除時)を開いてメッセージ文とボタンの間隔が詰まりすぎていないこと・タイトルとメッセージの間隔が自然であることを確認する。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/ConfirmDialog.svelte
git commit -m "fix: ConfirmDialogのメッセージ上マージンを明示してUAデフォルト復活を防ぐ"
```

---

### Task 2: `AddColumnModal.svelte`のTailwind移行

**Files:**
- Modify: `frontend/src/ui/AddColumnModal.svelte`

**Interfaces:**
- Consumes: 共通`Modal.svelte`(`title: string`, `onclose: () => void` props、`children` snippet)、既存の`Button`(`$lib/components/ui/button`)
- Produces: 見た目・挙動は現状維持。呼び出し元(`onclose`/`groupId`/`editTab` props)からの使い方は変更しない

- [ ] **Step 1: `<script>`にimportを追加**

`<script lang="ts">`ブロック冒頭のimport群に追加:

```ts
import Modal from "./Modal.svelte";
import { Button } from "$lib/components/ui/button";
```

- [ ] **Step 2: テンプレート全体を置き換え**

364〜572行目(`<div class="overlay">`から`</div>`の閉じタグまで)を以下に置き換える(`<script>`ブロックは変更しない):

```svelte
<Modal title={isEdit ? "タブを編集" : groupId ? "タブを追加" : "カラムを追加"} {onclose}>
  <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
    <span class="text-muted-foreground">入力方法</span>
    <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
      <button
        type="button"
        class={uiMode === "guided"
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground"}
        onclick={() => (uiMode = "guided")}
      >簡単</button>
      <button
        type="button"
        class={uiMode === "expert"
          ? "bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground"
          : "bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground"}
        onclick={switchToExpert}
      >エキスパート(TQL)</button>
    </div>
  </div>

  <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
    <span class="text-muted-foreground">アカウント{isEdit ? "（変更不可）" : ""}</span>
    <AccountSelect bind:value={accountId} accounts={app.accounts} showLabel disabled={isEdit} />
  </div>

  <label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
    <span class="text-muted-foreground">名前（空欄で自動）</span>
    <input
      class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
      placeholder={edit?.title ?? "自動でつけます"}
      bind:value={name}
    />
  </label>

  {#if uiMode === "expert"}
    <label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">from ... where ...（複数ソースはカンマ区切り。例: from home, list("id") where has_files）</span>
      <TqlCompletionField
        mode="query"
        bind:value={tqlText}
        rows={4}
        placeholder={'from home, list("...") where has_files && !cw'}
        invalid={!!tqlErr}
        oninput={onTqlInput}
        {lists}
        {antennas}
        {channels}
      />
    </label>
    {#if tqlErr}<p class="mb-0 mt-2 text-[0.82rem] text-destructive break-words">TQLエラー: {tqlErr}</p>{/if}
    <p class="mb-2 mt-0 text-[0.75rem] text-muted-foreground">
      ソース: <code class="rounded bg-accent px-1">home</code> / <code class="rounded bg-accent px-1">local</code> / <code class="rounded bg-accent px-1">hybrid</code> / <code class="rounded bg-accent px-1">global</code> /
      <code class="rounded bg-accent px-1">list("id")</code> / <code class="rounded bg-accent px-1">antenna("id")</code> / <code class="rounded bg-accent px-1">channel("id")</code> /
      <code class="rounded bg-accent px-1">user("@acct")</code> / <code class="rounded bg-accent px-1">tag("name")</code> / <code class="rounded bg-accent px-1">search("q")</code> /
      <code class="rounded bg-accent px-1">cache</code>（ローカルキャッシュ検索）。list/antenna/channel は生IDが必要です。
    </p>
  {/if}

  {#if uiMode === "guided"}
  <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
    <span class="text-muted-foreground">ソース</span>
    <Dropdown bind:value={sourceType} options={srcOptions.map((s) => ({ value: s.v, label: s.label }))} />
  </div>

  {#if sourceType === "list"}
    <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">リスト</span>
      {#if lists.length > 0}
        <Dropdown bind:value={listId} options={lists.map((l) => ({ value: l.id, label: l.name || l.id }))} />
      {:else}
        <span class="text-[0.75rem] text-muted-foreground">リストがありません（Misskey 側で作成してください）</span>
      {/if}
    </div>
  {/if}

  {#if sourceType === "antenna"}
    <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">アンテナ</span>
      {#if antennas.length > 0}
        <Dropdown bind:value={antennaId} options={antennas.map((a) => ({ value: a.id, label: a.name || a.id }))} />
      {:else}
        <span class="text-[0.75rem] text-muted-foreground">アンテナがありません（Misskey 側で作成してください）</span>
      {/if}
    </div>
  {/if}

  {#if sourceType === "channel"}
    <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">チャンネル（フォロー中）</span>
      {#if channels.length > 0}
        <Dropdown bind:value={channelId} options={channels.map((c) => ({ value: c.id, label: c.name || c.id }))} />
      {:else}
        <span class="text-[0.75rem] text-muted-foreground">フォロー中のチャンネルがありません</span>
      {/if}
    </div>
  {/if}

  {#if sourceType === "user"}
    <label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">ユーザ（@user@host。ローカルは @host 省略可）</span>
      <input
        class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
        placeholder={editUserId ? "空欄で現在のユーザを維持" : "@alice@misskey.example"}
        bind:value={userAcct}
      />
    </label>
  {/if}

  {#if sourceType === "tag"}
    <label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">ハッシュタグ（# は省略可）</span>
      <input
        class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
        placeholder="misskey"
        bind:value={tagText}
      />
    </label>
  {/if}

  {#if sourceType === "search"}
    <label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">検索語</span>
      <input
        class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
        placeholder="キーワード"
        bind:value={searchQuery}
      />
    </label>
  {/if}

  {#if sourceType !== "search" && sourceType !== "user" && sourceType !== "tag"}
    <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">このタブの通知</span>
      <label class="flex items-center gap-1.5 text-[0.85rem]"><input type="checkbox" bind:checked={notifyDesktop} /> デスクトップ通知</label>
      <label class="flex items-center gap-1.5 text-[0.85rem]"><input type="checkbox" bind:checked={notifySound} /> 通知音</label>
    </div>
    {#if notifySound}
      <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
        <span class="text-muted-foreground">通知音の種類</span>
        <Dropdown bind:value={soundMode} options={soundModeOptions} />
        {#if soundMode === "custom"}
          <div class="flex items-center gap-2.5">
            <Button type="button" variant="outline" size="xs" disabled={pickingSound} onclick={pickSound}>
              {pickingSound ? "読み込み中…" : notifySoundChoice.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
            </Button>
            {#if notifySoundChoice.startsWith("data:")}
              <Button type="button" variant="outline" size="xs" onclick={() => playNotifySound(notifySoundChoice)}>試聴</Button>
            {/if}
          </div>
        {:else if soundMode !== "inherit"}
          <Button type="button" variant="outline" size="xs" onclick={() => playNotifySound(soundMode)}>試聴</Button>
        {/if}
      </div>
    {/if}
    <p class="mb-2 mt-0 text-[0.75rem] text-muted-foreground">
      {sourceType === "notifications" ? "通知カラムへの新着" : "このタブに新着ノート"}が届いたら発火します。
      設定→通知のグローバルスイッチも ON の場合のみ実際に鳴ります。
    </p>
  {:else}
    <p class="mb-2 mt-0 text-[0.75rem] text-muted-foreground">このソースはライブ更新（ストリーミング）に対応していないため通知は鳴りません。</p>
  {/if}

  {#if sourceType !== "notifications"}
    <label class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">フィルタ（TQL・空欄で全件）</span>
      <TqlCompletionField
        mode="predicate"
        bind:value={filterText}
        placeholder={"例: has_files && !cw && reactions >= 5"}
        invalid={!!filterErr}
        oninput={onFilterInput}
      />
    </label>
    <p class="mb-2 mt-0 text-[0.75rem] text-muted-foreground">
      例: <code class="rounded bg-accent px-1">has_files</code> / <code class="rounded bg-accent px-1">!bot && local</code> /
      <code class="rounded bg-accent px-1">reactions &gt;= 10</code> / <code class="rounded bg-accent px-1">text -&gt; "rust"</code>
    </p>
    {#if filterErr}<p class="mb-0 mt-2 text-[0.82rem] text-destructive break-words">TQLエラー: {filterErr}</p>{/if}
  {/if}
  {/if}

  {#if uiMode === "expert"}
    <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
      <span class="text-muted-foreground">このタブの通知</span>
      <label class="flex items-center gap-1.5 text-[0.85rem]"><input type="checkbox" bind:checked={notifyDesktop} /> デスクトップ通知</label>
      <label class="flex items-center gap-1.5 text-[0.85rem]"><input type="checkbox" bind:checked={notifySound} /> 通知音</label>
    </div>
    {#if notifySound}
      <div class="mb-2.5 flex flex-col gap-1 text-[0.85rem]">
        <span class="text-muted-foreground">通知音の種類</span>
        <Dropdown bind:value={soundMode} options={soundModeOptions} />
        {#if soundMode === "custom"}
          <div class="flex items-center gap-2.5">
            <Button type="button" variant="outline" size="xs" disabled={pickingSound} onclick={pickSound}>
              {pickingSound ? "読み込み中…" : notifySoundChoice.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
            </Button>
            {#if notifySoundChoice.startsWith("data:")}
              <Button type="button" variant="outline" size="xs" onclick={() => playNotifySound(notifySoundChoice)}>試聴</Button>
            {/if}
          </div>
        {:else if soundMode !== "inherit"}
          <Button type="button" variant="outline" size="xs" onclick={() => playNotifySound(soundMode)}>試聴</Button>
        {/if}
      </div>
    {/if}
    <p class="mb-2 mt-0 text-[0.75rem] text-muted-foreground">ストリーミング対応のソースに新着があれば発火します。設定→通知のグローバルスイッチも ON の場合のみ実際に鳴ります。</p>
  {/if}

  <div class="mt-1.5 flex justify-end">
    <Button disabled={busy || !!filterErr || !!tqlErr} onclick={submit}>
      {busy ? (isEdit ? "保存中…" : "作成中…") : isEdit ? "保存" : "追加"}
    </Button>
  </div>
  {#if submitErr}<p class="mb-0 mt-2 text-[0.82rem] text-destructive break-words">{submitErr}</p>{/if}
</Modal>
```

補足:
- 「簡単/エキスパート」セグメントボタンは、Global Constraintsの「完全な文字列を選ぶ三項演算子」ルールに従い、`class:active`のような部分適用は使わず、アクティブ/非アクティブそれぞれで完結したクラス文字列を`class={...}`に丸ごと渡している。
- 「このタブの通知」ブロックはguided/expert両モードに同じマークアップが重複しているが、これは移行前の元コードの構造をそのまま踏襲したもの(このバッチのスコープはCSS/マークアップのTailwind化のみで、ロジック・構造上の重複解消は対象外)。
- `<Modal>`は内部で幅`w-[min(480px,92vw)]`・`z-[1000]`・portal・closeボタンを提供するため、旧`.overlay`/`.modal`/`.head`相当のマークアップは丸ごと削除している。

- [ ] **Step 3: `<style>`ブロックを削除**

`<style>...</style>`ブロック全体(旧574〜706行目)を削除する。`color-mix()`等の変換不能パターンは含まれていないため、`<style>`ブロック自体が不要になる。

- [ ] **Step 4: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: `pnpm build`を実行**

Run: `cd frontend && pnpm build`
Expected: 成功する

- [ ] **Step 6: `pnpm test`を実行**

Run: `cd frontend && pnpm test`
Expected: 既存テストが全て通る(`AddColumnModal.svelte`自体のテストは存在しないが、他コンポーネントのテストが本変更で壊れていないことを確認する)

- [ ] **Step 7: `cargo tauri dev`で目視確認**

リポジトリルートから`cargo tauri dev`を起動し、以下を確認する:
- 「カラムを追加」(バックステージから新規追加)・「タブを追加」(グループ内追加)・「タブを編集」(既存タブ編集)の3パターンでタイトル文言とモーダルの見た目が正しいこと
- 簡単/エキスパートの切替ボタンの見た目(アクティブ側が塗りつぶし、非アクティブ側が枠のみ)
- 簡単モードで各ソース種別(list/antenna/channel/user/tag/search/notifications)を選んだ時のフィールド出し分け
- 通知設定(デスクトップ通知/通知音チェックボックス、音声プリセット選択、カスタム音声ファイル選択・試聴)
- エキスパートモードでのTQL入力・エラー表示・簡単→エキスパート切替時のシード文字列
- フィルタ入力・エラー表示
- 送信ボタンの活性/非活性、送信エラー時のメッセージ表示
- モーダルを閉じる(背景クリック/Escape/closeボタン)動作

- [ ] **Step 8: Commit**

```bash
git add frontend/src/ui/AddColumnModal.svelte
git commit -m "style: AddColumnModal.svelteを共通Modal+Tailwindクラスに移行"
```
