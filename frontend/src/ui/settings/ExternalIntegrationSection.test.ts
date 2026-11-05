import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";
import { commands } from "../../lib/ipc";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: ExternalIntegrationSection } = await import("./ExternalIntegrationSection.svelte");

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ExternalIntegrationSection 翻訳先言語", () => {
  it("OSネイティブの<select>ではなく、テーマ適用済みのDropdownを使う", () => {
    // ネイティブ<select>の一覧はWebKitGTK/OSが描画し、アプリのテーマが効かない(Issue #440の修正)。
    const { container } = render(ExternalIntegrationSection);
    expect(container.querySelector("select")).toBeNull();
  });

  it("既定は ja(日本語)が選ばれている", () => {
    app.ui = { ...app.ui, translateTargetLang: "ja" };
    const { getByTestId } = render(ExternalIntegrationSection);
    expect(getByTestId("translate-target-lang").textContent).toContain("日本語");
  });

  it("変更して保存すると translateTargetLang だけが変わり、他の項目は保持される", async () => {
    app.ui = { ...app.ui, translateTargetLang: "ja", searchEngineUrl: "https://example.com/?q={query}" };
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByTestId, getByText } = render(ExternalIntegrationSection);

    getByTestId("translate-target-lang").click();
    await vi.waitFor(() => expect(getByTestId("translate-target-lang-option-en")).toBeTruthy());
    getByTestId("translate-target-lang-option-en").click();
    getByText("保存").click();

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    const saved = spy.mock.calls[0][0];
    expect(saved.translateTargetLang).toBe("en");
    expect(saved.searchEngineUrl).toBe("https://example.com/?q={query}");
  });

  it("プリセットに無い保存値(手編集した設定ファイル等)も選択肢に出して保持する", async () => {
    app.ui = { ...app.ui, translateTargetLang: "pt-BR" };
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByTestId, getByText } = render(ExternalIntegrationSection);
    expect(getByTestId("translate-target-lang").textContent).toContain("pt-BR");

    getByText("保存").click();
    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy.mock.calls[0][0].translateTargetLang).toBe("pt-BR");
  });
});

describe("ExternalIntegrationSection 外部サービスのURL", () => {
  const SEARCH_PLACEHOLDER = "検索URLテンプレート（{query} をクエリ文字列に置換）";
  const SUMMALY_PLACEHOLDER = "空欄なら接続先インスタンスの /url を使用";

  function mockImported(value: boolean) {
    return vi.spyOn(commands, "getSummalyProxyImported").mockResolvedValue({ status: "ok", data: value });
  }

  it("javascript: の検索URLは保存できず、エラーが出る", async () => {
    mockImported(false);
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByPlaceholderText, getByText } = render(ExternalIntegrationSection);

    await fireEvent.input(getByPlaceholderText(SEARCH_PLACEHOLDER), { target: { value: "javascript:alert({query})" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(getByText(/検索エンジンのURL/)).toBeTruthy());
    expect(spy).not.toHaveBeenCalled();
  });

  it("http/https 以外のURLプレビュー取得先は保存できず、エラーが出る", async () => {
    mockImported(false);
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByPlaceholderText, getByText } = render(ExternalIntegrationSection);

    await fireEvent.input(getByPlaceholderText(SUMMALY_PLACEHOLDER), { target: { value: "ftp://example.com/url" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(getByText(/URLプレビューの取得先/)).toBeTruthy());
    expect(spy).not.toHaveBeenCalled();
  });

  it("手入力のLANの取得先は保存できる", async () => {
    mockImported(false);
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByPlaceholderText, getByText } = render(ExternalIntegrationSection);

    await fireEvent.input(getByPlaceholderText(SUMMALY_PLACEHOLDER), { target: { value: "http://192.168.1.10:3030/url" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy.mock.calls[0][0].summalyProxyUrl).toBe("http://192.168.1.10:3030/url");
  });

  it("インポート由来の取得先では注意文が出て、保存すると信頼のコマンドが呼ばれて注意文が消える", async () => {
    mockImported(true);
    vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const trust = vi.spyOn(commands, "trustSummalyProxy").mockResolvedValue({ status: "ok", data: null });
    const { getByTestId, queryByTestId, getByText } = render(ExternalIntegrationSection);

    await vi.waitFor(() => expect(getByTestId("summaly-imported-notice")).toBeTruthy());
    getByText("保存").click();

    await vi.waitFor(() => expect(trust).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(queryByTestId("summaly-imported-notice")).toBeNull());
  });

  it("インポート由来でなければ注意文は出ず、信頼のコマンドも呼ばれない", async () => {
    mockImported(false);
    vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const trust = vi.spyOn(commands, "trustSummalyProxy").mockResolvedValue({ status: "ok", data: null });
    const { queryByTestId, getByText } = render(ExternalIntegrationSection);

    getByText("保存").click();

    await vi.waitFor(() => expect(getByText("保存しました")).toBeTruthy());
    expect(queryByTestId("summaly-imported-notice")).toBeNull();
    expect(trust).not.toHaveBeenCalled();
  });

  it("検証に落ちて保存しなかったときは、信頼のコマンドを呼ばない", async () => {
    mockImported(true);
    vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const trust = vi.spyOn(commands, "trustSummalyProxy").mockResolvedValue({ status: "ok", data: null });
    const { getByPlaceholderText, getByTestId, getByText } = render(ExternalIntegrationSection);

    await vi.waitFor(() => expect(getByTestId("summaly-imported-notice")).toBeTruthy());
    await fireEvent.input(getByPlaceholderText(SUMMALY_PLACEHOLDER), { target: { value: "ftp://example.com/url" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(getByText(/URLプレビューの取得先/)).toBeTruthy());
    expect(trust).not.toHaveBeenCalled();
  });
});
