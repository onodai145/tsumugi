<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { commands } from "../../bindings/tauri.gen";
  import { app } from "../../lib/store.svelte";

  const REPO_URL = "https://github.com/onodai145/tsumugi";

  let appVersion = $state<string | null>(null);
  let commitHash = $state<string | null>(null);

  $effect(() => {
    void getVersion().then((v) => (appVersion = v));
    void commands.gitCommitHash().then((v) => (commitHash = v));
    void app.checkForUpdate();
  });
</script>

<div class="flex flex-col gap-1">
  <h2 class="m-0 text-lg font-bold">tsumugi</h2>
  <p class="mb-3 mt-0 text-sm text-muted-foreground">Misskey マルチカラムデスクトップクライアント</p>

  {#if app.updateAvailable}
    <button
      type="button"
      class="update-banner mb-3 mt-1 block w-full rounded-lg border border-primary px-2.5 py-2 text-left font-[inherit] text-sm text-foreground"
      onclick={() => openUrl(app.updateAvailable!.url)}
    >
      新しいバージョン v{app.updateAvailable.version} が公開されています(クリックで開く)
    </button>
  {/if}

  <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
    <dt class="text-sm text-muted-foreground">バージョン</dt>
    <dd class="m-0 break-all text-sm">{appVersion ?? "…"}</dd>

    <dt class="text-sm text-muted-foreground">コミット</dt>
    <dd class="m-0 break-all text-sm">{commitHash ?? "…"}</dd>

    <dt class="text-sm text-muted-foreground">ライセンス</dt>
    <dd class="m-0 break-all text-sm">MIT</dd>

    <dt class="text-sm text-muted-foreground">リポジトリ</dt>
    <dd class="m-0 break-all text-sm">
      <button type="button" class="border-0 bg-transparent p-0 text-left text-sm text-primary hover:underline" onclick={() => openUrl(REPO_URL)}>{REPO_URL}</button>
    </dd>
  </dl>
</div>

<style>
  .update-banner {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }
</style>
