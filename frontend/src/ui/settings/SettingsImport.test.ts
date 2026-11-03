import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const dialog = await import("@tauri-apps/plugin-dialog");
const { commands } = await import("../../lib/ipc");
const { default: SettingsImport } = await import("./SettingsImport.svelte");

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(dialog.open).mockReset();
});

describe("SettingsImport", () => {
  it("失敗時はメッセージ、新しい試行の開始時はnullをonerrorに渡す", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/bad.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "bad file" },
    } as never);
    const onerror = vi.fn();
    render(SettingsImport, { props: { onerror } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(onerror).toHaveBeenCalledTimes(2));
    expect(onerror.mock.calls[0][0]).toBeNull();
    expect(String(onerror.mock.calls[1][0])).toContain("bad file");
  });

  it("disabled のときはボタンを押せない", () => {
    render(SettingsImport, { props: { disabled: true } });
    expect((screen.getByText("インポート…") as HTMLButtonElement).disabled).toBe(true);
  });
});
