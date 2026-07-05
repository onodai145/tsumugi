// アプリの ViewModel（Svelte 5 runes）。カラム構成・受信ノート・接続状態を保持し、
// Rust からの columnNote / columnConnectionState イベントを購読して更新する。
import { commands, events, unwrap } from "./ipc";
import type { Account, Note, ConnectionState } from "../bindings/tauri.gen";
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

class AppStore {
  accounts = $state<Account[]>([]);
  columns = $state<ColumnView[]>([]);
  booting = $state(true);
  error = $state<string | null>(null);

  #unlisten: UnlistenFn[] = [];

  async boot() {
    this.booting = true;
    try {
      this.accounts = await unwrap(commands.listAccounts());
      await this.#subscribe();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.booting = false;
    }
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
  }

  async addAccount(host: string): Promise<string> {
    // 認可URLを取得し、既定ブラウザで開く。認可後に completeAccount を呼ぶ。
    const session = await unwrap(commands.startMiauth(host));
    const { openUrl } = await import("@tauri-apps/plugin-opener");
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
    const acc = this.accounts.find((a) => a.id === accountId);
    const opened = await unwrap(commands.openHomeColumn(accountId));
    this.columns = [
      ...this.columns,
      {
        id: opened.columnId,
        accountId,
        title: acc ? `Home @${acc.username}` : "Home",
        notes: opened.notes,
        state: "connecting",
        loadingMore: false,
      },
    ];
  }

  async loadMore(columnId: string) {
    const col = this.columns.find((c) => c.id === columnId);
    if (!col || col.loadingMore || col.notes.length === 0) return;
    col.loadingMore = true;
    try {
      const oldest = col.notes[col.notes.length - 1].id;
      const older = await unwrap(commands.fetchBackfill(col.accountId, oldest));
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
}

export const app = new AppStore();
