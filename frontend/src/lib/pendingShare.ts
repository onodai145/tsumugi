// Android の共有シートから受け取ったテキスト/添付を取り込む(Issue #116)。
// 実行中の共有はイベントプッシュではなく、起動時と可視化のたびのポーリングで拾う
// (singleTask のため、共有経由でタスクが前面に戻る=可視化イベントが必ず起きる)。
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

export function setupPendingShareListener(): () => void {
  void pollPendingShare();
  const onVisibilityChange = () => {
    if (document.visibilityState === "visible") void pollPendingShare();
  };
  document.addEventListener("visibilitychange", onVisibilityChange);
  return () => document.removeEventListener("visibilitychange", onVisibilityChange);
}
