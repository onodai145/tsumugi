<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { unicodeEmojiUrl, type EmojiStyle } from "../../lib/emoji";
  import { Button } from "$lib/components/ui/button";

  let width = $state(app.ui.defaultColumnWidth);
  let fontFamily = $state(app.ui.fontFamily ?? "");
  let emojiStyle = $state<EmojiStyle>((app.ui.emojiStyle as EmojiStyle) ?? "twemoji");
  let mfmAnimationEnabled = $state(app.ui.mfmAnimationEnabled ?? true);
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

  const emojiStyles: { id: EmojiStyle; label: string }[] = [
    { id: "twemoji", label: "Twemoji" },
    { id: "fluentEmoji", label: "Fluent Emoji" },
    { id: "native", label: "OS標準" },
  ];
  function emojiPreviewUrl(style: EmojiStyle): string | null {
    return unicodeEmojiUrl("😺", style);
  }

  const fontPresets: { label: string; value: string }[] = [
    { label: "既定", value: "" },
    { label: "游ゴシック", value: '"Yu Gothic", "Hiragino Kaku Gothic ProN", sans-serif' },
    { label: "メイリオ", value: "Meiryo, sans-serif" },
    { label: "等幅", value: 'ui-monospace, "Cascadia Code", "SF Mono", monospace' },
    { label: "明朝", value: '"Yu Mincho", "Hiragino Mincho ProN", serif' },
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
        fontFamily,
        uiMode,
        emojiStyle,
        mfmAnimationEnabled,
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

<h3 class="mb-3.5 mt-0 text-base font-semibold">表示</h3>

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

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">絵文字のスタイル</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each emojiStyles as s (s.id)}
      <button
        type="button"
        class={emojiStyle === s.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
        onclick={() => (emojiStyle = s.id)}
      >
        {#if emojiPreviewUrl(s.id)}
          <img class="mr-1 h-[1.2em] w-[1.2em] object-contain align-[-0.25em]" src={emojiPreviewUrl(s.id)} alt="" />
        {/if}
        {s.label}
      </button>
    {/each}
  </div>
  <p class="mb-4 mt-0 flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
    Unicode絵文字(リアクション等)の見た目です。プレビュー:
    {#each ["😺", "👍", "🎉"] as c}
      {#if emojiPreviewUrl(emojiStyle)}
        <img class="h-[1.3em] w-[1.3em] object-contain" src={unicodeEmojiUrl(c, emojiStyle) ?? undefined} alt={c} />
      {:else}
        {c}
      {/if}
    {/each}
  </p>
</div>

<label class="mb-2 flex items-center gap-2 text-sm"
  ><input type="checkbox" bind:checked={mfmAnimationEnabled} /> MFMアニメーション($[shake]等)を有効にする</label
>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  他人の投稿に含まれる装飾($[shake]/$[spin]/$[rainbow]等)のアニメーション表示です。
  環境によってはこの描画コストが高く、CPU使用率が上がることがあります
  (Linux/Wayland環境で特に発生しやすい既知の問題です)。気になる場合はOFFにしてください
  (静的な装飾は残ります)。
</p>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">フォント</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each fontPresets as p (p.value)}
      <button
        type="button"
        class={fontFamily === p.value
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
        onclick={() => (fontFamily = p.value)}
      >
        {p.label}
      </button>
    {/each}
  </div>
  <input
    type="text"
    class="mt-1.5 w-full rounded-md border border-border bg-muted px-[9px] py-[7px] font-[inherit] text-foreground"
    placeholder='CSS の font-family 値(例: "Noto Sans JP", sans-serif)'
    bind:value={fontFamily}
  />
</div>
<p class="mb-4 mt-0 text-xs text-muted-foreground" style={fontFamily ? `font-family: ${fontFamily}` : undefined}>
  プレビュー: あいうえお ABCDEFG 123
</p>

<div class="flex items-center justify-end gap-3">
  {#if saved}<span class="text-sm text-[var(--success)]">保存しました</span>{/if}
  <Button type="button" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</Button>
</div>
{#if err}<p class="mt-2 mb-0 text-sm text-destructive">{err}</p>{/if}
