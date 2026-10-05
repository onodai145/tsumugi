# fetch_backfill の境界延長の競合と、update_column の失敗順序 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `fetch_backfill` の境界延長の計画を、ロックの中で最新の境界から作り、`update_column` の `load_groups` / `host_token` を `invalidate` の前へ動かす(#456 の P5)。

**Architecture:** `commit_backfill_writes` が、`extend: &[(u32, String)]` の代わりに `extend_until: Option<&str>` を受け取り、`write_if_current` のロックの中で `get_fetch_boundaries` → `plan_boundary_extend` → `extend_fetch_boundaries` を行う。`update_column_core` は、読むだけの2つの処理を `invalidate` の前へ移す。

**Tech Stack:** Rust(Tauri v2)、`tokio`、`wiremock`、`tauri::test::mock_app`。

**Spec:** `docs/superpowers/specs/2026-10-05-fetch-backfill-boundary-race-design.md`(実行者は spec も読むこと)

## Global Constraints

- ブランチは `fix/p5-fetch-race-456`(作成済み)。`main` に直接コミットしない。
- コミットメッセージは**件名のみ**(本文なし)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別段落として付ける(`-m` を2回)。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら、リトライせず報告する(GPG 署名のタイムアウトが起きたことがある)。
- Tauri コマンドの署名、TS バインディング、`lib.rs` の `specta_builder()`、DB スキーマ、フロントエンドは変えない。変更は `src-tauri/src/commands/column.rs` の中だけ。
- Rust のテストは `cd src-tauri && cargo test --lib <フィルタ>`。コンパイルが数分かかる。長いコマンドはバックグラウンドで流して出力をファイルに落とし、末尾だけ読む。ディスクの空きが少ない環境なので、`No space left on device` が出たら止めてユーザーに報告する(`target/` の削除は、ユーザーの承認を取ってから)。
- 変異確認では、**変異の前に、テストの追加・修正を必ずコミットする**。変異を戻すときは `git checkout -- <file>` を使うので、未コミットの変更が消える(P3 で起きた事故)。
- `pkill` / `killall` は使わない。ユーザーの実アプリ(`target/debug/tsumugi`、`cargo tauri dev`)のプロセスには触らない。
- clippy の警告数は変更前と同じ(基準: `cargo clippy --all-targets 2>&1 | grep -c "^warning"` が `23`)。増やさない。

## Review Focus

- 境界の行が無いソースは、延長で**挿入されない**(`extend_fetch_boundaries` は行が無ければ挿入するので、計画に入れない責務が `plan_boundary_extend` にある。Task 1 のテスト)。
- 境界を読めなかったとき(DB の失敗)、延長を飛ばし、ノートのキャッシュは成功して `Some(Ok(()))` を返すこと(Task 1 のテスト)。
- 取得中にギャップ埋めが境界を引き上げたとき、`fetch_backfill` の延長が、引き上げた境界を古い方へ広げないこと(Task 1 の並行テスト)。
- 未知のグループ、未登録のアカウントで `update_column_core` が `Err` を返したとき、定義・境界・世代・ストリームのどれも変わらないこと(Task 2 のテスト)。

---

## File Structure

| ファイル | 責任 |
|---|---|
| `src-tauri/src/commands/column.rs` | `commit_backfill_writes` / `fetch_backfill` / `update_column_core` の変更と、そのテスト(`mod tests` は同ファイル内) |

## Task 1: 境界の延長計画を、ロックの中で、最新の境界から作る

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`commit_backfill_writes`、`fetch_backfill`、テスト)

**Interfaces:**
- Consumes: 既存の `ColumnFence::{begin, write_if_current}`、`plan_boundary_extend(prev: &HashMap<u32, String>, until_id: &str, outcomes: &[SourceOutcome]) -> Vec<(u32, String)>`、`NoteCacheStore::{get_fetch_boundaries, extend_fetch_boundaries, replace_fetch_boundaries}`、`fetched(id) -> SourceOutcome`(テストヘルパー)、P3 のテストヘルパー `command_state` / `command_column` / `mount_one_note_page`。
- Produces:
  - `async fn commit_backfill_writes(fence: &ColumnFence, cache: &NoteCacheStore, column_id: &str, epoch: &Epoch, fetch: &FilteredFetch, extend_until: Option<&str>) -> Option<Result<()>>`(最後の引数が `&[(u32, String)]` から `Option<&str>` に変わる)
  - `async fn extend_boundaries_in_lock(cache: &NoteCacheStore, column_id: &str, until_id: &str, outcomes: &[SourceOutcome])`(private)
  - テストヘルパー `fn fetch_with_outcomes(ids: &[&str], outcomes: Vec<SourceOutcome>) -> FilteredFetch`

- [ ] **Step 1: テストヘルパーを足し、既存の `commit_backfill_writes` の呼び出し5か所を新しい引数に直す(`mod tests`)**

`fetch_of` の直後に足す:

```rust
    fn fetch_with_outcomes(ids: &[&str], outcomes: Vec<SourceOutcome>) -> FilteredFetch {
        FilteredFetch { source_outcomes: outcomes, ..fetch_of(ids) }
    }
```

既存の呼び出し(5か所。`commit_backfill_writes_caches_notes_and_extends_boundaries_when_current`、`..._writes_nothing_after_update_column`、`..._leaves_no_orphans_after_close`、`..._caches_notes_but_does_not_resurrect_boundaries_after_set_mute`、`..._extends_boundaries_when_the_epoch_was_begun_after_set_mute`)は、引数が次の形になっている:

```rust
commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), &[pair(0, "n300")])
```

すべて次に置き換える(「連続していて、最古が `n300`」を表す。境界 `n500` に対して `until_id = n600` は連続):

```rust
commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_with_outcomes(&["n400"], vec![fetched("n300")]), Some("n600"))
```

機械的に置換するスクリプト(`commands/column.rs` の `mod tests` のみが対象。改行を挟む形も含む):

```bash
python3 - <<'PYEOF'
import re
p='src-tauri/src/commands/column.rs'
s=open(p).read()
pat=re.compile(r'commit_backfill_writes\(\s*&fence,\s*&cache,\s*"c1",\s*&epoch,\s*&fetch_of\(&\["n400"\]\),\s*&\[pair\(0, "n300"\)\]\s*\)')
new='commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_with_outcomes(&["n400"], vec![fetched("n300")]), Some("n600"))'
s,n=pat.subn(new,s)
print("replaced",n)
assert n==5
open(p,'w').write(s)
PYEOF
```

(`replaced 5` が出ること。`rustfmt` の整形で改行位置が変わっても、挙動は同じ。)

- [ ] **Step 2: 新しいテストを足す(`mod tests` の末尾)**

```rust
    #[tokio::test]
    async fn commit_backfill_writes_does_not_insert_a_boundary_for_a_source_without_a_row() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1"); // 境界の行が、まだ無い(set_mute が捨てた後など)

        let written = commit_backfill_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_with_outcomes(&["n400"], vec![fetched("n300")]),
            Some("n600"),
        )
        .await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1, "ノートは書かれる");
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "行が無いソースは、延長で挿入しない");
    }

    #[tokio::test]
    async fn commit_backfill_writes_skips_the_extension_when_the_boundaries_cannot_be_read() {
        let fence = ColumnFence::default();
        // 境界のテーブルを失った DB。`get_fetch_boundaries` が実際の SQL エラーになる(ノートの表は生きている)
        let conn = crate::store::db::open_cache_in_memory().unwrap();
        conn.execute("DROP TABLE column_source_boundary", []).unwrap();
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(conn));
        let epoch = fence.begin("c1");

        let written = commit_backfill_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_with_outcomes(&["n400"], vec![fetched("n300")]),
            Some("n600"),
        )
        .await;

        assert!(matches!(written, Some(Ok(()))), "境界を読めなくても、ノートのキャッシュは成功する");
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn commit_backfill_writes_plans_the_extension_from_the_boundary_read_inside_the_lock() {
        use std::sync::Arc;
        use tokio::sync::{mpsc, Semaphore};

        let fence = Arc::new(ColumnFence::default());
        let cache = Arc::new(mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n100")]).await.unwrap();
        let fetch_epoch = fence.begin("c1"); // fetch_backfill が、境界 n100 のもとで取得を始めた
        let gap_epoch = fence.begin("c1"); // 並行するギャップ埋め
        let (ready_tx, mut ready_rx) = mpsc::unbounded_channel();
        let gate = Arc::new(Semaphore::new(0));
        let gap_fill = {
            let (fence, cache, gate) = (Arc::clone(&fence), Arc::clone(&cache), Arc::clone(&gate));
            tokio::spawn(async move {
                fence
                    .write_if_current("c1", &gap_epoch, |_| async move {
                        ready_tx.send(()).unwrap();
                        gate.acquire().await.unwrap().forget();
                        // 打ち切られたギャップ埋めが、境界を n350 へ引き上げる(完全と言える範囲を縮める)
                        cache.replace_fetch_boundaries("c1", &[pair(0, "n350")]).await.unwrap();
                    })
                    .await
            })
        };
        ready_rx.recv().await.unwrap(); // ギャップ埋めが、カラムのロックを持った
        let commit = {
            let (fence, cache) = (Arc::clone(&fence), Arc::clone(&cache));
            tokio::spawn(async move {
                commit_backfill_writes(
                    &fence,
                    &cache,
                    "c1",
                    &fetch_epoch,
                    &fetch_with_outcomes(&["n80"], vec![fetched("n60")]),
                    Some("n120"), // 古い境界 n100 に対しては、連続している
                )
                .await
            })
        };
        tokio::task::yield_now().await;
        tokio::task::yield_now().await; // commit が、ロックの待ちに入る
        gate.add_permits(1); // ギャップ埋めが、境界を引き上げて、ロックを放す

        gap_fill.await.unwrap();
        let written = commit.await.unwrap();

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(
            cache.get_fetch_boundaries("c1").await.unwrap(),
            vec![pair(0, "n350")],
            "ロックの中で読んだ最新の境界(n350)に対しては、n120 は連続でない。引き上げた境界を、古い写しで広げない"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_backfill_extends_a_contiguous_boundary() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await; // ノート n9 の1ページ
        let app = tauri::test::mock_app();
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Local)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n95".to_string())]).await.unwrap();
        app.manage(state);

        let notes = fetch_backfill(app.state::<AppState>(), "c1".into(), "n99".into(), true).await.unwrap();

        assert_eq!(notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n9"]);
        let boundaries = app.state::<AppState>().cache.get_fetch_boundaries("c1").await.unwrap();
        assert_eq!(boundaries.len(), 1);
        assert!(
            boundaries[0].1.as_str() < "n95",
            "n99 は境界 n95 と連続しているので、境界が古い方へ延長される: {boundaries:?}"
        );
    }
```

- [ ] **Step 3: コンパイルエラー(引数の型の不一致)で失敗することを確認する**

Run: `cd src-tauri && cargo test --lib commands::column::tests::commit_backfill_writes 2>&1 | grep -E "^error" | sort | uniq -c`
Expected: `error[E0308]: mismatched types`(`expected \`&[(u32, String)]\`, found \`Option<&str>\`)が、更新した呼び出しの数だけ。

- [ ] **Step 4: `commit_backfill_writes` と `fetch_backfill` を実装する**

`commit_backfill_writes`(doc コメントを含む。`async fn commit_backfill_writes(...) -> Option<Result<()>> { ... }` 全体)を、次に置き換える。

```rust
/// `fetch_backfill` の書き込み: 取得ノートのキャッシュと、ソースごとの境界の延長。
/// `epoch` が古ければ(取得中に `update_column` / `close_column` が走った)何も書かず `None` を返す
/// (Issue #446)。境界の延長の失敗は握りつぶす(更新できなくても従来の挙動に戻るだけ)。
///
/// 境界の延長は、**ロックの中で読んだ最新の境界**から計画する(`extend_until` は、キャッシュ対象の
/// カラムで `Some(until_id)`)。取得の前にロックの外で読んだ境界から計画すると、取得中に、同じ世代の
/// ギャップ埋めが境界を新しい方へ引き上げた場合に、その引き上げを古い方へ広げ戻して、未取得の区間を
/// 「完全」と主張してしまう(Issue #456)。
async fn commit_backfill_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    fetch: &FilteredFetch,
    extend_until: Option<&str>,
) -> Option<Result<()>> {
    fence
        .write_if_current(column_id, epoch, |boundaries_ok| async move {
            cache_fetched(cache, column_id, fetch).await?;
            // 境界の世代が古い(取得中に set_mute が境界を捨てた)なら、旧ミュートの結果に基づく
            // 延長を書かない(Issue #452)。
            if boundaries_ok {
                if let Some(until_id) = extend_until {
                    extend_boundaries_in_lock(cache, column_id, until_id, &fetch.source_outcomes).await;
                }
            }
            Ok::<(), Error>(())
        })
        .await
}

/// カラムのロックの中で、最新の境界を読み、`plan_boundary_extend` の計画で延長する。境界を読めなければ、
/// 延長を飛ばす(保守的)。計画に入るのは、既存の行があるソースだけなので、境界の行は挿入されない。
async fn extend_boundaries_in_lock(cache: &NoteCacheStore, column_id: &str, until_id: &str, outcomes: &[SourceOutcome]) {
    let Ok(prev) = cache.get_fetch_boundaries(column_id).await else {
        return;
    };
    let prev: std::collections::HashMap<u32, String> = prev.into_iter().collect();
    let plan = plan_boundary_extend(&prev, until_id, outcomes);
    if !plan.is_empty() {
        let _ = cache.extend_fetch_boundaries(column_id, &plan).await;
    }
}
```

`fetch_backfill` の、取得後の部分を置き換える。置き換え前:

```rust
    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    // ソースごとに、既存の境界と連続している場合のみ延長する(plan_boundary_extend)。
    // 境界未確定のソースは連続性を検証できないので延長せず、カラム開き直し時の
    // open_stream_and_fetch が改めて確定させる。
    let extend = if cache_eligible {
        plan_boundary_extend(&boundaries, &until_id, &fetch.source_outcomes)
    } else {
        vec![]
    };
    // 取得中に update_column / close_column が走っていた(世代が古い)なら、何も書かず空で返す(Issue #446)。
    match commit_backfill_writes(&state.column_fence, &state.cache, &column.id, &epoch, &fetch, &extend).await {
```

置き換え後:

```rust
    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    // 境界の延長は、ソースごとに、既存の境界と連続している場合のみ行う(plan_boundary_extend)。
    // 境界未確定のソースは連続性を検証できないので延長せず、カラム開き直し時の
    // open_stream_and_fetch が改めて確定させる。計画は、ここ(ロックの外)の古い境界ではなく、
    // commit_backfill_writes がロックの中で読む最新の境界から作る(Issue #456)。
    let extend_until = cache_eligible.then_some(until_id.as_str());
    // 取得中に update_column / close_column が走っていた(世代が古い)なら、何も書かず空で返す(Issue #446)。
    match commit_backfill_writes(&state.column_fence, &state.cache, &column.id, &epoch, &fetch, extend_until).await {
```

(取得前に読む `boundaries` は、キャッシュ提供の判断で、引き続き使うので、未使用警告にはならない。)

- [ ] **Step 5: テストを流して、全件が通ることを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -6`
Expected: `test result: ok.`(追加4件を含む。失敗がある場合は、`fetch_backfill_extends_a_contiguous_boundary` で、`n9` が境界 `n95` より小さくならない原因を `superpowers:systematic-debugging` で切り分ける。期待が `source_outcome_from_page` の振る舞いと食い違うときは、`boundaries` の実際の値を確認して、`assert!` の比較を、同じ意図(古い方へ延長された)のまま、実際の値に合わせて直し、台帳に `Ruling:` として残す)。

- [ ] **Step 6: clippy の警告数を確認する**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: `23`。

- [ ] **Step 7: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "fix: fetch_backfillの境界の延長を、ロックの中で読んだ最新の境界から計画する" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8: 変異確認(コミット後。変異のたびに、確認したら必ず元に戻す)**

1. **ロックの外で読む変異**: `commit_backfill_writes` の先頭(`fence.write_if_current` の前)に `let snapshot = cache.get_fetch_boundaries(column_id).await.unwrap_or_default();` を足し、`extend_boundaries_in_lock(cache, column_id, until_id, &fetch.source_outcomes).await;` の呼び出しを、次に置き換える(取得前の写しで計画する、古い実装の再現):

   ```rust
   let prev: std::collections::HashMap<u32, String> = snapshot.into_iter().collect();
   let plan = plan_boundary_extend(&prev, until_id, &fetch.source_outcomes);
   if !plan.is_empty() {
       let _ = cache.extend_fetch_boundaries(column_id, &plan).await;
   }
   ```

   Run: `cd src-tauri && cargo test --lib commit_backfill_writes_plans_the_extension 2>&1 | tail -12`
   Expected: `FAILED`(`引き上げた境界を、古い写しで広げない`。境界が `n60` になる)。戻す: `git checkout -- src-tauri/src/commands/column.rs`。
2. **挿入する変異**: `extend_boundaries_in_lock` の `plan_boundary_extend(&prev, until_id, outcomes)` を、`outcomes` の `Fetched(id)` を無条件に `(i, id)` にした計画(`prev` を無視)に置き換える。Run: `cd src-tauri && cargo test --lib commit_backfill_writes_does_not_insert 2>&1 | tail -12`。Expected: `FAILED`(`行が無いソースは、延長で挿入しない`)。戻す: `git checkout -- src-tauri/src/commands/column.rs`。

`git status --short` が空であること。

## Task 2: `update_column_core` の `load_groups` / `host_token` を `invalidate` の前へ

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`update_column_core`、テスト)

**Interfaces:**
- Consumes: P3 のテストヘルパー `command_state(&MockServer) -> AppState` / `command_column(id, kind) -> Column` / `mount_empty_pages(&MockServer)`、`ColumnFence::{begin, write_if_current}`、`ConnectionManager::open_count()`。
- Produces: なし(挙動の変更のみ。`update_column_core` の署名は変わらない)。

- [ ] **Step 1: 失敗するテストを書く(`mod tests` の末尾)**

```rust
    /// `update_column_core` が `Err` を返したあと、定義・境界・世代・ストリームが変わっていないこと。
    async fn assert_update_left_nothing_changed(state: &AppState, original_kind: &ColumnKind, before: &Epoch) {
        let saved = state.settings.load_columns().unwrap();
        assert_eq!(saved[0].kind, *original_kind, "定義は旧定義のまま");
        assert_eq!(
            state.cache.get_fetch_boundaries("c1").await.unwrap(),
            vec![(0, "n100".to_string())],
            "キャッシュ(境界)は破棄されない"
        );
        let still_current = state.column_fence.write_if_current("c1", before, |_| async {}).await;
        assert!(still_current.is_some(), "世代は進まない(実行中の取得を捨てない)");
        assert_eq!(state.connections.open_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_changes_nothing_when_the_group_is_unknown() {
        let mock = MockServer::start().await;
        mount_empty_pages(&mock).await;
        let state = command_state(&mock);
        let column = Column { group_id: "ghost".into(), ..command_column("c1", ColumnKind::Home) };
        state.settings.upsert_column(&column).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap();
        let before = state.column_fence.begin("c1");
        let app = tauri::test::mock_app();

        let result = update_column_core(
            app.handle(),
            &state,
            "c1".into(),
            ColumnKind::Tag { tag: "foo".into() },
            FilterQuery::Keywords(vec![]),
            None,
        )
        .await;

        assert!(matches!(result, Err(Error::Invalid(_))));
        assert_update_left_nothing_changed(&state, &ColumnKind::Home, &before).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_changes_nothing_when_the_account_is_not_registered() {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory()); // アカウントを登録しない
        state
            .settings
            .upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 400, auto: false })
            .unwrap();
        state.settings.upsert_column(&command_column("c1", ColumnKind::Home)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap();
        let before = state.column_fence.begin("c1");
        let app = tauri::test::mock_app();

        let result = update_column_core(
            app.handle(),
            &state,
            "c1".into(),
            ColumnKind::Tag { tag: "foo".into() },
            FilterQuery::Keywords(vec![]),
            None,
        )
        .await;

        assert!(matches!(result, Err(Error::Invalid(_))));
        assert_update_left_nothing_changed(&state, &ColumnKind::Home, &before).await;
    }
```

- [ ] **Step 2: テストが、想定どおりの理由で失敗することを確認する**

Run: `cd src-tauri && cargo test --lib update_column_core_changes_nothing 2>&1 | grep -E "^test |panicked|定義は旧定義|破棄されない|世代は進まない" | head -8`
Expected: 2件とも `FAILED`。現状は `invalidate` の後で `Err` を返すので、`定義は旧定義のまま`(新定義 `Tag` に変わっている)で落ちる。

- [ ] **Step 3: `update_column_core` の順序を変える**

`update_column_core` の本体で、次の2つのブロックを、`invalidate` の呼び出しの**前**(`column.title = ...` の代入の直後、`let (epoch, cleared) = state.column_fence.invalidate(...)` の前)へ動かす。

```rust
    let group = state
        .settings
        .load_groups()?
        .into_iter()
        .find(|g| g.id == column.group_id)
        .ok_or_else(|| Error::Invalid(format!("unknown group: {}", column.group_id)))?;
    let (host, token) = state.host_token(&column.account_id)?;
```

(元の位置、つまり `cleared?;` の後ろからは、この2つのブロックを**削除**する。`group`、`host`、`token` は、後ろの `open_stream_and_fetch(...)` と `OpenedColumn { ... }` で、そのまま使われる。)

動かす場所に、次のコメントを足す:

```rust
    // どちらも読むだけで、`invalidate` の結果に依存しない(グループとアカウントは、更新で変わらない)。
    // `invalidate` の後に置くと、失敗したときに、新定義が保存され、ストリームが閉じ、キャッシュが消えた後に
    // `Err` を返してしまう(Issue #456)。
```

そして、`update_column_core` の doc コメントに、既知の制限を足す:

```rust
/// 既知の制限(Issue #456。設計は docs/superpowers/specs/2026-10-05-fetch-backfill-boundary-race-design.md):
/// `invalidate` の後の失敗(キャッシュ DB の書き込み、通知カラムの初回取得)は、新定義が保存され、
/// ストリームが閉じたまま `Err` を返す。画面は旧定義のままで、再編集で直る。
```

- [ ] **Step 4: テストを流して、全件が通ることを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -6`
Expected: `test result: ok.`(追加2件を含む。P3 の `update_column_core_*` のテストも通る)。

- [ ] **Step 5: clippy の警告数を確認する**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: `23`。

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "fix: update_columnのグループ検索とトークン取得をinvalidateの前へ移す" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

(この Task は、実装前に失敗するテストを書いて、実装後に通ることを確認する通常の RED→GREEN で、別の変異確認は要らない。)

## 完了後(実行者が行う。ユーザーの承認を取ってから)

- 全体の確認: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED"` と、`git status --short` で `frontend/src/bindings/tauri.gen.ts` に差分が出ていないこと。
- push して PR を作る。本文は `.github/pull_request_template.md` の構成に沿い、関連 Issue は `Refs #456`(自動クローズさせない)。検証欄には、実機・実 UI の確認をしていない旨を書き、既知の制限(`invalidate` の後の失敗は現状のまま)を明記する。
- マージ後、#456 の対応済みの2項目(`fetch_backfill` の境界延長の競合、`update_column` が `invalidate` の後で失敗する件。後者は「`load_groups` / `host_token` を前へ移した。残りの失敗は既知の制限」と注記して)にチェックを入れる。
