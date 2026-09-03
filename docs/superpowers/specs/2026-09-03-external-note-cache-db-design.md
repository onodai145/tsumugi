# note cacheの外部DB対応 設計 (Issue #115)

## 背景・動機

Issue #115「外部のデータベースを使えるようにする」。本文は空だったため、ヒアリングにより以下2点を動機として確認した。

1. 複数端末間でnote cacheを共有したい
2. 大規模データにおける性能・運用面で、SQLiteのファイルベース運用よりPostgreSQL等の既存DBサーバ運用に寄せたい

## スコープ

- **対象**: note cache(`store/note_cache.rs`が扱う`note`/`user`/`note_reaction`/`note_tag`/`note_mention`/`note_emoji`/`note_file`/`column_note`/`column_fetch_boundary`)のみ
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

### 依存クレート追加

- `sqlx`(features: `sqlite`, `postgres`, `mysql`, `runtime-tokio`)
- `sea-query`, `sea-query-binder`
- `async-trait`
- 既存の`rusqlite`は設定関連の旧移行コード(`db.rs`の`open_settings`、旧SQLite一体型からの一回限り移行)のために残す。note cache側の`rusqlite`利用は今回撤去する

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
