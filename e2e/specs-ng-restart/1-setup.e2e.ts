// Issue #458: #453 の「NG解除後に上スクロールで出てくる」を、境界が実際に効く形で確認する セッション1。
// NG(ユーザー)を設定してからカラムを開く(境界はNG下で作られる)→ 1回遡る(境界が延びる)→ 終了。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { signInAsSeededUser, createNote, signUp, allowRegistration, followUser } from "../helpers/misskeyApi";
import { saveState, loadState } from "../helpers/sessionHooks";
import { addAccountViaUi, addTqlColumn, waitForNote, setWindow, saveNgUsers, columnsInfo, scrollColumnAt, queryCache } from "../helpers/appInspect";

const RUN = Date.now();
const MARK = `v458ngb${RUN}`;
const NG_USER = `e2engb${String(RUN).slice(-8)}`;
const OK_USER = `e2eokb${String(RUN).slice(-8)}`;

describe("#453 boundary-effective un-mute: session 1", () => {
  let bridge: MiauthBridge;
  before(async function () {
    this.timeout(300000);
    const admin = await signInAsSeededUser();
    await allowRegistration(admin);
    const ng = await signUp(NG_USER, "e2eTestPassword10!");
    const ok = await signUp(OK_USER, "e2eTestPassword11!");
    await followUser(admin, NG_USER);
    await followUser(admin, OK_USER);
    for (let i = 0; i < 80; i++) await createNote(i % 2 === 0 ? ng.token : ok.token, `v458 n${i} ${MARK}`);
    saveState({ e: { MARK, NG_USER } });
    await new Promise((r) => setTimeout(r, 10000));
    bridge = await startMiauthBridge();
  });
  after(async () => { await bridge?.teardown(); });

  it("sets NG first, opens the column (boundary created under NG), loads more once", async function () {
    this.timeout(180000);
    await addAccountViaUi(bridge, "e2etestadmin");
    await setWindow();
    await saveNgUsers(`@${NG_USER}`);
    await addTqlColumn(`from home where text -> "${MARK}"`);
    await waitForNote(MARK);
    const rowsOpen = queryCache(`SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary`);
    expect(rowsOpen.length).toBe(1);
    await columnsInfo(); // カラムのスクロール要素に印を付ける
    await scrollColumnAt(0);
    await browser.pause(3500);
    const rows = queryCache(`SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary`);
    const cached = queryCache(`SELECT n.text FROM column_note cn JOIN note n ON n.id=cn.note_id`).map((r) => /v458 n(\d+) /.exec(r[0])).filter((m): m is RegExpExecArray => !!m).map((m) => Number(m[1]));
    const minCached = Math.min(...cached);
    console.log(`[e2e] e1: boundary at open=${JSON.stringify(rowsOpen)} after loadMore=${JSON.stringify(rows)}; cached notes=${cached.length}, min cached idx=${minCached}, NG-user notes cached=${cached.filter((n) => n % 2 === 0).length}`);
    expect(cached.filter((n) => n % 2 === 0).length).toBe(0);
    saveState({ e: { ...loadState().e, minCached, boundaryBefore: rows } });
  });
});
