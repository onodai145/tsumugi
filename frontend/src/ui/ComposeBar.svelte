<script lang="ts">
  import { app } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import VisibilitySelect from "./VisibilitySelect.svelte";
  import Dropdown from "./Dropdown.svelte";
  import DrivePicker from "./DrivePicker.svelte";
  import Modal from "./Modal.svelte";
  import { commands, unwrap, formatError } from "../lib/ipc";
  import { open } from "@tauri-apps/plugin-dialog";
  import { ImagePlus, SmilePlus, X } from "@lucide/svelte";
  import { portal } from "../lib/portal";
  import { tick } from "svelte";
  import ReactionPicker from "../input/ReactionPicker.svelte";
  import { emojiKeyToInsertText } from "../lib/emojiKey";
  import CompletionPopover from "./CompletionPopover.svelte";
  import { applyCompletion, buildCompletionItems, detectTrigger, type CompletionItem, type Trigger } from "../lib/mfmCompletion";
  import { getCaretCoordinates } from "../lib/caretPosition";
  import { searchHashtagItems, searchMentionItems } from "../lib/mfmSearch";
  import { pickComposePlaceholder } from "../lib/composePlaceholder";
  import type {
    NoteDraft_Deserialize as NoteDraft,
    VisibilityInput,
    DriveFile,
    Note,
    SourceItem,
  } from "../bindings/tauri.gen";

  // expanded: モバイルの投稿モーダルなど、常に複数行分の入力欄を確保したい文脈向け
  // (コンパクト表示への収縮を無効化する)。
  let { onPosted, expanded = false }: { onPosted?: () => void; expanded?: boolean } = $props();

  let accountId = $state(app.defaultAccountId());
  // ユーザが手動でアカウントを切り替えたら、以後は設定→アカウントの既定変更に追従しない
  let accountTouched = $state(false);
  let text = $state("");
  let placeholder = $state(pickComposePlaceholder());
  $effect(() => {
    if (text === "") {
      placeholder = pickComposePlaceholder();
    }
  });
  let cw = $state("");
  let useCw = $state(false);
  let visibility = $state<VisibilityInput>("public");
  let useChannel = $state(false);
  let channelId = $state("");
  let channels = $state<SourceItem[]>([]);
  let localOnly = $state(false);
  const MAX_POLL_CHOICES = 10;
  let usePoll = $state(false);
  let pollChoices = $state<string[]>(["", ""]);
  let pollMultiple = $state(false);
  type PollExpiryMode = "none" | "at" | "after";
  let pollExpiryMode = $state<PollExpiryMode>("none");
  let pollExpiresAt = $state(""); // datetime-local文字列(mode="at"用)
  let pollAfterAmount = $state(1); // mode="after"用の数量
  type PollAfterUnit = "minute" | "hour" | "day";
  let pollAfterUnit = $state<PollAfterUnit>("hour");
  const POLL_AFTER_UNIT_MS: Record<PollAfterUnit, number> = {
    minute: 60_000,
    hour: 3_600_000,
    day: 86_400_000,
  };
  const pollExpiryModes: { value: PollExpiryMode; label: string }[] = [
    { value: "none", label: "無期限" },
    { value: "at", label: "日時を指定" },
    { value: "after", label: "期間を指定" },
  ];
  const pollAfterUnits: { value: PollAfterUnit; label: string }[] = [
    { value: "minute", label: "分後" },
    { value: "hour", label: "時間後" },
    { value: "day", label: "日後" },
  ];
  type AttachmentItem =
    | { kind: "local"; id: string; path: string; name: string; previewUrl: string | null }
    | { kind: "drive"; id: string; file: DriveFile }
    | { kind: "clipboard"; id: string; name: string; bytes: number[]; previewUrl: string };

  const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp"]);

  function extLower(name: string): string {
    const i = name.lastIndexOf(".");
    return i >= 0 ? name.slice(i + 1).toLowerCase() : "";
  }

  let attachments = $state<AttachmentItem[]>([]);
  let attachTrigger = $state<HTMLElement | undefined>(undefined);
  let showAttachMenu = $state(false);
  let attachMenuPos = $state<{ left: number; top: number } | null>(null);
  let showDrivePicker = $state(false);
  let showEmojiPicker = $state(false);
  let emojiPickerTrigger = $state<HTMLElement | undefined>(undefined);
  let emojiPickerPos = $state<{ left: number; top: number } | null>(null);

  // ボタンをテキストエリア右上に重ねて配置しているため、素直に左揃えで開くと
  // ポップオーバー(幅300px)の大半がウィンドウ外にはみ出す。ボタンの右端に揃えつつ
  // ビューポート内に収まるようクランプする(上下もNoteCardのリアクションピッカーと同様)。
  const EMOJI_PICKER_W = 300;
  const EMOJI_PICKER_H = 380;
  function toggleEmojiPicker() {
    if (showEmojiPicker) {
      showEmojiPicker = false;
      return;
    }
    const r = emojiPickerTrigger?.getBoundingClientRect();
    if (r) {
      const left = Math.min(
        Math.max(8, r.right - EMOJI_PICKER_W),
        window.innerWidth - EMOJI_PICKER_W - 8,
      );
      const spaceBelow = window.innerHeight - r.bottom;
      const top =
        spaceBelow >= EMOJI_PICKER_H + 8 ? r.bottom + 4 : Math.max(8, r.top - EMOJI_PICKER_H - 4);
      emojiPickerPos = { left, top };
    }
    showEmojiPicker = true;
  }

  function toggleAttachMenu() {
    if (showAttachMenu) {
      showAttachMenu = false;
      return;
    }
    const r = attachTrigger?.getBoundingClientRect();
    if (r) attachMenuPos = { left: r.left, top: r.bottom + 4 };
    showAttachMenu = true;
  }

  async function chooseLocalUpload() {
    showAttachMenu = false;
    await pickFiles();
  }

  function chooseDrivePicker() {
    showAttachMenu = false;
    showDrivePicker = true;
  }

  function onDriveFilesSelected(picked: DriveFile[]) {
    const known = new Set(
      attachments.flatMap((a) => (a.kind === "drive" ? [a.file.id] : [])),
    );
    const additions: AttachmentItem[] = picked
      .filter((f) => !known.has(f.id))
      .map((f) => ({ kind: "drive", id: f.id, file: f }));
    attachments = [...attachments, ...additions];
  }
  let busy = $state(false);
  let uploadingAttachmentId = $state<string | null>(null);
  let failedAttachmentId = $state<string | null>(null);
  let err = $state<string | null>(null);
  let replyTo = $state<Note | undefined>(undefined);
  let quoteOf = $state<Note | undefined>(undefined);
  let textarea = $state<HTMLTextAreaElement | undefined>(undefined);
  let cursorPos = $state(0);
  let suppressAt = $state<number | null>(null);
  let composing = $state(false);
  let selectedIndex = $state(0);
  let selectionMoved = $state(false);
  let asyncCandidates = $state<CompletionItem[]>([]);
  let asyncSearchToken = 0; // 古い応答を無視するための世代カウンタ
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let focused = $state(false);
  // フォーカスが無く、かつ何も入力/添付/展開していない時だけコンパクト表示にする
  // (未送信の内容がある間は縮めない)。
  const compact = $derived(
    !expanded &&
      !focused &&
      !text.trim() &&
      !cw.trim() &&
      attachments.length === 0 &&
      !usePoll &&
      !replyTo &&
      !quoteOf &&
      !showEmojiPicker,
  );

  const customEmojiList = $derived(accountId ? (app.emojis[accountId] ?? []) : []);
  const trigger = $derived<Trigger | null>(
    composing || cursorPos === suppressAt ? null : detectTrigger(text, cursorPos),
  );
  $effect(() => {
    const t = trigger;
    clearTimeout(debounceTimer);
    asyncCandidates = [];
    asyncSearchToken++;
    if (!t || (t.kind !== "mention" && t.kind !== "hashtag") || t.query.length < 1) return;
    const token = asyncSearchToken;
    debounceTimer = setTimeout(async () => {
      if (!accountId) return;
      try {
        const items =
          t.kind === "mention" ? await searchMentionItems(accountId, t.query) : await searchHashtagItems(accountId, t.query);
        if (token === asyncSearchToken) asyncCandidates = items;
      } catch {
        if (token === asyncSearchToken) asyncCandidates = [];
      }
    }, 300);
    return () => clearTimeout(debounceTimer);
  });

  const candidates = $derived<CompletionItem[]>(
    !trigger
      ? []
      : trigger.kind === "mention" || trigger.kind === "hashtag"
        ? asyncCandidates
        : buildCompletionItems(trigger, customEmojiList),
  );
  const popoverOpen = $derived(trigger !== null && candidates.length > 0);

  // クエリが変わって候補集合が変わるたびに選択位置を先頭へ戻す
  $effect(() => {
    trigger;
    selectedIndex = 0;
    selectionMoved = false;
  });

  let popoverPos = $state<{ left: number; top: number } | null>(null);
  $effect(() => {
    if (!popoverOpen || !trigger || !textarea) {
      popoverPos = null;
      return;
    }
    const rect = textarea.getBoundingClientRect();
    const caret = getCaretCoordinates(textarea, trigger.start);
    popoverPos = { left: rect.left + caret.left, top: rect.top + caret.top + caret.height };
  });

  // アカウントが後から読まれた場合／既定アカウントが変更された場合の追従（手動選択後は止める）
  $effect(() => {
    if (!accountTouched) accountId = app.defaultAccountId();
  });

  // 補完ポップアップで使うカスタム絵文字を先読みする(ReactionPickerと同じパターン)。
  $effect(() => {
    if (accountId) app.loadEmojis(accountId).catch(() => {});
  });

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

  // 返信/引用/新規投稿ショートカット・ボタンからの「開く」要求を消費してこのバーへ反映する。
  // app.compose は一過性のシグナルとして扱い、消費後すぐ null に戻す（次の要求も同じ形で届くため）。
  $effect(() => {
    const c = app.compose;
    if (!c) return;
    // 返信/引用は対象ノートのアカウントに固定。素の新規投稿(ショートカットNなど)は
    // 既定アカウント追従を維持したいので accountTouched は立てない。
    if (c.replyTo || c.quoteOf) {
      accountId = c.accountId;
      accountTouched = true;
    }
    replyTo = c.replyTo;
    quoteOf = c.quoteOf;
    const contextChannelId = c.replyTo?.channelId ?? c.quoteOf?.channelId ?? null;
    useChannel = contextChannelId !== null;
    channelId = contextChannelId ?? "";
    // 返信先の @acct を本文へ自動挿入する（本家Misskeyクライアント準拠）。
    // 未入力の時だけ差し込み、既に何か書きかけている場合は上書きしない。
    if (c.replyTo && !text.trim()) {
      text = `${acctOf(c.replyTo.user)} `;
    }
    app.compose = null;
    textarea?.focus();
  });

  function acctOf(u: Note["user"]): string {
    return u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
  }

  function syncCursor() {
    const pos = textarea?.selectionStart ?? 0;
    if (pos !== cursorPos) suppressAt = null;
    cursorPos = pos;
  }

  async function insertEmoji(reactionKey: string) {
    const insertText = emojiKeyToInsertText(reactionKey);
    const pos = Math.min(textarea?.selectionStart ?? cursorPos, text.length);
    text = text.slice(0, pos) + insertText + text.slice(pos);
    const newPos = pos + insertText.length;
    suppressAt = newPos;
    await tick();
    textarea?.setSelectionRange(newPos, newPos);
    textarea?.focus();
    cursorPos = newPos;
  }

  function onTextareaInput() {
    syncCursor();
    suppressAt = null;
  }

  async function confirmCompletion(index: number) {
    const t = trigger;
    const item = candidates[index];
    if (!t || !item) return;
    const result = applyCompletion(text, t, item);
    text = result.text;
    suppressAt = result.cursor;
    await tick();
    textarea?.setSelectionRange(result.cursor, result.cursor);
    textarea?.focus();
    cursorPos = result.cursor;
  }

  function cancelContext() {
    replyTo = undefined;
    quoteOf = undefined;
  }

  async function pickFiles() {
    err = null;
    // filters は付けない: Misskey のドライブは画像/動画に限らず任意のファイル種別を
    // 添付できる。加えて Android では画像/動画の MIME タイプに絞ると OS が自動的に
    // フォトピッカーへリダイレクトし、選択後の content:// URI から本来のファイル名を
    // 復元できなくなる（Google Issue Tracker #268079113, #330118234）。filters を外して
    // 汎用の "*/*" にすることで通常のドキュメント選択になり、ファイル名も正しく解決される。
    const picked = await open({ multiple: true });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    for (const p of paths) {
      const name = p.split(/[\\/]/).pop() ?? p;
      let previewUrl: string | null = null;
      if (IMAGE_EXTENSIONS.has(extLower(name))) {
        try {
          previewUrl = await unwrap(commands.readAttachmentPreview(p));
        } catch {
          previewUrl = null;
        }
      }
      attachments = [...attachments, { kind: "local", id: crypto.randomUUID(), path: p, name, previewUrl }];
    }
  }

  function removeAttached(id: string) {
    const removed = attachments.find((a) => a.id === id);
    if (removed?.kind === "clipboard") URL.revokeObjectURL(removed.previewUrl);
    attachments = attachments.filter((a) => a.id !== id);
  }

  async function handlePaste(e: ClipboardEvent) {
    if (e.clipboardData?.getData("text/plain")) return;
    e.preventDefault();
    const r = await commands.readClipboardImage();
    if (r.status === "error") {
      if (r.error.kind !== "invalid") err = formatError(r.error);
      return;
    }
    const blob = new Blob([new Uint8Array(r.data.bytes)], { type: "image/png" });
    const previewUrl = URL.createObjectURL(blob);
    attachments = [
      ...attachments,
      { kind: "clipboard", id: crypto.randomUUID(), name: r.data.filename, bytes: r.data.bytes, previewUrl },
    ];
  }

  async function submit() {
    err = null;
    if (!accountId) {
      err = "アカウントを選択してください";
      return;
    }
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    if (!text.trim() && !quoteOf && choices.length === 0 && attachments.length === 0) return;
    let expiresAt: number | null = null;
    if (pollExpiryMode === "at" && pollExpiresAt) {
      expiresAt = new Date(pollExpiresAt).getTime();
    } else if (pollExpiryMode === "after") {
      expiresAt = Date.now() + pollAfterAmount * POLL_AFTER_UNIT_MS[pollAfterUnit];
    }

    busy = true;
    failedAttachmentId = null;
    try {
      for (const a of attachments) {
        if (a.kind === "drive") continue;
        uploadingAttachmentId = a.id;
        let file: DriveFile;
        try {
          file =
            a.kind === "clipboard"
              ? await unwrap(commands.uploadBytes(accountId, a.name, a.bytes))
              : await unwrap(commands.uploadFile(accountId, a.path));
        } catch (e) {
          failedAttachmentId = a.id;
          err = String(e);
          return;
        } finally {
          uploadingAttachmentId = null;
        }
        if (a.kind === "clipboard") URL.revokeObjectURL(a.previewUrl);
        attachments = attachments.map((x) => (x.id === a.id ? { kind: "drive", id: file.id, file } : x));
      }

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
      await app.postNote(accountId, draft);
      text = "";
      cw = "";
      useCw = false;
      usePoll = false;
      pollChoices = ["", ""];
      pollMultiple = false;
      pollExpiryMode = "none";
      pollExpiresAt = "";
      pollAfterAmount = 1;
      pollAfterUnit = "hour";
      localOnly = false;
      useChannel = false;
      channelId = "";
      attachments = [];
      replyTo = undefined;
      quoteOf = undefined;
      onPosted?.();
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
      uploadingAttachmentId = null;
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      if (busy) return;
      submit();
      return;
    }
    if (showEmojiPicker && e.key === "Escape") {
      e.preventDefault();
      showEmojiPicker = false;
      return;
    }
    if (popoverOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        // 未選択(ハイライト非表示)からの最初の移動は先頭を選ぶ。2回目以降は前後移動。
        selectedIndex = selectionMoved ? Math.min(selectedIndex + 1, candidates.length - 1) : 0;
        selectionMoved = true;
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        selectedIndex = selectionMoved ? Math.max(selectedIndex - 1, 0) : candidates.length - 1;
        selectionMoved = true;
        return;
      }
      if (e.key === "Tab" || e.key === "Enter") {
        if (e.key === "Enter" && !selectionMoved) {
          return; // 矢印キーで明示的に選ぶまでEnterでは確定しない(改行のつもりでEnterを押した場合の誤確定を防ぐ)
        }
        e.preventDefault();
        confirmCompletion(selectedIndex);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        suppressAt = cursorPos; // ポップアップだけ閉じる(返信/引用のキャンセルは行わない)
        return;
      }
    }
    if (e.key === "Escape" && (replyTo || quoteOf)) {
      e.preventDefault();
      cancelContext();
    }
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (showEmojiPicker && e.key === "Escape") {
      e.preventDefault();
      showEmojiPicker = false;
    }
  }}
/>

<div class="composewrap">
  <AccountSelect
    bind:value={
      () => accountId,
      (v) => {
        accountId = v;
        accountTouched = true;
      }
    }
    accounts={app.accounts}
    large={!expanded}
  />

  <div class="composebox">
  {#if replyTo || quoteOf}
    <div class="context">
      <span class="context-text">
        {replyTo ? "返信: " : "引用: "}@{(replyTo ?? quoteOf)!.user.username} — {(replyTo ?? quoteOf)!.text ?? ""}
      </span>
      <button class="context-x" title="キャンセル" onclick={cancelContext}><X size={12} /></button>
    </div>
  {/if}

  {#if useCw}
    <input class="cw-input" placeholder="内容警告 (CW)" bind:value={cw} />
  {/if}

  <div class="text-wrap">
    <textarea
      class="text"
      class:compact
      class:expanded
      rows={expanded ? 4 : 1}
      placeholder={placeholder}
      bind:value={text}
      bind:this={textarea}
      onkeydown={onKey}
      onkeyup={syncCursor}
      onclick={syncCursor}
      oninput={onTextareaInput}
      oncompositionstart={() => (composing = true)}
      oncompositionend={() => {
        composing = false;
        syncCursor();
      }}
      onfocus={() => (focused = true)}
      onblur={() => {
        focused = false;
        suppressAt = cursorPos;
      }}
      onpaste={handlePaste}
    ></textarea>
    <button
      class="emoji-trigger"
      class:active={showEmojiPicker}
      title="絵文字を挿入"
      bind:this={emojiPickerTrigger}
      onmousedown={(e) => e.preventDefault()}
      onclick={toggleEmojiPicker}
      disabled={busy || !accountId}
    ><SmilePlus size={16} /></button>
  </div>

  {#if popoverOpen && popoverPos}
    <!-- 矢印キーで選ぶまでEnterで確定しない(誤爆防止)ため、
         選んでいないのに選択済みに見えないようハイライトも合わせて隠す -->
    <CompletionPopover
      items={candidates}
      selectedIndex={selectionMoved ? selectedIndex : -1}
      left={popoverPos.left}
      top={popoverPos.top}
      onpick={confirmCompletion}
    />
  {/if}

  {#if attachments.length > 0}
    <div class="thumbs">
      {#each attachments as a (a.id)}
        <div class="thumb-wrap">
          {#if a.kind === "drive"}
            {#if a.file.mimeType.startsWith("image/")}
              <img class="thumb" src={a.file.thumbnailUrl ?? a.file.url} alt="" />
            {:else}
              <span class="thumb badge">{a.file.mimeType.split("/")[0]}</span>
            {/if}
          {:else if a.previewUrl}
            <img class="thumb" src={a.previewUrl} alt="" />
          {:else}
            <span class="thumb badge">{extLower(a.name).toUpperCase() || "FILE"}</span>
          {/if}
          {#if uploadingAttachmentId === a.id}
            <span class="thumb-status" title="アップロード中">…</span>
          {:else if failedAttachmentId === a.id}
            <span class="thumb-status error">!</span>
          {/if}
          <button class="thumb-x" title="削除" onclick={() => removeAttached(a.id)}><X size={10} /></button>
        </div>
      {/each}
    </div>
  {/if}

  {#if usePoll}
    <div class="poll">
      {#each pollChoices as _, i}
        <div class="poll-choice-row">
          <input class="poll-choice" placeholder={`選択肢 ${i + 1}`} bind:value={pollChoices[i]} />
          <button
            class="poll-choice-x"
            title="この選択肢を削除"
            disabled={pollChoices.length <= 2}
            onclick={() => (pollChoices = pollChoices.filter((_, j) => j !== i))}
          >
            <X size={12} />
          </button>
        </div>
      {/each}
      <div class="poll-actions">
        <button
          class="mini"
          disabled={pollChoices.length >= MAX_POLL_CHOICES}
          onclick={() => (pollChoices = [...pollChoices, ""])}
        >
          ＋選択肢
        </button>
        <label><input type="checkbox" bind:checked={pollMultiple} /> 複数選択</label>
      </div>
      <div class="poll-expiry">
        <span class="expiry-label">期限:</span>
        {#each pollExpiryModes as m (m.value)}
          <button
            class="mini"
            class:active={pollExpiryMode === m.value}
            onclick={() => (pollExpiryMode = m.value)}
          >
            {m.label}
          </button>
        {/each}
        {#if pollExpiryMode === "at"}
          <input type="datetime-local" bind:value={pollExpiresAt} class="poll-expires" />
        {:else if pollExpiryMode === "after"}
          <input
            type="number"
            min="1"
            class="poll-after-amount"
            bind:value={pollAfterAmount}
          />
          <div class="poll-after-unit">
            <Dropdown bind:value={pollAfterUnit} options={pollAfterUnits} />
          </div>
        {/if}
      </div>
    </div>
  {/if}

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
  </div>
</div>

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

{#if showAttachMenu && attachMenuPos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="attach-overlay" use:portal onclick={() => (showAttachMenu = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="attach-menu"
      style={`left:${attachMenuPos.left}px;top:${attachMenuPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="menu"
      tabindex="-1"
    >
      <button
        class="attach-item"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseLocalUpload}
      >ローカルから選択</button>
      <button
        class="attach-item"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseDrivePicker}
      >ドライブから選択</button>
    </div>
  </div>
{/if}

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

{#if showDrivePicker && accountId}
  <DrivePicker {accountId} onSelect={onDriveFilesSelected} onclose={() => (showDrivePicker = false)} />
{/if}

<style>
  .composewrap {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .composebox {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }
  .context {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.78rem;
    color: var(--text-dim);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 3px 6px;
  }
  .context-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .context-x {
    display: inline-flex;
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .text-wrap {
    position: relative;
  }
  .text {
    width: 100%;
    resize: vertical;
    padding: 6px 34px 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.86rem;
    line-height: 1.4;
    min-height: 80px;
    box-sizing: border-box;
    transition: min-height 0.12s ease;
  }
  .emoji-trigger {
    position: absolute;
    top: 6px;
    right: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 6px;
    background: var(--surface-1);
    color: var(--text-dim);
    cursor: pointer;
    opacity: 0.85;
  }
  .emoji-trigger:hover {
    opacity: 1;
    color: var(--text);
    background: var(--surface-3);
  }
  .emoji-trigger.active {
    color: var(--accent);
    opacity: 1;
  }
  .emoji-trigger:disabled {
    opacity: 0.4;
    cursor: default;
  }
  /* フォーカスが無く未入力の時はコンパクトに(フォーカス/入力があれば通常サイズへ戻す) */
  .text.compact {
    min-height: 34px;
    resize: none;
  }
  /* モバイル投稿モーダルなど: 常に4行分の高さを確保する */
  .text.expanded {
    min-height: 96px;
  }
  .cw-input,
  .poll-choice {
    width: 100%;
    padding: 6px 9px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.84rem;
    box-sizing: border-box;
  }
  .poll {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .poll-choice-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .poll-choice-row .poll-choice {
    flex: 1;
  }
  .poll-choice-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    padding: 4px;
  }
  .poll-choice-x:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .poll-actions {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .poll-actions .mini:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .poll-expiry {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .expiry-label {
    flex: none;
  }
  .poll-expires {
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.78rem;
  }
  .poll-after-amount {
    width: 60px;
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    font-size: 0.78rem;
  }
  .poll-after-unit {
    width: 90px;
  }
  .thumbs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .thumb-wrap {
    position: relative;
    width: 28px;
    height: 28px;
  }
  .thumb {
    width: 28px;
    height: 28px;
    border-radius: 4px;
    object-fit: cover;
  }
  .thumb.badge {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    background: var(--surface-3);
    color: var(--text-dim);
    font-size: 0.6rem;
    border-radius: 4px;
  }
  .thumb-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    top: -4px;
    right: -4px;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    border-radius: 50%;
    width: 14px;
    height: 14px;
    cursor: pointer;
  }
  .thumb-status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    bottom: -4px;
    left: -4px;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    border-radius: 50%;
    width: 14px;
    height: 14px;
    font-size: 0.6rem;
  }
  .thumb-status.error {
    background: var(--danger);
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .tools.left {
    flex: 1;
    min-width: 0;
  }
  .tools.right {
    flex: none;
  }
  .icon {
    display: inline-flex;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    border-radius: 4px;
    padding: 4px 7px;
    cursor: pointer;
    flex: none;
  }
  .icon:disabled {
    opacity: 0.5;
  }
  .mini {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-1);
    color: var(--text);
    cursor: pointer;
    font-size: 0.78rem;
    flex: none;
  }
  .mini.active {
    border-color: var(--accent);
    color: var(--accent);
  }
  .lo {
    font-size: 0.78rem;
    color: var(--text-dim);
    flex: none;
    white-space: nowrap;
  }
  .hint {
    font-size: 0.78rem;
    color: var(--text-dim);
    flex: none;
    white-space: nowrap;
  }
  .post {
    border: none;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    border-radius: 6px;
    padding: 7px 20px;
    cursor: pointer;
    flex: none;
  }
  .post:disabled {
    opacity: 0.5;
  }
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
  .attach-overlay {
    position: fixed;
    inset: 0;
    z-index: 55;
  }
  .attach-menu {
    position: fixed;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
    padding: 4px;
    min-width: 160px;
  }
  .emoji-picker-pop {
    position: fixed;
  }
  .attach-item {
    display: block;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-size: 0.82rem;
  }
  .attach-item:hover {
    background: var(--surface-2);
  }
  .attach-item:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
