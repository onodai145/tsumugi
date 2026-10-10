// クライアント側予約(Issue #60 B)の E2E シナリオ 2: セッション 1。アカウントを追加し、1 時間後の
// ローカル予約を 2 件作ってアプリを終了する。セッションの合間に scheduled_posts.json が書き換えられる
// (helpers/sessionHooks.ts の runPreSession)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountViaUi, setWindow } from "../helpers/appInspect";
import { saveState } from "../helpers/sessionHooks";
import { openPicker, scheduleAt } from "../helpers/scheduledUi";

const RUN = Date.now();
const MARK = `e2esr${RUN}`;
const TEXT_A = `${MARK} A expires`;
const TEXT_B = `${MARK} B posting`;

describe("local scheduled posts across a restart: session 1", function () {
  // wdio は各テストの開始時点の mocha タイムアウト(既定 60 秒)で打ち切るため、長い待ちはここで延ばす。
  this.timeout(300000);

  let bridge: MiauthBridge;
  before(async () => {
    bridge = await startMiauthBridge();
  });
  after(async () => {
    await bridge?.teardown();
  });

  it("adds an account and schedules two posts one hour ahead", async () => {
    await addAccountViaUi(bridge, "e2etestadmin");
    await (await $('[data-testid="compose-textarea"]')).waitForDisplayed({ timeout: 20000 });
    await setWindow();
    await openPicker();
    await (await $('[data-testid="compose-schedule-local-hint"]')).waitForDisplayed({ timeout: 15000 });

    const inAnHour = Date.now() + 3_600_000;
    await scheduleAt(TEXT_A, inAnHour);
    await scheduleAt(TEXT_B, inAnHour);
    saveState({ s: { MARK, textA: TEXT_A, textB: TEXT_B } });
  });
});
