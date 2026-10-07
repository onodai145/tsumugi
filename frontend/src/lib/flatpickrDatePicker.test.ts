import { afterEach, describe, expect, it, vi } from "vitest";
import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";
import { datePicker } from "./flatpickrDatePicker";

function mount(
  over: {
    defaultHour?: number;
    defaultMinute?: number;
    onValueUpdate?: (d: Date | null) => void;
  } = {},
) {
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

const ANDROID_UA = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/130.0 Mobile Safari/537.36";
const originalUA = Object.getOwnPropertyDescriptor(navigator, "userAgent");
function stubUserAgent(ua: string) {
  Object.defineProperty(navigator, "userAgent", { value: ua, configurable: true });
}

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  // navigator 自身に定義したスタブを外し、jsdom 本来の getter(prototype 側)に戻す
  if (originalUA) Object.defineProperty(navigator, "userAgent", originalUA);
  else delete (navigator as unknown as Record<string, unknown>).userAgent;
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

  // flatpickr は UA に Android を含むと isMobile=true で build() を飛ばすが onReady は呼ぶ。
  // ヘッダ入れ替えの onReady が currentYearElement(未生成)を触ると例外になり、インスタンスが [] になる。
  it("Android(isMobile)でも onCreate に本物のインスタンスが渡り、console.error も出ない", () => {
    stubUserAgent(ANDROID_UA);
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { fp, action } = mount();
    expect(Array.isArray(fp)).toBe(false);
    expect(fp.isMobile).toBe(true);
    expect(Array.isArray(fp.selectedDates)).toBe(true);
    expect(typeof fp.setDate).toBe("function");
    expect(errorSpy).not.toHaveBeenCalled();
    expect(() => action.destroy()).not.toThrow();
  });

  it("onValueUpdate は setDate(d, true) と時刻欄の変更で Date が渡り、setDate(d, false) では呼ばれない", () => {
    const onValueUpdate = vi.fn();
    const { fp } = mount({ onValueUpdate });
    const d = new Date(2026, 4, 6, 9, 0);
    fp.setDate(d, false);
    expect(onValueUpdate).not.toHaveBeenCalled();
    fp.setDate(d, true);
    expect(onValueUpdate).toHaveBeenLastCalledWith(d);
    // 時刻欄の手入力: onChange は 300ms デバウンスされるが onValueUpdate は blur で同期的に呼ばれる
    onValueUpdate.mockClear();
    fp.minuteElement!.value = "45";
    fp.minuteElement!.dispatchEvent(new Event("blur"));
    expect(onValueUpdate).toHaveBeenCalledTimes(1);
    const got = onValueUpdate.mock.calls[0][0] as Date;
    expect(got.getHours()).toBe(9);
    expect(got.getMinutes()).toBe(45);
  });
});
