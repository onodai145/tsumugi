# サーバーサイド検索 設計

Issue: #430

## 背景 / 目的

現状、Misskey サーバーの検索機能（`notes/search`）を使うには Search カラムを作る必要がある。
キャッシュDB検索（検索モーダル、Issue #248）と同じ導線に載せ、カラムを作らずに一回性の
サーバー検索をできるようにする。

## 合意済みの前提

- 導線は既存の検索モーダル（`SearchModal.svelte`）。「検索対象: キャッシュ / サーバー」を切り替える。
- サーバー検索で使える条件: キーワード、ユーザー、インスタンス、日時範囲。
  - 「エキスパート(TQL)」はサーバー検索では使えない（`notes/search` が TQL を受け付けないため）ので隠す。
- 日時範囲はサーバーのバージョンによって対応が異なるため、**バージョンで分岐する**（案1）。

## 調査結果（Misskey `notes/search`）

OpenAPI は `/api.json` で取得できる（`/api-doc.json` ではない。後者は SPA の HTML が返る）。
`misskey.omhnc.net`（2026.9.1）と `dev.misskey.omhnc.net`（2026.10.0-alpha.0）で確認済み。

| 条件 | パラメータ | 備考 |
|---|---|---|
| キーワード | `query`（必須） | 空キーワードの挙動は未確認。使わない（UIで必須にし、Rustでも拒否する） |
| ユーザー | `userId` | `@acct` から userId への解決が必要（`resolve_user`） |
| インスタンス | `host` | ローカルは `"."` |
| 日時 | `sinceDate` / `untilDate`（ミリ秒） | 2025.7.0 以降。ID境界に変換される。`untilId` が指定されていればそちらが優先 |
| 日時（別系統） | `rangeStartAt` / `rangeEndAt`（ミリ秒） | 2026.6.0 以降。本設計では使わない |
| ページング | `untilId` | Meilisearch 経路でも `untilId` は createdAt 条件に変換されるため有効（上流 `SearchService.ts` で確認） |

同梱スナップショット（`src-tauri/openapi/misskey-api-doc.json`）は 2025.4.1 で古く、日時系パラメータが載っていない。
本機能では REST クライアントを手書きしているため、スナップショットの更新は行わない。

## 仕様

### UI（`SearchModal.svelte`）

- アカウント選択の下に「検索対象」のセグメント（キャッシュ / サーバー）を追加。既定はキャッシュ。
- アカウント欄の注記は検索対象で切り替える。
  - キャッシュ: 現行のまま（検索条件には影響しない）。
  - サーバー: そのアカウントのサーバーに問い合わせる旨。
- サーバー選択時:
  - 「エキスパート(TQL)」タブを隠し、簡単モードに固定する（TQL入力は保持しない。キャッシュに戻すと空に戻ってよい）。
  - キーワードを必須にする（空なら検索ボタン無効）。
  - 「詳細条件」のうち、ユーザー・インスタンスは常に表示。日時（開始/終了）は `dateRange` が true のときだけ表示。
  - `dateRange` が false、または能力の取得に失敗/未取得のときは日時欄を出さず、
    取得完了後に false だった場合のみ「日時範囲の指定は Misskey 2025.7.0 以降のサーバーで利用できます」と表示する。
  - インスタンス欄の注記: 自インスタンスの検索には自身のホスト名または `.` を入力できる。
- 追加読み込み・再試行は、検索ボタンを押した時点の条件を使う（入力欄を書き換えても連結されない）。
- 検索対象を変えたら、結果をクリアして取得世代（`requestGen`）を進める。アカウントを変えたときも、
  サーバー検索のときだけ同様にクリアする（キャッシュ検索の結果はアカウントに依存しないため、現行どおり保持）。
- 結果表示・追加読み込み・重複除去・エラー再試行は既存ロジックをそのまま使う。

### Rust

#### 新規: `SearchCapabilities`（`domain/`）

```rust
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchCapabilities { pub date_range: bool }
```

純粋関数（同ファイル）:

- `parse_misskey_version(&str) -> Option<(u32, u32, u32)>`: 先頭の `YYYY.M.P` のみを読む
  （`-alpha.0` / `-io.12b-...` などのサフィックスは無視。要素が足りない/数値でなければ None）。
- `search_capabilities(version: Option<&str>) -> SearchCapabilities`:
  パース成功かつ `>= (2025, 7, 0)` なら `date_range: true`。それ以外（None含む）は false。

将来 `/api.json` から実際の対応パラメータを判定する方式（約1.3MB）へ置き換える場合も、
`SearchCapabilities` を介してこの関数の中身を差し替えるだけで済む。

#### `api/meta.rs`

- `fetch_server_version(client) -> Result<Option<String>>` を追加（`meta` を `detail:false` で叩き `version` だけ読む）。
  `InstanceInfo` / `RawMeta` は変更しない（`InstanceInfo` は `User.instance` とキャッシュDB列と共用のため）。

#### `AppState`

- `server_versions: Mutex<HashMap<String, String>>`（account_id → version）を追加（`emoji_cache` 等と同じパターン）。
  取得に成功した値のみ保存し、失敗は保存しない（次回再試行）。プロセス存続中は再取得しない（サーバー更新はアプリ再起動で反映）。

#### `api/notes.rs`

- `build_search_body(&SearchParams) -> serde_json::Value`（純粋関数）と `search_notes(client, &SearchParams)` を追加。
  `SearchParams { query, user_id, host, since_date_ms, until_date_ms, until_id, limit }`。
  `None` のキーはボディに含めない。

#### コマンド（`commands/column.rs`、`lib.rs` の `specta_builder()` に登録）

- `get_search_capabilities(account_id) -> SearchCapabilities`
  - キャッシュ済みならそれを使い、無ければ `fetch_server_version` で取得して保存。取得失敗は `date_range: false` を返す（エラーにしない）。
- `search_server_notes(account_id, query, acct: Option<String>, host: Option<String>, since_date: Option<u32>, until_date: Option<u32>, until_id: Option<String>, limit: u32) -> Vec<Note>`
  - 日時はフロントの TQL `created_at` と同じ**秒**で受け、Rust でミリ秒へ変換する
    （specta は i64 を TS へ出せないため `u32`。2106年まで有効）。
  - `query` が空白のみなら `Error::Invalid`。
  - 日時が指定されたのに `get_search_capabilities` が `date_range: false` なら `Error::Invalid`（UIが隠すための二重防御）。
  - `acct` が指定されていれば `resolve_user` で `user_id` に解決。失敗はそのままエラー。
  - `host` はトリム後、空なら省略。アカウント自身のホスト（大文字小文字無視）または `.` なら `"."` に正規化。
  - 取得結果に、キャッシュ検索と同じミュート（ローカル・サーバーミュート・ワードミュート）を適用する。
    `search_cache_core` と同じクロージャ（`server_muted_note` / `is_word_muted`）を使う。
  - 並びはサーバー応答（id降順）のまま。ページングは `until_id`（最後のノートID）。
  - 開始日時だけが指定され、`until_id` も無いときは、現在時刻を `untilDate` として補う。
    Misskey の `makePaginationQuery` は `sinceId` だけだと昇順（開始に近い側から）で返すため、
    補わないと1ページ目が最新ではなく最古側になり、ページングも重複・取りこぼしを起こす（実機で確認）。
  - ミュートで1ページが全滅しても、生応答が `limit` 件ちょうどなら生の最後のidから最大5パス取り直す。
    画面側は空配列を「これ以上なし」と解釈するため、取り直さないと検索が途中で終わって見える。

### フロント

- `store.svelte.ts`: `searchServerNotes` / `getSearchCapabilities` を追加。エラー処理は `searchCacheNotes` と同じ
  （呼び出し元がエラー表示を持つので `#fail()` は呼ばず `#logFailure` のみ）。
- `bindings/tauri.gen.ts` は生成物。手編集しない。

## 範囲外（YAGNI）

- キャッシュとサーバーの同時検索・結果合成。
- サーバー結果への TQL 後段フィルタ。
- `channelId` 指定。
- `/api.json` による能力判定（将来の差し替え先は上記の通り用意しておく）。
- Misskey 以外のフォーク固有の判定。Sharkey 等はバージョン番号が本家と対応しない可能性があるが、未確認。
  番号が `>= 2025.7.0` でも実際は日時未対応のフォークでは、日時指定がサーバーに無視されうる
  （逆に対応済みでも番号が小さければ日時欄が出ないだけ）。この限界はユーザーガイドに記載する。

## テスト

- Rust 単体:
  - `parse_misskey_version`: `2026.10.0-alpha.0` / `2025.4.1-io.12b-fb6fbea074` / `2025.7.0` / 不正文字列。
  - `search_capabilities`: 2025.6.4=false、2025.7.0=true、2026.10.0-alpha.0=true、None=false。
  - `build_search_body`: 全項目あり/なし、`None` キーが出ないこと、ミリ秒変換。
  - ホスト正規化（自ホスト→`.`）、空クエリ拒否、能力なしでの日時拒否。
  - 能力キャッシュ（取得失敗は保存しない）は、HTTP を注入できる範囲で検証。難しければ純粋関数側のテストに留める。
- Rust 結合（`#[ignore]`）: `misskey.omhnc.net` / `dev.misskey.omhnc.net` に対する実接続で、`search_server_notes` と `get_search_capabilities` が動くこと
  （要トークン。既存の実接続テストの流儀に合わせる）。
- Vitest（`SearchModal.test.ts`）:
  - サーバー切替でTQLタブが隠れ、キーワード空で検索不可。
  - `dateRange` true のとき日時欄が出て、秒値が `search_server_notes` へ渡る。
  - false/失敗のとき日時欄が出ず、false のとき注記が出る。
  - 検索対象切替で結果がクリアされる。
- 実機確認: Xvfb + `dbus-run-session` + `WAYLAND_DISPLAY` unset の隔離環境で、実インスタンスに対する検索を確認する。

## ドキュメント

- `docs/guide/user-guide.md` の「検索」節: 検索対象の切替、サーバー検索の条件、日時範囲は Misskey 2025.7.0 以降が必要、
  フォーク等で効かない場合があること。
