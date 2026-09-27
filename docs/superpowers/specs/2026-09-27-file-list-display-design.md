# 非メディア添付ファイルのリスト表示 (Issue #387)

## 背景

`MediaGrid.svelte` は画像・動画・音声以外の添付ファイルも、メディアと同じ 16:10 のグリッドセルに `📄 ファイル名` のボタンとして表示する。プレビューできないファイルであることが分かりにくく、クリックすると `openUrl` で外部ブラウザが開く。

## 方針

非メディアの添付ファイルをグリッドから分離し、リスト行で表示する。クリックで保存ダイアログを開く。外部ブラウザは開かない。

## 設計

### Rust

- `domain::DriveFile` に `size: Option<i64>`（specta では number として出力。`Poll.expires_at` と同じ既存パターン） を追加する。`#[serde(default)]` を付け、既存の JSON との互換を保つ。
- `api/normalize.rs` で Misskey の `size` を詰める。
- `cargo test` で `frontend/src/bindings/tauri.gen.ts` を再生成する。

### フロント

- `lib/` にバイト数を `1.2 MB` 形式にする整形関数を追加する。`size` が無い場合は表示しない。
- 新規 `render/FileList.svelte`:
  - 1 行は `[Download アイコン] ファイル名  サイズ` のボタン。
  - クリックで `saveMediaToDisk(f.url, fileName(f), onError)` を呼ぶ。`openUrl` は使わない。
  - 見た目は `docs/design/style-guide.md` の border-radius / font-size / icon-size のスケールに従う。
  - 閲覧注意のファイルは、従来どおりカバーを表示してから開示する。
- `MediaGrid.svelte`:
  - ファイルをメディア(画像・動画・音声)と非メディアに分ける。
  - メディアは従来のグリッドのまま。
  - 非メディアはグリッドの下に `FileList` で表示する。
  - `{:else}` の `📄` ボタン分岐は削除する。

### エラー処理

`saveMediaToDisk` の `onError` には、既存のメディア DL と同じエラー表示を渡す。

### テスト

- サイズ整形関数の Vitest。
- `MediaGrid`: 非メディアがリスト行で表示されること、クリックで `saveMediaToDisk` が呼ばれること。
- Rust: `normalize` の `size` マッピング。

## スコープ外

メディアのグリッド表示の変更、ファイル種別ごとのアイコン出し分け。
