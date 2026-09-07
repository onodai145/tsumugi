<script lang="ts">
  import Avatar from "./Avatar.svelte";
  let { isCat }: { isCat: boolean } = $props();
</script>

<Avatar {isCat} class="h-8 w-8">
  <img data-testid="inner-img" src="https://example.com/a.png" alt="" />
</Avatar>
