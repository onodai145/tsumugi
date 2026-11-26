import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet, tick } from "svelte";

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

const { default: LicenseConsentDialog } = await import("./LicenseConsentDialog.svelte");
const { default: Modal } = await import("../Modal.svelte");

afterEach(cleanup);

const base = { sizeBytes: 573_126_848, onAccept: vi.fn(), onCancel: vi.fn(), onOpenLicense: vi.fn() };

describe("LicenseConsentDialog", () => {
  it("networkWarning があれば、同意の文面の中に警告を出す", () => {
    const { getByTestId } = render(LicenseConsentDialog, { props: { ...base, networkWarning: "モバイル通信を使用中です。" } });
    expect(getByTestId("license-network-warning").textContent).toContain("モバイル通信を使用中です。");
    expect(getByTestId("license-dialog").textContent).toContain("モバイル通信を使用中です。");
  });

  it("networkWarning が無ければ、警告の領域を出さない", () => {
    const { queryByTestId } = render(LicenseConsentDialog, { props: base });
    expect(queryByTestId("license-network-warning")).toBeNull();
  });

  it("networkWarning が null でも、警告の領域を出さない", () => {
    const { queryByTestId } = render(LicenseConsentDialog, { props: { ...base, networkWarning: null } });
    expect(queryByTestId("license-network-warning")).toBeNull();
  });
});

describe("LicenseConsentDialog のフォーカスが外れた後の Escape(Issue #700)", () => {
  const body = createRawSnippet(() => ({ render: () => `<span>settings</span>` }));

  it("bodyに移った後のEscapeは、手前のダイアログだけをキャンセルする", async () => {
    const onclose = vi.fn();
    render(Modal, { props: { title: "親", onclose, children: body } });
    await tick();
    const onCancel = vi.fn();
    render(LicenseConsentDialog, { props: { ...base, onCancel } });
    await tick();
    (document.activeElement as HTMLElement).blur();
    expect(document.activeElement).toBe(document.body);
    await fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onclose).not.toHaveBeenCalled();
  });

  it("単体でも、bodyでのEscapeでキャンセルする", async () => {
    const onCancel = vi.fn();
    render(LicenseConsentDialog, { props: { ...base, onCancel } });
    await tick();
    (document.activeElement as HTMLElement).blur();
    expect(document.activeElement).toBe(document.body);
    await fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
