<script lang="ts">
  import { untrack } from "svelte";
  import { app } from "../lib/store.svelte";
  import type { TabView } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import Dropdown from "./Dropdown.svelte";
  import { X } from "@lucide/svelte";
  import type { ColumnKind, FilterQuery, UserList, SourceItem } from "../bindings/tauri.gen";

  // editTab を渡すと「編集」モード（アカウントは固定、ソース/フィルタ/名前を変更）。
  let {
    onclose,
    groupId,
    editTab,
  }: { onclose: () => void; groupId: string | null; editTab?: TabView } = $props();
  // モーダルは開くたび再生成されるので editTab は初期値スナップショットで扱う。
  const edit = untrack(() => editTab);
  const isEdit = !!edit;

  type SrcType =
    | "home"
    | "local"
    | "hybrid"
    | "global"
    | "list"
    | "antenna"
    | "channel"
    | "user"
    | "tag"
    | "search"
    | "notifications";
  const srcOptions: { v: SrcType; label: string }[] = [
    { v: "home", label: "Home（ホーム）" },
    { v: "local", label: "Local（ローカル）" },
    { v: "hybrid", label: "Hybrid（ソーシャル）" },
    { v: "global", label: "Global（グローバル）" },
    { v: "list", label: "List（リスト）" },
    { v: "antenna", label: "Antenna（アンテナ）" },
    { v: "channel", label: "Channel（チャンネル）" },
    { v: "user", label: "User（ユーザのノート・ライブ更新なし）" },
    { v: "tag", label: "Tag（ハッシュタグ・ライブ更新なし）" },
    { v: "search", label: "Search（検索・ライブ更新なし）" },
    { v: "notifications", label: "Notifications（通知）" },
  ];

  const k = edit?.kind;
  let accountId = $state(edit?.accountId ?? app.accounts[0]?.id ?? "");
  let sourceType = $state<SrcType>((k?.type as SrcType) ?? "home");
  let searchQuery = $state(k?.type === "search" ? k.query : "");
  let listId = $state(k?.type === "list" ? k.listId : "");
  let lists = $state<UserList[]>([]);
  let antennaId = $state(k?.type === "antenna" ? k.antennaId : "");
  let antennas = $state<SourceItem[]>([]);
  let channelId = $state(k?.type === "channel" ? k.channelId : "");
  let channels = $state<SourceItem[]>([]);
  let userAcct = $state("");
  // User 編集時は元の userId を保持（acct 未入力なら維持）
  const editUserId = k?.type === "user" ? k.userId : "";
  let tagText = $state(k?.type === "tag" ? k.tag : "");
  let name = $state(edit?.customTitle ?? "");
  let filterText = $state(edit?.filter.kind === "tql" ? edit.filter.value : "");
  let filterErr = $state<string | null>(null);
  let busy = $state(false);
  let submitErr = $state<string | null>(null);

  // List 選択時にアカウントのリストを取得
  $effect(() => {
    if (sourceType === "list" && accountId) {
      app
        .fetchUserLists(accountId)
        .then((l) => {
          lists = l;
          if (l.length > 0 && !l.some((x) => x.id === listId)) listId = l[0].id;
        })
        .catch((e) => (submitErr = String(e)));
    }
  });

  // Antenna 選択時にアンテナ一覧を取得
  $effect(() => {
    if (sourceType === "antenna" && accountId) {
      app
        .fetchAntennas(accountId)
        .then((l) => {
          antennas = l;
          if (l.length > 0 && !l.some((x) => x.id === antennaId)) antennaId = l[0].id;
        })
        .catch((e) => (submitErr = String(e)));
    }
  });

  // Channel 選択時にフォロー中チャンネル一覧を取得
  $effect(() => {
    if (sourceType === "channel" && accountId) {
      app
        .fetchChannels(accountId)
        .then((l) => {
          channels = l;
          if (l.length > 0 && !l.some((x) => x.id === channelId)) channelId = l[0].id;
        })
        .catch((e) => (submitErr = String(e)));
    }
  });

  // User/Tag 以外は同期的に kind を組める。User は submit 時に acct を解決する。
  function buildKind(): ColumnKind | null {
    switch (sourceType) {
      case "list":
        return listId ? { type: "list", listId } : null;
      case "antenna":
        return antennaId ? { type: "antenna", antennaId } : null;
      case "channel":
        return channelId ? { type: "channel", channelId } : null;
      case "tag": {
        const t = tagText.trim().replace(/^#/, "");
        return t ? { type: "tag", tag: t } : null;
      }
      case "search":
        return searchQuery.trim() ? { type: "search", query: searchQuery.trim() } : null;
      case "user":
        return null; // submit 側で解決
      default:
        return { type: sourceType };
    }
  }

  function buildFilter(): FilterQuery {
    return filterText.trim()
      ? { kind: "tql", value: filterText.trim() }
      : { kind: "keywords", value: [] };
  }

  async function onFilterInput() {
    submitErr = null;
    if (!filterText.trim()) {
      filterErr = null;
      return;
    }
    filterErr = await app.validateFilter(buildFilter());
  }

  const missingMsg: Partial<Record<SrcType, string>> = {
    list: "リストを選択してください",
    antenna: "アンテナを選択してください",
    channel: "チャンネルを選択してください",
    tag: "ハッシュタグを入力してください",
    search: "検索語を入力してください",
    user: "ユーザ（@user@host）を入力してください",
  };

  async function submit() {
    submitErr = null;
    if (!accountId) {
      submitErr = "アカウントを選択してください";
      return;
    }
    if (filterErr) return;
    busy = true;
    try {
      let kind = buildKind();
      // User は acct を userId へ解決。編集時に acct 未入力なら元の userId を維持。
      if (sourceType === "user") {
        if (userAcct.trim()) {
          const u = await app.resolveUser(accountId, userAcct.trim());
          kind = { type: "user", userId: u.id };
        } else if (editUserId) {
          kind = { type: "user", userId: editUserId };
        } else {
          submitErr = missingMsg.user!;
          return;
        }
      }
      if (!kind) {
        submitErr = missingMsg[sourceType] ?? "入力が不足しています";
        return;
      }
      if (isEdit && edit) {
        await app.updateColumn(edit.id, kind, buildFilter(), name);
      } else {
        await app.addColumn(accountId, kind, buildFilter(), groupId ?? undefined, name);
      }
      onclose();
    } catch (e) {
      submitErr = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>{isEdit ? "タブを編集" : groupId ? "タブを追加" : "カラムを追加"}</span>
      <button class="x" onclick={onclose}><X size={16} /></button>
    </header>

    <div class="field">
      <span>アカウント{isEdit ? "（変更不可）" : ""}</span>
      <AccountSelect bind:value={accountId} accounts={app.accounts} showLabel disabled={isEdit} />
    </div>

    <label class="field">
      <span>名前（空欄で自動）</span>
      <input placeholder={edit?.title ?? "自動でつけます"} bind:value={name} />
    </label>

    <div class="field">
      <span>ソース</span>
      <Dropdown bind:value={sourceType} options={srcOptions.map((s) => ({ value: s.v, label: s.label }))} />
    </div>

    {#if sourceType === "list"}
      <div class="field">
        <span>リスト</span>
        {#if lists.length > 0}
          <Dropdown bind:value={listId} options={lists.map((l) => ({ value: l.id, label: l.name || l.id }))} />
        {:else}
          <span class="hint">リストがありません（Misskey 側で作成してください）</span>
        {/if}
      </div>
    {/if}

    {#if sourceType === "antenna"}
      <div class="field">
        <span>アンテナ</span>
        {#if antennas.length > 0}
          <Dropdown bind:value={antennaId} options={antennas.map((a) => ({ value: a.id, label: a.name || a.id }))} />
        {:else}
          <span class="hint">アンテナがありません（Misskey 側で作成してください）</span>
        {/if}
      </div>
    {/if}

    {#if sourceType === "channel"}
      <div class="field">
        <span>チャンネル（フォロー中）</span>
        {#if channels.length > 0}
          <Dropdown bind:value={channelId} options={channels.map((c) => ({ value: c.id, label: c.name || c.id }))} />
        {:else}
          <span class="hint">フォロー中のチャンネルがありません</span>
        {/if}
      </div>
    {/if}

    {#if sourceType === "user"}
      <label class="field">
        <span>ユーザ（@user@host。ローカルは @host 省略可）</span>
        <input
          placeholder={editUserId ? "空欄で現在のユーザを維持" : "@alice@misskey.io"}
          bind:value={userAcct}
        />
      </label>
    {/if}

    {#if sourceType === "tag"}
      <label class="field">
        <span>ハッシュタグ（# は省略可）</span>
        <input placeholder="misskey" bind:value={tagText} />
      </label>
    {/if}

    {#if sourceType === "search"}
      <label class="field">
        <span>検索語</span>
        <input placeholder="キーワード" bind:value={searchQuery} />
      </label>
    {/if}

    {#if sourceType !== "notifications"}
      <label class="field">
        <span>フィルタ（TQL・空欄で全件）</span>
        <input
          placeholder={"例: has_files && !cw && reactions >= 5"}
          bind:value={filterText}
          oninput={onFilterInput}
          class:invalid={!!filterErr}
        />
      </label>
      <p class="hint">
        例: <code>has_files</code> / <code>!bot && local</code> /
        <code>reactions &gt;= 10</code> / <code>text -&gt; "rust"</code>
      </p>
      {#if filterErr}<p class="err">TQLエラー: {filterErr}</p>{/if}
    {/if}

    <div class="actions">
      <button class="submit" disabled={busy || !!filterErr} onclick={submit}>
        {busy ? (isEdit ? "保存中…" : "作成中…") : isEdit ? "保存" : "追加"}
      </button>
    </div>
    {#if submitErr}<p class="err">{submitErr}</p>{/if}
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
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
    font-size: 0.85rem;
  }
  .field > span:first-child {
    color: var(--text-dim);
  }
  input {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
  }
  input.invalid {
    border-color: #ef4444;
  }
  .hint {
    font-size: 0.75rem;
    color: var(--text-dim);
    margin: 0 0 8px;
  }
  .hint code {
    background: var(--surface-3);
    padding: 0 4px;
    border-radius: 4px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 6px;
  }
  .submit {
    padding: 8px 20px;
    border: none;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .submit:disabled {
    opacity: 0.5;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
    word-break: break-word;
  }
</style>
