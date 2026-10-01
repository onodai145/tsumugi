# 複数ソースbackfillの読み飛ばしを防ぐ「透かし」方式 設計 (Issue #428)

Issue #238（`docs/superpowers/specs/2026-09-30-multi-source-backfill-cache-design.md`）で「別 Issue」とした既存の問題を直す。

## 背景・原因

`fetch_and_filter_multi`（`commands/column.rs`）は、全ソースに同じ `until_id` を渡し、各ソースから `INITIAL_LIMIT`（20件）の生ページを取得する。取得結果をマージしてフィルタを通し、新しい順に20件へ切り詰めて画面へ返す。次回の `until_id` は、フロント（`loadMore`）が「表示中の最古ノートの id」から決める。

1. 密なソース A と疎なソース B があり、フィルタで A のノートの大半が弾かれる。
2. 疎な B の生ページ20件は A より深く(古く)まで届く。画面の最古ノートは、B の生ページの最古 `b_old` 付近になり、A の生ページの最古 `a_old` より古くなる（`b_old < a_old`）。
3. 次の `until_id` はその古い値なので、A のノートのうち `(次の until_id, a_old)` の区間は、どのページにも現れない。

#238 / PR #426 のキャッシュ優先化とは独立した既存の問題(複数ソースは従来から常に API 経由で同じ飛ばしが起きていた)。キャッシュ優先経路は、有効境界 `E = max(b_i)`（各ソースの取得済み最古idの最大）以上のノートだけを返すため、この飛ばしを起こさない。

`fillRemainingGap`（#427）も `fetch_backfill` を通るので同じ飛ばしの影響を受ける。

## 方針（透かし）

各ソースの生ページの最古 id のうち**最も新しいもの**を透かし `c` とする（最も浅いソースの深さ）。1ページ分の取得結果のうち、`id >= c` のノートだけを画面へ返す。`c` より古いノートは画面へ返さず、キャッシュ(`cacheable`)にだけ入れる。

返したノートは全て `c` 以上なので、次の `until_id`（フロントが表示中の最古から決める）も `c` 以上になる。次回は全ソースが `until_id` から取り直すので、A の区間は飛ばされない。`c` より古い B のノートは次回再取得され、重複は既存の id 除去で消える。これはキャッシュ優先経路の有効境界 `E` と同じ意味づけで、境界テーブルの意味は変えない。

フロント・TS バインディング・DB スキーマ・`NoteCacheBackend` trait は変更しない。次の `until_id` は今までどおり表示中の最古ノートで、`loadMore` が追記のみでソートしない点も問題にならない（返るノートは全て `until_id` より古い）。

## スコープ

- 対象: `fetch_and_filter_multi` が返す画面用ノートの選択（透かし）と、返すノートが0件のときの内部再取得。`fetch_backfill`（API経路）と `open_stream_and_fetch`（初回取得）の両方が通る。
- 対象外:
  - `fetch_backfill` のキャッシュ優先経路（既に有効境界 `E` で絞っている）。
  - `fill_gap`（#432）。ソース単位のカーソルを既に持つ。
  - フロント。通知カラム。
  - 取得に失敗した(`Failed`)ソースの飛ばし。そもそもそのソースのノートは取得できないので従来どおり。

## 設計

### 透かしの計算（純粋関数）

```rust
/// 1パス分の取得結果から、画面へ返してよい最古のid(透かし)を決める。
/// 各ソースの生ページの最古idのうち最大(=最も浅いソース)。`Exhausted`/`Failed` のソースは制約しない。
/// `cache_min`(`from cache` の検索結果が `INITIAL_LIMIT` 件に達したときのその最小id)も1ソースとして含める。
/// 制約するソースが無ければ None(全ノートを返してよい)。
fn backfill_watermark(outcomes: &[SourceOutcome], cache_min: Option<&str>) -> Option<String>
```

### 画面用ノートの選択

`split_display_and_cacheable(filtered, use_cache)` に引数 `watermark: Option<&str>` を足す。画面用 = 重複除去・`created_at` 降順の後、`id >= watermark` のものだけを先頭から `INITIAL_LIMIT` 件。`cacheable` は従来どおり（`use_cache` でなければ重複除去済みフィルタ通過分の全件、`use_cache` なら画面用と同じ）。`watermark = None` のときは従来と同じ挙動。

### 取得ループ（再取得）

画面用が0件で、透かしがあり(=まだ深く取れるソースがある)、パス数が上限未満のとき、`until_id = c` で取り直す。これが無いと、A が全部フィルタで落ち B のノートが全て `c` より古い場合に、フロントが同じ `until_id` を繰り返してそのカラムが遡れなくなる。

- パス数の上限 `BACKFILL_MAX_PASSES = 5`。
- 上限に達しても画面用が0件のときは、最後の1パスだけ透かしを外して返す（従来の挙動）。まれに読み飛ばしが起こりうるが、スクロールは止まらない。ユーザーに「止まる」ことは「たまに飛ぶ」ことより悪いため。
- `cacheable` は全パスの和集合（id 重複除去）。
- `source_outcomes` は、パス間でソースごとに次の規則でマージする(`merge_source_outcome(prev, next)`)。
  - 最初のパスで `Failed` のソースは、以降の結果によらず `Failed`（先頭から連続して取得できていないため、境界に使えない）。
  - `Exhausted` はそれ以降も `Exhausted`。`Fetched(a)` と `Fetched(b)` は `Fetched(min(a, b))`、`Fetched` と `Exhausted` は `Exhausted`、`Fetched` と(後続パスの)`Failed` は `Fetched` のまま。
  - これにより、`fetch_backfill` の `plan_boundary_extend`（`until_id >= b_i` の連続性チェック）と `open_stream_and_fetch` の `plan_boundary_initial` は、再取得を挟んでも各ソースの「先頭から連続して取得した最古id」を受け取る。

### テスト容易性

取得ループを、1パス分の取得を引数で受け取る関数に切り出す。ネットワークに依存せず、issue のシナリオを単体テストで再現できる。

```rust
struct BackfillPass {
    /// フィルタ/ミュート適用済み、重複除去前のノート。
    notes: Vec<Note>,
    /// `resolved.kinds` と同じ並び。
    outcomes: Vec<SourceOutcome>,
    /// `from cache` の検索結果が `INITIAL_LIMIT` 件に達したときの最小id。
    cache_min: Option<String>,
}

async fn collect_backfill_pages<F, Fut>(until_id: Option<&str>, use_cache: bool, fetch_pass: F) -> Result<FilteredFetch>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<BackfillPass>>;
```

`fetch_and_filter_multi` は、1パス分の取得(各ソースの REST 取得・`from cache` 検索・フィルタ適用)をクロージャとして渡す。

## 限界

- 最悪の API 呼び出しは `BACKFILL_MAX_PASSES × ソース数` 回に増える。通常は1パスで終わる(画面用が0件になるのは、浅いソースの生ページ全体がフィルタで落ち、かつ他ソースのノートが全て `c` より古い場合だけ)。
- 取得に失敗した(`Failed`)ソースは透かしを制約しない。そのソースの飛ばしは従来どおり。
- `from cache` を含むカラムのキャッシュ側の透かしは、検索結果が `INITIAL_LIMIT` 件に達した場合だけ使う(下回れば枯渇扱い)。
- 上限(5パス)に達した場合のみ、従来どおりの読み飛ばしが起こりうる。

## 代替案（不採用）

- **ソースごとに独立したカーソルをフロントへ返す**: `fetch_backfill` の戻り値型とフロントの `loadMore` の状態(カーソル保持・キャッシュ復元時の初期化・追記順の整合)に及び、変更が大きい。透かし方式はバックエンドだけで済む。
- **返すノートが0件のとき空を返す**: フロントが同じ `until_id` を繰り返すため、そのカラムは遡れなくなる。

## テスト

- `backfill_watermark`: 全ソースが `Fetched` なら最大の最古id／`Exhausted` と `Failed` は無視／制約するソースが無ければ None／`cache_min` を含める。
- `split_display_and_cacheable`: 透かしより古いノートは画面用に入らず `cacheable` には入る／`watermark = None` は従来どおり／`use_cache` では `cacheable` が画面用と同じ。既存テストは新しい引数 `None` に更新する。
- `merge_source_outcome`: 上記の規則を網羅する。
- `collect_backfill_pages`（スクリプト化した偽の1パス取得を使う）:
  - issue のシナリオ: 密な A（全てフィルタで落ちる）と疎な B で、1パス目の画面用が透かし以上のノートだけになる。
  - 画面用が0件なら `until_id = c` で再取得する。取得に渡された `until_id` を検証する。
  - 全ソースが枯渇していれば再取得しない。
  - 上限パス数で止まり、最後の1回は透かしなしで返す。
  - 先頭パスで `Failed` のソースは `Failed` のままになる。
- `cd src-tauri && cargo test` が通ること。TS バインディングは変更なし(`generates_frontend_bindings` で差分が出ないこと)。
- 実機確認(任意・難しい見込み): 密なソースと疎なソースを `from` に並べ、強いフィルタを掛けたカラムで遡り、ノートの抜けが無いことを Xvfb + `dbus-run-session` 越しに確認する。再現できなければ省略した旨を報告する。
