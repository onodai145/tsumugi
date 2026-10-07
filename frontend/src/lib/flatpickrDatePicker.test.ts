import { afterEach, describe, expect, it, vi } from "vitest";
import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";
import { datePicker } from "./flatpickrDatePicker";

function mount(over: { defaultHour?: number; defaultMinute?: number } = {}) {
  const node = document.createElement("input");
  document.body.appendChild(node);
  let fp!: FlatpickrInstance;
  const onChange = vi.fn();
  const action = datePicker(node, {
    defaultHour: 0,
    onChange,
    onCreate: (f) => (fp = f),
    ...over,
  });
  return { node, fp, onChange, action };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("datePicker", () => {
  it("onCreate でインスタンスを渡し、24時間制・日本語・Y-m-d H:i で作る", () => {
    const { fp } = mount();
    expect(fp.config.enableTime).toBe(true);
    expect(fp.config.time_24hr).toBe(true);
    expect(fp.config.dateFormat).toBe("Y-m-d H:i");
    expect(fp.l10n.weekdays.shorthand[0]).toBe("日");
  });

  it("defaultMinute 未指定は、defaultHour が 0 なら 0、それ以外は 59(検索の開始/終了の既存規則)", () => {
    expect(mount({ defaultHour: 0 }).fp.config.defaultMinute).toBe(0);
    expect(mount({ defaultHour: 23 }).fp.config.defaultMinute).toBe(59);
  });

  it("defaultMinute を指定するとその値になる", () => {
    const { fp } = mount({ defaultHour: 9, defaultMinute: 30 });
    expect(fp.config.defaultHour).toBe(9);
    expect(fp.config.defaultMinute).toBe(30);
  });

  it("日付を選ぶと onChange に Date が、クリアすると null が渡る", () => {
    const { fp, onChange } = mount();
    const d = new Date(2026, 0, 2, 3, 4);
    fp.setDate(d, true);
    expect(onChange).toHaveBeenLastCalledWith(d);
    fp.clear();
    expect(onChange).toHaveBeenLastCalledWith(null);
  });

  it("destroy でカレンダー要素が body から消える", () => {
    const { action } = mount();
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(1);
    action.destroy();
    expect(document.querySelectorAll(".flatpickr-calendar").length).toBe(0);
  });
});
