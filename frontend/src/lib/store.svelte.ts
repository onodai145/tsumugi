// アプリの ViewModel（Svelte 5 runes）。視覚カラム(GroupView)=タブ(TabView)の集合を保持し、
// Rust からの columnNote / columnNotification / columnConnectionState を購読して更新する。
import { commands, events, unwrap, formatError } from "./ipc";
import { openUrl } from "@tauri-apps/plugin-opener";
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
} from "../bindings/tauri.gen";
import type { UnlistenFn } from "@tauri-apps/api/event";

const MAX_NOTES = 300; // タブあたり DOM に保持する上限（仮想化-lite）

/// タブ = 1タイムライン。
export interface TabView {
  id: string;
  accountId: string;
  kind: ColumnKind;
  title: string;
  notes: Note[];
  notifications: Notification[];
  state: ConnectionState;
  loadingMore: boolean;
}

/// 視覚カラム = タブの集合。幅と並び順を持ち、アクティブタブを表示する。
export interface GroupView {
  id: string;
  width: number;
  tabs: TabView[];
  activeTabId: string;
}

/// 投稿フォーム（返信/引用の文脈つき）
export interface ComposeState {
  accountId: string;
  replyTo?: Note;
  quoteOf?: Note;
}

class AppStore {
  accounts = $state<Account[]>([]);
  groups = $state<GroupView[]>([]);
  booting = $state(true);
  error = $state<string | null>(null);
  compose = $state<ComposeState | null>(null);
  emojis = $state<Record<string, EmojiDef[]>>({});
  mute = $state<MuteConfig>({ ngWords: [], ngUsers: [], ngInstances: [] });
  notify = $state<NotifyConfig>({ desktop: false, sound: false });

  #unlisten: UnlistenFn[] = [];

  async boot() {
    this.booting = true;
    try {
      this.accounts = await unwrap(commands.listAccounts());
      this.mute = await unwrap(commands.getMute());
      this.notify = await unwrap(commands.getNotify());
      await this.#subscribe();
      const groupDefs = await unwrap(commands.listGroups());
      this.groups = groupDefs.map((g) => ({ id: g.id, width: g.width, tabs: [], activeTabId: "" }));
      const tabDefs = await unwrap(commands.listColumns());
      for (const tab of tabDefs) {
        try {
          const opened = await unwrap(commands.resumeColumn(tab.id));
          this.#insertTab(opened);
          this.#captureInitial(opened.column.id, opened.notes);
        } catch (e) {
          this.error = String(e);
        }
      }
    } catch (e) {
      this.error = String(e);
    } finally {
      this.booting = false;
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
      notes: opened.notes,
      notifications: opened.notifications,
      state: "connecting",
      loadingMore: false,
    };
  }

  /// OpenedColumn を該当グループに差し込む（無ければグループを作る）。
  #insertTab(opened: OpenedColumn) {
    let g = this.groups.find((x) => x.id === opened.group.id);
    if (!g) {
      g = { id: opened.group.id, width: opened.group.width, tabs: [], activeTabId: "" };
      this.groups = [...this.groups, g];
    }
    const tab = this.#makeTab(opened);
    g.tabs = [...g.tabs, tab];
    if (!g.activeTabId) g.activeTabId = tab.id;
    return tab;
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
      this.error = String(e);
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
      this.error = String(e);
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
      this.error = String(e);
    }
  }

  async #subscribe() {
    for (const u of this.#unlisten) u();
    this.#unlisten = [];

    this.#unlisten.push(
      await events.columnNote.listen((e) => {
        const tab = this.#findTab(e.payload.columnId);
        if (!tab) return;
        if (tab.notes.some((n) => n.id === e.payload.note.id)) return;
        tab.notes = [e.payload.note, ...tab.notes].slice(0, MAX_NOTES);
      }),
    );
    this.#unlisten.push(
      await events.columnConnectionState.listen((e) => {
        const tab = this.#findTab(e.payload.columnId);
        if (tab) tab.state = e.payload.state;
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
        // デスクトップ通知 / 音
        if (this.notify.desktop) void this.#osNotify(e.payload.notification);
        if (this.notify.sound) beep();
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
  }

  async removeAccount(accountId: string) {
    await unwrap(commands.removeAccount(accountId));
    this.accounts = this.accounts.filter((a) => a.id !== accountId);
    for (const t of this.#allTabs().filter((t) => t.accountId === accountId)) {
      await this.closeTab(t.id);
    }
  }

  /// タブを追加する。`groupId` を指定するとそのカラムに、None なら新しいカラムを作る。
  async addColumn(accountId: string, kind: ColumnKind, filter: FilterQuery, groupId?: string) {
    const opened = await unwrap(commands.addColumn(accountId, kind, filter, groupId ?? null));
    const tab = this.#insertTab(opened);
    const g = this.groups.find((x) => x.id === opened.group.id);
    if (g) g.activeTabId = tab.id;
    this.#captureInitial(opened.column.id, opened.notes);
  }

  async validateFilter(filter: FilterQuery): Promise<string | null> {
    const r = await commands.validateFilter(filter);
    return r.status === "ok" ? null : formatError(r.error);
  }

  async fetchUserLists(accountId: string) {
    return await unwrap(commands.listUserLists(accountId));
  }

  /// 通知設定を保存。desktop を有効化したら権限を要求する。
  async setNotify(config: NotifyConfig) {
    if (config.desktop && !(await isPermissionGranted())) {
      const p = await requestPermission();
      if (p !== "granted") config = { ...config, desktop: false };
    }
    await unwrap(commands.setNotify(config));
    this.notify = config;
  }

  async #osNotify(n: Notification) {
    try {
      if (!(await isPermissionGranted())) return;
      const actor = n.user ? (n.user.name ?? n.user.username) : "";
      const title = `${actor} ${notifActionLabel(n.type)}`.trim();
      const body = n.note?.text ?? (n.reaction ?? "");
      sendNotification({ title: title || "通知", body });
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
        tab.notes = [...tab.notes, ...older.filter((n) => !known.has(n.id))].slice(0, MAX_NOTES);
      }
    } catch (e) {
      this.error = String(e);
    } finally {
      tab.loadingMore = false;
    }
  }

  /// タブを閉じる。グループが空になったらグループも消す。
  async closeTab(tabId: string) {
    await unwrap(commands.closeColumn(tabId));
    for (const g of this.groups) {
      if (!g.tabs.some((t) => t.id === tabId)) continue;
      g.tabs = g.tabs.filter((t) => t.id !== tabId);
      if (g.activeTabId === tabId) g.activeTabId = g.tabs[0]?.id ?? "";
    }
    this.groups = this.groups.filter((g) => g.tabs.length > 0);
  }

  // ---- Phase 3: 投稿・リアクション ----

  openCompose(accountId: string, opts: { replyTo?: Note; quoteOf?: Note } = {}) {
    this.compose = { accountId, ...opts };
  }
  closeCompose() {
    this.compose = null;
  }

  async postNote(accountId: string, draft: NoteDraft) {
    await unwrap(commands.postNote(accountId, draft));
    this.compose = null;
  }

  async renote(accountId: string, noteId: string, visibility: VisibilityInput = "public") {
    await unwrap(commands.renote(accountId, noteId, visibility));
  }

  async deleteNote(accountId: string, noteId: string) {
    await unwrap(commands.deleteNoteCmd(accountId, noteId));
    for (const t of this.#allTabs()) t.notes = t.notes.filter((n) => n.id !== noteId);
  }

  async loadEmojis(accountId: string): Promise<EmojiDef[]> {
    if (this.emojis[accountId]) return this.emojis[accountId];
    const list = await unwrap(commands.listCustomEmojis(accountId));
    this.emojis = { ...this.emojis, [accountId]: list };
    return list;
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
        await unwrap(commands.unreact(accountId, noteId));
      } else {
        if (already) await unwrap(commands.unreact(accountId, noteId));
        await unwrap(commands.react(accountId, noteId, reaction));
      }
    } catch (e) {
      backups.forEach(restoreReaction);
      this.error = String(e);
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
    pollEnded: "の投票が終了",
    receiveFollowRequest: "からフォローリクエスト",
    followRequestAccepted: "がフォローを承認",
  };
  return labels[type] ?? type;
}

let audioCtx: AudioContext | null = null;
function beep() {
  try {
    audioCtx ??= new AudioContext();
    const ctx = audioCtx;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = "sine";
    osc.frequency.value = 880;
    gain.gain.value = 0.0001;
    osc.connect(gain).connect(ctx.destination);
    const now = ctx.currentTime;
    gain.gain.exponentialRampToValueAtTime(0.15, now + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.18);
    osc.start(now);
    osc.stop(now + 0.19);
  } catch {
    // 音の失敗は無視
  }
}

export const app = new AppStore();

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
  }
}
