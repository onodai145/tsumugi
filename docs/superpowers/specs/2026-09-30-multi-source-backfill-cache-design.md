# TQL複数ソースカラムのbackfillキャッシュ優先化 設計 (Issue #238)

Issue #228 / PR #237（`docs/superpowers/specs/2026-08-23-cache-first-backfill-design.md`）の続き。単一ソースカラム限定だった backfill のキャッシュ優先読み出しを、TQLで複数ソースを `from` に列挙したカラムへ拡張する。

## 背景

#228 は「カラム単位のスカラー境界 `column_fetch_boundary.oldest_fetched_id`（これより新しいIDのノートは、そのカラムのソース・フィルタに対してAPI取得済みで完全）」を導入した。対象は `resolved.kinds.len() == 1 && !resolved.use_cache` のカラムのみ。

複数ソースのカラムでは、ソースごとに投稿密度が違うため、同じ `until_id` で全ソースが1ページ（`INITIAL_LIMIT`=20件）取得しても、各ソースの生レスポンスの最古IDはソースごとにバラバラになる。したがって完全性はソースごとに追跡する。

## 方針の経緯

ブレインストーミングで「スカラー境界を流用し、更新値を `min(現境界, 各ソース生最古IDの最大)` にする案（案A）」も検討した。スキーマ変更が要らず小さく済む案だったが、Issue の記載どおり**ソース単位の境界テーブルを新設する案（案B）**を採用した。以降は案B の設計である。

## 設計変更の経緯

当初は対象を `!use_cache && !kinds.is_empty()` としていたが、ブランチ全体の最終レビューで穴が見つかった。境界「`id > E` は `column_note` に完全」が成り立つのは、初回RESTページで取得したノートかストリーミングで届くライブノート(`stream/connection.rs` が `column_note` へ記録)だけである。`User` / `Tag` / `Search` などストリーミングを持たないソース(`stream_request()` が None)にはライブノートが無く、初回取得より新しい分が `column_note` に入らない。そのため `from home, user:@alice` のようなカラムで一覧が初回取得を超えて伸びた後に `fetch_backfill` がキャッシュで応答すると、alice の新着ノートが黙って欠落する。対象を全ソースがストリーミング対応のカラムに絞ることで解消する。

## スコープ

- 対象: `resolve_sources` の結果が `!use_cache && !kinds.is_empty() && kinds.iter().all(|k| k.stream_request().is_some())` のカラム。単一ソースも複数ソースも同じ経路で扱う（単一ソースは N=1 の特殊ケース）。
- 対象外（実装しない）:
  - `from cache` を含むカラム。`search_cache` はグローバルな `note` テーブルを読み、他カラムの backfill で古いIDのノートが増え続ける。そのため `column_note` だけを見るキャッシュ優先経路では、API 経路と同じ結果を再現できない。常にAPI経由のまま。
  - ストリーミングを持たないソース(User / Tag / Search)を含むカラム。これらのソースはライブノートが `column_note` に入らないため、初回取得範囲より新しい側で「`id > E` は完全」が成り立たない。#228 が単一ソースの user/tag/search カラムに対して抱えていた同じ穴もここで塞ぐ(#228 からの挙動変更: これらのカラムは常にAPI経由になる)。
  - `fill_gap` / `gap_fill_on_reconnect`。「新しい方向」のギャップ埋めで、境界の意味と無関係。
  - 既存の別問題: `fillRemainingGap` も同じ `fetch_backfill` 経路を通るため、キャッシュに Hit してギャップマーカーを消すだけで実際にはギャップを埋めないことがある(#228 から存在する)。`fetch_backfill` はギャップ埋め呼び出しとスクロールを区別できないため、別途の修正が必要。本 Issue では直さない。当初の「`fillRemainingGap` は境界と無関係」という記述はこの点で誤りだった。
  - 既存の別問題: フィルタが強い複数ソースで、フロントの `until_id`（表示中の最古ノート）が浅いソースの生最古IDより古くなり、API 経路でその間のノートを読み飛ばす。本 Issue では直さず、別 Issue として起票する。連続性チェックがあるため、キャッシュ優先経路が読み飛ばしを「完全」と誤認することはない。
  - ミュート設定変更後の古いキャッシュ問題（既存挙動を変えない。ミュート変更時は従来どおり `clear_all_fetch_boundaries` で全境界が破棄される）。

## 設計

### 境界の意味

ソース `i`（`from` 節内の位置。`resolved.kinds` のインデックス）について、`b_i` は「ID が `b_i` より大きい（新しい）ノートは、ソース `i` からAPI取得済みで、フィルタ通過分は `column_note` に記録済み」を表す。ID比較は既存と同じ辞書順。

- `b_i = ""`（空文字）は「ソース `i` はタイムラインの先頭まで枯渇済み（それより古いノートは存在しない）」を表す。空文字は任意のIDより小さいため、`MIN`/`LEAST` による延長、prune の引き上げ、`>=` 比較がそのまま成立する。
- ソース `i` の行が存在しない = 未確定。
- カラム全体で完全な範囲は `id > E`、`E = max_i(b_i)`。ただし全ソースの行が存在する場合に限る。

ソース識別子に位置インデックスを使う理由: TQL 文字列の変更は `update_column` → `clear_column_notes` で境界ごとリセットされるので、並べ替えによるズレは起こらない。旧単一ソース境界は `source_idx=0` として機械的に移行できる。

### データモデル

```sql
CREATE TABLE IF NOT EXISTS column_source_boundary (
    column_id         TEXT NOT NULL,
    source_idx        INTEGER NOT NULL,
    oldest_fetched_id TEXT NOT NULL,
    PRIMARY KEY (column_id, source_idx)
);
```

3バックエンドすべてに追加する（SQLite: `db.rs` の `CACHE_SCHEMA`、Postgres/MySQL: 各 `backend` のスキーマ初期化。カラム型は既存 `column_fetch_boundary` の定義に合わせる。MySQL は `column_id` を `VARCHAR(64)`、`source_idx` を `INT`）。

旧 `column_fetch_boundary` は廃止する。マイグレーションは CLAUDE.md の規則（マーカーテーブルを作らず「旧構造がまだ存在する」を未移行マーカーにする）に従い、冪等にする:

1. 旧テーブルが存在するか確認（SQLite: `sqlite_master`、Postgres: `to_regclass`、MySQL: `information_schema.tables`）。
2. 存在すれば、旧行を `source_idx = 0` で新テーブルへコピー（Postgres/SQLite: `ON CONFLICT DO NOTHING`、MySQL: `INSERT IGNORE`）。
3. 旧テーブルを `DROP TABLE`。MySQL の DDL は自動コミットのためコピーと削除は原子的にならないが、1→3 の途中で落ちても再実行で完了する。

旧行が単一ソースカラムのものだけであることは #228 の仕様（複数ソースには境界を付けない）で保証されるため、`source_idx = 0` への写像は正確である。

### `NoteCacheBackend` / `NoteCacheStore` の API

単一ソース用の3関数を次のソース単位の関数に置き換える。

```rust
/// カラムの全ソース境界を (source_idx, oldest_fetched_id) で返す。行が無ければ空。
async fn get_fetch_boundaries(&self, column_id: &str) -> Result<Vec<(u32, String)>>;

/// カラムの境界を entries で置き換える(初回取得用)。カラムの既存行を全削除してから挿入する
/// (トランザクション内)。entries に含まれないソースは未確定になる。
async fn replace_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()>;

/// 各 (source_idx, id) について境界を古い方向へのみ延長する。行が無ければ挿入する。
/// 既存値の方が古い場合は何もしない。
async fn extend_fetch_boundaries(&self, column_id: &str, entries: &[(u32, String)]) -> Result<()>;
```

- `clear_column_notes` は `column_source_boundary` のカラム行を全削除する。
- `clear_all_fetch_boundaries` は `column_source_boundary` を全削除する（`commands/mute.rs` の呼び出しは変更なし）。
- `delete_matching`（prune。SQLite の `delete_matching`、Postgres/MySQL の `delete_matching_ids` 系）: 生存最古ID・削除最大IDから候補を求める既存ロジックは変えない。対象テーブルだけ差し替える。`UPDATE column_source_boundary SET oldest_fetched_id = 候補 WHERE column_id = ? AND oldest_fetched_id < 候補` はカラムの全ソース行に効く。`column_note` はソースの帰属を持たず、削除されたノートがどのソース由来かは分からないので、全ソース行を一律に引き上げるのが保守的で正しい。生存ノートが全滅したカラムは、そのカラムの全行を削除する。

### `commands/column.rs`

#### `fetch_and_filter_multi` の戻り値

```rust
enum SourceOutcome {
    Fetched(String), // 生レスポンスの最古ID(id辞書順の最小)
    Exhausted,       // 取得成功だが0件(それより古いノートは無い)
    Failed,          // 取得失敗、または rest_request が None
}

struct FilteredFetch {
    notes: Vec<Note>,               // 画面へ返す分(従来どおり重複除去・ソート・INITIAL_LIMITへtruncate済み)
    cacheable: Vec<Note>,           // truncate前の、重複除去済みのフィルタ通過分(全ソース分)
    source_outcomes: Vec<SourceOutcome>, // resolved.kinds と同じ並び
}
```

`raw_oldest_id` は廃止する。

**truncate前の全件をキャッシュする**（本設計の最重要ポイント）。従来は `notes`（truncate後）だけを `cache_notes` しており、単一ソースでは生レスポンス最大20件 → フィルタ通過は最大20件なので実質無害だった。複数ソースでは最大 20×N 件がフィルタを通り、20件目以降が未キャッシュのまま境界だけが進むと、`column_note` に無いノートを「完全」と誤認して静かに欠落する。呼び出し元は `cache_notes(&column.id, &fetch.cacheable)` を呼び、`fetch.notes` を返す。

`use_cache` を含むカラムは境界の対象外で、従来の挙動を変えない。このため `use_cache` のとき `cacheable` は従来どおり truncate後（`notes` と同じ内容）とする。

#### 境界更新の純関数

ネットワーク・DBなしでテストできるよう、更新内容の決定を純関数に切り出す。

```rust
/// fetch_backfill での延長内容を決める。until_id は今回の取得の until_id。
fn plan_boundary_extend(
    prev: &HashMap<u32, String>,
    until_id: &str,
    outcomes: &[SourceOutcome],
) -> Vec<(u32, String)>;

/// open_stream_and_fetch での初回セット内容を決める(until_id 無し)。
fn plan_boundary_initial(outcomes: &[SourceOutcome]) -> Vec<(u32, String)>;
```

- `plan_boundary_extend`: 各ソース `i` について、`prev[i]` が存在し `until_id >= prev[i]`（連続）のときだけ更新する。`Fetched(o)` なら `(i, o)`、`Exhausted` なら `(i, "")`。`Failed` と連続でないソース（`prev[i]` 無し、または `until_id < prev[i]`）は更新しない。連続性のチェックをソースごとに行うので、1ソースが不連続でも他のソースは延長できる。
- `plan_boundary_initial`: `Fetched(o)` → `(i, o)`、`Exhausted` → `(i, "")`、`Failed` は含めない。

#### `open_stream_and_fetch`

対象条件 `!use_cache && !kinds.is_empty() && kinds.iter().all(|k| k.stream_request().is_some())` のとき、`cache_notes(&fetch.cacheable)` の後に `replace_fetch_boundaries(column.id, plan_boundary_initial(...))` を呼ぶ。

#### `fetch_backfill`

```text
cache_eligible = !use_cache && !kinds.is_empty() && kinds.iter().all(|k| k.stream_request().is_some())
if cache_eligible:
    boundaries = get_fetch_boundaries(column.id)      // 失敗は握りつぶして空扱い
    if boundaries が 0..kinds.len() の全インデックスを含む:
        E = max(boundaries の値)
        if until_id > E:
            cached = load_cached_before(column.id, until_id, INITIAL_LIMIT)
            cached.retain(id >= E)
            cached にフィルタ/ミュートを再適用(既存どおり)
            if cache_backfill_page(Some(E), until_id, cached, INITIAL_LIMIT) が Some → Hit で返す
        Fallback(FallbackOther)
    else:
        Fallback(FallbackBoundaryUnset)

fetch = fetch_and_filter_multi(...)
cache_notes(fetch.cacheable)
if cache_eligible:
    extend_fetch_boundaries(column.id, plan_boundary_extend(&prev, until_id, &fetch.source_outcomes))   // 失敗は握りつぶす
return fetch.notes
```

読み出し側でマージ・重複除去・ソートを再実装する必要はない。`column_note` が既にカラム全体のマージ済み・重複除去済みの集合であり、既存の `cache_backfill_page`（境界より新しい範囲で、件数が `limit` 以上）がそのまま「全ソースの境界がページより古い」かつ「件数が足りる」の二重判定になる。

`state.rs` の `FallbackBoundaryUnset` のコメントは「いずれかのソースの境界が未確定」に更新する。

### エラーハンドリング

- 境界の読み書きの失敗は `fetch_backfill` 全体を失敗させない。読み出しの失敗は空（境界未確定）として API へ、書き込みの失敗は握りつぶす（#228 と同じ方針）。
- ソース単位の取得失敗は他ソースの結果と境界更新を妨げない。失敗したソースの境界は変えない（初回取得では行を作らない）。次回以降そのソースに境界が無ければ、そのカラムのキャッシュ優先は `FallbackBoundaryUnset` のままとなり、カラムを開き直すまで API 経由が続く（保守的な挙動）。
- 0件ページを「枯渇」とみなして `""` にする扱いは `fill_gap` の枯渇判定と同じ。#228 の単一ソースは0件ページで境界を更新しなかったので、これは本設計で新たに置く前提である。`""` は最も広い完全性の主張になる。ただし枯渇を `""` にしないと、疎なソースが有効境界 `E = max(b_i)` を永久に塞ぎ、密なソースとの組み合わせでキャッシュ優先が実質働かなくなるため、この前提を採る。0件ページが枯渇を意味しない API があれば、そのソースを含むカラムでは欠落しうる（実装後の実機確認項目とする）。

## テスト方針

### `store/`（SQLite 通常テスト、Postgres/MySQL は既存パターンに倣い `#[ignore]` の実DBテスト）

- `get_fetch_boundaries` / `replace_fetch_boundaries` / `extend_fetch_boundaries` のラウンドトリップ。`replace` は既存行を全消去してから挿入する。`extend` は古い方向にのみ進み、行が無ければ挿入する。`""` への延長が成立する。
- prune: 複数ソース行が一括で生存最古IDへ引き上がる（`""` の行も引き上がる）。連合ノートで削除最大IDが生存最古より新しい場合はその値まで引き上がる。全滅時はカラムの全行が消える。3種の間引き経路（`keep` 超過・`max_age_days`・`max_size_mb`）を既存テストに倣って確認する。Postgres/MySQL のチャンク化ケースも既存テストを新テーブルに合わせて更新する。
- `clear_column_notes` / `clear_all_fetch_boundaries` が新テーブルの行を消す。
- マイグレーション: 旧 `column_fetch_boundary` に行がある状態で `open_cache` すると新テーブルに `source_idx=0` でコピーされ、旧テーブルが消える。二度実行しても壊れない（冪等）。旧テーブルが無い新規DBでは何もしない。

### `commands/column.rs`

- `plan_boundary_initial` / `plan_boundary_extend` の単体テスト: 全ソース成功、1ソース失敗、枯渇ソース（`""`）、ソース単位の連続性（1ソースだけ `until_id < prev`）、境界未確定ソース。
- 回帰テスト（truncateによる欠落）: 2ソースが交互にインターリーブし全件フィルタを通過する状態で `fetch_and_filter_multi` 相当の合成を行い、`cacheable` をキャッシュした後、`E` 以上の全ノートが `load_cached_before` で取得できること。`notes`（truncate後）だけをキャッシュした場合は欠落することを対比で示す。
- 有効境界 `E = max(b_i)` の判定（全ソースの境界が揃わない場合は API へ、枯渇ソース `""` が `E` を塞がない）。既存の `cache_backfill_page` テストは維持する。
- `from cache` 併用カラム、およびストリーミングを持たないソース(User / Tag / Search)を含むカラムは常にAPI経由（`cache_eligible == false`）。

## 実装ボリュームの見立て

変更は `store/note_cache.rs`（trait + `delete_matching`）、`store/db.rs`（スキーマ + マイグレーション）、`store/sqlite_backend.rs` / `store/postgres_backend.rs` / `store/mysql_backend.rs`（実装・スキーマ・prune・テスト）、`commands/column.rs`、`state.rs`（コメント）。フロントエンド・TS バインディングの変更はない（Tauri コマンドの署名は変えない）。

## 後続の変更(Issue #429)

上記の `fetch_backfill` の判定は、レビュー指摘への対応で次の2点が変わっている。本文は当時の設計のまま残す。

- `cache_backfill_page(boundary, until_id, cached, raw_loaded, limit)` は、有効境界が `""`(全ソース枯渇済み)かつ `load_cached_before` の生の読み出し件数 `raw_loaded` が `limit` 未満なら、`cached.len()` が足りなくてもキャッシュだけで返す(0件も可)。フィルタ後の件数ではなく生の件数で判定するのは、ミュート等で間引かれて短くなっただけのページを末尾到達と取り違えないため。`prune` は `""` の行も生存最古IDへ引き上げるので、キャッシュが削られた後に完全扱いが残ることはない。
- `get_fetch_boundaries` の失敗は「境界未確定」ではなく `FallbackOther` に計上する(空扱いでAPIへ落ちる動作は変わらない)。`FallbackBoundaryUnset` はDBエラーでは増えない。
