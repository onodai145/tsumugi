# 背景画像の基準点設定 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 背景画像の `background-position`（基準点）を9点グリッドUIで設定できるようにする（Issue #76）。

**Architecture:** 既存の `background_fit_mode`（Issue #45）と全く同じ配線パターンを踏襲する。Rust `UiPrefs` に文字列フィールドを1つ追加 → フロントの `lib/backgroundPosition.ts` で値↔CSSのマップを定義 → `store.svelte.ts` の `#applyBackground` が `<html>` に CSS変数 `--bg-position` をセット → `app.css` の `body::before` がそれを読む → `DisplaySection.svelte` に3×3グリッドの選択UIを追加。

**Tech Stack:** Rust (serde, specta), Svelte 5 (runes), TypeScript, plain CSS。

## Global Constraints

- コミットメッセージは1行のみ（サブジェクト行だけ、本文なし）。[[feedback-commit-message-length]]
- Issue紐づき修正はPR経由でマージする（直接mainにマージしない）。[[feedback-pr-for-issue-fixes]]
- PR本文でのIssueクローズは `Fixes #76` / `Closes #76` のキーワードを使う（番号参照だけでは自動クローズされない）。[[feedback-pr-closing-keyword]]
- push後はCIをMonitorで待たない。
- `cargo tauri dev` 以外でアプリを直接起動しない（`cargo run` は不可、devサーバ未起動だと接続エラーになる）。
- specta/tauri-specta のバージョンは pinned。ここでは変更しない。
- 配置方法（`background_fit_mode`）が `"fill"` のときは基準点UIを非表示にする（見た目に影響しないため）。

---

### Task 1: Rust `UiPrefs` に `background_position` フィールドを追加

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `UiPrefs.background_position: String`（serdeで `backgroundPosition` としてJSON化される。フロントの `UiPrefs`型に自動反映される — `specta` が `tauri-specta` 経由で `frontend/src/bindings/tauri.gen.ts` を再生成するため、Task 2以降ではこのフィールドを手で足さなくてよい）。既定値は `"center"`。

- [ ] **Step 1: `UiPrefs` 構造体にフィールドを追加**

`src-tauri/src/domain/ui.rs` の `background_fit_mode` フィールドの直後（71行目の後）に追加:

```rust
    /// 背景画像の基準点（background-position）。9点グリッドから選択（Issue #76）。
    /// "top-left" | "top" | "top-right" | "left" | "center" | "right"
    /// | "bottom-left" | "bottom" | "bottom-right"
    #[serde(default = "default_background_position")]
    pub background_position: String,
```

- [ ] **Step 2: デフォルト値関数を追加**

`default_background_fit_mode` 関数（118〜120行目）の直後に追加:

```rust
fn default_background_position() -> String {
    "center".into()
}
```

- [ ] **Step 3: `Default` impl に追加**

`impl Default for UiPrefs` の中、`background_fit_mode: default_background_fit_mode(),` の直後（160行目の後）に追加:

```rust
            background_position: default_background_position(),
```

- [ ] **Step 4: 既存テスト3件を更新**

`deserializes_legacy_json_without_new_fields`（181行目〜）に、`background_fit_mode` のアサーションの直後に追加:

```rust
        // background_position も同様に既定値(center, 追加前の見た目)へフォールバックすること。
        assert_eq!(v.background_position, "center");
```

`roundtrips_keymap`（227行目〜）の `UiPrefs { ... }` リテラルに、`background_fit_mode: "tile".into(),` の直後に追加:

```rust
            background_position: "top-left".into(),
```

- [ ] **Step 5: テストを実行して通ることを確認**

Run: `cd src-tauri && cargo test domain::ui`
Expected: PASS（3テストとも成功。`roundtrips_keymap` は `background_position: "top-left"` を含めてラウンドトリップできること、legacyテストは新フィールドが `"center"` にフォールバックすることを確認する）

- [ ] **Step 6: 全体のRustテストとバインディング生成を確認**

Run: `cd src-tauri && cargo test`
Expected: PASS。`generates_frontend_bindings` テストが通り、`frontend/src/bindings/tauri.gen.ts` の `UiPrefs` 型に `backgroundPosition?: string` が追加されていることを確認する:

Run: `grep -n "backgroundPosition" ../frontend/src/bindings/tauri.gen.ts`
Expected: `backgroundPosition?: string,` を含む行がヒットする

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsに背景画像の基準点(background_position)を追加"
```

---

### Task 2: `lib/backgroundPosition.ts` を新規作成

**Files:**
- Create: `frontend/src/lib/backgroundPosition.ts`

**Interfaces:**
- Consumes: なし（Task 1で生成された `bindings/tauri.gen.ts` の `UiPrefs.backgroundPosition?: string` とは文字列として緩く連携するのみ、型インポートはしない — `backgroundFitMode.ts` も同様の設計）
- Produces:
  - `export type BackgroundPosition = "top-left" | "top" | "top-right" | "left" | "center" | "right" | "bottom-left" | "bottom" | "bottom-right";`
  - `export const BACKGROUND_POSITION_CSS: Record<string, string>` — 値 → CSS `background-position` 文字列
  - `export const BACKGROUND_POSITION_GRID: BackgroundPosition[]` — 3×3グリッド描画順（row-major、9要素）

- [ ] **Step 1: ファイルを作成**

`frontend/src/lib/backgroundFitMode.ts` を参考に、以下の内容で作成:

```ts
// 背景画像の基準点（background-position）。9点グリッドから選択（Issue #76）。
// Rust 側 UiPrefs.backgroundPosition の文字列値と対応する。
export type BackgroundPosition =
  | "top-left"
  | "top"
  | "top-right"
  | "left"
  | "center"
  | "right"
  | "bottom-left"
  | "bottom"
  | "bottom-right";

export const BACKGROUND_POSITION_CSS: Record<string, string> = {
  "top-left": "left top",
  top: "center top",
  "top-right": "right top",
  left: "left center",
  center: "center center",
  right: "right center",
  "bottom-left": "left bottom",
  bottom: "center bottom",
  "bottom-right": "right bottom",
};

// 3x3グリッドUIの描画順（row-major: 左上から右下へ）。
export const BACKGROUND_POSITION_GRID: BackgroundPosition[] = [
  "top-left",
  "top",
  "top-right",
  "left",
  "center",
  "right",
  "bottom-left",
  "bottom",
  "bottom-right",
];
```

- [ ] **Step 2: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: PASS（新規ファイルなので既存の型エラーは出ないこと。まだどこからも import していないため未使用変数エラーも出ない）

- [ ] **Step 3: コミット**

```bash
git add frontend/src/lib/backgroundPosition.ts
git commit -m "feat: 背景画像の基準点マッピングを追加"
```

---

### Task 3: `store.svelte.ts` に配線

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `BACKGROUND_POSITION_CSS` from `./backgroundPosition`（Task 2で作成）
- Produces: `<html>` 要素に CSS変数 `--bg-position` がセットされる（Task 4で `app.css` が読む）

- [ ] **Step 1: import を追加**

31行目の `import { BACKGROUND_FIT_MODE_CSS } from "./backgroundFitMode";` の直後に追加:

```ts
import { BACKGROUND_POSITION_CSS } from "./backgroundPosition";
```

- [ ] **Step 2: 初期 `ui` state に既定値を追加**

118〜138行目の `ui = $state<UiPrefs>({...})` の中、`backgroundFitMode: "cover",` の直後に追加:

```ts
    backgroundPosition: "center",
```

- [ ] **Step 3: `#applyBackground` の型と処理を拡張**

1029〜1049行目の `#applyBackground` を以下に置き換える:

```ts
  #applyBackground(
    prefs: Pick<
      UiPrefs,
      | "backgroundImage"
      | "backgroundDim"
      | "backgroundBlur"
      | "columnOpacity"
      | "backgroundFitMode"
      | "backgroundPosition"
    >,
  ) {
    const root = document.documentElement;
    const img = prefs.backgroundImage ?? "";
    if (img) {
      root.style.setProperty("--bg-image", `url("${img}")`);
    } else {
      root.style.removeProperty("--bg-image");
    }
    root.style.setProperty("--bg-dim", String((prefs.backgroundDim ?? 0) / 100));
    root.style.setProperty("--bg-blur", `${prefs.backgroundBlur ?? 0}px`);
    root.style.setProperty("--column-opacity", `${prefs.columnOpacity ?? 100}%`);
    const [bgSize, bgRepeat] = BACKGROUND_FIT_MODE_CSS[prefs.backgroundFitMode ?? "cover"] ??
      BACKGROUND_FIT_MODE_CSS.cover;
    root.style.setProperty("--bg-size", bgSize);
    root.style.setProperty("--bg-repeat", bgRepeat);
    const bgPosition = BACKGROUND_POSITION_CSS[prefs.backgroundPosition ?? "center"] ??
      BACKGROUND_POSITION_CSS.center;
    root.style.setProperty("--bg-position", bgPosition);
  }
```

- [ ] **Step 4: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: 背景画像の基準点をCSS変数へ反映"
```

---

### Task 4: `app.css` の `background-position` を可変にする

**Files:**
- Modify: `frontend/src/app.css`

**Interfaces:**
- Consumes: CSS変数 `--bg-position`（Task 3でセットされる）

- [ ] **Step 1: 固定値を変数参照に変更**

`app.css` 86行目の `background-position: center;` を以下に置き換える:

```css
  background-position: var(--bg-position, center);
```

- [ ] **Step 2: 目視確認（手動）**

Run: `cargo tauri dev`（プロジェクトルートで）
設定→表示→背景画像を設定した状態で、`document.documentElement.style` に `--bg-position` が付いていないこと（まだUIから変更する手段がないため既定の `center center` のまま）を確認する。見た目が Task 実施前と変わらないこと。

- [ ] **Step 3: コミット**

```bash
git add frontend/src/app.css
git commit -m "feat: 背景画像のbackground-positionをCSS変数化"
```

---

### Task 5: `DisplaySection.svelte` に基準点グリッドUIを追加

**Files:**
- Modify: `frontend/src/ui/settings/DisplaySection.svelte`

**Interfaces:**
- Consumes: `BackgroundPosition`, `BACKGROUND_POSITION_GRID` from `../../lib/backgroundPosition`（Task 2）

- [ ] **Step 1: import を追加**

5行目の `import { BACKGROUND_FIT_MODE_OPTIONS, type BackgroundFitMode } from "../../lib/backgroundFitMode";` の直後に追加:

```ts
  import { BACKGROUND_POSITION_GRID, type BackgroundPosition } from "../../lib/backgroundPosition";
```

- [ ] **Step 2: `$state` を追加**

16〜18行目の `backgroundFitMode` の `$state` 定義の直後に追加:

```ts
  let backgroundPosition = $state<BackgroundPosition>(
    (app.ui.backgroundPosition as BackgroundPosition) ?? "center",
  );
```

- [ ] **Step 3: 基準点ラベルのマップを追加**

`const themes: ...` の定義（28行目）の手前に追加（UIモードのコメントと同様の位置に置く）:

```ts
  // 背景画像の基準点（9点グリッド、Issue #76）。position→アクセシブルラベル。
  const positionLabels: Record<BackgroundPosition, string> = {
    "top-left": "左上",
    top: "上",
    "top-right": "右上",
    left: "左",
    center: "中央",
    right: "右",
    "bottom-left": "左下",
    bottom: "下",
    "bottom-right": "右下",
  };
```

- [ ] **Step 4: `save()` に含める**

148〜182行目の `save()` 内、`await app.setUiPrefs({...})` の `backgroundFitMode,` の直後に追加:

```ts
        backgroundPosition,
```

- [ ] **Step 5: グリッドUIをテンプレートに追加**

358〜372行目、「背景画像の配置方法」フィールドの直後（373行目の `<label class="field">` 「背景の暗さ」の手前）に追加。`backgroundFitMode !== "fill"` のときだけ表示する:

```svelte
  {#if backgroundFitMode !== "fill"}
    <div class="field">
      <span>基準点</span>
      <div class="pos-grid">
        {#each BACKGROUND_POSITION_GRID as p (p)}
          <button
            class="pos-btn"
            class:active={backgroundPosition === p}
            title={positionLabels[p]}
            aria-label={positionLabels[p]}
            onclick={() => (backgroundPosition = p)}
          ></button>
        {/each}
      </div>
    </div>
  {/if}
```

- [ ] **Step 6: グリッドUI用のCSSを追加**

`<style>` ブロック内、`.seg-btn.active { ... }`（429〜432行目）の直後に追加:

```css
  .pos-grid {
    display: grid;
    grid-template-columns: repeat(3, 28px);
    grid-template-rows: repeat(3, 28px);
    gap: 4px;
    width: fit-content;
  }
  .pos-btn {
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    cursor: pointer;
    padding: 0;
  }
  .pos-btn:hover {
    border-color: var(--accent);
  }
  .pos-btn.active {
    background: var(--accent);
    border-color: var(--accent);
  }
```

- [ ] **Step 7: 型チェックを実行**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 8: 手動UI確認**

Run: `cargo tauri dev`（プロジェクトルートで、既に起動中なら再利用）

1. 設定→表示→背景画像に何か画像を設定する。
2. 「背景画像の配置方法」を `Fill` 以外（例: `Cover`）にする → 「基準点」フィールドが表示されること。
3. 3×3グリッドの左上ボタンをクリック → `active` スタイルが左上に付くこと。
4. 「保存」を押す → 背景画像の表示位置が左上寄りに変わること（画像内容によっては分かりにくい場合、大きめの画像や顔写真等で確認）。
5. 配置方法を `Fill` に切り替える → 「基準点」フィールドが非表示になること。
6. 設定画面を閉じて開き直す → 基準点の選択が保持されていること（再読み込み後も `active` が正しい位置にあること）。

- [ ] **Step 9: コミット**

```bash
git add frontend/src/ui/settings/DisplaySection.svelte
git commit -m "feat: 背景画像の基準点を選ぶ9点グリッドUIを追加"
```

---

### Task 6: 最終確認とPR作成

**Files:** なし（確認のみ）

- [ ] **Step 1: Rust側テストを全体で再実行**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 2: フロント型チェックを再実行**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 3: 全差分をレビュー**

Run: `git log --oneline main..HEAD` と `git diff main...HEAD --stat`
Expected: Task 1〜5の5コミットが並んでおり、意図した5ファイル + バインディング生成物のみが変更されていること

- [ ] **Step 4: push してPRを作成**

```bash
git push -u origin feat/issue-76-background-position
gh pr create --title "背景画像の基準点(background-position)を設定可能にする" --body "$(cat <<'EOF'
## Summary
- 背景画像の基準点（9点グリッド）を設定できるようにした

Fixes #76
EOF
)"
```

Expected: PR URLが返る。CIの結果はユーザーが確認する（Monitorで待たない）。
