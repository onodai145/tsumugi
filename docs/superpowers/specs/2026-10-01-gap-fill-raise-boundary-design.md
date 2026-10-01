# 打ち切られたギャップ埋め後のbackfill境界の引き上げ 設計 (Issue #432)

Issue #228 / #238 のbackfill境界（`column_source_boundary`。`docs/superpowers/specs/2026-09-30-multi-source-backfill-cache-design.md`）に対する穴を塞ぐ。#427（`fillRemainingGap` のキャッシュバイパス, `docs/superpowers/specs/2026-10-01-gap-fill-bypass-cache-design.md`）の続き。

## 背景・原因

backfill境界は「`id > E` はAPI取得済みで完全（`E = max(b_i)`）」を主張する。一方、起動時の `resume_column` と再接続時の `gap_fill_on_reconnect` は、`fill_gap` が `gap_fill_limit` / ページ数上限で打ち切られても境界を更新しない。

打ち切られると、キャッシュには「ギャップより新しい取得済みノート G」と「閉じる前から持っていた古いノート K」が入り、間の区間 `(newest_known_id, G の最古)` は未取得のまま残る。この穴は境界 `E` より新しい側にあり、境界の主張が偽になる。

その結果、次の2つが起きる。

1. **`loadMore` が穴を飛ばす。** 再起動すると gapMarker（フロントのメモリ上のみ）が消える。表示中の最古ノートが G の中にあるとき、`until_id > E` なので `fetch_backfill` は `load_cached_before` で G の残りの後に K を続けて返し、穴を黙って飛ばす。
2. **復元一覧が穴をまたぐ。** `resume_column` の `load_cached(INITIAL_LIMIT)` は、G が20件未満だと穴をまたいで K まで返す。穴はマーカーなしで一覧の中に隠れる。

gapMarker の永続化は採らない。マーカーは1つしか持てず、穴が複数できると古い穴の情報が上書きされる。また、復元一覧が穴をまたぐ問題は解決しない（後述の代替案）。

## スコープ

- 対象: `resume_column`（起動時）と `gap_fill_on_reconnect`（再接続時）の `fill_gap` 後に境界を引き上げる。`resume_column` の復元一覧を有効境界以上に絞る。
- 対象外:
  - `fillRemainingGap` / `fetch_backfill` の挙動（#427 で対応済み。自己修復はその経路でも働く）。
  - 通知カラムのギャップ埋め（`notification_gap_fill_on_reconnect`。キャッシュ境界を持たない）。
  - フロント・TSバインディング・DBスキーマ・バックエンド trait。変更しない。
  - `cache_eligible` でないカラム（`from cache` / User・Tag・Search を含む）。境界が空のため計画関数が空を返し、実質何も起きない。

## 設計

### 境界の意味（再掲）

ソース `i` の `b_i` は「`id > b_i` のノートはソース `i` からAPI取得済みで、フィルタ通過分は `column_note` に記録済み」。`""` はそのソースが先頭まで枯渇済み。行が無いソースは未確定。カラム全体で完全な範囲は、全ソースの行が揃うときの `id > max(b_i)`。ID比較は辞書順。

### `fill_gap` が返す情報

`GapFillResult`（`commands/column.rs` 内の非公開構造体）に、`fill_gap` が内部に持っている情報を足す。

```rust
struct GapSourceState {
    /// このソースが遡って取得できた最古の生ページのID。1ページも取れなければ None。
    oldest_fetched: Option<String>,
    /// newest_known_id に追いついた(またはソースが枯渇した)か。
    reached_target: bool,
}
// GapFillResult に追加
sources: Vec<GapSourceState>,
all_reached: bool,
/// limit で切り捨てたノートの id の最大値。切り捨てが無ければ None。
dropped_floor: Option<String>,
```

`oldest_fetched` は既存の `cursors[i]`、`reached_target` は既存の `reached_target[i]` から作る（ロジックの変更なし）。`all_reached` は `reached_target.iter().all(..)`。`dropped_floor` は `finalize_gap_fill` が `limit` へ切り詰める前に、捨てるノートの id の最大値として計算する。`resolved.kinds.is_empty()` の早期 return では `sources: vec![]`, `all_reached: true`, `dropped_floor: None`。

### 引き上げの計画（純粋関数）

```rust
/// 打ち切られたギャップ埋めの後に書き戻す境界。変更が無ければ None。
fn plan_boundary_raise_after_gap(
    prev: &HashMap<u32, String>,
    sources: &[GapSourceState],
    all_reached: bool,
    dropped_floor: Option<&str>, // GapFillResult::dropped_floor(limit で捨てたノートidの最大値)
) -> Option<Vec<(u32, String)>>
```

- `all_reached` が真で、かつ `dropped_floor` が None なら None（穴は無い。何もしない）。
- そうでないとき、`prev` の各行 `(i, b)` について新しい境界を決め、全行を並べた `Vec` を返す（`replace_fetch_boundaries` に渡すため、変更しない行も含める）。
  - `sources[i].reached_target` が真: `dropped_floor` があれば `max(b, dropped_floor)`、無ければ `b` のまま。
  - `reached_target` が偽で `oldest_fetched` が `Some(o)`: `max(b, o, dropped_floor)`。
  - `reached_target` が偽で `oldest_fetched` が `None`: その行を落とす（未確定にする）。
- `prev` に無いソース（未確定）は行を作らない。
- 結果が `prev` と同一なら None。

`dropped_floor` を `max` に含める理由: `finalize_gap_fill` は収集結果を新しい順に `limit` 件へ切り詰めて `cache_notes` に書く。そのため、ソースが `o` まで遡って取得していても、切り捨てられた範囲は `column_note` に無い。完全と言えるのは `max(各ソースの o, 捨てたノートの id の最大値)` より新しい側だけ。切り詰めは created_at 順、境界は id 順なので、捨てたノートの id が残した最古より大きくなりうる。下限は「残した最古」(`boundary_id`)ではなく「捨てた id の最大値」とする。

追いついたソースにも `dropped_floor` を適用し、`all_reached` が真でも `dropped_floor` があれば動かす理由（実装中の最終レビューで判明した、当初の spec の前提の誤り）: `fill_gap` の内側ループは1周で各ソースが1ページずつ足すため、全ソースが追いついたのに収集件数が `limit` を超えて切り捨てが起きうる。切り詰めは全ソース合算なので追いついたソースのノートも捨てられる。行は「ソースごとの id > b は揃っている」を表し、後続の延長（`plan_boundary_extend` / `extend_fetch_boundaries`）もソースごとに動くため、追いついたソースの行を古いまま残すと、後で有効境界が穴の中へ戻る。

判定を `truncated`（収集が1件以上ある場合のみ真）ではなく `all_reached` にする理由: 全ソースの取得失敗や強いフィルタで収集が0件でも、未取得の範囲は残る。その場合 `floor` は None で、`o` だけで引き上げる。`o` が取れないソースは行を落とす。

### 呼び出し側

`resume_column`（バックグラウンドタスク内、`fill_gap` の直後）と `gap_fill_on_reconnect`（`fill_gap` の直後、`gap_result.notes.is_empty()` の早期 return より**前**）の両方で、共通の非同期ヘルパーを呼ぶ。

```rust
async fn apply_gap_fill_boundaries(cache: &NoteCacheStore, column_id: &str, gap: &GapFillResult)
```

`get_fetch_boundaries` → `plan_boundary_raise_after_gap` → `Some` なら `replace_fetch_boundaries`。DBエラーは握りつぶす（`let _ =`。境界が更新されなくても既存の挙動に戻るだけで、他の呼び出しと同じ扱い）。`cache_eligible` でないカラムは `get_fetch_boundaries` が空を返すので何も起きない。

### 復元一覧の絞り込み

`resume_column` で `load_cached(INITIAL_LIMIT)` の結果を、`cache_eligible` かつ有効境界 `E` が確定している場合に限り `n.id >= E` で絞る（`fetch_backfill` の `cached.retain` と同じ比較）。絞った後が空なら、既存の「キャッシュが空」経路（`open_stream_and_fetch`）に入る。`record_resume` は絞った後の結果で記録する。

境界が未確定のカラムは絞らない（現状維持）。

### 自己修復（既存の仕組み、追加実装なし）

引き上げ後、`until_id <= E` のbackfillは `fetch_backfill` がAPIへ行く。取得が既存境界と連続していれば `plan_boundary_extend`（`until_id >= b_i`）が境界を古い方向へ延ばし、穴が埋まるとキャッシュ優先に戻る。`fillRemainingGap`（#427でバイパス済み）で埋めた場合も同じ。

## 限界

- 打ち切られたギャップ埋めで、ある境界行のあるソースが1ページも取得できなかった場合、その行を落として未確定にする。有効境界が `None` になり、そのカラムはキャッシュ優先をやめてAPI経由になる。復元一覧も絞らない。境界は、カラムのキャッシュが空になって `open_stream_and_fetch` が作り直すまで戻らない。かなり稀で、動作は安全側（APIへ行く）。
- 引き上げは、`fetch_backfill` の境界延長と競合しうる（#429 の既知項目と同種）。`fetch_backfill` は先頭で境界を読み、API取得の往復の後で `extend_fetch_boundaries` を書くため、窓は往復の間ずっと開いている。たとえば起動直後のギャップ埋め中にユーザーが最下部まで `loadMore` すると、古い境界を根拠に延長が書かれ、引き上げた境界が穴より下へ戻りうる。窓を塞ぐには境界の更新をカラム単位で直列化する必要があり、本Issueの範囲外とする。
- ギャップ埋めが `limit` で切り捨てたのに全ソースが追いついた場合(`truncated=false`)、フロントにギャップマーカーは出ない。境界は引き上げられるので再起動後は穴が復元一覧から外れるが、同一セッション内の一覧には穴が静かに残る（既存の挙動）。

## 代替案（不採用）

- **gapMarker の永続化**: 穴が複数できると1つのマーカーでは古い穴を失う。復元一覧が穴をまたぐ問題も残る。
- **穴より古いキャッシュ行の削除**: 3バックエンドへの新規操作とそのテストが必要で、キャッシュを不必要に捨てる。境界の引き上げで同じ効果が得られる。
- **境界の延長専用操作の追加**: 引き上げは `replace_fetch_boundaries` で足りる。trait を変えずに済む。

## テスト

- Rust 単体（`plan_boundary_raise_after_gap`）:
  - 全ソース追いつき、切り捨て無し → None。
  - 全ソース追いついたが切り捨てあり（`dropped_floor = Some`）→ 全ソースを `max(b, dropped_floor)` へ引き上げ。
  - 単一ソースが打ち切り → `max(b, oldest_fetched, dropped_floor)` へ引き上げ。
  - 複数ソースで一部だけ追いつき → 切り捨て無しなら追いついたソースの行は据え置き、切り捨てありなら `dropped_floor` まで引き上げ。
  - 収集0件（`dropped_floor = None`）でも `oldest_fetched` で引き上げ。
  - `finalize_gap_fill`: 全ソース追いついても `limit` 超過で `dropped_floor` を返す。created_at 順と id 順がずれる場合は捨てた id の最大値を返す。
  - `oldest_fetched = None` のソース → 行を落とす。
  - `prev` に無いソース → 行を作らない。
  - 引き上げ後の値が既存と同一 → None。
  - 境界が空（`cache_eligible` でないカラム）→ None。
- Rust 単体（復元一覧の絞り込み判定を純粋関数に切り出して検証）: `E` 未確定なら絞らない／`E` 確定なら `id >= E` だけ残す／すべて落ちたら空。
- `cd src-tauri && cargo test` が通ること。TSバインディングは変更なし（`generates_frontend_bindings` で差分が出ないこと）。
- 実機確認（任意・難しい見込み）: ギャップ埋めが打ち切られる状況（`gap_fill_limit` を小さくして再起動）を作り、再起動後にスクロールしてもギャップが飛ばされないことを、Xvfb + `dbus-run-session` 越しにdebug bridgeで確認する。再現できなければ省略した旨を報告する。
