// Issue #458: セッション2。再起動後、NGを解除して保存 → 境界が空になり、上スクロールで、
// 以前キャッシュ済みだった範囲(境界が残っていればキャッシュから返っていた範囲)のNGユーザーのノートが出てくる。
import { loadState } from "../helpers/sessionHooks";
import { queryCache, saveNgUsers, columnsInfo, scrollColumnAt, waitForNote } from "../helpers/appInspect";

const boundaryRows = () => queryCache(`SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary`);

describe("#453 boundary-effective un-mute: session 2 (after restart)", () => {
  it("clears the boundary on un-mute and shows the formerly skipped notes", async function () {
    this.timeout(180000);
    const { MARK, minCached } = loadState().e;
    await waitForNote(MARK);
    await browser.pause(2000);
    let cols = await columnsInfo();
    const startIdx = cols[0].idx;
    const startMin = Math.min(...startIdx);
    console.log(`[e2e] e2 start: displayed=${startIdx.length}, min idx=${startMin}, NG-user notes displayed=${startIdx.filter((n) => n % 2 === 0).length}, boundary rows=${boundaryRows().length}, formerly cached down to idx ${minCached}`);
    expect(boundaryRows().length).toBe(1); // 保存前は、境界が残っている
    await saveNgUsers(" "); // NG解除(UI)
    expect(boundaryRows().length).toBe(0);
    for (let i = 0; i < 3; i++) {
      await scrollColumnAt(0);
      await browser.pause(3000);
    }
    cols = await columnsInfo();
    const evens = cols[0].idx.filter((n) => n % 2 === 0);
    const inFormerRange = evens.filter((n) => n >= minCached && n < startMin);
    console.log(`[e2e] e2 after un-mute+scroll: displayed=${cols[0].idx.length}; NG-user notes=${evens.length}; of those inside the formerly cached range [${minCached}, ${startMin}) = ${inFormerRange.length}`);
    expect(inFormerRange.length).toBeGreaterThan(0);
  });
});
