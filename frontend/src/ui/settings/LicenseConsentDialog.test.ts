import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";

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
