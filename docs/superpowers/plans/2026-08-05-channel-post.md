# チャンネル投稿機能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** コンポーズバーからフォロー中チャンネルを選択して投稿できるようにする(Issue #95)。

**Architecture:** `NoteDraft`(Rust)に `channel_id: Option<String>` を追加してMisskeyの `notes/create` の `channelId` パラメータをそのまま通す。フロントエンドの `ComposeBar.svelte` にチャンネル選択トグル+ドロップダウンを追加し、選択中は可視性ピッカーを隠す。チャンネル内ノートへの返信/引用時は返信/引用元ノートの `channelId` を自動検出して事前選択する。

**Tech Stack:** Rust (Tauri v2 backend, serde, specta), Svelte 5 (runes: `$state`/`$effect`/`$derived`), tauri-specta で生成される `frontend/src/bindings/tauri.gen.ts`。

## Global Constraints

- 設計は `docs/superpowers/specs/2026-08-05-channel-post-design.md` に準拠する。
- チャンネル一覧はフォロー中チャンネルのみ(検索機能は対象外)。
- チャンネル選択中は可視性(visibility)ピッカーを非表示にする。CW・ローカルオンリー等の他オプションはそのまま利用可能。
- チャンネルタイムラインカラムからの新規投稿導線(FABからの自動選択)は対象外。
- 新規 REST API 呼び出しは追加しない(既存の `list_channels` コマンドを再利用)。
- `frontend/src/bindings/tauri.gen.ts` は生成ファイル。手で編集せず、`cargo test`(または `cargo tauri dev`)で再生成する。
- ブランチは `feat/issue-95-channel-post`(作成済み)。コミットメッセージは 1 行の subject のみ。

---

### Task 1: Backend — `NoteDraft` に `channel_id` を追加

**Files:**
- Modify: `src-tauri/src/api/notes.rs:22-38`(`NoteDraft` 構造体)
- Modify: `src-tauri/src/api/notes.rs:188-`(`mod tests`)
- Generated (via `cargo test`): `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: 既存の `NoteDraft` 構造体(`text`, `cw`, `visibility`, `file_ids`, `poll`, `reply_id`, `renote_id`, `local_only`)。
- Produces: `NoteDraft.channel_id: Option<String>`(serde `camelCase` で `channelId`、`skip_serializing_if = "Option::is_none"`)。Task 2 のフロントエンドはこのフィールドをTS側 `channelId: string | null`(Deserialize)/`channelId?: string | null`(Serialize)として利用する。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/notes.rs` の `mod tests` 内、既存の `draft_serializes_only_present_fields` テストを以下のように変更し(`channel_id` 未設定時は出力されないことを追加検証)、さらに新規テストを追加する:

```rust
    #[test]
    fn draft_serializes_only_present_fields() {
        let d = NoteDraft {
            text: Some("hi".into()),
            visibility: VisibilityInput::Home,
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["text"], "hi");
        assert_eq!(v["visibility"], "home");
        assert!(v.get("cw").is_none());
        assert!(v.get("replyId").is_none());
        assert!(v.get("renoteId").is_none());
        assert!(v.get("channelId").is_none());
        // 空の fileIds は送らない
        assert!(v.get("fileIds").is_none());
    }

    #[test]
    fn channel_post_serializes_channel_id() {
        let d = NoteDraft {
            text: Some("channel post".into()),
            channel_id: Some("ch1".into()),
            visibility: VisibilityInput::Public,
            ..Default::default()
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["text"], "channel post");
        assert_eq!(v["channelId"], "ch1");
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test --lib api::notes::tests`
Expected: コンパイルエラー(`NoteDraft` に `channel_id` フィールドが存在しない、`error[E0560] no field \`channel_id\` on type \`NoteDraft\``)

- [ ] **Step 3: `NoteDraft` に `channel_id` を追加する**

`src-tauri/src/api/notes.rs:22-38` の `NoteDraft` を以下のように変更する:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct NoteDraft {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cw: Option<String>,
    pub visibility: VisibilityInput,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<PollInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renote_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub local_only: bool,
}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test --lib api::notes::tests`
Expected: PASS(`draft_serializes_only_present_fields`, `channel_post_serializes_channel_id` を含む全テスト)

- [ ] **Step 5: バインディング再生成を含む全テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: PASS。`generates_frontend_bindings` テストが `frontend/src/bindings/tauri.gen.ts` を再生成し、`NoteDraft_Deserialize`/`NoteDraft_Serialize` に `channelId` フィールドが追加されていることを確認する:

```bash
grep -n "channelId" ../frontend/src/bindings/tauri.gen.ts
```

Expected: `NoteDraft_Deserialize` 内に `channelId: string | null,`、`NoteDraft_Serialize` 内に `channelId?: string | null,` が出力されている(既存の `cw`/`replyId`/`renoteId` と同じ形)。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/api/notes.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: NoteDraftにchannel_idを追加してチャンネル投稿を可能にする"
```

---

### Task 2: Frontend — コンポーズバーにチャンネル選択UIを追加

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`(状態追加・チャンネル取得・トグル/ドロップダウンUI・visibility非表示・返信/引用時の自動選択・投稿ドラフトへの反映・投稿後リセット)

**Interfaces:**
- Consumes: `NoteDraft_Deserialize.channelId: string | null`(Task 1 で生成)、`app.fetchChannels(accountId): Promise<SourceItem[]>`(`frontend/src/lib/store.svelte.ts:1072`、既存)、`SourceItem { id: string; name: string }`(`frontend/src/bindings/tauri.gen.ts`、既存)、`Note.channelId: string | null`(`frontend/src/bindings/tauri.gen.ts`、既存)。
- Produces: なし(末端UIタスク)。

- [ ] **Step 1: `SourceItem` 型をインポートし、チャンネル関連の状態を追加する**

`frontend/src/ui/ComposeBar.svelte:20-25` の import 型リストに `SourceItem` を追加する:

```ts
  import type {
    NoteDraft_Deserialize as NoteDraft,
    VisibilityInput,
    DriveFile,
    Note,
    SourceItem,
  } from "../bindings/tauri.gen";
```

`frontend/src/ui/ComposeBar.svelte:43`(`let visibility = $state<VisibilityInput>("public");` の直後)に以下を追加する:

```ts
  let useChannel = $state(false);
  let channelId = $state("");
  let channels = $state<SourceItem[]>([]);
```

- [ ] **Step 2: チャンネル一覧取得の `$effect` を追加する**

`frontend/src/ui/ComposeBar.svelte` の「補完ポップアップで使うカスタム絵文字を先読みする」`$effect`(232行目付近、`if (accountId) app.loadEmojis(accountId).catch(() => {});`)の直後に、`AddColumnModal.svelte:148-159` と同じパターンで以下を追加する:

```ts
  // チャンネル投稿トグルON時、フォロー中チャンネル一覧を取得する
  $effect(() => {
    if (useChannel && accountId) {
      app
        .fetchChannels(accountId)
        .then((l) => {
          channels = l;
        })
        .catch((e) => (err = String(e)));
    }
  });
```

- [ ] **Step 3: 返信/引用時にチャンネルを自動検出する**

`frontend/src/ui/ComposeBar.svelte:246-247` の `replyTo = c.replyTo; quoteOf = c.quoteOf;` の直後に以下を追加する:

```ts
    const contextChannelId = c.replyTo?.channelId ?? c.quoteOf?.channelId ?? null;
    useChannel = contextChannelId !== null;
    channelId = contextChannelId ?? "";
```

- [ ] **Step 4: ツールバーにチャンネルトグル/ドロップダウンを追加し、visibility を条件表示にする**

`frontend/src/ui/ComposeBar.svelte:630-643` のツールバー部分を以下のように変更する:

```svelte
  <div class="toolbar">
    <div class="tools left">
      {#if !useChannel}
        <VisibilitySelect bind:value={visibility} />
      {/if}
      <button
        class="icon"
        title="画像を添付"
        bind:this={attachTrigger}
        onclick={toggleAttachMenu}
        disabled={busy}
      ><ImagePlus size={16} /></button>
      <button class="mini" class:active={useCw} onclick={() => (useCw = !useCw)}>CW</button>
      <button class="mini" class:active={usePoll} onclick={() => (usePoll = !usePoll)}>投票</button>
      <button class="mini" class:active={useChannel} onclick={() => (useChannel = !useChannel)}>チャンネル</button>
      {#if useChannel}
        {#if channels.length > 0}
          <Dropdown bind:value={channelId} options={channels.map((c) => ({ value: c.id, label: c.name || c.id }))} />
        {:else}
          <span class="hint">フォロー中のチャンネルがありません</span>
        {/if}
      {/if}
      <label class="lo"><input type="checkbox" bind:checked={localOnly} /> 連合なし</label>
    </div>
    <div class="tools right">
      <button class="post" disabled={busy} onclick={submit}>{busy ? "…" : "投稿"}</button>
    </div>
  </div>
```

- [ ] **Step 5: 投稿ドラフトに `channelId` を積み、投稿成功後に状態をリセットする**

`frontend/src/ui/ComposeBar.svelte:386-395` の `draft` オブジェクト構築を以下のように変更する:

```ts
      const draft: NoteDraft = {
        text: text.trim() || null,
        cw: useCw && cw.trim() ? cw.trim() : null,
        visibility,
        fileIds: attachments.flatMap((a) => (a.kind === "drive" ? [a.file.id] : [])),
        poll: usePoll && choices.length >= 2 ? { choices, multiple: pollMultiple, expiresAt } : null,
        replyId: replyTo?.id ?? null,
        renoteId: quoteOf?.id ?? null,
        channelId: useChannel && channelId ? channelId : null,
        localOnly,
      };
```

`frontend/src/ui/ComposeBar.svelte:406-410`(`localOnly = false;` の付近、投稿成功後のリセット処理)に以下を追加する:

```ts
      localOnly = false;
      useChannel = false;
      channelId = "";
      attachments = [];
```

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし(svelte-check + tsc)。

- [ ] **Step 7: 手動確認**

`cargo tauri dev` を起動し、以下を確認する:
1. コンポーズバーで「チャンネル」ボタンを押すとフォロー中チャンネルのドロップダウンが表示され、可視性ピッカーが消えること。
2. チャンネルを選択して投稿すると、Misskey 側でそのチャンネルにノートが作成されること。
3. チャンネル内ノートに対して返信を開くと、チャンネルが自動選択された状態でコンポーズバーが開くこと。
4. 「チャンネル」ボタンを再度押して解除すると、通常の可視性ピッカー付き投稿に戻ること。

- [ ] **Step 8: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte
git commit -m "feat: コンポーズバーにチャンネル選択UIを追加"
```
