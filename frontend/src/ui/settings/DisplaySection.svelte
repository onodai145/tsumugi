<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { unicodeEmojiUrl, type EmojiStyle } from "../../lib/emoji";
  import { PRESETS, THEME_VAR_KEYS } from "../../lib/theme";
  import type { CustomTheme, ThemeColors } from "../../bindings/tauri.gen";
  import { X, Check, Pencil, Trash2, Plus } from "@lucide/svelte";

  let theme = $state(app.ui.theme);
  let width = $state(app.ui.defaultColumnWidth);
  let fontFamily = $state(app.ui.fontFamily ?? "");
  let backgroundImage = $state(app.ui.backgroundImage ?? "");
  let backgroundDim = $state(app.ui.backgroundDim ?? 0);
  let backgroundBlur = $state(app.ui.backgroundBlur ?? 0);
  let columnOpacity = $state(app.ui.columnOpacity ?? 100);
  let emojiStyle = $state<EmojiStyle>((app.ui.emojiStyle as EmojiStyle) ?? "twemoji");
  let gapFillLimit = $state(app.ui.gapFillLimit ?? 200);
  let mediaThumbnailHeight = $state(app.ui.mediaThumbnailHeight ?? 200);
  let noteCacheLimit = $state(app.ui.noteCacheLimit ?? 5000);
  let pickingImage = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  const themes: { id: string; label: string }[] = [
    { id: "auto", label: "OSに合わせる" },
    { id: "light", label: "ライト" },
    { id: "dark", label: "ダーク" },
  ];

  // ---- カスタムテーマ(プリセット + ユーザー作成) ----
  const customThemes = $derived(app.ui.customThemes ?? []);
  const colorLabels: Record<keyof ThemeColors, string> = {
    surface1: "背景1",
    surface2: "背景2",
    surface3: "背景3",
    border: "枠線",
    text: "文字",
    textDim: "文字(淡)",
    accent: "アクセント",
  };
  const HEX_RE = /^#[0-9a-fA-F]{6}$/;
  function blankColors(): ThemeColors {
    return {
      surface1: "#1a1a1a",
      surface2: "#242424",
      surface3: "#2e2e2e",
      border: "#3a3a3a",
      text: "#eeeeee",
      textDim: "#999999",
      accent: "#7c5cff",
    };
  }
  let editingTheme = $state<CustomTheme | null>(null);
  let editErr = $state<string | null>(null);

  function startCreateTheme() {
    editingTheme = { id: crypto.randomUUID(), name: "", colors: blankColors() };
    editErr = null;
  }
  function startEditTheme(t: CustomTheme) {
    editingTheme = { id: t.id, name: t.name, colors: { ...t.colors } };
    editErr = null;
  }
  function cancelEditTheme() {
    editingTheme = null;
    editErr = null;
  }
  async function saveCustomTheme() {
    if (!editingTheme) return;
    if (!editingTheme.name.trim()) {
      editErr = "名前を入力してください";
      return;
    }
    for (const { key } of THEME_VAR_KEYS) {
      if (!HEX_RE.test(editingTheme.colors[key])) {
        editErr = `${colorLabels[key]}は #rrggbb 形式で入力してください`;
        return;
      }
    }
    const exists = customThemes.some((t) => t.id === editingTheme!.id);
    const next = exists
      ? customThemes.map((t) => (t.id === editingTheme!.id ? editingTheme! : t))
      : [...customThemes, editingTheme];
    await app.setUiPrefs({ ...app.ui, customThemes: next });
    editingTheme = null;
    editErr = null;
  }
  async function removeCustomTheme(id: string) {
    const next = customThemes.filter((t) => t.id !== id);
    // 削除対象が選択中のテーマなら、この1回の保存でthemeもautoへ戻す。
    // (theme はそのままにすると #applyTheme のフォールバックが二重保存を起こすため)
    const clearing = theme === `custom:${id}`;
    await app.setUiPrefs({ ...app.ui, customThemes: next, theme: clearing ? "auto" : app.ui.theme });
    if (clearing) theme = "auto";
  }

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
      const gapLimit = Math.min(1000, Math.max(0, Math.round(gapFillLimit) || 0));
      gapFillLimit = gapLimit;
      const thumbHeight = Math.min(600, Math.max(80, Math.round(mediaThumbnailHeight) || 200));
      mediaThumbnailHeight = thumbHeight;
      const cacheLimit = Math.min(100000, Math.max(0, Math.round(noteCacheLimit) || 0));
      noteCacheLimit = cacheLimit;
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
        gapFillLimit: gapLimit,
        mediaThumbnailHeight: thumbHeight,
        noteCacheLimit: cacheLimit,
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

{#snippet swatchStrip(colors: ThemeColors)}
  <span class="swatch-strip">
    {#each THEME_VAR_KEYS as v (v.key)}
      <span class="sw" style={`background:${colors[v.key]}`}></span>
    {/each}
  </span>
{/snippet}

<div class="field">
  <span>プリセットテーマ</span>
  <div class="theme-grid">
    {#each PRESETS as p (p.id)}
      {@const isActive = theme === `preset:${p.id}`}
      <button class="theme-card" class:active={isActive} onclick={() => (theme = `preset:${p.id}`)}>
        {@render swatchStrip(p.colors)}
        <span class="theme-card-name">
          {p.name}
          {#if isActive}<Check size={13} class="theme-card-check" />{/if}
        </span>
      </button>
    {/each}
  </div>
</div>

<div class="field">
  <span>カスタムテーマ</span>
  <div class="theme-grid">
    {#each customThemes as t (t.id)}
      {@const isActive = theme === `custom:${t.id}`}
      <div class="theme-card-wrap">
        <button class="theme-card" class:active={isActive} onclick={() => (theme = `custom:${t.id}`)}>
          {@render swatchStrip(t.colors)}
          <span class="theme-card-name">
            {t.name}
            {#if isActive}<Check size={13} class="theme-card-check" />{/if}
          </span>
        </button>
        <div class="theme-card-actions">
          <button class="icon-btn" title="編集" onclick={() => startEditTheme(t)}><Pencil size={13} /></button>
          <button class="icon-btn" title="削除" onclick={() => removeCustomTheme(t.id)}><Trash2 size={13} /></button>
        </div>
      </div>
    {/each}
  </div>
  <button class="mini-btn add-theme" onclick={startCreateTheme}><Plus size={13} /> 新規作成</button>

  {#if editingTheme}
    <div class="theme-editor">
      <input type="text" class="theme-name-input" placeholder="テーマ名" bind:value={editingTheme.name} />
      {#each THEME_VAR_KEYS as v (v.key)}
        <div class="color-row">
          <span class="color-label">{colorLabels[v.key]}</span>
          <span class="swatch" style={`background:${editingTheme.colors[v.key]}`}></span>
          <input type="text" class="hex-input" bind:value={editingTheme.colors[v.key]} />
        </div>
      {/each}
      {#if editErr}<p class="err">{editErr}</p>{/if}
      <div class="editor-actions">
        <button class="mini-btn" onclick={cancelEditTheme}><X size={13} /> キャンセル</button>
        <button class="save" onclick={saveCustomTheme}>このテーマを保存</button>
      </div>
    </div>
  {/if}
</div>

<label class="field">
  <span>新規カラムの既定幅（px, 220〜720）</span>
  <input type="number" min="220" max="720" step="10" bind:value={width} />
</label>
<p class="hint">既定幅は次に追加するカラムから適用されます。既存カラムはカラム端のドラッグで個別調整できます。</p>

<label class="field">
  <span>起動時のギャップ埋め（件, 0〜1000。0で無効）</span>
  <input type="number" min="0" max="1000" step="50" bind:value={gapFillLimit} />
</label>
<p class="hint">
  アプリを閉じていた間に流れたノートを、起動時にこの件数まで遡ってREST取得します。
  0にすると従来どおりキャッシュのみ表示します。
</p>

<label class="field">
  <span>メディアサムネイルの高さ上限（px, 80〜600）</span>
  <input type="number" min="80" max="600" step="20" bind:value={mediaThumbnailHeight} />
</label>
<p class="hint">
  ノートに添付された画像/動画のサムネイル最大高さです。小さくするとノートを詰めて表示でき、
  大きくすると画像を大きく見られます。
</p>

<label class="field">
  <span>ノートキャッシュの保持件数上限（件, 0〜100000。0で無制限）</span>
  <input type="number" min="0" max="100000" step="500" bind:value={noteCacheLimit} />
</label>
<p class="hint">
  ローカルDBに保持するノート件数の上限です。超えた分は古い順に自動で削除されます。
  0にすると無制限に溜め続けます（ディスク容量を圧迫する可能性があります）。
</p>

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
        <img class="emoji-preview" src={unicodeEmojiUrl(c, emojiStyle) ?? undefined} alt={c} />
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
  .theme-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
    gap: 10px;
  }
  .theme-card-wrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .theme-card {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    font-size: 0.78rem;
    overflow: hidden;
    text-align: left;
  }
  .theme-card:hover {
    border-color: var(--accent);
  }
  .theme-card.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .swatch-strip {
    display: flex;
    width: 100%;
    height: 30px;
    flex: none;
  }
  .sw {
    flex: 1;
    height: 100%;
  }
  .theme-card-name {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    padding: 7px 9px;
  }
  .theme-card-name :global(.theme-card-check) {
    flex: none;
    color: var(--accent);
  }
  .theme-card-actions {
    display: flex;
    gap: 4px;
  }
  .icon-btn {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 5px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text-dim);
    cursor: pointer;
  }
  .icon-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .add-theme {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-top: 8px;
  }
  .theme-editor {
    margin-top: 10px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .theme-name-input {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-1);
    color: var(--text);
    font-family: inherit;
  }
  .color-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .color-label {
    width: 80px;
    flex: none;
    font-size: 0.8rem;
    color: var(--text-dim);
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    border: 1px solid var(--border);
    flex: none;
  }
  .hex-input {
    width: 100px;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-1);
    color: var(--text);
    font-family: ui-monospace, monospace;
    font-size: 0.82rem;
  }
  .editor-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
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
