// Issue #458: #451(カラムの編集と閉じる)の実機確認。
//  - 上スクロール(loadMore)の最中にフィルタを編集しても、旧フィルタのノートが混ざらない(画面とキャッシュDB)
//  - 名前だけの変更は、進行中のloadMoreの結果を捨てない
//  - 上スクロールの最中にカラムを閉じても、孤児データ(column_note/column_source_boundary)が残らない
//  - 編集後、新しいフィルタのストリームが開き、ライブのノートが届く
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { signInAsSeededUser, createNote, signUp, allowRegistration, followUser } from "../helpers/misskeyApi";
import {
  addAccountViaUi, addTqlColumn, installTrace, takeTrace, noteTexts, waitForNote, openEditActiveTab,
  queryCache, setWindow, SCROLL_ACTIVE_JS,
} from "../helpers/appInspect";

const RUN = Date.now();
const ALPHA = `v458alpha${RUN}`;
const BRAVO = `v458bravo${RUN}`;
const marker = (m: string) => (m === ALPHA ? "alpha" : "bravo");
const tqlFor = (m: string) => `from home where text -> "${m}"`;

describe("#451 column edit / close during backfill", () => {
  let bridge: MiauthBridge;
  let token: string;
  let posterToken: string;

  before(async function () {
    this.timeout(300000);
    token = await signInAsSeededUser();
    // 投稿枠(notes/createは1時間300件)をadminと分けるため、毎回新しい投稿者を作り、adminにフォローさせる。
    await allowRegistration(token);
    const poster = `e2ep${String(RUN).slice(-9)}`;
    ({ token: posterToken } = await signUp(poster, "e2eTestPassword5!"));
    await followUser(token, poster);
    // ページ(20件)を超える量を、alpha/bravoで交互に投稿する。古い側ほど遡りが必要になる。
    for (let i = 0; i < 100; i++) {
      await createNote(posterToken, `v458 n${i} ${i % 2 === 0 ? ALPHA : BRAVO}`);
    }
    await new Promise((r) => setTimeout(r, 10000));
    bridge = await startMiauthBridge();
  });
  after(async () => { await bridge?.teardown(); });

  it("adds an account and an alpha column", async function () {
    this.timeout(120000);
    await addAccountViaUi(bridge, "e2etestadmin");
    await setWindow();
    await addTqlColumn(tqlFor(ALPHA));
    await waitForNote(ALPHA);
    const texts = await noteTexts();
    console.log(`[e2e] initial alpha column: ${texts.length} notes, bravo mixed=${texts.some((t) => t.includes(BRAVO))}`);
    expect(texts.some((t) => t.includes(BRAVO))).toBe(false);
    await installTrace();
  });

  // delay=scrollイベント(loadMore)からsubmit(updateColumn)までのms。窓の前後を掃引する。
  for (const delay of [0, 3, 10, 25, 60]) {
    it(`edit filter during loadMore (delay ${delay}ms): no stale notes, new stream live`, async function () {
      this.timeout(120000);
      const cur = (await noteTexts()).some((t) => t.includes(ALPHA)) ? ALPHA : BRAVO;
      const next = cur === ALPHA ? BRAVO : ALPHA;
      await takeTrace();
      await openEditActiveTab();
      const ta = await $('[data-testid="add-column-tql-textarea"]');
      await ta.waitForDisplayed({ timeout: 15000 });
      await ta.setValue(tqlFor(next));
      await takeTrace(); // モーダル操作中のノイズを捨てる
      const scrolled = await browser.executeAsync(
        (script: string, ms: number, done: (v: boolean) => void) => {
          const ok = new Function(script)() as boolean;
          setTimeout(() => {
            (document.querySelector('[data-testid="add-column-submit"]') as HTMLElement).click();
            done(ok);
          }, ms);
        },
        SCROLL_ACTIVE_JS,
        delay,
      );
      expect(scrolled).toBe(true);

      // 編集が反映され、新フィルタの内容に入れ替わるまで待つ
      await browser.waitUntil(
        async () => {
          const t = await noteTexts();
          return t.length > 0 && t.every((x) => x.includes(next));
        },
        { timeout: 30000, interval: 300, timeoutMsg: `column did not switch to ${marker(next)}` },
      );
      // 遅れて届く旧loadMoreの結果が混ざらないか、少し待ってから見る
      await browser.pause(3000);
      const texts = await noteTexts();
      const stale = texts.filter((t) => t.includes(cur));
      expect(stale.length).toBe(0);

      const trace = await takeTrace();
      const fb = trace.filter((e) => e.cmd === "fetch_backfill");
      const uc = trace.filter((e) => e.cmd === "update_column");
      const fbStart = fb.find((e) => e.ev === "start")?.t;
      const fbEnd = fb.find((e) => e.ev !== "start")?.t;
      const ucEnd = uc.find((e) => e.ev !== "start")?.t;
      const overlap = fbStart !== undefined && ucEnd !== undefined && fbStart < ucEnd;
      const staleReturn = fbEnd !== undefined && ucEnd !== undefined && fbEnd > ucEnd;
      console.log(`[e2e] delay=${delay} ${marker(cur)}->${marker(next)} trace=${JSON.stringify(trace)} overlap=${overlap} fetchReturnedAfterUpdate=${staleReturn} notes=${texts.length}`);

      // キャッシュDBにも旧フィルタのノートが残っていない
      const rows = queryCache(
        `SELECT COUNT(*) FROM column_note cn JOIN note n ON n.id = cn.note_id WHERE n.text LIKE '%${cur}%'`,
      );
      console.log(`[e2e] delay=${delay} stale rows in column_note (${marker(cur)}): ${rows[0][0]}`);
      expect(Number(rows[0][0])).toBe(0);

      // 新しいフィルタのストリームでライブのノートが届く
      const live = `v458 live ${next} ${Date.now()}`;
      await createNote(posterToken, live);
      await waitForNote(live, 20000);
    });
  }

  it("name-only change keeps the in-flight loadMore result", async function () {
    this.timeout(120000);
    // 直前までのupdate_columnで一覧は20件+ライブ。件数を控え、名前だけを変えて上スクロールと同時に保存する。
    const before = (await noteTexts()).length;
    await takeTrace();
    await openEditActiveTab();
    const name = await $('[data-testid="add-column-name-input"]');
    await name.waitForDisplayed({ timeout: 15000 });
    await name.setValue(`renamed-${RUN}`);
    await takeTrace();
    await browser.executeAsync(
      (script: string, done: (v: boolean) => void) => {
        const ok = new Function(script)() as boolean;
        setTimeout(() => {
          (document.querySelector('[data-testid="add-column-submit"]') as HTMLElement).click();
          done(ok);
        }, 3);
      },
      SCROLL_ACTIVE_JS,
    );
    await browser.waitUntil(async () => (await noteTexts()).length > before, {
      timeout: 30000,
      interval: 300,
      timeoutMsg: `loadMore result was dropped by a name-only change (notes stayed at ${before})`,
    });
    const trace = await takeTrace();
    console.log(`[e2e] name-only: ${before} -> ${(await noteTexts()).length} trace=${JSON.stringify(trace)}`);
    expect(trace.some((e) => e.cmd === "update_column")).toBe(false);
  });

  it("closing the column during loadMore leaves no orphan rows and no error", async function () {
    this.timeout(120000);
    await takeTrace();
    const colRows = queryCache(`SELECT DISTINCT column_id FROM column_note`);
    console.log(`[e2e] column ids before close: ${JSON.stringify(colRows)}`);
    await browser.executeAsync(
      (script: string, done: (v: boolean) => void) => {
        const ok = new Function(script)() as boolean;
        setTimeout(() => {
          const btn = Array.from(document.querySelectorAll('button[title="タブを閉じる"]')).find(
            (b) => (b as HTMLElement).offsetParent !== null,
          ) as HTMLElement;
          btn.click();
          done(ok);
        }, 3);
      },
      SCROLL_ACTIVE_JS,
    );
    await browser.waitUntil(async () => (await noteTexts()).length === 0, { timeout: 15000, interval: 300 });
    await browser.pause(4000);
    const trace = await takeTrace();
    console.log(`[e2e] close trace=${JSON.stringify(trace)}`);
    const cn = queryCache(`SELECT COUNT(*) FROM column_note`)[0][0];
    const bd = queryCache(`SELECT COUNT(*) FROM column_source_boundary`)[0][0];
    console.log(`[e2e] after close: column_note=${cn} column_source_boundary=${bd}`);
    expect(Number(cn)).toBe(0);
    expect(Number(bd)).toBe(0);
    // 失敗モーダル等が出ていない
    const modals = await browser.execute(() => document.querySelectorAll('[data-testid="modal-close"]').length);
    expect(modals).toBe(0);
  });
});
