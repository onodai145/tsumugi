// メディアビューワー(render/MediaViewer.svelte)のセーフエリア検証(Issue #384)。
// 画像付きノートを事前にMisskeyへ投稿してHomeカラムに表示し、画像セルをタップしてビューワーを開く。
// 画像準備の失敗リスクを既存specから切り離すため、safe-area.e2e.ts とは別ファイルにしている。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { createNote, signInAsSeededUser, uploadImage } from "../helpers/misskeyApi";
import { addAccountAndEnableMobile, addHomeColumn, rect, SAFE_AREA, setSafeArea } from "../helpers/mobile";

describe("mobile safe-area: media viewer", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    const token = await signInAsSeededUser();
    const fileId = await uploadImage(token);
    await createNote(token, `e2e media viewer ${Date.now()}`, [fileId]);

    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileSafeAreaMedia");
    await addHomeColumn();

    const image = await $(".media-cell img");
    await image.waitForDisplayed({ timeout: 30000 });
    await image.click();
    await $('button[aria-label="閉じる"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await bridge?.teardown();
  });

  it("keeps the close button below the top safe area", async () => {
    // 既定padding(0.5rem=8px)より大きい SAFE_AREA.top(47) で var(--safe-top) の効きを検出する。
    const close = await rect('button[aria-label="閉じる"]');
    expect(close).not.toBeNull();
    expect(close!.top).toBeGreaterThanOrEqual(SAFE_AREA.top);
  });

  it("keeps the close button inside the right safe area", async () => {
    const RIGHT = 24;
    await setSafeArea({ ...SAFE_AREA, right: RIGHT });
    try {
      const close = await rect('button[aria-label="閉じる"]');
      const vw = await browser.execute(() => window.innerWidth);
      expect(close).not.toBeNull();
      expect(close!.right).toBeLessThanOrEqual(vw - RIGHT);
    } finally {
      await setSafeArea(SAFE_AREA);
    }
  });

  it("keeps the image toolbar buttons above the bottom safe area", async () => {
    // ツールバー(group)自体はpaddingを含めて下端まで届くため、内側のボタンで測る。
    const zoomIn = await rect('button[aria-label="ズームイン"]');
    const vh = await browser.execute(() => window.innerHeight);
    expect(zoomIn).not.toBeNull();
    expect(zoomIn!.bottom).toBeLessThanOrEqual(vh - SAFE_AREA.bottom);
  });
});
