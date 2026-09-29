# キャッシュDBのインスタンス情報を `instance` テーブルへ正規化する(Issue #409)

## 目的

キャッシュDBの `user.instance_name` / `instance_icon_url` / `instance_theme_color` は、同一インスタンスのユーザー全員に複製されている。これを `host` をキーにした `instance` テーブルへ切り出す。

- 同一インスタンスの重複コピーをなくす。
- 1人分の受信で、そのインスタンスの全ユーザーの表示が更新される(#406 のような取得元変更が即時に反映される)。
- PostgreSQL/MySQL に「一度だけ実行」のマーカー機構が無くても、安全に既存データを移行できる。

対象は SQLite / PostgreSQL / MySQL の全キャッシュバックエンド。優先度は低く、表示の正しさは #406 で担保済み。

## 非目標

- `domain::InstanceInfo` / `domain::User` の形、TQL(`filter/`)、フロントエンドは変更しない(いずれも `instance_*` 列を直接参照していない)。
- ローカルユーザー(`host = NULL`)の instance は従来どおり保存しない(`api/normalize.rs` が常に `None` にし、フロントが `account.instance` で補う)。`instance` テーブルにはリモートホストのみ入る。

## スキーマ

各バックエンドの新規スキーマに `instance` を追加し、`user` の `CREATE` から `instance_*` 3列を外す。

| | `instance` テーブル |
|---|---|
| SQLite | `host TEXT PRIMARY KEY, name TEXT, icon_url TEXT, theme_color TEXT` |
| PostgreSQL | 同上(sea-query の `InstanceTable`、`if_not_exists`) |
| MySQL | `host VARCHAR(255) PRIMARY KEY, name TEXT, icon_url TEXT, theme_color TEXT`(TEXT には PK を張れないため。ホスト名の上限は253文字) |

## 書き込み

`user.instance` が `Some` かつ `user.host` が `Some` のときだけ `instance` を upsert し、その後に `user` を upsert する。FK は張らないので順序による失敗は無く、PostgreSQL/MySQL では両者を1トランザクションにする必要もない(SQLite は `upsert_note` の既存トランザクション内で実行される)。

- **`upsert_user`(ライブ経路)**: 列ごとに `COALESCE(新, 既存)`。null の再取得失敗で既知の値を消さない現行規約を維持する。単位がユーザーからホストに変わるため、誰か1人の受信でそのホスト全員の表示が更新される。
- **`fill_user_from_snapshot`(自己修復経路)**: 列ごとに `COALESCE(既存, 新)`(既存値が無いときだけ埋める)。古いノートのスナップショットが、#406 で直した faviconUrl を古い iconUrl で上書きしないため。

`user` 側の UPSERT 文からは `instance_*` の3列と対応する bind を除く。

## 読み出し

`fetch_users_by_ids` を `user u LEFT JOIN instance i ON i.host = u.host` に変更し、`i.name` / `i.icon_url` / `i.theme_color` から `InstanceInfo` を組み立てる。`Some` にする条件(いずれかの列が非null)は現行のまま。

## 移行(一度きり)

旧 `user.instance_name` 列が**まだ存在する**ことを「未移行」のマーカーとして使う(マーカーテーブルは作らない)。存在するときだけ、次を順に行う。

1. `instance` テーブルを作る(`IF NOT EXISTS`)。
2. 旧列から `instance` へ、競合時は何もしない形でコピーする。
   `INSERT ... SELECT host, MAX(instance_name), MAX(instance_icon_url), MAX(instance_theme_color) FROM user WHERE host IS NOT NULL AND (いずれかの列が非null) GROUP BY host`
   - SQLite/PostgreSQL: `ON CONFLICT (host) DO NOTHING`
   - MySQL: `INSERT IGNORE` 相当(`ON DUPLICATE KEY UPDATE host = host`)
3. `user` から3列を `DROP COLUMN` する。

性質:

- 競合時に何もしないため、途中でクラッシュして再実行しても、その間に再受信した新しい値を潰さない。MySQL は DDL が暗黙コミットされ 2→3 を原子的にできないので、この性質が必須。
- PostgreSQL は 1〜3 をトランザクションで包む。SQLite は既存の `migrate_cache` の流れに乗せる。
- 同一ホストで旧行の値が食い違う場合は、列ごとの `MAX` で1つに決める(値の組が別行由来になり得るが、次回の受信で自己修復される)。
- 列の有無の判定は、SQLite は既存の `column_exists`、PostgreSQL は `information_schema.columns`、MySQL は `information_schema.columns`(`DATABASE()` 限定)で行う。
- 旧バージョンの tsumugi が同じ PostgreSQL/MySQL DB に接続すると `instance_*` 列が無くて壊れる(ダウングレード不可)。これは許容済み(2026-09-29 合意)。

### SQLite の既存マイグレーションの修正(必須)

`db.rs::migrate_cache` の Issue #263 ブロックは `!column_exists(user, "instance_name")` を条件に `avatar_url` / `bio` / `banner_url` / `emojis` と `instance_*` をまとめて追加している。DROP 後に再起動すると条件が真になり、全列を再追加してしまう。

- 条件を非 `instance_*` 列(`emojis`)の有無に付け替える。
- このブロックは `instance_*` を追加しない(追加してすぐ DROP するだけになるため)。
- 新しい移行処理(上記 1〜3)はこのブロックの後に置く。

## テスト

- SQLite(`user_ref.rs` / `db.rs` / `sqlite_backend.rs` の既存テストを更新):
  - 同一ホストの2ユーザーで、片方の `upsert_user` がもう片方の読み出し結果に反映される。
  - `instance: None` の再受信で既知の値が消えない。
  - `fill_user_from_snapshot` が既存値を上書きしない。
  - `host = NULL`(ローカル)ユーザーは `instance` 行を作らず `instance: None` で返る。
  - 旧スキーマ(`instance_*` あり)から移行すると、`instance` に値が入り、旧列が消え、再実行しても冪等で、移行後に更新した値が再実行で戻らない。
  - `migrate_cache_adds_user_normalization_columns` は `instance_*` を期待しない形に直す。
- PostgreSQL/MySQL: 既存の実DBテスト(`ensure_schema_is_idempotent_and_creates_tables` 等、`#[ignore]` 相当の流儀)に合わせ、`ensure_schema` の冪等性、旧スキーマからの移行、`*_user_ref` の upsert/読み出しを追加する。

## 実装時に確認するリスク

- rusqlite 0.40.1(bundled、libsqlite3-sys 0.38.1)の SQLite が `DROP COLUMN`(3.35+)に対応しているか。SQLite の移行テストが通ることで確認する。
- MySQL で `user.host`(TEXT)と `instance.host`(VARCHAR)を JOIN するときの照合順序の不一致(`Illegal mix of collations`)と、`MAX(text)` を使う移行 SQL の挙動。実DBテストで確認する。
- `sea_query` の `Table::create()` は既存テーブルに列を追加も削除もしない。既存インストールの旧列は移行処理の `DROP COLUMN` だけが消す。

## ドキュメント

`docs/design/misskey-multicolumn-client-design.md` などにキャッシュDBの `user` スキーマの記述があれば、実装時に `instance` テーブルの追加へ合わせて更新する(設計書と食い違う場合は設計書が正)。
