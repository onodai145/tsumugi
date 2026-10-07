import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { tick } from "svelte";
import Harness from "./DateTimeInputHarness.svelte";

type Fp = { _flatpickr: { setDate(d: Date, t: boolean): void; selectedDates: Date[] } };
const fpOf = (el: HTMLElement) => (el as unknown as Fp)._flatpickr;

afterEach(() => cleanup());

describe("DateTimeInput", () => {
  it("ネイティブの datetime-local ではなく、読み取り専用のテキスト入力を使う", () => {
    const { getByTestId } = render(Harness);
    const input = getByTestId("dt") as HTMLInputElement;
    expect(input.type).toBe("text");
    expect(input.readOnly).toBe(true);
  });

  it("初期値を Y-m-d H:i で表示する", () => {
    const { getByTestId } = render(Harness, { props: { initial: "2026-03-04T05:06" } });
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("2026-03-04 05:06");
  });

  // Review Focus 1: UTC ではなくローカルの成分で value を作る(0 時台は UTC 変換すると前日にずれる)
  it("選んだ日時をローカルタイムゾーンの YYYY-MM-DDTHH:mm で value に書き戻す", async () => {
    const { getByTestId } = render(Harness);
    fpOf(getByTestId("dt")).setDate(new Date(2026, 0, 1, 0, 30), true);
    await tick();
    expect(getByTestId("bound").textContent).toBe("2026-01-01T00:30");
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("2026-01-01 00:30");
  });

  // Review Focus 2: 外からの更新は表示にだけ反映し、書き戻しやループを起こさない
  it("value を外から変えると表示が追従し、value は変わらない", async () => {
    const { getByTestId, component } = render(Harness);
    component.setValue("2026-10-08T09:30");
    await tick();
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("2026-10-08 09:30");
    expect(getByTestId("bound").textContent).toBe("2026-10-08T09:30");
    expect(fpOf(getByTestId("dt")).selectedDates).toHaveLength(1);
  });

  // Review Focus 3
  it("value を空にすると表示も空になる", async () => {
    const { getByTestId, component } = render(Harness, { props: { initial: "2026-03-04T05:06" } });
    component.setValue("");
    await tick();
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("");
    expect(fpOf(getByTestId("dt")).selectedDates).toHaveLength(0);
    expect(getByTestId("bound").textContent).toBe("");
  });

  it("アンマウントでカレンダー要素が body から消える", () => {
    const { unmount } = render(Harness);
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(1);
    unmount();
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(0);
  });
});
