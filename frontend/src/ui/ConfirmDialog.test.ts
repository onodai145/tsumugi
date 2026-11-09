import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import ConfirmDialog from "./ConfirmDialog.svelte";

afterEach(cleanup);

describe("ConfirmDialog のフォーカス(Issue #570)", () => {
  it("開いた時にダイアログ内(キャンセルボタン)へフォーカスする", () => {
    const { getByText } = render(ConfirmDialog, {
      props: { message: "削除しますか?", onConfirm: vi.fn(), onCancel: vi.fn() },
    });
    expect(document.activeElement).toBe(getByText("キャンセル").closest("button"));
  });

  it("開いた直後にクリックせずとも Escape でキャンセルできる", async () => {
    const onCancel = vi.fn();
    render(ConfirmDialog, { props: { message: "削除しますか?", onConfirm: vi.fn(), onCancel } });
    await fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
