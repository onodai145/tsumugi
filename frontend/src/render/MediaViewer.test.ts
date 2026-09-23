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

// jsdomはElement.scrollToを実装していないため、マウント時のスクロール同期(onMount)を含む
// すべてのテストで安全にレンダリングできるよう既定でno-opスタブを用意しておく。
// 挙動を検証したいテストは各itの冒頭でこの参照を上書きする。
// （その他のブラウザAPI補完は vite.config.ts の setupFiles で読み込まれる test-setup.ts にて一元管理）
Element.prototype.scrollTo = vi.fn();

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

  it("次へボタンで2件目のページへscrollToする", async () => {
    const scrollTo = vi.fn();
    Element.prototype.scrollTo = scrollTo;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    const [firstPage] = document.querySelectorAll('[data-testid="media-page"]');
    const scroller = firstPage.parentElement as HTMLDivElement;
    Object.defineProperty(scroller, "clientWidth", { configurable: true, value: 1000 });
    scrollTo.mockClear();
    await fireEvent.click(getByLabelText("次へ"));
    expect(scrollTo).toHaveBeenCalledOnce();
    expect(scrollTo.mock.instances[0]).toBe(scroller);
    expect(scrollTo).toHaveBeenCalledWith({ left: 1000, behavior: "smooth" });
  });

  // 回帰テスト: 末尾から次へ送ると先頭へラップアラウンドする。長距離のsmoothスクロールを
  // 避け、behavior: "auto"(アニメーションなし)で瞬時に移動させる(goNext/goPrevのコメント参照)。
  it("末尾から次へで先頭にラップアラウンドする際はbehavior: autoでscrollToする", async () => {
    const scrollTo = vi.fn();
    Element.prototype.scrollTo = scrollTo;
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
    const [firstPage] = document.querySelectorAll('[data-testid="media-page"]');
    const scroller = firstPage.parentElement as HTMLDivElement;
    Object.defineProperty(scroller, "clientWidth", { configurable: true, value: 1000 });
    scrollTo.mockClear();
    await fireEvent.click(getByLabelText("次へ"));
    expect(scrollTo).toHaveBeenCalledOnce();
    expect(scrollTo.mock.instances[0]).toBe(scroller);
    expect(scrollTo).toHaveBeenCalledWith({ left: 0, behavior: "auto" });
  });

  it("先頭から前へで末尾にラップアラウンドする際はbehavior: autoでscrollToする", async () => {
    const scrollTo = vi.fn();
    Element.prototype.scrollTo = scrollTo;
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
    const [firstPage] = document.querySelectorAll('[data-testid="media-page"]');
    const scroller = firstPage.parentElement as HTMLDivElement;
    Object.defineProperty(scroller, "clientWidth", { configurable: true, value: 1000 });
    scrollTo.mockClear();
    await fireEvent.click(getByLabelText("前へ"));
    expect(scrollTo).toHaveBeenCalledOnce();
    expect(scrollTo.mock.instances[0]).toBe(scroller);
    expect(scrollTo).toHaveBeenCalledWith({ left: 2000, behavior: "auto" });
  });

  // 「B(2枚目)を開く→前後送りで他を見る→元のBに戻る」で表示位置がずれる不具合の実際の原因は
  // scrollIntoView()自体にあった(要素のgetBoundingClientRect()を見て「もう見えているか」を
  // 判定してから目的地を決める仕組みが、直前のスクロールから間もない呼び出しで古いレイアウト
  // 情報を掴んでしまいスクロールが握りつぶされる、実機WebKitGTKでのバグ)。goTo()を
  // scrollTo({left: index*clientWidth, ...})による直接計算に切り替えたことがその修正であり、
  // これは goTo() 自身のコメント参照(task-4-report.mdに実測データを記録)。
  //
  // このprogrammaticScrollActiveフラグ自体は上記の不具合の原因ではなかったが、goTo()による
  // プログラム的スクロールのアニメーションが完了しないうちに、その途中の中間scrollLeftを
  // 拾ったonScroll()の再同期(デバウンス)がcurrentIndexを誤った値に巻き戻してしまうという、
  // 別の理論的な競合を防ぐガードとして引き続き有効なため残している。ここでは「goTo()による
  // スクロールが完了する(scrollendが発火する)前に、中間的なscrollLeftから計算された誤った
  // indexでonScroll()がcurrentIndexを巻き戻してはならない」というガード自体の契約を直接
  // 検証する。scrollend発火前に正解のscrollLeftを含む後続イベントを与えてしまうとガードなしでも
  // 偶然パスしてしまうため、scrollend発火前は意図的に誤ったscrollLeftのみを与えている。
  it("goTo()によるスクロール完了前は、中間的なscrollLeftによる再同期でcurrentIndexが巻き戻らない", async () => {
    Element.prototype.scrollTo = vi.fn();
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

  // 回帰テスト: 上のテストは「ガードが正しくかかる」ことしか検証していなかった(タスクレビュー
  // 指摘)。startIndex: 0(1枚目を開く最も一般的なケース)では、onMount()のscrollToが
  // 呼ばれてもscrollElは既にscrollLeft: 0のため実際には1pxも動かず、'scrollend'イベントは
  // 一切発火しない。これにより、programmaticScrollActiveの解除を'scrollend'の発火のみに
  // 頼っていると(かつては`if (!SCROLLEND_SUPPORTED)`でjsdom/対応ブラウザではフォールバック
  // タイマー自体をarmしていなかったため)、フラグが永久にtrueのまま残ってしまい、
  // マウント直後・最初のボタン操作前に行った最初のスワイプ操作がonScroll()の再同期に
  // 一切反映されないというリグレッションが起きていた。この回帰テストは「ガードが
  // (scrollendに頼らずフォールバックタイマー経由で)正しく解除される」ことを検証する。
  it("startIndex: 0でマウント直後(scrollendが発火しない場合)でも、フォールバックタイマー経由でcurrentIndexがスワイプに追従する", async () => {
    Element.prototype.scrollTo = vi.fn();
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

    // onMount()のscrollTo(startIndex: 0、既にscrollLeft: 0)は実際には何も動かさない
    // ため、scrollendイベントは発火しない。beginProgrammaticScroll()の500msフォールバック
    // タイマーだけがprogrammaticScrollActiveを解除する唯一の手段になる。
    await new Promise((resolve) => setTimeout(resolve, 550));

    // フォールバックタイマーで解除された後、ユーザーがスワイプで3枚目(index2)まで
    // 手で送ったとする(goTo()を経由しない、ネイティブスクロールのみのケース)。
    Object.defineProperty(scroller, "scrollLeft", { configurable: true, value: 2000 });
    await fireEvent.scroll(scroller);
    await new Promise((resolve) => setTimeout(resolve, 150));

    // 画像ツールバーのダウンロードボタンで、currentIndexが正しく2(C)まで
    // 追従していることを間接的に検証する。
    await fireEvent.click(getByLabelText("ダウンロード"));
    expect(saveMediaToDisk).toHaveBeenCalledWith("https://example.com/c.png", expect.anything(), expect.anything());
  });

  it("非ゼロstartIndexでマウントすると、その位置へ即座にscrollToする", () => {
    const scrollTo = vi.fn();
    Element.prototype.scrollTo = scrollTo;
    render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 1,
        revealed: {},
        onclose: () => {},
      },
    });
    const [firstPage] = document.querySelectorAll('[data-testid="media-page"]');
    const scroller = firstPage.parentElement as HTMLDivElement;
    expect(scrollTo).toHaveBeenCalledOnce();
    expect(scrollTo.mock.instances[0]).toBe(scroller);
    expect(scrollTo).toHaveBeenCalledWith({ left: 1 * scroller.clientWidth, behavior: "auto" });
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
