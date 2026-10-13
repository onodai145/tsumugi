<script lang="ts">
  import Modal from "./Modal.svelte";
  import ModalFocusChild from "./ModalFocusChild.test.host.svelte";

  let { compact = false }: { compact?: boolean } = $props();
</script>

<Modal title="t" onclose={() => {}} {compact}>
  {#snippet children()}
    <ModalFocusChild />
  {/snippet}
</Modal>
