# 検索機能（キャッシュDB検索）設計

Issue #248 対応。「ユーザーID・インスタンス・日時を指定してキャッシュDBから検索したい」という要望に対する設計。

## 背景・現状

TQL（`docs/design/filter-dsl-design.md`）には既に `cache` ソース（受信せずローカルSQLiteキャッシュのみを検索）があり、バックエンドは既に配線済みである：

- `filter/sql.rs`: TQL AST → SQLite WHERE句への射影（`build_where`）
- `store/note_cache.rs::search_cache()`: SQLite `note` テーブルを実際に検索
- `commands/column.rs` の `fetch_and_filter`: `cache` ソースを含むカラムの更新時に上記2つを呼び出し、SQL射影での粗いフィルタ後、in-memory の `CompiledFilter::matches` で再検証する二段構成
- TQLの語彙には `user.acct`（`@user@host`形式）、`host`（インスタンス）、`created_at`（epoch秒）が既にある

しかし現状これに到達する手段は `AddColumnModal.svelte` の「エキスパート(TQL)」モードで生のTQL文字列を手書きする以外になく、`cache` はガイドモードのソース選択肢にも入っていない。つまりTQL構文を知らないユーザーは事実上この機能を使えない。

本設計は、この既存バックエンドをそのまま使い、専用の検索モーダルUIを新設する。カラムの永続化は伴わない一回性の検索とする。

## 1. アクセス導線

`ui/AppMenu.svelte` に既存の「カラム追加」「設定」と並べて「検索」メニュー項目を追加する（`Search` アイコン、`@lucide/svelte`）。クリックで新規 `ui/SearchModal.svelte` を開く。`App.svelte` に `showSearch` の state を追加し、他のモーダルと同様の開閉パターンに従う。

## 2. 検索フォーム（SearchModal.svelte）

### ガイドモード（既定）
固定入力項目。空欄の項目は述語を生成しない。全項目が空なら `from cache`（無条件、新着順)。

| 項目 | UI | 生成されるTQL述語 |
|---|---|---|
| キーワード | テキスト入力 | `text -> "..."` |
| ユーザー | テキスト入力（`@user@host`形式） | `user.acct == "..."` |
| インスタンス | テキスト入力（ローカルは空欄のまま） | `host == "..."` |
| 日時（開始） | `datetime-local` | `created_at >= <epoch秒>` |
| 日時（終了） | `datetime-local` | `created_at <= <epoch秒>` |

複数項目を入力した場合は `&&` で結合する。エスケープは `AddColumnModal.svelte` の `tqlStr()` と同等のロジックを共有ユーティリティとして切り出して再利用する。

### エキスパート(TQL)モード
`AddColumnModal.svelte` と同じUXのトグルボタンで切り替える。切替時、ガイドモードで組み立て済みのTQL where句を種として引き継ぐ（既に何か書きかけていれば上書きしない）。`input/TqlCompletionField.svelte` を使い、where句のみを編集する（source節は `cache` 固定で編集不可）。

既知の制約（仕様として明記）：`mine` / `following` / `@account` 系の述語は、検索が特定アカウントに紐づかないため `following_ids: None` で評価され、常に不一致になる。必要になれば別Issueで拡張する（YAGNI）。

### アカウント選択
`AccountSelect` コンポーネントを配置。検索条件そのものには影響しない（キャッシュは全アカウント共通）が、検索結果 `NoteCard` の操作（返信・リノート・リアクション）をどのアカウントとして行うかを決める。既定は既定アカウント。

## 3. バックエンド

新規コマンド（`commands/column.rs` に追加。cache検索専用の小さな関数のため新規ファイルは作らない）:

```rust
#[tauri::command]
pub async fn search_cache_notes(
    state: State<'_, AppState>,
    filter: FilterQuery,       // FilterQuery::Tql(where句文字列) を受け取る
    until_id: Option<String>,
    limit: u32,
) -> Result<Vec<Note>>
```

処理内容：
1. `CompiledFilter::compile(&filter)` でパース（既存、空文字列は `PassAll` になる）。
2. `sql::SqlCtx { my_ids: state.eval_context().my_user_ids.into_iter().collect(), following_ids: None }` を組み、`CompiledFilter::Tql` の場合のみ `sql::build_where` でSQL WHERE句を生成（`PassAll` の場合は `"1=1"` / 空パラメータ）。
3. `state.cache.search_cache(&where_sql, until_id.as_deref(), limit)` でSQLite検索。
4. 取得したノート列を、`fetch_and_filter` と同じ二段フィルタ（`CompiledFilter::matches` + `crate::filter::mute::is_muted` によるミュート除外）にかける。
5. `created_at` 降順・`id` 降順でソートし、`limit` に切り詰めて返す。

`specta_builder()` に登録し、`cargo test`（`generates_frontend_bindings`）でTSバインディングを再生成する。

## 4. 検索結果表示

- モーダル内リスト（別ページ遷移はしない）。`ui/FollowListModal.svelte` と同じ無限スクロールパターンを踏襲する：
  - スクロール残り300pxで `search_cache_notes` を `until_id`（現在の結果末尾ノートのID）付きで追加取得
  - 世代カウンタ（`requestGen`）は「検索」フォーム送信時にのみ進め、結果をリセットして再検索する。フィールドやアカウントの変更だけでは自動的には再検索しない（明示的な再送信が必要）。検索中にアカウントを変更しても、既に表示中の結果の取得条件は変わらない — 以降「もっと見る」でページ追加取得する際にどのアカウントのミュートリストを適用するかにのみ影響する。
- 各行は `ui/NoteCard.svelte`（`accountId` に選択中アカウントを渡し、通常カラムと同様に操作ボタンを表示）。
- 「検索条件を保存してカラム化」等の追加機能は今回のスコープ外（YAGNI）。将来必要になれば別Issueで、生成済みTQL文字列を `AddColumnModal` に渡す形で追加できる。

## 5. テスト

- Rust: `search_cache_notes` の単体テスト。既存の `store/note_cache.rs` の `search_cache` テスト（`search_cache_applies_predicate_and_until_id_boundary` 等）と同様のパターンで、フィルタ・ミュート除外・ページングを検証する。
- フロントエンド:
  - `SearchModal` のvitestテスト：フォーム入力→TQL組み立てロジックの単体テスト（`AddColumnModal` の `sourceDsl`/`guidedToTql` 相当のテストがあれば参考にする）
  - 無限スクロールの挙動は `FollowListModal` 相当のモックパターンで検証する

## スコープ外

- 検索条件を保存してカラム化する機能
- 検索専用の高度なUI（ハイライト、ファセット等）
- `mine`/`following`/`@account` 述語のアカウント紐づけ対応
