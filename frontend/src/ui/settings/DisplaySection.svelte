<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { unicodeEmojiUrl, type EmojiStyle } from "../../lib/emoji";

  let theme = $state(app.ui.theme);
  let width = $state(app.ui.defaultColumnWidth);
  let fontFamily = $state(app.ui.fontFamily ?? "");
  let backgroundImage = $state(app.ui.backgroundImage ?? "");
  let backgroundDim = $state(app.ui.backgroundDim ?? 0);
  let backgroundBlur = $state(app.ui.backgroundBlur ?? 0);
  let columnOpacity = $state(app.ui.columnOpacity ?? 100);
  let emojiStyle = $state<EmojiStyle>((app.ui.emojiStyle as EmojiStyle) ?? "twemoji");
  let pickingImage = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  const themes: { id: string; label: string }[] = [
    { id: "auto", label: "OSに合わせる" },
    { id: "light", label: "ライト" },
    { id: "dark", label: "ダーク" },
  ];

  const emojiStyles: { id: EmojiStyle; label: string }[] = [
    { id: "twemoji", label: "Twemoji" },
    { id: "fluentEmoji", label: "Fluent Emoji" },
    { id: "native", label: "OS標準" },
  ];
  const emojiHost = app.accounts.find((a) => a.id === app.defaultAccountId())?.host;
  function emojiPreviewUrl(style: EmojiStyle): string | null {
    return unicodeEmojiUrl("😺", style, emojiHost);
  }

  const fontPresets: { label: string; value: string }[] = [
    { label: "既定", value: "" },
    { label: "游ゴシック", value: '"Yu Gothic", "Hiragino Kaku Gothic ProN", sans-serif' },
    { label: "メイリオ", value: "Meiryo, sans-serif" },
    { label: "等幅", value: 'ui-monospace, "Cascadia Code", "SF Mono", monospace' },
    { label: "明朝", value: '"Yu Mincho", "Hiragino Mincho ProN", serif' },
  ];

  async function pickImage() {
    err = null;
    pickingImage = true;
    try {
      const url = await app.pickBackgroundImage();
      if (url) backgroundImage = url;
    } catch (e) {
      err = String(e);
    } finally {
      pickingImage = false;
    }
  }

  function clearImage() {
    backgroundImage = "";
  }

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      const w = Math.min(720, Math.max(220, Math.round(width) || 300));
      width = w;
      // このセクションが編集しないフィールド(既定アカウント等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        theme,
        defaultColumnWidth: w,
        fontFamily,
        backgroundImage,
        backgroundDim,
        backgroundBlur,
        columnOpacity,
        emojiStyle,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="title">表示</h3>

<div class="field">
  <span>テーマ</span>
  <div class="seg">
    {#each themes as t (t.id)}
      <button class="seg-btn" class:active={theme === t.id} onclick={() => (theme = t.id)}>{t.label}</button>
    {/each}
  </div>
</div>

<label class="field">
  <span>新規カラムの既定幅（px, 220〜720）</span>
  <input type="number" min="220" max="720" step="10" bind:value={width} />
</label>
<p class="hint">既定幅は次に追加するカラムから適用されます。既存カラムはカラム端のドラッグで個別調整できます。</p>

<div class="field">
  <span>絵文字のスタイル</span>
  <div class="seg">
    {#each emojiStyles as s (s.id)}
      <button class="seg-btn" class:active={emojiStyle === s.id} onclick={() => (emojiStyle = s.id)}>
        {#if emojiPreviewUrl(s.id)}
          <img class="emoji-style-thumb" src={emojiPreviewUrl(s.id)} alt="" />
        {/if}
        {s.label}
      </button>
    {/each}
  </div>
  <p class="hint preview-row">
    Unicode絵文字（リアクション等）の見た目です。プレビュー:
    {#each ["😺", "👍", "🎉"] as c}
      {#if emojiPreviewUrl(emojiStyle)}
        <img class="emoji-preview" src={unicodeEmojiUrl(c, emojiStyle, emojiHost) ?? undefined} alt={c} />
      {:else}
        {c}
      {/if}
    {/each}
  </p>
</div>

<div class="field">
  <span>フォント</span>
  <div class="seg">
    {#each fontPresets as p (p.value)}
      <button
        class="seg-btn"
        class:active={fontFamily === p.value}
        onclick={() => (fontFamily = p.value)}
      >
        {p.label}
      </button>
    {/each}
  </div>
  <input
    type="text"
    class="font-input"
    placeholder='CSS の font-family 値（例: "Noto Sans JP", sans-serif）'
    bind:value={fontFamily}
  />
</div>
<p class="hint" style={fontFamily ? `font-family: ${fontFamily}` : undefined}>
  プレビュー: あいうえお ABCDEFG 123
</p>

<div class="field">
  <span>背景画像</span>
  <div class="bg-row">
    {#if backgroundImage}
      <img class="bg-thumb" src={backgroundImage} alt="背景プレビュー" />
    {/if}
    <button class="mini-btn" disabled={pickingImage} onclick={pickImage}>
      {pickingImage ? "読み込み中…" : backgroundImage ? "画像を変更" : "画像を選択"}
    </button>
    {#if backgroundImage}
      <button class="mini-btn" onclick={clearImage}>解除</button>
    {/if}
  </div>
</div>

{#if backgroundImage}
  <label class="field">
    <span>背景の暗さ（{backgroundDim}%）</span>
    <input type="range" min="0" max="100" step="5" bind:value={backgroundDim} />
  </label>
  <label class="field">
    <span>背景のぼかし（{backgroundBlur}px）</span>
    <input type="range" min="0" max="40" step="2" bind:value={backgroundBlur} />
  </label>
  <label class="field">
    <span>カラムの不透明度（{columnOpacity}%）</span>
    <input type="range" min="60" max="100" step="5" bind:value={columnOpacity} />
  </label>
  <p class="hint">数値が低いほど背景画像が透けて見えます。</p>
{/if}

<div class="actions">
  {#if saved}<span class="ok">保存しました</span>{/if}
  <button class="save" disabled={busy} onclick={save}>{busy ? "保存中…" : "保存"}</button>
</div>
{#if err}<p class="err">{err}</p>{/if}

<style>
  .title {
    margin: 0 0 14px;
    font-size: 1rem;
    font-weight: 600;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
    font-size: 0.82rem;
  }
  .field > span {
    color: var(--text-dim);
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    width: fit-content;
  }
  .seg-btn {
    padding: 6px 14px;
    border: none;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.82rem;
    border-right: 1px solid var(--border);
  }
  .seg-btn:last-child {
    border-right: none;
  }
  .seg-btn.active {
    background: var(--accent);
    color: #fff;
  }
  input[type="number"] {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    width: 140px;
  }
  .font-input {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
    width: 100%;
    margin-top: 6px;
  }
  .hint {
    font-size: 0.76rem;
    color: var(--text-dim);
    margin: 0 0 16px;
  }
  .emoji-style-thumb {
    height: 1.2em;
    width: 1.2em;
    object-fit: contain;
    vertical-align: -0.25em;
    margin-right: 4px;
  }
  .preview-row {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }
  .emoji-preview {
    height: 1.3em;
    width: 1.3em;
    object-fit: contain;
  }
  .bg-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .bg-thumb {
    width: 56px;
    height: 36px;
    object-fit: cover;
    border-radius: 6px;
    border: 1px solid var(--border);
  }
  .mini-btn {
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.8rem;
  }
  .mini-btn:hover {
    border-color: var(--accent);
  }
  .mini-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  input[type="range"] {
    accent-color: var(--accent);
    width: 100%;
    max-width: 320px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
  }
  .ok {
    font-size: 0.8rem;
    color: #22c55e;
  }
  .save {
    padding: 7px 18px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .save:disabled {
    opacity: 0.5;
  }
  .err {
    color: #ef4444;
    font-size: 0.82rem;
    margin: 8px 0 0;
  }
</style>
