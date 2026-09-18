<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { SEARCH_ENGINE_PRESETS, DEFAULT_SEARCH_ENGINE_URL } from "../../lib/searchEngine";
  import { Button } from "$lib/components/ui/button";

  let searchEngineUrl = $state(app.ui.searchEngineUrl ?? DEFAULT_SEARCH_ENGINE_URL);
  let urlPreviewEnabled = $state(app.ui.urlPreviewEnabled ?? true);
  let summalyProxyUrl = $state(app.ui.summalyProxyUrl ?? "");
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // このセクションが編集しないフィールド(外観・データ等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        searchEngineUrl: searchEngineUrl.trim() || DEFAULT_SEARCH_ENGINE_URL,
        urlPreviewEnabled,
        summalyProxyUrl: summalyProxyUrl.trim(),
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">外部連携</h3>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">MFM検索構文($[search]相当)で使う検索エンジン</span>
  <div class="inline-flex w-fit flex-wrap overflow-hidden rounded-md border border-border">
    {#each SEARCH_ENGINE_PRESETS as p (p.url)}
      <button
        type="button"
        class={searchEngineUrl === p.url
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
        onclick={() => (searchEngineUrl = p.url)}
      >
        {p.label}
      </button>
    {/each}
  </div>
  <input
    type="text"
    class="mt-1.5 w-full rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
    placeholder={"検索URLテンプレート（{query} をクエリ文字列に置換）"}
    bind:value={searchEngineUrl}
  />
  <p class="mb-4 mt-0 text-xs text-muted-foreground">
    プレースホルダ<code class="mfm-code">{"{query}"}</code>を含むURLを指定すると好きな検索エンジンを使えます。
    空欄や<code class="mfm-code">{"{query}"}</code>を含まない値を保存した場合はGoogle検索に戻ります。
  </p>
</div>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <label class="flex items-center gap-2"
    ><input type="checkbox" bind:checked={urlPreviewEnabled} /> 投稿本文中のURLにリンクプレビューを表示する</label
  >
  <span class="text-muted-foreground">カスタムsummalyプロキシURL（任意）</span>
  <input
    type="text"
    class="w-full rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
    placeholder="空欄なら接続先インスタンスの /url を使用"
    bind:value={summalyProxyUrl}
  />
  <p class="mb-0 mt-0 text-xs text-muted-foreground">
    設定すると、リンクプレビュー対象のURLは接続先インスタンスではなく指定したプロキシへ直接送信されます。
    信頼できるプロキシのみを指定してください。
  </p>
</div>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
