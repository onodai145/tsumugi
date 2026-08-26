import { commands } from "./ipc";
import { app } from "./store.svelte";
import type { UrlPreview } from "../bindings/tauri.gen";

// リンクプレビュー(OGP相当)のセッション内キャッシュ(Issue #9)。lib/mentionAvatar.tsと同構造だが、
// プレビュー内容自体はアカウントに依存しない公開情報のため、キーはURLのみ(accountIdを含めない)。
//   - キャッシュ未登録(Map.getがundefined): 未取得
//   - null: 確定的にプレビュー無し(OGPフィールドが全て空の応答、または解決不能なアカウント)。
//     以後リトライしない
//   - UrlPreview: 取得済み
// ネットワークエラー・タイムアウト・IPC層自体の失敗等の一時的な失敗はキャッシュしない
// (次回呼び出しで再試行される)。
const cache = new Map<string, UrlPreview | null>();
// 同一URLへの同時フェッチを1回のリクエストに集約するための in-flight Promise。
const inflight = new Map<string, Promise<UrlPreview | null>>();

/// キャッシュ済みなら即値を返す（同期的にレンダリング判定するため）。未取得ならundefined。
export function cachedUrlPreview(url: string): UrlPreview | null | undefined {
  return cache.get(url);
}

/// URLのリンクプレビューを取得する。`app.defaultAccountId()`のアカウントを使う
/// (lib/mentionAvatar.tsと同じ慣例)。
export async function fetchUrlPreview(url: string): Promise<UrlPreview | null> {
  const cached = cache.get(url);
  if (cached !== undefined) return cached;

  const existing = inflight.get(url);
  if (existing) return existing;

  const promise = resolve(url)
    .then(({ data, permanent }) => {
      if (permanent) cache.set(url, data);
      return data;
    })
    .finally(() => inflight.delete(url));
  inflight.set(url, promise);
  return promise;
}

/// UrlPreviewCard.svelte がリンクとして扱う(http/https)スキームか判定する。
/// カード側の描画判定(iframe埋め込み・リンク化)と合わせて、危険なスキームは無視する。
export function isSafeUrl(url: string): boolean {
  return /^https?:\/\//i.test(url);
}

/// UrlPreviewCard.svelte が実際に描画に使うフィールドが1つでもあれば「内容あり」とみなす。
/// icon はカードが描画しないため対象外。player は isSafeUrl を満たす場合のみ
/// (安全でないスキームは再生ボタン・iframeとも描画されない)。
function hasContent(p: UrlPreview): boolean {
  return !!(p.title || p.description || p.thumbnail || p.sitename || (p.player && isSafeUrl(p.player.url)));
}

async function resolve(url: string): Promise<{ data: UrlPreview | null; permanent: boolean }> {
  const accountId = app.defaultAccountId();
  if (!accountId) return { data: null, permanent: false };
  try {
    const r = await commands.fetchUrlPreview(accountId, url);
    if (r.status === "ok") return { data: hasContent(r.data) ? r.data : null, permanent: true };
    return { data: null, permanent: false };
  } catch {
    // IPC層自体の失敗(開発時のコマンド未登録等)も一時的エラーとして扱い、キャッシュしない
    return { data: null, permanent: false };
  }
}
