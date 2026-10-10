// クライアント側予約(Issue #60 B)の E2E シナリオ 1: ローカル予約の投稿・取り消し・作成欄に戻す。
//
// E2E の Misskey(2026.7.0)はサーバー側予約に対応しているが、TSUMUGI_DEBUG_SERVER_VERSION=2025.9.0
// (デバッグビルド限定)でアプリに「非対応サーバー」と判定させ、ローカル予約の経路を通す。投稿は本物の
// notes/create で行われるので、投稿されたかは Misskey の API(users/notes)で確認する。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountViaUi, setWindow, until } from "../helpers/appInspect";
import { getUserId, listUserNotes, signInAsSeededUser } from "../helpers/misskeyApi";
import { listRows, nextMinuteAfter, openList, openPicker, rowContaining, rowId, scheduleAt } from "../helpers/scheduledUi";

const ADMIN = "e2etestadmin";
const RUN = Date.now();
const MARK = `e2esched${RUN}`;
const P1 = `${MARK} P1 posts`;
const P2 = `${MARK} P2 cancelled`;
const P3 = `${MARK} P3 restored`;

describe("local scheduled posts (client-side fallback)", function () {
  // wdio は各テストの開始時点の mocha タイムアウト(既定 60 秒)で打ち切りタイマーを張るため、`it` の中の
  // this.timeout(...) では 60 秒を超える待ちを延ばせない。実時間で待つこのスイートは、ここで延ばす。
  this.timeout(300000);

  let bridge: MiauthBridge;
  let token: string;
  let userId: string;
  let dueMs: number;

  // 予約時刻までに最低限残っているべき時間。遅い環境で 3 件の予約や取り消し・戻す操作が時刻に間に合わないと、
  // アプリが過去の日時を拒否したり P2 が先に投稿されたりして、分かりにくい失敗になる。原因をここで明示する。
  const assertTimeLeft = (step: string) => {
    if (dueMs - Date.now() < 15_000) {
      throw new Error(`${step} took too long; the due time is too close (CI too slow?). remaining=${dueMs - Date.now()}ms`);
    }
  };

  const postedTexts = async () =>
    (await listUserNotes(token, userId, 50)).map((n) => n.text ?? "").filter((t) => t.includes(MARK));

  before(async function () {
    this.timeout(60000);
    token = await signInAsSeededUser();
    userId = await getUserId(token, ADMIN);
    bridge = await startMiauthBridge();
  });
  after(async () => {
    await bridge?.teardown();
  });

  it("adds an account; the schedule button is available and shows the local-only notice", async function () {
    this.timeout(120000);
    await addAccountViaUi(bridge, ADMIN);
    await (await $('[data-testid="compose-textarea"]')).waitForDisplayed({ timeout: 20000 });
    await setWindow();
    await openPicker();
    const hint = await $('[data-testid="compose-schedule-local-hint"]');
    await hint.waitForDisplayed({ timeout: 15000 }); // 機能判定は非同期なので待つ
    expect(await hint.getText()).toContain("アプリを起動している間だけ");
  });

  it("schedules three posts for the same minute (P1: post, P2: cancel, P3: restore)", async function () {
    this.timeout(180000);
    // 3 件の予約・取り消し・戻す操作が時刻に間に合うよう余裕を持たせる(次の分の 0 秒。75〜135 秒先になる)
    dueMs = nextMinuteAfter(75_000);
    await scheduleAt(P1, dueMs);
    await scheduleAt(P2, dueMs);
    await scheduleAt(P3, dueMs);
    assertTimeLeft("scheduling the three posts");

    // 予約しただけでは投稿されていない
    expect(await postedTexts()).toEqual([]);

    await openList();
    await until(listRows, (rows) => rows.length >= 3, 15000, "three scheduled rows");
    const rows = await listRows();
    for (const t of [P1, P2, P3]) expect(rows.some((r) => r.text.includes(t))).toBe(true);
    // ローカル予約の注意書きが出ている
    expect(rows.every((r) => r.text.includes("アプリを起動している間だけ"))).toBe(true);
  });

  it("cancels P2 and restores P3 into the compose box", async function () {
    this.timeout(60000);
    assertTimeLeft("scheduling and listing the three posts");
    const p2 = await rowContaining(P2);
    expect(p2).toBeTruthy();
    await (await $(`[data-testid="scheduled-cancel-${rowId(p2!.testid)}"]`)).click();
    await browser.waitUntil(async () => (await rowContaining(P2)) === undefined, { timeout: 10000, timeoutMsg: "P2 stayed in the list" });

    const p3 = await rowContaining(P3);
    expect(p3).toBeTruthy();
    await (await $(`[data-testid="scheduled-restore-${rowId(p3!.testid)}"]`)).click();
    // 内容が作成欄に戻り、(未来の予約なので)予約日時も復元されて投稿ボタンは「予約」のまま
    await browser.waitUntil(async () => (await (await $('[data-testid="compose-textarea"]')).getValue()) === P3, {
      timeout: 10000,
      timeoutMsg: "P3 was not restored to the compose box",
    });
    expect(await (await $('[data-testid="compose-submit"]')).getText()).toContain("予約");
    assertTimeLeft("cancelling P2 and restoring P3");
    // 作成欄に戻したので、予約としては残らない(ここでは送信しない)
  });

  it("posts exactly P1 at the scheduled time; P2 and P3 are never posted", async function () {
    this.timeout(300000);
    // 予約時刻 + スケジューラの最大スリープ(30 秒)+ 余裕
    const waitMs = Math.max(0, dueMs - Date.now()) + 90_000;
    const posted = await until(postedTexts, (t) => t.some((x) => x.includes("P1")), waitMs, "P1 to be posted");
    expect(posted.filter((t) => t.includes("P1"))).toHaveLength(1);
    expect(posted.some((t) => t.includes("P2"))).toBe(false);
    expect(posted.some((t) => t.includes("P3"))).toBe(false);

    // 余分に待っても、P2・P3 は投稿されない(取り消した/作成欄に戻した予約は残っていない)
    await browser.pause(35_000);
    const later = await postedTexts();
    expect(later.filter((t) => t.includes("P1"))).toHaveLength(1);
    expect(later.some((t) => t.includes("P2") || t.includes("P3"))).toBe(false);
  });

  it("removes P1 from the list once it is posted", async function () {
    this.timeout(60000);
    // 作成欄に戻した P3 の内容が残っているが、一覧を開くだけなので影響しない
    await openList();
    await browser.waitUntil(async () => (await rowContaining(P1)) === undefined, {
      timeout: 15000,
      timeoutMsg: "P1 stayed in the scheduled list after being posted",
    });
    expect(await rowContaining(P2)).toBeUndefined();
    expect(await rowContaining(P3)).toBeUndefined();
  });
});
