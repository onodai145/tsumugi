<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";
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
  let channelsLoading = $state(false);
  let channelsError = $state(false);
  // 選択中チャンネルが取得済み一覧に含まれない場合(未フォローチャンネルへの返信など)、
  // 表示名は分からないがIDだけで選択済み扱いにするための合成オプションを補う。
  // (Dropdown は options.find(o => o.value === value) で一致を探すため、
  //  補わないと「選択…」のまま表示され、開いて別チャンネルへ誤って切り替えてしまう)
  const channelOptions = $derived(
    channelId && !channels.some((c) => c.id === channelId)
      ? [...channels.map((c) => ({ value: c.id, label: c.name || c.id })), { value: channelId, label: channelId }]
      : channels.map((c) => ({ value: c.id, label: c.name || c.id })),
  );
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
  // Button の ref prop は $bindable(null) のためフォールバック値と型を合わせて null 初期化する
  // (undefined 初期化のままだと Svelte が "Cannot do bind:ref={undefined} when ref has
  // a fallback value" で例外を投げ、ComposeBar 全体がマウントに失敗する)。
  let attachTrigger = $state<HTMLElement | null>(null);
  let showAttachMenu = $state(false);
  let attachMenuPos = $state<{ left: number; top: number } | null>(null);
  let showDrivePicker = $state(false);
  let showEmojiPicker = $state(false);
  let emojiPickerTrigger = $state<HTMLElement | null>(null);
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
      channelsLoading = true;
      channelsError = false;
      app
        .fetchChannels(accountId)
        .then((l) => {
          channels = l;
        })
        .catch(() => {
          channelsError = true;
        })
        .finally(() => {
          channelsLoading = false;
        });
    }
  });

  // アカウント切替時にchannelIdをリセットする処理(下の別effect)が、返信/引用コンテキストの
  // 自動選択を誤って打ち消さないようにするための「最後にaccountId+channelIdを同期し終えた
  // アカウント」の記録。$stateにせず素のクロージャ変数にすることで、この値自体の変化が
  // リアクティブな再実行トリガーにならないようにしている。
  let lastSyncedAccountId: string | null = null;

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
    // accountIdとchannelIdをこのeffectが一体で同期したことを記録する
    // (下のアカウント切替リセットeffectが、この直後にchannelIdを"" に巻き戻さないように)。
    lastSyncedAccountId = accountId;
    // 返信先の @acct を本文へ自動挿入する（本家Misskeyクライアント準拠）。
    // 未入力の時だけ差し込み、既に何か書きかけている場合は上書きしない。
    if (c.replyTo && !text.trim()) {
      text = `${acctOf(c.replyTo.user)} `;
    }
    app.compose = null;
    textarea?.focus();
  });

  // アカウントが(上記の返信/引用同期以外の理由で、例えばアカウント選択欄からの手動切替で)
  // 変わった場合は、他アカウントのチャンネルIDを持ち越さないようリセットする。
  $effect(() => {
    if (accountId !== lastSyncedAccountId) {
      channelId = "";
      // 別アカウントのチャンネル一覧を再取得(下のfetchChannels effect)が終わるまでの間、
      // 前アカウントの一覧を表示し続けて誤選択を招かないようクリアしておく。
      channels = [];
      channelsError = false;
      lastSyncedAccountId = accountId;
    }
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
    if (useChannel && !channelId) {
      err = "チャンネルを選択してください";
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
        localOnly: useChannel || localOnly,
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

<div class="flex flex-1 min-w-0 items-start gap-1.5">
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

  <div class="flex flex-1 min-w-0 flex-col gap-1">
  {#if replyTo || quoteOf}
    <div class="flex items-center gap-1.5 rounded-md border border-border bg-muted px-1.5 py-[3px] text-[0.78rem] text-muted-foreground">
      <span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
        {replyTo ? "返信: " : "引用: "}@{(replyTo ?? quoteOf)!.user.username} — {(replyTo ?? quoteOf)!.text ?? ""}
      </span>
      <Button type="button" variant="ghost" size="icon-xs" class="flex-none text-muted-foreground" title="キャンセル" onclick={cancelContext}><X size={12} /></Button>
    </div>
  {/if}

  {#if useCw}
    <input class="w-full box-border rounded border border-border bg-muted px-[9px] py-1.5 font-[inherit] text-[0.84rem] text-foreground" placeholder="内容警告 (CW)" bind:value={cw} />
  {/if}

  <div class="relative">
    <textarea
      class={expanded
        ? "w-full box-border resize-y rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-[0.86rem] leading-[1.4] text-foreground min-h-24 [transition:min-height_0.12s_ease]"
        : compact
          ? "w-full box-border resize-none rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-[0.86rem] leading-[1.4] text-foreground min-h-[34px] [transition:min-height_0.12s_ease]"
          : "w-full box-border resize-y rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-[0.86rem] leading-[1.4] text-foreground min-h-20 [transition:min-height_0.12s_ease]"}
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
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      class={showEmojiPicker
        ? "absolute top-1.5 right-1.5 bg-background text-primary opacity-100 hover:bg-accent hover:text-foreground dark:hover:bg-accent disabled:cursor-default disabled:opacity-40"
        : "absolute top-1.5 right-1.5 bg-background text-muted-foreground opacity-85 hover:bg-accent hover:text-foreground dark:hover:bg-accent hover:opacity-100 disabled:cursor-default disabled:opacity-40"}
      title="絵文字を挿入"
      bind:ref={emojiPickerTrigger}
      onmousedown={(e) => e.preventDefault()}
      onclick={toggleEmojiPicker}
      disabled={busy || !accountId}
    ><SmilePlus size={16} class="size-4" /></Button>
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
    <div class="flex flex-wrap gap-1">
      {#each attachments as a (a.id)}
        <div class="relative h-7 w-7">
          {#if a.kind === "drive"}
            {#if a.file.mimeType.startsWith("image/")}
              <img class="h-7 w-7 rounded object-cover" src={a.file.thumbnailUrl ?? a.file.url} alt="" />
            {:else}
              <span class="grid h-7 w-7 place-items-center rounded bg-accent text-[0.6rem] text-muted-foreground">{a.file.mimeType.split("/")[0]}</span>
            {/if}
          {:else if a.previewUrl}
            <img class="h-7 w-7 rounded object-cover" src={a.previewUrl} alt="" />
          {:else}
            <span class="grid h-7 w-7 place-items-center rounded bg-accent text-[0.6rem] text-muted-foreground">{extLower(a.name).toUpperCase() || "FILE"}</span>
          {/if}
          {#if uploadingAttachmentId === a.id}
            <span class="absolute -bottom-1 -left-1 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-black/60 text-[0.6rem] text-white" title="アップロード中">…</span>
          {:else if failedAttachmentId === a.id}
            <span class="absolute -bottom-1 -left-1 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-destructive text-[0.6rem] text-white">!</span>
          {/if}
          <Button type="button" variant="ghost" size="icon-xs" class="absolute -top-1 -right-1 h-3.5 w-3.5 rounded-full bg-black/60 text-white hover:bg-black/60" title="削除" onclick={() => removeAttached(a.id)}><X size={10} /></Button>
        </div>
      {/each}
    </div>
  {/if}

  {#if usePoll}
    <div class="flex flex-col gap-[5px]">
      {#each pollChoices as _, i}
        <div class="flex items-center gap-1">
          <input class="flex-1 box-border rounded border border-border bg-muted px-[9px] py-1.5 font-[inherit] text-[0.84rem] text-foreground" placeholder={`選択肢 ${i + 1}`} bind:value={pollChoices[i]} />
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            class="flex-none text-muted-foreground disabled:opacity-35"
            title="この選択肢を削除"
            disabled={pollChoices.length <= 2}
            onclick={() => (pollChoices = pollChoices.filter((_, j) => j !== i))}
          >
            <X size={12} />
          </Button>
        </div>
      {/each}
      <div class="flex flex-wrap items-center gap-3 text-[0.8rem] text-muted-foreground">
        <Button
          type="button"
          variant="outline"
          size="xs"
          disabled={pollChoices.length >= MAX_POLL_CHOICES}
          onclick={() => (pollChoices = [...pollChoices, ""])}
        >
          ＋選択肢
        </Button>
        <label><input type="checkbox" bind:checked={pollMultiple} /> 複数選択</label>
      </div>
      <div class="flex flex-wrap items-center gap-1.5 text-[0.8rem] text-muted-foreground">
        <span class="flex-none">期限:</span>
        {#each pollExpiryModes as m (m.value)}
          <Button
            type="button"
            variant="outline"
            size="xs"
            class={pollExpiryMode === m.value ? "border-primary text-primary" : ""}
            onclick={() => (pollExpiryMode = m.value)}
          >
            {m.label}
          </Button>
        {/each}
        {#if pollExpiryMode === "at"}
          <input type="datetime-local" bind:value={pollExpiresAt} class="rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-[0.78rem] text-foreground" />
        {:else if pollExpiryMode === "after"}
          <input
            type="number"
            min="1"
            class="w-[60px] rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-[0.78rem] text-foreground"
            bind:value={pollAfterAmount}
          />
          <div class="w-[90px]">
            <Dropdown bind:value={pollAfterUnit} options={pollAfterUnits} />
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <div class="flex items-center justify-between gap-2">
    <div class="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
      <VisibilitySelect bind:value={visibility} disabled={useChannel} />
      <Button
        type="button"
        variant="outline"
        size="icon-xs"
        title="画像を添付"
        bind:ref={attachTrigger}
        onclick={toggleAttachMenu}
        disabled={busy}
      ><ImagePlus size={16} class="size-4" /></Button>
      <Button type="button" variant="outline" size="xs" class={useCw ? "border-primary text-primary" : ""} onclick={() => (useCw = !useCw)}>CW</Button>
      <Button type="button" variant="outline" size="xs" class={usePoll ? "border-primary text-primary" : ""} onclick={() => (usePoll = !usePoll)}>投票</Button>
      <Button type="button" variant="outline" size="xs" class={useChannel ? "border-primary text-primary" : ""} onclick={() => (useChannel = !useChannel)}>チャンネル</Button>
      {#if useChannel}
        {#if channelsLoading}
          <span class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">読み込み中…</span>
        {:else if channelsError}
          <span class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">読み込みに失敗しました</span>
        {:else if channelOptions.length > 0}
          <div class="channel-select w-[140px]">
            <Dropdown bind:value={channelId} options={channelOptions} />
          </div>
        {:else}
          <span class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">フォロー中のチャンネルがありません</span>
        {/if}
      {/if}
      <label class="flex-none whitespace-nowrap text-[0.78rem] text-muted-foreground">
        <input
          type="checkbox"
          checked={useChannel || localOnly}
          disabled={useChannel}
          onchange={(e) => (localOnly = e.currentTarget.checked)}
        /> 連合なし
      </label>
    </div>
    <div class="flex flex-none flex-wrap items-center gap-1.5">
      <Button type="button" size="sm" disabled={busy} onclick={submit}>{busy ? "…" : "投稿"}</Button>
    </div>
  </div>
  </div>
</div>

{#if err}
  <Modal title="エラー" onclose={() => (err = null)}>
    {#snippet children()}
      <p class="mb-3.5 mt-0 whitespace-pre-wrap break-words text-[0.9rem] text-foreground">{err}</p>
      <div class="flex justify-end">
        <Button onclick={() => (err = null)}>わかった</Button>
      </div>
    {/snippet}
  </Modal>
{/if}

{#if showAttachMenu && attachMenuPos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (showAttachMenu = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed min-w-[160px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
      style={`left:${attachMenuPos.left}px;top:${attachMenuPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="menu"
      tabindex="-1"
    >
      <button
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-[0.82rem] text-foreground hover:bg-muted disabled:cursor-default disabled:opacity-50"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseLocalUpload}
      >ローカルから選択</button>
      <button
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-[0.82rem] text-foreground hover:bg-muted disabled:cursor-default disabled:opacity-50"
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
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (showEmojiPicker = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed"
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
  .channel-select :global(.trigger) {
    padding: 5px 8px;
    font-size: 0.82rem;
    gap: 5px;
  }
</style>
