<script lang="ts">
  // ComposeBar と同様、マウント時の $effect で入力欄へフォーカスする子(返信・共有の事前入力)
  let el = $state<HTMLInputElement | null>(null);
  $effect(() => {
    el?.focus();
  });
</script>

<input bind:this={el} data-testid="child-input" />
