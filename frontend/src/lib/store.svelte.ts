// アプリの ViewModel（Svelte 5 runes）。カラム構成・受信ノート・接続状態を保持し、
// Rust からの columnNote / columnConnectionState イベントを購読して更新する。
import { commands, events, unwrap } from "./ipc";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  Account,
  Note,
  ConnectionState,
  EmojiDef,
  NoteDraft_Deserialize as NoteDraft,
  VisibilityInput,
  OpenedColumn,
  NoteUpdate,
} from "../bindings/tauri.gen";
import type { UnlistenFn } from "@tauri-apps/api/event";

const MAX_NOTES = 300; // カラムあたり DOM に保持する上限（仮想化-lite）

export interface ColumnView {
  id: string;
  accountId: string;
  title: string;
  notes: Note[];
  state: ConnectionState;
  loadingMore: boolean;
}

/// 投稿フォーム（返信/引用の文脈つき）
export interface ComposeState {
  accountId: string;
  replyTo?: Note;
  quoteOf?: Note;
}

class AppStore {
  accounts = $state<Account[]>([]);
  columns = $state<ColumnView[]>([]);
  booting = $state(true);
  error = $state<string | null>(null);
  compose = $state<ComposeState | null>(null);
  emojis = $state<Record<string, EmojiDef[]>>({}); // accountId -> 絵文字

  #unlisten: UnlistenFn[] = [];

  async boot() {
    this.booting = true;
    try {
      this.accounts = await unwrap(commands.listAccounts());
      await this.#subscribe();
      // 永続化済みカラムを復元（Streaming を張り直し初期ページを取得）
      const persisted = await unwrap(commands.listColumns());
      for (const col of persisted) {
        try {
          const opened = await unwrap(commands.resumeColumn(col.id));
          this.columns = [...this.columns, this.#toView(opened)];
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

  #toView(opened: OpenedColumn): ColumnView {
    const acc = this.accounts.find((a) => a.id === opened.column.accountId);
    return {
      id: opened.column.id,
      accountId: opened.column.accountId,
      title: acc ? `Home @${acc.username}` : "Home",
      notes: opened.notes,
      state: "connecting",
      loadingMore: false,
    };
  }

  async #subscribe() {
    // 既存購読を掃除
    for (const u of this.#unlisten) u();
    this.#unlisten = [];

    this.#unlisten.push(
      await events.columnNote.listen((e) => {
        const col = this.columns.find((c) => c.id === e.payload.columnId);
        if (!col) return;
        // 先頭に追加（重複はスキップ）
        if (col.notes.some((n) => n.id === e.payload.note.id)) return;
        col.notes = [e.payload.note, ...col.notes].slice(0, MAX_NOTES);
      }),
    );
    this.#unlisten.push(
      await events.columnConnectionState.listen((e) => {
        const col = this.columns.find((c) => c.id === e.payload.columnId);
        if (col) col.state = e.payload.state;
      }),
    );
    this.#unlisten.push(
      await events.columnNoteUpdated.listen((e) => this.#applyNoteUpdate(e.payload)),
    );
  }

  /// 他者のリアクション/投票/削除を該当ノートへ反映（自分の操作は楽観的更新済みなので無視）。
  #applyNoteUpdate(p: {
    columnId: string;
    noteId: string;
    update: NoteUpdate;
    actorId: string | null;
  }) {
    const col = this.columns.find((c) => c.id === p.columnId);
    if (!col) return;

    if (p.update.type === "deleted") {
      col.notes = col.notes.filter((n) => n.id !== p.noteId);
      return;
    }

    // 対象ノート（renote 先も）を集める
    const targets: Note[] = [];
    for (const n of col.notes) {
      if (n.id === p.noteId) targets.push(n);
      if (n.renote && n.renote.id === p.noteId) targets.push(n.renote);
    }
    if (targets.length === 0) return;

    const mine = this.#myUserIds();
    const isMine = p.actorId != null && mine.has(p.actorId);

    for (const n of targets) {
      switch (p.update.type) {
        case "reacted":
          if (isMine) break; // 楽観的更新済み
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
          if (n.poll && n.poll.choices[p.update.choice]) {
            n.poll.choices[p.update.choice].votes += 1;
          }
          break;
      }
    }
  }

  #myUserIds(): Set<string> {
    return new Set(this.accounts.map((a) => a.userId));
  }

  /// カラム表示中ノートをキャプチャ購読する（初期ページ分。Streaming 受信分は Rust が自動登録）。
  #captureInitial(columnId: string, notes: Note[]) {
    const ids = notes.map((n) => n.id);
    if (ids.length > 0) void commands.captureNotes(columnId, ids);
  }

  async addAccount(host: string): Promise<string> {
    // 認可URLを取得し、既定ブラウザで開く。認可後に completeAccount を呼ぶ。
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
    // 関連カラムも閉じる
    for (const c of this.columns.filter((c) => c.accountId === accountId)) {
      await this.closeColumn(c.id);
    }
  }

  async addHomeColumn(accountId: string) {
    const opened = await unwrap(commands.openHomeColumn(accountId));
    this.columns = [...this.columns, this.#toView(opened)];
    this.#captureInitial(opened.column.id, opened.notes);
  }

  async loadMore(columnId: string) {
    const col = this.columns.find((c) => c.id === columnId);
    if (!col || col.loadingMore || col.notes.length === 0) return;
    col.loadingMore = true;
    try {
      const oldest = col.notes[col.notes.length - 1].id;
      const older = await unwrap(commands.fetchBackfill(col.accountId, col.id, oldest));
      const known = new Set(col.notes.map((n) => n.id));
      const fresh = older.filter((n) => !known.has(n.id));
      col.notes = [...col.notes, ...fresh].slice(0, MAX_NOTES);
    } catch (e) {
      this.error = String(e);
    } finally {
      col.loadingMore = false;
    }
  }

  async closeColumn(columnId: string) {
    await unwrap(commands.closeColumn(columnId));
    this.columns = this.columns.filter((c) => c.id !== columnId);
  }

  // ---- Phase 3: 投稿・リアクション ----

  openCompose(accountId: string, opts: { replyTo?: Note; quoteOf?: Note } = {}) {
    this.compose = { accountId, ...opts };
  }
  closeCompose() {
    this.compose = null;
  }

  async postNote(accountId: string, draft: NoteDraft) {
    // 投稿結果は streaming(home) でも届くため、ここでは送信のみ（重複は購読側で排除）
    await unwrap(commands.postNote(accountId, draft));
    this.compose = null;
  }

  async renote(accountId: string, noteId: string, visibility: VisibilityInput = "public") {
    await unwrap(commands.renote(accountId, noteId, visibility));
  }

  async deleteNote(accountId: string, noteId: string) {
    await unwrap(commands.deleteNoteCmd(accountId, noteId));
    for (const col of this.columns) {
      col.notes = col.notes.filter((n) => n.id !== noteId);
    }
  }

  async loadEmojis(accountId: string): Promise<EmojiDef[]> {
    if (this.emojis[accountId]) return this.emojis[accountId];
    const list = await unwrap(commands.listCustomEmojis(accountId));
    this.emojis = { ...this.emojis, [accountId]: list };
    return list;
  }

  /// リアクションのトグル（楽観的更新 → 失敗時ロールバック）。
  async toggleReaction(accountId: string, noteId: string, reaction: string) {
    const targets = this.#collectNotes(noteId);
    if (targets.length === 0) return;
    const backups = targets.map((n) => snapshotReaction(n));
    const already = targets[0].myReaction;

    // 楽観的にローカル反映
    if (already === reaction) {
      targets.forEach(removeReaction);
    } else {
      targets.forEach((n) => addReaction(n, reaction));
    }

    try {
      if (already === reaction) {
        await unwrap(commands.unreact(accountId, noteId));
      } else {
        if (already) await unwrap(commands.unreact(accountId, noteId)); // 付け替えは一旦解除
        await unwrap(commands.react(accountId, noteId, reaction));
      }
    } catch (e) {
      // ロールバック
      backups.forEach(restoreReaction);
      this.error = String(e);
    }
  }

  /// 指定 noteId のノート実体を全カラムから集める（renote 先も対象）。
  #collectNotes(noteId: string): Note[] {
    const out: Note[] = [];
    for (const col of this.columns) {
      for (const n of col.notes) {
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

export const app = new AppStore();
