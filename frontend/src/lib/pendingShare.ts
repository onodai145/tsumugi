// Android の共有シートから受け取ったテキスト/添付を取り込む(Issue #116)。
// 実行中の共有はイベントプッシュではなく、起動時と可視化のたびのポーリングで拾う
// (singleTask のため、共有経由でタスクが前面に戻る=可視化イベントが必ず起きる)。
// visibilitychange が確実に発火する保証がまだ実機未検証のため、window の focus も
// 独立した第二のトリガーとして併用する(どちらもidempotentで安価なポーリング)。
import { commands } from "./ipc";
import { app } from "./store.svelte";

export async function pollPendingShare(): Promise<void> {
  const share = await commands.getPendingShare();
  if (!share) return;
  if (!share.text && share.filePaths.length === 0) return;
  app.openCompose(app.defaultAccountId(), {
    text: share.text ?? undefined,
    filePaths: share.filePaths,
  });
}

function pollPendingShareSafely(): void {
  void pollPendingShare().catch(() => {});
}

export function setupPendingShareListener(): () => void {
  pollPendingShareSafely();
  const onVisibilityChange = () => {
    if (document.visibilityState === "visible") pollPendingShareSafely();
  };
  const onFocus = () => pollPendingShareSafely();
  document.addEventListener("visibilitychange", onVisibilityChange);
  window.addEventListener("focus", onFocus);
  return () => {
    document.removeEventListener("visibilitychange", onVisibilityChange);
    window.removeEventListener("focus", onFocus);
  };
}
