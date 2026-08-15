<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";

  let noteCacheLimit = $state(app.ui.noteCacheLimit ?? 10000);
  let noteCacheMaxAgeDays = $state(app.ui.noteCacheMaxAgeDays ?? 0);
  let noteCacheMaxSizeMb = $state(app.ui.noteCacheMaxSizeMb ?? 0);
  let gapFillLimit = $state(app.ui.gapFillLimit ?? 200);
  let enableFileLogging = $state(app.ui.enableFileLogging ?? false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      const cacheLimit = Math.min(100000, Math.max(0, Math.round(noteCacheLimit) || 0));
      noteCacheLimit = cacheLimit;
      const cacheMaxAge = Math.min(3650, Math.max(0, Math.round(noteCacheMaxAgeDays) || 0));
      noteCacheMaxAgeDays = cacheMaxAge;
      const cacheMaxSize = Math.min(10000, Math.max(0, Math.round(noteCacheMaxSizeMb) || 0));
      noteCacheMaxSizeMb = cacheMaxSize;
      const gapLimit = Math.min(1000, Math.max(0, Math.round(gapFillLimit) || 0));
      gapFillLimit = gapLimit;
      // このセクションが編集しないフィールド(外観等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        noteCacheLimit: cacheLimit,
        noteCacheMaxAgeDays: cacheMaxAge,
        noteCacheMaxSizeMb: cacheMaxSize,
        gapFillLimit: gapLimit,
        enableFileLogging,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">データ</h3>

<h4 class="mb-2 mt-0 text-sm font-semibold text-muted-foreground">ノートキャッシュ</h4>

<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">ノートキャッシュの保持件数上限(件, 0〜100000。0で無制限)</span>
  <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="0" max="100000" step="500" bind:value={noteCacheLimit} />
</label>
<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">ノートキャッシュの保持日数上限(日, 0〜3650。0で無制限)</span>
  <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="0" max="3650" step="1" bind:value={noteCacheMaxAgeDays} />
</label>
<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">ノートキャッシュのサイズ上限(MB, 0〜10000。0で無制限)</span>
  <input class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground" type="number" min="0" max="10000" step="50" bind:value={noteCacheMaxSizeMb} />
</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  ローカルDBに保持するノートの上限です。件数・投稿からの経過日数・DBファイルサイズのいずれかを
  超えた分は古い順に自動で削除されます。すべて0にすると無制限に溜め続けます
  (ディスク容量を圧迫する可能性があります)。
</p>

<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">起動時のギャップ埋め(件, 0〜1000。0で無効)</span>
  <input class="w-[140px] rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" type="number" min="0" max="1000" step="50" bind:value={gapFillLimit} />
</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  アプリを閉じていた間に流れたノートを、起動時にこの件数まで遡ってREST取得します。
  0にすると従来どおりキャッシュのみ表示します。
</p>

<h4 class="mb-2 mt-0 text-sm font-semibold text-muted-foreground">デバッグ</h4>

<label class="mb-2 flex items-center gap-2 text-sm"><input type="checkbox" bind:checked={enableFileLogging} /> 動作ログをファイルに残す(デバッグ用)</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  WebSocket再接続やpingタイムアウトなどの内部ログを、アプリのログディレクトリにファイルとして
  永続化します。通知が来るタイミングがおかしい等の不具合調査用で、既定はOFFです。
  切り替えは次回起動から反映されます。
</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
