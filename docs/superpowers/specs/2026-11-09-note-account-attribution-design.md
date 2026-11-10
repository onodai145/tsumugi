# ノートキャッシュのアカウント帰属(`note_account`)設計 (Issue #557 の前提作業)

## 背景

Issue #557(MCPサーバー)で、キャッシュ済みノートを「公開を許可したアカウントの分だけ」外部(LLM)へ返したい。しかし現在のキャッシュ(`note` テーブル)は**全アカウント共通**で、どのアカウントが取得したノートかを直接は持たない。アカウントとの関係は `column_note(column_id, note_id)` → `Column.account_id` という間接経路だけである。

この間接経路には次の問題がある。

- **カラムの寿命に左右される。** カラムを削除すると `column_note` が消え、帰属も消える。
- **`from cache` カラムによる帰属の混入。** `from cache` を含むカラムは、グローバルなキャッシュを読んだ表示結果(`display_acc`)を、そのカラムの `column_note` に書き戻す(`commands/column.rs` の `cacheable = if use_cache { display_acc.clone() } …`)。そのため、アカウント B 由来のノートが、アカウント A の `from cache` カラムを経由して A のものとして記録される。
- キャッシュDBとアカウント/カラム定義は別の場所にあり、SQL の JOIN で結べない。`column_id IN (…)` を呼び出し側が組み立てる必要がある。

そこで「どのアカウントの取得経路で得たか」を明示的に記録する `note_account` を導入する。MCP とは独立に導入でき、後続の MCP 実装(別 spec)がこれを使う。

## 目標 / 非目標

目標:

- ノートがアカウントの取得経路(ストリーミング / REST の取得・ギャップ埋め)で得られた事実を `note_account` に記録する。
- 帰属が不確かな場合は**記録しない側**(=外部に出ない側)に倒れる。
- SQLite / PostgreSQL / MySQL の3 backend で同じ意味を持つ。

非目標:

- 検索 API の変更(`search_cache` へのアカウント条件の追加)。MCP 側の spec で扱う。
- UI の変更。帰属はユーザーに見せない内部情報。
- GUI の挙動の変更。既存の `cache_notes` / `cache_note` のシグネチャと挙動は変えない。

## 設計

### データモデル

```sql
CREATE TABLE IF NOT EXISTS note_account (
    account_id TEXT NOT NULL,
    note_id    TEXT NOT NULL,
    PRIMARY KEY (account_id, note_id)
);
CREATE INDEX IF NOT EXISTS idx_na_note ON note_account(note_id);
```

- 主キーは `account_id` 先頭で、「アカウントの全ノート」を引ける。
- `note_id` のインデックスは、prune 時の連鎖削除と「このノートは誰の帰属か」の参照用。
- `created_at` などは持たない。必要になれば `note` を JOIN する。
- PostgreSQL / MySQL は既存の `column_note` と同様に sea-query の `Table::create().if_not_exists()` で作る(型は `text`)。MySQL の `text` は主キーに使えない可能性があるため、既存 `column_note` の MySQL 側の定義に合わせる(実装時に確認)。

### 書き込み経路(trait へ追加するメソッド)

既存メソッドは変えず、`NoteCacheBackend` に次のメソッドを**追加**する。

```rust
/// アカウントがこれらのノートを取得経路で得たことを記録する。冪等。
async fn record_note_accounts(&self, account_id: &str, note_ids: &[String]) -> Result<()>;
/// 既存の column_note から、あるカラムのノートをアカウントの帰属として複写する(埋め直し用)。冪等。
async fn backfill_note_accounts(&self, account_id: &str, column_id: &str) -> Result<()>;
/// アカウント削除時に、そのアカウントの帰属を全て消す。
async fn forget_account_notes(&self, account_id: &str) -> Result<()>;
/// このノートがそのアカウントに帰属しているか(テストと、後続の MCP の `get_note` が使う読み取り口)。
async fn account_has_note(&self, account_id: &str, note_id: &str) -> Result<bool>;
/// 帰属が1件もないか(埋め直しを一度きりの移行にするための判定)。
async fn note_account_is_empty(&self) -> Result<bool>;
```

`NoteCacheStore` にも同名の委譲メソッドを足す。

書き込み元(非テスト)は3か所で、いずれも `cache_notes` / `cache_note` の**直後**に `record_note_accounts` を呼ぶ。

1. `stream/connection.rs`(ストリーミング受信): `account_id` がスコープ内にある。
2. `commands/column.rs` の `cache_fetched`(取得結果の書き込み)。
3. `commands/column.rs` の `commit_gap_fill_writes`(ギャップ埋め)。

2 は `FilteredFetch` に `attribute_to: Option<String>`(記録先のアカウント ID)を持たせ、`cache_fetched` がそれを見て記録する。3 は `commit_gap_fill_writes` に同名の引数を追加する。どちらも呼び出し元が `ResolvedSources.use_cache` から決める。

**`from cache` を含むカラムの REST 取得・ギャップ埋めでは記録しない(`attribute_to = None`)。** その結果にはグローバルなキャッシュ由来のノートが混ざり、そのアカウントが取得した証拠にならないため。一方、**ストリーミング受信(1)は常に記録する。** 受信したノートは、そのアカウント自身の WebSocket から届いた事実であり、`from home, cache` のような混在カラムでも変わらない。

記録の失敗では、取得処理(境界の延長を含む)を失敗させない。警告ログだけ出す(未記録は「見えない」側に倒れるだけのため)。

2段階書き込み(`cache_notes` → `record_note_accounts`)は原子的でない。間でクラッシュすると帰属が欠けるが、欠けた場合は「見えない」側に倒れるだけで、外部への漏れにはならない。

### 削除・寿命

- **prune / ノート削除**: 3 backend の `delete_matching` 系が走査する固定テーブル一覧(`column_note`, `note_reaction`, …)に `note_account` を加え、ノートと一緒に削除する。
- **カラムのキャッシュ消去(`clear_column_notes`)**: `note_account` は消さない。ノート本体(`note`)も残るため、帰属も残すほうが一貫する。
- **アカウント削除**: `remove_account` から `forget_account_notes` を呼ぶ。

### 既存データの埋め直し

起動時と、キャッシュのバックエンドを切り替えた直後に、アプリ層で、各アカウントのカラムのうち `from cache` を含まないものについて `backfill_note_accounts(account_id, column_id)` を呼ぶ。`from cache` を含むかの判定は、ネットワークを使わない純粋な関数(TQL のパースのみ。パース失敗は「含む」として除外)で行う。`INSERT … SELECT note_id FROM column_note WHERE column_id = ?` を `ON CONFLICT DO NOTHING`(MySQL は `INSERT IGNORE`)で行い、冪等にする(既存のマイグレーション方針と同じ: マーカー用テーブルを持たない)。

**一度きりの移行として、`note_account` が空のときだけ実行する。** `column_note` は「そのカラムの今の定義で取得したもの」とは限らない。たとえば設定のインポートは、`column_note` を消さずにカラム定義(同じカラム ID のまま `from cache` の有無や、アカウント)を入れ替えるため、毎回の起動で埋め直すと、別アカウント由来のノートを帰属させ直しうる(フェイルオープン)。新しい取得はすべて取得経路で記録されるので、埋め直しは移行の1回で足りる。起動直後にストリーミングが先に記録した場合は、埋め直しを飛ばす(見えない側に倒れる)。

- 過去に `from cache` へ設定変更されたカラムは、当時の状態が分からない。現在 `from cache` を含むカラムは除外する(安全側)。
- 既知の限界: 移行の1回目に限り、`column_note` がカラム定義と食い違っていると(設定インポートで定義が入れ替わった後の残骸など)、別アカウント由来のノートが帰属しうる。通常のカラム編集(`update_column`)は `column_note` を消すので、この食い違いは設定インポートや、同じ DB を共有する複数端末の定義のずれからだけ生じる。
- 既知の限界: `forget_account_notes` は、アカウント削除時にアクティブな backend にしか効かない。削除の後に別の backend へ切り替えると、その DB には削除済みアカウントの帰属が残る。また、アカウント削除と実行中の取得が競合すると、削除済みアカウントの帰属が再記録されうる(削除の前後に2回 `forget` を呼んで窓を狭めているが、完全には塞げない)。アカウント ID は再利用されないため、実害はない(設定インポートで同じ ID を戻した場合は、同じ本人のアカウント)。

## 安全性の性質

- 記録されない場合は常に「見えない」側に倒れる(fail-closed)。
- `from cache` カラム経由では記録されない。
- 帰属が増える経路は、アカウント自身の取得経路(ストリーミング・REST)だけ。

## テスト

- SQLite(通常のユニットテスト)と、PostgreSQL / MySQL(`#[ignore]` の実 DB テスト。既存の `TestBackend` を使う)の3 backend で次を検証する。
  - 記録が冪等であること、複数アカウントが同じノートに帰属できること。
  - prune で `note_account` が連鎖削除されること。
  - `forget_account_notes` が対象アカウントだけを消すこと。
  - `backfill_note_accounts` が `column_note` から複写し、再実行しても変わらないこと。
- アプリ層: `from cache` カラムの取得結果では `record_note_accounts` が呼ばれないこと。通常カラムでは呼ばれること。
- 埋め直し: `from cache` を含むカラムが除外されること。

## 要確認事項(plan で解消する)

- PostgreSQL / MySQL の DDL は実 DB テスト(`#[ignore]`)で確認する。MySQL の ID 列は既存の `column_note` と同じ `string_len(64)` を使う(`text` は主キーにできないため)。

解消済み: `account_id` と `use_cache` の渡し方(上記「書き込み経路」)、3 backend の prune 対象テーブル一覧(それぞれに `note_account` を追加する)。

## 後続作業

MCP 本体(別 spec): `note_account` を使い、`search_cache` の where 句に `n.id IN (SELECT note_id FROM note_account WHERE account_id = ?)` と公開範囲条件を AND する。
