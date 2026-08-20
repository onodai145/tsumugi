<script lang="ts">
  import { untrack } from "svelte";
  import { app, NOTIFY_SOUND_PRESETS, playNotifySound } from "../lib/store.svelte";
  import type { TabView } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import Dropdown from "./Dropdown.svelte";
  import TqlCompletionField from "../input/TqlCompletionField.svelte";
  import type { ColumnKind, FilterQuery, UserList, SourceItem } from "../bindings/tauri.gen";
  import Modal from "./Modal.svelte";
  import { Button } from "$lib/components/ui/button";

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
  // エキスパートモード: from/where全文を自分で書く(複数ソース対応)。ソース選択等のガイド付き
  // フィールドは全て隠し、1つのテキストエリアのみ表示する。
  let uiMode = $state<"guided" | "expert">(k?.type === "tql" ? "expert" : "guided");
  let tqlText = $state(k?.type === "tql" && edit?.filter.kind === "tql" ? edit.filter.value : "");
  let tqlErr = $state<string | null>(null);
  async function onTqlInput() {
    submitErr = null;
    if (!tqlText.trim()) {
      tqlErr = null;
      return;
    }
    tqlErr = await app.validateTqlQuery(tqlText);
  }

  let accountId = $state(edit?.accountId ?? app.defaultAccountId());
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
  // 新規作成時の既定は通知タブのみON（Global等の高頻度タブでの通知過多を避ける）。
  // ソース種別を後で切り替えても追従させない（ユーザの選択を上書きしないため）ので untrack で初期値を固定する。
  let notifyDesktop = $state(edit?.notifyDesktop ?? untrack(() => sourceType === "notifications"));
  let notifySound = $state(edit?.notifySound ?? untrack(() => sourceType === "notifications"));
  // 空文字＝設定→通知のグローバル選択を継承
  let notifySoundChoice = $state(edit?.notifySoundChoice ?? "");
  let pickingSound = $state(false);

  type SoundMode = "inherit" | "custom" | (string & {});
  function modeFromChoice(choice: string): SoundMode {
    if (!choice) return "inherit";
    if (choice.startsWith("data:")) return "custom";
    return choice;
  }
  let soundMode = $state<SoundMode>(untrack(() => modeFromChoice(notifySoundChoice)));
  const soundModeOptions = [
    { value: "inherit", label: "継承（グローバル設定）" },
    ...NOTIFY_SOUND_PRESETS.map((p) => ({ value: p.id, label: p.label })),
    { value: "custom", label: "カスタム（音声ファイル）" },
  ];
  $effect(() => {
    if (soundMode === "inherit") notifySoundChoice = "";
    else if (soundMode === "custom") {
      if (!notifySoundChoice.startsWith("data:")) notifySoundChoice = "";
    } else {
      notifySoundChoice = soundMode;
    }
  });

  async function pickSound() {
    submitErr = null;
    pickingSound = true;
    try {
      const url = await app.pickNotifySoundFile();
      if (url) notifySoundChoice = url;
    } catch (e) {
      submitErr = String(e);
    } finally {
      pickingSound = false;
    }
  }
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

  // エキスパートモードではガイドモードのsourceType選択と関係なく、list/antenna/channelの
  // 実ID補完(TqlCompletionField)のため常にアカウントの一覧を取得しておく。
  // (ガイドモード用の上の3つのeffectと違い、id自動選択やエラー表示は行わない)
  $effect(() => {
    if (uiMode === "expert" && accountId) {
      app
        .fetchUserLists(accountId)
        .then((l) => {
          lists = l;
        })
        .catch(() => {});
      app
        .fetchAntennas(accountId)
        .then((l) => {
          antennas = l;
        })
        .catch(() => {});
      app
        .fetchChannels(accountId)
        .then((l) => {
          channels = l;
        })
        .catch(() => {});
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

  // TQL文字列リテラル用エスケープ（\ と " のみ。本家パーサの読み方に合わせる）
  function tqlStr(s: string): string {
    return `"${s.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  }

  // 現在の「簡単」モードの選択(ソース+フィルタ)から from/where 全文を組み立てる。
  // notifications は from 節に対応ソースが無いため null（エキスパートモードでは表現できない）。
  function sourceDsl(): string | null {
    switch (sourceType) {
      case "notifications":
        return null;
      case "list":
        return listId ? `list(${tqlStr(listId)})` : null;
      case "antenna":
        return antennaId ? `antenna(${tqlStr(antennaId)})` : null;
      case "channel":
        return channelId ? `channel(${tqlStr(channelId)})` : null;
      case "user": {
        const acct = userAcct.trim() || editUserId;
        return acct ? `user(${tqlStr(acct)})` : null;
      }
      case "tag": {
        const t = tagText.trim().replace(/^#/, "");
        return t ? `tag(${tqlStr(t)})` : null;
      }
      case "search":
        return searchQuery.trim() ? `search(${tqlStr(searchQuery.trim())})` : null;
      default:
        return sourceType; // home/local/hybrid/global
    }
  }

  function guidedToTql(): string {
    const src = sourceDsl();
    if (!src) return "";
    return filterText.trim() ? `from ${src} where ${filterText.trim()}` : `from ${src}`;
  }

  // 簡単→エキスパートへ切替た時、まだ何も書いていなければ今の選択内容を反映する
  // (ユーザが既にエキスパート欄へ書きかけている場合は上書きしない)。
  function switchToExpert() {
    uiMode = "expert";
    if (!tqlText.trim()) {
      const seeded = guidedToTql();
      if (seeded) {
        tqlText = seeded;
        void onTqlInput();
      }
    }
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
    if (uiMode === "expert") {
      if (tqlErr) return;
      if (!tqlText.trim()) {
        submitErr = "from ... where ... の形でクエリを入力してください";
        return;
      }
      busy = true;
      try {
        const kind: ColumnKind = { type: "tql" };
        const filter: FilterQuery = { kind: "tql", value: tqlText.trim() };
        if (isEdit && edit) {
          await app.updateColumn(edit.id, kind, filter, name);
          await app.setColumnNotify(edit.id, notifyDesktop, notifySound, notifySoundChoice);
        } else {
          const tab = await app.addColumn(accountId, kind, filter, groupId ?? undefined, name);
          if (
            notifyDesktop !== tab.notifyDesktop ||
            notifySound !== tab.notifySound ||
            notifySoundChoice !== tab.notifySoundChoice
          ) {
            await app.setColumnNotify(tab.id, notifyDesktop, notifySound, notifySoundChoice);
          }
        }
        onclose();
      } catch (e) {
        submitErr = String(e);
      } finally {
        busy = false;
      }
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
        await app.setColumnNotify(edit.id, notifyDesktop, notifySound, notifySoundChoice);
      } else {
        const tab = await app.addColumn(accountId, kind, buildFilter(), groupId ?? undefined, name);
        // 既定値と異なる場合のみ追加で呼ぶ（既定は backend 側の add_column が設定済み）
        if (
          notifyDesktop !== tab.notifyDesktop ||
          notifySound !== tab.notifySound ||
          notifySoundChoice !== tab.notifySoundChoice
        ) {
          await app.setColumnNotify(tab.id, notifyDesktop, notifySound, notifySoundChoice);
        }
      }
      onclose();
    } catch (e) {
      submitErr = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={isEdit ? "タブを編集" : groupId ? "タブを追加" : "カラムを追加"} {onclose}>
  <div class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">入力方法</span>
    <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
      <button
        type="button"
        class={uiMode === "guided"
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground"}
        onclick={() => (uiMode = "guided")}
      >簡単</button>
      <button
        type="button"
        class={uiMode === "expert"
          ? "bg-primary px-3.5 py-1.5 text-sm text-primary-foreground"
          : "bg-muted px-3.5 py-1.5 text-sm text-foreground"}
        onclick={switchToExpert}
      >エキスパート(TQL)</button>
    </div>
  </div>

  <div class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">アカウント{isEdit ? "（変更不可）" : ""}</span>
    <AccountSelect bind:value={accountId} accounts={app.accounts} showLabel disabled={isEdit} />
  </div>

  <label class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">名前（空欄で自動）</span>
    <input
      class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
      placeholder={edit?.title ?? "自動でつけます"}
      bind:value={name}
    />
  </label>

  {#if uiMode === "expert"}
    <label class="mb-2.5 flex flex-col gap-1 text-sm">
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
    {#if tqlErr}<p class="mb-0 mt-2 text-sm text-destructive break-words">TQLエラー: {tqlErr}</p>{/if}
    <p class="mb-2 mt-0 text-xs text-muted-foreground">
      ソース: <code class="rounded bg-accent px-1">home</code> / <code class="rounded bg-accent px-1">local</code> / <code class="rounded bg-accent px-1">hybrid</code> / <code class="rounded bg-accent px-1">global</code> /
      <code class="rounded bg-accent px-1">list("id")</code> / <code class="rounded bg-accent px-1">antenna("id")</code> / <code class="rounded bg-accent px-1">channel("id")</code> /
      <code class="rounded bg-accent px-1">user("@acct")</code> / <code class="rounded bg-accent px-1">tag("name")</code> / <code class="rounded bg-accent px-1">search("q")</code> /
      <code class="rounded bg-accent px-1">cache</code>（ローカルキャッシュ検索）。list/antenna/channel は生IDが必要です。
    </p>
  {/if}

  {#if uiMode === "guided"}
  <div class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">ソース</span>
    <Dropdown bind:value={sourceType} options={srcOptions.map((s) => ({ value: s.v, label: s.label }))} />
  </div>

  {#if sourceType === "list"}
    <div class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">リスト</span>
      {#if lists.length > 0}
        <Dropdown bind:value={listId} options={lists.map((l) => ({ value: l.id, label: l.name || l.id }))} />
      {:else}
        <span class="text-xs text-muted-foreground">リストがありません（Misskey 側で作成してください）</span>
      {/if}
    </div>
  {/if}

  {#if sourceType === "antenna"}
    <div class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">アンテナ</span>
      {#if antennas.length > 0}
        <Dropdown bind:value={antennaId} options={antennas.map((a) => ({ value: a.id, label: a.name || a.id }))} />
      {:else}
        <span class="text-xs text-muted-foreground">アンテナがありません（Misskey 側で作成してください）</span>
      {/if}
    </div>
  {/if}

  {#if sourceType === "channel"}
    <div class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">チャンネル（フォロー中）</span>
      {#if channels.length > 0}
        <Dropdown bind:value={channelId} options={channels.map((c) => ({ value: c.id, label: c.name || c.id }))} />
      {:else}
        <span class="text-xs text-muted-foreground">フォロー中のチャンネルがありません</span>
      {/if}
    </div>
  {/if}

  {#if sourceType === "user"}
    <label class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">ユーザ（@user@host。ローカルは @host 省略可）</span>
      <input
        class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
        placeholder={editUserId ? "空欄で現在のユーザを維持" : "@alice@misskey.example"}
        bind:value={userAcct}
      />
    </label>
  {/if}

  {#if sourceType === "tag"}
    <label class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">ハッシュタグ（# は省略可）</span>
      <input
        class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
        placeholder="misskey"
        bind:value={tagText}
      />
    </label>
  {/if}

  {#if sourceType === "search"}
    <label class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">検索語</span>
      <input
        class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
        placeholder="キーワード"
        bind:value={searchQuery}
      />
    </label>
  {/if}

  {#if sourceType !== "search" && sourceType !== "user" && sourceType !== "tag"}
    <div class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">このタブの通知</span>
      <label class="flex items-center gap-1.5 text-sm"><input type="checkbox" bind:checked={notifyDesktop} /> デスクトップ通知</label>
      <label class="flex items-center gap-1.5 text-sm"><input type="checkbox" bind:checked={notifySound} /> 通知音</label>
    </div>
    {#if notifySound}
      <div class="mb-2.5 flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">通知音の種類</span>
        <Dropdown bind:value={soundMode} options={soundModeOptions} />
        {#if soundMode === "custom"}
          <div class="flex items-center gap-2.5">
            <Button type="button" variant="outline" size="sm" disabled={pickingSound} onclick={pickSound}>
              {pickingSound ? "読み込み中…" : notifySoundChoice.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
            </Button>
            {#if notifySoundChoice.startsWith("data:")}
              <Button type="button" variant="outline" size="sm" onclick={() => playNotifySound(notifySoundChoice)}>試聴</Button>
            {/if}
          </div>
        {:else if soundMode !== "inherit"}
          <Button type="button" variant="outline" size="sm" onclick={() => playNotifySound(soundMode)}>試聴</Button>
        {/if}
      </div>
    {/if}
    <p class="mb-2 mt-0 text-xs text-muted-foreground">
      {sourceType === "notifications" ? "通知カラムへの新着" : "このタブに新着ノート"}が届いたら発火します。
      設定→通知のグローバルスイッチも ON の場合のみ実際に鳴ります。
    </p>
  {:else}
    <p class="mb-2 mt-0 text-xs text-muted-foreground">このソースはライブ更新（ストリーミング）に対応していないため通知は鳴りません。</p>
  {/if}

  {#if sourceType !== "notifications"}
    <label class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">フィルタ（TQL・空欄で全件）</span>
      <TqlCompletionField
        mode="predicate"
        bind:value={filterText}
        placeholder={"例: has_files && !cw && reactions >= 5"}
        invalid={!!filterErr}
        oninput={onFilterInput}
      />
    </label>
    <p class="mb-2 mt-0 text-xs text-muted-foreground">
      例: <code class="rounded bg-accent px-1">has_files</code> / <code class="rounded bg-accent px-1">!bot && local</code> /
      <code class="rounded bg-accent px-1">reactions &gt;= 10</code> / <code class="rounded bg-accent px-1">text -&gt; "rust"</code>
    </p>
    {#if filterErr}<p class="mb-0 mt-2 text-sm text-destructive break-words">TQLエラー: {filterErr}</p>{/if}
  {/if}
  {/if}

  {#if uiMode === "expert"}
    <div class="mb-2.5 flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">このタブの通知</span>
      <label class="flex items-center gap-1.5 text-sm"><input type="checkbox" bind:checked={notifyDesktop} /> デスクトップ通知</label>
      <label class="flex items-center gap-1.5 text-sm"><input type="checkbox" bind:checked={notifySound} /> 通知音</label>
    </div>
    {#if notifySound}
      <div class="mb-2.5 flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">通知音の種類</span>
        <Dropdown bind:value={soundMode} options={soundModeOptions} />
        {#if soundMode === "custom"}
          <div class="flex items-center gap-2.5">
            <Button type="button" variant="outline" size="sm" disabled={pickingSound} onclick={pickSound}>
              {pickingSound ? "読み込み中…" : notifySoundChoice.startsWith("data:") ? "音声を変更" : "音声ファイルを選択"}
            </Button>
            {#if notifySoundChoice.startsWith("data:")}
              <Button type="button" variant="outline" size="sm" onclick={() => playNotifySound(notifySoundChoice)}>試聴</Button>
            {/if}
          </div>
        {:else if soundMode !== "inherit"}
          <Button type="button" variant="outline" size="sm" onclick={() => playNotifySound(soundMode)}>試聴</Button>
        {/if}
      </div>
    {/if}
    <p class="mb-2 mt-0 text-xs text-muted-foreground">ストリーミング対応のソースに新着があれば発火します。設定→通知のグローバルスイッチも ON の場合のみ実際に鳴ります。</p>
  {/if}

  <div class="mt-1.5 flex justify-end">
    <Button disabled={busy || !!filterErr || !!tqlErr} onclick={submit} data-testid="add-column-submit">
      {busy ? (isEdit ? "保存中…" : "作成中…") : isEdit ? "保存" : "追加"}
    </Button>
  </div>
  {#if submitErr}<p class="mb-0 mt-2 text-sm text-destructive break-words">{submitErr}</p>{/if}
</Modal>
