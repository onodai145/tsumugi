<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { commands } from "../../bindings/tauri.gen";

  const REPO_URL = "https://github.com/onodai145/tsumugi";

  let appVersion = $state<string | null>(null);
  let commitHash = $state<string | null>(null);

  $effect(() => {
    void getVersion().then((v) => (appVersion = v));
    void commands.gitCommitHash().then((v) => (commitHash = v));
  });
</script>

<div class="about">
  <h2 class="app-name">tsumugi</h2>
  <p class="desc">Misskey マルチカラムデスクトップクライアント</p>

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
