<script lang="ts">
  import type { Account } from "../bindings/tauri.gen";
  import NotifySection from "./settings/NotifySection.svelte";
  import MuteSection from "./settings/MuteSection.svelte";
  import DisplaySection from "./settings/DisplaySection.svelte";
  import ReactionSection from "./settings/ReactionSection.svelte";
  import DataSection from "./settings/DataSection.svelte";
  import AccountsSection from "./settings/AccountsSection.svelte";
  import KeysSection from "./settings/KeysSection.svelte";
  import AboutSection from "./settings/AboutSection.svelte";
  import Modal from "./Modal.svelte";

  type Section = "accounts" | "display" | "reaction" | "data" | "notify" | "mute" | "keys" | "about";

  let {
    onclose,
    onAddAccount,
    onReauth,
    initial = "notify",
  }: {
    onclose: () => void;
    onAddAccount: () => void;
    onReauth: (account: Account) => void;
    initial?: Section;
  } = $props();

  const nav: { id: Section; label: string }[] = [
    { id: "accounts", label: "アカウント" },
    { id: "display", label: "表示" },
    { id: "reaction", label: "リアクション" },
    { id: "data", label: "データ" },
    { id: "notify", label: "通知" },
    { id: "mute", label: "NG（ミュート）" },
    { id: "keys", label: "キー操作" },
    { id: "about", label: "Tsumugiについて" },
  ];

  // initial は開いた時点の初期タブのみ。モーダルは開くたび再生成されるので初期値参照でよい。
  // svelte-ignore state_referenced_locally
  let active = $state<Section>(initial);
</script>

<Modal title="設定" {onclose} width="640px">
  <!-- rounded-b-[11px]はModal.svelteのrounded-xl(12px) - border(1px)を差し引いた値
       （このdivはModalの内側にネガティブマージンで縁までにじみ出すため、外側の角丸に沿わせる必要がある）。
       スタイルガイド(docs/design/style-guide.md §2)が認める「親要素のborder-width分を差し引く」例外。 -->
  <div class="-mx-4 -mb-4 flex max-h-[calc(84vh-3rem)] flex-col overflow-hidden rounded-b-[11px]">
    <div class="flex min-h-0 flex-1 border-t border-border">
      <nav class="flex w-40 flex-none flex-col gap-0.5 overflow-y-auto border-r border-border bg-muted px-2 py-2.5">
        {#each nav as item (item.id)}
          <button
            type="button"
            class={active === item.id
              ? "rounded-md bg-primary px-2.5 py-2 text-left text-sm text-primary-foreground"
              : "rounded-md px-2.5 py-2 text-left text-sm text-foreground hover:bg-background"}
            onclick={() => (active = item.id)}
          >
            {item.label}
          </button>
        {/each}
      </nav>
      <section class="min-w-0 flex-1 overflow-y-auto px-5 py-[18px]">
        {#if active === "accounts"}
          <AccountsSection {onAddAccount} {onReauth} />
        {:else if active === "display"}
          <DisplaySection />
        {:else if active === "reaction"}
          <ReactionSection />
        {:else if active === "data"}
          <DataSection />
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
</Modal>
