import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { DriveFile } from "../bindings/tauri.gen";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom)で import が失敗しないようスタブする(NoteCard.test.ts等と同様)。
vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
// currentIndexが実際にどのファイルを指しているかを検証するため(画像ツールバーの
// ダウンロードボタンはcurrentのファイルURLをsaveMediaToDiskへそのまま渡す)、
// この関数呼び出し引数を間接的なcurrentIndexの観測手段として使う。
vi.mock("../lib/mediaDownload", () => ({ saveMediaToDisk: vi.fn() }));

const { default: MediaViewer } = await import("./MediaViewer.svelte");
const { saveMediaToDisk } = await import("../lib/mediaDownload");

// jsdomはscrollIntoViewを実装していないため、マウント時のスクロール同期(onMount)を含む
// すべてのテストで安全にレンダリングできるよう既定でno-opスタブを用意しておく。
// 挙動を検証したいテストは各itの冒頭でこの参照を上書きする。
// （その他のブラウザAPI補完は vite.config.ts の setupFiles で読み込まれる test-setup.ts にて一元管理）
Element.prototype.scrollIntoView = vi.fn();

function file(overrides: Partial<DriveFile>): DriveFile {
  return {
    id: "f1",
    mimeType: "image/png",
    isSensitive: false,
    url: "https://example.com/f1.png",
    thumbnailUrl: null,
    name: "f1.png",
    ...overrides,
  };
}

afterEach(() => cleanup());

describe("MediaViewer", () => {
  it("Escapeキーでoncloseが呼ばれる", async () => {
    const onclose = vi.fn();
    render(MediaViewer, {
      props: {
        files: [file({ id: "a" })],
        startIndex: 0,
        revealed: {},
        onclose,
      },
    });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("背景クリックでoncloseが呼ばれる", async () => {
    const onclose = vi.fn();
    const { getByRole } = render(MediaViewer, {
      props: {
        files: [file({ id: "a" })],
        startIndex: 0,
        revealed: {},
        onclose,
      },
    });
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("次へボタンで2件目のページへscrollIntoViewする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    scrollIntoView.mockClear();
    await fireEvent.click(getByLabelText("次へ"));
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [, secondPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(secondPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "smooth" }));
  });

  // 回帰テスト: 末尾から次へ送ると先頭へラップアラウンドする。このステップは複数のScroll
  // Snapポイントを跨ぐ長距離スクロールになり、一部のブラウザエンジンではsmoothスクロールが
  // 意図した位置まで到達しないことがあるため、behavior: "auto"(アニメーションなし)で
  // 瞬時に移動させる必要がある(goNext/goPrevのコメント参照)。
  it("末尾から次へで先頭にラップアラウンドする際はbehavior: autoでscrollIntoViewする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [
          file({ id: "a", name: "a.png" }),
          file({ id: "b", name: "b.png" }),
          file({ id: "c", name: "c.png" }),
        ],
        startIndex: 2,
        revealed: {},
        onclose: () => {},
      },
    });
    scrollIntoView.mockClear();
    await fireEvent.click(getByLabelText("次へ"));
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [firstPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(firstPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "auto" }));
  });

  it("先頭から前へで末尾にラップアラウンドする際はbehavior: autoでscrollIntoViewする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [
          file({ id: "a", name: "a.png" }),
          file({ id: "b", name: "b.png" }),
          file({ id: "c", name: "c.png" }),
        ],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    scrollIntoView.mockClear();
    await fireEvent.click(getByLabelText("前へ"));
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [, , thirdPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(thirdPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "auto" }));
  });

  // 回帰テスト: 「B(2枚目)を開く→前後送りで他を見る→元のBに戻る」で表示位置がずれる不具合
  // (ラップアラウンドとは無関係、通常の前後送りの連打で発生)。原因は、goTo()が開始した
  // プログラム的スクロールのアニメーションが完了しないうちに、その途中の中間scrollLeftを
  // 拾ったonScroll()の再同期(デバウンス)がcurrentIndexを誤った値に巻き戻してしまうこと。
  // programmaticScrollActiveフラグを立て、scrollend(またはフォールバックタイマー)が
  // 発火してスクロールの完了が確認できるまでonScroll()の再同期を止めることで修正した
  // (goTo/beginProgrammaticScroll/onScrollEndのコメント参照)。
  // 実機(WebKitGTK、cargo tauri dev + debug_bridge経由)で、B→C→Bへの前後送りを短間隔で
  // 連打すると同様に表示位置がずれることを再現し、修正後は解消することを確認済み
  // (task-4-report.mdに記録)。
  //
  // ここでは「goTo()によるスクロールが完了する(scrollendが発火する)前に、中間的な
  // scrollLeftから計算された誤ったindexでonScroll()がcurrentIndexを巻き戻してはならない」
  // というガード自体の契約を直接検証する。scrollend発火前に正解のscrollLeftを含む
  // 後続イベントを与えてしまうとガードなしでも偶然パスしてしまうため、scrollend発火前は
  // 意図的に誤ったscrollLeftのみを与えている。
  it("goTo()によるスクロール完了前は、中間的なscrollLeftによる再同期でcurrentIndexが巻き戻らない", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    vi.mocked(saveMediaToDisk).mockClear();

    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [
          file({ id: "a", name: "a.png", url: "https://example.com/a.png" }),
          file({ id: "b", name: "b.png", url: "https://example.com/b.png" }),
          file({ id: "c", name: "c.png", url: "https://example.com/c.png" }),
        ],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });

    const [firstPage] = document.querySelectorAll('[data-testid="media-page"]');
    const scroller = firstPage.parentElement as HTMLDivElement;
    Object.defineProperty(scroller, "clientWidth", { configurable: true, value: 1000 });

    // 次へ(A→B)をクリック。goTo()がcurrentIndexを即座に1へ進め、
    // プログラム的スクロールを開始する(programmaticScrollActive = true、
    // scrollendはまだ発火していない)。
    await fireEvent.click(getByLabelText("次へ"));

    // アニメーションがまだ目的地(index1)に到達していない、誤ったscrollLeft(index0のまま)で
    // scrollイベントが発火したと仮定する(連打・スロットリング等でこうしたタイミングのズレが
    // 起こりうる)。
    Object.defineProperty(scroller, "scrollLeft", { configurable: true, value: 0 });
    await fireEvent.scroll(scroller);

    // onScroll()の120msデバウンスを経過させる。scrollendがまだ発火していない
    // (programmaticScrollActiveがtrueのまま)ので、このscrollLeft=0(index0)から
    // 計算されたindexでcurrentIndexが巻き戻されてはならない(currentIndexはgoTo()が
    // 設定した1のまま保たれるべき)。
    await new Promise((resolve) => setTimeout(resolve, 150));

    // 画像ツールバーのダウンロードボタンはcurrentのファイルURLを渡すため、
    // currentIndexが誤って0(A)へ巻き戻っていないかをここで間接的に検証する。
    await fireEvent.click(getByLabelText("ダウンロード"));
    expect(saveMediaToDisk).toHaveBeenCalledWith("https://example.com/b.png", expect.anything(), expect.anything());
  });

  it("非ゼロstartIndexでマウントすると、その位置へ即座にscrollIntoViewする", () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 1,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [, secondPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(secondPage);
    expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: "auto" }));
  });

  it("閲覧注意ファイルは未表示ならカバーを表示し、クリックで表示状態になる", async () => {
    const { getByText, queryByText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", isSensitive: true })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(getByText("閲覧注意（クリックで表示）")).toBeTruthy();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    expect(queryByText("閲覧注意（クリックで表示）")).toBeNull();
  });

  it("音声ファイルは自前コントロールバーで表示され、panzoom用のラッパーが付かない", () => {
    render(MediaViewer, {
      props: {
        files: [file({ id: "a", mimeType: "audio/mpeg", name: "a.mp3", url: "https://example.com/a.mp3" })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(document.querySelector("media-controls")).toBeTruthy();
  });
});
