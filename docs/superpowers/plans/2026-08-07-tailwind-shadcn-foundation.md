# Tailwind + shadcn-svelte 基盤導入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `frontend/`にTailwind CSS v4とshadcn-svelteの実行基盤(ビルド設定・トークンブリッジ・ダーク/ライト切替)を導入し、以降のコンポーネント移行タスクが`bg-background`/`text-foreground`等のshadcn標準クラスをそのまま使える状態にする。

**Architecture:** Tailwind v4は`@tailwindcss/vite`プラグイン経由でViteに統合する。既存のRust側`ThemeColors`データモデル・`settings.json`・`--surface-*`等のCSS変数は一切変更せず、`app.css`に追加する`@theme`ブロックでshadcn標準のCSS変数名を既存変数へマッピングする(CSS層のみのブリッジ)。ダーク/ライト切替は`data-theme`属性ベースから、shadcn-svelteの標準である`.dark`/`.light`クラスベースへ変更する。

**Tech Stack:** Tailwind CSS v4, `@tailwindcss/vite`, shadcn-svelte CLI, bits-ui, tailwind-merge, tailwind-variants, clsx

## Global Constraints

- Rust側 `ThemeColors` 構造体(`src-tauri/src/domain/ui.rs`)・`settings.json`のデータ形式・`CustomSyntaxTheme`は変更しない(specの「テーマ変数の移行」節参照)
- 既存の13プリセット(`frontend/src/lib/theme.ts`の`PRESETS`)・`ThemeColors`型・`applyThemeColors()`は変更しない
- auto(OS追従)/light/darkの3状態という挙動は変えない
- 本プランでは既存39コンポーネントのTailwindクラスへの書き換えは行わない(別プランで扱う)。本プランのゴールは基盤導入のみ

---

### Task 1: Tailwind CSS v4 のインストールとVite統合

**Files:**
- Modify: `frontend/package.json`
- Modify: `frontend/vite.config.ts`
- Modify: `frontend/src/app.css:1`(先頭に`@import`を追加)
- Test: 手動確認(下記Step参照)

**Interfaces:**
- Consumes: なし(このタスクが起点)
- Produces: `frontend/src/app.css`の先頭で`@import "tailwindcss";`が有効になり、以降のタスクで`@theme`ブロックやTailwindユーティリティクラスが使えるようになる

- [ ] **Step 1: パッケージを追加**

```bash
cd frontend
pnpm add -D tailwindcss @tailwindcss/vite
```

- [ ] **Step 2: `vite.config.ts`にプラグインを追加**

`frontend/vite.config.ts`の`import`群に以下を追加:

```ts
import tailwindcss from "@tailwindcss/vite";
```

`plugins: [svelte()],`を以下に変更:

```ts
plugins: [tailwindcss(), svelte()],
```

- [ ] **Step 3: `app.css`の先頭にTailwindのインポートを追加**

`frontend/src/app.css`の1行目(`:root {`の直前)に追加:

```css
@import "tailwindcss";

```

- [ ] **Step 4: dev サーバが起動し、Tailwindが読み込まれることを確認**

Run: `cd frontend && pnpm dev` を起動し、ブラウザで `http://127.0.0.1:5173` を開く。コンソールにTailwind関連のエラーが出ないこと、既存の画面がこれまで通り表示されることを確認する(この時点ではまだTailwindクラスをどこにも使っていないため、見た目の変化はない)。確認後 `Ctrl+C` で停止する。

- [ ] **Step 5: Commit**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml frontend/vite.config.ts frontend/src/app.css
git commit -m "build: Tailwind CSS v4をViteに統合"
```

---

### Task 2: shadcn-svelte の初期化

**Files:**
- Create: `frontend/components.json`
- Create: `frontend/src/lib/utils.ts`
- Modify: `frontend/package.json`
- Modify: `frontend/tsconfig.app.json`(パスエイリアスが無ければ追加)

**Interfaces:**
- Consumes: Task 1で導入したTailwind環境
- Produces: `cn(...)`ヘルパー関数(`frontend/src/lib/utils.ts`からexport、シグネチャ`cn(...inputs: ClassValue[]): string`)。以降のタスク・shadcn-svelte CLIで追加するコンポーネントがこれを使う

- [ ] **Step 1: 依存パッケージを追加**

```bash
cd frontend
pnpm add clsx tailwind-merge tailwind-variants
pnpm add -D shadcn-svelte
```

- [ ] **Step 2: パスエイリアスを確認**

`frontend/tsconfig.app.json`を開き、`compilerOptions.paths`に`"$lib/*": ["./src/lib/*"]`相当のエイリアスが無ければ追加する(shadcn-svelte CLIが生成するコンポーネントの import 解決に使われる)。既存の`vite.config.ts`にも`resolve.alias`で同じエイリアスを追加する:

```ts
import path from "node:path";
// ...
export default defineConfig({
  // ...
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, "./src/lib"),
    },
  },
});
```

- [ ] **Step 3: shadcn-svelte CLIを初期化**

```bash
cd frontend
pnpm dlx shadcn-svelte@latest init
```

対話プロンプトには以下で回答する:
- Base color: `Neutral`(色自体は`@theme`ブリッジで上書きするため、初期値は何でもよい)
- Global CSS file: `src/app.css`
- Tailwind config: (v4のためconfigファイルなし、そのまま進める)
- Import alias for components: `$lib/components`
- Import alias for utils: `$lib/utils`

CLIが `frontend/components.json` と `frontend/src/lib/utils.ts`(`cn()`関数)を生成する。

- [ ] **Step 4: 生成された`utils.ts`の内容を確認**

`frontend/src/lib/utils.ts`が以下と同等の内容になっていることを確認する:

```ts
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
```

- [ ] **Step 5: `pnpm check`が通ることを確認**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 6: Commit**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml frontend/components.json frontend/src/lib/utils.ts frontend/tsconfig.app.json frontend/vite.config.ts
git commit -m "build: shadcn-svelteを初期化"
```

---

### Task 3: shadcn標準トークンをCSS変数ブリッジで定義

**Files:**
- Modify: `frontend/src/app.css`(Task 1で追加した`@import`の直後、既存の`:root { ... }`ブロックの後に`@theme`ブロックを追加)

**Interfaces:**
- Consumes: Task 1の`@import "tailwindcss";`
- Produces: `bg-background`, `text-foreground`, `bg-card`, `bg-popover`, `bg-primary`, `text-primary-foreground`, `bg-secondary`, `text-secondary-foreground`, `bg-muted`, `text-muted-foreground`, `bg-accent`, `text-accent-foreground`, `bg-destructive`, `text-destructive-foreground`, `border-border`, `border-input`, `ring-ring` の各Tailwindユーティリティクラスが、既存の`--surface-*`等のCSS変数値を参照して機能するようになる。以降のコンポーネント移行タスクはこれらのクラス名を使う

既存の`app.css`の`:root { ... }`ブロック(`--surface-1`から`--warning`までを含む、1〜24行目付近)は変更しない。その直後に以下を追加する:

- [ ] **Step 1: `@theme`ブロックを追加**

```css
@theme {
  --color-background: var(--surface-1);
  --color-foreground: var(--text);

  --color-card: var(--surface-2);
  --color-card-foreground: var(--text);

  --color-popover: var(--surface-3);
  --color-popover-foreground: var(--text);

  --color-primary: var(--accent);
  --color-primary-foreground: var(--surface-1);

  --color-secondary: var(--surface-3);
  --color-secondary-foreground: var(--text);

  --color-muted: var(--surface-2);
  --color-muted-foreground: var(--text-dim);

  --color-accent: var(--surface-3);
  --color-accent-foreground: var(--text);

  --color-destructive: var(--danger);
  --color-destructive-foreground: var(--surface-1);

  --color-border: var(--border);
  --color-input: var(--border);
  --color-ring: var(--accent);
}
```

補足: Tailwindのユーティリティ名`accent`とアプリ既存の`--accent`(強調色/リンク色)は意味が異なる。shadcn標準の`accent`はホバー時のハイライト背景等に使う中立色を指すため、ここでは`--surface-3`をマッピングしている。アプリの強調色(既存`--accent`)は`--color-primary`側にマッピングされている。この対応は今後のコンポーネント移行タスクで実際に使ってみて調整が必要になる可能性があるため、違和感があれば移行タスクの中で本タスクのマッピングに立ち戻って調整してよい。

- [ ] **Step 2: dev サーバでビルドが通ることを確認**

Run: `cd frontend && pnpm dev` を起動し、ブラウザのコンソールにCSSパースエラーが出ないこと、既存画面の見た目に変化がないこと(まだどのコンポーネントも新クラスを使っていないため)を確認する。確認後停止する。

- [ ] **Step 3: 簡単な動作確認用の一時マークアップで検証**

`frontend/src/App.svelte`(または現在のルートコンポーネント)の末尾に一時的に以下を追加して`pnpm dev`で表示を確認する:

```svelte
<div class="bg-primary text-primary-foreground p-4 rounded">shadcn token test</div>
```

`--accent`の色(紫系, `#7c5cff`)の背景に白系の文字で表示されることを確認したら、この一時マークアップを削除する。

- [ ] **Step 4: `pnpm check`が通ることを確認**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 5: Commit**

```bash
git add frontend/src/app.css
git commit -m "style: shadcn標準トークンを既存CSS変数にブリッジする@themeを追加"
```

---

### Task 4: ダーク/ライト切替を `data-theme` 属性から `.dark`/`.light` クラスへ変更

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:1269-1282`(`#applyTheme`メソッド)
- Modify: `frontend/src/app.css:28-46`(auto-dark媒体クエリ、明示dark指定のセレクタ)
- Modify: `frontend/src/app.css:183-198`(shikiコードブロックのdata-theme参照セレクタ)
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: なし(Task 1〜3と独立に実施可能)
- Produces: `<html>`要素に`theme === "dark"`のとき`dark`クラス、`theme === "light"`のとき`light`クラスが付与される(`auto`のときはどちらも付与されない)。今後のコンポーネント移行タスクやshadcn-svelte生成コンポーネントは、標準的な`dark:`バリアント記法(例: `dark:bg-zinc-900`)がこの`.dark`クラスを前提に動く

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts`の末尾に追加(既存の`import`群・`ACCOUNT_ID`等はファイル冒頭で定義済みのものを使う):

```ts
describe("#applyTheme (Issue #170: data-theme属性から.darkクラスへ移行)", () => {
  afterEach(() => {
    document.documentElement.classList.remove("dark", "light");
  });

  it("theme='dark'のとき<html>にdarkクラスが付与される", async () => {
    await app.setUiPrefs({ ...app.ui, theme: "dark" });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.classList.contains("light")).toBe(false);
  });

  it("theme='light'のとき<html>にlightクラスが付与される", async () => {
    await app.setUiPrefs({ ...app.ui, theme: "light" });
    expect(document.documentElement.classList.contains("light")).toBe(true);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("theme='auto'のとき<html>にdark/lightどちらのクラスも付与されない", async () => {
    await app.setUiPrefs({ ...app.ui, theme: "dark" });
    await app.setUiPrefs({ ...app.ui, theme: "auto" });
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.documentElement.classList.contains("light")).toBe(false);
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cd frontend && pnpm test -- store.svelte.test.ts`
Expected: 上記3つのテストがFAIL(現状は`dataset.theme`を設定するだけで`classList`は変化しないため)

- [ ] **Step 3: `#applyTheme`を実装変更**

`frontend/src/lib/store.svelte.ts`の`#applyTheme`メソッド冒頭を変更する。変更前:

```ts
  #applyTheme(theme: string) {
    const root = document.documentElement;
    if (theme === "light" || theme === "dark") {
      root.dataset.theme = theme;
    } else {
      delete root.dataset.theme;
    }
```

変更後:

```ts
  #applyTheme(theme: string) {
    const root = document.documentElement;
    root.classList.remove("light", "dark");
    if (theme === "light" || theme === "dark") {
      root.classList.add(theme);
    }
```

(このメソッドの以降の行 ― `parseThemeRef`によるpreset/custom判定とapplyThemeColors呼び出し ― は変更しない)

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cd frontend && pnpm test -- store.svelte.test.ts`
Expected: PASS

- [ ] **Step 5: `app.css`のセレクタを書き換え**

`frontend/src/app.css`の28〜46行目付近、変更前:

```css
/* data-theme 未指定(=auto)のときのみ OS 設定に追従。明示指定は下の [data-theme] が優先。 */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --surface-1: #16181d;
    --surface-2: #1d2026;
    --surface-3: #2a2e37;
    --border: #2c313b;
    --text: #e2dfec;
    --text-dim: #9aa1ac;
    --accent: #8b6dff;
  }
}
/* テーマを明示的に dark 指定した場合は OS 設定に関わらずダーク */
:root[data-theme="dark"] {
  --surface-1: #16181d;
  --surface-2: #1d2026;
  --surface-3: #2a2e37;
  --border: #2c313b;
  --text: #e2dfec;
  --text-dim: #9aa1ac;
  --accent: #8b6dff;
}
```

変更後:

```css
/* .light/.dark 未指定(=auto)のときのみ OS 設定に追従。明示指定は下の .dark が優先。 */
@media (prefers-color-scheme: dark) {
  :root:not(.light) {
    --surface-1: #16181d;
    --surface-2: #1d2026;
    --surface-3: #2a2e37;
    --border: #2c313b;
    --text: #e2dfec;
    --text-dim: #9aa1ac;
    --accent: #8b6dff;
  }
}
/* テーマを明示的に dark 指定した場合は OS 設定に関わらずダーク */
:root.dark {
  --surface-1: #16181d;
  --surface-2: #1d2026;
  --surface-3: #2a2e37;
  --border: #2c313b;
  --text: #e2dfec;
  --text-dim: #9aa1ac;
  --accent: #8b6dff;
}
```

- [ ] **Step 6: shikiコードブロックのセレクタを書き換え**

`frontend/src/app.css`の183〜198行目付近、変更前:

```css
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) .mfm-codeblock .shiki,
  :root:not([data-theme="light"]) .mfm-codeblock .shiki span {
    color: var(--shiki-dark);
  }
  :root:not([data-theme="light"]) .mfm-codeblock .shiki {
    background-color: var(--shiki-dark-bg);
  }
}
:root[data-theme="dark"] .mfm-codeblock .shiki,
:root[data-theme="dark"] .mfm-codeblock .shiki span {
  color: var(--shiki-dark);
}
:root[data-theme="dark"] .mfm-codeblock .shiki {
  background-color: var(--shiki-dark-bg);
}
```

変更後:

```css
@media (prefers-color-scheme: dark) {
  :root:not(.light) .mfm-codeblock .shiki,
  :root:not(.light) .mfm-codeblock .shiki span {
    color: var(--shiki-dark);
  }
  :root:not(.light) .mfm-codeblock .shiki {
    background-color: var(--shiki-dark-bg);
  }
}
:root.dark .mfm-codeblock .shiki,
:root.dark .mfm-codeblock .shiki span {
  color: var(--shiki-dark);
}
:root.dark .mfm-codeblock .shiki {
  background-color: var(--shiki-dark-bg);
}
```

- [ ] **Step 7: 同じパターンの他の参照箇所がないか確認**

Run: `cd frontend && grep -rn "data-theme" src`
Expected: 0件(コメント含め全て置き換わっていること)

- [ ] **Step 8: 全テストを実行**

Run: `cd frontend && pnpm test`
Expected: 全テストPASS

- [ ] **Step 9: `pnpm check`を実行**

Run: `cd frontend && pnpm check`
Expected: エラーなく完了する

- [ ] **Step 10: `cargo tauri dev`で目視確認**

設定画面からテーマを auto → light → dark → auto と切り替え、それぞれ画面全体の配色が正しく切り替わることを確認する。プリセット(例: Dracula)とカスタムテーマ(作成済みのものがあれば)も選択して配色が反映されることを確認する。

- [ ] **Step 11: Commit**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts frontend/src/app.css
git commit -m "refactor: テーマ切替をdata-theme属性から.dark/.lightクラスに変更"
```

---

## 本プラン完了後の進め方

本プランの完了時点で、既存39コンポーネントは(スタイル的には)まだ移行前の状態のまま動作する(Task 3は既存CSS変数をブリッジしただけで、既存コンポーネントのCSSは変更していない)。

次のステップとして、39コンポーネントをTailwindクラスへ書き換えるプランを、画面領域ごとに分割して個別に作成する(モーダル群/ノート・通知表示/入力系ウィジェット/設定画面/レイアウト系、など)。各書き換えプランは、対象コンポーネントの現在の`<style>`ブロックを実際に読んでから、正確なクラス名で作成する。

`components.json`の`"style": "vega"`は、Task 2のCLI事故の原因になったプリセットだが、出力(生成物)のみ復元してプリセット自体は変更していない。次のプランで`shadcn-svelte add <component>`のような形で初めてshadcn UIプリミティブを追加する前に、まずscratchワークツリーで生成されたコンポーネントのトークン参照が`app.css`の`@theme`ブリッジと一致するか検証すること。
