import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { Account, ImportPreview } from "../bindings/tauri.gen";
import { app } from "../lib/store.svelte";

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

const dialog = await import("@tauri-apps/plugin-dialog");
const { commands } = await import("../lib/ipc");
const { default: AddAccount } = await import("./AddAccount.svelte");

const preview: ImportPreview = {
  columnCount: 3,
  accountCount: 2,
  accountsNeedingLogin: 2,
  hasCustomCss: false,
  hasCustomThemes: false,
  hasCustomEndpoints: false,
};

function account(id: string): Account {
  return {
    id,
    host: "misskey.test",
    username: id,
    userId: `u-${id}`,
    displayName: id,
    avatarUrl: null,
    instance: null,
    isCat: false,
    avatarBlurhash: null,
  };
}

let reload: Mock<() => void>;

beforeEach(() => {
  app.accounts = [];
  reload = vi.fn<() => void>();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(dialog.open).mockReset();
  app.accounts = [];
  app.needsLogin = new Set();
});

describe("AddAccount: 設定のインポート導線(Issue #544)", () => {
  it("アカウント0件の通常表示では、インポートボタンを出す", () => {
    render(AddAccount);
    expect(screen.getByText("インポート…")).toBeTruthy();
  });

  it("アカウントが既にあるときは出さない", () => {
    app.accounts = [account("a1")];
    render(AddAccount);
    expect(screen.queryByText("インポート…")).toBeNull();
  });

  it("再認証フローでは出さない", () => {
    vi.spyOn(app, "addAccount").mockResolvedValue("session");
    render(AddAccount, { props: { reauthAccount: { id: "a1", host: "misskey.test", username: "alice" } } });
    expect(screen.queryByText("インポート…")).toBeNull();
  });

  it("検証に失敗したらエラーを表示し、取り込まない", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/bad.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "tsumugiの設定ファイルではありません" },
    } as never);
    const importSpy = vi.spyOn(commands, "importSettings");
    render(AddAccount, { props: { reloadAfterImport: reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() =>
      expect(screen.getByTestId("add-account-import-error").textContent).toContain(
        "tsumugiの設定ファイルではありません",
      ),
    );
    expect(importSpy).not.toHaveBeenCalled();
    expect(reload).not.toHaveBeenCalled();
  });

  it("確認して取り込むと、importSettingsを呼んでリロードする", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    const importSpy = vi
      .spyOn(commands, "importSettings")
      .mockResolvedValue({ status: "ok", data: { columnCount: 3, accountsNeedingLogin: ["a1", "a2"] } } as never);
    render(AddAccount, { props: { reloadAfterImport: reload } });

    await fireEvent.click(screen.getByText("インポート…"));
    await vi.waitFor(() => expect(screen.getByText(/カラム3件・アカウント2件/)).toBeTruthy());
    expect(importSpy).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByText("インポート"));

    await vi.waitFor(() => expect(reload).toHaveBeenCalledTimes(1));
    expect(importSpy).toHaveBeenCalledWith("/tmp/in.json");
  });
});

describe("AddAccount: 再認証画面の文言(Issue #553)", () => {
  const reauth = { id: "a1", host: "misskey.test", username: "alice" };

  it("トークンなしのアカウントでは、インポート後の文脈の文言を出す", async () => {
    vi.spyOn(app, "addAccount").mockResolvedValue("session");
    app.needsLogin = new Set(["a1"]);

    render(AddAccount, { props: { reauthAccount: reauth } });

    await vi.waitFor(() => expect(screen.getByText(/まだログインしていません/)).toBeTruthy());
    expect(screen.queryByText(/スコープが更新/)).toBeNull();
  });

  it("トークンがあるアカウントの再認証では、スコープ更新の文言を出す", async () => {
    vi.spyOn(app, "addAccount").mockResolvedValue("session");
    app.needsLogin = new Set();

    render(AddAccount, { props: { reauthAccount: reauth } });

    await vi.waitFor(() => expect(screen.getByText(/スコープが更新されたトークンを取得します/)).toBeTruthy());
    expect(screen.queryByText(/まだログインしていません/)).toBeNull();
  });
});
