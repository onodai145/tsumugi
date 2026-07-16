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

<div class="about">
  <h2 class="app-name">tsumugi</h2>
  <p class="desc">Misskey マルチカラムデスクトップクライアント</p>

  {#if app.updateAvailable}
    <button class="update-banner" onclick={() => openUrl(app.updateAvailable!.url)}>
      新しいバージョン v{app.updateAvailable.version} が公開されています（クリックで開く）
    </button>
  {/if}

  <dl class="info">
    <dt>バージョン</dt>
    <dd>{appVersion ?? "…"}</dd>

    <dt>コミット</dt>
    <dd>{commitHash ?? "…"}</dd>

    <dt>ライセンス</dt>
    <dd>MIT</dd>

    <dt>リポジトリ</dt>
    <dd>
      <button class="link" onclick={() => openUrl(REPO_URL)}>{REPO_URL}</button>
    </dd>
  </dl>
</div>

<style>
  .about {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .app-name {
    font-size: 1.2rem;
    font-weight: 700;
    margin: 0;
  }
  .desc {
    margin: 0 0 12px;
    color: var(--text-dim);
    font-size: 0.85rem;
  }
  .update-banner {
    display: block;
    width: 100%;
    text-align: left;
    margin: 4px 0 12px;
    padding: 8px 10px;
    border: 1px solid var(--accent);
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--text);
    font-size: 0.82rem;
    cursor: pointer;
    font-family: inherit;
  }
  .info {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 16px;
    margin: 0;
  }
  .info dt {
    color: var(--text-dim);
    font-size: 0.82rem;
  }
  .info dd {
    margin: 0;
    font-size: 0.85rem;
    word-break: break-all;
  }
  .link {
    border: none;
    background: transparent;
    color: var(--accent);
    cursor: pointer;
    padding: 0;
    font-size: 0.85rem;
    text-align: left;
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
