import { commands } from "./ipc";
import { app } from "./store.svelte";

// メンション本文中のアバターアイコン表示用: account+acctごとにセッション内でアバターURLを
// キャッシュする。同一ユーザーへの複数メンション（同一ノート内・別ノート間）で
// resolve_user_acct を重複して叩かないようにするため。値の意味:
//   - キャッシュ未登録（Map.get が undefined）: 未取得
//   - null: 解決失敗（NotFound）またはアバター未設定。以後リトライしない
//   - string: 解決済みのアバターURL
// レート制限・ネットワーク等の一時的な失敗はキャッシュしない（次回呼び出しで再試行される）。
const cache = new Map<string, string | null>();
// 同一acctへの同時フェッチを1回のリクエストに集約するための in-flight Promise。
const inflight = new Map<string, Promise<string | null>>();

function acctStr(username: string, host: string | null): string {
  return host ? `${username}@${host}` : username;
}

// 複数アカウント間でのキャッシュ衝突を避けるため accountId をキーに含める
// （ホスト無しメンションはローカルユーザー名のみのため、アカウントが異なれば別ユーザーになりうる）。
function cacheKey(accountId: string, acct: string): string {
  return `${accountId}:${acct}`;
}

/// キャッシュ済みなら即値を返す（同期的にレンダリング判定するため）。未取得ならundefined。
export function cachedAvatarUrl(username: string, host: string | null): string | null | undefined {
  const accountId = app.defaultAccountId();
  if (!accountId) return null;
  return cache.get(cacheKey(accountId, acctStr(username, host)));
}

/// acctからアバターURLを解決する。`app.defaultAccountId()` を使い、mentionクリック時の
/// openProfile と同じフォールバック慣例に倣う（呼び出し元でaccountIdを配線しない）。
/// 解決失敗時はnullを返す。NotFound（ユーザーが存在しない）のみ恒久的にnullをキャッシュし、
/// レート制限・ネットワーク等の一時的なエラーはキャッシュしない（再フェッチ可能なままにする）。
export async function fetchAvatarUrl(username: string, host: string | null): Promise<string | null> {
  const accountId = app.defaultAccountId();
  if (!accountId) return null;

  const acct = acctStr(username, host);
  const key = cacheKey(accountId, acct);
  const cached = cache.get(key);
  if (cached !== undefined) return cached;

  const existing = inflight.get(key);
  if (existing) return existing;

  const promise = resolve(accountId, acct)
    .then(({ url, permanent }) => {
      if (permanent) cache.set(key, url);
      return url;
    })
    .finally(() => inflight.delete(key));
  inflight.set(key, promise);
  return promise;
}

/// resolve_user_acct を呼び、結果を判定する。`app.resolveUserSilently` は経由しない
/// （バックグラウンドの自動取得のためエラーをBackstageログに残したくない）。
async function resolve(accountId: string, acct: string): Promise<{ url: string | null; permanent: boolean }> {
  try {
    const r = await commands.resolveUserAcct(accountId, acct);
    if (r.status === "ok") return { url: r.data.avatarUrl ?? null, permanent: true };
    return { url: null, permanent: r.error.kind === "notFound" };
  } catch {
    // IPC層自体の失敗（開発時のコマンド未登録等）も一時的エラーとして扱い、キャッシュしない
    return { url: null, permanent: false };
  }
}
