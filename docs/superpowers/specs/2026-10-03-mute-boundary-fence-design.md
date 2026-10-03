# set_mute と実行中の取得の競合 設計 (Issue #452)

`set_mute` が捨てた backfill 境界を、実行中の REST 取得が書き込んで復活させる競合を、`ColumnFence` に「境界の世代」を足して防ぐ。#446 / PR #451(カラムごとの世代フェンス)で「後続の候補」とした項目。

## 背景・原因

`set_mute`(`src-tauri/src/commands/mute.rs`)は、ミュート設定を保存して `state.mute` を差し替えた後、`clear_all_fetch_boundaries` で全カラムの境界を捨てる。ミュート解除方向の変更は、除外済み(=キャッシュされていない)ノートを読み直せないため、次回の backfill を API 経由に倒す意図(#228)。

ところが `set_mute` は `ColumnFence` を通らない。取得経路は、取得を始めた時点の `state.mute` でフィルタした結果を、取得の後で書き込む。このため、`clear_all_fetch_boundaries` より後に、次の書き込みが入りうる。

| 経路 | 書き込み | 復活するもの |
|---|---|---|
| `fetch_backfill` | `extend_fetch_boundaries`(開始時に読んだ境界の写しから `plan_boundary_extend` で作った延長) | `extend` は行が無ければ挿入するので、捨てた境界が復活する |
| `open_stream_and_fetch` | `replace_fetch_boundaries` | 旧ミュートでフィルタした結果を、完全な範囲として書く |
| ギャップ埋め(起動時・再接続時) | `apply_gap_fill_boundaries`(ロックの外の `begin` と、中での読み書きが分かれていた) | 最新の境界ではなく古い写しに基づく書き込みになりうる。ただし、境界を**読み取ってから書く**までを、`set_mute` と排他にすれば守れる(後述) |

境界が復活すると、キャッシュ優先の backfill が、旧ミュート設定で除外されたノートを欠いたキャッシュを「完全」として返す。ミュートを解除したのに、解除したユーザーのノートが上スクロールで出てこない状態になりうる。窓は取得の往復1回分で狭い。コードの読解による推定で、再現は確認していない。

## スコープ

- 対象: `set_mute` が捨てる境界を、実行中の取得が復活させないようにする。
- 対象外:
  - サーバー側ミュートの同期(`set_server_mutes`)。こちらは、境界を捨てる処理自体が無い、別の問題。
  - ライブ受信の `cache_note`(#446 の spec で対象外とした隙間)。
  - DB スキーマ、Tauri コマンドの署名、TS バインディング、`Error` の variant。

## 設計

### 方針: 境界の書き込みだけを守る

ミュート変更で無効になるのは、「境界より新しい範囲は完全」という主張だけである。キャッシュしたノートは、読み出し時にミュートを再適用するので、書いてよい。したがって、境界の世代が古い書き込みは、**古い情報から作った境界(`fetch_backfill` の `extend` と `open_stream_and_fetch` の `replace`)の書き込みだけを飛ばし**、ノートのキャッシュ、ノートの返却、ストリームを開く処理、`ColumnGapFill` イベントは、そのまま行う。境界が未確定のままなら、次回の backfill は API 経由になるので安全である。

カラム定義の変更(#446)のように「全部捨てる」にすると、カラムを開いている最中にミュートを変えたとき、`open_stream_and_fetch` が失敗して、カラムが開けなくなる。競合そのものより悪い結果なので採らない。

### `ColumnFence`(`src-tauri/src/fence.rs`)

- フィールドを足す: `boundary_gen: AtomicU64`(境界の世代。全カラム共通)、`boundary_lock: tokio::sync::RwLock<()>`。
- `Epoch` に、`begin` / `invalidate` が控えた時点の境界の世代を足す(`PartialEq` は両方を比べる)。

```rust
pub struct Epoch { column: u64, boundary: u64 }

/// 従来どおりカラムの世代を確認する(古ければ `None`)。`f` には、控えた境界の世代が
/// いまも現在か(`boundaries_ok`)が渡される。
pub async fn write_if_current<F, Fut, T>(&self, column_id: &str, epoch: &Epoch, f: F) -> Option<T>
where F: FnOnce(bool) -> Fut, Fut: Future<Output = T>;

/// 境界の書き込みロックを取り、`f` を実行し、その後で境界の世代を進める。
pub async fn invalidate_boundaries<F, Fut, T>(&self, f: F) -> T
where F: FnOnce() -> Fut, Fut: Future<Output = T>;
```

- `write_if_current` は、`boundary_lock` の**読みロック**を取ってから、カラムのロックを取る。`f` の実行中は両方を持つ。
- `invalidate_boundaries` は、`boundary_lock` の**書きロック**を取る。実行中の書き込み(読みロックを持つ)が終わるまで待つので、`f` が捨てた境界を、直後に書き込みが復活させる隙間が無い。`f` の後、書きロックを持ったまま `boundary_gen` を進める。
- `begin` は、`boundary_gen` を読むだけでロックを取らない。`invalidate`(単一カラム)が返す `Epoch` には、その時点の `boundary_gen` を入れる。
- ロックの順序は常に「境界 → カラム」。`invalidate`(単一カラム)は境界のロックを取らないので、循環は無い。`f` の中から、別の `write_if_current` / `invalidate` / `invalidate_boundaries` を呼ばない。

### 不変条件

1. 書き込み側は、ミュート設定を読む(フィルタする)**前に** `begin` する。既存のすべての書き込み経路は、取得の前に `begin`(または `invalidate` の戻り値)で世代を控えており、すでに満たしている。
2. `set_mute` は、`state.mute` を差し替えた**後で** `invalidate_boundaries` を呼ぶ(現在のコードの順序のまま)。

この2つで、どの順序でも、旧ミュートの結果に基づく境界は復活しない。

| 書き込み側(W)と `set_mute`(S)の順序 | 結果 |
|---|---|
| W が `begin` → W が旧ミュートでフィルタ → S の書きロック取得 → S が `clear` → W が書く | W の境界の世代が古いので、境界を飛ばす |
| W が `begin` → W が書く(読みロックを持つ) → S の書きロック取得 → S が `clear` | S が、W が書いた境界を `clear` する |
| W が書いている最中に S が書きロックを要求 | S は W の完了を待つ。その後 `clear` する |
| W が `begin`(S の世代更新より後) | `state.mute` はすでに新しいので、W は新ミュートでフィルタし、境界を書いてよい |
| W が `begin`(`state.mute` の更新後、世代更新の前) → 書き込みは世代更新の後 | 境界を飛ばす。新ミュートでフィルタした結果でも飛ばすが、害は無い(不要な破棄だけ) |

### `commit_*` ヘルパー(`src-tauri/src/commands/column.rs`)

`write_if_current` の `f` が受け取る `boundaries_ok` が偽なら、**古い情報から作った境界**の書き込みだけを飛ばす。

- `commit_backfill_writes`: `extend_fetch_boundaries` を飛ばす(ノートの `cache_notes` は行う)。
- `commit_initial_writes`: `replace_fetch_boundaries` を飛ばす(ノートのキャッシュと `on_current` は行う)。
- `commit_gap_fill_writes`: **飛ばさない**。`apply_gap_fill_boundaries` は、ロックの中で最新の境界を読み、既存の行を新しい側(保守的な方向)へ動かすだけで、`clear` 後の空の状態からは何も書かない(`prev` が空なら `None`)。したがって、`set_mute` との排他(境界のロック)だけで、境界を復活させない。しかも、境界の世代が古いことを理由に引き上げを飛ばすと、打ち切られたギャップが境界で覆われないまま残り(#432)、キャッシュ優先の backfill が穴をまたいでしまう。

ヘルパーの外向きの引数と戻り値は変わらない。

### `set_mute`(`src-tauri/src/commands/mute.rs`)

`clear_all_fetch_boundaries` を、`state.column_fence.invalidate_boundaries(|| async { ... })` の中で実行する。`state.mute` の差し替えは、その前のまま。

## 検討した代替案

- **全カラムのエントリをID順に全部ロックして、世代を進める**: 古い世代の書き込みが全部捨てられるので、`open_stream_and_fetch` が `Err` を返し、ミュートを変えた拍子にカラムが開けなくなる。
- **境界の世代を DB に持つ**: 3バックエンドのスキーマ変更とマイグレーションが要る。実行中の処理はプロセスをまたがないので、永続化の利点が無い。

## テスト

- `fence.rs` の単体テスト:
  - 既存のテストは、`write_if_current` の `f` の引数が増えるので、`|| async {..}` を `|_| async {..}` に直す(機械的な変更)。
  - `begin` の後に `invalidate_boundaries` すると、`write_if_current` の `boundaries_ok` が偽になる。
  - `invalidate_boundaries` を呼んでいなければ、`boundaries_ok` は真のまま。
  - `invalidate_boundaries` の後に `begin` した世代は、`boundaries_ok` が真。
  - カラムの世代が古ければ、境界の世代に関わらず `None`(従来どおり)。
  - 実行中の `write_if_current` が終わるまで、`invalidate_boundaries` が待たされる。実行中の `invalidate_boundaries` が終わるまで、`write_if_current` が待たされる(`tokio::sync::Notify` で順序を固定する)。
  - `invalidate`(単一カラム)が返す `Epoch` は、その時点の境界の世代を持つ。
- `commit_*` のテスト(実際の `NoteCacheStore`、SQLite のメモリ DB): `set_mute` 相当(`invalidate_boundaries` の中で境界を `clear`)の後に、境界の世代が古い書き込みをして、次を確認する。
  - ノートは書かれる。
  - 境界は復活しない(`clear` 後の空のまま)。
  - `commit_initial_writes` の `on_current` は走る。
  - `commit_gap_fill_writes` は、境界の世代が古くても、`clear` 後の空の状態からは何も書かず(復活しない)、`true` を返す。ノートは書かれる。
  - `commit_gap_fill_writes` は、境界の世代だけが古く、別の書き込みが `clear` 後に作った行がある場合、引き上げを**行う**(ギャップを覆うため)。
  - 境界の世代が現在の書き込みは、従来どおり境界を書く(対照)。
- 世代を進める操作を外すと、上のテストが落ちること(変異確認)。

## 影響範囲

- 変更するファイル: `src-tauri/src/fence.rs`、`src-tauri/src/commands/column.rs`、`src-tauri/src/commands/mute.rs`。
- DB スキーマ、Tauri コマンドの署名、TS バインディング、フロントエンドは変わらない。
- `ColumnFence::write_if_current` のクロージャの引数が増える(crate 内のみ。呼び出しは `commit_*` ヘルパー3つとテスト)。

## 後続の候補(本設計では扱わない)

- サーバー側ミュート(`set_server_mutes`)やワードミュートの変更は、境界を捨てない。ミュート解除方向の変更が、キャッシュ優先の backfill に反映されない点は同じで、別 Issue とする。
