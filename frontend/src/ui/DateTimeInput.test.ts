import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { tick } from "svelte";
import Harness from "./DateTimeInputHarness.svelte";

// UTC より進んだゾーンに固定する。CI が UTC でも、ローカル成分と UTC 成分が食い違う状況
// (0:30 JST は UTC では前日 15:30)を再現して Review Focus 1 を実効性のあるテストにする。
// (tsconfig.app.json に node の型が無いため globalThis 経由で代入する)
(globalThis as unknown as { process: { env: Record<string, string> } }).process.env.TZ = "Asia/Tokyo";

type Fp = {
  _flatpickr: {
    setDate(d: Date | Date[], t: boolean): void;
    selectedDates: Date[];
    config: { onChange: unknown[] };
  };
};
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
  it("value を外から変えると表示が追従し、onChange は発火せず value も変わらない", async () => {
    const { getByTestId, component } = render(Harness);
    const fp = fpOf(getByTestId("dt"));
    // 書き戻しは同じ文字列を書くだけで値からは検出できないため、flatpickr の onChange フックで観測する
    const spy = vi.fn();
    fp.config.onChange.push(spy);
    component.setValue("2026-10-08T09:30");
    await tick();
    expect((getByTestId("dt") as HTMLInputElement).value).toBe("2026-10-08 09:30");
    expect(getByTestId("bound").textContent).toBe("2026-10-08T09:30");
    expect(fp.selectedDates).toHaveLength(1);
    expect(spy).not.toHaveBeenCalled();
  });

  it("ユーザーが選んだ後、effect が setDate を呼び直さない(ループしない)", async () => {
    const { getByTestId } = render(Harness);
    const fp = fpOf(getByTestId("dt"));
    fp.setDate(new Date(2026, 4, 6, 7, 8), true);
    await tick();
    const spy = vi.spyOn(fp, "setDate");
    await tick();
    expect(spy).not.toHaveBeenCalled();
    expect(getByTestId("bound").textContent).toBe("2026-05-06T07:08");
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
