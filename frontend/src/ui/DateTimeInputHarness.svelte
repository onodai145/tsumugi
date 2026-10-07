<script lang="ts">
  // DateTimeInput.test.ts 用のホスト。bind:value の書き戻しを <output> で観測し、
  // setValue() で外からの更新(予約成功後の消去・復元など)を再現する。
  import DateTimeInput from "./DateTimeInput.svelte";

  let {
    initial = "",
    defaultHour,
    defaultMinute,
  }: { initial?: string; defaultHour?: number; defaultMinute?: number } = $props();

  // 初期値としてだけ使う(以後の更新は setValue 経由)ため、意図的に初回の値を捕まえる。
  // svelte-ignore state_referenced_locally
  let value = $state(initial);
  export function setValue(v: string) {
    value = v;
  }
</script>

<DateTimeInput bind:value {defaultHour} {defaultMinute} data-testid="dt" />
<output data-testid="bound">{value}</output>
