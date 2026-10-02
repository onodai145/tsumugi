# ノート翻訳 設計

Issue: #440

## 背景 / 目的

タイムライン上の外国語ノートを、読める言語に翻訳して確認できるようにする。
Issue 本文は空（タイトル「翻訳機能」のみ）。以下は設計相談で合意した内容。

## 合意済みの前提

- 翻訳エンジンは接続先 Misskey サーバーの `notes/translate`。外部翻訳 API の直接利用はしない
  （API キー管理・本文の外部送信が不要になるため）。
- 翻訳先言語は設定で固定指定（既定 `ja`）。翻訳ごとの言語選択はしない。
- 起動は NoteMenu の「翻訳」項目。結果は NoteCard の本文直下に展開する。
- 翻訳結果のキャッシュはしない（メモリ・永続とも）。閉じて再度翻訳すれば再取得する。
- 設定項目は `ExternalIntegrationSection` に置く。

## 調査結果（Misskey `notes/translate`）

`misskey.omhnc.net`（2026.9.1）と `dev.misskey.omhnc.net`（2026.10.0-alpha.0）の `/api.json` で確認済み
（User-Agent 無しの取得は 403 になるため、ブラウザ相当の UA を付ける）。

| 項目 | 内容 |
|---|---|
| リクエスト | `noteId`（必須）, `targetLang`（必須） |
| 200 | `{ sourceLang: string, text: string }` |
| 204 | 本文なし。結果なしの意味（翻訳元が空・同一言語など）。**ボディが空なので JSON として読めない** |
| 400 `UNAVAILABLE` | サーバーで翻訳機能が未設定 |
| 権限 | `read:account` |
| 可否判定 | `/meta` の `translatorAvailable: boolean`。`MetaLite` に含まれるため `detail:false` でも取得できる |

同梱スナップショット（`src-tauri/openapi/misskey-api-doc.json`）は古い。REST クライアントは手書きなので更新しない。

## 仕様

### Rust

- `api/notes.rs`: `translate_note(client, note_id, target_lang) -> Result<Option<Translation>>`。
  - 204 は `Ok(None)`。`client.post` が空ボディをデシリアライズできない場合は、空ボディ対応の経路を
    `client.rs` 側に用意する（実装時に既存 `post` の挙動を確認して決める）。
  - 400 `UNAVAILABLE` は専用のエラー（フロントで「このサーバーは翻訳に対応していません」と表示できる区別）にする。
- `api/meta.rs`: `fetch_translator_available(client) -> Result<bool>`。`fetch_server_version`（#430）と同様、
  `InstanceInfo` には足さず独立に取得する（`InstanceInfo` はキャッシュ DB 列・`User.instance` と共用のため）。
- `domain/note.rs`（または適切な場所）: `Translation { source_lang: String, text: String }`（`specta::Type`、camelCase）。
- `state.rs`: `server_version` と同じ形で `translator_available` を `AppState` にキャッシュする。
  取得失敗は「非対応」扱いにし、失敗はキャッシュしない（次回また取りに行く）。
- `commands/note.rs`:
  - `get_translator_available(account_id) -> bool`
  - `translate_note(account_id, note_id) -> Option<Translation>`
    — 翻訳先は `UiPrefs.translate_target_lang` を Rust 側で読む（フロントから渡さない）。
  - どちらも `specta_builder()`（`lib.rs`）に登録する。
- `domain/ui.rs`: `UiPrefs` に `translate_target_lang: String` を追加。
  `#[serde(default = "default_translate_target_lang")]`（`"ja"`）で、追加前の JSON も読めるようにする。
  `Default` 実装・既存テストの網羅リテラルも更新する。空文字や空白のみは保存時に `"ja"` へ戻す。

### フロントエンド

- `ui/settings/ExternalIntegrationSection.svelte`: 「翻訳先言語」を追加する。
  - 言語コードのテキスト入力ではなく、主要言語のプリセット選択にする（`ja` / `en` / `zh` / `ko` / `fr` / `de` / `es` / `pt` / `ru` など）。
    Misskey サーバーは DeepL / LibreTranslate の言語コードを受けるため、プリセットは両者で通る大文字小文字区別なしのコードにする。
    どのコードをサーバーに渡すかの正規化は実装時に確認する（DeepL は `EN-US` 等を要求する場合がある）。
  - 既存の `setUiPrefs({ ...app.ui, ... })` の保存方式に合わせる。
- `ui/NoteMenu.svelte`: 「翻訳」項目を追加する。
  - 表示条件: 対象ノート（純リノートはリノート先）に `text` があり、かつ `translatorAvailable` が true。
  - 対象ノートの判定は既存の `note` / `pureRenoteOf` の扱いに合わせる。
  - 項目を押すとメニューを閉じ、翻訳要求を NoteCard 側に伝える（`app` ストア経由かコールバックかは実装時に既存パターンへ合わせる）。
- `ui/NoteCard.svelte`: 本文直下に翻訳ブロックを展開する。
  - 状態: 読み込み中 / 成功 / 結果なし（204）/ 未対応 / エラー。
  - 成功時は「{検出元言語} から翻訳」と翻訳文、「翻訳を閉じる」ボタンを出す。
  - 翻訳文はプレーンテキスト（MFM としては描画しない。サーバーは翻訳済みプレーンテキストを返し、絵文字解決等はできない）。
    改行は `whitespace-pre-wrap` で保つ。
  - 状態はコンポーネント内に持ち、永続化しない。ノートが入れ替わったときは破棄する。
  - CW 付きノートは、CW が開いているときだけ本文（と翻訳）が見えるという現行の挙動に従う。CW 文自体の翻訳はしない。
- 視覚値（角丸・フォントサイズ・アイコンサイズ）は `docs/design/style-guide.md` のスケールに従い、一回限りの値は使わない。

### 既存機能との関係

- ミュート・フィルタ・キャッシュ DB・Streaming には触れない（ノートのデータ構造は変えない）。
- ユーザーガイド `docs/guide/user-guide.md` に翻訳の使い方を追記する。

## スコープ外

- 外部翻訳 API の直接利用 / 両対応
- 自動翻訳、翻訳ボタンの言語別出し分け
- 翻訳結果のキャッシュ（メモリ・永続）
- 翻訳ごとの翻訳先言語の選択
- CW 文・投票選択肢の翻訳

## テスト

- Rust
  - `translate_note`: 200 のパース、204 で `None`、400 `UNAVAILABLE` の専用エラー（wiremock 等、既存 API テストの流儀に合わせる）。
  - `fetch_translator_available`: true / false / 欠落（false 扱い）。
  - `UiPrefs`: 旧 JSON（`translate_target_lang` 無し）が `"ja"` で読めること、保存時の空文字補正。
  - `cargo test` の `generates_frontend_bindings` で新コマンド・型が TS バインディングに出ること。
- Vitest
  - NoteMenu: 本文なし / 翻訳不可アカウントで項目が出ない、出る条件で出る。
  - NoteCard: 読み込み中 → 成功の表示、「翻訳を閉じる」、204・未対応・エラーの各表示。
  - ExternalIntegrationSection: 翻訳先言語の保存が他フィールドを消さない。
- 実機確認
  - Xvfb（`WAYLAND_DISPLAY` も unset）上の `cargo tauri dev` で、翻訳が有効な実インスタンスに対し確認する。
  - 未対応サーバーでメニュー項目が出ないことも確認する。
