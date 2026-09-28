# 背景に動画を設定できるようにする (Issue #46)

## 背景・課題

現在の背景画像設定 (`UiPrefs.background_image`, `src-tauri/src/domain/ui.rs`) は選択したファイルを
Rustコマンド `read_image_data_url` で Base64 data URL 化し、そのまま `UiPrefs` の一フィールドとして
JSON 設定ファイル (`SettingsStore` が管理する1ファイル) に丸ごと埋め込んでいる。フロントエンドはこの
data URL をそのまま CSS 変数 `--bg-image` に渡し、`body::before` の `background-image` として描画する。

この仕組みは静止画・GIF程度のサイズでは問題ないが、動画では以下の理由で成立しない。

- 動画は数MB〜数十MBになり得るため、設定ファイル(JSON1ファイル)への Base64 埋め込みは肥大化を招く。
- CSS の `background-image` は `<video>` 要素を背景として敷けない。動画再生には DOM 上の `<video>` 要素が必要。

Issue #46 は Issue #43 (背景画像設定の自由度を上げる) の子課題で、既にクローズ済みの #44 (GIF対応) /
#45 (配置方法) / #76 (基準点) と同じ背景設定機能群の一部。

## 決定事項

- 対応する動画形式: ブラウザ (WebKitGTK 等の `<video>` 要素) が再生できる形式全般を許可する。拡張子で
  image/video を判定するリストは今後拡張可能な形にする。
- 背景メディアの保存方式を **Base64 data URL 埋め込みから、ファイルパス参照方式に統一** する。これは
  動画だけでなく既存の画像 (GIFを含む) にも適用し、実装を一本化する。
- 選択したファイルは `app_data_dir()/backgrounds/` にコピーして保持する (元ファイルを移動・削除・
  リネームしても背景が壊れないようにするため)。
- 動画ファイルサイズの上限は設けない。コピー処理は既存の非同期Tauriコマンドと同様、UIをブロックしない。
- 既存ユーザーの Base64 背景画像は、起動時に自動でファイル化して新形式に移行する (ユーザー操作不要)。
- 配置方法の Tile は動画では意味を持たないため、動画選択時は UI 上非表示にする。

## データモデル変更 (`src-tauri/src/domain/ui.rs`)

`UiPrefs.background_image: String` (Base64 data URL) を廃止し、以下に置き換える。

```rust
#[derive(specta::Type, ...)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundKind {
    Image,
    Video,
}

// UiPrefs 内
pub background_kind: Option<BackgroundKind>,   // 背景未設定なら None
pub background_path: Option<String>,           // backgrounds/ にコピーした後の絶対パス
```

`background_fit_mode` / `background_position` / `background_dim` / `background_blur` /
`column_opacity` は既存のまま変更しない (image/video 双方に適用可能な値のため)。

既存フィールドと同様、`#[serde(default)]` を付与し前バージョンJSONとの後方互換を保つ。

## Rust側実装

### 新コマンド: `import_background_media`

`src-tauri/src/commands/mute.rs` (または新設する `commands/background.rs`) に追加。

```rust
#[tauri::command]
#[specta::specta]
async fn import_background_media(app: AppHandle, path: String) -> Result<BackgroundMedia, String>
```

- `BackgroundMedia { kind: BackgroundKind, absolute_path: String }` (specta::Type)
- 拡張子で image (`png`/`jpg`/`jpeg`/`gif`/`webp`/`avif`/`bmp`/`svg`) / video
  (`mp4`/`webm`/`mov`/`m4v`/`mkv`/`avi`/`ogv`) を判定。両リストに無い拡張子は `Err` を返す。
- `app_data_dir()/backgrounds/<uuid>.<ext>` にファイルをコピー。
- 直前に設定されていた背景ファイルがあれば (呼び出し元から渡された、または `UiPrefs` から読んだ)
  コピー後に削除し、`backgrounds/` にゴミが溜まらないようにする。
- 既存の `read_image_data_url` はそのまま残す (他に用途があるため削除しない)。
- `specta_builder()` (`src-tauri/src/lib.rs`) に登録する。

### サイズ判定・MIME推定

- 画像は既存の `guess_image_mime` / `MAX_BACKGROUND_IMAGE_BYTES` (8MB) をそのまま維持。
- 動画は新規 `guess_video_mime` (拡張子ベース) を追加。サイズ上限は設けない。

### 既存データの自動移行

`store/settings.rs` の `SettingsStore::load_ui()` を拡張する。

1. 生JSONを読んだ際、旧フィールド `backgroundImage` (Base64 data URL文字列) が残っていて空文字でない場合、
   移行処理を実行する。
2. `data:{mime};base64,{...}` から MIME・拡張子を復元し、Base64をdecode。
3. `app_data_dir()/backgrounds/` にファイルとして書き出す。
4. `background_kind = Image`, `background_path = <新パス>` をセットする。
5. 移行結果を即座に `save_ui()` で書き戻し、次回起動時は再移行が走らないようにする。

`domain/ui.rs` の既存の後方互換テスト群 (321-524行目) と同じ形式で、旧JSON→新フィールド移行のテストケースを
追加する。`store/settings.rs` にも移行の統合テストを追加する。

## フロントエンド実装

### マッピング拡張

- `frontend/src/lib/backgroundFitMode.ts`: 動画用の `object-fit` 値 (cover/contain/fill) を追加。
- `frontend/src/lib/backgroundPosition.ts`: 既存の9点マッピング値 (`"left top"` 等) は `object-position`
  にもそのまま使えるため追加変更は不要。
- `frontend/src/ui/settings/BackgroundSection.svelte`: `background_kind === 'video'` のとき、配置方法の
  Tile ボタンを非表示にする。既存設定が Tile+動画だった場合は Cover にフォールバック表示する。

### ダミング(暗さ)オーバーレイの共通化

現状は `body::before` の `background-image` に `linear-gradient` を重ねてダミングを実現しているが、
動画では `background-image` が使えないため、画像・動画共通の独立したオーバーレイに統一する。

- 新規 `<div class="bg-dim-overlay">` を追加し、`background: rgba(0, 0, 0, var(--bg-dim))`,
  `position: fixed; inset: 0; z-index: -1;` を背景要素より上・本体コンテンツより下に配置する。
- `frontend/src/app.css` の `body::before` からは `linear-gradient` 部分を取り除き、素の背景画像描画のみ
  に簡素化する。

### 画像の描画 (既存パスを流用)

変更なし。`body::before` の `background-image` に渡す URL を Base64 data URL から
`convertFileSrc(background_path)` (`@tauri-apps/api/core`) の結果に置き換えるのみ。

### 動画の描画 (新規)

新規Svelteコンポーネント `frontend/src/ui/BackgroundVideo.svelte` を追加する。

- `background_kind === 'video'` のときのみマウントする。
- `position: fixed; inset: 0; z-index: -1;` で配置し、`<video autoplay loop muted playsinline>` を使う。
- `src` は `convertFileSrc(background_path)`。
- `object-fit` / `object-position` / `filter: blur(var(--bg-blur))` をCSS変数から適用する。

### `store.svelte.ts` 変更

- `pickBackgroundImage()` を `pickBackgroundMedia()` にリネームし、ファイルダイアログの拡張子フィルタに
  動画拡張子 (mp4/webm/mov/m4v/mkv/avi/ogv) を追加する。
- `commands.importBackgroundMedia(path)` を呼び、返ってきた `kind` / `absolutePath` を `setUiPrefs` で
  `background_kind` / `background_path` として保存する。
- `#applyBackground` を `background_kind` で分岐させる (画像なら CSS 変数、動画なら
  `BackgroundVideo.svelte` の表示切替)。

### `tauri.conf.json` 変更

`app.security.assetProtocol.enable: true` と、`app_data_dir()/backgrounds/**` に相当するパターンを
`scope` に追加する。ローカルファイルを `asset://` プロトコルで配信するために必須。

## テスト計画

- **Rust単体テスト**
  - `domain/ui.rs`: `BackgroundKind` のシリアライズ/デシリアライズ、旧JSON (`backgroundImage` あり) から
    新フィールドへの移行のデフォルト適用確認
  - `store/settings.rs`: 旧形式JSONを `load_ui()` した際に自動移行が走り、`backgrounds/` にファイルが
    作成され、`save_ui()` で新形式に書き戻されることを統合テストで確認
  - `import_background_media` の拡張子判定 (image/video/未知拡張子でエラー)、コピー、直前ファイルの削除
- **フロントエンドテスト (Vitest)**
  - `backgroundFitMode.ts` の動画向けマッピング追加分
  - `pickBackgroundMedia` / `#applyBackground` の `background_kind` 分岐ロジック
- **手動確認**
  - `cargo tauri dev` を Xvfb 越しで起動し、動画背景を設定→ループ再生・ミュート・Cover/Fit/Fill 各配置・
    基準点・暗さ・ぼかしを目視確認する。
  - 旧設定ファイル (Base64画像入り) を用意して起動し、自動移行されることを確認する。

## 影響・注意点

- `UiPrefs` のフィールド構造が変わるが、自動移行処理により既存ユーザーの見た目・設定は変わらない想定。
- CHANGELOG に「背景画像の保存方式をファイル参照方式に変更 (自動移行)」を記載する。
- スコープ外: 動画の音声再生 (常にミュート固定)、Tile相当の動画タイリング表示、動画サイズ上限。
