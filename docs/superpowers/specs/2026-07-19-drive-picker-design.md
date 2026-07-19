# ドライブからメディアを添付できるようにする（Issue #13）

## 背景

現在 `ComposeBar.svelte` の画像添付は `pickFiles()`（ネイティブファイルダイアログ→ローカルファイルを
`upload_file` コマンドで毎回新規アップロード）のみ。Misskey ドライブに既にあるファイル（過去に投稿した画像、
Web クライアント側でアップロード済みのファイルなど）を選んで再添付する手段がない。

Rust 側もアップロード API（`api/drive.rs::upload_file`）しか実装されておらず、ドライブの一覧取得
（`drive/files`）・フォルダ取得（`drive/folders`）は未実装。

## スコープ

- フォルダ階層のナビゲーション対応（ルート→フォルダに入る→中のファイル一覧、パンくずで戻る）。
- 複数ファイル選択、「もっと見る」ボタンでの追加読み込み（`untilId` ページング）。
- 既存の画像添付ボタン（`ImagePlus`）にポップオーバーメニューを付与し、「ローカルから選択」（既存動作）と
  「ドライブから選択」（新規）を選べるようにする。新規ボタンの追加はしない。
- センシティブファイル（`isSensitive`）は `MediaGrid.svelte` と同じ「閲覧注意（クリックで表示）」カバーで隠す。
- ドライブへの新規アップロード・フォルダ作成・ファイル削除などドライブ管理機能は対象外（YAGNI、既存の
  Misskey Web クライアント側で行う）。
- ファイル種別フィルタは画像/動画のみに絞る（`drive/files` の `type` パラメータに `image/*,video/*`）。
  投稿に添付できるのは元々メディアのみのため。

## 変更内容

### 1. `src-tauri/src/domain/drive.rs`（新規）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveFolder {
    pub id: String,
    pub name: String,
}
```

`domain/mod.rs` に `pub use drive::DriveFolder;` を追加。既存 `DriveFile`（`domain/note.rs`）はそのまま。

### 2. `src-tauri/src/api/drive.rs`

`upload_file` に加えて2関数を追加。エラーハンドリングは `upload_file` と同じパターン（401/403/429/その他を
`Error` に変換）。

```rust
pub async fn list_files(
    http: &reqwest::Client, host: &str, token: &str,
    folder_id: Option<&str>, until_id: Option<&str>,
) -> Result<Vec<DriveFile>>

pub async fn list_folders(
    http: &reqwest::Client, host: &str, token: &str,
    folder_id: Option<&str>,
) -> Result<Vec<DriveFolder>>
```

- `list_files` は `POST /api/drive/files` に `{ i, folderId, untilId, limit: 30, type: "image/*,video/*" }`
  （`folderId`/`untilId` が `None` ならキー自体を省略）を送り、`Vec<RawFile>` を受けて `DriveFile` へ変換
  （既存 `normalize.rs::RawFile::into()` を再利用）。
- `list_folders` は `POST /api/drive/folders` に `{ i, folderId, limit: 100 }` を送り、素の
  `{ id, name }` を `DriveFolder` にマップ（フォルダは folder→folder の再帰も一覧に含まれる仕様通りそのまま）。

### 3. `src-tauri/src/commands/note.rs`

```rust
#[tauri::command]
#[specta::specta]
pub async fn list_drive_files(
    state: State<'_, AppState>, account_id: String,
    folder_id: Option<String>, until_id: Option<String>,
) -> Result<Vec<DriveFile>>

#[tauri::command]
#[specta::specta]
pub async fn list_drive_folders(
    state: State<'_, AppState>, account_id: String, folder_id: Option<String>,
) -> Result<Vec<DriveFolder>>
```

既存 `upload_file` コマンドと同じく `state` からホスト/トークンを解決して `api::drive` を呼ぶだけの薄いラッパ。

### 4. `src-tauri/src/lib.rs`

`specta_builder()` の `commands::note::upload_file` の並びに `list_drive_files`, `list_drive_folders` を追加登録。
これにより `cargo test`（`generates_frontend_bindings`）でTSバインディングが再生成される。

### 5. `frontend/src/ui/DrivePicker.svelte`（新規）

`AddColumnModal.svelte` と同じオーバーレイ＋モーダルの見た目パターンを踏襲する選択モーダル。

- props: `{ accountId: string; onSelect: (files: DriveFile[]) => void; onclose: () => void }`
- state: `folderId: string | null`（現在地、ルート=null）、`breadcrumb: DriveFolder[]`（戻り用の経路）、
  `folders: DriveFolder[]`、`files: DriveFile[]`、`selected: Map<string, DriveFile>`、`loading`、
  `loadingMore`、`hasMore`、`err`
- フォルダ一覧・ファイル一覧は `folderId` が変わるたびに `list_drive_folders` / `list_drive_files`
  （`untilId: null`）を並行取得して置き換え、`hasMore` はファイル取得件数が30件（limit）だったかで判定。
- パンくずのクリック / 「戻る」でスタックを pop し `folderId` を更新。フォルダ行クリックで
  `breadcrumb.push(現在のフォルダ情報)` して中へ。
- ファイルサムネイルはクリックでトグル選択（`selected` に追加/削除）、選択中はチェックマークオーバーレイ。
  `isSensitive` なファイルは `MediaGrid.svelte` の `.sensitive-cover` と同じ「閲覧注意（クリックで表示）」
  ボタンを最初に表示し、クリックで通常表示に切り替える（選択操作とは別トグル）。
- フッター: 「もっと見る」ボタン（`hasMore` の時のみ、`untilId` に現在の最後のファイル `id` を渡して追加取得
  し `files` に連結）、「選択中 N件」表示、「添付」ボタン（`selected` の値を配列化して `onSelect` を呼び
  `onclose`）。
- 取得失敗時は `err` をモーダル内に表示（既存 `.err` パターン踏襲）。

### 6. `frontend/src/ui/ComposeBar.svelte`

- `ImagePlus` ボタンをクリックで開閉するポップオーバー（`Dropdown.svelte` と同様の `portal` + `fixed` 位置
  決めロジックを踏襲した軽量な自前実装、2項目のみなので `Dropdown<T>` は使わず直書き）に変更。
  項目は「ローカルから選択」（クリックで既存 `pickFiles()` を呼ぶ）と「ドライブから選択」
  （`showDrivePicker = true` にする）。
- `showDrivePicker` が true の間 `<DrivePicker accountId={accountId} onSelect={...} onclose={...} />` を表示。
  `onSelect` はアップロード不要（既にドライブ上のファイル）なので `attached = [...attached, ...files]` する
  だけ（同一ファイルの重複選択は `id` で除外)。
- アカウント未選択（`!accountId`）の場合はドライブ選択項目を disabled にし、理由をタイトル属性で示す
  （既存の「アカウントを選択してください」エラーパターンに合わせる）。

## テスト

- Rust: `cargo test` で `generates_frontend_bindings` が通ること（TSバインディング再生成・camelCase
  確認）。`list_files`/`list_folders` は実 Misskey 接続が要るため他の drive 系と同様ネットワークテストは
  `#[ignore]` にする（もし追加するなら）。
- フロント: `pnpm check` で型チェック。
- UI: `cargo tauri dev` で目視確認 — ImagePlus ボタンから「ドライブから選択」→フォルダ移動→複数選択→
  「もっと見る」→「添付」→ComposeBar のサムネイル一覧に反映されること。センシティブファイルのカバーが
  効くこと。
