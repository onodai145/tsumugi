<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";

  let width = $state(app.ui.defaultColumnWidth);
  let uiMode = $state(app.ui.uiMode ?? "auto");
  let gapFillLimit = $state(app.ui.gapFillLimit ?? 200);
  let mediaThumbnailHeight = $state(app.ui.mediaThumbnailHeight ?? 200);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  // 投稿欄の見せ方(常時表示のPC版 or FAB+モーダルのモバイル版)を切り替える(Issue #51)。
  const uiModes: { id: string; label: string }[] = [
    { id: "auto", label: "OSに合わせる" },
    { id: "desktop", label: "PC版" },
    { id: "mobile", label: "モバイル版" },
  ];

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      const w = Math.min(720, Math.max(220, Math.round(width) || 300));
      width = w;
      const gapLimit = Math.min(1000, Math.max(0, Math.round(gapFillLimit) || 0));
      gapFillLimit = gapLimit;
      const thumbHeight = Math.min(600, Math.max(80, Math.round(mediaThumbnailHeight) || 200));
      mediaThumbnailHeight = thumbHeight;
      // このセクションが編集しないフィールド(テーマ・背景等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        defaultColumnWidth: w,
        uiMode,
        gapFillLimit: gapLimit,
        mediaThumbnailHeight: thumbHeight,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">レイアウト</h3>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">UIモード</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each uiModes as m (m.id)}
      <button
        type="button"
        class={uiMode === m.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
        onclick={() => (uiMode = m.id)}
      >{m.label}</button>
    {/each}
  </div>
  <p class="mb-4 mt-0 text-xs text-muted-foreground">モバイル版は投稿欄がFAB+モーダルに、PC版は投稿欄が常時表示になります。</p>
</div>

<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">新規カラムの既定幅(px, 220〜720)</span>
  <input class="w-[140px] rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" type="number" min="220" max="720" step="10" bind:value={width} />
</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">既定幅は次に追加するカラムから適用されます。既存カラムはカラム端のドラッグで個別調整できます。</p>

<!-- 起動時のギャップ埋めは本来「データ」寄りの設定(#212で移動を検討中)だが、今回のスコープでは現状維持。 -->
<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">起動時のギャップ埋め(件, 0〜1000。0で無効)</span>
  <input class="w-[140px] rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" type="number" min="0" max="1000" step="50" bind:value={gapFillLimit} />
</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  アプリを閉じていた間に流れたノートを、起動時にこの件数まで遡ってREST取得します。
  0にすると従来どおりキャッシュのみ表示します。
</p>

<label class="mb-2.5 flex flex-col gap-1 text-sm">
  <span class="text-muted-foreground">メディアサムネイルの高さ上限(px, 80〜600)</span>
  <input class="w-[140px] rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground" type="number" min="80" max="600" step="20" bind:value={mediaThumbnailHeight} />
</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  ノートに添付された画像/動画のサムネイル最大高さです。小さくするとノートを詰めて表示でき、
  大きくすると画像を大きく見られます。
</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
