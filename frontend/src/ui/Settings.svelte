<script lang="ts">
  import type { Account } from "../bindings/tauri.gen";
  import NotifySection from "./settings/NotifySection.svelte";
  import MuteSection from "./settings/MuteSection.svelte";
  import LayoutSection from "./settings/LayoutSection.svelte";
  import MobileSection from "./settings/MobileSection.svelte";
  import AppearanceSection from "./settings/AppearanceSection.svelte";
  import BackgroundSection from "./settings/BackgroundSection.svelte";
  import ReactionSection from "./settings/ReactionSection.svelte";
  import ExternalIntegrationSection from "./settings/ExternalIntegrationSection.svelte";
  import DataSection from "./settings/DataSection.svelte";
  import CacheBackendSettings from "./settings/CacheBackendSettings.svelte";
  import DeveloperSection from "./settings/DeveloperSection.svelte";
  import AccountsSection from "./settings/AccountsSection.svelte";
  import KeysSection from "./settings/KeysSection.svelte";
  import AboutSection from "./settings/AboutSection.svelte";
  import Modal from "./Modal.svelte";
  import { isMobilePlatform } from "../lib/platform";
  import { app } from "../lib/store.svelte";

  type Section =
    | "accounts"
    | "layout"
    | "mobile"
    | "appearance"
    | "background"
    | "reaction"
    | "externalIntegration"
    | "data"
    | "notify"
    | "mute"
    | "keys"
    | "about"
    | "developer";

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

  // モバイル(振動等)はデスクトップでは意味を持たないタブ自体を隠す。
  // 開発者オプションは「Tsumugiについて」タブのバージョン表示を7回タップして解除するまで隠す
  // (Issue #326)。app.ui.developerOptionsEnabled は設定モーダルを開いたまま解除されうるため
  // $derived で再計算させる(constでは解除後もタブが出ない)。
  const nav = $derived([
    { id: "accounts" as const, label: "アカウント" },
    { id: "layout" as const, label: "レイアウト" },
    ...(isMobilePlatform ? [{ id: "mobile" as const, label: "モバイル" }] : []),
    { id: "appearance" as const, label: "外観" },
    { id: "background" as const, label: "背景" },
    { id: "reaction" as const, label: "リアクション" },
    { id: "externalIntegration" as const, label: "外部連携" },
    { id: "data" as const, label: "データ" },
    { id: "notify" as const, label: "通知" },
    { id: "mute" as const, label: "NG（ミュート）" },
    { id: "keys" as const, label: "キー操作" },
    { id: "about" as const, label: "Tsumugiについて" },
    ...(app.ui.developerOptionsEnabled ? [{ id: "developer" as const, label: "開発者オプション" }] : []),
  ]);

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
            data-testid={`settings-tab-${item.id}`}
            onclick={() => (active = item.id)}
          >
            {item.label}
          </button>
        {/each}
      </nav>
      <section class="min-w-0 flex-1 overflow-y-auto px-5 py-[18px]">
        {#if active === "accounts"}
          <AccountsSection {onAddAccount} {onReauth} />
        {:else if active === "layout"}
          <LayoutSection />
        {:else if active === "mobile"}
          <MobileSection />
        {:else if active === "appearance"}
          <AppearanceSection />
        {:else if active === "background"}
          <BackgroundSection />
        {:else if active === "reaction"}
          <ReactionSection />
        {:else if active === "externalIntegration"}
          <ExternalIntegrationSection />
        {:else if active === "data"}
          <DataSection />
          <hr class="my-5 border-0 border-t border-border" />
          <CacheBackendSettings />
        {:else if active === "notify"}
          <NotifySection />
        {:else if active === "mute"}
          <MuteSection />
        {:else if active === "keys"}
          <KeysSection />
        {:else if active === "about"}
          <AboutSection />
        {:else if active === "developer"}
          <DeveloperSection />
        {/if}
      </section>
    </div>
  </div>
</Modal>
