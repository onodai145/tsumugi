// クライアント側予約(Issue #60 B)の E2E シナリオ 2: セッション 2(再起動後)。
// 事前フックで、A は「アプリが起動していない間に予約時刻を過ぎた(猶予超過)」、B は「送信中に終了した」
// 状態に書き換えてある。期待:
//  - A は自動では投稿されず「期限切れ」として一覧に残る。「今すぐ投稿」でちょうど 1 件投稿される。
//  - B は再送されず「投稿に失敗」(結果不明)になる。
import { until } from "../helpers/appInspect";
import { getUserId, listUserNotes, signInAsSeededUser } from "../helpers/misskeyApi";
import { loadState } from "../helpers/sessionHooks";
import { listRows, openList, rowContaining, rowId } from "../helpers/scheduledUi";

describe("local scheduled posts across a restart: session 2", function () {
  // wdio は各テストの開始時点の mocha タイムアウト(既定 60 秒)で打ち切るため、長い待ちはここで延ばす。
  this.timeout(300000);

  let token: string;
  let userId: string;
  let textA: string;
  let textB: string;
  let mark: string;

  const postedTexts = async () =>
    (await listUserNotes(token, userId, 50)).map((n) => n.text ?? "").filter((t) => t.includes(mark));

  before(async () => {
    const st = loadState();
    ({ textA, textB, MARK: mark } = st.s);
    token = await signInAsSeededUser();
    userId = await getUserId(token, "e2etestadmin");
    // アカウントは再利用する一時 HOME に残っているので、再ログイン(MiAuth のブリッジ)は不要
  });

  it("shows A as expired (not posted) and B as failed with an unknown result (not re-sent)", async () => {
    await (await $('[data-testid="compose-textarea"]')).waitForDisplayed({ timeout: 30000 });
    // 起動直後にスケジューラが処理するので、少し待ってから一覧を開く
    await browser.pause(3000);
    await openList();

    const a = await until(() => rowContaining(textA), (r) => r !== undefined, 15000, "row A");
    const b = await until(() => rowContaining(textB), (r) => r !== undefined, 15000, "row B");
    const idA = rowId(a!.testid);
    const idB = rowId(b!.testid);
    expect(await (await $(`[data-testid="scheduled-local-status-${idA}"]`)).getText()).toContain("期限切れ");
    expect(await (await $(`[data-testid="scheduled-local-status-${idB}"]`)).getText()).toContain("投稿に失敗");
    expect(b!.text).toContain("投稿結果が不明");

    // どちらも Misskey には投稿されていない(期限切れは自動投稿せず、送信中だったものは再送しない)
    expect(await postedTexts()).toEqual([]);
    await browser.pause(35_000); // スケジューラの周期(最大 30 秒)を 1 回以上またぐ
    expect(await postedTexts()).toEqual([]);
  });

  it("posts A exactly once with 'post now', and B is still never sent", async () => {
    const a = await rowContaining(textA);
    expect(a).toBeTruthy();
    await (await $(`[data-testid="scheduled-run-now-${rowId(a!.testid)}"]`)).click();

    const posted = await until(postedTexts, (t) => t.some((x) => x.includes("A expires")), 60000, "A to be posted");
    expect(posted.filter((t) => t.includes("A expires"))).toHaveLength(1);
    expect(posted.some((t) => t.includes("B posting"))).toBe(false);

    await browser.waitUntil(async () => (await rowContaining(textA)) === undefined, {
      timeout: 15000,
      timeoutMsg: "A stayed in the list after 'post now'",
    });
    // B は失敗として残ったまま(自動では消えず、再送もされない)
    expect(await rowContaining(textB)).toBeTruthy();
    expect((await listRows()).length).toBe(1);
    // 「今すぐ投稿」の後、もう一度スケジューラの周期をまたいでも A は 1 件のまま、B は未投稿
    await browser.pause(35_000);
    const finalPosted = await postedTexts();
    expect(finalPosted.filter((t) => t.includes("A expires"))).toHaveLength(1);
    expect(finalPosted.some((t) => t.includes("B posting"))).toBe(false);
  });
});
