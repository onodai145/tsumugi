<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { commands } from "../../bindings/tauri.gen";
  import { app } from "../../lib/store.svelte";

  const REPO_URL = "https://github.com/onodai145/tsumugi";
  const DEVELOPER_OPTIONS_TAP_THRESHOLD = 7;

  let appVersion = $state<string | null>(null);
  let commitHash = $state<string | null>(null);
  let versionTapCount = $state(0);
  let developerOptionsJustUnlocked = $state(false);
  let unlocking = $state(false);
  let err = $state<string | null>(null);

  $effect(() => {
    void getVersion().then((v) => (appVersion = v));
    void commands.gitCommitHash().then((v) => (commitHash = v));
    void app.checkForUpdate();
  });

  // Androidの「ビルド番号連打」を模した隠し機能(Issue #326)。バージョン表示を7回タップすると
  // 「開発者オプション」タブが恒久的に出現する(無効化する手段は用意しない)。
  async function onVersionTap() {
    if (app.ui.developerOptionsEnabled || unlocking) return;
    versionTapCount += 1;
    if (versionTapCount < DEVELOPER_OPTIONS_TAP_THRESHOLD) return;
    unlocking = true;
    try {
      await app.setUiPrefs({ ...app.ui, developerOptionsEnabled: true });
      developerOptionsJustUnlocked = true;
    } catch (e) {
      err = String(e);
    } finally {
      unlocking = false;
    }
  }
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
    <dd class="m-0 break-all text-sm">
      <button
        type="button"
        class="border-0 bg-transparent p-0 text-left font-[inherit] text-sm text-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        data-testid="settings-version-tap-target"
        onclick={onVersionTap}
      >{appVersion ?? "…"}</button>
    </dd>

    <dt class="text-sm text-muted-foreground">コミット</dt>
    <dd class="m-0 break-all text-sm">{commitHash ?? "…"}</dd>

    <dt class="text-sm text-muted-foreground">ライセンス</dt>
    <dd class="m-0 break-all text-sm">MIT</dd>

    <dt class="text-sm text-muted-foreground">リポジトリ</dt>
    <dd class="m-0 break-all text-sm">
      <button type="button" class="border-0 bg-transparent p-0 text-left text-sm text-primary hover:underline" onclick={() => openUrl(REPO_URL)}>{REPO_URL}</button>
    </dd>
  </dl>

  {#if developerOptionsJustUnlocked}
    <p class="mt-3 mb-0 text-sm text-[var(--success)]">開発者オプションを有効にしました</p>
  {/if}
  {#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
</div>

<style>
  .update-banner {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }
</style>
