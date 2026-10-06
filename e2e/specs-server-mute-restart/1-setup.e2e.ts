// Issue #458: #455 セッション1。2アカウントを追加し、初回の同期では境界が消えず、スナップショットが保存されることを確認する。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { signInAsSeededUser, createNote, signUp, allowRegistration, followUser, getUserId, muteUser } from "../helpers/misskeyApi";
import { loadState, saveState } from "../helpers/sessionHooks";
import {
  addAccountViaUi, addTqlColumn, noteTexts, waitForNote, queryCache, readSettingsJson, setWindow, scrollActiveToBottom,
  listAccounts, openSettingsAccountsAndAdd, closeModal, until,
} from "../helpers/appInspect";

const RUN = Date.now();
const SUF = String(RUN).slice(-8);
const CM = `v458srv${RUN}`;
const names = { m: `e2em${SUF}`, q: `e2eq${SUF}`, x: `e2ex${SUF}`, acc2: `e2eb${SUF}` };
const PASS = "e2eTestPassword8!";
const tql = `from home where text -> "${CM}"`;
const boundaryRows = () => queryCache(`SELECT column_id, source_idx, oldest_fetched_id FROM column_source_boundary ORDER BY column_id`);

describe("#455 session 1: first sync only saves a snapshot", () => {
  let bridge: MiauthBridge | undefined;
  let admin: string;

  before(async function () {
    this.timeout(300000);
    admin = await signInAsSeededUser();
    await allowRegistration(admin);
    const m = await signUp(names.m, PASS);
    const q = await signUp(names.q, PASS);
    await signUp(names.x, PASS);
    const acc2 = await signUp(names.acc2, PASS);
    const mId = await getUserId(admin, names.m);
    const xId = await getUserId(admin, names.x);
    for (const t of [admin, acc2.token]) {
      await followUser(t, names.m);
      await followUser(t, names.q);
    }
    // 取得するサーバー側ミュート: adminもacc2も M をミュートしている状態でアプリに追加する
    await muteUser(admin, mId);
    await muteUser(acc2.token, mId);
    for (let i = 0; i < 60; i++) await createNote(i % 2 === 0 ? m.token : q.token, `v458 n${i} ${CM}`);
    saveState({ c: { adminToken: admin, acc2Token: acc2.token, mId, xId, CM, names, PASS } });
    await new Promise((r) => setTimeout(r, 10000));
    bridge = await startMiauthBridge();
  });
  after(async () => { await bridge?.teardown(); });

  it("adds the first account and its column; boundary rows exist", async function () {
    this.timeout(180000);
    await addAccountViaUi(bridge!, "e2etestadmin");
    await setWindow();
    await addTqlColumn(tql);
    await waitForNote(CM);
    await until(() => boundaryRows(), (r) => r.length >= 1, 20000, "admin's column boundary");
    console.log(`[e2e] after admin column: boundaries=${JSON.stringify(boundaryRows())}`);
    const texts = await noteTexts();
    // Mはサーバー側でミュート済み → Mの投稿(偶数)はサーバーから来ない
    const even = texts.filter((t) => /v458 n(\d+) /.test(t) && Number(/v458 n(\d+) /.exec(t)![1]) % 2 === 0);
    console.log(`[e2e] admin column: ${texts.length} notes, muted-user (M) notes=${even.length}`);
    expect(even.length).toBe(0);
  });

  it("adding the 2nd account (first sync for it) does not clear the 1st account's boundaries", async function () {
    this.timeout(180000);
    await bridge!.teardown();
    await new Promise((r) => setTimeout(r, 10000));
    bridge = await startMiauthBridge({ username: names.acc2, password: PASS });
    const before = boundaryRows();
    await openSettingsAccountsAndAdd();
    // addAccountViaUi と同じ手順(ホスト入力から)
    await addAccountViaUi(bridge, names.acc2);
    await closeModal();
    const accounts = await listAccounts();
    const adminAcc = accounts.find((a) => a.username === "e2etestadmin")!;
    const acc2Acc = accounts.find((a) => a.username === names.acc2)!;
    saveState({ c: { ...loadState().c, adminId: adminAcc.id, acc2Id: acc2Acc.id } });
    // 初回の同期(保存だけ)が済んだのを、スナップショットの保存で確認してから、境界を見る
    const snap = await until(() => (readSettingsJson().server_mute_snapshots ?? {}) as Record<string, { users: string[]; words: string[] }>,
      (s) => !!s[acc2Acc.id], 30000, "acc2 snapshot saved");
    const after = boundaryRows();
    console.log(`[e2e] boundaries before 2nd account: ${JSON.stringify(before)} / after first sync of acc2: ${JSON.stringify(after)}`);
    console.log(`[e2e] server_mute_snapshots=${JSON.stringify(snap)}`);
    expect(after).toEqual(before);
    expect(snap[adminAcc.id].users).toContain(loadState().c.mId);
    expect(snap[acc2Acc.id].users).toContain(loadState().c.mId);
    expect(Array.isArray(snap[adminAcc.id].words)).toBe(true);
  });

  it("adds a column for the 2nd account and loads more on both", async function () {
    this.timeout(180000);
    const { acc2Id } = loadState().c;
    await addTqlColumn(tql, acc2Id);
    await until(() => boundaryRows(), (r) => r.length >= 2, 20000, "boundary rows for both columns");
    const rows = boundaryRows();
    console.log(`[e2e] boundaries with 2 columns: ${JSON.stringify(rows)}`);
    saveState({ c: { ...loadState().c, columnIds: rows.map((r) => r[0]) } });
    expect(rows.length).toBe(2);
    expect(await scrollActiveToBottom()).toBe(true);
    await browser.pause(2500);
  });
});
