import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";
import { commands } from "../../lib/ipc";
import type { McpStatus } from "../../lib/ipc";

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

const { default: McpSection } = await import("./McpSection.svelte");

const status = (over: Partial<McpStatus> = {}): McpStatus => ({
  enabled: false,
  port: 49731,
  state: { type: "stopped" },
  exposedAccountIds: [],
  ...over,
});
const ok = <T,>(data: T) => ({ status: "ok" as const, data });

beforeEach(() => {
  app.accounts = [
    { id: "a1", host: "one.example", username: "me", displayName: "Me", userId: "u1" } as never,
    { id: "a2", host: "two.example", username: "you", displayName: "You", userId: "u2" } as never,
  ];
  vi.spyOn(commands, "getMcpStatus").mockResolvedValue(ok(status()));
  vi.spyOn(commands, "getMcpToken").mockResolvedValue(ok("tok-123"));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("McpSection", () => {
  it("既定はオフで、アカウントのチェックも全部オフ。注意書きがある", async () => {
    const { getByTestId, findByTestId } = render(McpSection);
    const toggle = (await findByTestId("mcp-enabled")) as HTMLInputElement;
    expect(toggle.checked).toBe(false);
    expect((getByTestId("mcp-account-a1") as HTMLInputElement).checked).toBe(false);
    expect((getByTestId("mcp-account-a2") as HTMLInputElement).checked).toBe(false);
    expect(getByTestId("mcp-warning").textContent).toContain("第三者");
    expect(getByTestId("mcp-state").textContent).toContain("停止中");
  });

  it("トグルを押すと setMcpEnabled を呼び、返った状態を表示する", async () => {
    const spy = vi
      .spyOn(commands, "setMcpEnabled")
      .mockResolvedValue(ok(status({ enabled: true, state: { type: "running" } })));
    const { findByTestId, getByTestId } = render(McpSection);
    await fireEvent.click(await findByTestId("mcp-enabled"));
    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith(true));
    await vi.waitFor(() => expect(getByTestId("mcp-state").textContent).toContain("稼働中"));
  });

  it("有効化がエラー(keyringなど)なら、オフのまま理由を出す", async () => {
    vi.spyOn(commands, "setMcpEnabled").mockResolvedValue({ status: "error", error: "no keyring" });
    const { findByTestId, getByTestId } = render(McpSection);
    const toggle = (await findByTestId("mcp-enabled")) as HTMLInputElement;
    await fireEvent.click(toggle);
    await vi.waitFor(() => expect(getByTestId("mcp-error").textContent).toContain("no keyring"));
    expect(toggle.checked).toBe(false);
  });

  it("バインド失敗(状態がError)なら理由を出し、トグルはオンのまま", async () => {
    vi.mocked(commands.getMcpStatus).mockResolvedValue(
      ok(status({ enabled: true, state: { type: "error", message: "port in use" } })),
    );
    const { findByTestId, getByTestId } = render(McpSection);
    expect(((await findByTestId("mcp-enabled")) as HTMLInputElement).checked).toBe(true);
    expect(getByTestId("mcp-state").textContent).toContain("port in use");
  });

  it("アカウントのチェックで setMcpAccountExposed を呼ぶ", async () => {
    const spy = vi
      .spyOn(commands, "setMcpAccountExposed")
      .mockResolvedValue(ok(status({ exposedAccountIds: ["a1"] })));
    const { findByTestId } = render(McpSection);
    await fireEvent.click(await findByTestId("mcp-account-a1"));
    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith("a1", true));
  });

  it("ポートを確定すると setMcpPort を呼ぶ", async () => {
    const spy = vi.spyOn(commands, "setMcpPort").mockResolvedValue(ok(status({ port: 50000 })));
    const { findByTestId } = render(McpSection);
    const input = (await findByTestId("mcp-port")) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "50000" } });
    await fireEvent.change(input);
    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith(50000));
  });

  it("エンドポイントは127.0.0.1のURLで、ポートに追従する", async () => {
    vi.mocked(commands.getMcpStatus).mockResolvedValue(ok(status({ port: 50123 })));
    const { findByTestId } = render(McpSection);
    expect((await findByTestId("mcp-endpoint")).textContent).toContain("http://127.0.0.1:50123/mcp");
  });

  it("トークンは通常マスクされ、「表示」で見える。表示するまで取得しない", async () => {
    const { findByTestId, getByTestId } = render(McpSection);
    await findByTestId("mcp-enabled");
    expect(commands.getMcpToken).not.toHaveBeenCalled();
    expect(getByTestId("mcp-token").textContent).not.toContain("tok-123");
    await fireEvent.click(getByTestId("mcp-token-toggle"));
    await vi.waitFor(() => expect(getByTestId("mcp-token").textContent).toContain("tok-123"));
  });

  it("再生成は確認の2段階で、確定すると新しいトークンを表示する", async () => {
    const spy = vi.spyOn(commands, "regenerateMcpToken").mockResolvedValue(ok("tok-new"));
    const { findByTestId, getByTestId, queryByTestId } = render(McpSection);
    await findByTestId("mcp-enabled");
    await fireEvent.click(getByTestId("mcp-regenerate"));
    expect(spy).not.toHaveBeenCalled();
    expect(queryByTestId("mcp-regenerate-confirm")).toBeTruthy();
    await fireEvent.click(getByTestId("mcp-regenerate-confirm"));
    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(getByTestId("mcp-token").textContent).toContain("tok-new"));
  });

  it("ポートが範囲外・空なら setMcpPort を呼ばず、日本語のエラーを出して元の値に戻す", async () => {
    const spy = vi.spyOn(commands, "setMcpPort").mockResolvedValue(ok(status({ port: 50000 })));
    const { findByTestId, getByTestId } = render(McpSection);
    const input = (await findByTestId("mcp-port")) as HTMLInputElement;
    for (const bad of ["80", "70000", ""]) {
      await fireEvent.input(input, { target: { value: bad } });
      await fireEvent.change(input);
      await vi.waitFor(() => expect(getByTestId("mcp-error").textContent).toContain("1024"));
      expect(input.value).toBe("49731");
    }
    expect(spy).not.toHaveBeenCalled();
  });

  it("初期ロードに失敗したら、理由を表示する", async () => {
    vi.mocked(commands.getMcpStatus).mockResolvedValue({ status: "error", error: "boom" });
    const { findByTestId } = render(McpSection);
    expect((await findByTestId("mcp-error")).textContent).toContain("boom");
  });

  it("バインド失敗のときだけ「再試行」を出し、押すと setMcpEnabled(true) を呼ぶ", async () => {
    const spy = vi
      .spyOn(commands, "setMcpEnabled")
      .mockResolvedValue(ok(status({ enabled: true, state: { type: "running" } })));
    vi.mocked(commands.getMcpStatus).mockResolvedValue(
      ok(status({ enabled: true, state: { type: "error", message: "port in use" } })),
    );
    const { findByTestId, getByTestId, queryByTestId } = render(McpSection);
    await fireEvent.click(await findByTestId("mcp-retry"));
    await vi.waitFor(() => expect(spy).toHaveBeenCalledWith(true));
    await vi.waitFor(() => expect(getByTestId("mcp-state").textContent).toContain("稼働中"));
    expect(queryByTestId("mcp-retry")).toBeNull();
  });

  it("停止中・稼働中のときは「再試行」を出さない", async () => {
    const { findByTestId, queryByTestId } = render(McpSection);
    await findByTestId("mcp-enabled");
    expect(queryByTestId("mcp-retry")).toBeNull();
  });
});
