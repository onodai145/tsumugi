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
  app.needsLogin = new Set();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.accounts = [];
  app.needsLogin = new Set();
});

describe("AccountsSection: 要ログイン(Issue #544/#553)", () => {
  it("トークンを持たないアカウント(app.needsLogin)の行にだけ「要ログイン」を出す", () => {
    app.needsLogin = new Set(["a2"]);
    const { container } = render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    expect(screen.getAllByText("要ログイン")).toHaveLength(1);
    const row = (id: string) => container.querySelector(`[data-account-row-id="${id}"]`)!;
    expect(row("a2").textContent).toContain("要ログイン");
    expect(row("a1").textContent).not.toContain("要ログイン");
  });

  it("全員がトークンを持っていれば、バッジは出ない", () => {
    render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    expect(screen.getByText("@alice@misskey.test")).toBeTruthy();
    expect(screen.queryByText("要ログイン")).toBeNull();
  });

  it("再認証が済んで app.needsLogin から外れたら、バッジが消える(バナーと食い違わない)", async () => {
    app.needsLogin = new Set(["a2"]);
    render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });
    expect(screen.getAllByText("要ログイン")).toHaveLength(1);

    app.needsLogin = new Set();

    await vi.waitFor(() => expect(screen.queryByText("要ログイン")).toBeNull());
  });

  it("要ログインの情報源は app.needsLogin だけで、自分では取り直さない", () => {
    const spy = vi.spyOn(commands, "accountsWithoutToken");
    render(AccountsSection, { props: { onAddAccount: vi.fn(), onReauth: vi.fn() } });

    expect(spy).not.toHaveBeenCalled();
  });
});
