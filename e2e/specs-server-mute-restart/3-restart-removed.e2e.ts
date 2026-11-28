// Issue #458: #455 セッション3。サーバーでMのミュートを**解除**して再起動すると、そのアカウントのカラムの境界だけが破棄され、
// 解除したユーザーのノートが上スクロールで出てくることを確認する。破棄された境界は、起動時のカラム復元で最新の1ページから
// 作り直される(Issue #555)ので、行が無くなるのではなく、前より新しい値になる。
import { loadState } from "../helpers/sessionHooks";
import { queryCache, readSettingsJson, until, columnsInfo, scrollColumnAt } from "../helpers/appInspect";

const boundaryRows = () => queryCache(`SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary ORDER BY column_id`);
type Snaps = Record<string, { users: string[]; words: string[] }>;

describe("#455 session 3: restart after a muted user was UNMUTED on the server", () => {
  it("clears only that account's boundaries and the user's older notes appear on scroll", async function () {
    this.timeout(240000);
    const { adminId, acc2Id, mId, columnIds } = loadState().c;
    console.log(`[e2e] boundaries at session start: ${JSON.stringify(boundaryRows())}`);
    const snaps = await until(() => (readSettingsJson().server_mute_snapshots ?? {}) as Snaps,
      (s) => !!s[adminId] && !s[adminId].users.includes(mId), 40000, "startup sync (admin snapshot no longer has M)");
    await browser.pause(5000);
    const rows = boundaryRows();
    console.log(`[e2e] snapshots after startup sync: ${JSON.stringify(snaps)}`);
    console.log(`[e2e] boundaries after startup sync: ${JSON.stringify(rows)}`);
    // どちらのカラムがどのアカウントか: 設定のcolumnsから引く
    const settings = readSettingsJson() as { columns: { id: string; account_id?: string; accountId?: string }[] };
    const accountOf = (cid: string) => settings.columns.find((c) => c.id === cid)?.account_id ?? settings.columns.find((c) => c.id === cid)?.accountId;
    const adminCol = columnIds.find((c: string) => accountOf(c) === adminId);
    const acc2Col = columnIds.find((c: string) => accountOf(c) === acc2Id);
    console.log(`[e2e] adminCol=${adminCol} acc2Col=${acc2Col}`);
    const adminRows = rows.filter((r) => r[0] === adminCol);
    const acc2Rows = rows.filter((r) => r[0] === acc2Col);
    console.log(`[e2e] admin column boundary rows=${adminRows.length}, acc2 column boundary rows=${acc2Rows.length}`);
    expect(acc2Rows.length).toBeGreaterThan(0); // 他のアカウントのカラムの境界は残る
    // 解除したアカウントのカラムの境界は破棄される。破棄されずに残れば前と同じ値なので、取り直した値は前より新しい。
    // 末尾まで取得し切った境界は空文字になる。その場合も、取り直した値(ID)は空文字より大きい。
    const before = loadState().boundaries as Record<string, string>;
    for (const r of adminRows) expect(String(r[2]) > before[adminCol]).toBe(true);
    expect(acc2Rows.map((r) => r[2])).toEqual([before[acc2Col]]); // 他のアカウントの境界は変わらない

    // 解除したM(偶数番号)の、より古いノートが上スクロールで出てくる(adminのカラムは左端)
    let cols = await columnsInfo();
    console.log(`[e2e] columns: ${cols.length}, per-column oldest idx=${cols.map((c) => Math.min(...c.idx)).join(",")}`);
    const adminIdx = 0;
    const oldestBefore = Math.min(...cols[adminIdx].idx);
    const evenBefore = cols[adminIdx].idx.filter((n) => n % 2 === 0).length;
    for (let i = 0; i < 3; i++) {
      await scrollColumnAt(adminIdx);
      await browser.pause(3000);
    }
    cols = await columnsInfo();
    const evenOlder = cols[adminIdx].idx.filter((n) => n % 2 === 0 && n < oldestBefore);
    console.log(`[e2e] admin column: M(even) notes before=${evenBefore}, after scroll: older-than-${oldestBefore} M notes=${evenOlder.length}; acc2 column even notes=${cols[1]?.idx.filter((n) => n % 2 === 0).length}`);
    expect(evenOlder.length).toBeGreaterThan(0);
    // acc2はまだMをミュートしている: Mのノートは出ない
    expect(cols[1].idx.filter((n) => n % 2 === 0).length).toBe(0);
  });
});
