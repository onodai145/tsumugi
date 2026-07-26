# SQLiteチューニング設計 (Issue #114)

## 背景・目的

Issue #114「SQLiteのチューニング」。読み取り性能、特に起動時にカラムごとの直近ノートを
復元する `NoteCacheStore::load_cached`（`src-tauri/src/store/note_cache.rs`）の高速化を狙う。

現状のクエリ:

```sql
SELECT n.payload FROM note n
JOIN column_note cn ON cn.note_id = n.id
WHERE cn.column_id = ?1
ORDER BY n.created_at DESC, n.id DESC
LIMIT ?2
```

`column_id` の絞り込みは `idx_cn_column` で高速だが、`ORDER BY` のキー（`created_at`）が
`note` 側にしかないため、SQLiteは対象カラムに紐づく全行を `note` と結合してから並べ替える
必要があり、`LIMIT` があっても事前に絞り込めない。カラムに数千件キャッシュされていると、
直近数十件を表示するだけでも該当カラム分の全JOIN＋ソートが走ってしまう。

なお `SettingsStore`（Account/Column設定）は現在JSONファイル永続化であり、
`store/db.rs` の `open_settings`（SQLite）はレガシーDBからの1回限りの移行読み込みにしか
使われていない。そのため本チューニングは `open_cache`（cache.db、ノートキャッシュ）のみを
対象とする。

## 変更1: column_note へのソートキー非正規化 + カバリングインデックス

`column_note` に `created_at`（note.created_at の複製）を追加し、
`(column_id, created_at DESC, note_id DESC)` の複合インデックスを張る。
これにより `load_cached` はインデックスから直接「そのカラムの新しい順」に `LIMIT` 件だけ
取り出せるようになり、`note` テーブルへのpayload取得も `LIMIT` 件分の点検索で済む。

### スキーマ変更（`src-tauri/src/store/db.rs`）

`CACHE_SCHEMA` の `column_note` 定義:

```sql
CREATE TABLE IF NOT EXISTS column_note (
    column_id   TEXT NOT NULL,
    note_id     TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    created_at  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (column_id, note_id)
);
CREATE INDEX IF NOT EXISTS idx_cn_column ON column_note(column_id);
```

既存の `idx_cn_column` は `clear_column_notes` 等の等値検索で使われ続けるため残す。

`idx_cn_column_created` は `CACHE_SCHEMA` には含めない。既存DBに対して `CACHE_SCHEMA` の
`execute_batch` が走る時点ではまだ `created_at` 列が存在しないため、この位置に置くと
`CREATE INDEX` がエラーになる。列追加後の `migrate_cache`（後述）側で作成する。

### マイグレーション

`CACHE_SCHEMA` は現状 `migrate()` を持たない（キャッシュは破棄しても再取得で復元できるため）。
今回は非破壊的な列追加なので、既存の `migrate()`（設定DB用）と同様のパターンを
`open_cache` 内に追加する:

- `column_exists(&conn, "column_note", "created_at")` が false なら:
  1. `ALTER TABLE column_note ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0`
  2. `UPDATE column_note SET created_at = (SELECT created_at FROM note WHERE note.id = column_note.note_id)`
     で `note` から逆算してバックフィル
  3. インデックス作成（`CREATE INDEX IF NOT EXISTS` なので新規DBでも既存DBでも安全に実行できる）

### 書き込み側の変更（`src-tauri/src/store/note_cache.rs`）

`cache_notes` の INSERT を拡張:

```sql
INSERT OR IGNORE INTO column_note (column_id, note_id, received_at, created_at)
VALUES (?1, ?2, ?3, ?4)
```

`INSERT OR IGNORE` なので既存行がある場合は上書きされない。ノートの `created_at` は
一度作成されたら変わらない値なので問題ない。

### 読み取り側の変更

`load_cached` のクエリを `column_note` 起点に変更し、ソートをインデックスに委ねる:

```sql
SELECT n.payload FROM column_note cn
JOIN note n ON n.id = cn.note_id
WHERE cn.column_id = ?1
ORDER BY cn.created_at DESC, cn.note_id DESC
LIMIT ?2
```

## 変更2: PRAGMAチューニング（cache.db のみ）

`open_cache`（`src-tauri/src/store/db.rs`）に以下を追加。単一接続を `Mutex` で直列化して
使う構成（`NoteCacheStore`）なので、値は控えめに設定する。

| PRAGMA | 値 | 目的 |
|---|---|---|
| `synchronous` | `NORMAL` | WAL下では`FULL`ほどの同期コストは不要。書き込み時のfsync待ちを削減 |
| `temp_store` | `MEMORY` | `ORDER BY`等の一時ソート領域をディスクではなくメモリに置く |
| `cache_size` | `-20000`（約20MB、デフォルトは-2000=2MB） | ページキャッシュ拡大でnote/column_noteの再読込を削減 |
| `mmap_size` | `67108864`（64MB） | ファイルをmmapし読み取りのシステムコールを削減 |

`open_settings` はレガシー移行専用のため変更しない。

## テスト方針

- 既存の `note_cache.rs` 内テスト（`cache_roundtrip_preserves_note_and_order` 等）は
  クエリ結果の外部仕様（順序・内容）を変えないため無変更でグリーンを維持する。
- `db.rs` に、旧スキーマ（`column_note` に `created_at` 列が無い状態、`note` に行がある状態）
  から `open_cache` 相当のマイグレーションを通した際に `created_at` が `note` の値で
  正しくバックフィルされることを確認するテストを追加する（既存の
  `migrates_old_column_def_to_groups` と同様のパターン）。

## スコープ外

- `open_settings`（レガシー移行専用）のPRAGMAチューニング
- `search_cache`（TQL cacheソース検索）のインデックス最適化
- WALチェックポイント間隔などの追加チューニング
