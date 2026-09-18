# 設定画面タブ分類見直し(Issue #326) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定画面の12タブを性質軸で再分類する（外観タブに混在していた外部連携系設定を分離、データ/キャッシュバックエンドを統合、デバッグ設定を新設の「開発者オプション」タブへ移動し、Tsumugiについてタブのバージョン表示連打で解除する隠し機能を追加する）。

**Architecture:** Rust側`UiPrefs`(`src-tauri/src/domain/ui.rs`)に`developer_options_enabled: bool`を1フィールド追加するのみ。既存の`setUiPrefs`コマンド・`app.ui`ストアの仕組みをそのまま流用し、新規Tauriコマンドは作らない。フロントエンドはSvelteコンポーネントの分割・移動が中心で、既存の「各セクションが`app.ui`を読み書きし、保存ボタンで`app.setUiPrefs({...app.ui, 編集項目})`を呼ぶ」パターンをそのまま踏襲する。

**Tech Stack:** Rust (Tauri v2, specta), Svelte 5 (runes), Vitest + @testing-library/svelte

## Global Constraints

- 設計書: `docs/superpowers/specs/2026-09-18-settings-tabs-reclassification-design.md`（このPlanの元になった設計。矛盾があれば設計書を優先して確認する）
- `app.ui`の既存フィールド（`noteCacheLimit`, `enableFileLogging`, `searchEngineUrl`, `urlPreviewEnabled`, `summalyProxyUrl`等）はキー名・型とも変更しない。UI上でどのコンポーネントから編集するかのみを変える。
- 新規Tauriコマンドは追加しない。既存の`setUiPrefs` / `load_ui`のみを使う。
- 開発者オプションタブの解除は一方向（無効化する手段は実装しない）。
- コミットメッセージは1行（件名のみ）。末尾に`Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`を付与する。
- 各タスックの最後にコミットする。

---

## File Structure

**Rust:**
- Modify: `src-tauri/src/domain/ui.rs` — `UiPrefs`に`developer_options_enabled: bool`を追加

**Frontend（新規）:**
- Create: `frontend/src/ui/settings/ExternalIntegrationSection.svelte` — MFM検索エンジンURL・URLプレビュー(summalyプロキシ)
- Create: `frontend/src/ui/settings/DeveloperSection.svelte` — 動作ログをファイルに残す設定
- Create: `frontend/src/ui/settings/AboutSection.test.ts` — バージョン表示連打での開発者オプション解除のテスト
- Create: `frontend/src/ui/Settings.test.ts` — タブ構成(表示/非表示)のテスト

**Frontend（変更）:**
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte` — MFM検索エンジンURL・URLプレビュー関連を削除（MFMアニメーションは残す）
- Modify: `frontend/src/ui/settings/DataSection.svelte` — デバッグ項目（`enableFileLogging`）を削除
- Modify: `frontend/src/ui/settings/AboutSection.svelte` — バージョン表示7回タップで開発者オプション解除
- Modify: `frontend/src/ui/Settings.svelte` — タブ構成変更（`externalIntegration`追加、`cacheBackend`廃止・`data`タブへ統合、`developer`を条件付き追加）

**ドキュメント:**
- Modify: `docs/guide/user-guide.md` — 「設定画面」節を新タブ構成に更新

**変更しないもの:**
- `frontend/src/ui/settings/CacheBackendSettings.svelte`（中身は変更せず、`Settings.svelte`から`DataSection`と並べて描画するだけ）
- `frontend/src/ui/settings/LayoutSection.svelte`, `BackgroundSection.svelte`, `ReactionSection.svelte`, `MobileSection.svelte`, `NotifySection.svelte`, `MuteSection.svelte`, `KeysSection.svelte`, `AccountsSection.svelte`

---

### Task 1: Rust — UiPrefsに開発者オプション解除フラグを追加

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `UiPrefs.developer_options_enabled: bool`（serde経由で`developerOptionsEnabled`としてTSへ公開。デフォルト`false`）

- [ ] **Step 1: 失敗するテストを追加する**

`src-tauri/src/domain/ui.rs`の`mod tests`内、`url_preview_enabled_defaults_to_true_for_legacy_json`テストの直後に新しいテストを追加する:

```rust
    #[test]
    fn developer_options_enabled_defaults_to_false_for_legacy_json() {
        // developer_options_enabled 追加前に保存されたJSONも読めること（#[serde(default)]）。
        // 既定はOFF(追加前は開発者オプションタブが存在しなかった挙動を維持)。
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.developer_options_enabled, false);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test developer_options_enabled_defaults_to_false_for_legacy_json`
Expected: コンパイルエラー（`UiPrefs`に`developer_options_enabled`フィールドが存在しない）

- [ ] **Step 3: `UiPrefs`にフィールドを追加する**

`src-tauri/src/domain/ui.rs`の`UiPrefs`構造体末尾（`haptics_enabled: bool,`の直後、構造体を閉じる`}`の直前）に追加:

```rust
    /// 隠し機能「開発者オプション」タブの解除状態（Issue #326）。
    /// 「Tsumugiについて」タブのバージョン表示を7回タップすると true になり、以後タブを表示し続ける。
    /// 一度trueになったら無効化する手段は用意しない（Androidの開発者向けオプション解除と同様）。
    #[serde(default)]
    pub developer_options_enabled: bool,
}
```

（元の`pub haptics_enabled: bool,\n}`を上記に置き換える形。既存の閉じ`}`を1つ後ろにずらす。）

`impl Default for UiPrefs`内、`haptics_enabled: default_haptics_enabled(),`の直後に追加:

```rust
            haptics_enabled: default_haptics_enabled(),
            developer_options_enabled: false,
        }
    }
}
```

- [ ] **Step 4: 既存の`roundtrips_keymap`テストの構造体リテラルにフィールドを追加する**

`src-tauri/src/domain/ui.rs`の`roundtrips_keymap`テスト内、`haptics_enabled: true,`の直後に追加:

```rust
            haptics_enabled: true,
            developer_options_enabled: true,
        };
```

- [ ] **Step 5: `deserializes_legacy_json_without_new_fields`テストにもアサーションを追加する**

同テスト内、`assert!(v.haptics_enabled);`の直後に追加:

```rust
        assert!(v.haptics_enabled);
        // developer_options_enabled も同様に既定値(false, 追加前は開発者オプションタブ自体が
        // 存在しなかった)へフォールバックすること。
        assert_eq!(v.developer_options_enabled, false);
    }
```

- [ ] **Step 6: テストを実行してすべて通ることを確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS（`domain::ui::tests`配下のテスト全件、および`generates_frontend_bindings`を含む全テスト）

このコマンドの実行により`frontend/src/bindings/tauri.gen.ts`が自動的に再生成され、`developerOptionsEnabled?: boolean`が`UiPrefs`型に追加される。

- [ ] **Step 7: 差分を確認してコミットする**

Run: `git diff --stat`
Expected: `src-tauri/src/domain/ui.rs`と`frontend/src/bindings/tauri.gen.ts`の2ファイルのみ変更されている

```bash
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsに開発者オプション解除フラグを追加"
```

---

### Task 2: 「外部連携」タブの新設とMFM検索エンジン/URLプレビュー設定の移動

**Files:**
- Create: `frontend/src/ui/settings/ExternalIntegrationSection.svelte`
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`

**Interfaces:**
- Produces: `ExternalIntegrationSection`コンポーネント（props無し。`app.ui.searchEngineUrl` / `app.ui.urlPreviewEnabled` / `app.ui.summalyProxyUrl`を編集して`app.setUiPrefs`で保存する）
- Consumes: `app`（`../../lib/store.svelte`）, `SEARCH_ENGINE_PRESETS` / `DEFAULT_SEARCH_ENGINE_URL`（`../../lib/searchEngine`）, `Button`（`$lib/components/ui/button`）

このタスクは既存UIの移設のみで、新規のロジックは追加しない。UIコンポーネントのテストは既存の`AppearanceSection.svelte`にも専用テストが無く、本タスクでも作らない（既存コードベースの慣習に合わせる）。移設漏れが無いことは Task 6 での`pnpm check`（型チェック）と手動確認で担保する。

- [ ] **Step 1: `ExternalIntegrationSection.svelte`を新規作成する**

`frontend/src/ui/settings/ExternalIntegrationSection.svelte`を新規作成:

```svelte
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
```

- [ ] **Step 2: `AppearanceSection.svelte`からimportを削除する**

`frontend/src/ui/settings/AppearanceSection.svelte`の以下の行を削除する:

```
  import { SEARCH_ENGINE_PRESETS, DEFAULT_SEARCH_ENGINE_URL } from "../../lib/searchEngine";
```

- [ ] **Step 3: `AppearanceSection.svelte`の状態変数を削除する**

以下の3行を削除する（`mfmAnimationEnabled`の宣言と`instanceTicker`の宣言の間にある）:

```
  let searchEngineUrl = $state(app.ui.searchEngineUrl ?? DEFAULT_SEARCH_ENGINE_URL);
  let urlPreviewEnabled = $state(app.ui.urlPreviewEnabled ?? true);
  let summalyProxyUrl = $state(app.ui.summalyProxyUrl ?? "");
```

- [ ] **Step 4: `AppearanceSection.svelte`の`save()`内から該当フィールドを削除する**

`save()`関数内の`await app.setUiPrefs({...})`オブジェクトから以下の3行を削除する:

```
        searchEngineUrl: searchEngineUrl.trim() || DEFAULT_SEARCH_ENGINE_URL,
        urlPreviewEnabled,
        summalyProxyUrl: summalyProxyUrl.trim(),
```

（`mfmAnimationEnabled,`と`instanceTicker,`は残す）

- [ ] **Step 5: `AppearanceSection.svelte`のテンプレートから該当UIブロックを削除する**

MFMアニメーションのチェックボックス・説明文の直後、フォント設定の`<div>`の直前にある以下のブロック全体（検索エンジンの`<div>`とURLプレビューの`<div>`、間の空行含む）を削除する:

```
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
```

削除後は、MFMアニメーションの説明文`</p>`の直後に空行1つを挟んで、そのままフォント設定の`<div class="mb-3 flex flex-col gap-1.5 text-sm">`（`<span class="text-muted-foreground">フォント</span>`）が続く形になる。

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: PASS（未使用importや未定義変数の参照エラーが無いこと）

- [ ] **Step 7: コミットする**

```bash
git add frontend/src/ui/settings/ExternalIntegrationSection.svelte frontend/src/ui/settings/AppearanceSection.svelte
git commit -m "feat: 外部連携タブを新設しMFM検索エンジン/URLプレビュー設定を移動"
```

---

### Task 3: デバッグ設定の「開発者オプション」タブへの移動

**Files:**
- Create: `frontend/src/ui/settings/DeveloperSection.svelte`
- Modify: `frontend/src/ui/settings/DataSection.svelte`

**Interfaces:**
- Produces: `DeveloperSection`コンポーネント（props無し。`app.ui.enableFileLogging`を編集して`app.setUiPrefs`で保存する）

- [ ] **Step 1: `DeveloperSection.svelte`を新規作成する**

`frontend/src/ui/settings/DeveloperSection.svelte`を新規作成:

```svelte
<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";

  let enableFileLogging = $state(app.ui.enableFileLogging ?? false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // このセクションが編集しないフィールド(データ・外観等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
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

<h3 class="mb-3.5 mt-0 text-base font-semibold">開発者オプション</h3>

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
```

- [ ] **Step 2: `DataSection.svelte`から`enableFileLogging`の状態変数を削除する**

`frontend/src/ui/settings/DataSection.svelte`の以下の行を削除する:

```
  let enableFileLogging = $state(app.ui.enableFileLogging ?? false);
```

- [ ] **Step 3: `DataSection.svelte`の`save()`内から該当フィールドを削除する**

`save()`関数内の`await app.setUiPrefs({...})`オブジェクトから以下の行を削除する:

```
        enableFileLogging,
```

- [ ] **Step 4: `DataSection.svelte`のテンプレートから「デバッグ」見出しとUIブロックを削除する**

以下のブロック全体（`<h4>`見出しから`</p>`まで、前後の空行を含めて1つ分）を削除する:

```
<h4 class="mb-2 mt-0 text-sm font-semibold text-muted-foreground">デバッグ</h4>

<label class="mb-2 flex items-center gap-2 text-sm"><input type="checkbox" bind:checked={enableFileLogging} /> 動作ログをファイルに残す(デバッグ用)</label>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  WebSocket再接続やpingタイムアウトなどの内部ログを、アプリのログディレクトリにファイルとして
  永続化します。通知が来るタイミングがおかしい等の不具合調査用で、既定はOFFです。
  切り替えは次回起動から反映されます。
</p>
```

削除後、`<h4 ... ノートキャッシュ</h4>`配下の項目群の直後に、そのまま`<div class="flex items-center justify-end gap-3">`（保存ボタン）が続く形になる。

- [ ] **Step 5: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 6: コミットする**

```bash
git add frontend/src/ui/settings/DeveloperSection.svelte frontend/src/ui/settings/DataSection.svelte
git commit -m "feat: デバッグ設定を開発者オプションセクションへ分離"
```

---

### Task 4: 「Tsumugiについて」タブのバージョン表示連打で開発者オプションを解除する

**Files:**
- Modify: `frontend/src/ui/settings/AboutSection.svelte`
- Test: `frontend/src/ui/settings/AboutSection.test.ts`

**Interfaces:**
- Consumes: `app.ui.developerOptionsEnabled`（Task 1で追加）, `app.setUiPrefs`
- Produces: バージョン表示に`data-testid="settings-version-tap-target"`を持つ`<button>`。7回クリックで`app.setUiPrefs({ ...app.ui, developerOptionsEnabled: true })`を呼び、以後のクリックでは何もしない。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/settings/AboutSection.test.ts`を新規作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("9.9.9") }));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: AboutSection } = await import("./AboutSection.svelte");
const { app } = await import("../../lib/store.svelte");

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.ui = { ...app.ui, developerOptionsEnabled: false };
});

async function tap(target: HTMLElement, times: number) {
  for (let i = 0; i < times; i++) {
    await fireEvent.click(target);
  }
}

describe("AboutSection 開発者オプション解除(Issue #326)", () => {
  it("バージョン表示を7回タップするとdeveloperOptionsEnabled:trueで保存する", async () => {
    const { getByTestId } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 7);

    expect(invokeMock).toHaveBeenCalledWith(
      "set_ui_prefs",
      expect.objectContaining({ prefs: expect.objectContaining({ developerOptionsEnabled: true }) }),
    );
  });

  it("6回のタップでは保存が発生しない", async () => {
    const { getByTestId } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 6);

    expect(invokeMock).not.toHaveBeenCalledWith("set_ui_prefs", expect.anything());
  });

  it("7回タップすると有効化メッセージを表示する", async () => {
    const { getByTestId, findByText } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 7);

    expect(await findByText("開発者オプションを有効にしました")).toBeTruthy();
  });

  it("既に有効な場合はタップしても再度保存しない", async () => {
    app.ui = { ...app.ui, developerOptionsEnabled: true };
    const { getByTestId } = render(AboutSection);
    await tap(getByTestId("settings-version-tap-target"), 7);

    expect(invokeMock).not.toHaveBeenCalledWith("set_ui_prefs", expect.anything());
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/ui/settings/AboutSection.test.ts`
Expected: FAIL（`getByTestId("settings-version-tap-target")`が見つからない）

- [ ] **Step 3: `AboutSection.svelte`にタップ検知を実装する**

`frontend/src/ui/settings/AboutSection.svelte`の`<script>`部分を以下に置き換える:

```svelte
<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { commands } from "../../bindings/tauri.gen";
  import { app } from "../../lib/store.svelte";

  const REPO_URL = "https://github.com/onodai145/tsumugi";
  const DEVELOPER_OPTIONS_TAP_THRESHOLD = 7;

  let appVersion = $state<string | null>(null);
  let commitHash = $state<string | null>(null);
  let versionTapCount = $state(0);
  let developerOptionsJustUnlocked = $state(false);

  $effect(() => {
    void getVersion().then((v) => (appVersion = v));
    void commands.gitCommitHash().then((v) => (commitHash = v));
    void app.checkForUpdate();
  });

  // Androidの「ビルド番号連打」を模した隠し機能(Issue #326)。バージョン表示を7回タップすると
  // 「開発者オプション」タブが恒久的に出現する(無効化する手段は用意しない)。
  async function onVersionTap() {
    if (app.ui.developerOptionsEnabled) return;
    versionTapCount += 1;
    if (versionTapCount < DEVELOPER_OPTIONS_TAP_THRESHOLD) return;
    await app.setUiPrefs({ ...app.ui, developerOptionsEnabled: true });
    developerOptionsJustUnlocked = true;
  }
</script>
```

- [ ] **Step 4: テンプレートのバージョン表示をタップ可能なボタンに変更する**

`<dt class="text-sm text-muted-foreground">バージョン</dt>`の直後の行を置き換える:

置き換え前:
```
    <dt class="text-sm text-muted-foreground">バージョン</dt>
    <dd class="m-0 break-all text-sm">{appVersion ?? "…"}</dd>
```

置き換え後:
```
    <dt class="text-sm text-muted-foreground">バージョン</dt>
    <dd class="m-0 break-all text-sm">
      <button
        type="button"
        class="border-0 bg-transparent p-0 text-left font-[inherit] text-sm text-foreground"
        data-testid="settings-version-tap-target"
        onclick={onVersionTap}
      >{appVersion ?? "…"}</button>
    </dd>
```

- [ ] **Step 5: 有効化メッセージの表示を追加する**

`</dl>`の直後（`</div>`の直前）に追加:

```
  {#if developerOptionsJustUnlocked}
    <p class="mt-3 mb-0 text-sm text-[var(--success)]">開発者オプションを有効にしました</p>
  {/if}
```

- [ ] **Step 6: テストを実行して通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/settings/AboutSection.test.ts`
Expected: PASS（4件すべて）

- [ ] **Step 7: コミットする**

```bash
git add frontend/src/ui/settings/AboutSection.svelte frontend/src/ui/settings/AboutSection.test.ts
git commit -m "feat: バージョン表示7回タップで開発者オプションを解除する"
```

---

### Task 5: `Settings.svelte`のタブ構成を更新する

**Files:**
- Modify: `frontend/src/ui/Settings.svelte`
- Test: `frontend/src/ui/Settings.test.ts`

**Interfaces:**
- Consumes: `app.ui.developerOptionsEnabled`（Task 1）, `ExternalIntegrationSection`（Task 2）, `DeveloperSection`（Task 3）, 既存の`DataSection` / `CacheBackendSettings`
- Produces: `Section`型に`"externalIntegration"`と`"developer"`を追加、`"cacheBackend"`を削除。nav配列は`app.ui.developerOptionsEnabled`に応じて`developer`タブの有無を動的に切り替える（`$derived`化）。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/Settings.test.ts`を新規作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("9.9.9") }));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: Settings } = await import("./Settings.svelte");
const { app } = await import("../lib/store.svelte");

function renderSettings() {
  return render(Settings, {
    props: { onclose: () => {}, onAddAccount: () => {}, onReauth: () => {} },
  });
}

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.ui = { ...app.ui, developerOptionsEnabled: false };
});

describe("Settings タブ構成(Issue #326)", () => {
  it("外部連携タブを表示し、キャッシュバックエンドタブは単独では表示しない", () => {
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-externalIntegration")).not.toBeNull();
    expect(queryByTestId("settings-tab-data")).not.toBeNull();
    expect(queryByTestId("settings-tab-cacheBackend")).toBeNull();
  });

  it("開発者オプション未解除時は「開発者オプション」タブを表示しない", () => {
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-developer")).toBeNull();
  });

  it("開発者オプション解除後は「開発者オプション」タブを表示する", () => {
    app.ui = { ...app.ui, developerOptionsEnabled: true };
    const { queryByTestId } = renderSettings();
    expect(queryByTestId("settings-tab-developer")).not.toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/ui/Settings.test.ts`
Expected: FAIL（`settings-tab-externalIntegration`が存在しない、`settings-tab-cacheBackend`が存在してしまっている 等）

- [ ] **Step 3: `Settings.svelte`のimportを更新する**

`frontend/src/ui/Settings.svelte`の`<script>`冒頭を以下に置き換える:

```svelte
<script lang="ts">
  import type { Account } from "../bindings/tauri.gen";
  import NotifySection from "./settings/NotifySection.svelte";
  import MuteSection from "./settings/MuteSection.svelte";
  import LayoutSection from "./settings/LayoutSection.svelte";
  import MobileSection from "./settings/MobileSection.svelte";
  import AppearanceSection from "./settings/AppearanceSection.svelte";
  import BackgroundSection from "./settings/BackgroundSection.svelte";
  import ReactionSection from "./settings/ReactionSection.svelte";
  import ExternalIntegrationSection from "./settings/ExternalIntegrationSection.svelte";
  import DataSection from "./settings/DataSection.svelte";
  import CacheBackendSettings from "./settings/CacheBackendSettings.svelte";
  import DeveloperSection from "./settings/DeveloperSection.svelte";
  import AccountsSection from "./settings/AccountsSection.svelte";
  import KeysSection from "./settings/KeysSection.svelte";
  import AboutSection from "./settings/AboutSection.svelte";
  import Modal from "./Modal.svelte";
  import { isMobilePlatform } from "../lib/platform";
  import { app } from "../lib/store.svelte";

  type Section =
    | "accounts"
    | "layout"
    | "mobile"
    | "appearance"
    | "background"
    | "reaction"
    | "externalIntegration"
    | "data"
    | "notify"
    | "mute"
    | "keys"
    | "about"
    | "developer";

  let {
    onclose,
    onAddAccount,
    onReauth,
    initial = "notify",
  }: {
    onclose: () => void;
    onAddAccount: () => void;
    onReauth: (account: Account) => void;
    initial?: Section;
  } = $props();

  // モバイル(振動等)はデスクトップでは意味を持たないタブ自体を隠す。
  // 開発者オプションは「Tsumugiについて」タブのバージョン表示を7回タップして解除するまで隠す
  // (Issue #326)。app.ui.developerOptionsEnabled は設定モーダルを開いたまま解除されうるため
  // $derived で再計算させる(constでは解除後もタブが出ない)。
  const nav = $derived([
    { id: "accounts" as const, label: "アカウント" },
    { id: "layout" as const, label: "レイアウト" },
    ...(isMobilePlatform ? [{ id: "mobile" as const, label: "モバイル" }] : []),
    { id: "appearance" as const, label: "外観" },
    { id: "background" as const, label: "背景" },
    { id: "reaction" as const, label: "リアクション" },
    { id: "externalIntegration" as const, label: "外部連携" },
    { id: "data" as const, label: "データ" },
    { id: "notify" as const, label: "通知" },
    { id: "mute" as const, label: "NG（ミュート）" },
    { id: "keys" as const, label: "キー操作" },
    { id: "about" as const, label: "Tsumugiについて" },
    ...(app.ui.developerOptionsEnabled ? [{ id: "developer" as const, label: "開発者オプション" }] : []),
  ]);

  // initial は開いた時点の初期タブのみ。モーダルは開くたび再生成されるので初期値参照でよい。
  // svelte-ignore state_referenced_locally
  let active = $state<Section>(initial);
</script>
```

- [ ] **Step 4: テンプレートの分岐を更新する**

`{#if active === "background"}`〜`{:else if active === "reaction"}`の間はそのまま、`{:else if active === "reaction"}`ブロックの直後、`{:else if active === "data"}`より前に追加:

```
        {:else if active === "externalIntegration"}
          <ExternalIntegrationSection />
```

`{:else if active === "data"}`のブロックを以下に置き換える（`CacheBackendSettings`を`DataSection`と同じタブ内に並べて描画し、独立した`{:else if active === "cacheBackend"}`ブロックは削除する）:

置き換え前:
```
        {:else if active === "data"}
          <DataSection />
        {:else if active === "cacheBackend"}
          <CacheBackendSettings />
```

置き換え後:
```
        {:else if active === "data"}
          <DataSection />
          <hr class="my-5 border-0 border-t border-border" />
          <CacheBackendSettings />
```

`{:else if active === "about"}`のブロック（`<AboutSection />`）の直後、`{/if}`より前に追加:

```
        {:else if active === "developer"}
          <DeveloperSection />
```

- [ ] **Step 5: テストを実行して通ることを確認する**

Run: `cd frontend && pnpm vitest run src/ui/Settings.test.ts`
Expected: PASS（3件すべて）

- [ ] **Step 6: 型チェックとフロントエンド全体のテストを実行する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 両方PASS

- [ ] **Step 7: コミットする**

```bash
git add frontend/src/ui/Settings.svelte frontend/src/ui/Settings.test.ts
git commit -m "feat: 設定タブを性質軸で再構成する(外部連携新設・データ統合・開発者オプション追加)"
```

---

### Task 6: `docs/guide/user-guide.md`の更新

**Files:**
- Modify: `docs/guide/user-guide.md`

- [ ] **Step 1: 「設定画面」節を新タブ構成に合わせて書き換える**

`docs/guide/user-guide.md`の`## 設定画面`節にある箇条書き（`- **アカウント**: ...`から`- **このアプリについて**: ...`まで）を、以下に全面置き換えする:

```markdown
- **アカウント**: ログイン中アカウントの一覧、既定アカウントの切替、再認証、アカウントの削除、新規追加。
- **レイアウト**: UIモード（OSに合わせる/PC版/モバイル版）、新規カラムの既定幅、メディアサムネイルの高さ上限。
- **モバイル**: スマホ版でのみ表示されるタブ。ハプティクス（振動）のON/OFF。ONの場合、ノート投稿時とリアクション付与時に端末を振動させます（既定ON）。
- **外観**: テーマ（ライト/ダーク/OSに合わせる、プリセット、カスタムテーマの作成・編集）、コードハイライトテーマ、絵文字スタイル（Twemoji/Fluent Emoji/OS標準）、フォント、Instance Ticker（投稿者インスタンス表示。表示しない/リモートのみ/常に表示）、アイコンの丸み（0～100%。アバター画像の角丸。既定20%）、MFMアニメーション（$[shake]等の装飾アニメーションのON/OFF）。
- **背景**: 背景画像とその配置・基準点、暗さ、ぼかし、カラム不透明度。
- **リアクション**: 絵文字ピッカーの「ピン留め」タブに表示する絵文字の追加・削除・並べ替え。
- **外部連携**: MFM検索構文($[search]相当)で使う検索エンジンのURLテンプレート、投稿本文中のURLへのリンクプレビュー表示ON/OFFとカスタムsummalyプロキシURL。
- **データ**: ノートキャッシュの保持件数・保持日数・サイズの上限（いずれも0で無制限。超過分は古い順に自動削除）、起動時のギャップ埋め件数、ノートキャッシュの保存先バックエンド（SQLite（ローカル、既定）・PostgreSQL・MySQL/MariaDBから選択）。PostgreSQL/MySQL利用時はユーザー自身が用意した稼働中のインスタンス（ホスト・ポート・データベース名・ユーザー名・パスワード）への接続情報を入力します。切り替えは保存と同時に即座に反映され、アプリの再起動は不要です。接続に失敗した場合はエラーを表示し、切替前のバックエンドのまま維持されます。なお、ノートキャッシュのサイズ上限（MB）はPostgreSQL/MySQLバックエンドには効果がありません（保持件数上限・保持日数上限は引き続き有効です）。
- **通知**: デスクトップ通知・通知音のグローバルON/OFFと通知音の選択。実際に鳴るのは、このグローバル設定とタブ側の個別設定が両方ONのときです。
- **NG／ミュート**: NGワード（本文/CWへの部分一致）、NGユーザー、NGインスタンスをそれぞれ複数登録できます。保存後は以降受信するノート・表示中のノートの両方に適用されます。
- **Tsumugiについて**: バージョン・コミットハッシュ・ライセンス・リポジトリURLの表示。新しいバージョンが公開されている場合はバナーで通知されます。バージョン表示を7回タップすると「開発者オプション」タブが表示されるようになります（無効化する手段はありません）。
- **開発者オプション**: 「Tsumugiについて」タブのバージョン表示を7回タップすると表示されるタブ。動作ログをファイルに残す設定（デバッグ用、次回起動から反映）。
```

- [ ] **Step 2: 変更箇所を確認する**

Run: `git diff docs/guide/user-guide.md`
Expected: 「設定画面」節の箇条書きが上記の内容に置き換わっている（キーボードショートカット節など他の節は変更なし）

- [ ] **Step 3: コミットする**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: 設定画面タブ構成の変更をユーザーガイドに反映"
```

---

### Task 7: 最終確認

**Files:** なし（検証のみ）

- [ ] **Step 1: Rust側のテストとフォーマットを確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS（全件）

- [ ] **Step 2: フロントエンド側の型チェック・テストを確認する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 両方PASS

- [ ] **Step 3: 実アプリでの手動確認**

`cargo tauri dev`でアプリを起動し（Xvfb越しに実行する。実画面に影響させない）、以下を確認する:
- 設定モーダルのタブ一覧が「アカウント/レイアウト/(モバイル)/外観/背景/リアクション/外部連携/データ/通知/NG（ミュート）/キー操作/Tsumugiについて」の順で表示され、「キャッシュバックエンド」という独立タブが無いこと
- 「外観」タブにMFM検索エンジン・URLプレビューの項目が無く、MFMアニメーションの項目は残っていること
- 「外部連携」タブでMFM検索エンジン・URLプレビュー・summalyプロキシを編集・保存できること
- 「データ」タブでノートキャッシュ設定とキャッシュバックエンド選択が両方表示され、それぞれ独立して保存できること
- 「データ」タブに「デバッグ」項目が無いこと
- 「Tsumugiについて」タブのバージョン表示を7回クリックすると、有効化メッセージが表示され、ナビゲーションに「開発者オプション」タブが現れること
- 「開発者オプション」タブで「動作ログをファイルに残す」を編集・保存できること
- アプリを再起動しても「開発者オプション」タブが表示され続けること（`app.ui.developerOptionsEnabled`が永続化されていること）

作業が終わったら、確認のために起動した`cargo tauri dev`プロセスを終了する。

- [ ] **Step 4: 完了**

すべてのタスクのコミットが完了していることを確認する:

Run: `git log --oneline main..HEAD`
Expected: Task 1〜6の各コミットが並んでいる
