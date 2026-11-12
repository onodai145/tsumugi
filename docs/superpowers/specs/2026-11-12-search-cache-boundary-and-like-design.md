# ノートキャッシュ検索のページ境界と部分一致の設計 (Issue #583)

## 背景

`NoteCacheStore::search_cache`(SQLite / PostgreSQL / MySQL の3 backend)に、次の2つの不揃いがある。MCP サーバー(#557)の最終レビューで見つかったが、既存の `from cache` カラム検索と共有している挙動なので、ここで扱う。どちらも、見える範囲(スコープ)は広がらない。

1. **ページ境界が並び順と一致しない。** `ORDER BY n.created_at DESC, n.id DESC` で並べるのに、続きの取得は `n.id < ?`(`until_id`)で切る。`created_at` と ID の順序が食い違うノート(連合ノートなど)があると、境界で飛ばされる、または重複する。
2. **部分一致(LIKE)の挙動が backend ごとに違う。** `filter/sql.rs` の `Contains` / `StartsWith` / `EndsWith` は値を `%値%` などにして `LIKE` で比較し、エスケープしない。`%` / `_` がワイルドカードとして働く。大文字小文字は SQLite(ASCII は区別しない)・PostgreSQL(区別する)・MySQL(照合順序による)で異なる。

## 決定事項

| 論点 | 決定 | 理由 |
| --- | --- | --- |
| ページ境界 | `search_cache` を **ID 順**(`ORDER BY n.id DESC`、境界 `n.id < ?`)にする。Issue の `(created_at, id)` 案は採らない。 | バックフィル(#228 / #428)の透かし `backfill_watermark` と `cache_min`、REST の `untilId`、`SearchModal` の「最後の ID」、MCP の `cursor` は、すべて **ID をカーソルとして共有**している。選抜と境界を同じ基準にしないと、`cache_min`(ページの最小 ID)が「これ以上の ID は全部含まれている」という前提を満たさない。#228 の「選抜も境界と同じ id 基準で行う」とも一致する。 |
| 部分一致の大文字小文字 | **区別する**。 | ライブ受信のインメモリ評価(`filter/eval.rs`、Rust の `contains` / `starts_with` / `ends_with`)と PostgreSQL の現状に一致する。eval 側は変更しない。 |
| SQL 生成 | **方言ごとに分ける**(SQLite / PostgreSQL / MySQL)。 | `LIKE` のエスケープ文字の扱い(MySQL はバックスラッシュが既定でエスケープ文字)と大文字小文字を、1つの SQL 文字列では揃えられない。文字列置換(PostgreSQL の `to_postgres_sql`)を増やす方式は、生成側の変更で黙って壊れる。 |

## 目標 / 非目標

目標:

- 同じ検索語・同じカーソルで、3 backend が同じ結果を返す。
- 部分一致で `%` / `_` が実文字として扱われる。
- ID と `created_at` の順序が食い違うノートが、ページ境界で飛ばされず、重複もしない。

非目標:

- `match`(正規表現)の backend 差。#598 で扱う。
- `column_note` 経由の `load_cached` / `load_cached_before`。前者は `cn.created_at` 順、後者は `cn.note_id` 順で選抜と境界が一貫している(#228)。今回は触らない。
- ID の照合順序への依存(MySQL / PostgreSQL の既定照合順序と SQLite のバイト比較の一致)。Misskey の ID(小文字英数字)では一致する前提で、既存コメント(`postgres_backend.rs` の `load_cached_before`)と同じ扱いを続ける。
- eval(インメモリ評価)の挙動の変更。

## 設計

### 1. SQL の方言ごとの生成(`filter/sql.rs`)

`build_where(expr, ctx)` のシグネチャは**変えない**。呼び出し側(`commands/column.rs` ×2、`mcp/query.rs`)は backend を知らない(backend は実行時に `swap_backend` で入れ替わる)ので、組み立ては方言なしの呼び出しのまま、**3方言分をまとめて生成**する。

```rust
pub enum Dialect { Sqlite, Postgres, MySql }

pub struct Rendered { pub sql: String, pub params: Vec<SqlParam> }

pub struct SqlWhere { /* 3方言分の Rendered を内部に持つ */ }

impl SqlWhere {
    pub fn always_true() -> Self;                                  // 旧 `SqlWhere { sql: "1=1", params: [] }`
    pub fn and_raw(self, sql: &str, params: Vec<SqlParam>) -> Self; // 旧 mcp の with_scope の連結
    pub fn render(&self, dialect: Dialect) -> &Rendered;
}
```

- `Builder` が `dialect` を持ち、方言依存の部分(プレースホルダ、文字列マッチ、正規表現)だけを分岐する。`build_where` は3方言ぶん `Builder` を走らせる(コストは無視できる)。エラーは方言によらず同じなので、最初の方言のエラーを返す。
- **プレースホルダ**: SQLite / MySQL は `?`。PostgreSQL は生成時に `$n`(その時点の `params.len() + 1`)を出す。PostgreSQL の `to_postgres_sql`(`?` → `$n` と ` REGEXP ` → ` ~ ` の文字列置換)は廃止する。
- `and_raw(sql, params)` の `sql` は、3方言で同じ意味になる SQL を `?` プレースホルダで書く(MCP のスコープ条件がこれに当たる)。PostgreSQL 向けには `?` を `$n` に振り直してから連結する。
- **`REGEXP`**: 方言ごとの出力は現状と同じ(SQLite / MySQL は `col REGEXP ?`、PostgreSQL は `col ~ ?`)。意味の差は #598。

### 2. 文字列マッチの生成(大文字小文字を区別する)

`Contains`(文字列)/ `StartsWith` / `EndsWith` は、値が文字列リテラルのときだけ受け付ける(現状どおり。それ以外は `LIKE operand must be a string literal` 相当のエラー)。`LIKE` も `ESCAPE` も使わず、方言ごとの関数で書く。検索語は**エスケープせず**、そのままバインドする。

| 種類 | SQLite | PostgreSQL | MySQL |
| --- | --- | --- | --- |
| `Contains` | `instr(col, ?) > 0` | `strpos(col, ?) > 0` | `LOCATE(BINARY ?, col) > 0` |
| `StartsWith` | `substr(col, 1, length(?)) = ?` | `left(col, length(?)) = ?` | `LEFT(col, CHAR_LENGTH(?)) = BINARY ?` |
| `EndsWith` | `substr(col, length(col) - length(?) + 1) = ?` | `right(col, length(?)) = ?` | `RIGHT(col, CHAR_LENGTH(?)) = BINARY ?` |

- 上の表は**出発点**で、関数の細部は実 DB テスト(後述)の結果で確定する。表の式が3方言で次のケースに通らなければ、その方言の式を直す。
- SQLite の `substr(X, -N)` は `N = 0` で全体を返してしまうため、`EndsWith` は `-length(?)` ではなく `length(col) - length(?) + 1` で開始位置を計算する。
- 検索語は式の中で2回現れる場合があるので、同じ値を2回バインドする(`params` に2つ積む)。
- 列が NULL のときは、従来の `LIKE` と同じく偽(NULL)のまま。`text` は従来どおり `COALESCE(n.text, '')` を通る。

### 3. ページ境界(各 backend の `search_cache`)

- `search_cache(where_sql, until_id, limit)` のシグネチャは**変えない**。
- 各 backend の SQL を `ORDER BY n.id DESC LIMIT ?`、境界を `AND n.id < ?` にする。`id` は主キーで一意なので、`created_at` の副キーは要らない(主キーの索引で並べられる)。
- `where_sql` は `render(Dialect::X)` の結果を使う。PostgreSQL は、`where_sql` のバインド数の続きから `until_id` と `limit` に `$n` を振る(現状と同じ)。
- 返す順序は **ID 降順**になる。これにより、返したページの最後のノートが常にそのページの最小 ID になり、`until_id` に渡すカーソルとして一貫する。

### 4. 呼び出し側

- **`commands/column.rs` の `search_cache_core`**: `created_at` 降順への並べ替えをやめ、`search_cache` の返す **ID 降順のまま**返す(`truncate(limit)` は残す)。`SearchModal` は「表示中の最後のノートの ID」を次の `until_id` にするため、ページの最後が最小 ID でないと取りこぼしや重複が起きる。ミュート・eval の再検証で行が落ちても、残った最後の行は次ページの `n.id < ?` で正しく続く(落ちた行は次ページで再び落ちるだけ)。
- **バックフィル(`commands/column.rs` の `cache_min`)**: 変更なし。`cache_min`(件数が `INITIAL_LIMIT` に達したときのページの最小 ID)が、ID 順の選抜と一致して意味を持つようになる。最終的な表示順は、既存どおり全パスの和集合を `created_at` 降順に並べ直す。
- **MCP(`mcp/service.rs`)**: `next_cursor = notes.last().id` は、ID 降順になるので最小 ID を指し、正しいカーソルになる。返すノートの順序は ID 降順(Misskey の ID は時刻順なので、ほぼ新しい順)。`mcp/tools.rs` の説明文の `newest first` は、実態に合わせて「ID の新しい順」へ直す。
- **スコープ**: MCP の `cursor` は任意の ID を受け取るが、使うのは並び順の位置だけで、スコープ条件は従来どおり AND される。見える範囲は広がらない。

### 5. ドキュメント・コメントの更新

- `docs/design/filter-dsl-design.md` §10 の表(`startswith` / `endswith` / `contains` の SQL 射影)と §11 の LIKE のエスケープの記述を、方言ごとの生成に合わせて直す。
- `filter/sql.rs` 冒頭の「LIKE のワイルドカードエスケープは未対応」の注記を削除し、方言の生成を説明する。
- `store/note_cache.rs` の `search_cache` のドキュメント(「until_id は作成順の境界」)を、ID 順の境界に直す。
- `mcp/query.rs` 冒頭の「`Contains` は LIKE の値をエスケープしない」という注記を直す。
- `CLAUDE.md` の `filter/` の項に、「`build_where` は方言ごとの SQL を同時に作り、backend が `render` で選ぶ」旨を1文足す。

## テスト

実 DB を使う検証を中心にする。PostgreSQL / MySQL は `#[ignore]`(CLAUDE.md の手順どおり、`cargo test --lib postgres_ -- --ignored` と `mysql_ ... --test-threads=2`)。

**文字列マッチ**(3 backend で同じケースを同じ期待値で):

- `%` / `_` / `\` / `!` を含む検索語が、実文字として一致する(ワイルドカードにならない)。
- 大文字小文字を区別する(`ABC` の検索語は `abc` に一致しない)。ASCII と非 ASCII の両方。
- 空の検索語は、`Contains` / `StartsWith` / `EndsWith` のすべてで真(Rust の `""` 判定と一致)。
- 検索語が列より長いとき、`StartsWith` / `EndsWith` は偽。
- 多バイト文字(日本語、絵文字)を含む列と検索語。
- 列が NULL(`cw` など)のとき偽。
- 同じケースを、インメモリ評価(`filter/eval.rs`)にも流し、SQL とインメモリが一致することを確かめる。

**ページ境界**(3 backend):

- `created_at` と ID の順序が逆転したノートを含む集合を、`limit` を小さくしてページ送りし、全ノートが**重複も欠落もなく**1回ずつ現れる。
- `until_id` が指すノートがキャッシュに無くても(`n.id < ?` だけで)動く。
- バックフィルの `cache_min` に相当するケース(件数が `limit` に達したとき、最小 ID より下のページが続く)。

**単体**(`filter/sql.rs`):

- 3方言の出力 SQL とバインド値の並び(PostgreSQL の `$n` 番号、検索語の2回バインド)。
- `and_raw` で連結したとき、PostgreSQL の `$n` が正しく続く。

**既存テストの追従**:

- `filter/sql.rs` のテストは `w.sql` の直接比較から `render(Dialect::Sqlite)` 経由に書き換える。
- 各 backend の `search_cache_*` テストと `mcp/query.rs` のテスト(`%` / `_` がワイルドカードでもスコープが外れないことを固定していたもの)を、実文字として一致する期待に直す。スコープ条件が外れないことの確認は残す。
- `commands/column.rs` の `search_cache_core_filters_by_tql_predicate_and_orders_desc` などを、ID 降順の期待へ直す。

## リスクと未確認事項

- **関数の挙動は未確認**: 上の表の関数が、空文字列・NULL・多バイト文字・末尾空白で3 backend とも期待どおり動くかは、まだ実 DB で確かめていない。MySQL の `LOCATE(BINARY ?, col)` と `= BINARY ?` は、バイト比較になる。`Contains` は位置が正かだけを見るので問題ないが、末尾空白と PAD SPACE の扱いはテストで固定する。
- **ID 順への変更は、`LIMIT` が選ぶ範囲を変える**: `created_at` と ID が食い違うノートが、1ページ目に入るかどうかが変わる。Misskey の ID は時刻順で、ほとんどのノートでは一致する。食い違うノートは、新しい ID を持つ側が先に来る。
- **PostgreSQL の `left` / `right` / `strpos`** は十分古い関数で、CI の PostgreSQL イメージで動く想定だが、実 DB テストで確認する。
- **SQLite の `REGEXP` 関数が未登録の疑い**(#598)がある。本 spec の描画層は、`REGEXP` の出力を現状と同じに保つだけで、登録の有無には触れない。

## 実装の順序(plan で詳細化)

1. `Dialect` / `Rendered` / `SqlWhere` と、方言ごとの生成を `filter/sql.rs` に入れる(単体テスト付き)。
2. 3 backend の `search_cache` を `render` + ID 順に切り替え、`to_postgres_sql` を削除する。
3. `search_cache_core`、MCP(`with_scope`)、手組みの `SqlWhere` を新 API に追従させる。
4. 実 DB テスト(3 backend)と、ドキュメント・コメントの更新。
