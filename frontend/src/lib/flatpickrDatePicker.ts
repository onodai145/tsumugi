// flatpickr(自前描画のカレンダー)を素の input に被せるアクション。
// WebKitGTK のネイティブ datetime-local/date/time は UI が未成熟(時刻を操作できない等)なため、
// 日時入力はネイティブ input ではなくこれを使う(Issue #430, #60)。
import flatpickr from "flatpickr";
import "flatpickr/dist/flatpickr.min.css";
import "./flatpickrTheme.css"; // flatpickr.min.css より後に読み込む(同じ詳細度のルールを上書きするため)
import { Japanese } from "flatpickr/dist/l10n/ja.js";
import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";

export type DatePickerOptions = {
  /// 日付だけ選んで時刻を触らなかったときの時。
  defaultHour: number;
  /// 同じく分。未指定は、defaultHour が 0 なら 0、それ以外は 59(検索の開始=0:00、終了=その日の終わり)。
  defaultMinute?: number;
  onChange: (d: Date | null) => void;
  /// fp インスタンスは、クリアボタン(fp.clear())や値の外部更新(fp.setDate())から使えるよう返す。
  onCreate: (fp: FlatpickrInstance) => void;
};

export function datePicker(node: HTMLInputElement, opts: DatePickerOptions) {
  const fp: FlatpickrInstance = flatpickr(node, {
    enableTime: true,
    time_24hr: true,
    dateFormat: "Y-m-d H:i",
    locale: Japanese,
    defaultHour: opts.defaultHour,
    defaultMinute: opts.defaultMinute ?? (opts.defaultHour === 0 ? 0 : 59),
    onChange: (dates) => opts.onChange(dates[0] ?? null),
    // flatpickr本体はヘッダを常に「月セレクト→年input」の順でDOM生成し、これを入れ替える
    // 設定は無い。buildMonths()自体は初期化時に1度しか呼ばれない(月送りは値の更新のみ)
    // ので、onReadyで年側のwrapperを月セレクトの前に差し替えれば以後も維持される。
    onReady: (_selectedDates, _dateStr, instance) => {
      const yearWrapper = instance.currentYearElement.closest(".numInputWrapper");
      const monthSelect = instance.monthsDropdownContainer;
      if (yearWrapper && monthSelect?.parentElement) {
        monthSelect.parentElement.insertBefore(yearWrapper, monthSelect);
      }
    },
  });
  opts.onCreate(fp);
  return {
    destroy() {
      fp.destroy();
    },
  };
}
