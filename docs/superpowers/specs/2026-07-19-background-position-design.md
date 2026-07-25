# 背景画像の基準点設定（Issue #76）

## 背景

Issue #45（`background_fit_mode`）で背景画像の配置方法（cover/contain/fill/tile）は設定可能になったが、
`background-position` は `app.css` に `center` 固定でハードコードされている。画像の見せたい部分（人物の顔など）
が中央からずれている場合に調整できないため、基準点を設定可能にする。

## スコープ

- 9点グリッド（上左/上/上右/左/中央/右/下左/下/下右）から選択するUI。
- 配置方法が `fill`（縦横比無視で全面引き伸ばし）のときは基準点が見た目に影響しないため、設定UIごと非表示にする。
- 自由配置（ドラッグ）やパーセント指定は対象外（YAGNI）。

## 変更内容

### 1. `frontend/src/lib/backgroundPosition.ts`（新規）

`backgroundFitMode.ts` と同型。

```ts
export type BackgroundPosition =
  | "top-left" | "top" | "top-right"
  | "left" | "center" | "right"
  | "bottom-left" | "bottom" | "bottom-right";

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

export const BACKGROUND_POSITION_OPTIONS: { value: BackgroundPosition }[] = [
  { value: "top-left" }, { value: "top" }, { value: "top-right" },
  { value: "left" }, { value: "center" }, { value: "right" },
  { value: "bottom-left" }, { value: "bottom" }, { value: "bottom-right" },
];
```

### 2. `src-tauri/src/domain/ui.rs`

`UiPrefs` に `background_position: String`（`#[serde(default = "default_background_position")]`、既定 `"center"`）を
`background_fit_mode` の隣に追加。`Default` impl・legacy fallback テスト・roundtrip テストを更新。

### 3. `frontend/src/lib/store.svelte.ts`

`#applyBackground` の対象フィールドに `backgroundPosition` を追加し、`root.style.setProperty("--bg-position", ...)` を
設定（`BACKGROUND_POSITION_CSS` にない値は `center center` にフォールバック）。

### 4. `frontend/src/app.css`

`body::before` の `background-position: center;` を `background-position: var(--bg-position, center);` に変更。

### 5. `frontend/src/ui/settings/DisplaySection.svelte`

- `backgroundPosition` の `$state` を追加（初期値 `app.ui.backgroundPosition ?? "center"`）。
- 「背景画像の配置方法」フィールドの下に「基準点」フィールドを追加。`backgroundFitMode !== "fill"` の場合のみ表示。
- 3×3グリッドのボタン（アイコン無し、位置で意味が分かる正方形ボタン、選択中は `active` クラス）。
- 保存処理（`save()` 内のオブジェクト）に `backgroundPosition` を含める。

## テスト

- `src-tauri/src/domain/ui.rs` の既存テスト3件（legacy fallback / roundtrip / custom_themes）を `background_position` 込みで更新。
- `cargo test`（Rust）、`pnpm check`（フロント型チェック）で確認。
- UIは `cargo tauri dev` で目視確認（fill選択時に基準点UIが消えること、他の配置方法で基準点を変えると見た目が変わること）。
