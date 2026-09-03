<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";
  import AccountSelect from "./AccountSelect.svelte";
  import VisibilitySelect from "./VisibilitySelect.svelte";
  import ReactionAcceptanceSelect from "./ReactionAcceptanceSelect.svelte";
  import Dropdown from "./Dropdown.svelte";
  import DrivePicker from "./DrivePicker.svelte";
  import Modal from "./Modal.svelte";
  import { commands, unwrap, unwrapAcc, formatError } from "../lib/ipc";
  import { open } from "@tauri-apps/plugin-dialog";
  import { FileText, ImagePlus, SmilePlus, X } from "@lucide/svelte";
  import { portal } from "../lib/portal";
  import { onDestroy, tick } from "svelte";
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
    ReactionAcceptanceInput,
    DriveFile,
    Note,
    SourceItem,
    Draft,
    DraftInput,
    DraftNoteSnapshot,
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
  let reactionAcceptance = $state<ReactionAcceptanceInput>("all");
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

  /// 返信/引用コンテキストとして保持する最小限の形。banner表示(user.username/text)と
  /// submit時の.id参照にしか使わないため、下書き復元時にNote全体を持たずに済むよう
  /// フルのNote型ではなくこの最小型で持つ。
  type ComposeContextNote = { id: string; text: string | null; user: { username: string } };

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
  let showDraftMenu = $state(false);
  let draftMenuTrigger = $state<HTMLElement | null>(null);
  let draftMenuPos = $state<{ left: number; top: number } | null>(null);
  let manualDrafts = $state<Draft[]>([]);
  let draftsLoading = $state(false);
  /// 呼び出し中の手動下書きのID(投稿成功時にこれを自動削除する)。手動保存/新規入力/
  /// 自動下書き復元時はnullに戻す。
  let loadedDraftId = $state<string | null>(null);

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

  function snapshotToContextNote(s: DraftNoteSnapshot): ComposeContextNote {
    return { id: s.id, text: s.text, user: { username: s.username } };
  }

  function contextNoteToSnapshot(n: ComposeContextNote): DraftNoteSnapshot {
    return { id: n.id, username: n.user.username, text: n.text };
  }

  async function loadManualDrafts() {
    if (!accountId) {
      manualDrafts = [];
      return;
    }
    draftsLoading = true;
    try {
      manualDrafts = await unwrapAcc(accountId, commands.listDrafts(accountId));
    } catch {
      manualDrafts = [];
    } finally {
      draftsLoading = false;
    }
  }

  // ボタンを投稿ボタンの隣(ツールバー右端)に置いているため、素直に左揃えで開くと
  // ポップオーバー(幅280px)の大半がウィンドウ外にはみ出す。ボタンの右端に揃えつつ
  // ビューポート内に収まるようクランプする(絵文字ピッカーと同様)。
  const DRAFT_MENU_W = 280;
  function toggleDraftMenu() {
    if (showDraftMenu) {
      showDraftMenu = false;
      return;
    }
    const r = draftMenuTrigger?.getBoundingClientRect();
    if (r) {
      const left = Math.min(Math.max(8, r.right - DRAFT_MENU_W), window.innerWidth - DRAFT_MENU_W - 8);
      draftMenuPos = { left, top: r.bottom + 4 };
    }
    showDraftMenu = true;
    void loadManualDrafts();
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
  let replyTo = $state<ComposeContextNote | undefined>(undefined);
  let quoteOf = $state<ComposeContextNote | undefined>(undefined);
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
    // 共有インテント等からの初期本文(Issue #116)。返信のメンション挿入と同様、
    // 既に何か入力中ならそちらを優先し上書きしない。
    if (c.text && !text.trim()) {
      text = c.text;
    }
    for (const p of c.filePaths ?? []) {
      void addLocalAttachment(p);
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

  /// 自動一時保存: text/cw/添付/投票のいずれかが非空の間、入力変更を2秒デバウンスして
  /// save_auto_draftを呼ぶ。全て空になったらclear_auto_draftで消す(空の下書きを残さない)。
  /// 復元(下の自動復元effect)が完了するまでは、text等が一時的に空/暫定アカウントのままの
  /// タイミングで「空なのでclear」と誤判定し、これから復元しようとしている自動下書きを
  /// 消してしまうため、autoRestoreDoneがtrueになるまでこの効果は実質何もしない(依存値は
  /// 読むが save/clear は呼ばない)。$stateにせず素のクロージャ変数にすることで、この値自体の
  /// 変化がリアクティブな再実行トリガーにならないようにしている(lastSyncedAccountIdと同じ
  /// パターン)。
  let autoRestoreDone = false;
  let autoSaveTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    // 依存関係として拾うため、使う値をすべて先に読む
    const snapshot = { text, cw, useCw, attachmentsLen: attachments.length, usePoll, hasAccount: !!accountId };
    clearTimeout(autoSaveTimer);
    autoSaveTimer = undefined;
    if (!snapshot.hasAccount || !autoRestoreDone) return;
    const acc = accountId!;
    const nonEmpty =
      snapshot.text.trim() !== "" ||
      (snapshot.useCw && snapshot.cw.trim() !== "") ||
      snapshot.attachmentsLen > 0 ||
      snapshot.usePoll;
    if (!nonEmpty) {
      void unwrapAcc(acc, commands.clearAutoDraft(acc)).catch(() => {});
      return;
    }
    autoSaveTimer = setTimeout(() => {
      autoSaveTimer = undefined;
      void unwrapAcc(acc, commands.saveAutoDraft(acc, buildDraftInput())).catch(() => {});
    }, 2000);
    return () => clearTimeout(autoSaveTimer);
  });

  // コンポーネントのアンマウント時、デバウンス中(まだ発火していない)自動保存が残っていれば
  // 即座に確定させる。モバイルの投稿モーダル(App.svelte)はComposeBarを都度マウント/アンマウント
  // するため、これが無いと閉じる直前2秒以内に入力した内容が保存されずに失われる
  // (上の$effectのクリーンアップはキーストローク毎の再実行時にも走るため、そこでflushすると
  // デバウンスがキーストローク毎保存になってしまい使えない。onDestroyは実際のアンマウント時
  // にしか走らないため、ここでのみflushする)。
  onDestroy(() => {
    if (autoSaveTimer === undefined) return;
    clearTimeout(autoSaveTimer);
    if (!accountId) return;
    void unwrapAcc(accountId, commands.saveAutoDraft(accountId, buildDraftInput())).catch(() => {});
  });

  /// マウント時の自動復元。app.bootingが終わるまで待ってから一度だけ試みる:
  /// ComposeBarはapp.accountsが非空になった時点でマウントされるが、既定アカウント設定
  /// (app.ui.defaultAccountId)はそれより後にawaitを挟んで非同期に読み込まれる
  /// (store.svelte.tsのboot())。そのため、bootingがtrueのうちにaccountIdを読むと
  /// 複数アカウント環境では暫定的なfallback値(accounts[0])を掴むことがあり、誤った
  /// アカウントで復元を試みて「無し」と判定した直後に上の自動保存effectが「空なのでclear」を
  /// 呼んでしまい、accountIdが後から正しい既定アカウントへ補正された際にその正しいアカウントの
  /// 自動下書きを事故的に消してしまう。これを避けるため、boot完了(accountIdが安定した後)まで
  /// 待ってから一度だけ復元を試みる。
  $effect(() => {
    if (app.booting || autoRestoreDone) return;
    autoRestoreDone = true;
    if (!accountId || text.trim() || replyTo || quoteOf || attachments.length > 0) return;
    const acc = accountId;
    unwrapAcc(acc, commands.getAutoDraft(acc))
      .then((d) => {
        if (!d) return;
        // 復元試行中、他の初期化(app.compose消費など)で既に何か入力/文脈が付いていたら
        // 上書きしない
        if (text.trim() || replyTo || quoteOf || attachments.length > 0) return;
        void loadDraft(d);
      })
      .catch(() => {});
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

  async function addLocalAttachment(path: string) {
    const name = path.split(/[\\/]/).pop() ?? path;
    let previewUrl: string | null = null;
    if (IMAGE_EXTENSIONS.has(extLower(name))) {
      try {
        previewUrl = await unwrap(commands.readAttachmentPreview(path));
      } catch {
        previewUrl = null;
      }
    }
    attachments = [...attachments, { kind: "local", id: crypto.randomUUID(), path, name, previewUrl }];
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
      await addLocalAttachment(p);
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

  function computePollExpiresAt(): number | null {
    if (pollExpiryMode === "at" && pollExpiresAt) return new Date(pollExpiresAt).getTime();
    if (pollExpiryMode === "after") return Date.now() + pollAfterAmount * POLL_AFTER_UNIT_MS[pollAfterUnit];
    return null;
  }

  function buildDraftInput(): DraftInput {
    const choices = pollChoices.map((s) => s.trim()).filter(Boolean);
    return {
      text,
      cw: useCw && cw.trim() ? cw : null,
      visibility,
      localOnly,
      reactionAcceptance,
      channelId: useChannel && channelId ? channelId : null,
      poll: usePoll && choices.length >= 2 ? { choices, multiple: pollMultiple, expiresAt: computePollExpiresAt() } : null,
      fileIds: attachments.flatMap((a) => (a.kind === "drive" ? [a.file.id] : [])),
      replyNote: replyTo ? contextNoteToSnapshot(replyTo) : null,
      quoteNote: quoteOf ? contextNoteToSnapshot(quoteOf) : null,
    };
  }

  async function saveCurrentAsDraft() {
    if (!accountId) return;
    try {
      await unwrapAcc(accountId, commands.saveDraft(accountId, buildDraftInput()));
      await loadManualDrafts();
    } catch (e) {
      err = String(e);
    }
  }

  async function deleteManualDraft(id: string) {
    if (!accountId) return;
    try {
      await unwrapAcc(accountId, commands.deleteDraft(accountId, id));
      manualDrafts = manualDrafts.filter((d) => d.id !== id);
      if (loadedDraftId === id) loadedDraftId = null;
    } catch (e) {
      err = String(e);
    }
  }

  async function loadDraft(d: Draft) {
    text = d.text;
    cw = d.cw ?? "";
    useCw = d.cw != null;
    visibility = d.visibility;
    localOnly = d.localOnly;
    reactionAcceptance = d.reactionAcceptance;
    if (d.channelId) {
      useChannel = true;
      channelId = d.channelId;
    } else {
      useChannel = false;
      channelId = "";
    }
    if (d.poll) {
      usePoll = true;
      const padded = [...d.poll.choices];
      while (padded.length < 2) padded.push("");
      pollChoices = padded;
      pollMultiple = d.poll.multiple;
      if (d.poll.expiresAt != null) {
        pollExpiryMode = "at";
        // datetime-local入力/computePollExpiresAtは共にローカル時刻として文字列を扱うため、
        // toISOString()(UTC)ではなくローカル成分から組み立てる(でないとタイムゾーン分ずれる)。
        const dt = new Date(d.poll.expiresAt);
        const pad = (n: number) => String(n).padStart(2, "0");
        pollExpiresAt = `${dt.getFullYear()}-${pad(dt.getMonth() + 1)}-${pad(dt.getDate())}T${pad(dt.getHours())}:${pad(dt.getMinutes())}`;
      } else {
        pollExpiryMode = "none";
        pollExpiresAt = "";
      }
    } else {
      usePoll = false;
      pollChoices = ["", ""];
      pollMultiple = false;
      pollExpiryMode = "none";
      pollExpiresAt = "";
    }
    replyTo = d.replyNote ? snapshotToContextNote(d.replyNote) : undefined;
    quoteOf = d.quoteNote ? snapshotToContextNote(d.quoteNote) : undefined;
    attachments = [];
    if (d.fileIds.length > 0 && accountId) {
      const acc = accountId;
      const results = await Promise.allSettled(
        d.fileIds.map((id) => unwrapAcc(acc, commands.getDriveFile(acc, id))),
      );
      attachments = results.flatMap((r) =>
        r.status === "fulfilled" ? [{ kind: "drive" as const, id: r.value.id, file: r.value }] : [],
      );
    }
    loadedDraftId = d.kind === "manual" ? d.id : null;
    showDraftMenu = false;
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
    const expiresAt = computePollExpiresAt();

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
        reactionAcceptance,
      };
      await app.postNote(accountId, draft);
      const draftToDelete = loadedDraftId;
      void unwrapAcc(accountId, commands.clearAutoDraft(accountId)).catch(() => {});
      if (draftToDelete) {
        void unwrapAcc(accountId, commands.deleteDraft(accountId, draftToDelete)).catch(() => {});
      }
      loadedDraftId = null;
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
      reactionAcceptance = "all";
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
    <div class="flex items-center gap-1.5 rounded-md border border-border bg-muted px-1.5 py-[3px] text-sm text-muted-foreground">
      <span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
        {replyTo ? "返信: " : "引用: "}@{(replyTo ?? quoteOf)!.user.username} — {(replyTo ?? quoteOf)!.text ?? ""}
      </span>
      <Button type="button" variant="ghost" size="icon-xs" class="flex-none text-muted-foreground" title="キャンセル" onclick={cancelContext}><X size={12} /></Button>
    </div>
  {/if}

  {#if useCw}
    <input class="w-full box-border rounded border border-border bg-muted px-[9px] py-1.5 font-[inherit] text-sm text-foreground" placeholder="内容警告 (CW)" bind:value={cw} />
  {/if}

  <div class="relative">
    <textarea
      class={expanded
        ? "w-full box-border resize-y rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-sm leading-[1.4] text-foreground min-h-24 [transition:min-height_0.12s_ease]"
        : compact
          ? "w-full box-border resize-none rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-sm leading-[1.4] text-foreground min-h-[34px] [transition:min-height_0.12s_ease]"
          : "w-full box-border resize-y rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-sm leading-[1.4] text-foreground min-h-20 [transition:min-height_0.12s_ease]"}
      rows={expanded ? 4 : 1}
      placeholder={placeholder}
      data-testid="compose-textarea"
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
        <!-- text-[0.6rem]（このブロック内4箇所）はスタイルガイド(docs/design/style-guide.md §5)の対象外。
             28px/14px四方の固定サイズバッジのため例外的に即値を維持。 -->
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
          <Button type="button" variant="ghost" size="icon-xs" class="absolute -top-1 -right-1 h-3.5 w-3.5 rounded-full bg-black/60 text-white hover:bg-black/60" title="削除" onclick={() => removeAttached(a.id)}><X size={12} /></Button>
        </div>
      {/each}
    </div>
  {/if}

  {#if usePoll}
    <div class="flex flex-col gap-[5px]">
      {#each pollChoices as _, i}
        <div class="flex items-center gap-1">
          <input class="flex-1 box-border rounded border border-border bg-muted px-[9px] py-1.5 font-[inherit] text-sm text-foreground" placeholder={`選択肢 ${i + 1}`} bind:value={pollChoices[i]} />
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
      <div class="flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={pollChoices.length >= MAX_POLL_CHOICES}
          onclick={() => (pollChoices = [...pollChoices, ""])}
        >
          ＋選択肢
        </Button>
        <label><input type="checkbox" bind:checked={pollMultiple} /> 複数選択</label>
      </div>
      <div class="flex flex-wrap items-center gap-1.5 text-sm text-muted-foreground">
        <span class="flex-none">期限:</span>
        {#each pollExpiryModes as m (m.value)}
          <Button
            type="button"
            variant="outline"
            size="sm"
            class={pollExpiryMode === m.value ? "border-primary text-primary" : ""}
            onclick={() => (pollExpiryMode = m.value)}
          >
            {m.label}
          </Button>
        {/each}
        {#if pollExpiryMode === "at"}
          <input type="datetime-local" bind:value={pollExpiresAt} class="rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-sm text-foreground" />
        {:else if pollExpiryMode === "after"}
          <input
            type="number"
            min="1"
            class="w-[60px] rounded border border-border bg-muted px-1.5 py-[3px] font-[inherit] text-sm text-foreground"
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
        size="icon-sm"
        title="画像を添付"
        bind:ref={attachTrigger}
        onclick={toggleAttachMenu}
        disabled={busy}
      ><ImagePlus size={16} class="size-4" /></Button>
      <Button type="button" variant="outline" size="sm" class={useCw ? "border-primary text-primary" : ""} onclick={() => (useCw = !useCw)}>CW</Button>
      <Button type="button" variant="outline" size="sm" class={usePoll ? "border-primary text-primary" : ""} onclick={() => (usePoll = !usePoll)}>投票</Button>
      <Button type="button" variant="outline" size="sm" class={useChannel ? "border-primary text-primary" : ""} onclick={() => (useChannel = !useChannel)}>チャンネル</Button>
      <ReactionAcceptanceSelect bind:value={reactionAcceptance} />
      {#if useChannel}
        {#if channelsLoading}
          <span class="flex-none whitespace-nowrap text-sm text-muted-foreground">読み込み中…</span>
        {:else if channelsError}
          <span class="flex-none whitespace-nowrap text-sm text-muted-foreground">読み込みに失敗しました</span>
        {:else if channelOptions.length > 0}
          <div class="w-[140px]">
            <Dropdown bind:value={channelId} options={channelOptions} />
          </div>
        {:else}
          <span class="flex-none whitespace-nowrap text-sm text-muted-foreground">フォロー中のチャンネルがありません</span>
        {/if}
      {/if}
      <label class="flex-none whitespace-nowrap text-sm text-muted-foreground">
        <input
          type="checkbox"
          checked={useChannel || localOnly}
          disabled={useChannel}
          onchange={(e) => (localOnly = e.currentTarget.checked)}
        /> 連合なし
      </label>
    </div>
    <div class="flex flex-none flex-wrap items-center gap-1.5">
      <Button
        type="button"
        variant="outline"
        size="icon-sm"
        title="下書き"
        bind:ref={draftMenuTrigger}
        onclick={toggleDraftMenu}
        disabled={busy || !accountId}
      ><FileText size={16} class="size-4" /></Button>
      <Button type="button" size="sm" disabled={busy} onclick={submit} data-testid="compose-submit">{busy ? "…" : "投稿"}</Button>
    </div>
  </div>
  </div>
</div>

{#if err}
  <Modal title="エラー" onclose={() => (err = null)}>
    {#snippet children()}
      <p class="mb-3.5 mt-0 whitespace-pre-wrap break-words text-sm text-foreground">{err}</p>
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
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted disabled:cursor-default disabled:opacity-50"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseLocalUpload}
      >ローカルから選択</button>
      <button
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted disabled:cursor-default disabled:opacity-50"
        type="button"
        disabled={!accountId}
        title={accountId ? undefined : "アカウントを選択してください"}
        onclick={chooseDrivePicker}
      >ドライブから選択</button>
    </div>
  </div>
{/if}

{#if showDraftMenu && draftMenuPos}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (showDraftMenu = false)} role="presentation">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="fixed w-[280px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
      style={`left:${draftMenuPos.left}px;top:${draftMenuPos.top}px`}
      onclick={(e) => e.stopPropagation()}
      role="menu"
      tabindex="-1"
    >
      <button
        class="block w-full rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted"
        type="button"
        onclick={saveCurrentAsDraft}
      >現在の内容を下書き保存</button>
      <div class="my-1 border-t border-border"></div>
      {#if draftsLoading}
        <div class="px-2.5 py-[7px] text-sm text-muted-foreground">読み込み中…</div>
      {:else if manualDrafts.length === 0}
        <div class="px-2.5 py-[7px] text-sm text-muted-foreground">保存済みの下書きはありません</div>
      {:else}
        <div class="max-h-[280px] overflow-y-auto">
          {#each manualDrafts as d (d.id)}
            <div class="flex items-center gap-1">
              <button
                class="min-w-0 flex-1 truncate rounded-md px-2.5 py-[7px] text-left font-[inherit] text-sm text-foreground hover:bg-muted"
                type="button"
                title={d.text}
                onclick={() => loadDraft(d)}
              >{d.text.trim() || "(本文なし)"}</button>
              <Button type="button" variant="ghost" size="icon-xs" class="flex-none text-muted-foreground" title="削除" onclick={() => deleteManualDraft(d.id)}><X size={12} /></Button>
            </div>
          {/each}
        </div>
      {/if}
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
