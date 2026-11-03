import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { ImportPreview } from "../../bindings/tauri.gen";

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
const { commands } = await import("../../lib/ipc");
const { default: SettingsTransfer } = await import("./SettingsTransfer.svelte");

const preview: ImportPreview = {
  columnCount: 3,
  accountCount: 2,
  accountsNeedingLogin: 1,
  hasCustomCss: true,
  hasCustomThemes: false,
  hasCustomEndpoints: false,
};

let reload: Mock<() => void>;

beforeEach(() => {
  reload = vi.fn<() => void>();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(dialog.open).mockReset();
  vi.mocked(dialog.save).mockReset();
});

describe("SettingsTransfer: エクスポート", () => {
  it("保存先を選ぶと書き出し、完了を表示する", async () => {
    vi.mocked(dialog.save).mockResolvedValue("/tmp/out.json");
    const spy = vi.spyOn(commands, "exportSettings").mockResolvedValue({ status: "ok", data: null } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("エクスポート…"));

    await vi.waitFor(() => expect(screen.getByText("書き出しました")).toBeTruthy());
    expect(spy).toHaveBeenCalledWith("/tmp/out.json");
    const options = vi.mocked(dialog.save).mock.calls[0][0]!;
    expect(options.defaultPath).toMatch(/^tsumugi-settings-\d{8}\.json$/);
  });

  it("保存先を選ばずに閉じたら、何もしない", async () => {
    vi.mocked(dialog.save).mockResolvedValue(null);
    const spy = vi.spyOn(commands, "exportSettings");
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("エクスポート…"));

    await vi.waitFor(() => expect(dialog.save).toHaveBeenCalled());
    expect(spy).not.toHaveBeenCalled();
  });

  it("書き出しに失敗したらエラーを表示する", async () => {
    vi.mocked(dialog.save).mockResolvedValue("/tmp/out.json");
    vi.spyOn(commands, "exportSettings").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "cannot write file" },
    } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("エクスポート…"));

    await vi.waitFor(() => expect(screen.getByText(/cannot write file/)).toBeTruthy());
  });
});

describe("SettingsTransfer: インポート", () => {
  it("検証の概要を確認ダイアログに出し、承認すると取り込んでリロードする", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    const importSpy = vi
      .spyOn(commands, "importSettings")
      .mockResolvedValue({ status: "ok", data: { columnCount: 3, accountsNeedingLogin: ["a2"] } } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(screen.getByText(/カラム3件・アカウント2件/)).toBeTruthy());
    expect(screen.getByText(/1件のアカウントは、取り込み後に再認証が必要/)).toBeTruthy();
    expect(screen.getByText(/カスタムCSSを含みます/)).toBeTruthy();
    expect(screen.getByText(/現在の設定とカラムは置き換えられます/)).toBeTruthy();
    expect(importSpy).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByText("インポート"));

    await vi.waitFor(() => expect(reload).toHaveBeenCalledTimes(1));
    expect(importSpy).toHaveBeenCalledWith("/tmp/in.json");
  });

  it("送信先URLが初期値から変えられているときは、確認ダイアログで知らせる", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({
      status: "ok",
      data: { ...preview, hasCustomEndpoints: true },
    } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() =>
      expect(screen.getByText(/検索エンジンまたはURLプレビューの送信先URLが、初期値から変更されています/)).toBeTruthy(),
    );
  });

  it("送信先URLが初期値のままなら、その注意書きは出さない", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(screen.getByText(/カラム3件/)).toBeTruthy());
    expect(screen.queryByText(/送信先URL/)).toBeNull();
  });

  it("確認でキャンセルしたら、取り込まない", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    const importSpy = vi.spyOn(commands, "importSettings");
    render(SettingsTransfer, { props: { reload } });
    await fireEvent.click(screen.getByText("インポート…"));
    await vi.waitFor(() => expect(screen.getByText(/カラム3件/)).toBeTruthy());

    await fireEvent.click(screen.getByText("キャンセル"));

    expect(importSpy).not.toHaveBeenCalled();
    expect(reload).not.toHaveBeenCalled();
    expect(screen.queryByText(/カラム3件/)).toBeNull();
  });

  it("ファイルを選ばずに閉じたら、検証しない", async () => {
    vi.mocked(dialog.open).mockResolvedValue(null);
    const inspect = vi.spyOn(commands, "inspectSettingsFile");
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(dialog.open).toHaveBeenCalled());
    expect(inspect).not.toHaveBeenCalled();
  });

  it("検証に落ちたらエラーを表示し、確認ダイアログは出さない", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/bad.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "tsumugiの設定ファイルではありません" },
    } as never);
    render(SettingsTransfer, { props: { reload } });

    await fireEvent.click(screen.getByText("インポート…"));

    await vi.waitFor(() => expect(screen.getByText(/tsumugiの設定ファイルではありません/)).toBeTruthy());
    expect(screen.queryByText(/現在の設定とカラムは置き換えられます/)).toBeNull();
  });

  it("取り込みに失敗したらリロードせず、エラーを表示する", async () => {
    vi.mocked(dialog.open).mockResolvedValue("/tmp/in.json");
    vi.spyOn(commands, "inspectSettingsFile").mockResolvedValue({ status: "ok", data: preview } as never);
    vi.spyOn(commands, "importSettings").mockResolvedValue({
      status: "error",
      error: { kind: "invalid", message: "disk full" },
    } as never);
    render(SettingsTransfer, { props: { reload } });
    await fireEvent.click(screen.getByText("インポート…"));
    await vi.waitFor(() => expect(screen.getByText(/カラム3件/)).toBeTruthy());

    await fireEvent.click(screen.getByText("インポート"));

    await vi.waitFor(() => expect(screen.getByText(/disk full/)).toBeTruthy());
    expect(reload).not.toHaveBeenCalled();
  });
});
