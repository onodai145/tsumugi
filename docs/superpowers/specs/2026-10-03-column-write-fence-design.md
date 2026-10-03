# カラムのキャッシュ書き込みの世代フェンス 設計 (Issue #446)

`update_column` / `close_column` が実行中の REST 取得と競合し、旧フィルタの結果や古い境界がカラムのキャッシュに書かれる問題を、カラムごとの世代(epoch)とロックで防ぐ。#238 / PR #426 のレビュー指摘(#429 の項目1)の切り出し。

## 背景・原因

`update_column` は次の順で動く。

1. `load_column` → 新しい `kind` / `filter` を `settings.upsert_column` で保存。
2. ストリームを閉じ、`clear_column_notes`(`column_note` と全ソースの境界行を消す)。
3. `open_stream_and_fetch` が新フィルタで REST 取得し、`cache_notes` と `replace_fetch_boundaries` を行う。

一方、`fetch_backfill` などの REST 経路は、**開始時に読んだ**カラム定義(`resolved`)と境界の写しを持ったまま、ネットワーク取得(往復1回分以上)を挟んで進み、最後に `cache_notes` と `extend_fetch_boundaries` を行う。この間に上の 1〜3 が走ると、次が起きる。

- 旧フィルタの通過分が、新フィルタのカラムの `column_note` に入る。
- 境界が、新フィルタに対して古い方向へ進む(「境界より新しい範囲は完全」という前提が崩れ、キャッシュだけで返したページから、新フィルタで本来含まれるノートが欠けうる)。
- ソースの識別子が位置インデックスなので、`from` を並べ替えると、旧ソースの境界が別ソースの行に書かれる。
- `close_column` の後に、削除済みカラムの `column_note` と境界が孤児として書かれる。

同じ形の競合が、`fetch_backfill` 以外のカラムへの書き込み経路でも起きる(調査結果)。

| 経路 | 書き込み | 場所 |
|---|---|---|
| `fetch_backfill` | `cache_notes`、`extend_fetch_boundaries` | `commands/column.rs` |
| 起動時のギャップ埋め(`resume_column` が裏で `spawn`) | `apply_gap_fill_boundaries`、`cache_notes` | 同上 |
| 再接続時のギャップ埋め(`gap_fill_on_reconnect`) | 同上 | 同上 |
| `open_stream_and_fetch`(`add_column` / `update_column` / `resume_column` から) | `cache_notes`、`replace_fetch_boundaries` | 同上。`update_column` が連続すると、遅い方が新しい方を上書きできる |
| `close_column` | `clear_column_notes` | 同上 |

画面側にも同じ競合がある。`loadMore` / `fillRemainingGap` / `fillGapBelow` は `commands.fetchBackfill` の結果を `tab.notes` に足すが、`await` の間に `updateColumn` が `tab.notes` を差し替えても、旧フィルタで取得した結果がそのまま足される(`store.svelte.ts`)。

## スコープ

- 対象:
  - 上表の REST 経路の書き込みを、世代とロックで守る。世代が古い処理は書き込まずに捨てる。
  - 世代が古い `fetch_backfill` は、ノートを返さず空で返す。
  - フロントエンドで、`updateColumn` をまたいで返ってきた `fetchBackfill` の結果を捨てる。
- 対象外:
  - ライブ受信(`stream/connection.rs` の `cache_note`)。閉じる直前に処理中だったノートが `clear` の後に入りうる、別の隙間として残る。
  - `set_mute` の `clear_all_fetch_boundaries` と実行中の backfill の競合。同じ機構で守れるが、無効化の契機が違うので別 Issue とする(後述)。
  - DB スキーマ、TS バインディング、`Error` の variant。

## 設計

### `ColumnFence`(新規 `src-tauri/src/fence.rs`、`AppState` のフィールド)

カラムごとに「世代」と「ロック」を持つ。世代の値はプロセス全体の単調増加カウンタから払い出すので、`close_column` でエントリを消して作り直しても、同じ値が再利用されない。

```rust
pub struct ColumnFence {
    next: AtomicU64,                                            // 世代の払い出し
    columns: std::sync::Mutex<HashMap<String, Arc<Entry>>>,     // column_id -> Entry
}
struct Entry { epoch: AtomicU64, lock: tokio::sync::Mutex<()> }
pub struct Epoch(u64);

impl ColumnFence {
    /// 現在の世代を控える。エントリが無ければ作る。ロックは取らない。
    pub fn begin(&self, column_id: &str) -> Epoch;
    /// ロックを取り、世代が `epoch` と一致する時だけ `f` を実行する。
    /// 不一致、またはエントリが無い(close_column 済み)なら `None`(=古い)。
    pub async fn write_if_current<F, Fut, T>(&self, column_id: &str, epoch: &Epoch, f: F) -> Option<T>;
    /// ロックを取り、`f`(新しい定義の保存と clear)を実行し、**その後で**世代を進める。
    /// 進めた後の世代を `Epoch` で返す(`f` が失敗しても進める。定義が中途半端に更新されうるため)。
    pub async fn invalidate<F, Fut, T>(&self, column_id: &str, f: F) -> (Epoch, T);
    /// `invalidate` の後にエントリを消す(close_column 用)。
    pub fn remove(&self, column_id: &str);
}
```

- ロックを持つのは、`f`(DB 書き込み・`clear`)を実行している間だけ。ネットワーク取得の間は持たない。
- `f` の中から、別のカラムのフェンスやこのカラムの別の `write_if_current` / `invalidate` を呼ばない(ロックの入れ子を作らない)。

### 不変条件

1. **書き込み側は、カラム定義(`load_column` / `resolve_sources`)を読む前に `begin` する。**
2. **`update_column` は、新しい定義の保存(`upsert_column`)と clear を `invalidate` の `f` の中で行う。** `invalidate` は `f` の後に世代を進めるので、世代が進む時点で新しい定義は保存済みである。

この2つで、次のどの順序でも旧フィルタの結果が新しいカラムに入らない。

| 書き込み側が読んだ定義 | 控えた世代 | 書き込み時 |
|---|---|---|
| 旧 | 旧(bump より前) | 世代が進んでいるので捨てる |
| 新 | 新(bump より後) | 一致するので書く(新フィルタの結果) |
| 新(保存後〜bump の間に読んだ) | 旧 | 捨てる。害は無い(無駄な破棄だけ) |
| 旧(保存前に読んだ) | 新 | **起きない**(世代を控えるのは定義を読む前で、bump は保存後。控えた時点で bump 済みなら、保存も済んでいるため) |

世代を先に進めてから保存する順序だと、4行目の「旧定義+新世代」が生まれ、旧フィルタの結果を書けてしまう。不変条件2はこれを避けるための順序である(`f` の実行中に `begin` した処理は、まだ旧い世代を控えるので、`f` が終わって世代が進んだ後の書き込みで捨てられる)。

**`update_column` が同じカラムに2つ同時に走る場合**も、保存と世代の更新が同じロックの下で直列になるので、最後に `invalidate` を終えた呼び出しだけが現在の世代を持ち、その呼び出しの定義が、最後に保存された定義になる。先に終えた呼び出しの世代は古くなり、その取得結果は書き込まれない。保存を `invalidate` の外で行うと、「保存はBが最後だが、現在の世代を持つのはA」という食い違いが起きうるため、保存は `f` の中に置く。

### 各経路の変更(`commands/column.rs`)

- `fetch_backfill`: 先頭で `begin`。`cache_fetched` と、境界の延長(`extend_fetch_boundaries`)を、1つの `write_if_current` の中で実行する。`None`(古い)なら、取得したノートを返さず `Ok(vec![])` を返す。
- 起動時のギャップ埋め(`resume_column` の `spawn` 内)と `gap_fill_on_reconnect`: `begin` は `load_column` の前。`apply_gap_fill_boundaries` と `cache_notes` を `write_if_current` の中で実行する。`None` なら `ColumnGapFill` イベントも出さない。
- `open_stream_and_fetch`: 呼び出し元(`add_column` / `update_column` / `resume_column`)が `load_column` の前に `begin` した `Epoch` を引数で受け取る(private 関数のシグネチャ変更)。`cache_fetched` と `replace_fetch_boundaries` を `write_if_current` の中で実行する。`None`(古い)なら、書き込まず、`open_streams_only` も呼ばず、`Ok((vec![], vec![]))` を返す。世代が進んだということは、後続の `update_column`(または `close_column`)が、自分の定義でカラムを開き直す(閉じる)ので、この呼び出しが古い定義のストリームを開いてはならない。
- `update_column`: `upsert_column`、`state.connections.close`、`clear_column_notes` をこの順に `invalidate` の `f` の中で実行する(`upsert_column` が失敗したら、ストリームを閉じる前に中断する)。`invalidate` が返した `Epoch` を、後続の `open_stream_and_fetch` に渡す。
- `close_column`: `invalidate` の中で `clear_column_notes` を実行し、その後 `remove` でエントリを消す。エントリが無い状態の書き込みは `None`(古い)になるので、実行中の backfill は孤児データを書かない。

### フロントエンド(`frontend/src/lib/store.svelte.ts`)

- `TabView` に `epoch: number` を足し、`updateColumn` が差し替えを行うときに進める。
- `loadMore`、`fillRemainingGap`、`fillGapBelow` は、`commands.fetchBackfill` を呼ぶ前の `tab.epoch` を控え、結果が返った時に `tab.epoch` が変わっていたら、結果を捨てる(`tab.notes` に足さない)。
- バックエンドの「古い」判定とは独立した防御。DB が正しくても、`updateColumn` より前に取得し終わった旧フィルタの結果が、後から画面に届く経路を塞ぐ。

### エラーハンドリング

- 「古い」は正常系として扱う(`Ok(vec![])`)。新しい `Error` の variant は作らず、TS バインディングは変わらない。
- フロントは空の結果を、すでに「取得できる分がない」として扱える(`fillRemainingGap` は `fetched.length === 0` で `break`、`loadMore` は何も足さない)。

## 検討した代替案

- **世代を DB に持ち、条件付きで書き込む**: 原子性は DB が保証するが、3バックエンドのスキーマ変更とマイグレーションが要る。実行中の処理はプロセスをまたがないので、永続化の利点が無い。
- **ロックだけで、取得を含む処理全体を直列化する**: `update_column` が遅い REST に待たされる。しかも、`update_column` より前に始まった backfill が後で書く問題は直らない(世代が要る)。
- **ソースの識別子を位置インデックスから内容ベースにする**: 並べ替えの変種だけが直り、旧フィルタの通過分が混ざる問題は直らない。

## テスト

- `fence.rs` の単体テスト:
  - `begin` の後に `invalidate` すると、`write_if_current` が `None` になる。
  - 世代が一致すれば `f` が実行される。
  - `invalidate` が返した `Epoch` は現在の世代で書き込める。それ以前に `begin` で控えた世代は古い。
  - 同じカラムに `invalidate` が2回続くと、1回目が返した `Epoch` は古くなり、2回目のものだけが書き込める。
  - `remove` の後の `write_if_current` は `None` になる。
  - `invalidate` の実行中に呼んだ `write_if_current` は、`invalidate` が終わるまで待たされ、その後 `None` になる(`tokio::sync::Notify` で順序を固定する)。
  - `remove` で作り直したエントリの世代が、以前のものと一致しない。
- 実際の `NoteCacheStore`(SQLite のメモリ DB)での再現テスト: 「`begin` → `update_column` 相当(`invalidate` と `clear_column_notes`)→ 旧世代の `cache_notes` と `extend_fetch_boundaries`」のあと、ノートも境界も書かれていないこと。修正前のコードでは、同じ手順でノートと境界が書かれてしまうことも確認する。
- フロントエンド(Vitest): `updateColumn` をまたいで返った `fetchBackfill` の結果が、`loadMore` / `fillRemainingGap` / `fillGapBelow` のいずれでも `tab.notes` に入らない。

## 影響範囲

- 変更するファイル: `src-tauri/src/fence.rs`(新規)、`state.rs`(フィールド追加)、`lib.rs`(モジュール宣言)、`commands/column.rs`、`frontend/src/lib/store.svelte.ts`。
- DB スキーマ、Tauri コマンドの署名、TS バインディングは変わらない。
- `open_stream_and_fetch` の private なシグネチャが変わる。

## 後続の候補(本設計では扱わない)

- `set_mute` の `clear_all_fetch_boundaries` も、実行中の backfill が古い境界の写しで `extend_fetch_boundaries` すると、消したはずの境界が復活しうる。全カラムの世代を進める `invalidate_all` を足せば、同じ機構で守れる。
- ライブ受信の `cache_note` が、`close` と `clear` の間に処理中だったノートを入れる隙間。
