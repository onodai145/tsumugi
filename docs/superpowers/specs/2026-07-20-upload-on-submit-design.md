# 添付ファイルのアップロードタイミングを投稿時に変える (Issue #66)

## 背景・目的

現状 `frontend/src/ui/ComposeBar.svelte` の `pickFiles()` は、ファイル選択ダイアログでパスを取得した直後に1件ずつ `commands.uploadFile()` を呼び、ドライブへ即時アップロードしている。

この設計では、添付済みの状態で下書きを破棄(コンポーズ欄を閉じる/クリア)した場合、投稿されないままドライブにファイルだけが残ってしまう(孤児ファイル化)。本Issueの目的は、アップロードを「投稿ボタン押下時」まで遅延させ、投稿しない限りドライブへのアップロードが発生しないようにすること。

## データモデル

`attached: DriveFile[]` を以下のUnion型に置き換える。

```ts
type AttachmentItem =
  | { kind: "local"; id: string; path: string; previewUrl: string; name: string }
  | { kind: "drive"; id: string; file: DriveFile };

let attachments = $state<AttachmentItem[]>([]);
```

- `id`: クライアント側で `crypto.randomUUID()` により採番する。未アップロードのローカル項目にも一意キーを持たせ、削除・状態更新をindexではなくidベースで行う。
- `previewUrl`: 画像拡張子(png/jpg/jpeg/gif/webp)の場合のみ、新設する Rust コマンド `read_attachment_preview` でファイルを data URL(base64) に変換して表示する。動画(mp4/webm)や変換失敗時は `null` とし、拡張子バッジ表示にフォールバックする。
  - Tauriのアセットプロトコル(`convertFileSrc`)は、静的スコープ設定が必要でユーザーが任意に選んだパスに対しては使いにくく、Android の `content://` パスも扱えないため採用しない。既存の `src-tauri/src/commands/mute.rs` の `read_image_data_url`(背景画像設定用に全く同じ「ローカル画像→data URL」変換を行っており、Android SAFパスにも対応済み)と同じ実装パターンを再利用する。
- ドライブピッカー(`DrivePicker.svelte`)経由で選択された既存ドライブファイルは、最初から `kind: "drive"` として追加する(アップロード不要)。

## ファイル選択時の挙動

`pickFiles()` は、ダイアログでパスを取得した後、**アップロードせずに** `kind: "local"` の `AttachmentItem` を `attachments` に追加するのみとする。既存の即時 `uploadFile` 呼び出しループは削除する。

## 投稿時の挙動

`submit()` を以下のように変更する。

1. `posting` フラグを立てる(既存の `uploading` boolean は廃止しこちらに統合)。
2. `attachments` を順に走査し、`kind === "local"` の項目のみ `commands.uploadFile(accountId, path)` を呼ぶ。
3. アップロードに成功した項目は、同じ `id` を保ったまま配列内で `kind: "drive"` に差し替える(再送信時に成功済み分を再アップロードしないため)。
4. いずれかのアップロードが失敗した場合、即座にループを中断して投稿全体を中止する。失敗した項目の `id` と紐づくエラーメッセージを保持し、`posting = false` にして処理を終える。
5. 全項目が `kind: "drive"` になったら `fileIds = attachments.map(a => a.file.id)` を組み立て、既存の `postNote` 呼び出しへ進む(ここは現状ロジックを維持)。
6. ノート投稿自体が成功したら、既存の成功時処理(本文・添付クリア等)を行う。失敗した場合は既存のエラー表示経路を使う。添付ファイルは既に全て `kind: "drive"` になっているため、再送信時に再アップロードは発生しない。

## エラー・リトライ

失敗した `local` 項目はそのままリストに残し、サムネイル上に小さいエラーバッジを重ねて示す。コンポーズ欄下部の既存エラー表示にもメッセージを出す。ユーザーは失敗項目を削除するか、投稿ボタンを再度押して再試行できる。再試行時、既に `kind: "drive"` になった項目はアップロード処理をスキップする。

## プログレス表示

既存の boolean `uploading` の代わりに、現在アップロード中の `local` 項目にのみ「アップロード中…」バッジを表示する(既存の "…" バッジと同等の簡素さを維持)。バイト単位の進捗バーはスコープ外とする。

## Rust側の変更

投稿・アップロードのコマンド自体(`upload_file` / `post_note` / `NoteDraft.file_ids`)は変更不要で、既存インターフェースのままこのフローに適合する。

ローカル画像プレビュー用に、新規コマンド `read_attachment_preview(path: String) -> Result<String>` を `src-tauri/src/commands/note.rs` に追加する。`src-tauri/src/commands/mute.rs` の `read_file_as_data_url` ヘルパー(`pub(crate)` に変更して再利用)に、画像拡張子(png/jpg/jpeg/gif/webp)のみを判定する新しい mime 判定関数を渡して実装する。`specta_builder()` への登録が必要。

## テスト方針

自動テストの対象外(フロントは `pnpm check`、Rust側は無変更のため `cargo test` 追加なし)。`cargo tauri dev` による手動確認を行う:

1. ローカル画像を複数選択し、送信前はアップロードされずプレビューのみ表示されることを確認する。
2. 投稿を実行し、順にアップロードされた後ノートが作成されることを確認する。
3. 存在しないパス等でアップロードを意図的に失敗させ、投稿が中止されエラー表示されること、再送信時に成功済みファイルが再アップロードされないことを確認する。
4. 添付したまま投稿せずにコンポーズ欄を閉じても、ドライブに何もアップロードされていないことを確認する(本Issueの主目的)。
