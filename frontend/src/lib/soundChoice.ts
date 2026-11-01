// sound_choice の "mml:<テキスト>" を、Rust が再生できる data: URL に解決する(Issue #75)。
// 解決 = Rust でパース → Tone.js でレンダリング。結果は choice をキーにメモ化し、
// 進行中の Promise ごと共有する(初回レンダリング中に通知が続いても二重に走らせない)。
import { commands } from "../bindings/tauri.gen";
import { renderScore } from "./mmlRender";

export const MML_PREFIX = "mml:";
// エフェクトの尾を含めて最大約 95.6 秒の MML の data: URL は約 5.6MB(尾が無い 90 秒なら約 5.3MB)。全部埋まっても約 45MB に収めるため 8 件に抑える。
const CACHE_LIMIT = 8;
const cache = new Map<string, Promise<string>>();

async function renderMml(text: string): Promise<string> {
  const r = await commands.parseMml(text);
  if (r.status !== "parsed") {
    throw new Error(`MML 構文エラー (${r.error.position + 1} 文字目): ${r.error.message}`);
  }
  return renderScore(r.score);
}

/// mml: 以外はそのまま返す。mml: は data: URL に解決する(失敗したら reject。キャッシュはしない)。
export function resolveSoundChoice(choice: string): Promise<string> {
  if (!choice.startsWith(MML_PREFIX)) return Promise.resolve(choice);
  const hit = cache.get(choice);
  if (hit) {
    cache.delete(choice); // 末尾へ移して、最近使ったものを残す
    cache.set(choice, hit);
    return hit;
  }
  const p = renderMml(choice.slice(MML_PREFIX.length));
  cache.set(choice, p);
  p.catch(() => {
    if (cache.get(choice) === p) cache.delete(choice);
  });
  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
  return p;
}

/// 初回の遅れを隠すための先読み。結果は捨て、失敗しても何もしない。
export function prewarmSoundChoice(choice: string): void {
  if (!choice.startsWith(MML_PREFIX)) return;
  resolveSoundChoice(choice).catch(() => {});
}

/// テスト用。
export function clearSoundChoiceCache(): void {
  cache.clear();
}
