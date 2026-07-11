<script lang="ts">
  import NotifySection from "./settings/NotifySection.svelte";
  import MuteSection from "./settings/MuteSection.svelte";
  import DisplaySection from "./settings/DisplaySection.svelte";
  import AccountsSection from "./settings/AccountsSection.svelte";
  import KeysSection from "./settings/KeysSection.svelte";
  import AboutSection from "./settings/AboutSection.svelte";
  import { X } from "@lucide/svelte";

  type Section = "accounts" | "display" | "notify" | "mute" | "keys" | "about";

  let {
    onclose,
    onAddAccount,
    initial = "notify",
  }: { onclose: () => void; onAddAccount: () => void; initial?: Section } = $props();

  const nav: { id: Section; label: string }[] = [
    { id: "accounts", label: "アカウント" },
    { id: "display", label: "表示" },
    { id: "notify", label: "通知" },
    { id: "mute", label: "NG（ミュート）" },
    { id: "keys", label: "キー操作" },
    { id: "about", label: "Tsumugiについて" },
  ];

  // initial は開いた時点の初期タブのみ。モーダルは開くたび再生成されるので初期値参照でよい。
  // svelte-ignore state_referenced_locally
  let active = $state<Section>(initial);
</script>

<div class="overlay" onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="-1">
    <header class="head">
      <span>設定</span>
      <button class="x" onclick={onclose}><X size={16} /></button>
    </header>

    <div class="body">
      <nav class="side">
        {#each nav as item (item.id)}
          <button class="nav-item" class:active={active === item.id} onclick={() => (active = item.id)}>
            {item.label}
          </button>
        {/each}
      </nav>
      <section class="pane">
        {#if active === "accounts"}
          <AccountsSection {onAddAccount} />
        {:else if active === "display"}
          <DisplaySection />
        {:else if active === "notify"}
          <NotifySection />
        {:else if active === "mute"}
          <MuteSection />
        {:else if active === "keys"}
          <KeysSection />
        {:else if active === "about"}
          <AboutSection />
        {/if}
      </section>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: start center;
    padding-top: 8vh;
    z-index: 50;
  }
  .modal {
    width: min(640px, 94vw);
    max-height: 84vh;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: 12px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  .x {
    display: inline-flex;
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .side {
    flex: none;
    width: 160px;
    border-right: 1px solid var(--border);
    padding: 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--surface-2);
  }
  .nav-item {
    text-align: left;
    padding: 8px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    font-size: 0.85rem;
  }
  .nav-item:hover {
    background: var(--surface-1);
  }
  .nav-item.active {
    background: var(--accent);
    color: #fff;
  }
  .pane {
    flex: 1;
    min-width: 0;
    padding: 18px 20px;
    overflow-y: auto;
  }
</style>
