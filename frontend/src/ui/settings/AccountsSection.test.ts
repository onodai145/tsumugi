import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import type { Account } from "../../bindings/tauri.gen";
import { app } from "../../lib/store.svelte";

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

const { commands } = await import("../../lib/ipc");
const { default: AccountsSection } = await import("./AccountsSection.svelte");

function account(id: string, username: string): Account {
  return {
    id,
    host: "misskey.test",
    username,
    userId: `u-${id}`,
    displayName: username,
    avatarUrl: null,
    instance: null,
    isCat: false,
    avatarBlurhash: null,
  };
}

beforeEach(() => {
  app.accounts = [account("a1", "alice"), account("a2", "bob")];
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.accounts = [];
});

describe("AccountsSection: 要ログイン(Issue #544)", () => {
  it("トークンを持たないアカウントの行にだけ「要ログイン」を出す", async () => {
    vi.spyOn(commands, "accountsWithoutToken").mockResolvedValue({ status: "ok", data: ["a2"] } as never);
    const { container } = render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    await vi.waitFor(() => expect(screen.getAllByText("要ログイン")).toHaveLength(1));
    const row = (id: string) => container.querySelector(`[data-account-row-id="${id}"]`)!;
    expect(row("a2").textContent).toContain("要ログイン");
    expect(row("a1").textContent).not.toContain("要ログイン");
  });

  it("全員がトークンを持っていれば、バッジは出ない", async () => {
    vi.spyOn(commands, "accountsWithoutToken").mockResolvedValue({ status: "ok", data: [] } as never);
    render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    await vi.waitFor(() => expect(commands.accountsWithoutToken).toHaveBeenCalled());
    expect(screen.queryByText("要ログイン")).toBeNull();
  });

  it("取得に失敗してもアカウント一覧は表示する", async () => {
    vi.spyOn(commands, "accountsWithoutToken").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "boom" },
    } as never);
    render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    await vi.waitFor(() => expect(commands.accountsWithoutToken).toHaveBeenCalled());
    expect(screen.getByText("@alice@misskey.test")).toBeTruthy();
    expect(screen.queryByText("要ログイン")).toBeNull();
  });
});
