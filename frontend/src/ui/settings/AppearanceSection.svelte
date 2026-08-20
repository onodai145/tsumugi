<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { PRESETS, THEME_VAR_KEYS, SYNTAX_VAR_KEYS } from "../../lib/theme";
  import { unicodeEmojiUrl, type EmojiStyle } from "../../lib/emoji";
  import type { CustomTheme, ThemeColors, CustomSyntaxTheme } from "../../bindings/tauri.gen";
  import { BUNDLED_SHIKI_THEMES } from "../../lib/shikiThemeList";
  import { SEARCH_ENGINE_PRESETS, DEFAULT_SEARCH_ENGINE_URL } from "../../lib/searchEngine";
  import { X, Check, Pencil, Trash2, Plus } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let theme = $state(app.ui.theme);
  let codeHighlightTheme = $state(app.ui.codeHighlightTheme ?? "auto");
  let fontFamily = $state(app.ui.fontFamily ?? "");
  let emojiStyle = $state<EmojiStyle>((app.ui.emojiStyle as EmojiStyle) ?? "twemoji");
  let mfmAnimationEnabled = $state(app.ui.mfmAnimationEnabled ?? true);
  let searchEngineUrl = $state(app.ui.searchEngineUrl ?? DEFAULT_SEARCH_ENGINE_URL);
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
    success: "成功色(Renoteバナー等)",
    info: "情報色(リプライバナー等)",
    danger: "危険色(削除ボタン等)",
    warning: "警告色",
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
      success: "#22c55e",
      info: "#3b82f6",
      danger: "#ef4444",
      warning: "#eab308",
    };
  }
  let editingTheme = $state<CustomTheme | null>(null);
  let editErr = $state<string | null>(null);

  function startCreateTheme() {
    editingTheme = { id: crypto.randomUUID(), name: "", colors: blankColors() };
    editErr = null;
  }
  function startEditTheme(t: CustomTheme) {
    // success/info/danger/warning 追加前に作られたカスタムテーマは colors に無いことがあるため、
    // 既定値で埋めてから編集フォームへ渡す(undefinedのまま渡すとhex入力欄が空になる)。
    editingTheme = { id: t.id, name: t.name, colors: { ...blankColors(), ...t.colors } };
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
      if (!HEX_RE.test(editingTheme.colors[key] ?? "")) {
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

  // ---- カスタムシンタックステーマ ----
  const customSyntaxThemes = $derived(app.ui.customSyntaxThemes ?? []);
  const syntaxColorLabels: Record<keyof CustomSyntaxTheme, string> = {
    id: "id",
    name: "名前",
    background: "背景",
    text: "文字（既定）",
    comment: "コメント",
    string: "文字列",
    keyword: "キーワード",
    function: "関数",
    constant: "定数・数値",
    parameter: "引数",
    stringExpression: "文字列内の式展開",
    punctuation: "記号",
    link: "リンク",
  };
  function blankSyntaxColors(): CustomSyntaxTheme {
    return {
      id: crypto.randomUUID(),
      name: "",
      background: "#1e1e1e",
      text: "#d4d4d4",
      comment: "#6a9955",
      string: "#ce9178",
      keyword: "#569cd6",
      function: "#dcdcaa",
      constant: "#b5cea8",
      parameter: "#9cdcfe",
      stringExpression: "#d7ba7d",
      punctuation: "#d4d4d4",
      link: "#569cd6",
    };
  }
  let editingSyntaxTheme = $state<CustomSyntaxTheme | null>(null);
  let syntaxEditErr = $state<string | null>(null);

  function startCreateSyntaxTheme() {
    editingSyntaxTheme = blankSyntaxColors();
    syntaxEditErr = null;
  }
  function startEditSyntaxTheme(t: CustomSyntaxTheme) {
    editingSyntaxTheme = { ...t };
    syntaxEditErr = null;
  }
  function cancelEditSyntaxTheme() {
    editingSyntaxTheme = null;
    syntaxEditErr = null;
  }
  async function saveCustomSyntaxTheme() {
    if (!editingSyntaxTheme) return;
    if (!editingSyntaxTheme.name.trim()) {
      syntaxEditErr = "名前を入力してください";
      return;
    }
    for (const { key } of SYNTAX_VAR_KEYS) {
      if (!HEX_RE.test(editingSyntaxTheme[key] ?? "")) {
        syntaxEditErr = `${syntaxColorLabels[key]}は #rrggbb 形式で入力してください`;
        return;
      }
    }
    const exists = customSyntaxThemes.some((t) => t.id === editingSyntaxTheme!.id);
    const next = exists
      ? customSyntaxThemes.map((t) => (t.id === editingSyntaxTheme!.id ? editingSyntaxTheme! : t))
      : [...customSyntaxThemes, editingSyntaxTheme];
    await app.setUiPrefs({ ...app.ui, customSyntaxThemes: next });
    editingSyntaxTheme = null;
    syntaxEditErr = null;
  }
  async function removeCustomSyntaxTheme(id: string) {
    const next = customSyntaxThemes.filter((t) => t.id !== id);
    const clearing = codeHighlightTheme === `custom:${id}`;
    await app.setUiPrefs({
      ...app.ui,
      customSyntaxThemes: next,
      codeHighlightTheme: clearing ? "auto" : app.ui.codeHighlightTheme,
    });
    if (clearing) codeHighlightTheme = "auto";
  }

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // このセクションが編集しないフィールド(レイアウト・背景等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        theme,
        codeHighlightTheme,
        fontFamily,
        emojiStyle,
        mfmAnimationEnabled,
        searchEngineUrl: searchEngineUrl.trim() || DEFAULT_SEARCH_ENGINE_URL,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<h3 class="mb-3.5 mt-0 text-base font-semibold">外観</h3>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">テーマ</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each themes as t (t.id)}
      <button
        type="button"
        class={theme === t.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
        onclick={() => (theme = t.id)}
      >{t.label}</button>
    {/each}
  </div>
</div>

{#snippet swatchStrip(colors: ThemeColors)}
  <span class="flex h-[30px] w-full flex-none">
    {#each THEME_VAR_KEYS as v (v.key)}
      <span class="h-full flex-1" style={`background:${colors[v.key]}`}></span>
    {/each}
  </span>
{/snippet}

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">プリセットテーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    {#each PRESETS as p (p.id)}
      {@const isActive = theme === `preset:${p.id}`}
      <button
        type="button"
        class={isActive
          ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-sm text-foreground shadow-[0_0_0_1px_var(--accent)]"
          : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-sm text-foreground hover:border-primary"}
        onclick={() => (theme = `preset:${p.id}`)}
      >
        {@render swatchStrip(p.colors)}
        <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
          {p.name}
          {#if isActive}<Check size={12} class="flex-none text-primary" />{/if}
        </span>
      </button>
    {/each}
  </div>
</div>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">カスタムテーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    {#each customThemes as t (t.id)}
      {@const isActive = theme === `custom:${t.id}`}
      <div class="flex flex-col gap-1">
        <button
          type="button"
          class={isActive
            ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-sm text-foreground shadow-[0_0_0_1px_var(--accent)]"
            : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-sm text-foreground hover:border-primary"}
          onclick={() => (theme = `custom:${t.id}`)}
        >
          {@render swatchStrip(t.colors)}
          <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
            {t.name}
            {#if isActive}<Check size={12} class="flex-none text-primary" />{/if}
          </span>
        </button>
        <div class="flex gap-1">
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="編集" onclick={() => startEditTheme(t)}><Pencil size={12} /></button>
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="削除" onclick={() => removeCustomTheme(t.id)}><Trash2 size={12} /></button>
        </div>
      </div>
    {/each}
  </div>
  <Button type="button" variant="outline" size="sm" class="mt-2" onclick={startCreateTheme}><Plus size={12} /> 新規作成</Button>

  {#if editingTheme}
    <div class="mt-2.5 flex flex-col gap-2 rounded-lg border border-border bg-muted p-3">
      <input type="text" class="rounded-md border border-border bg-background px-2.5 py-[7px] font-[inherit] text-foreground" placeholder="テーマ名" bind:value={editingTheme.name} />
      {#each THEME_VAR_KEYS as v (v.key)}
        <div class="flex items-center gap-2">
          <span class="w-20 flex-none text-sm text-muted-foreground">{colorLabels[v.key]}</span>
          <span class="h-[22px] w-[22px] flex-none rounded-md border border-border" style={`background:${editingTheme.colors[v.key]}`}></span>
          <input type="text" class="w-[100px] rounded-md border border-border bg-background px-2 py-[5px] font-[ui-monospace,monospace] text-sm text-foreground" bind:value={editingTheme.colors[v.key]} />
        </div>
      {/each}
      {#if editErr}<p class="mt-2 mb-0 text-sm text-destructive">{editErr}</p>{/if}
      <div class="mt-1 flex justify-end gap-2">
        <Button type="button" variant="outline" size="sm" onclick={cancelEditTheme}><X size={12} /> キャンセル</Button>
        <Button type="button" size="sm" onclick={saveCustomTheme}>このテーマを保存</Button>
      </div>
    </div>
  {/if}
</div>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">コードハイライトテーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    <button
      type="button"
      class={codeHighlightTheme === "auto"
        ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-sm text-foreground shadow-[0_0_0_1px_var(--accent)]"
        : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-sm text-foreground hover:border-primary"}
      onclick={() => (codeHighlightTheme = "auto")}
    >
      <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
        自動(OSに合わせる)
        {#if codeHighlightTheme === "auto"}<Check size={12} class="flex-none text-primary" />{/if}
      </span>
    </button>
    {#each BUNDLED_SHIKI_THEMES as t (t.id)}
      {@const isActive = codeHighlightTheme === `shiki:${t.id}`}
      <button
        type="button"
        class={isActive
          ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-sm text-foreground shadow-[0_0_0_1px_var(--accent)]"
          : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-sm text-foreground hover:border-primary"}
        onclick={() => (codeHighlightTheme = `shiki:${t.id}`)}
      >
        <span class="flex h-[30px] w-full flex-none">
          <span class="h-full flex-1" style={`background:${t.swatch.bg}`}></span>
          <span class="h-full flex-1" style={`background:${t.swatch.fg}`}></span>
          <span class="h-full flex-1" style={`background:${t.swatch.accent}`}></span>
        </span>
        <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
          {t.label}
          {#if isActive}<Check size={12} class="flex-none text-primary" />{/if}
        </span>
      </button>
    {/each}
  </div>
</div>

<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">カスタムシンタックステーマ</span>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2.5">
    {#each customSyntaxThemes as t (t.id)}
      {@const isActive = codeHighlightTheme === `custom:${t.id}`}
      <div class="flex flex-col gap-1">
        <button
          type="button"
          class={isActive
            ? "flex w-full flex-col overflow-hidden rounded-lg border border-primary bg-muted p-0 text-left text-sm text-foreground shadow-[0_0_0_1px_var(--accent)]"
            : "flex w-full flex-col overflow-hidden rounded-lg border border-border bg-muted p-0 text-left text-sm text-foreground hover:border-primary"}
          onclick={() => (codeHighlightTheme = `custom:${t.id}`)}
        >
          <span class="flex h-[30px] w-full flex-none">
            {#each SYNTAX_VAR_KEYS as v (v.key)}
              <span class="h-full flex-1" style={`background:${t[v.key]}`}></span>
            {/each}
          </span>
          <span class="flex items-center justify-between gap-1 px-2.5 py-[7px]">
            {t.name}
            {#if isActive}<Check size={12} class="flex-none text-primary" />{/if}
          </span>
        </button>
        <div class="flex gap-1">
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="編集" onclick={() => startEditSyntaxTheme(t)}><Pencil size={12} /></button>
          <button type="button" class="flex flex-1 items-center justify-center rounded-md border border-border bg-muted py-[5px] text-muted-foreground hover:border-primary hover:text-primary" title="削除" onclick={() => removeCustomSyntaxTheme(t.id)}><Trash2 size={12} /></button>
        </div>
      </div>
    {/each}
  </div>
  <Button type="button" variant="outline" size="sm" class="mt-2" onclick={startCreateSyntaxTheme}><Plus size={12} /> 新規作成</Button>

  {#if editingSyntaxTheme}
    <div class="mt-2.5 flex flex-col gap-2 rounded-lg border border-border bg-muted p-3">
      <input type="text" class="rounded-md border border-border bg-background px-2.5 py-[7px] font-[inherit] text-foreground" placeholder="テーマ名" bind:value={editingSyntaxTheme.name} />
      {#each SYNTAX_VAR_KEYS as v (v.key)}
        <div class="flex items-center gap-2">
          <span class="w-20 flex-none text-sm text-muted-foreground">{syntaxColorLabels[v.key]}</span>
          <span class="h-[22px] w-[22px] flex-none rounded-md border border-border" style={`background:${editingSyntaxTheme[v.key]}`}></span>
          <input type="text" class="w-[100px] rounded-md border border-border bg-background px-2 py-[5px] font-[ui-monospace,monospace] text-sm text-foreground" bind:value={editingSyntaxTheme[v.key]} />
        </div>
      {/each}
      {#if syntaxEditErr}<p class="mt-2 mb-0 text-sm text-destructive">{syntaxEditErr}</p>{/if}
      <div class="mt-1 flex justify-end gap-2">
        <Button type="button" variant="outline" size="sm" onclick={cancelEditSyntaxTheme}><X size={12} /> キャンセル</Button>
        <Button type="button" size="sm" onclick={saveCustomSyntaxTheme}>このテーマを保存</Button>
      </div>
    </div>
  {/if}
</div>

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
