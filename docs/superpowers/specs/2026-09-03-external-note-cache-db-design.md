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

Phase 1で確定した実装: `store/note_cache.rs`の`NoteCacheStore`は`NoteCacheBackend`トレイトオブジェクトへ委譲する薄いラッパー。Phase 2では、設定画面からの切り替えで再起動なしに即時再接続できるよう、内部を`RwLock`で保持する:

```rust
pub struct NoteCacheStore {
    backend: RwLock<Box<dyn NoteCacheBackend>>,
}
```

既存の委譲メソッド(`cache_notes`/`load_cached`/...)は内部で`self.backend.read().await`してから委譲するだけなので、**呼び出し元(`state.cache.method(...)`)は一切変更不要**。バックエンド切替は新規メソッド`NoteCacheStore::swap_backend(&self, new_backend: impl NoteCacheBackend + 'static) -> Result<()>`が`write().await`して差し替える。起動時、および設定変更時に`SettingsData.cache_backend`を読み、対応する`NoteCacheBackend`実装(`SqliteBackend`/`PostgresBackend`/将来の`MySqlBackend`)を構築して渡す。

### DBアクセス手段: Phase 1はrusqlite継続 + spawn_blocking、sqlx/sea-queryはPhase 2から

**方針転換(重要)**: 当初`sqlx`をPhase 1から導入する設計だったが、実装検証の結果、`rusqlite`(bundled機能、`libsqlite3-sys ^0.38.1`)と`sqlx`のSQLiteドライバ(`sqlx-sqlite`、`libsqlite3-sys >=0.30.1, <0.38.0`)は要求するネイティブライブラリバージョン範囲が重ならず、**同一Cargo依存グラフに共存できない**ことが判明した(`links = "sqlite3"`をどちらも宣言するため、Cargoのlinksルールにより1つのバイナリに2つの`libsqlite3-sys`を含められない。`cargo add sqlx --no-default-features --features sqlite,runtime-tokio`で実際に確認済み。最新の`sqlx 0.9.0`でも`libsqlite3-sys <0.38.0`までしか対応しておらず、`rust-version`を引き上げても解消しない)。

account/column設定側(`store/settings.rs`ほか)は今後も`rusqlite`を使い続ける前提のため、note cache側だけ別バージョンのSQLiteドライバに切り替えることはできない。したがって:

- **Phase 1**: `sqlx`を導入せず、既存の`rusqlite::Connection`(`Arc<Mutex<Connection>>`)をそのまま使う。トレイト(`NoteCacheBackend`)のメソッドを`async fn`にする手段は、`tauri::async_runtime::spawn_blocking`(このコードベースで既に`commands/note.rs`・`commands/mute.rs`が使っている確立されたパターン)で同期のrusqlite呼び出しをブロッキングタスクへ包む方式にする。SQL文字列・クエリロジック自体は現状のものをそのまま`spawn_blocking`クロージャの中へ移すだけで、書き換えない
- **Phase 2(PostgreSQL対応)**: ここで初めて`sqlx`(features: `postgres`, `runtime-tokio`)を導入する。`sqlx`のPostgresドライバ(`sqlx-postgres`)は`libsqlite3-sys`に依存しないため、`rusqlite`との共存問題は起きない。SQL組み立てに`sea-query`(+`sea-query-binder`)を使う方針も維持する
- **Phase 3(MySQL対応)**: 同様に`sqlx`の`mysql`featureを追加(これも`libsqlite3-sys`非依存)

行→`Note`構造体へのマッピング(JOIN結果の集約、`note_reaction`等側テーブルの集約)は現状の手書きスタイルを維持する。フルORM(sea-orm)を不採用とする理由(JOIN+JSON payload組み立てとEntityモデルの相性の悪さ)はPhase 2以降も変わらない。

**sea-query採用理由の再確認(Phase 2着手時)**: 当初sea-query導入の主目的は「SQLite/Postgres/MySQL 3バックエンドでクエリ構築コードを1箇所にまとめる」ことだったが、Phase 1で`SqliteBackend`が`rusqlite`ベースの独立実装になった(`PostgresBackend`とは接続型もクエリ文字列も一切共有しない)ため、この目的は部分的にしか成立しなくなった。それでもPhase 2でsea-queryを採用するのは、Phase 3で追加する`MySqlBackend`と`PostgresBackend`の2つではクエリ構築コードを共有できる見込みがあるため(SQLiteだけが仲間外れになる形)。`SqliteBackend`は今後もsea-queryを使わず、rusqliteの手書きSQLのまま維持する。

### 非同期化(Phase 1: spawn_blocking方式)

`NoteCacheBackend`は`async-trait`によるasync traitとして定義する。既存の呼び出し元(`commands/column.rs`, `commands/mute.rs`, `commands/note.rs`, `stream/connection.rs`)はすべて既に`async fn`内から同期呼び出ししているため、各呼び出しに`.await`を追加する変更で足りる。

各トレイトメソッドの実装は、既存のrusqliteロジックをそのまま`tauri::async_runtime::spawn_blocking(move || { ... })`のクロージャに移す。クロージャは`'static`である必要があるため、借用引数(`&str`/`&[Note]`/`&SqlWhere`等)は呼び出し前に所有値へ変換する(`to_string()`/`to_vec()`/`clone()`)。`std::sync::Mutex`のロックガードはクロージャ内で完結する(`.await`をまたがない)ため、そのまま使ってよい。`spawn_blocking`は`JoinError`を返しうるため、`commands/note.rs::read_clipboard_image`と同じパターンで`.await.map_err(...)？？`のように`crate::error::Error`へマッピングする。

`store/db.rs`の`open_cache`/`open_cache_in_memory`はrusqliteのまま(シグネチャ変更なし)。`lib.rs`/`state.rs`の初期化コードも変更不要(`AppState::new_for_test`は同期のまま)。`note_cache.rs`/`user_ref.rs`内の既存`#[test]`もrusqlite・同期のまま変更不要(トレイト抽出後の新しい`SqliteBackend`に新規で書くテストだけが`#[tokio::test]`になる)。

### filter/sql.rs(TQL `cache`ソース)の扱い

`search_cache`(`note_cache.rs`)は`filter/sql.rs`の`build_where`が返す`SqlWhere { sql: String, params: Vec<SqlParam> }`(`?`プレースホルダのSQL断片)をそのまま自前のSELECT文へ埋め込んでいる。Phase 1はrusqliteを使い続けるため、この生SQL文字列・バインド方式は一切変更しない(`?`プレースホルダはrusqliteネイティブの記法のまま)。`SqlWhere`はDeriveされた`Clone`を持たないため、`spawn_blocking`クロージャへ渡す際は`SqlWhere { sql: where_sql.sql.clone(), params: where_sql.params.clone() }`のようにフィールドごとに複製する(`SqlParam`は`Clone`実装済み)。

PostgreSQL対応で`$1`形式の番号付きプレースホルダへの対応が必要になる時点(Phase 2)で、`filter/sql.rs`の`build_where`をバックエンド非依存な形へ変更する。

### 依存クレート追加(Phase 1)

- `async-trait`のみ。`sqlx`/`sea-query`は追加しない(Phase 2から)
- 既存の`rusqlite`はnote cache側・設定側とも引き続き使用する(変更なし)

## 複数端末の同時書き込みに関する整合性

- `note`/`user`テーブルはそれぞれ`id`が主キーのため、UPSERTは常に対象IDの1行に対する原子的な書き込みになる。複数端末が同じノートを同時にキャッシュしても、後勝ちで上書きされるだけで重複行や壊れた行は生まれない
- 一方`note_reaction`/`note_tag`/`note_mention`/`note_emoji`/`note_file`テーブルは主キー・UNIQUE制約を持たず、現行コードは「対象noteの既存行を全DELETE→INSERTし直す」という複数文パターンで書き換えている。単一プロセス+単一SQLite接続(Mutexで直列化)では安全だが、外部DBに複数端末が同時に同じノートを書き込むと、2つのトランザクションのDELETE/INSERTが交互に割り込む余地があり、一時的な重複行や(読み取りタイミングによっては)瞬間的な空状態が起こり得る
- 対策として、これら側テーブルに以下のUNIQUE制約を追加し、DELETE+INSERTパターンをUPSERT(SQLiteの`ON CONFLICT DO UPDATE` / `DO NOTHING`)に置き換える。Phase 1時点では単一プロセス+`Mutex<Connection>`直列化のままなので実害は無いが、単一文で完結させておくことで冪等性を高め、Phase 2(外部DB・複数端末同時書き込み)でも同じテーブル設計を使い回せるようにする:
  - `note_reaction`: `UNIQUE(note_id, emoji_key)`
  - `note_tag`: `UNIQUE(note_id, tag)`
  - `note_mention`: `UNIQUE(note_id, user_id)`
  - `note_emoji`: `UNIQUE(note_id, emoji)`
  - `note_file`: `UNIQUE(note_id, mime_type, mime_category, is_sensitive)` — `DriveFile`にはid列を持たせておらず、これは自然キーではなく便宜上の重複排除キーである。理論上、同一mime種別・同一sensitiveフラグの添付ファイルが2つ以上あるノートでは行が縮退しうるが、`filter/sql.rs`でのこのテーブルの参照は`EXISTS`/相関サブクエリのみ(`COUNT`は使わない、Reactions/Tags/Mentions/Emojisも同様)であり、表示用の完全なファイル一覧は`note.payload`のJSONが真実の情報源のままなので、フィルタ述語の結果には影響しない
- このUNIQUE制約追加+UPSERT化は、実装の段階分割の1番目(`NoteCacheBackend` trait抽出 + `SqliteBackend`移植)に含める

### 既存キャッシュDBへのUNIQUE制約追加について

`CREATE UNIQUE INDEX`は既存データに重複行があると失敗し、起動時マイグレーションの失敗はアプリ起動不能に直結する。一方でキャッシュには蓄積された既存ノートデータ(再取得コストがかかる)が入っており、`note`/`user`/`column_note`/`column_fetch_boundary`を含めて丸ごと作り直すのは損失が大きい。

現行コードは単一プロセス+単一SQLite接続(`Mutex`で直列化)を前提にしており、`upsert_note`のDELETE→INSERTは常に1トランザクション内で完結しているため、**既存ユーザーのキャッシュDBには側テーブル(`note_reaction`等)の重複行はほぼ存在しない**はずである。UNIQUE制約は「今後、外部DBに複数端末が同時書き込みしたときの保険」であり、既存データを壊すものではない。

そのため、`db.rs`の`migrate_cache`に以下のマイグレーションステップを追加する形にする(既存の`migrate_cache`と同じ、列/インデックス追加パターンを踏襲):

1. 各側テーブル(`note_reaction`/`note_tag`/`note_mention`/`note_emoji`/`note_file`)に対し、UNIQUEインデックスがまだ無ければ、SQLiteの暗黙`rowid`を使って重複行を削除する(例: `note_reaction`なら`DELETE FROM note_reaction WHERE rowid NOT IN (SELECT MIN(rowid) FROM note_reaction GROUP BY note_id, emoji_key)`)。実運用では対象行が0件のケースがほとんどなので、コストは無視できる
2. その後`CREATE UNIQUE INDEX IF NOT EXISTS`で制約を追加する

`note`/`user`/`column_note`/`column_fetch_boundary`はこのマイグレーションで一切削除・作り直しをしない。DBファイル全体の削除・スキーマバージョンによる作り直しは行わない。

## `delete_matching`について(Phase 1では変更不要)

現行の`delete_matching`(prune処理)は`CREATE TEMP TABLE prune_ids AS SELECT ...`で一時テーブルを作り、複数の`DELETE ... WHERE note_id IN (SELECT id FROM prune_ids)`文と`DROP TABLE`をまたいで参照している。これは同一コネクション内で完結する一時テーブルなので、Phase 1(単一`rusqlite::Connection`を`Arc<Mutex<_>>`で共有)では問題なく動作し、**変更しない**。

この方式が問題になるのはPhase 2で`sqlx`の接続プール(既定で複数コネクション)を使うようになったときで、文が別々のコネクションに割り当てられうるためTEMP TABLEが「無い」扱いになりうる。Phase 2でPostgres対応する際に、対象ノートIDをRust側の`Vec<String>`として先に確定させ`IN (...)`リストとしてバインドする方式へ書き換える(TEMP TABLE構文の方言差も同時に解消できる)。

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

- 現行の`note_cache.rs`内テスト群と同じ検証内容を、トレイト抽出後の`SqliteBackend`(新規ファイル)向けに書く。Phase 1はrusqliteのままなので既存テストの大半はほぼそのまま踏襲でき、`SqliteBackend`の非同期メソッドを呼ぶ新しいテストだけ`#[tokio::test]`にする
- Postgres/MySQLは`testcontainers`等でDocker上に一時インスタンスを立てる統合テストを追加する(Phase 2/3)。既存の実Misskey接続テスト(`#[ignore]`)と同様、CI常時実行はしない方針で揃える

## Phase 2設計: PostgresBackend + 設定UI

Phase 1完了後の続き。以下のヒアリングで確定した内容:

- **バックエンド切替の挙動**: 設定画面で切り替えた瞬間に即時再接続する(アプリ再起動は不要)。上記「バックエンド抽象化」の`NoteCacheStore::swap_backend`で実現する
- **接続失敗時**: Phase 1の設計(エラー表示+SQLiteへ自動フォールバック)をそのまま踏襲する
- **統合テスト**: `testcontainers`でDocker上に一時PostgreSQLインスタンスを立てて検証する。既存の実Misskey接続テスト(`#[ignore]`)と同様、CI常時実行はしない
- **DDL**: `PostgresBackend`用のテーブルDDLもsea-queryの`Table::create()`で書く。SQLite版とは別定義になる(型の対応: `TEXT`→`TEXT`、`INTEGER`(64bit用途)→`BIGINT`、`INTEGER`(真偽値用途)→`BOOLEAN`等、実装時に列ごとに精査する)

### 実装の段階分割(1設計・複数PR)

1. `NoteCacheBackend` trait抽出 + `SqliteBackend`への挙動そのまま移植(rusqlite継続、`spawn_blocking`による非同期化のみ、外部から見た挙動は不変)【Phase 1・完了】
2. Phase 2: `sqlx`+`sea-query`導入 + `PostgresBackend`のDB層一式(接続・DDL・CRUD) + `NoteCacheStore`の`RwLock`化 + 設定UI(フロントエンド) + `swap_backend`の配線 + 接続失敗時フォールバックを1つのPRでまとめて実装する(ヒアリングで確認済み: DB層と設定UIを分けず一括で作る方針)
3. `MySqlBackend`追加(Phase 3)
