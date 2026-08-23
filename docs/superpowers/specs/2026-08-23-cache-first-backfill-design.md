# backfill系コマンドのキャッシュ優先化 設計 (Issue #228)

## 背景

`fetch_backfill`（上スクロールでの過去ページ取得）と `fill_gap`（起動時/再接続時のギャップ埋め、`fillRemainingGap` もこれを再利用）は、対象のノートが既にローカルSQLiteキャッシュDB（`cache.db`）に存在していても、毎回Misskey本体のREST APIへ問い合わせている。同じノートを何度もAPIに取りに行く無駄が発生しうる。

`resolve_sources`（`src-tauri/src/commands/column.rs`）は通常のカラム種別で常に `use_cache: false` を返す実装で、キャッシュDBから読むルートはTQLカラムで明示的に `from cache` を指定した場合のみである。

## 問題の核心: キャッシュの「穴」の曖昧さ

`column_note`（ノートとカラムの所属を記録するテーブル）には、フィルタ/ミュートを通過したノートしか記録されない（`fetch_and_filter_multi` はフィルタ適用後の結果だけを `cache_notes` する）。そのため、あるIDレンジに `column_note` の行が無いことは、以下のどちらかを意味し、区別できない:

1. そのレンジのノートをまだ一度もAPIに問い合わせていない（未取得）
2. そのレンジは取得済みだが、フィルタ/ミュートで弾かれて0件だった（取得済み・結果0件）

「キャッシュに無ければ即座にAPIへ」という素朴な実装は、この区別がつかないために「本来まだ見ていないレンジなのに、たまたま少数のキャッシュ行があるから完全とみなしてしまう」誤りを生みうる。これを避けるため、レンジの完全性を明示的に追跡する仕組みが必要。

さらに、Issue #6 のキャッシュ間引き（`prune`）は、フィルタ通過済みで正しくキャッシュされていたノートを後から削除しうる。単純な境界値だけでは、間引き後に「完全」の保証が静かに崩れる（＝本来表示されるべきノートが欠落する）リスクがある。この設計はこれも明示的に扱う。

## スコープ

- 対象: `resolve_sources` の結果が単一ソース（`resolved.kinds.len() == 1`）かつ `use_cache == false` のカラム。通常のカラム種別（ホーム/ローカル/ハイブリッド/グローバル/リスト/アンテナ/チャンネル/ユーザー等、TQLの単一ソース指定含む）はすべてこれに該当する。
- 対象外（将来課題、実装しない）: TQLで複数ソースを `from` に列挙したカラム、および `from cache` を含むカラム。複数ソースは各ソースごとに独立したページング・枯渇判定（`fill_gap` が既に持つ `cursors`/`done` 構造）が必要で、単一のスカラー境界では表現できないため、今回のスコープからは明確に除外する。
- `fill_gap` / `gap_fill_on_reconnect` / `fillRemainingGap` は対象外。これらは「新しい方向」のギャップを埋めるものであり、今回導入する境界（後述）は「古い方向」の取得済み範囲を表すため、意味的に無関係。今回は `fetch_backfill` のみを変更する。

## 設計: カラム単位のスカラー境界 (`oldest_fetched_id`)

カラムごとに「これより新しい（ID文字列比較で `>`）ノートは、そのカラムの現在のソース・フィルタに対してAPI取得済みで完全」と保証する境界値 `oldest_fetched_id` を1つ持つ。ID比較は既存コード（`n.id.as_str() <= newest_known_id` 等）と同じ、Misskeyのソート可能なID文字列の辞書順比較に倣う。

行が存在しない = 境界未確定（そのカラムでまだ一度もREST取得していない、またはフィルタ変更・カラム削除でリセットされた）。この場合は常にAPI経由（現状と同じ挙動）。

### データモデル

```sql
CREATE TABLE IF NOT EXISTS column_fetch_boundary (
    column_id TEXT PRIMARY KEY,
    oldest_fetched_id TEXT NOT NULL
);
```

`db.rs` の既存マイグレーションパターン（`column_exists` チェック→`CREATE TABLE IF NOT EXISTS`）に倣って追加する。

### 境界の更新・破棄

- **初期セット**: `open_stream_and_fetch`（カラム新規作成/更新/初回取得）で、対象スコープの条件を満たす場合、今回取得した生APIレスポンスの最古IDで境界を新規セットする（無条件上書き）。
- **延長（古い方向へ）**: `fetch_backfill` がキャッシュミスでAPIにフォールバックした場合、取得した生APIレスポンスの最古IDまで境界を延長する。既存の境界より新しい値が来た場合は無視する（単調に古い方向へのみ進む）。
- **破棄**: `clear_column_notes`（カラム削除・`update_column` でのフィルタ変更時に呼ばれる既存関数）で `column_fetch_boundary` の対応行も削除する。
- **間引き連動（重要）**: `prune`（`note_cache.rs` の `delete_matching` を経由する3種の間引き: 経過日数・件数上限・DBサイズ上限）で、削除によって影響を受けたカラム（削除されたノートが `column_note` に属していたカラム）について、境界を「そのカラムに現在残っている最古の `column_note.note_id`」まで引き上げる。該当カラムのキャッシュが全滅した場合は境界行ごと削除する（未確定状態に戻す）。この処理は `delete_matching` 内に実装し、3種の間引き経路すべてで自動的にカバーされるようにする。

### `NoteCacheStore` API追加

```rust
/// カラムの境界(oldest_fetched_id)を取得。未確定ならNone。
pub fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>>;

/// 境界を new_oldest_id で無条件に新規セット/上書きする(初回取得用)。
pub fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>;

/// 境界を new_oldest_id まで延長する(古い方向へのみ、単調性を保証)。
/// 既存値の方が古い(=既に境界がnew_oldest_idより古い)場合は何もしない。
pub fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>;

/// until_id より古いノートをカラムのキャッシュから取得(load_cached の until_id 版)。
/// 新しい順、最大 limit 件。
pub fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>>;
```

`clear_column_notes` は `DELETE FROM column_fetch_boundary WHERE column_id = ?1` を追加する。

`delete_matching` 内の間引き連動処理は、削除対象ノートの一時テーブル（既存の `prune_ids`）から影響を受けた `column_id` の集合を求め（`SELECT DISTINCT column_id FROM column_note WHERE note_id IN (SELECT id FROM prune_ids)`。ただし `column_note` からの削除は同関数内で行われるため、この集合取得は `column_note` を削除する**前**に行う必要がある）、各カラムについて生存最古IDへ境界を引き上げる。

## `commands/column.rs` の変更

### 生の最古IDを取り出すための戻り値拡張

`fetch_and_filter_multi` は現状フィルタ後の `Vec<Note>` のみを返す。境界にはフィルタ前の「APIが実際に返した最古ID」を使う必要がある（フィルタで末尾が弾かれても、そのページ自体は最後まで見ているため）。単一ソース時のみ意味を持つ小さな戻り値拡張を行う:

```rust
struct FilteredFetch {
    notes: Vec<Note>,               // フィルタ後(従来の戻り値と同じ)
    raw_oldest_id: Option<String>,  // 単一ソース時: 生APIレスポンスの最古ID。複数ソース時はNone。
}
```

呼び出し元（`open_stream_and_fetch`, `fetch_backfill`）は `resolved.kinds.len() == 1 && !resolved.use_cache` のときのみ `raw_oldest_id` を使って境界を更新する。

### `fetch_backfill`

```rust
pub async fn fetch_backfill(state, column_id, until_id) -> Result<Vec<Note>> {
    let column = load_column(&state, &column_id)?;
    let resolved = resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?;

    let cache_eligible = resolved.kinds.len() == 1 && !resolved.use_cache;
    if cache_eligible {
        if let Some(boundary) = state.cache.get_fetch_boundary(&column.id)? {
            if until_id.as_str() > boundary.as_str() {
                let cached = state.cache.load_cached_before(&column.id, &until_id, INITIAL_LIMIT)?;
                if cached.len() as u32 >= INITIAL_LIMIT {
                    return Ok(cached);
                }
                // 件数不足 = このページはキャッシュだけで賄いきれない → 通常フローへフォールスルー
            }
        }
    }

    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    state.cache.cache_notes(&column.id, &fetch.notes)?;
    if cache_eligible {
        if let Some(oldest) = fetch.raw_oldest_id {
            let _ = state.cache.extend_fetch_boundary(&column.id, &oldest);
        }
    }
    Ok(fetch.notes)
}
```

キャッシュ+API混在（境界をまたぐページ）の合成は行わない（YAGNI）。キャッシュ件数が `INITIAL_LIMIT` に満たない場合は素直に通常のAPIフルフェッチへフォールバックする。次スクロール時の `until_id` はキャッシュ提供時より古い値になるため、境界へ向かって自然に収束し、境界到達後は通常のAPIパスへ戻る。

### `open_stream_and_fetch`

初期REST取得後、`cache_eligible` の場合のみ `fetch.raw_oldest_id` があれば `set_fetch_boundary` で境界を新規セットする。

## エラーハンドリング

- 境界の読み書き（`get_fetch_boundary` / `set_fetch_boundary` / `extend_fetch_boundary`）のSQLite失敗は `fetch_backfill` 全体を失敗させない。キャッシュ優先を単にスキップしAPIフォールバックへ倒す（`.ok()` 相当で握りつぶす）。境界が失われても常にAPIフォールバックで正しい結果が返るため、ここを厳密にエラー伝播させる必要はない。
- `load_cached_before` のデシリアライズ失敗行は既存の `load_cached` と同様、警告ログを出してスキップする（`deserialize_note_or_warn` を再利用）。

## テスト方針

### `store/note_cache.rs`

- `extend_fetch_boundary` は単調にのみ古い方向へ進む（新しい値を渡しても後退しないこと）
- `set_fetch_boundary` は無条件に上書きすること
- `prune` の3パターン（`keep` 超過・`max_age_days`・`max_size_mb`）それぞれで、影響を受けたカラムの境界が生存最古IDまで正しく引き上がること／該当カラムのキャッシュが全滅した場合は境界行が削除されること
- `clear_column_notes` で境界も削除されること
- `load_cached_before` が `until_id` 境界と `column_id` スコープを正しく守ること

### `commands/column.rs`

- `fetch_backfill`: 境界より新しい範囲を要求した場合、キャッシュのみで賄われAPI呼び出しが発生しないこと（既存のテストダブル/フェイクAPIクライアントパターンで検証）
- 境界より古い範囲・境界未確定・キャッシュ件数不足の3パターンでAPIフォールバックが発生すること
- TQL複数ソース・`from cache` 併用カラムでは常にAPI経由になること（スコープ外の確認）
- `open_stream_and_fetch` の初期取得後に境界が正しくセットされること

## 非対象・将来課題

- TQL複数ソースカラムのキャッシュ優先化: ソースごとの独立したカーソル/枯渇判定（`fill_gap` が既に持つ構造に近いもの）が必要。今回は実装しない。
- `fill_gap` / `gap_fill_on_reconnect` のキャッシュ優先化: 今回導入する境界とは意味的に無関係（「古い方向」ではなく「新しい方向」のギャップ埋めのため）。対象外。
- ミュート設定変更時、既にキャッシュされたページ（変更前のミュート状態でフィルタ済み）が古いままになる問題は、本Issueのスコープ外（既存の挙動を変えない）。
