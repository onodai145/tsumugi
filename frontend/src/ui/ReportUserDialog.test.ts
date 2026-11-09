import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import ReportUserDialog from "./ReportUserDialog.svelte";

afterEach(() => cleanup());

function mount(onsubmit = vi.fn().mockResolvedValue(undefined)) {
  const oncancel = vi.fn();
  const r = render(ReportUserDialog, { props: { acct: "@alice@remote.example", onsubmit, oncancel } });
  return { ...r, onsubmit, oncancel };
}

describe("ReportUserDialog", () => {
  it("対象のacctを表示する", () => {
    const { getByText } = mount();
    expect(getByText(/@alice@remote\.example/)).toBeTruthy();
  });

  it("コメントが空(空白のみを含む)の間は、送信ボタンを押せない", async () => {
    const { getByRole, getByLabelText } = mount();
    const send = getByRole("button", { name: "通報する" }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);

    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "   \n" } });
    expect(send.disabled).toBe(true);

    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "スパムです" } });
    expect(send.disabled).toBe(false);
  });

  it("送信するとonsubmitにコメントを渡す", async () => {
    const { getByRole, getByLabelText, onsubmit } = mount();
    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "スパムです" } });

    await fireEvent.click(getByRole("button", { name: "通報する" }));

    expect(onsubmit).toHaveBeenCalledWith("スパムです");
  });

  it("送信中は二重に送信できない", async () => {
    let resolve!: () => void;
    const onsubmit = vi.fn(() => new Promise<void>((r) => (resolve = r)));
    const { getByRole, getByLabelText } = mount(onsubmit);
    await fireEvent.input(getByLabelText("通報の内容"), { target: { value: "スパムです" } });
    const send = getByRole("button", { name: "通報する" }) as HTMLButtonElement;

    await fireEvent.click(send);
    await fireEvent.click(send);

    expect(onsubmit).toHaveBeenCalledTimes(1);
    expect(send.disabled).toBe(true);
    resolve();
  });

  it("失敗したら、エラーを出し、入力を残し、もう一度送信できる", async () => {
    const onsubmit = vi.fn().mockRejectedValueOnce(new Error("users/report-abuse: CANNOT_REPORT_THE_ADMIN")).mockResolvedValueOnce(undefined);
    const { getByRole, getByLabelText, findByText } = mount(onsubmit);
    const textarea = getByLabelText("通報の内容") as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "スパムです" } });

    await fireEvent.click(getByRole("button", { name: "通報する" }));

    expect(await findByText(/CANNOT_REPORT_THE_ADMIN/)).toBeTruthy();
    expect(textarea.value).toBe("スパムです");
    await waitFor(() => expect((getByRole("button", { name: "通報する" }) as HTMLButtonElement).disabled).toBe(false));

    await fireEvent.click(getByRole("button", { name: "通報する" }));
    expect(onsubmit).toHaveBeenCalledTimes(2);
  });

  it("コメントは2048文字までに制限する(maxlength)", () => {
    const { getByLabelText } = mount();
    expect((getByLabelText("通報の内容") as HTMLTextAreaElement).maxLength).toBe(2048);
  });

  it("キャンセルでoncancelを呼ぶ", async () => {
    const { getByRole, oncancel } = mount();
    await fireEvent.click(getByRole("button", { name: "キャンセル" }));
    expect(oncancel).toHaveBeenCalledTimes(1);
  });
});
