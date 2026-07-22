// アプリの ViewModel（Svelte 5 runes）。視覚カラム(GroupView)=タブ(TabView)の集合を保持し、
// Rust からの columnNote / columnNotification / columnConnectionState を購読して更新する。
import { commands, events, unwrap, unwrapAcc, formatError, ForbiddenError } from "./ipc";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type {
  Account,
  Note,
  ConnectionState,
  EmojiDef,
  NoteDraft_Deserialize as NoteDraft,
  VisibilityInput,
  OpenedColumn,
  NoteUpdate,
  ColumnKind,
  FilterQuery,
  Notification,
  MuteConfig,
  NotifyConfig,
  UiPrefs,
  LatestRelease,
  Clip,
} from "../bindings/tauri.gen";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { KeyAction } from "./keymap";
import { unicodeEmojiUrl, type EmojiStyle } from "./emoji";
import { BACKGROUND_FIT_MODE_CSS } from "./backgroundFitMode";
import { BACKGROUND_POSITION_CSS } from "./backgroundPosition";
import { DEFAULT_PINNED_EMOJIS } from "./unicodeEmojiList";
import { applyThemeColors, findPreset, parseThemeRef } from "./theme";
import { isMobilePlatform } from "./platform";

const MAX_NOTES = 300; // タブあたり DOM に保持する上限（仮想化-lite）
const UPDATE_CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000; // 新バージョン確認の間隔（4時間）
const PRUNE_INTERVAL_MS = 6 * 60 * 60 * 1000; // ノートキャッシュ間引きの間隔（6時間）

/// タブ = 1タイムライン。
export interface TabView {
  id: string;
  accountId: string;
  kind: ColumnKind;
  /// 種別＋アカウントから自動生成した名前
  title: string;
  /// ユーザが付けたカスタム名（無ければ自動生成名を使う）
  customTitle: string | null;
  /// 適用中フィルタ（編集モーダルのプレフィル用）
  filter: FilterQuery;
  /// タブ単位の通知フィルタ（設定→通知のグローバルスイッチと両方ONで発火）
  notifyDesktop: boolean;
  notifySound: boolean;
  /// このタブの通知音（プリセットID/data URL）。空文字なら設定→通知のグローバル選択を継承
  notifySoundChoice: string;
  notes: Note[];
  notifications: Notification[];
  state: ConnectionState;
  loadingMore: boolean;
  selectedNoteId: string | null;
}

/// タブに表示する名前（カスタム名優先）。
export function tabName(t: TabView): string {
  return t.customTitle || kindLabel(t.kind);
}

/// 視覚カラム = タブの集合。幅と並び順を持ち、アクティブタブを表示する。
export interface GroupView {
  id: string;
  width: number;
  /// true ならウィンドウ幅に応じて自動調整(flex)。false なら width(px) 固定
  auto: boolean;
  tabs: TabView[];
  activeTabId: string;
}

/// 投稿フォーム（返信/引用の文脈つき）
export interface ComposeState {
  accountId: string;
  replyTo?: Note;
  quoteOf?: Note;
}

export type LogLevel = "info" | "success" | "warn" | "error";

// リアクションピッカーの所有者トークン。同じノートが複数のNoteCard（Renote直後の
// オリジナル＋Renote版の並列表示や、複数カラムでの重複表示）に同時に描画されうるため、
// noteId一致だけでは開いた側とは別のインスタンスにもピッカーが出てしまう。
// マウス操作は各NoteCardインスタンス固有のオブジェクト参照で、キーボード操作は
// フォーカス中タブのtabIdで識別し、一致するインスタンスだけが表示するようにする。
// id/tabId は文字列で保持する。$state はオブジェクトを深くプロキシするため、
// オブジェクト参照(===)で持つと state 経由で読み出した側が別プロキシになり
// 常に不一致になる（一意な文字列なら値比較になるためこの問題を回避できる）。
export type ReactPickerToken =
  | { kind: "instance"; id: string }
  | { kind: "keyboard"; tabId: string };
/// Backstage（操作ログ/エラー）の1エントリ。
export interface LogEntry {
  id: number;
  at: number; // epoch ms
  level: LogLevel;
  text: string;
  reauthAccountId?: string; // 403由来のエラーなら、再認証を促すボタンをBackstageが描画する
}

class AppStore {
  accounts = $state<Account[]>([]);
  groups = $state<GroupView[]>([]);
  booting = $state(true);
  error = $state<string | null>(null);
  compose = $state<ComposeState | null>(null);
  // スマホでは投稿欄を常時表示せず、モーダルとして開く(Issue #34/#35)。
  // openCompose() から一元的に開閉するので、返信/引用/新規投稿のどの経路でも自動で開く。
  showComposeModal = $state(false);
  emojis = $state<Record<string, EmojiDef[]>>({});
  mute = $state<MuteConfig>({ ngWords: [], ngUsers: [], ngInstances: [] });
  notify = $state<NotifyConfig>({ desktop: false, sound: false, soundChoice: "" });
  ui = $state<UiPrefs>({
    theme: "auto",
    defaultColumnWidth: 300,
    keymap: {},
    fontFamily: "",
    backgroundImage: "",
    backgroundDim: 0,
    backgroundBlur: 0,
    columnOpacity: 100,
    backgroundFitMode: "cover",
    backgroundPosition: "center",
    pinnedEmojis: DEFAULT_PINNED_EMOJIS,
    uiMode: "auto",
    defaultAccountId: "",
    emojiStyle: "twemoji",
    gapFillLimit: 200,
    customThemes: [],
    mediaThumbnailHeight: 200,
    noteCacheLimit: 10000,
    noteCacheMaxAgeDays: 0,
    noteCacheMaxSizeMb: 0,
  });
  // キーボード操作: フォーカス中カラムと、開いているリアクションピッカー
  focusedGroupId = $state<string | null>(null);
  reactPicker = $state<{ noteId: string; token: ReactPickerToken } | null>(null);
  // Backstage: 操作ログ・エラー（新しいものが先頭）
  logs = $state<LogEntry[]>([]);
  #logSeq = 0;
  // Backstage右下のステータス表示用（Krile風: DB件数/流速/起動からの経過）
  bootedAt = $state(Date.now());
  noteCount = $state(0);
  noteRatePerMin = $state(0);
  #statsTimer: ReturnType<typeof setInterval> | null = null;
  // 新バージョン通知（Issue #4）。起動時/「Tsumugiについて」表示時/数時間おきに確認する。
  updateAvailable = $state<LatestRelease | null>(null);
  #updateCheckTimer: ReturnType<typeof setInterval> | null = null;
  #loggedUpdateVersion: string | null = null;
  // ノートキャッシュの間引き（Issue #6）。起動時と数時間おきに実行する。
  #pruneTimer: ReturnType<typeof setInterval> | null = null;

  #unlisten: UnlistenFn[] = [];
  // columnId -> 直近の接続状態。resumeColumn/addColumn の await 解決前に届いた
  // ColumnConnectionState イベントを取りこぼさないための記録(タブがまだ groups に
  // 挿入されておらず #findTab が見つけられない一瞬の間に Connected が来て捨てられる、
  // という起動時レースが実際にあった)。#makeTab がタブ生成時にここから初期状態を拾う。
  #connState = new Map<string, ConnectionState>();
  // 直近に reconnecting を経由した columnId の集合。「再接続しました」ログを、
  // 起動時の connecting→connected ではなく実際の再接続時にだけ出すための判定に使う
  // (reconnecting→connecting→connected と必ず connecting を経由するため、
  // 直前状態が connecting かどうかでは区別できない)。
  #wasReconnecting = new Set<string>();

  /// Tauri イベント購読をすべて解除する。dev の HMR で古いインスタンスの
  /// リスナーが残り通知が多重化するのを防ぐために使う（本番では未使用）。
  teardown() {
    for (const u of this.#unlisten) u();
    this.#unlisten = [];
    if (this.#statsTimer !== null) {
      clearInterval(this.#statsTimer);
      this.#statsTimer = null;
    }
    if (this.#pruneTimer !== null) {
      clearInterval(this.#pruneTimer);
      this.#pruneTimer = null;
    }
    if (this.#updateCheckTimer !== null) {
      clearInterval(this.#updateCheckTimer);
      this.#updateCheckTimer = null;
    }
  }

  async boot() {
    this.booting = true;
    this.bootedAt = Date.now();
    try {
      this.accounts = await unwrap(commands.listAccounts());
      this.mute = await unwrap(commands.getMute());
      const notify = await unwrap(commands.getNotify());
      this.notify = { ...notify, soundChoice: notify.soundChoice ?? "" };
      const ui = await unwrap(commands.getUiPrefs());
      this.ui = {
        ...ui,
        keymap: ui.keymap ?? {},
        fontFamily: ui.fontFamily ?? "",
        backgroundImage: ui.backgroundImage ?? "",
        backgroundDim: ui.backgroundDim ?? 0,
        backgroundBlur: ui.backgroundBlur ?? 0,
        columnOpacity: ui.columnOpacity ?? 100,
        defaultAccountId: ui.defaultAccountId ?? "",
        emojiStyle: ui.emojiStyle ?? "twemoji",
        gapFillLimit: ui.gapFillLimit ?? 200,
        customThemes: ui.customThemes ?? [],
        mediaThumbnailHeight: ui.mediaThumbnailHeight ?? 200,
        noteCacheLimit: ui.noteCacheLimit ?? 10000,
        noteCacheMaxAgeDays: ui.noteCacheMaxAgeDays ?? 0,
        noteCacheMaxSizeMb: ui.noteCacheMaxSizeMb ?? 0,
      };
      this.#applyTheme(this.ui.theme);
      this.#applyFont(this.ui.fontFamily ?? "");
      this.#applyBackground(this.ui);
      this.#applyMediaThumbnailHeight(this.ui.mediaThumbnailHeight ?? 200);
      // サーバ側ミュート/ブロックを同期（カラム復元前に済ませ、初期取得へ反映）
      await Promise.all(this.accounts.map((a) => this.#syncServerMutes(a.id)));
      await this.#subscribe();
      const groupDefs = await unwrap(commands.listGroups());
      this.groups = groupDefs.map((g) => ({ id: g.id, width: g.width, auto: g.auto, tabs: [], activeTabId: "" }));
      const tabDefs = await unwrap(commands.listColumns());
      for (const tab of tabDefs) {
        try {
          const opened = await unwrap(commands.resumeColumn(tab.id));
          this.#insertTab(opened);
          this.#captureInitial(opened.column.id, opened.notes);
        } catch (e) {
          this.#fail(e);
        }
      }
      this.#log("success", "起動完了");
    } catch (e) {
      this.#fail(e);
    } finally {
      this.booting = false;
    }
    await this.#pollStats();
    if (this.#statsTimer !== null) clearInterval(this.#statsTimer);
    this.#statsTimer = setInterval(() => void this.#pollStats(), 10_000);

    void this.checkForUpdate();
    if (this.#updateCheckTimer !== null) clearInterval(this.#updateCheckTimer);
    this.#updateCheckTimer = setInterval(() => void this.checkForUpdate(), UPDATE_CHECK_INTERVAL_MS);

    void this.#pruneNoteCache();
    if (this.#pruneTimer !== null) clearInterval(this.#pruneTimer);
    this.#pruneTimer = setInterval(() => void this.#pruneNoteCache(), PRUNE_INTERVAL_MS);
  }

  /// 設定の上限に従ってノートキャッシュから古いノートを削除する（Issue #6）。失敗しても
  /// 致命的でないのでログのみ（削除件数が0件超の時だけBackstageに記録し、
  /// 通常運用では静かにしておく）。他の定期同期処理（#syncServerMutes）とログ文言を揃えている。
  async #pruneNoteCache() {
    try {
      const deleted = await unwrap(commands.pruneNoteCache());
      if (deleted > 0) this.#log("info", `ノートキャッシュを削除: ${deleted}件`);
    } catch (e) {
      this.#log("warn", `ノートキャッシュ削除に失敗: ${String(e)}`);
    }
  }

  /// GitHub Releases を確認し、新しいバージョンがあれば updateAvailable にセットして
  /// Backstage へ記録する（Issue #4）。起動時・「Tsumugiについて」表示時・数時間おきに呼ばれる。
  /// 同じバージョンを複数回ログに出さないよう、ログは初回検知時だけに絞る。
  async checkForUpdate() {
    try {
      const latest = await unwrap(commands.checkLatestRelease());
      this.updateAvailable = latest;
      if (latest && this.#loggedUpdateVersion !== latest.version) {
        this.#loggedUpdateVersion = latest.version;
        this.#log("info", `新しいバージョン v${latest.version} が公開されています`);
      }
    } catch {
      // オフライン等での失敗は静かに無視する（バックグラウンドの補助チェックのため）
    }
  }

  /// DBキャッシュ済みノート総数と、直近1分に「投稿された」(created_at基準)ノート件数を取得する。
  /// DBへのINSERT時刻ではなく実際の投稿時刻で数えることで、起動時ギャップ埋めや上スクロールでの
  /// 過去取得時に古いノートをまとめてupsertしても流速が誤って跳ね上がらないようにしている。
  async #pollStats() {
    try {
      const nowSec = Math.floor(Date.now() / 1000);
      const [count, recent] = await Promise.all([
        unwrap(commands.noteCount()),
        unwrap(commands.notesSince(nowSec - 60)),
      ]);
      this.noteCount = count;
      this.noteRatePerMin = recent;
    } catch {
      // ステータス表示用の補助情報なので、失敗してもログには出さず静かに諦める
    }
  }

  #makeTab(opened: OpenedColumn): TabView {
    const acc = this.accounts.find((a) => a.id === opened.column.accountId);
    const src = kindLabel(opened.column.kind);
    return {
      id: opened.column.id,
      accountId: opened.column.accountId,
      kind: opened.column.kind,
      title: acc ? `${src} @${acc.username}` : src,
      customTitle: opened.column.title,
      filter: opened.column.filter,
      notifyDesktop: opened.column.notifyDesktop,
      notifySound: opened.column.notifySound,
      notifySoundChoice: opened.column.notifySoundChoice,
      notes: opened.notes,
      notifications: opened.notifications,
      state: this.#connState.get(opened.column.id) ?? "connecting",
      loadingMore: false,
      selectedNoteId: null,
    };
  }

  /// OpenedColumn を該当グループに差し込む（無ければグループを作る）。
  #insertTab(opened: OpenedColumn) {
    let g = this.groups.find((x) => x.id === opened.group.id);
    if (!g) {
      g = { id: opened.group.id, width: opened.group.width, auto: opened.group.auto, tabs: [], activeTabId: "" };
      this.groups = [...this.groups, g];
    }
    const tab = this.#makeTab(opened);
    g.tabs = [...g.tabs, tab];
    if (!g.activeTabId) g.activeTabId = tab.id;
    if (!this.focusedGroupId) this.focusedGroupId = g.id;
    return tab;
  }

  // ---- Backstage（操作ログ） ----

  static #LOG_CAP = 300;
  // Rust側の log::Level には無い "success" は info 相当として送る。
  static #RUST_LEVEL: Record<LogLevel, "error" | "warn" | "info"> = {
    error: "error",
    warn: "warn",
    info: "info",
    success: "info",
  };
  #log(level: LogLevel, text: string, reauthAccountId?: string) {
    this.logs = [
      { id: ++this.#logSeq, at: Date.now(), level, text, reauthAccountId },
      ...this.logs,
    ].slice(0, AppStore.#LOG_CAP);
    if (this.ui.enableFileLogging) void commands.logFrontendEvent(AppStore.#RUST_LEVEL[level], text);
  }
  /// Backstage(UI)には出さず、設定でONならRust側のファイルログにのみ書く
  /// (Issue #12調査用の詳細ログ。debugレベルなのでBackstageの通常ログよりファイル側でのみ見える)。
  #logDebug(text: string) {
    if (this.ui.enableFileLogging) void commands.logFrontendEvent("debug", text);
  }
  /// エラーをバナー表示＋Backstage へ記録する共通処理。
  /// ForbiddenError なら「再認証」アクションをログ行に付与する。
  #fail(e: unknown) {
    const msg = String(e);
    this.error = msg;
    this.#log("error", msg, e instanceof ForbiddenError ? e.accountId : undefined);
  }
  clearLogs() {
    this.logs = [];
  }
  /// store の非同期フロー外(単発の子コンポーネント操作等)から失敗を報告する共通口。
  reportError(e: unknown) {
    this.#fail(e);
  }

  #allTabs(): TabView[] {
    return this.groups.flatMap((g) => g.tabs);
  }
  #findTab(tabId: string): TabView | undefined {
    for (const g of this.groups) {
      const t = g.tabs.find((t) => t.id === tabId);
      if (t) return t;
    }
    return undefined;
  }

  setActiveTab(groupId: string, tabId: string) {
    const g = this.groups.find((x) => x.id === groupId);
    if (g) g.activeTabId = tabId;
    this.focusedGroupId = groupId;
  }

  /// タブ名を変更（空なら自動生成名に戻す）。永続化して即反映。
  async renameTab(tabId: string, title: string) {
    const trimmed = title.trim();
    const value = trimmed.length > 0 ? trimmed : null;
    const tab = this.#findTab(tabId);
    if (tab) tab.customTitle = value;
    try {
      await unwrap(commands.renameColumn(tabId, value));
    } catch (e) {
      this.#fail(e);
    }
  }

  /// タブ単位の通知設定（デスクトップ/音）を変更する。ストリームは張り直さない。
  async setColumnNotify(
    tabId: string,
    notifyDesktop: boolean,
    notifySound: boolean,
    notifySoundChoice: string,
  ) {
    const tab = this.#findTab(tabId);
    if (tab) {
      tab.notifyDesktop = notifyDesktop;
      tab.notifySound = notifySound;
      tab.notifySoundChoice = notifySoundChoice;
    }
    try {
      await unwrap(commands.setColumnNotify(tabId, notifyDesktop, notifySound, notifySoundChoice));
    } catch (e) {
      this.#fail(e);
    }
  }

  /// 音声ファイルを選んで通知音として読み込む（data URL化のみ。保存は setColumnNotify/setNotify で）。
  async pickNotifySoundFile(): Promise<string | null> {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "音声", extensions: ["mp3", "wav", "ogg", "m4a", "aac", "flac", "webm"] }],
    });
    if (!path || Array.isArray(path)) return null;
    return unwrap(commands.readAudioDataUrl(path));
  }

  // ---- キーボード操作 ----

  #focusedGroup(): GroupView | undefined {
    return this.groups.find((g) => g.id === this.focusedGroupId) ?? this.groups[0];
  }
  #focusedTab(): TabView | undefined {
    const g = this.#focusedGroup();
    if (!g) return undefined;
    return g.tabs.find((t) => t.id === g.activeTabId) ?? g.tabs[0];
  }

  /// ノートをクリック等で選択（そのカラムにフォーカスも移す）。
  selectNote(tabId: string, noteId: string) {
    const t = this.#findTab(tabId);
    if (!t) return;
    t.selectedNoteId = noteId;
    for (const g of this.groups) {
      if (g.tabs.some((x) => x.id === tabId)) {
        this.focusedGroupId = g.id;
        break;
      }
    }
  }

  /// フォーカス中タブの選択を delta 分だけ動かす（ノートタブのみ）。
  #moveSelection(delta: number) {
    const t = this.#focusedTab();
    if (!t || t.kind.type === "notifications" || t.notes.length === 0) return;
    const cur = t.notes.findIndex((n) => n.id === t.selectedNoteId);
    let next = cur < 0 ? 0 : cur + delta;
    next = Math.max(0, Math.min(t.notes.length - 1, next));
    t.selectedNoteId = t.notes[next].id;
  }

  /// フォーカスを隣のカラムへ。
  #moveFocusColumn(delta: number) {
    if (this.groups.length === 0) return;
    const idx = this.groups.findIndex((g) => g.id === this.focusedGroupId);
    const cur = idx < 0 ? 0 : idx;
    const next = Math.max(0, Math.min(this.groups.length - 1, cur + delta));
    this.focusedGroupId = this.groups[next].id;
  }

  #selectedNote(): { tab: TabView; note: Note } | null {
    const t = this.#focusedTab();
    if (!t || t.kind.type === "notifications") return null;
    let note = t.notes.find((n) => n.id === t.selectedNoteId);
    if (!note) {
      // 未選択なら先頭を選択して対象にする
      note = t.notes[0];
      if (!note) return null;
      t.selectedNoteId = note.id;
    }
    return { tab: t, note };
  }

  /// キーバインドから呼ばれるアクション実行。
  runKeyAction(action: KeyAction) {
    switch (action) {
      case "note.next":
        this.#moveSelection(1);
        return;
      case "note.prev":
        this.#moveSelection(-1);
        return;
      case "column.prev":
        this.#moveFocusColumn(-1);
        return;
      case "column.next":
        this.#moveFocusColumn(1);
        return;
      case "compose.new": {
        const t = this.#focusedTab();
        const accountId = t?.accountId ?? this.defaultAccountId();
        if (accountId) this.openCompose(accountId);
        return;
      }
    }
    // 以降は選択ノートが対象
    const sel = this.#selectedNote();
    if (!sel) return;
    const target = !sel.note.text && sel.note.renote ? sel.note.renote : sel.note; // 純粋Renoteは中身
    switch (action) {
      case "note.reply":
        this.openCompose(sel.tab.accountId, { replyTo: target });
        return;
      case "note.quote":
        this.openCompose(sel.tab.accountId, { quoteOf: target });
        return;
      case "note.renote":
        void this.renote(sel.tab.accountId, target.id);
        return;
      case "note.react": {
        const isOpen =
          this.reactPicker?.noteId === target.id &&
          this.reactPicker?.token.kind === "keyboard" &&
          this.reactPicker.token.tabId === sel.tab.id;
        this.reactPicker = isOpen
          ? null
          : { noteId: target.id, token: { kind: "keyboard", tabId: sel.tab.id } };
        return;
      }
      case "note.open": {
        const acc = this.accounts.find((a) => a.id === sel.tab.accountId);
        if (acc) void openUrl(`https://${acc.host}/notes/${target.id}`);
        return;
      }
    }
  }

  // ---- タブの D&D（グループ内並べ替え / グループ間移動） ----

  draggingTabId = $state<string | null>(null);

  #findTabLoc(tabId: string): { group: GroupView; index: number } | null {
    for (const g of this.groups) {
      const index = g.tabs.findIndex((t) => t.id === tabId);
      if (index >= 0) return { group: g, index };
    }
    return null;
  }

  startDragTab(tabId: string) {
    this.draggingTabId = tabId;
  }

  /// ドラッグ中タブを overTab の位置へ（別グループなら移動）。ライブ反映。
  dragOverTab(overGroupId: string, overTabId: string) {
    const dragId = this.draggingTabId;
    if (!dragId || dragId === overTabId) return;
    const src = this.#findTabLoc(dragId);
    const dst = this.groups.find((g) => g.id === overGroupId);
    if (!src || !dst) return;
    const tab = src.group.tabs[src.index];
    src.group.tabs = src.group.tabs.filter((t) => t.id !== dragId);
    const overIdx = dst.tabs.findIndex((t) => t.id === overTabId);
    const at = overIdx < 0 ? dst.tabs.length : overIdx;
    dst.tabs = [...dst.tabs.slice(0, at), tab, ...dst.tabs.slice(at)];
    dst.activeTabId = dragId;
    this.groups = this.groups.filter((g) => g.tabs.length > 0);
  }

  /// タブバーの空き部分に落とした = そのグループの末尾へ。
  dragOverTabBarEnd(groupId: string) {
    const dragId = this.draggingTabId;
    if (!dragId) return;
    const src = this.#findTabLoc(dragId);
    const dst = this.groups.find((g) => g.id === groupId);
    if (!src || !dst) return;
    if (src.group.id === groupId && src.index === src.group.tabs.length - 1) return;
    const tab = src.group.tabs[src.index];
    src.group.tabs = src.group.tabs.filter((t) => t.id !== dragId);
    dst.tabs = [...dst.tabs, tab];
    dst.activeTabId = dragId;
    this.groups = this.groups.filter((g) => g.tabs.length > 0);
  }

  async endDragTab() {
    const dragId = this.draggingTabId;
    this.draggingTabId = null;
    if (!dragId) return;
    const loc = this.#findTabLoc(dragId);
    if (!loc) return;
    try {
      await unwrap(commands.moveTab(dragId, loc.group.id, loc.group.tabs.map((t) => t.id)));
      await unwrap(commands.reorderGroups(this.groups.map((g) => g.id)));
    } catch (e) {
      this.#fail(e);
    }
  }

  // ---- グループの並べ替え / 幅 ----

  draggingGroupId = $state<string | null>(null);

  startDragGroup(id: string) {
    this.draggingGroupId = id;
  }
  dragOverGroup(overId: string) {
    const from = this.groups.findIndex((c) => c.id === this.draggingGroupId);
    const to = this.groups.findIndex((c) => c.id === overId);
    if (from < 0 || to < 0 || from === to) return;
    const arr = [...this.groups];
    const [moved] = arr.splice(from, 1);
    arr.splice(to, 0, moved);
    this.groups = arr;
  }
  async endDragGroup() {
    if (!this.draggingGroupId) return;
    this.draggingGroupId = null;
    try {
      await unwrap(commands.reorderGroups(this.groups.map((g) => g.id)));
    } catch (e) {
      this.#fail(e);
    }
  }

  setGroupWidthLocal(groupId: string, width: number) {
    const g = this.groups.find((c) => c.id === groupId);
    if (g) g.width = width;
  }
  async persistGroupWidth(groupId: string, width: number) {
    try {
      await unwrap(commands.setGroupWidth(groupId, width));
    } catch (e) {
      this.#fail(e);
    }
  }

  async setGroupAuto(groupId: string, auto: boolean) {
    const g = this.groups.find((c) => c.id === groupId);
    if (g) g.auto = auto;
    try {
      await unwrap(commands.setGroupAuto(groupId, auto));
    } catch (e) {
      this.#fail(e);
    }
  }

  async #subscribe() {
    for (const u of this.#unlisten) u();
    this.#unlisten = [];

    this.#unlisten.push(
      await events.columnNote.listen((e) => {
        const tab = this.#findTab(e.payload.columnId);
        if (!tab) return;
        // ライブでノートを受信できている = 接続は生きている証拠。ConnectionState イベントの
        // 取りこぼし(順序/タイミングの問題等)があっても、ここで自己修復する。
        if (tab.state !== "connected") tab.state = "connected";
        if (tab.notes.some((n) => n.id === e.payload.note.id)) return;
        tab.notes = [e.payload.note, ...tab.notes].slice(0, MAX_NOTES);
        // 通知カラム以外でも「このタブに新着ノートが届いたら」通知できる（タブごとの設定次第）。
        // 通知種別(main チャンネル)はサーバ側で自分の操作を除外するが、Home/Local 等の
        // ストリームは自分の投稿もそのまま流れてくるため、自分のノートは通知しない。
        const acc = this.accounts.find((a) => a.id === tab.accountId);
        const isOwn = acc && e.payload.note.user.id === acc.userId;
        const wantsDesktop = !isOwn && this.notify.desktop && tab.notifyDesktop;
        const wantsSound = !isOwn && this.notify.sound && tab.notifySound;
        if ((wantsDesktop || wantsSound) && this.#markNotified(`note:${e.payload.note.id}`)) {
          this.#logDebug(`新着ノート通知を発火: desktop=${wantsDesktop} sound=${wantsSound} (${tabName(tab)})`);
          if (wantsDesktop) void this.#osNotifyNote(tab, e.payload.note);
          if (wantsSound) playNotifySound(this.#resolveSoundChoice(tab));
        }
      }),
    );
    this.#unlisten.push(
      await events.columnGapFill.listen((e) => {
        const tab = this.#findTab(e.payload.columnId);
        if (!tab) return;
        // 起動時のギャップ埋め結果をまとめて反映する。1件ずつの新着とは違い、
        // 新着通知/通知音は鳴らさない（不在中に溜まったノートで誤爆しないため）。
        const known = new Set(tab.notes.map((n) => n.id));
        const merged = [...tab.notes];
        for (const n of e.payload.notes) {
          if (!known.has(n.id)) merged.push(n);
        }
        merged.sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
        tab.notes = merged.slice(0, MAX_NOTES);
      }),
    );
    this.#unlisten.push(
      await events.columnConnectionState.listen((e) => {
        // タブがまだ #insertTab 前でも(resumeColumn/addColumn の await 解決前に
        // Connected が届くことがある)状態を取りこぼさないよう、まず記録する。
        this.#connState.set(e.payload.columnId, e.payload.state);
        const tab = this.#findTab(e.payload.columnId);
        if (!tab) return;
        tab.state = e.payload.state;
        // 状態遷移を Backstage に記録（起動時の connecting→connected は除外）
        const name = tab.title;
        if (e.payload.state === "error") this.#log("error", `接続エラー: ${name}`);
        else if (e.payload.state === "reconnecting") {
          this.#wasReconnecting.add(e.payload.columnId);
          this.#log("warn", `再接続中: ${name}`);
        } else if (e.payload.state === "connected" && this.#wasReconnecting.delete(e.payload.columnId)) {
          this.#log("success", `再接続しました: ${name}`);
        }
      }),
    );
    this.#unlisten.push(
      await events.columnNoteUpdated.listen((e) => this.#applyNoteUpdate(e.payload)),
    );
    this.#unlisten.push(
      await events.columnNotification.listen((e) => {
        const tab = this.#findTab(e.payload.columnId);
        if (!tab) return;
        if (tab.notifications.some((n) => n.id === e.payload.notification.id)) return;
        tab.notifications = [e.payload.notification, ...tab.notifications].slice(0, MAX_NOTES);
        // 通知の到着タイミングを Backstage に残す。接続断→再接続のログと突き合わせれば
        // 「通知が謎のタイミングで届く」系の問題を後から追いやすくなる。
        const n = e.payload.notification;
        const who = n.user ? `@${n.user.username}` : "";
        this.#log("info", `通知を受信: ${n.type}${who ? ` ${who}` : ""} (${tabName(tab)})`);
        // 発火条件は「設定→通知のグローバルスイッチ」と「このタブの通知設定」の両方がON。
        // デスクトップ通知 / 音は「通知IDでグローバルに1回だけ」。通知カラムが複数あると
        // 同じ通知が各カラムに届くため、ここで重複を弾く（このタブが望まない場合は
        // dedup 枠を消費しない＝別タブに同じ通知が来たときそちらで発火できるようにする）。
        const wantsDesktop = this.notify.desktop && tab.notifyDesktop;
        const wantsSound = this.notify.sound && tab.notifySound;
        if ((wantsDesktop || wantsSound) && this.#markNotified(e.payload.notification.id)) {
          this.#logDebug(`通知を発火: desktop=${wantsDesktop} sound=${wantsSound} (${tabName(tab)})`);
          if (wantsDesktop) void this.#osNotify(e.payload.notification);
          if (wantsSound) playNotifySound(this.#resolveSoundChoice(tab));
        }
      }),
    );
  }

  #applyNoteUpdate(p: {
    columnId: string;
    noteId: string;
    update: NoteUpdate;
    actorId: string | null;
  }) {
    const tab = this.#findTab(p.columnId);
    if (!tab) return;

    if (p.update.type === "deleted") {
      tab.notes = tab.notes.filter((n) => n.id !== p.noteId);
      return;
    }
    const targets: Note[] = [];
    for (const n of tab.notes) {
      if (n.id === p.noteId) targets.push(n);
      if (n.renote && n.renote.id === p.noteId) targets.push(n.renote);
    }
    if (targets.length === 0) return;

    const isMine = p.actorId != null && this.#myUserIds().has(p.actorId);
    for (const n of targets) {
      switch (p.update.type) {
        case "reacted":
          if (isMine) break;
          n.reactions[p.update.reaction] = (n.reactions[p.update.reaction] ?? 0) + 1;
          n.reactionCount += 1;
          break;
        case "unreacted": {
          if (isMine) break;
          const key = p.update.reaction;
          const next = (n.reactions[key] ?? 1) - 1;
          if (next <= 0) delete n.reactions[key];
          else n.reactions[key] = next;
          n.reactionCount = Math.max(0, n.reactionCount - 1);
          break;
        }
        case "pollVoted":
          if (isMine) break;
          if (n.poll && n.poll.choices[p.update.choice]) n.poll.choices[p.update.choice].votes += 1;
          break;
      }
    }
  }

  #myUserIds(): Set<string> {
    return new Set(this.accounts.map((a) => a.userId));
  }

  #captureInitial(tabId: string, notes: Note[]) {
    const ids = notes.map((n) => n.id);
    if (ids.length > 0) void commands.captureNotes(tabId, ids);
  }

  async addAccount(host: string): Promise<string> {
    const session = await unwrap(commands.startMiauth(host));
    await openUrl(session.url);
    return session.sessionId;
  }

  async completeAccount(sessionId: string) {
    const account = await unwrap(commands.completeMiauth(sessionId));
    if (!this.accounts.some((a) => a.id === account.id)) {
      this.accounts = [...this.accounts, account];
    }
    this.#log("success", `アカウントを追加: @${account.username}@${account.host}`);
    await this.#syncServerMutes(account.id);
  }

  /// サーバ側ミュート/ブロックを同期（失敗しても致命的でないのでログのみ）。
  async #syncServerMutes(accountId: string) {
    try {
      const n = await unwrapAcc(accountId, commands.syncServerMutes(accountId));
      if (n > 0) this.#log("info", `サーバのミュート/ブロックを同期: ${n}件`);
    } catch (e) {
      if (e instanceof ForbiddenError) {
        this.#log("warn", "サーバミュート同期: 権限不足。再認証してください", e.accountId);
      } else {
        this.#log("warn", `サーバミュート同期に失敗: ${String(e)}`);
      }
    }
  }

  async removeAccount(accountId: string) {
    const acc = this.accounts.find((a) => a.id === accountId);
    await unwrap(commands.removeAccount(accountId));
    this.accounts = this.accounts.filter((a) => a.id !== accountId);
    for (const t of this.#allTabs().filter((t) => t.accountId === accountId)) {
      await this.closeTab(t.id);
    }
    this.#log("info", `アカウントを削除: ${acc ? `@${acc.username}@${acc.host}` : accountId}`);
  }

  /// タブを追加する。`groupId` を指定するとそのカラムに、None なら新しいカラムを作る。
  async addColumn(
    accountId: string,
    kind: ColumnKind,
    filter: FilterQuery,
    groupId?: string,
    title?: string,
  ) {
    try {
      const opened = await unwrapAcc(
        accountId,
        commands.addColumn(accountId, kind, filter, groupId ?? null),
      );
      const tab = this.#insertTab(opened);
      const g = this.groups.find((x) => x.id === opened.group.id);
      if (g) g.activeTabId = tab.id;
      this.#captureInitial(opened.column.id, opened.notes);
      const name = title?.trim();
      if (name) await this.renameTab(tab.id, name);
      this.#log("success", `カラムを追加: ${name || kindLabel(kind)}`);
      return tab;
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// 既存タブのソース/フィルタ/名前を変更し、ストリームを張り直して内容を差し替える。
  async updateColumn(tabId: string, kind: ColumnKind, filter: FilterQuery, title?: string) {
    const name = title?.trim() || null;
    const opened = await unwrap(commands.updateColumn(tabId, kind, filter, name));
    const tab = this.#findTab(tabId);
    if (tab) {
      const acc = this.accounts.find((a) => a.id === opened.column.accountId);
      const src = kindLabel(opened.column.kind);
      tab.kind = opened.column.kind;
      tab.filter = opened.column.filter;
      tab.customTitle = opened.column.title;
      tab.title = acc ? `${src} @${acc.username}` : src;
      tab.notes = opened.notes;
      tab.notifications = opened.notifications;
      tab.selectedNoteId = null;
      tab.state = "connecting";
    }
    this.#captureInitial(tabId, opened.notes);
    this.#log("success", `タブを更新: ${opened.column.title || kindLabel(kind)}`);
  }

  async validateFilter(filter: FilterQuery): Promise<string | null> {
    const r = await commands.validateFilter(filter);
    return r.status === "ok" ? null : formatError(r.error);
  }

  /// エキスパートモード(TQL複数ソース)の `from ... where ...` 全文の構文検証のみ。
  /// list/antenna/channel の id 存在確認や user acct 解決は行わない。
  async validateTqlQuery(text: string): Promise<string | null> {
    const r = await commands.validateTqlQuery(text);
    return r.status === "ok" ? null : formatError(r.error);
  }

  async fetchUserLists(accountId: string) {
    try {
      return await unwrapAcc(accountId, commands.listUserLists(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  async fetchAntennas(accountId: string) {
    try {
      return await unwrapAcc(accountId, commands.listAntennas(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  async fetchChannels(accountId: string) {
    try {
      return await unwrapAcc(accountId, commands.listChannels(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// acct（@user@host）から userId を解決する。
  async resolveUser(accountId: string, acct: string) {
    try {
      return await unwrapAcc(accountId, commands.resolveUserAcct(accountId, acct));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  /// 通知設定を保存。desktop を有効化したら権限を要求する。
  async setNotify(config: NotifyConfig) {
    if (config.desktop && !(await isPermissionGranted())) {
      const p = await requestPermission();
      if (p !== "granted") config = { ...config, desktop: false };
    }
    await unwrap(commands.setNotify(config));
    this.notify = config;
    this.#log("info", "通知設定を保存しました");
  }

  /// 表示設定（テーマ・既定カラム幅・フォント）を保存し、即時反映。
  async setUiPrefs(prefs: UiPrefs) {
    await unwrap(commands.setUiPrefs(prefs));
    this.ui = {
      ...prefs,
      keymap: prefs.keymap ?? {},
      fontFamily: prefs.fontFamily ?? "",
      backgroundImage: prefs.backgroundImage ?? "",
      backgroundDim: prefs.backgroundDim ?? 0,
      backgroundBlur: prefs.backgroundBlur ?? 0,
      columnOpacity: prefs.columnOpacity ?? 100,
      pinnedEmojis: prefs.pinnedEmojis ?? DEFAULT_PINNED_EMOJIS,
      defaultAccountId: prefs.defaultAccountId ?? "",
      emojiStyle: prefs.emojiStyle ?? "twemoji",
      gapFillLimit: prefs.gapFillLimit ?? 200,
      customThemes: prefs.customThemes ?? [],
      mediaThumbnailHeight: prefs.mediaThumbnailHeight ?? 200,
      noteCacheLimit: prefs.noteCacheLimit ?? 10000,
      noteCacheMaxAgeDays: prefs.noteCacheMaxAgeDays ?? 0,
      noteCacheMaxSizeMb: prefs.noteCacheMaxSizeMb ?? 0,
    };
    this.#applyTheme(prefs.theme);
    this.#applyFont(prefs.fontFamily ?? "");
    this.#applyBackground(this.ui);
    this.#applyMediaThumbnailHeight(this.ui.mediaThumbnailHeight ?? 200);
    this.#log("info", "表示設定を保存しました");
  }

  /// 既定アカウントの id。未設定/削除済みならアカウント一覧の先頭にフォールバック。
  defaultAccountId(): string {
    const id = this.ui.defaultAccountId;
    if (id && this.accounts.some((a) => a.id === id)) return id;
    return this.accounts[0]?.id ?? "";
  }

  /// 実効UIモード(投稿モーダル+FABのモバイル版か、常時投稿欄のPC版か)。
  /// 設定→表示のuiModeが"auto"ならOS判定(Android/iOS)、それ以外は強制切替する（Issue #51）。
  useMobileUi(): boolean {
    if (this.ui.uiMode === "mobile") return true;
    if (this.ui.uiMode === "desktop") return false;
    return isMobilePlatform;
  }

  /// Unicode絵文字の画像URL（native設定時は null＝生テキストのまま表示）。
  /// 画像はアプリに同梱済み(@misskey-dev/emoji-assets)なのでインスタンス通信は発生しない。
  emojiImageUrl(char: string): string | null {
    return unicodeEmojiUrl(char, (this.ui.emojiStyle as EmojiStyle) ?? "twemoji");
  }

  /// 画像ファイルを選んで背景画像として読み込む（data URL化のみ。保存は setUiPrefs で）。
  async pickBackgroundImage(): Promise<string | null> {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "画像", extensions: ["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"] }],
    });
    if (!path || Array.isArray(path)) return null;
    return unwrap(commands.readImageDataUrl(path));
  }

  /// キーバインドの上書きを保存（他の表示設定は据え置き）。
  async setKeymap(overrides: Record<string, string>) {
    await unwrap(commands.setUiPrefs({ ...this.ui, keymap: overrides }));
    this.ui = { ...this.ui, keymap: overrides };
    this.#log("info", "キー割り当てを更新しました");
  }

  /// リアクションピッカーのピン留め絵文字リストを丸ごと差し替える（Issue #19、設定→リアクションで編集）。
  /// key は Unicode絵文字はそのまま、カスタム絵文字は ":name:" 形式。
  async setPinnedEmojis(list: string[]) {
    await unwrap(commands.setUiPrefs({ ...this.ui, pinnedEmojis: list }));
    this.ui = { ...this.ui, pinnedEmojis: list };
  }

  /// data-theme(auto/light/dark)またはプリセット/カスタムテーマの配色を <html> に反映する。
  #applyTheme(theme: string) {
    const root = document.documentElement;
    if (theme === "light" || theme === "dark") {
      root.dataset.theme = theme;
    } else {
      delete root.dataset.theme;
    }

    const presetId = parseThemeRef(theme, "preset:");
    const customId = parseThemeRef(theme, "custom:");
    if (presetId) {
      applyThemeColors(findPreset(presetId)?.colors ?? null);
      return;
    }
    if (customId) {
      const found = (this.ui.customThemes ?? []).find((t) => t.id === customId);
      if (found) {
        applyThemeColors(found.colors);
      } else {
        // 選択中のカスタムテーマが削除済み: auto にフォールバックして保存し直す
        applyThemeColors(null);
        void this.setUiPrefs({ ...this.ui, theme: "auto" });
      }
      return;
    }
    applyThemeColors(null);
  }

  /// --font-family を <html> に反映（CSS font-family 値をそのまま渡す）。
  /// 空文字なら未設定に戻し、app.css の既定フォントスタックへフォールバックさせる。
  #applyFont(fontFamily: string) {
    const root = document.documentElement;
    if (fontFamily.trim()) {
      root.style.setProperty("--font-family", fontFamily);
    } else {
      root.style.removeProperty("--font-family");
    }
  }

  /// 背景画像/オーバーレイ/カラム不透明度を <html> に反映する。
  #applyBackground(
    prefs: Pick<
      UiPrefs,
      | "backgroundImage"
      | "backgroundDim"
      | "backgroundBlur"
      | "columnOpacity"
      | "backgroundFitMode"
      | "backgroundPosition"
    >,
  ) {
    const root = document.documentElement;
    const img = prefs.backgroundImage ?? "";
    if (img) {
      root.style.setProperty("--bg-image", `url("${img}")`);
    } else {
      root.style.removeProperty("--bg-image");
    }
    root.style.setProperty("--bg-dim", String((prefs.backgroundDim ?? 0) / 100));
    root.style.setProperty("--bg-blur", `${prefs.backgroundBlur ?? 0}px`);
    root.style.setProperty("--column-opacity", `${prefs.columnOpacity ?? 100}%`);
    const [bgSize, bgRepeat] = BACKGROUND_FIT_MODE_CSS[prefs.backgroundFitMode ?? "cover"] ??
      BACKGROUND_FIT_MODE_CSS.cover;
    root.style.setProperty("--bg-size", bgSize);
    root.style.setProperty("--bg-repeat", bgRepeat);
    const bgPosition = BACKGROUND_POSITION_CSS[prefs.backgroundPosition ?? "center"] ??
      BACKGROUND_POSITION_CSS.center;
    root.style.setProperty("--bg-position", bgPosition);
  }

  /// メディアサムネイルの高さ上限を <html> に反映する（ノートを詰めたい人は小さく、
  /// 大きく見たい人は大きくできるように設定可能にしてある）。
  #applyMediaThumbnailHeight(px: number) {
    document.documentElement.style.setProperty("--media-thumbnail-height", `${px}px`);
  }

  // OS通知/音を出した通知IDを覚えておき、複数カラムからの重複配信を1回に抑える。
  #notifiedIds = new Set<string>();
  #notifiedOrder: string[] = [];
  #markNotified(id: string): boolean {
    if (this.#notifiedIds.has(id)) return false;
    this.#notifiedIds.add(id);
    this.#notifiedOrder.push(id);
    if (this.#notifiedOrder.length > 500) {
      const old = this.#notifiedOrder.shift();
      if (old) this.#notifiedIds.delete(old);
    }
    return true;
  }

  /// このタブで実際に鳴らす通知音を決める。タブ側の選択があればそれを優先し、
  /// 無ければ設定→通知のグローバル選択を継承する（どちらも空なら既定=beep）。
  #resolveSoundChoice(tab: TabView): string {
    return tab.notifySoundChoice || this.notify.soundChoice || "";
  }

  async #osNotify(n: Notification) {
    const actor = n.user ? (n.user.name ?? n.user.username) : "";
    const title = `${actor} ${notifActionLabel(n.type)}`.trim();
    const body = n.note?.text ?? (n.reaction ?? "");
    await this.#osNotifyRaw(title || "通知", body);
  }

  /// タブ(通知種別以外)に新着ノートが届いたときのOS通知。
  async #osNotifyNote(tab: TabView, note: Note) {
    const actor = note.user.name ?? note.user.username;
    const title = `${actor}（${tabName(tab)}）`;
    const body = note.text ?? (note.cw ? `CW: ${note.cw}` : note.files.length > 0 ? "(添付ファイル)" : "");
    await this.#osNotifyRaw(title, body);
  }

  async #osNotifyRaw(title: string, body: string) {
    try {
      if (!(await isPermissionGranted())) return;
      sendNotification({ title, body });
    } catch {
      // 通知失敗は無視
    }
  }

  /// NG 設定を保存し、表示中の該当ノートも即座に取り除く。
  async setMute(config: MuteConfig) {
    await unwrap(commands.setMute(config));
    this.mute = config;
    for (const t of this.#allTabs()) {
      t.notes = t.notes.filter((n) => !isMuted(n, config));
    }
    this.#log("info", "NG（ミュート）設定を保存しました");
  }

  async loadMore(tabId: string) {
    const tab = this.#findTab(tabId);
    if (!tab || tab.loadingMore) return;
    tab.loadingMore = true;
    try {
      if (tab.kind.type === "notifications") {
        if (tab.notifications.length === 0) return;
        const oldest = tab.notifications[tab.notifications.length - 1].id;
        const older = await unwrap(commands.fetchNotificationsBackfill(tab.id, oldest));
        const known = new Set(tab.notifications.map((n) => n.id));
        tab.notifications = [...tab.notifications, ...older.filter((n) => !known.has(n.id))].slice(0, MAX_NOTES);
      } else {
        if (tab.notes.length === 0) return;
        const oldest = tab.notes[tab.notes.length - 1].id;
        const older = await unwrap(commands.fetchBackfill(tab.id, oldest));
        const known = new Set(tab.notes.map((n) => n.id));
        const fresh = older.filter((n) => !known.has(n.id));
        tab.notes = [...tab.notes, ...fresh].slice(0, MAX_NOTES);
        // captureInitial 同様に subNote 購読しないと、この先そのノートへの
        // リアクション追加/削除が noteUpdated イベントとして届かず反映されない
        // （Issue #3: リアクションが表示されたりされなかったりする）。
        this.#captureInitial(tab.id, fresh);
      }
    } catch (e) {
      this.#fail(e);
    } finally {
      tab.loadingMore = false;
    }
  }

  /// タブを閉じる。グループが空になったらグループも消す。
  async closeTab(tabId: string) {
    await unwrap(commands.closeColumn(tabId));
    this.#connState.delete(tabId);
    this.#wasReconnecting.delete(tabId);
    for (const g of this.groups) {
      if (!g.tabs.some((t) => t.id === tabId)) continue;
      g.tabs = g.tabs.filter((t) => t.id !== tabId);
      if (g.activeTabId === tabId) g.activeTabId = g.tabs[0]?.id ?? "";
    }
    this.groups = this.groups.filter((g) => g.tabs.length > 0);
    if (this.groups.length > 0 && !this.groups.some((g) => g.id === this.focusedGroupId)) {
      this.focusedGroupId = this.groups[0].id;
    }
    this.#log("info", "タブを閉じました");
  }

  // ---- Phase 3: 投稿・リアクション ----

  openCompose(accountId: string, opts: { replyTo?: Note; quoteOf?: Note } = {}) {
    this.compose = { accountId, ...opts };
    // モバイル版UIは投稿欄がモーダル内にしか無いため、シグナルを消費できるよう先に開いておく
    // (PC版は常時表示なので不要)。isMobilePlatform(OS生判定)ではなく実効UIモード
    // (設定→表示のuiModeで上書き可能。Issue #51)で判定する。
    if (this.useMobileUi()) this.showComposeModal = true;
  }

  async postNote(accountId: string, draft: NoteDraft) {
    try {
      await unwrapAcc(accountId, commands.postNote(accountId, draft));
      this.#log("success", "投稿しました");
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  async renote(accountId: string, noteId: string, visibility: VisibilityInput = "public") {
    try {
      await unwrapAcc(accountId, commands.renote(accountId, noteId, visibility));
      this.#log("success", "Renote しました");
    } catch (e) {
      this.#fail(e);
    }
  }

  async deleteNote(accountId: string, noteId: string) {
    try {
      await unwrapAcc(accountId, commands.deleteNoteCmd(accountId, noteId));
      for (const t of this.#allTabs()) t.notes = t.notes.filter((n) => n.id !== noteId);
      this.#log("info", "ノートを削除しました");
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  #emojiLoads = new Map<string, Promise<EmojiDef[]>>();
  async loadEmojis(accountId: string): Promise<EmojiDef[]> {
    if (this.emojis[accountId]) return this.emojis[accountId];
    // 同一アカウントの並行呼び出しは1リクエストに集約する
    const inflight = this.#emojiLoads.get(accountId);
    if (inflight) return inflight;
    const p = unwrap(commands.listCustomEmojis(accountId))
      .then((list) => {
        this.emojis = { ...this.emojis, [accountId]: list };
        return list;
      })
      .finally(() => this.#emojiLoads.delete(accountId));
    this.#emojiLoads.set(accountId, p);
    return p;
  }

  /// アカウント（＝閲覧インスタンス）のローカルカスタム絵文字を name->url で返す。
  /// 最近の Misskey はローカル絵文字を note.emojis に含めないため、本文/リアクションの
  /// フォールバックに使う。未ロードなら取得を仕掛けて空を返す（ロード後に再描画される）。
  localEmojiUrls(accountId: string): Record<string, string> {
    const list = this.emojis[accountId];
    if (!list) {
      void this.loadEmojis(accountId).catch(() => {});
      return {};
    }
    const m: Record<string, string> = {};
    for (const e of list) if (!e.host) m[e.name] = e.url;
    return m;
  }

  async toggleReaction(accountId: string, noteId: string, reaction: string) {
    const targets = this.#collectNotes(noteId);
    if (targets.length === 0) return;
    const backups = targets.map((n) => snapshotReaction(n));
    const already = targets[0].myReaction;

    if (already === reaction) targets.forEach(removeReaction);
    else targets.forEach((n) => addReaction(n, reaction));

    try {
      if (already === reaction) {
        await unwrapAcc(accountId, commands.unreact(accountId, noteId));
        this.#log("info", "リアクションを取り消しました");
      } else {
        if (already) await unwrapAcc(accountId, commands.unreact(accountId, noteId));
        await unwrapAcc(accountId, commands.react(accountId, noteId, reaction));
        this.#log("success", `リアクション ${reaction}`);
      }
    } catch (e) {
      backups.forEach(restoreReaction);
      this.#fail(e);
    }
  }

  async toggleFavorite(accountId: string, noteId: string) {
    const targets = this.#collectNotes(noteId);
    if (targets.length === 0) return;
    const backups = targets.map((n) => ({ n, was: n.isFavoritedByMe }));
    const already = targets[0].isFavoritedByMe;
    targets.forEach((n) => (n.isFavoritedByMe = !already));

    try {
      if (already) {
        await unwrapAcc(accountId, commands.unfavoriteNote(accountId, noteId));
        this.#log("info", "お気に入りを解除しました");
      } else {
        await unwrapAcc(accountId, commands.favoriteNote(accountId, noteId));
        this.#log("success", "お気に入りに登録しました");
      }
    } catch (e) {
      const staleState = e instanceof Error && (e.message.includes("ALREADY_FAVORITED") || e.message.includes("NOT_FAVORITED"));
      if (staleState) {
        // サーバ側は既に希望の状態。is_favorited_by_me はバックフィルされないため
        // ローカルの表示状態がズレていただけ — 楽観的更新をそのまま確定させる。
        this.#log("info", "お気に入り状態を更新しました");
        return;
      }
      backups.forEach(({ n, was }) => (n.isFavoritedByMe = was));
      this.#fail(e);
    }
  }

  async listClips(accountId: string): Promise<Clip[]> {
    try {
      return await unwrapAcc(accountId, commands.listClips(accountId));
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  async createClip(accountId: string, name: string): Promise<Clip> {
    try {
      const clip = await unwrapAcc(accountId, commands.createClip(accountId, name));
      this.#log("success", `クリップを作成しました: ${clip.name}`);
      return clip;
    } catch (e) {
      this.#fail(e);
      throw e;
    }
  }

  async addNoteToClip(accountId: string, clipId: string, noteId: string) {
    try {
      await unwrapAcc(accountId, commands.addNoteToClip(accountId, clipId, noteId));
      this.#log("success", "クリップに追加しました");
    } catch (e) {
      this.#fail(e);
    }
  }

  async votePoll(accountId: string, noteId: string, choice: number) {
    const targets = this.#collectNotes(noteId).filter((n) => n.poll);
    if (targets.length === 0) return;
    const backups = targets.map((n) => snapshotPoll(n));
    targets.forEach((n) => applyVote(n, choice));

    try {
      await unwrapAcc(accountId, commands.votePoll(accountId, noteId, choice));
      this.#log("success", "投票しました");
    } catch (e) {
      backups.forEach(restorePoll);
      this.#fail(e);
    }
  }

  #collectNotes(noteId: string): Note[] {
    const out: Note[] = [];
    for (const t of this.#allTabs()) {
      for (const n of t.notes) {
        if (n.id === noteId) out.push(n);
        if (n.renote && n.renote.id === noteId) out.push(n.renote);
      }
    }
    return out;
  }
}

// ---- リアクションのローカル操作（Misskey は 1ユーザ1リアクション） ----

function addReaction(n: Note, reaction: string) {
  if (n.myReaction) removeReaction(n);
  n.reactions[reaction] = (n.reactions[reaction] ?? 0) + 1;
  n.myReaction = reaction;
  n.reactionCount += 1;
}
function removeReaction(n: Note) {
  const cur = n.myReaction;
  if (!cur) return;
  const next = (n.reactions[cur] ?? 1) - 1;
  if (next <= 0) delete n.reactions[cur];
  else n.reactions[cur] = next;
  n.reactionCount = Math.max(0, n.reactionCount - 1);
  n.myReaction = null;
}
function snapshotReaction(n: Note) {
  return { n, reactions: { ...n.reactions }, myReaction: n.myReaction, count: n.reactionCount };
}
function restoreReaction(s: ReturnType<typeof snapshotReaction>) {
  s.n.reactions = s.reactions;
  s.n.myReaction = s.myReaction;
  s.n.reactionCount = s.count;
}

// ---- 投票のローカル操作（1ユーザにつき: multiple=false なら1票のみ、trueなら選択肢ごとに1票） ----

function applyVote(n: Note, choice: number) {
  const c = n.poll?.choices[choice];
  if (!c) return;
  c.isVoted = true;
  c.votes += 1;
}
function snapshotPoll(n: Note) {
  return { n, choices: n.poll!.choices.map((c) => ({ ...c })) };
}
function restorePoll(s: ReturnType<typeof snapshotPoll>) {
  s.n.poll!.choices = s.choices;
}

// ---- NG 判定（Rust の filter::mute と同じ規則。表示中ノートの即時除去用） ----

function acctOf(u: Note["user"]): string {
  return u.host ? `@${u.username}@${u.host}` : `@${u.username}`;
}
function normalizeAcct(s: string): string {
  const t = s.trim().toLowerCase();
  return t.startsWith("@") ? t : `@${t}`;
}
function noteMutedOne(n: Note, cfg: MuteConfig): boolean {
  if (n.user.host) {
    const h = n.user.host.toLowerCase();
    if (cfg.ngInstances.some((i) => i.trim() && i.trim().toLowerCase() === h)) return true;
  }
  const acct = acctOf(n.user).toLowerCase();
  if (cfg.ngUsers.some((u) => u.trim() && normalizeAcct(u) === acct)) return true;
  const hay = `${n.text ?? ""} ${n.cw ?? ""}`.toLowerCase();
  return cfg.ngWords.some((w) => w.trim() && hay.includes(w.trim().toLowerCase()));
}
export function isMuted(n: Note, cfg: MuteConfig): boolean {
  return noteMutedOne(n, cfg) || (n.renote ? noteMutedOne(n.renote, cfg) : false);
}

// ---- 通知の見出し / 音 ----

function notifActionLabel(type: string): string {
  const labels: Record<string, string> = {
    follow: "にフォローされました",
    mention: "からメンション",
    reply: "から返信",
    renote: "がRenote",
    quote: "が引用",
    reaction: "がリアクション",
    pollEnded: "投票が終了",
    receiveFollowRequest: "からフォローリクエスト",
    followRequestAccepted: "がフォローを承認",
  };
  return labels[type] ?? type;
}

/// 通知音のプリセット一覧（設定UIの選択肢用）。"" は未選択＝継承を表す特別値なのでここには含めない。
export const NOTIFY_SOUND_PRESETS: { id: string; label: string }[] = [
  { id: "beep", label: "ビープ" },
  { id: "chime", label: "チャイム" },
  { id: "ping", label: "ピン" },
  { id: "pop", label: "ポップ" },
];

let audioCtx: AudioContext | null = null;
function playTone(freq: number, delay: number, dur: number, type: OscillatorType = "sine", peak = 0.15) {
  audioCtx ??= new AudioContext();
  const ctx = audioCtx;
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = type;
  osc.frequency.value = freq;
  gain.gain.value = 0.0001;
  osc.connect(gain).connect(ctx.destination);
  const now = ctx.currentTime + delay;
  gain.gain.exponentialRampToValueAtTime(peak, now + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + dur);
  osc.start(now);
  osc.stop(now + dur + 0.02);
}

function playPreset(preset: string) {
  switch (preset) {
    case "chime":
      playTone(660, 0, 0.12);
      playTone(880, 0.1, 0.16);
      break;
    case "ping":
      playTone(1300, 0, 0.09, "sine", 0.12);
      break;
    case "pop":
      playTone(220, 0, 0.09, "triangle", 0.2);
      break;
    case "beep":
    default:
      playTone(880, 0, 0.18);
      break;
  }
}

/// 通知音を鳴らす。choice は プリセットID / data URL(カスタム音声) / 空文字(既定=beep)。
export function playNotifySound(choice: string) {
  try {
    if (choice.startsWith("data:")) {
      void new Audio(choice).play().catch(() => {});
      return;
    }
    playPreset(choice || "beep");
  } catch {
    // 音の失敗は無視
  }
}

export const app = new AppStore();

// dev の HMR で本モジュールが差し替わる際、古いインスタンスの Tauri イベント
// 購読を解除する（本番では import.meta.hot が無く、このブロックは除去される）。
if (import.meta.hot) {
  import.meta.hot.dispose(() => app.teardown());
}

export function kindLabel(kind: ColumnKind): string {
  switch (kind.type) {
    case "home":
      return "Home";
    case "local":
      return "Local";
    case "global":
      return "Global";
    case "hybrid":
      return "Hybrid";
    case "notifications":
      return "通知";
    case "list":
      return "List";
    case "search":
      return "検索";
    case "antenna":
      return "アンテナ";
    case "channel":
      return "チャンネル";
    case "user":
      return "User";
    case "tag":
      return `#${kind.tag}`;
    case "tql":
      return "TQL";
  }
}
