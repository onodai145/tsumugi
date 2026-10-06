// Issue #458: #455 セッション2。サーバーでミュートを**追加**しただけ(+acc2のスナップショットを消して「起動時の初回同期」を再現)
// で再起動しても、境界が消えないことを確認する。
import { loadState } from "../helpers/sessionHooks";
import { queryCache, readSettingsJson, until, columnsInfo } from "../helpers/appInspect";

const boundaryRows = () => queryCache(`SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary ORDER BY column_id`);
type Snaps = Record<string, { users: string[]; words: string[] }>;

describe("#455 session 2: restart after a server mute was only ADDED", () => {
  it("keeps every boundary, and refreshes the snapshots", async function () {
    this.timeout(180000);
    const { adminId, acc2Id, xId, mId } = loadState().c;
    const startupBoundaries = boundaryRows();
    console.log(`[e2e] boundaries at session start: ${JSON.stringify(startupBoundaries)}`);
    const snaps = await until(() => (readSettingsJson().server_mute_snapshots ?? {}) as Snaps,
      (s) => !!s[adminId]?.users.includes(xId) && !!s[acc2Id], 40000, "startup sync (admin snapshot has X, acc2 snapshot re-saved)");
    await browser.pause(4000); // 起動時のresumeなどが落ち着くのを待つ
    const after = boundaryRows();
    console.log(`[e2e] snapshots after startup sync: ${JSON.stringify(snaps)}`);
    console.log(`[e2e] boundaries after startup sync: ${JSON.stringify(after)}`);
    const cols = await columnsInfo();
    console.log(`[e2e] columns on screen: ${cols.length} (notes per column: ${cols.map((c) => c.idx.length).join(",")})`);
    expect(snaps[adminId].users).toContain(mId); // 既存のミュートは残っている
    expect(snaps[adminId].users).toContain(xId); // 追加分が反映された
    expect(snaps[acc2Id].users).toContain(mId); // 消したスナップショットが、同期で復元された(初回=保存だけ)
    expect(after.length).toBe(2);
    expect(after.map((r) => r[0]).sort()).toEqual(loadState().c.columnIds.slice().sort());
  });
});
