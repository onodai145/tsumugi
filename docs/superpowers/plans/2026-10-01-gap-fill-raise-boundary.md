# 打ち切られたギャップ埋め後のbackfill境界引き上げ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `fill_gap` が打ち切られたとき、backfill境界を穴より新しい側へ引き上げ、再起動後の復元一覧も有効境界以上に絞ることで、`loadMore` や復元一覧がギャップを黙って飛ばす不具合(Issue #432)を直す。

**Architecture:** `fill_gap` が各ソースの最古の生IDと到達状況(`GapSourceState`)を返す。純粋関数 `plan_boundary_raise_after_gap` が新しい境界一覧を計算し、非同期ヘルパ `apply_gap_fill_boundaries` が既存の `replace_fetch_boundaries` で書き戻す。`resume_column` と `gap_fill_on_reconnect` の両方から呼ぶ。`resume_column` の復元一覧は純粋関数 `restrict_to_boundary` で有効境界以上に絞る。フロント・バインディング・スキーマ・バックエンド trait は変更しない。

**Tech Stack:** Rust (Tauri v2, tokio, rusqlite)

**Spec:** `docs/superpowers/specs/2026-10-01-gap-fill-raise-boundary-design.md`

## Global Constraints

- ブランチは `fix/432-gap-fill-raise-boundary`(作成済み)。`main` へ直接コミットしない。
- コミットメッセージは件名1行のみ。本文・箇条書きを書かない。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` トレーラーを付ける(`git commit -m "$(cat <<'EOF' ... EOF)"` で件名の後に空行を挟んで書く)。`--no-verify` / `--no-gpg-sign` 禁止。`git commit` が失敗/タイムアウトしたら止めてユーザーに報告し、リトライしない。
- 変更は `src-tauri/src/commands/column.rs` だけ。`tauri.gen.ts`、フロント、DBスキーマ、`NoteCacheBackend` trait は変更しない。`cargo test` 後に `frontend/src/bindings/tauri.gen.ts` に差分が出たら、意図しない型変更なので原因を調べる。
- ID比較は辞書順(既存の境界の規約)。`""` は枯渇済みを表す。
- 実機確認をする場合は Xvfb 越し、`WAYLAND_DISPLAY` を unset、`dbus-run-session` 必須。`cargo tauri dev` はリポジトリルートから実行する。自分で起動したプロセスは正確な PID で kill する(`pkill`/`killall` 禁止)。

## Review Focus

- `oldest_fetched` はAPI用カーソル(`cursors[i]`、created_at降順の最後のノート)ではなく、ページ内の**最小ID**で持つこと(境界比較がid基準のため)。Task 2 の実装方針で担保し、レビューで確認する。
- `fill_gap` が `Err` を返した場合は `all_reached: false`・`sources: []` として、境界の全行を落とす(未確定にして API 経由へ)こと。Task 2 で実装。
- `gap_fill_on_reconnect` で収集が0件(`notes.is_empty()`)でも境界の引き上げが先に行われること(早期 return より前に呼ぶ)。Task 2 のコード配置で担保。
- `gap_fill_limit == 0`(ギャップ埋めを無効にした設定)では `fill_gap` 自体が呼ばれず、境界は更新されない。この場合の境界の主張は偽になりうるが、本Issueの対象外(spec のスコープ外)。ユーザーに別Issueとして伝える。
- 復元一覧を絞った結果が空になるとき、既存の「キャッシュが空」経路(`open_stream_and_fetch`)へ入ること。Task 2 の実装(`notes.is_empty()` 判定を絞った後の結果で行う)で担保。
- 境界が未確定(`effective_boundary` が `None`)のカラムでは復元一覧を絞らない(現状維持)こと。Task 1 のテストで固定。

---

### Task 1: 純粋関数と構造体の拡張

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`GapFillResult` 約 830–840 行、`finalize_gap_fill` 約 843–855 行、`backfill_cache_eligible` の直後に関数追加、`mod tests` 内に追加)

**Interfaces:**
- Consumes: 既存の `effective_boundary`、`finalize_gap_fill`
- Produces:
  - `struct GapSourceState { oldest_fetched: Option<String>, reached_target: bool }`(`#[derive(Debug, Clone, PartialEq, Eq)]`)
  - `GapFillResult` に `sources: Vec<GapSourceState>`, `all_reached: bool` を追加
  - `fn plan_boundary_raise_after_gap(prev: &std::collections::HashMap<u32, String>, sources: &[GapSourceState], all_reached: bool, floor: Option<&str>) -> Option<Vec<(u32, String)>>`
  - `fn restrict_to_boundary(cached: Vec<Note>, effective: Option<&str>) -> Vec<Note>`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` 内、`backfill_cache_eligible_requires_streaming_api_sources_and_no_cache_source` の直後(`should_try_backfill_cache` テストがあればその後ろ)に追加する。

```rust
    fn gs(oldest: Option<&str>, reached: bool) -> GapSourceState {
        GapSourceState { oldest_fetched: oldest.map(str::to_string), reached_target: reached }
    }

    fn pairs(entries: &[(u32, &str)]) -> Vec<(u32, String)> {
        entries.iter().map(|(i, b)| (*i, b.to_string())).collect()
    }

    #[test]
    fn plan_boundary_raise_after_gap_none_when_gap_fully_filled() {
        let prev = bmap(&[(0, "n100")]);
        assert_eq!(plan_boundary_raise_after_gap(&prev, &[gs(Some("n050"), true)], true, None), None);
    }

    #[test]
    fn plan_boundary_raise_after_gap_raises_to_max_of_boundary_oldest_fetched_and_floor() {
        let prev = bmap(&[(0, "n100")]);
        // floor(切り詰め後の最古ノート)が最大
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, Some("n350")),
            Some(pairs(&[(0, "n350")]))
        );
        // oldest_fetched が最大(収集0件で floor が無い場合を含む)
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, None),
            Some(pairs(&[(0, "n300")]))
        );
        // 既存境界の方が新しい(大きい)なら変わらない → None
        let prev = bmap(&[(0, "n900")]);
        assert_eq!(plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, Some("n350")), None);
    }

    #[test]
    fn plan_boundary_raise_after_gap_treats_exhausted_boundary_as_smallest() {
        let prev = bmap(&[(0, "")]);
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, None),
            Some(pairs(&[(0, "n300")]))
        );
    }

    #[test]
    fn plan_boundary_raise_after_gap_keeps_reached_sources_and_raises_only_the_truncated_ones() {
        let prev = bmap(&[(0, "n100"), (1, "n200")]);
        let sources = [gs(Some("n500"), false), gs(Some("n150"), true)];
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &sources, false, Some("n450")),
            Some(pairs(&[(0, "n500"), (1, "n200")]))
        );
    }

    #[test]
    fn plan_boundary_raise_after_gap_drops_rows_it_cannot_raise() {
        // 1ページも取れなかったソース(oldest_fetched=None)は未確定にする
        let prev = bmap(&[(0, "n100"), (1, "n200")]);
        let sources = [gs(None, false), gs(Some("n150"), true)];
        assert_eq!(plan_boundary_raise_after_gap(&prev, &sources, false, None), Some(pairs(&[(1, "n200")])));
        // ソース情報が無い(fill_gap が Err)なら全行を落とす
        assert_eq!(plan_boundary_raise_after_gap(&prev, &[], false, None), Some(vec![]));
    }

    #[test]
    fn plan_boundary_raise_after_gap_does_not_create_rows_for_unknown_sources() {
        let prev = bmap(&[(0, "n100")]);
        let sources = [gs(Some("n300"), false), gs(Some("n300"), false)];
        assert_eq!(plan_boundary_raise_after_gap(&prev, &sources, false, None), Some(pairs(&[(0, "n300")])));
        // 境界が空(cache_eligible でないカラム)なら何もしない
        assert_eq!(plan_boundary_raise_after_gap(&bmap(&[]), &sources, false, None), None);
    }

    #[test]
    fn finalize_gap_fill_carries_all_reached_through() {
        assert!(finalize_gap_fill(vec![note("n1", 10)], true, 100).all_reached);
        assert!(!finalize_gap_fill(vec![], false, 100).all_reached);
    }

    #[test]
    fn restrict_to_boundary_keeps_notes_at_or_newer_than_the_effective_boundary() {
        let cached = vec![note("n5", 50), note("n3", 30), note("n1", 10)];
        // 境界未確定なら絞らない
        assert_eq!(restrict_to_boundary(cached.clone(), None).len(), 3);
        // 境界 "n3" 以上だけ残す(fetch_backfill の retain と同じ >=)
        let kept = restrict_to_boundary(cached.clone(), Some("n3"));
        assert_eq!(kept.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n5", "n3"]);
        // 枯渇済み("")は何も落とさない
        assert_eq!(restrict_to_boundary(cached.clone(), Some("")).len(), 3);
        // すべて境界より古ければ空になる
        assert!(restrict_to_boundary(cached, Some("n9")).is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib plan_boundary_raise_after_gap restrict_to_boundary finalize_gap_fill_carries 2>&1 | tail -20`(複数フィルタが使えないので、`cargo test --lib 2>&1 | tail -30` で全体のコンパイルエラーを確認してよい)
Expected: コンパイルエラー `cannot find type GapSourceState` / `cannot find function plan_boundary_raise_after_gap` / `restrict_to_boundary` / `no field all_reached`

- [ ] **Step 3: 実装する**

(a) `GapFillResult` の定義(`/// \`fill_gap\` の結果…` の構造体)に、`GapSourceState` とフィールドを追加する。

```rust
/// `fill_gap` の1ソース分の到達状況。打ち切られたギャップ埋めの後に、境界をどこまで
/// 引き上げるかの計算に使う(Issue #432)。
#[derive(Debug, Clone, PartialEq, Eq)]
struct GapSourceState {
    /// このソースが遡って取得できた最古の生ページのID(ページ内の最小id)。1ページも取れなければ None。
    oldest_fetched: Option<String>,
    /// newest_known_id に追いついた(またはソースが枯渇した)か。
    reached_target: bool,
}
```

`GapFillResult` 本体に次の2フィールドを末尾へ追加する。

```rust
    /// `resolved.kinds` と同じ並びの、ソースごとの到達状況。
    sources: Vec<GapSourceState>,
    /// 全ソースが newest_known_id に追いついたか。偽なら穴が残りうる(収集が0件でも偽になりうる)。
    all_reached: bool,
```

(b) `finalize_gap_fill` の最終行を置き換える(`sources` は `fill_gap` が後から埋める)。

```rust
    GapFillResult { notes: collected, truncated, boundary_id, sources: vec![], all_reached: all_sources_reached_target }
```

(c) `backfill_cache_eligible` の直後に純粋関数を2つ追加する。

```rust
/// 打ち切られたギャップ埋めの後に書き戻す境界の一覧(`replace_fetch_boundaries` に渡す全行)。
/// 変更が無ければ None。`prev` は既存の境界(ソース位置 -> id)。
///
/// 全ソースが追いついた(`all_reached`)なら穴は無いので None。そうでなければ `prev` の各行を、
/// - 追いついたソース: そのまま。
/// - 追いついていないソースで `oldest_fetched` が `Some(o)`: `max(b, o, floor)`。
///   `floor`(`GapFillResult::boundary_id`=切り詰め後の最古ノート)を含めるのは、
///   `finalize_gap_fill` が収集結果を `limit` 件に切り詰めてからキャッシュに書くため、
///   `o` まで遡っていても切り捨てた範囲は `column_note` に無いから。
/// - それ以外(1ページも取れていない、ソース情報が無い): 行を落として未確定にする。
/// `prev` に無いソースは行を作らない。
fn plan_boundary_raise_after_gap(
    prev: &std::collections::HashMap<u32, String>,
    sources: &[GapSourceState],
    all_reached: bool,
    floor: Option<&str>,
) -> Option<Vec<(u32, String)>> {
    if all_reached || prev.is_empty() {
        return None;
    }
    let mut next: Vec<(u32, String)> = prev
        .iter()
        .filter_map(|(i, b)| match sources.get(*i as usize) {
            Some(s) if s.reached_target => Some((*i, b.clone())),
            Some(GapSourceState { oldest_fetched: Some(o), .. }) => {
                let mut v = b.as_str().max(o.as_str());
                if let Some(f) = floor {
                    v = v.max(f);
                }
                Some((*i, v.to_string()))
            }
            _ => None,
        })
        .collect();
    next.sort();
    let mut before: Vec<(u32, String)> = prev.iter().map(|(i, b)| (*i, b.clone())).collect();
    before.sort();
    if next == before {
        None
    } else {
        Some(next)
    }
}

/// 再起動時の復元一覧を、有効境界 `effective` 以上のノートに絞る(Issue #432)。
/// 境界より古い側にはギャップの穴がありうるため、復元一覧が穴をまたがないようにする。
/// 境界未確定(None)のカラムは絞らない。
fn restrict_to_boundary(mut cached: Vec<Note>, effective: Option<&str>) -> Vec<Note> {
    if let Some(e) = effective {
        cached.retain(|n| n.id.as_str() >= e);
    }
    cached
}
```

(d) `fill_gap` の `unwrap_or(GapFillResult {...})`(約 344 行)と早期 return(約 892 行)はまだ編集しない。コンパイルが通らなくなるので、次の Step 4 で一緒に直す。

- [ ] **Step 4: 既存の `GapFillResult` 生成箇所を直してテストを通す**

約 344 行 `resume_column` 内:

old:
```rust
                    .unwrap_or(GapFillResult { notes: vec![], truncated: false, boundary_id: None });
```
new:
```rust
                    .unwrap_or(GapFillResult {
                        notes: vec![],
                        truncated: false,
                        boundary_id: None,
                        sources: vec![],
                        all_reached: false,
                    });
```

`fill_gap` の `resolved.kinds.is_empty()` 早期 return:

old:
```rust
        return Ok(GapFillResult { notes: vec![], truncated: false, boundary_id: None });
```
new:
```rust
        return Ok(GapFillResult {
            notes: vec![],
            truncated: false,
            boundary_id: None,
            sources: vec![],
            all_reached: true,
        });
```

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -15`
Expected: コンパイルが通り、追加した8テストを含む全テストが PASS(`plan_boundary_raise_after_gap_*`、`finalize_gap_fill_carries_all_reached_through`、`restrict_to_boundary_*`)。`fill_gap` はまだ `sources` を埋めない(空のまま)ので Task 2 で配線する。この時点で関数 `plan_boundary_raise_after_gap` / `restrict_to_boundary` は未使用の警告が出てよい(Task 2 で使う)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
feat: ギャップ埋め後の境界引き上げ計算と復元一覧の絞り込み関数を追加 (#432)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `fill_gap` の配線、書き戻しヘルパ、呼び出し側、復元一覧の絞り込み

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`resume_column` 約 296–360 行、`fill_gap` 約 880–952 行、`gap_fill_on_reconnect` 約 1030–1050 行、`mod tests` 内)

**Interfaces:**
- Consumes: Task 1 の `GapSourceState`、`GapFillResult.sources/all_reached`、`plan_boundary_raise_after_gap`、`restrict_to_boundary`、既存の `effective_boundary`、`backfill_cache_eligible`、`NoteCacheStore::{get_fetch_boundaries, replace_fetch_boundaries}`
- Produces:
  - `async fn apply_gap_fill_boundaries(cache: &NoteCacheStore, column_id: &str, gap: &GapFillResult)`
  - `async fn restore_boundary(state: &AppState, column_id: &str, resolved: &ResolvedSources) -> Option<String>`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` 内、Task 1 で追加したテストの後ろに追加する(`cache_with` は既存の補助関数)。

```rust
    fn gap_result(sources: Vec<GapSourceState>, all_reached: bool, boundary_id: Option<&str>) -> GapFillResult {
        GapFillResult {
            notes: vec![],
            truncated: boundary_id.is_some(),
            boundary_id: boundary_id.map(str::to_string),
            sources,
            all_reached,
        }
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_raises_stored_boundaries_when_truncated() {
        let cache = cache_with(&[]).await;
        cache.replace_fetch_boundaries("col1", &pairs(&[(0, "n100"), (1, "n200")])).await.unwrap();

        let gap = gap_result(vec![gs(Some("n300"), false), gs(Some("n150"), true)], false, Some("n350"));
        apply_gap_fill_boundaries(&cache, "col1", &gap).await;

        let mut got = cache.get_fetch_boundaries("col1").await.unwrap();
        got.sort();
        assert_eq!(got, pairs(&[(0, "n350"), (1, "n200")]));
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_leaves_boundaries_when_gap_fully_filled() {
        let cache = cache_with(&[]).await;
        cache.replace_fetch_boundaries("col1", &pairs(&[(0, "n100")])).await.unwrap();

        apply_gap_fill_boundaries(&cache, "col1", &gap_result(vec![gs(Some("n050"), true)], true, None)).await;

        assert_eq!(cache.get_fetch_boundaries("col1").await.unwrap(), pairs(&[(0, "n100")]));
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_is_a_no_op_for_columns_without_boundaries() {
        let cache = cache_with(&[]).await;
        apply_gap_fill_boundaries(&cache, "col1", &gap_result(vec![gs(None, false)], false, None)).await;
        assert!(cache.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_clears_rows_when_fill_gap_errored() {
        // fill_gap が Err のとき resume_column は sources=[] / all_reached=false を渡す
        let cache = cache_with(&[]).await;
        cache.replace_fetch_boundaries("col1", &pairs(&[(0, "n100")])).await.unwrap();

        apply_gap_fill_boundaries(&cache, "col1", &gap_result(vec![], false, None)).await;

        assert!(cache.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib apply_gap_fill_boundaries 2>&1 | tail -15`
Expected: コンパイルエラー `cannot find function apply_gap_fill_boundaries in this scope`

- [ ] **Step 3: 書き戻しヘルパと復元境界ヘルパを実装する**

`plan_boundary_raise_after_gap` / `restrict_to_boundary` の直後に追加する。

```rust
/// 打ち切られたギャップ埋めの結果に応じて、保存済みの境界を引き上げる(Issue #432)。
/// 境界が無い(cache_eligible でない)カラムでは何もしない。DBエラーは握りつぶす
/// (更新できなくても従来の挙動に戻るだけで、他の境界書き込みと同じ扱い)。
async fn apply_gap_fill_boundaries(cache: &NoteCacheStore, column_id: &str, gap: &GapFillResult) {
    let Ok(prev) = cache.get_fetch_boundaries(column_id).await else {
        return;
    };
    let prev: std::collections::HashMap<u32, String> = prev.into_iter().collect();
    if let Some(entries) =
        plan_boundary_raise_after_gap(&prev, &gap.sources, gap.all_reached, gap.boundary_id.as_deref())
    {
        let _ = cache.replace_fetch_boundaries(column_id, &entries).await;
    }
}

/// `resume_column` の復元一覧を絞る有効境界。キャッシュ優先の対象でないカラムや、
/// 境界が未確定のカラムは None(絞らない)。
async fn restore_boundary(state: &AppState, column_id: &str, resolved: &ResolvedSources) -> Option<String> {
    if !backfill_cache_eligible(resolved) {
        return None;
    }
    let boundaries: std::collections::HashMap<u32, String> = state
        .cache
        .get_fetch_boundaries(column_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();
    effective_boundary(&boundaries, resolved.kinds.len())
}
```

- [ ] **Step 4: `fill_gap` に `sources` を配線する**

`fill_gap` 内、`let mut reached_target: Vec<bool> = ...;` の直後に1行足す。

```rust
    // ソースごとに遡れた最古の生ページIDを「ページ内の最小id」で持つ(Issue #432)。境界の比較が
    // id の辞書順のため、API用の cursors(created_at降順の最後)とは別に持つ。
    let mut oldest_fetched: Vec<Option<String>> = vec![None; resolved.kinds.len()];
```

ページ取得ループ内、`any_fetched = true;` の直後(`page.sort_by(...)` の前)に追加する。

```rust
            if let Some(m) = page.iter().map(|n| n.id.as_str()).min() {
                if oldest_fetched[i].as_deref().is_none_or(|o| m < o) {
                    oldest_fetched[i] = Some(m.to_string());
                }
            }
```

(`Option::is_none_or` が使えない Rust バージョンなら `map_or(true, |o| m < o)` にする。)

関数末尾を置き換える。

old:
```rust
    let all_reached = reached_target.iter().all(|r| *r);
    Ok(finalize_gap_fill(collected, all_reached, limit))
```
new:
```rust
    let all_reached = reached_target.iter().all(|r| *r);
    let mut result = finalize_gap_fill(collected, all_reached, limit);
    result.sources = oldest_fetched
        .into_iter()
        .zip(reached_target)
        .map(|(oldest_fetched, reached_target)| GapSourceState { oldest_fetched, reached_target })
        .collect();
    Ok(result)
```

- [ ] **Step 5: 呼び出し側と復元一覧の絞り込みを配線する**

(a) `resume_column` のバックグラウンドタスク内、`fill_gap` の直後に追加し、`notes.is_empty()` の早期 return より**前**に置く。

old:
```rust
                if gap_result.notes.is_empty() {
                    return;
                }
                let _ = state.cache.cache_notes(&column_id, &gap_result.notes).await;
```
new:
```rust
                // 打ち切られた(=穴が残りうる)なら境界を引き上げる。収集が0件でも未取得の範囲は
                // 残るので、空判定の前に行う(Issue #432)。
                apply_gap_fill_boundaries(&state.cache, &column_id, &gap_result).await;
                if gap_result.notes.is_empty() {
                    return;
                }
                let _ = state.cache.cache_notes(&column_id, &gap_result.notes).await;
```

(b) `gap_fill_on_reconnect` も同様。

old:
```rust
    if gap_result.notes.is_empty() {
        return;
    }
    let _ = state.cache.cache_notes(&column.id, &gap_result.notes).await;
```
new:
```rust
    // 打ち切られた(=穴が残りうる)なら境界を引き上げる。収集が0件でも未取得の範囲は
    // 残るので、空判定の前に行う(Issue #432)。
    apply_gap_fill_boundaries(&state.cache, &column.id, &gap_result).await;
    if gap_result.notes.is_empty() {
        return;
    }
    let _ = state.cache.cache_notes(&column.id, &gap_result.notes).await;
```

(c) `resume_column` の復元一覧を絞る。

old:
```rust
        let cached = state.cache.load_cached(&column.id, INITIAL_LIMIT).await?;
        let notes = if cached.is_empty() { vec![] } else { cached };
```
new:
```rust
        let cached = state.cache.load_cached(&column.id, INITIAL_LIMIT).await?;
        // ギャップ埋めが打ち切られて境界が引き上げられていると、境界より古い側には穴がありうる。
        // 復元一覧が穴をまたがないよう、有効境界以上に絞る。結果が空なら下の「キャッシュが空」
        // 経路(REST初回取得)に入る(Issue #432)。
        let restore_e = match resolved.as_ref() {
            Some(r) => restore_boundary(&state, &column.id, r).await,
            None => None,
        };
        let notes = restrict_to_boundary(cached, restore_e.as_deref());
```

- [ ] **Step 6: テストが通ることを確認する**

Run: `cd src-tauri && cargo test 2>&1 | tee /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/fd89c674-63d3-4460-99d4-480bf4f29efb/scratchpad/t432.log | grep -E "test result|FAILED|error\[" ; grep -E "apply_gap_fill_boundaries|plan_boundary_raise|restrict_to_boundary|finalize_gap_fill" /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/fd89c674-63d3-4460-99d4-480bf4f29efb/scratchpad/t432.log`
Expected: 全テスト PASS(Postgres/MySQL/実 Misskey の `#[ignore]` は対象外)。上の grep で追加テスト(Task 1 の8件 + Task 2 の4件)がすべて `ok`。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short`
Expected: 変更は `src-tauri/src/commands/column.rs` のみ(`tauri.gen.ts` に差分が無いこと)。

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -E "column.rs" | head`
Expected: 今回の追加箇所に関する新しい警告なし(既存の別ファイルの警告は無視してよい)。

- [ ] **Step 7: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
fix: 打ち切られたギャップ埋め後にbackfill境界を引き上げ復元一覧を境界で絞る (#432)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: 実機確認(可能なら)と PR

**Files:** なし(確認と PR 作成のみ)

- [ ] **Step 1: 実機確認の可否を判断する**

ギャップ埋めが打ち切られる状況(設定の「起動時のギャップ埋め件数」を小さくして、閉じていた間に多数の投稿が流れる)を、Xvfb + `dbus-run-session` + 使い捨てプロファイルで作れるか確認する。ユーザーの実データ(`~/.cache/com.onodai.tsumugi/cache.db`)には触れない。作れなければ省略し、その旨を PR 本文に書く(確認済みと書かない)。

- [ ] **Step 2: PR を作成する**

`git push -u origin fix/432-gap-fill-raise-boundary` のあと、`.github/pull_request_template.md` の構成に沿って `gh pr create --body-file` で作る。本文に `Fixes #432` を入れ、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。影響範囲は全て未チェック(バインディング・スキーマ・E2E に影響なし)。検証欄に `cargo test` の結果と、実機確認の有無を書く。`gap_fill_limit == 0` の場合が本PRの対象外であること(Review Focus)も本文に書く。マージは指示があるまで行わない。push 後に CI を待たない。
