# note cacheの外部DB対応 設計 (Issue #115)

## 背景・動機

Issue #115「外部のデータベースを使えるようにする」。本文は空だったため、ヒアリングにより以下2点を動機として確認した。

1. 複数端末間でnote cacheを共有したい
2. 大規模データにおける性能・運用面で、SQLiteのファイルベース運用よりPostgreSQL等の既存DBサーバ運用に寄せたい

## スコープ

- **対象**: note cache(`store/note_cache.rs`が扱う`note`/`user`/`note_reaction`/`note_tag`/`note_mention`/`note_emoji`/`note_file`/`column_note`/`column_fetch_boundary`)のみ。`store/user_ref.rs`(`upsert_user`/`fill_user_from_snapshot`/`fetch_users_by_ids`、いずれも`&rusqlite::Connection`を取り`note_cache.rs`から呼ばれる)も同じ接続を共有するため対象に含む
- **対象外**: account/column/mute/notify/ui等の設定(`SettingsData`、プレーンJSONファイル)。account tokenはこれまでどおりOS keyringのみで、外部DBには一切保存しない
- **サポートするバックエンド**: SQLite(既定、現行踏襲)、PostgreSQL、MySQL

### column_idの端末間不一致について

`column_note`は`column_id`(=`column_def.id`、設定側で端末ごとに生成されるUUID)をキーに持つ。今回のスコープは設定を同期対象に含めないため、別端末では同じタイムラインでも`column_id`が別物になり、「どのカラムに何が流れたか」の復元は端末ローカルのままになる。外部DB化で得られる価値は`note`/`user`実体の重複キャッシュ削減・再取得コスト削減に限られる。この制約はヒアリングで許容と確認済み。

## アーキテクチャ

### バックエンド抽象化

`store/note_cache.rs`の`NoteCacheStore`は薄いラッパーとして残し、内部の接続を実行時選択可能なプールで持つ。

```rust
enum CachePool {
    Sqlite(sqlx::SqlitePool),
    Postgres(sqlx::PgPool),
    MySql(sqlx::MySqlPool),
}
```

起動時、および設定変更時に`SettingsData.cache_backend`を読み、対応するプールへ接続する。

### DBアクセス手段: sqlx + sea-query

- **接続・実行**: `sqlx`(features: `sqlite`, `postgres`, `mysql`, `runtime-tokio`)。3バックエンドを実行時に切り替える必要があり、コンパイル時ジェネリクスで縛られるDieselは不採用
- **SQL組み立て**: `sea-query`(+ `sea-query-binder`でsqlxと接続)。`INSERT ... ON CONFLICT` / `INSERT IGNORE`のようなUPSERT方言、プレースホルダ記法(`?` vs `$1`)、`LIMIT`等の差異を吸収し、クエリ構築コードを1箇所に保つ
- **フルORM(sea-orm)は不採用**: `note`本体+側テーブル(`note_reaction`等)をJOINしてJSON payloadに詰め直す現状の手書き行マッピングとEntity/ActiveRecordモデルの相性が悪く、移行コストが最大になるため
- 行→`Note`構造体へのマッピング(JOIN結果の集約、`note_reaction`等側テーブルの集約)は現状の手書きスタイルを維持する
- テーブルDDLも`sea-query`の`Table::create()`ビルダで記述し、3バックエンド分のCREATE TABLE文を一本化する
- サイズ見積り・eviction(現行`PRAGMA page_count`依存)はsea-queryで表現できないため、バックエンドごとの`match`分岐で個別実装する(Postgres: `pg_total_relation_size`、MySQL: `information_schema.TABLES`の`data_length`、SQLite: 現行の`PRAGMA`踏襲)

### 非同期化

`NoteCacheBackend`は`async-trait`によるasync traitとして定義する。既存の呼び出し元(`commands/column.rs`, `commands/mute.rs`, `commands/note.rs`, `stream/connection.rs`)はすべて既に`async fn`内から同期呼び出ししているため、各呼び出しに`.await`を追加する変更で足りる。

非同期化の波及範囲は上記4ファイルに留まらない。`store/db.rs`の`open_cache`/`open_cache_in_memory`もsqlx版に置き換わって`async fn`になるため、`lib.rs:234`(起動時の初期化)と`state.rs:136`(テスト用`AppState`構築ヘルパー)を呼び出し元として更新する。また`note_cache.rs`/`user_ref.rs`内の既存`#[test]`(合計約60件)は`#[tokio::test]`に置き換える。これらはTask 1(下記「実装の段階分割」参照)の一部として、対象ファイルを触るタスクにそれぞれ含める(別タスクに切り出さない)。

### filter/sql.rs(TQL `cache`ソース)の扱い

`search_cache`(`note_cache.rs`)は`filter/sql.rs`の`build_where`が返す`SqlWhere { sql: String, params: Vec<SqlParam> }`(`?`プレースホルダのSQL断片)をそのまま自前のSELECT文へ埋め込んでいる。PostgreSQLは`$1`形式の番号付きプレースホルダを要求するため、この生SQL断片は無変更ではPostgres/MySQL化を跨げない。

Phase 1はSQLiteのみを対象とし、sqlxのSQLiteドライバは`?`プレースホルダをそのまま受け付けるため、`search_cache`とその内部で組み立てているSQL文字列・`SqlParam`バインドは**今回は変更しない**(sea-query化の対象外とする)。`filter/sql.rs`の`build_where`をバックエンド非依存な形(sea-queryの`Cond`/`SimpleExpr`を返す、またはバックエンドごとにプレースホルダを採番し直す)へ変更するのは、Postgres対応を行うPhase 2のタスクとして扱う。

### 依存クレート追加

- `sqlx`(features: `sqlite`, `postgres`, `mysql`, `runtime-tokio`)
- `sea-query`, `sea-query-binder`
- `async-trait`
- 既存の`rusqlite`は設定関連の旧移行コード(`db.rs`の`open_settings`、旧SQLite一体型からの一回限り移行)のために残す。note cache側の`rusqlite`利用は今回撤去する

## 複数端末の同時書き込みに関する整合性

- `note`/`user`テーブルはそれぞれ`id`が主キーのため、UPSERTは常に対象IDの1行に対する原子的な書き込みになる。複数端末が同じノートを同時にキャッシュしても、後勝ちで上書きされるだけで重複行や壊れた行は生まれない
- 一方`note_reaction`/`note_tag`/`note_mention`/`note_emoji`/`note_file`テーブルは主キー・UNIQUE制約を持たず、現行コードは「対象noteの既存行を全DELETE→INSERTし直す」という複数文パターンで書き換えている。単一プロセス+単一SQLite接続(Mutexで直列化)では安全だが、外部DBに複数端末が同時に同じノートを書き込むと、2つのトランザクションのDELETE/INSERTが交互に割り込む余地があり、一時的な重複行や(読み取りタイミングによっては)瞬間的な空状態が起こり得る
- 対策として、これら側テーブルに以下のUNIQUE制約を追加し、DELETE+INSERTパターンをsea-query経由のUPSERT(`ON CONFLICT` / `ON DUPLICATE KEY UPDATE`相当)に置き換える。この変更はSQLiteバックエンドにも同様に適用し、単一プロセスでも冪等性を高める:
  - `note_reaction`: `UNIQUE(note_id, emoji_key)`
  - `note_tag`: `UNIQUE(note_id, tag)`
  - `note_mention`: `UNIQUE(note_id, user_id)`
  - `note_emoji`: `UNIQUE(note_id, emoji)`
  - `note_file`: `UNIQUE(note_id, mime_type, mime_category, is_sensitive)` — `DriveFile`にはid列を持たせておらず、これは自然キーではなく便宜上の重複排除キーである。理論上、同一mime種別・同一sensitiveフラグの添付ファイルが2つ以上あるノートでは行が縮退しうるが、`filter/sql.rs`でのこのテーブルの参照は`EXISTS`/相関サブクエリのみ(`COUNT`は使わない、Reactions/Tags/Mentions/Emojisも同様)であり、表示用の完全なファイル一覧は`note.payload`のJSONが真実の情報源のままなので、フィルタ述語の結果には影響しない
- このUNIQUE制約追加+UPSERT化は、実装の段階分割の1番目(`NoteCacheBackend` trait抽出 + `SqliteBackend`移植)に含める

### 既存キャッシュDBへのUNIQUE制約追加について

`CREATE UNIQUE INDEX`は既存データに重複行があると失敗し、起動時マイグレーションの失敗はアプリ起動不能に直結する。キャッシュは「破棄しても再取得で復元できる」設計(`db.rs`)なので、重複除去マイグレーションは行わない。代わりにキャッシュのスキーマバージョンを上げ、旧バージョンのキャッシュDBファイルは(WAL/SHMファイルごと)削除して新スキーマで作り直す方式を取る。

## `delete_matching`のコネクションプール対応

現行の`delete_matching`(prune処理)は`CREATE TEMP TABLE prune_ids AS SELECT ...`で一時テーブルを作り、複数の`DELETE ... WHERE note_id IN (SELECT id FROM prune_ids)`文と`DROP TABLE`をまたいで参照している。`sqlx::SqlitePool`（既定で複数コネクション）配下では、これらの文が別々のコネクションに割り当てられ得るため、TEMP TABLEが「無い」扱いになり壊れる。

対策として、TEMP TABLEを使わず、対象ノートIDを一度Rust側の`Vec<String>`として確定させてから、それを`IN (...)`リストとしてsea-queryで組み立てた各DELETE文にバインドする方式に変更する。これによりコネクションをまたいでも安全になり、かつPostgres/MySQLでも同じロジックをそのまま使える(TEMP TABLE構文の方言差を考えずに済む)。

## 設定・接続情報

- `SettingsData`(`store/settings.rs`、プレーンJSONファイル)に`cache_backend: CacheBackendConfig`フィールドを追加(`mute`/`notify`と同じ並び)
- `CacheBackendConfig`は`#[serde(tag = "type")]`のenum:
  - `Sqlite`(既定)
  - `Postgres { host, port, database, user }`
  - `MySql { host, port, database, user }`
- パスワードのみOS keyring(既存のaccount tokenと同じ`keyring`クレート経由)に保存し、JSONファイルには含めない
- 設定UIは`frontend/src/ui/settings`配下に新規セクションを追加する

## 接続失敗時の挙動

起動時・バックエンド切替時に外部DBへ接続できない場合はエラーを表示し、SQLiteローカルキャッシュへ自動フォールバックしてアプリの起動・利用を継続する(キャッシュは破棄前提・再取得で復元できる設計を踏襲)。

## データ移行

既存のSQLiteキャッシュから外部DBへの自動移行は行わない。バックエンド切替時は空の状態から始まる(再取得で復元される)。

## テスト

- 現行の`note_cache.rs`内テスト群を`NoteCacheBackend` trait経由の呼び出しに書き換え、`SqliteBackend`で従来どおり実行する(単体テスト)
- Postgres/MySQLは`testcontainers`等でDocker上に一時インスタンスを立てる統合テストを追加する。既存の実Misskey接続テスト(`#[ignore]`)と同様、CI常時実行はしない方針で揃える

## 実装の段階分割(1設計・複数PR)

1. `NoteCacheBackend` trait抽出 + `SqliteBackend`への挙動そのまま移植(sqlx化・非同期化のみ、外部から見た挙動は不変)
2. `PostgresBackend`追加 + 設定UI
3. `MySqlBackend`追加
