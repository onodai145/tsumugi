// アカウント追加(MiAuth) → 投稿 → 自分の投稿へのリアクション、という
// tsumugiの一番基本的な操作フローを実機(tauri-driver経由のWebKitGTK)で通しで検証する。
//
// セレクタは全て実際のSvelteコンポーネント(frontend/src/ui/*.svelte)から確認した実在の
// aria-label/data-testidであり、計画段階のドラフト仕様のセレクタは採用していない。
// data-testidが存在しなかった箇所は、表示に影響しない追加のみ行って新設した
// (frontend/src/ui/AddAccount.svelte, ComposeBar.svelte, NoteCard.svelte,
// AppMenu.svelte, AddColumnModal.svelte, input/ReactionPicker.svelte)。
import { chromium } from "playwright";
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";

const MISSKEY_HOST = "misskey.local:8443";

/**
 * MiAuth同意フローには、単一アカウントしか無くても必ず「アカウントを選択してください」
 * 画面(MkAuthConfirm.vueのaccountSelectフェーズ)が先に出る。miauthBridge.tsの
 * approveNext()は「許可」ボタン(consentフェーズ)しか押さないため、先にこの画面を
 * 通過させないとapproveNext()はタイムアウトする(Task 6 report「Known follow-up for
 * Task 7」参照)。
 *
 * ここではminiauthBridge.ts自体は変更せず、bridgeが公開しているCDPポートに対して
 * 別のPlaywright CDPクライアントで接続し、同じブラウザコンテキスト上のMiAuthタブを
 * 見つけて「続ける」を押す小さなヘルパーをspec側に追加する形で対応した
 * (Task 6 reportが示した「第二のCDPクライアントをapproveNext()と並行して走らせる」
 * 方式そのもの)。approveNext()自体を書き換えるより影響範囲が小さく、Task 6の
 * 成果物に手を入れずに済む。
 */
async function clickThroughAccountSelect(cdpPort: number): Promise<void> {
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${cdpPort}`);
  try {
    // bridgeはlaunchPersistentContext()で単一コンテキストを起動済みなので、
    // connectOverCDP()時点で既にそのコンテキストが1つ存在しているはず。
    const context = browser.contexts()[0];
    if (!context) throw new Error("clickThroughAccountSelect: no browser context found via CDP");
    const page =
      context.pages().find((p) => p.url().includes("/miauth/")) ??
      (await context.waitForEvent("page", { timeout: 30000 }));
    await page.waitForLoadState("domcontentloaded");
    // 単一アカウントの場合デフォルトで選択済みのことが多いが、念のため明示的に
    // アカウント項目もクリックしておく(未選択だと「続ける」が無効なままの可能性がある)。
    await page
      .getByText("e2etestadmin", { exact: false })
      .first()
      .click({ timeout: 5000 })
      .catch(() => {});
    await page.getByRole("button", { name: "続ける" }).click({ timeout: 15000 });
  } finally {
    // connectOverCDP()で得たBrowserをclose()してもCDP接続を切るだけで、
    // tsumugi/browser-open.shが開いた実ブラウザ自体は終了しない。
    await browser.close();
  }
}

describe("account → post → reaction", () => {
  let bridge: MiauthBridge;
  let noteText: string;

  before(async () => {
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge.teardown();
  });

  it("adds an account via MiAuth", async function () {
    this.timeout(90000);

    // 起動直後、アカウント0件なのでApp.svelteが自動的にAddAccount画面を表示する
    // (App.svelte: `showAdd || reauthAccount || app.accounts.length === 0`)。
    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    // clickThroughAccountSelect() と bridge.approveNext() は両方とも同じ
    // (単一の)MiAuthタブの"page"イベントを、それぞれ別のPlaywright接続
    // (別々のBrowserContext JSオブジェクト)経由で待ち受けている。
    // Playwrightの`waitForEvent("page")`は将来発火するイベントしか拾えず、
    // 既に発生済みのイベントを後から拾うことはできない。そのため
    // 逐次実行(clickThroughAccountSelectの完了を待ってからapproveNextを
    // 呼ぶ)にすると、タブは1つしか開かれないので、先にclickThroughAccountSelect
    // 側のリスナーがそのイベントを消費してしまい、approveNext()側の
    // waitForEventは(2つ目のタブなど来ないため)30秒タイムアウトするまで
    // 単純にハングする(実機検証済み)。
    // 3つとも並行に開始し、両方のリスナーがタブ生成前に登録されるようにする。
    // approveNext()内の「許可」ボタンクリックはPlaywrightのlocator.click()の
    // 暗黙のauto-wait(既定30秒)で、clickThroughAccountSelectが先に
    // 「続ける」をクリックしてconsentフェーズへ進めるのを自然に待つ。
    await Promise.all([bridge.approveNext(), clickThroughAccountSelect(bridge.cdpPort), startButton.click()]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    // アカウントが追加されるとComposeBarが描画される(App.svelte: accounts.length > 0の時のみ表示)。
    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });

  it("posts a note", async () => {
    // カラムが1つも無いとノートを表示する場所自体が無い(App.svelte:
    // `groups.length === 0`のプレースホルダのみ)ため、投稿の前にHomeカラムを追加する。
    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();

    const addColumnItem = await $('[data-testid="app-menu-add-column"]');
    await addColumnItem.waitForDisplayed({ timeout: 15000 });
    await addColumnItem.click();

    // ソースの既定値は"home"(AddColumnModal.svelte)なので、そのまま追加するだけでよい。
    const addColumnSubmit = await $('[data-testid="add-column-submit"]');
    await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await addColumnSubmit.click();

    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    noteText = `tsumugi e2e test note ${Date.now()}`;
    await textarea.setValue(noteText);

    const submitButton = await $('[data-testid="compose-submit"]');
    await submitButton.click();

    // 自分のHomeカラムにストリーミングで即座に流れてくるはず。
    const postedNote = await $(`*=${noteText}`);
    await postedNote.waitForDisplayed({ timeout: 20000 });
  });

  it("reacts to its own note", async () => {
    const reactionButton = await $('button[aria-label="リアクション"]');
    await reactionButton.waitForDisplayed({ timeout: 15000 });
    await reactionButton.click();

    const thumbsUp = await $('[data-testid="emoji-pick-👍"]');
    await thumbsUp.waitForDisplayed({ timeout: 15000 });
    await thumbsUp.click();

    const reactionCount = await $('[data-testid="note-reaction-count-👍"]');
    await reactionCount.waitForDisplayed({ timeout: 15000 });
    await expect(reactionCount).toHaveText("1");
  });
});
