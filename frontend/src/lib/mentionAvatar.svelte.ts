import { app } from "./store.svelte";

// メンション本文中のアバターアイコン表示用: acctごとにセッション内でアバターURLをキャッシュする。
// 同一ユーザーへの複数メンション（同一ノート内・別ノート間）で resolve_user_acct を重複して
// 叩かないようにするため。値の意味:
//   - キャッシュ未登録（Map.get が undefined）: 未取得
//   - null: 解決失敗（リモート到達不可等）またはアバター未設定。以後リトライしない
//   - string: 解決済みのアバターURL
const cache = new Map<string, string | null>();
// 同一acctへの同時フェッチを1回のリクエストに集約するための in-flight Promise。
const inflight = new Map<string, Promise<string | null>>();

function acctKey(username: string, host: string | null): string {
  return host ? `${username}@${host}` : username;
}

/// キャッシュ済みなら即値を返す（同期的にレンダリング判定するため）。未取得ならundefined。
export function cachedAvatarUrl(username: string, host: string | null): string | null | undefined {
  return cache.get(acctKey(username, host));
}

/// acctからアバターURLを解決する。`app.defaultAccountId()` を使い、mentionクリック時の
/// openProfile と同じフォールバック慣例に倣う（呼び出し元でaccountIdを配線しない）。
/// 失敗時・アカウント未設定時はnullを返し、以後の再フェッチを避けるためnullをキャッシュする。
export async function fetchAvatarUrl(username: string, host: string | null): Promise<string | null> {
  const key = acctKey(username, host);
  const cached = cache.get(key);
  if (cached !== undefined) return cached;

  const existing = inflight.get(key);
  if (existing) return existing;

  const promise = resolve(key).finally(() => inflight.delete(key));
  inflight.set(key, promise);
  const result = await promise;
  cache.set(key, result);
  return result;
}

async function resolve(acct: string): Promise<string | null> {
  const accountId = app.defaultAccountId();
  if (!accountId) return null;
  try {
    const user = await app.resolveUserSilently(accountId, acct);
    return user.avatarUrl ?? null;
  } catch {
    return null;
  }
}
