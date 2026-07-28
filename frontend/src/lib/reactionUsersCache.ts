// リアクション/Renoteの「誰が」一覧の取得結果キャッシュ。note+key単位、モジュールスコープなので
// ReactionUsersPopover の再マウントを跨いで保持される。
// リアクションを付与/解除した際は store.svelte.ts から invalidate() を呼び、古い一覧が
// 表示され続けないようにする（詳細は invalidate 側のコメント参照）。
import type { User } from "../bindings/tauri.gen";
import { commands, unwrap } from "./ipc";

const cache = new Map<string, Promise<User[]>>();

function cacheKey(accountId: string, noteId: string, reactionKey: string | null): string {
  return `${accountId}:${noteId}:${reactionKey ?? "\0renote"}`;
}

export function fetchReactionUsers(
  accountId: string,
  noteId: string,
  reactionKey: string | null,
): Promise<User[]> {
  const key = cacheKey(accountId, noteId, reactionKey);
  let p = cache.get(key);
  if (!p) {
    p =
      reactionKey !== null
        ? unwrap(commands.getNoteReactions(accountId, noteId, reactionKey)).then((rs) => rs.map((r) => r.user))
        : unwrap(commands.getNoteRenotes(accountId, noteId));
    p.catch(() => cache.delete(key));
    cache.set(key, p);
  }
  return p;
}

/// 自分のリアクション付与/解除が成功・失敗どちらの場合でも、次回ホバー時に実際のサーバー状態を
/// 再取得させるため、対象キーのキャッシュを破棄する。
export function invalidateReactionUsers(accountId: string, noteId: string, reactionKey: string): void {
  cache.delete(cacheKey(accountId, noteId, reactionKey));
}
