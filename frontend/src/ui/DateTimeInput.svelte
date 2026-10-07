<script lang="ts">
  // 日時入力。ネイティブの <input type="datetime-local"> は WebKitGTK で時刻を操作できない
  // (Issue #430 で診断済み)ため、flatpickr(自前描画)で置き換える。
  // value は datetime-local と同じ "YYYY-MM-DDTHH:mm"(ローカルタイムゾーン)、空は ""。
  import type { Instance as FlatpickrInstance } from "flatpickr/dist/types/instance";
  import { datePicker } from "../lib/flatpickrDatePicker";
  import { epochSecToLocalInput } from "../lib/schedule";

  let {
    value = $bindable(""),
    defaultHour = 9,
    defaultMinute = 0,
    placeholder = "",
    class: className = "",
    disabled = false,
    "data-testid": testid,
  }: {
    value?: string;
    defaultHour?: number;
    defaultMinute?: number;
    placeholder?: string;
    class?: string;
    disabled?: boolean;
    "data-testid"?: string;
  } = $props();

  let fp: FlatpickrInstance | undefined;

  const format = (d: Date) => epochSecToLocalInput(Math.floor(d.getTime() / 1000));
  const shownValue = () => (fp?.selectedDates[0] ? format(fp.selectedDates[0]) : "");

  // 外から value が変わったときだけ、表示を合わせる。setDate(.., false) は onChange を発火させないので
  // value を書き戻さない。表示と同じ値なら何もしない(ループ・二重更新を避ける)。
  $effect(() => {
    const v = value;
    if (!fp || v === shownValue()) return;
    fp.setDate(v ? new Date(v) : [], false);
  });

  function onCreate(f: FlatpickrInstance) {
    fp = f;
    if (value) f.setDate(new Date(value), false);
  }
</script>

<input
  type="text"
  readonly
  {placeholder}
  {disabled}
  class={className}
  data-testid={testid}
  use:datePicker={{
    defaultHour,
    defaultMinute,
    onChange: (d) => (value = d ? format(d) : ""),
    onCreate,
  }}
/>
