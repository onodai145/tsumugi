# 起動時の backfill 境界再確定 Implementation Plan (Issue #555)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `resume_column` で、キャッシュ有りでも backfill 境界が確定していないキャッシュ優先対象カラムの初回取得をやり直し、境界を再確定する。

**Architecture:** `resume_column` の本体を `resume_column_core<R: Runtime>(&AppHandle<R>, &AppState, &str)` に切り出してテスト可能にする(`update_column_core` と同じ形)。核の中で「対象カラム && 有効境界なし」を判定し、`reestablish_boundary` が全ソース成功のときだけ、既存の `commit_initial_writes` 経由でキャッシュ・境界・ストリームを反映する。失敗時は従来のキャッシュ優先経路へフォールバックし、世代の不一致だけは `ColumnFence::is_current` で見分けて `Err` を返す。

**Tech Stack:** Rust (Tauri v2, tokio, wiremock, tauri-specta `=2.0.0-rc.25` 固定), Svelte 5 + TypeScript (Backstage の表示 1 行のみ)。

**Spec:** `docs/superpowers/specs/2026-11-03-issue-555-resume-reestablish-backfill-boundary-design.md`

## Global Constraints

- ブランチは `feat/resume-reestablish-backfill-boundary`(作成済み)。`main` では編集もコミットもしない。
- コミットメッセージは**件名 1 行のみ**(本文・箇条書きなし)。形式は `type(scope): 件名 (#555)`。`Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` のトレーラーだけを別に付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら**再試行せず**ユーザーに報告する。
- キャッシュ DB のスキーマは変えない。カラムのキャッシュ(`column_note`)を消さない(`update_column` との違い)。
- `specta` / `specta-typescript` / `tauri-specta` のバージョン固定(`=2.0.0-rc.25` / `=0.0.12`)を緩めない。TS バインディング `frontend/src/bindings/tauri.gen.ts` は手編集せず、`cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- `./target/debug/tsumugi` や `cargo run` を直接実行しない。ユーザーの `cargo tauri dev` はワーキングツリーからホットリビルドされ、未完成の途中状態もユーザーの実データに対して動く。実機での再起動確認は Task 6 完了後にだけ、ユーザーに依頼して行う。
- プロセスの掃除は PID 指定のみ。`pkill` / `killall` を使わない。自分で起動した検証用プロセスはタスク完了前に自分で止める。
- コード内コメント・テスト名の説明は日本語、識別子は英語(既存の流儀)。
- PR は本計画の対象外。push・PR 作成はユーザーの指示を待つ(PR 本文に `Fixes #555`、マージは `--merge`)。

## Review Focus

仕様が含意するが、1 つのタスクの正常系テストだけでは踏まれにくい失敗モード(最も起きやすい順)。各行のテストは、担当するタスクに入れてある。

1. **オフライン起動**(全ソース失敗): キャッシュ有りのカラムが、空表示にならずキャッシュを表示し、ストリームを 1 本だけ開く。(Task 6 `..._keeps_the_cache_when_the_refetch_fails`)
2. **複数ソースの一部だけ失敗**: 取得できた分で表示を置き換えず、何も書かない。(Task 6 `..._keeps_the_cache_when_one_of_several_sources_fails`)
3. **取得中に `update_column` / `close_column` が走る**: `Err` を返し、フォールバックでストリームをリークさせない。(Task 6 `..._returns_an_error_when_the_column_changes_during_the_refetch`)
4. **境界が確定済みのカラム、対象外のカラム**(Tag など Streaming を持たないソース): 起動時に REST を叩かない。(Task 5 `..._serves_the_cache_when_the_boundary_is_set`、Task 6 `..._leaves_a_non_eligible_column_alone`)
5. **古いキャッシュ行が消えない**: 再確定後も既存の `column_note` が残る。(Task 6 `..._re_establishes_the_boundary_for_a_cached_column_without_one`)

---

## File Structure

| ファイル | 変更 | 責務 |
|---|---|---|
| `src-tauri/src/fence.rs` | 修正 | `ColumnFence::is_current`(読み取りだけの世代照会)を追加 |
| `src-tauri/src/state.rs` | 修正 | `CacheMetrics` に `resume_boundary_refetch` を追加 |
| `src-tauri/src/commands/column.rs` | 修正 | `DebugMetrics` の項目追加、`commit_initial_fetch` の切り出し、`resume_column_core` の切り出し、`needs_boundary_refetch` / `reestablish_boundary` の追加、再確定の組み込み |
| `frontend/src/ui/Backstage.svelte` | 修正 | メトリクス表示に 1 行追加 |
| `frontend/src/bindings/tauri.gen.ts` | 再生成 | `DebugMetrics.resumeBoundaryRefetch` |

`column.rs` は既に 4,000 行を超えるが、既存の流儀(同一ファイルに関数とテストを置く)に従い、分割はしない。

---

### Task 1: `ColumnFence::is_current`

**Files:**
- Modify: `src-tauri/src/fence.rs`(`impl ColumnFence` に追加、`mod tests` にテスト追加)

**Interfaces:**
- Consumes: 既存の `Epoch { column: u64, boundary: u64 }`、`ColumnFence::begin` / `invalidate` / `invalidate_boundaries` / `remove`
- Produces: `pub fn is_current(&self, column_id: &str, epoch: &Epoch) -> bool`(Task 6 が使う)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/fence.rs` の `mod tests` の末尾(最後のテストの直後、`}` の前)に追加する。

```rust
    #[tokio::test]
    async fn is_current_is_true_until_the_column_is_invalidated() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");
        assert!(fence.is_current("c1", &epoch));

        fence.invalidate("c1", || async {}).await;

        assert!(!fence.is_current("c1", &epoch), "update_column / close_column の後は古い");
    }

    #[tokio::test]
    async fn is_current_ignores_the_boundary_generation() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");

        fence.invalidate_boundaries(|| async {}).await;

        assert!(fence.is_current("c1", &epoch), "境界の世代はカラムの世代と別。境界が捨てられても現行");
    }

    #[test]
    fn is_current_is_false_for_a_removed_or_unknown_column() {
        let fence = ColumnFence::default();
        let epoch = fence.begin("c1");
        fence.remove("c1");
        assert!(!fence.is_current("c1", &epoch), "remove 済み");

        let other = fence.begin("c2");
        assert!(!fence.is_current("ghost", &other), "エントリの無いカラム");
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib fence::tests::is_current 2>&1 | tail -15`
Expected: コンパイルエラー `no method named 'is_current' found for struct 'ColumnFence'`

- [ ] **Step 3: 最小の実装を書く**

`src-tauri/src/fence.rs` の `remove` メソッドの直前(`/// エントリを消す(` のコメントの前)に追加する。

```rust
    /// `epoch` が、そのカラムの現在の世代と一致するか。ロックを取らない読み取りだけの照会。
    /// エントリが無い(`remove` 済み・未登録)なら偽。書き込みの可否を決めるものではなく(それは
    /// `write_if_current`)、失敗した取得が「定義の変更で捨てられた」のか「それ以外の失敗」なのかを
    /// 見分けるために使う(Issue #555)。境界の世代は見ない。
    pub fn is_current(&self, column_id: &str, epoch: &Epoch) -> bool {
        self.columns
            .lock()
            .unwrap()
            .get(column_id)
            .is_some_and(|entry| entry.epoch.load(Ordering::SeqCst) == epoch.column)
    }

```

- [ ] **Step 4: 通過を確認する**

Run: `cd src-tauri && cargo test --lib fence:: 2>&1 | tail -15`
Expected: `test result: ok.`(既存の fence テストも通る)

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/fence.rs
git commit -m "$(cat <<'EOF'
feat(fence): カラムの世代が現行かを照会するis_currentを追加 (#555)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: メトリクス `resumeBoundaryRefetch`

**Files:**
- Modify: `src-tauri/src/state.rs`(`CacheMetrics` とそのテスト)
- Modify: `src-tauri/src/commands/column.rs`(`DebugMetrics` と `get_debug_metrics`)
- Modify: `frontend/src/ui/Backstage.svelte`
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: 既存の `CacheMetrics`(`AtomicU64` のフィールド群)
- Produces: `CacheMetrics::record_resume_boundary_refetch(&self)`、`CacheMetrics::resume_boundary_refetch(&self) -> i32`(Task 6 が使う)、`DebugMetrics.resume_boundary_refetch: i32`(TS では `resumeBoundaryRefetch`)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/state.rs` の `cache_metrics_record_resume_increments_hit_or_fallback` テストの直後に追加する。

```rust
    #[test]
    fn cache_metrics_record_resume_boundary_refetch_increments_its_own_counter() {
        let m = CacheMetrics::default();
        assert_eq!(m.resume_boundary_refetch(), 0);

        m.record_resume_boundary_refetch();
        m.record_resume_boundary_refetch();

        assert_eq!(m.resume_boundary_refetch(), 2);
        assert_eq!(m.resume_hit(), 0, "resume hit / fallback とは別のカウンタ");
        assert_eq!(m.resume_fallback(), 0);
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib cache_metrics_record_resume_boundary_refetch 2>&1 | tail -10`
Expected: コンパイルエラー `no method named 'resume_boundary_refetch'`

- [ ] **Step 3: `CacheMetrics` に実装する**

`src-tauri/src/state.rs`:

(a) 構造体にフィールドを追加する。

```rust
    resume_hit: AtomicU64,
    resume_fallback: AtomicU64,
    /// 起動時の復元で、境界の無いカラムの初回取得をやり直して境界を再確定した回数(Issue #555)。
    resume_boundary_refetch: AtomicU64,
}
```

(b) `record_resume` の直後にメソッドを追加する。

```rust
    /// 起動時の復元で、境界の無いカラムの初回取得をやり直し、その結果を反映して画面に返した。
    /// 取得に失敗してキャッシュ表示にフォールバックしたときは数えない(そちらは `record_resume(true)`)。
    pub fn record_resume_boundary_refetch(&self) {
        self.resume_boundary_refetch.fetch_add(1, Ordering::Relaxed);
    }
```

(c) `resume_fallback()` の直後にゲッターを追加する。

```rust
    pub fn resume_boundary_refetch(&self) -> i32 {
        self.resume_boundary_refetch
            .load(Ordering::Relaxed)
            .try_into()
            .unwrap_or(i32::MAX)
    }
```

- [ ] **Step 4: `DebugMetrics` に項目を足す**

`src-tauri/src/commands/column.rs`:

```rust
pub struct DebugMetrics {
    pub backfill_cache_hit: i32,
    pub backfill_cache_fallback_boundary: i32,
    pub backfill_cache_fallback_other: i32,
    pub resume_cache_hit: i32,
    pub resume_cache_fallback: i32,
    pub resume_boundary_refetch: i32,
}
```

`get_debug_metrics` の構築側にも 1 行足す。

```rust
        resume_cache_fallback: state.cache_metrics.resume_fallback(),
        resume_boundary_refetch: state.cache_metrics.resume_boundary_refetch(),
    })
```

- [ ] **Step 5: Backstage に表示を足す**

`frontend/src/ui/Backstage.svelte` の `resume fallback` の行の直後に追加する。

```svelte
          <div>resume fallback: {metrics.resumeCacheFallback}</div>
          <div>resume 境界再確定: {metrics.resumeBoundaryRefetch}</div>
```

- [ ] **Step 6: バインディングを再生成して確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings 2>&1 | tail -8 && cd .. && git diff --stat frontend/src/bindings/tauri.gen.ts && git diff frontend/src/bindings/tauri.gen.ts | grep '^[+-]' | grep -v '^+++\|^---'`
Expected: テスト通過。差分は `resumeBoundaryRefetch: number` の追加 1 行だけ。

Run: `cd frontend && pnpm check 2>&1 | tail -6`
Expected: エラー 0

- [ ] **Step 7: 全体の通過を確認してコミット**

Run: `cd src-tauri && cargo test --lib cache_metrics 2>&1 | tail -6`
Expected: `test result: ok.`

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/column.rs frontend/src/ui/Backstage.svelte frontend/src/bindings/tauri.gen.ts
git commit -m "$(cat <<'EOF'
feat(metrics): 起動時の境界再確定の回数をデバッグメトリクスに追加 (#555)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `commit_initial_fetch` の切り出し(挙動を変えない)

`open_stream_and_fetch` の後半(境界の計画 → 書き込みとストリームを開く → ミュート対象の返信先の除去)を、再確定でも使えるよう関数に切り出す。既存のテストが、挙動が変わらないことの保証になる。

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`open_stream_and_fetch` の通知カラム以外の後半)

**Interfaces:**
- Consumes: `FilteredFetch`、`plan_boundary_initial`、`backfill_cache_eligible`、`commit_initial_writes`、`open_streams_only`、`AppState::strip_muted_replies`
- Produces: `async fn commit_initial_fetch<R: Runtime>(app: &AppHandle<R>, state: &AppState, column: &Column, resolved: &ResolvedSources, host: String, token: String, epoch: &Epoch, fetch: FilteredFetch) -> Result<Vec<Note>>`(Task 6 が使う。Err は `commit_initial_writes` の Err をそのまま返す)

- [ ] **Step 1: 現状のテストが通ることを確認する(基準)**

Run: `cd src-tauri && cargo test --lib open_stream_and_fetch 2>&1 | tail -10`
Expected: 4 件 `ok`(`..._hides_a_muted_reply_target_but_caches_the_original` / `..._keeps_a_reply_target_that_is_not_muted` / `..._returns_an_error_and_writes_nothing_when_the_epoch_is_stale` / `..._succeeds_for_a_current_epoch`)

- [ ] **Step 2: 関数を切り出す**

`src-tauri/src/commands/column.rs` の `open_stream_and_fetch` の、通知カラムの早期 return より後ろ(`let resolved = resolved.expect(...)` から末尾の `Ok((notes, vec![]))` まで)を次に置き換える。

```rust
    let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
    let fetch = fetch_and_filter_multi(state, &column.account_id, &resolved, None).await?;
    let notes = commit_initial_fetch(app, state, column, &resolved, host, token, epoch, fetch).await?;
    Ok((notes, vec![]))
}

/// 取得済みの初回ページを、カラムのキャッシュ・境界へ書き、ストリームを開き、画面へ返すノートを返す。
/// `open_stream_and_fetch`(新規・更新・キャッシュが空の復元)と、`reestablish_boundary`(キャッシュ有りで
/// 境界が無い復元, Issue #555)が共有する。書き込みと、ストリームを開く処理を、同じロックの中で行う。
/// 取得中に update_column / close_column が走っていた(世代が古い)なら、何も書かず、ストリームも開かず、
/// Err を返す(Issue #446)。Err のときストリームは開かれていない。
#[allow(clippy::too_many_arguments)]
async fn commit_initial_fetch<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    column: &Column,
    resolved: &ResolvedSources,
    host: String,
    token: String,
    epoch: &Epoch,
    fetch: FilteredFetch,
) -> Result<Vec<Note>> {
    let boundaries = backfill_cache_eligible(resolved).then(|| plan_boundary_initial(&fetch.source_outcomes));
    commit_initial_writes(
        &state.column_fence,
        &state.cache,
        &column.id,
        epoch,
        &fetch,
        boundaries.as_deref(),
        || open_streams_only(app, state, column, resolved, host, token),
    )
    .await?;
    // キャッシュには元の返信先を残し(上で書き終えている)、UI に返すコピーだけ除去する(Issue #486)。
    let mut notes = fetch.notes;
    state.strip_muted_replies(&column.account_id, &mut notes);
    Ok(notes)
}
```

(元の「書き込みと、ストリームを開く処理を、同じロックの中で行う」「キャッシュには元の返信先を残し」のコメントは、上の新しい関数に移したので、`open_stream_and_fetch` 側には残さない。)

- [ ] **Step 3: 挙動が変わっていないことを確認する**

Run: `cd src-tauri && cargo test --lib open_stream_and_fetch 2>&1 | tail -10`
Expected: Step 1 と同じ 4 件が `ok`

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | tail -15`
Expected: 新しい警告なし

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
refactor(column): 初回取得の書き込みとストリーム開始をcommit_initial_fetchに切り出す (#555)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `needs_boundary_refetch`(判定の純関数)

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`should_try_backfill_cache` の近くに関数、`mod tests` にテスト)

**Interfaces:**
- Consumes: なし(純関数)
- Produces: `fn needs_boundary_refetch(cache_eligible: bool, effective: Option<&str>) -> bool`(Task 6 が使う)

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の `cache_backfill_page` 系テストの近く(既存のテストの並びの任意の位置)に追加する。

```rust
    #[test]
    fn needs_boundary_refetch_is_true_only_for_an_eligible_column_without_an_effective_boundary() {
        assert!(needs_boundary_refetch(true, None), "対象 + 境界なし");
        assert!(!needs_boundary_refetch(true, Some("n100")), "境界が確定済み");
        assert!(!needs_boundary_refetch(true, Some("")), "全ソース枯渇済み(\"\")も確定済み");
        assert!(!needs_boundary_refetch(false, None), "対象外(from cache / Streaming なしのソース)は境界を持たない");
        assert!(!needs_boundary_refetch(false, Some("n100")));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib needs_boundary_refetch 2>&1 | tail -8`
Expected: コンパイルエラー `cannot find function 'needs_boundary_refetch'`

- [ ] **Step 3: 実装する**

`fn should_try_backfill_cache` の直後に追加する。

```rust
/// 起動時の復元で、キャッシュが有っても初回取得をやり直して境界を再確定するか(Issue #555)。
/// キャッシュ優先の対象カラム(`backfill_cache_eligible`)なのに、全ソース分そろった有効境界が無いとき真。
/// 境界が無いと `fetch_backfill` は常にAPIへ落ち、境界を新規に作る経路は初回取得だけなので、
/// キャッシュが有るカラムでは放っておくと二度と確定しない。`effective` は `restore_boundary` の戻り値で、
/// 全ソース枯渇済みの `Some("")` は確定済みとして扱う。
fn needs_boundary_refetch(cache_eligible: bool, effective: Option<&str>) -> bool {
    cache_eligible && effective.is_none()
}
```

- [ ] **Step 4: 通過を確認してコミット**

Run: `cd src-tauri && cargo test --lib needs_boundary_refetch 2>&1 | tail -6`
Expected: `test result: ok. 1 passed`

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
feat(column): 境界の再確定が要るカラムの判定を追加 (#555)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

(`needs_boundary_refetch` は Task 6 まで未使用なので、この時点では `dead_code` の警告が出る。Task 6 で解消する。コミット前の `cargo clippy` でこの警告だけが出ることは許容する。)

---

### Task 5: `resume_column_core` の切り出し(挙動を変えない)

`resume_column` はコマンドで `State<'_, AppState>` を取るため直接テストできない。`update_column` が `update_column_core` に本体を持つのと同じ形にし、まず現状の挙動を特性化テストで固定する。

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`resume_column` とそのテスト)

**Interfaces:**
- Consumes: 既存の `begin_and_load_column`、`resolve_sources`、`restore_boundary`、`restrict_to_boundary`、`open_stream_and_fetch`、`open_streams_only`、`fill_gap` ほか
- Produces: `async fn resume_column_core<R: Runtime>(app: &AppHandle<R>, state: &AppState, column_id: &str) -> Result<OpenedColumn>`(Task 6 が使う)

- [ ] **Step 1: 特性化テストを書く**

`mod tests` の `open_stream_and_fetch_succeeds_for_a_current_epoch` の直後に追加する。

```rust
    /// `OpenedColumn` のノートid一覧(順序つき)。
    fn opened_ids(opened: &OpenedColumn) -> Vec<&str> {
        opened.notes.iter().map(|n| n.id.as_str()).collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_serves_the_cache_when_the_boundary_is_set() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await; // 呼ばれたら、n9 が返って気づける
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        state.cache.cache_notes("c1", &[note("n5", 500)]).await.unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n1".to_string())]).await.unwrap();
        let app = tauri::test::mock_app();

        let opened = resume_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(opened_ids(&opened), ["n5"], "境界が確定済みなら、キャッシュをそのまま返す");
        assert_eq!(state.cache_metrics.resume_hit(), 1);
        assert_eq!(state.cache_metrics.resume_fallback(), 0);
        assert_eq!(mock.received_requests().await.unwrap().len(), 0, "起動時にRESTを叩かない");
        assert_eq!(state.connections.open_count(), 1, "ストリームは開く");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_fetches_the_first_page_when_the_cache_is_empty() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        let app = tauri::test::mock_app();

        let opened = resume_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(opened_ids(&opened), ["n9"], "キャッシュが空なら初回取得");
        assert_eq!(state.cache_metrics.resume_fallback(), 1);
        assert_eq!(state.cache_metrics.resume_hit(), 0);
        assert_eq!(
            state.cache.get_fetch_boundaries("c1").await.unwrap(),
            vec![(0, "n9".to_string())],
            "初回取得で境界が確定する"
        );
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib resume_column_core 2>&1 | tail -10`
Expected: コンパイルエラー `cannot find function 'resume_column_core'`

- [ ] **Step 3: 本体を切り出す**

`src-tauri/src/commands/column.rs` の `resume_column`(`/// 永続化済みタブを再開する（起動時の復元）。` から、次の `/// 永続化済みグループ一覧。` の直前まで)を、次に**丸ごと置き換える**。元の本体からの変更は、`State` / `AppHandle` 由来の `&state` / `&app` が `state` / `app`(すでに参照)になる点と、関数名・引数だけ。分岐とコメントは変えない。

```rust
/// 永続化済みタブを再開する（起動時の復元）。
#[tauri::command]
#[specta::specta]
pub async fn resume_column(
    app: AppHandle,
    state: State<'_, AppState>,
    column_id: String,
) -> Result<OpenedColumn> {
    resume_column_core(&app, &state, &column_id).await
}

/// `resume_column` の本体。`State` を引数に取らない形にして、テストできるようにしている
/// (`update_column_core` と同じ狙い)。
async fn resume_column_core<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    column_id: &str,
) -> Result<OpenedColumn> {
    let (epoch, column) = begin_and_load_column(state, column_id)?;
    let group = state
        .settings
        .load_groups()?
        .into_iter()
        .find(|g| g.id == column.group_id)
        .ok_or_else(|| Error::Invalid(format!("unknown group: {}", column.group_id)))?;
    let (host, token) = state.host_token(&column.account_id)?;
    let is_notif = matches!(column.kind, ColumnKind::Notifications);
    let resolved = if is_notif {
        None
    } else {
        Some(resolve_sources(state, &column.account_id, &column.kind, &column.filter).await?)
    };

    // 通知以外はキャッシュ優先で即時表示（空なら REST）
    let notes = if is_notif {
        vec![]
    } else {
        let cached = state.cache.load_cached(&column.id, INITIAL_LIMIT).await?;
        // ギャップ埋めが打ち切られて境界が引き上げられていると、境界より古い側には穴がありうる。
        // 復元一覧が穴をまたがないよう、有効境界以上に絞る。結果が空なら下の「キャッシュが空」
        // 経路(REST初回取得)に入る(Issue #432)。
        let restore_e = match resolved.as_ref() {
            Some(r) => restore_boundary(state, &column.id, r).await,
            None => None,
        };
        let mut notes = restrict_to_boundary(cached, restore_e.as_deref());
        state.cache_metrics.record_resume(!notes.is_empty());
        state.strip_muted_replies(&column.account_id, &mut notes);
        notes
    };

    let (fresh_notes, notifications) = if notes.is_empty() {
        open_stream_and_fetch(app, state, &column, resolved, host, token, &epoch).await?
    } else {
        // キャッシュがある: まずキャッシュを即返して体感速度を維持し、閉じていた間のギャップ埋めは
        // バックグラウンドで行って ColumnGapFill イベントでまとめて反映する（1件ずつ ColumnNote を
        // 出すと新着通知/通知音が誤爆するため、専用イベントで通知ロジックを経由させない）。
        let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
        let gap_limit = state
            .settings
            .load_ui()
            .map(|p| p.gap_fill_limit)
            .unwrap_or(0)
            .max(0);
        let newest_known_id = notes[0].id.clone(); // load_cached は created_at 降順（先頭が最新）
        open_streams_only(app, state, &column, &resolved, host, token);
        if gap_limit > 0 {
            let app2 = app.clone();
            let column_id = column.id.clone();
            let account_id = column.account_id.clone();
            tauri::async_runtime::spawn(async move {
                let Some(state) = app2.try_state::<AppState>() else { return };
                let gap_result = fill_gap(&state, &account_id, &resolved, &newest_known_id, gap_limit)
                    .await
                    .unwrap_or(GapFillResult {
                        notes: vec![],
                        truncated: false,
                        boundary_id: None,
                        sources: vec![],
                        all_reached: false,
                        dropped_floor: None,
                    });
                // 打ち切られた(=穴が残りうる)なら境界を引き上げる。収集が0件でも未取得の範囲は
                // 残るので、空判定の前に行う(Issue #432)。取得中に update_column / close_column が
                // 走っていた(世代が古い)なら、何も書かず、イベントも出さない(Issue #446)。
                if !commit_gap_fill_writes(&state.column_fence, &state.cache, &column_id, &epoch, &gap_result).await {
                    return;
                }
                if gap_result.notes.is_empty() {
                    return;
                }
                // キャッシュへは書き終えている。UI へ出すコピーだけ除去する(Issue #486)。
                let mut gap_notes = gap_result.notes;
                state.strip_muted_replies(&account_id, &mut gap_notes);
                let _ = crate::events::ColumnGapFill {
                    column_id,
                    notes: gap_notes,
                    truncated: gap_result.truncated,
                    boundary_id: gap_result.boundary_id,
                    target_id: if gap_result.truncated {
                        Some(newest_known_id)
                    } else {
                        None
                    },
                }
                .emit(&app2);
            });
        }
        (notes, vec![])
    };

    Ok(OpenedColumn {
        column,
        group,
        notes: fresh_notes,
        notifications,
    })
}
```

(置き換え前に、`git diff` で元の本体と見比べ、上記が元のコードと `&` の有無以外で一致することを確認する。差異があれば元のコードを優先する。)

- [ ] **Step 4: 通過を確認する**

Run: `cd src-tauri && cargo test --lib resume_column_core 2>&1 | tail -10`
Expected: 2 件 `ok`

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -6`
Expected: `test result: ok.`(`#[ignore]` は除く)

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | tail -15`
Expected: 新しい警告なし(`needs_boundary_refetch` の `dead_code` だけは許容)

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
refactor(column): resume_columnの本体をresume_column_coreに切り出して特性化テストを足す (#555)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: キャッシュ有り・境界なしの再確定

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`resume_column_core` の本体、`reestablish_boundary` を新設、テスト追加)

**Interfaces:**
- Consumes: Task 1 の `ColumnFence::is_current`、Task 2 の `CacheMetrics::record_resume_boundary_refetch`、Task 3 の `commit_initial_fetch`、Task 4 の `needs_boundary_refetch`、Task 5 の `resume_column_core`、既存の `fetch_and_filter_multi`、`SourceOutcome`
- Produces: `async fn reestablish_boundary<R: Runtime>(app: &AppHandle<R>, state: &AppState, column: &Column, resolved: &ResolvedSources, host: String, token: String, epoch: &Epoch) -> Result<Option<Vec<Note>>>`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` の Task 5 のテストの直後に追加する。

```rust
    /// ホームとローカルの2ソースを持つTQLカラム。
    fn tql_two_source_column(id: &str) -> Column {
        let mut column = command_column(id, ColumnKind::Tql);
        column.filter = FilterQuery::Tql("from home, local".into());
        column
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_re_establishes_the_boundary_for_a_cached_column_without_one() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        state.cache.cache_notes("c1", &[note("n1", 100)]).await.unwrap(); // キャッシュ有り、境界なし
        let app = tauri::test::mock_app();

        let opened = resume_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(opened_ids(&opened), ["n9"], "取得した新しいページを返す");
        assert_eq!(
            state.cache.get_fetch_boundaries("c1").await.unwrap(),
            vec![(0, "n9".to_string())],
            "境界が確定する"
        );
        let mut cached: Vec<String> =
            state.cache.load_cached("c1", 10).await.unwrap().into_iter().map(|n| n.id).collect();
        cached.sort();
        assert_eq!(cached, ["n1", "n9"], "既存のキャッシュ行は消えない");
        assert_eq!(state.cache_metrics.resume_boundary_refetch(), 1);
        assert_eq!(state.cache_metrics.resume_hit(), 0, "キャッシュ表示ではない");
        assert_eq!(state.cache_metrics.resume_fallback(), 0, "キャッシュが空だったわけでもない");
        assert_eq!(state.connections.open_count(), 1, "ストリームを開く");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_re_establishes_when_only_some_sources_have_a_boundary() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await;
        let state = command_state(&mock);
        let column = tql_two_source_column("c1");
        state.settings.upsert_column(&column).unwrap();
        state.cache.cache_notes("c1", &[note("n1", 100)]).await.unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(1, "n0".to_string())]).await.unwrap(); // source 0 の行が欠ける
        let app = tauri::test::mock_app();

        let opened = resume_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(opened_ids(&opened), ["n9"]);
        let mut boundaries = state.cache.get_fetch_boundaries("c1").await.unwrap();
        boundaries.sort();
        assert_eq!(boundaries, vec![(0, "n9".to_string()), (1, "n9".to_string())], "全ソース分そろう");
        assert_eq!(state.cache_metrics.resume_boundary_refetch(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_keeps_the_cache_when_the_refetch_fails() {
        let mock = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(500)).mount(&mock).await; // オフライン相当
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        state.cache.cache_notes("c1", &[note("n1", 100)]).await.unwrap();
        let app = tauri::test::mock_app();

        let opened = resume_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(opened_ids(&opened), ["n1"], "空表示にせず、キャッシュを返す");
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "境界は未確定のまま");
        assert_eq!(state.cache_metrics.resume_hit(), 1, "キャッシュ表示として数える");
        assert_eq!(state.cache_metrics.resume_boundary_refetch(), 0);
        assert!(!mock.received_requests().await.unwrap().is_empty(), "取得は試みた");
        assert_eq!(state.connections.open_count(), 1, "ストリームはフォールバック側で1回だけ開く");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_keeps_the_cache_when_one_of_several_sources_fails() {
        let mock = MockServer::start().await;
        let page = serde_json::json!([{
            "id": "n9",
            "createdAt": "2026-07-05T12:00:00.000Z",
            "text": "hello",
            "user": { "id": "u1", "username": "alice", "host": null },
            "visibility": "home"
        }]);
        Mock::given(method("POST"))
            .and(wiremock::matchers::path("/notes/timeline"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(wiremock::matchers::path("/notes/local-timeline"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock)
            .await;
        let state = command_state(&mock);
        let column = tql_two_source_column("c1");
        state.settings.upsert_column(&column).unwrap();
        state.cache.cache_notes("c1", &[note("n1", 100)]).await.unwrap();
        let app = tauri::test::mock_app();

        let opened = resume_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(opened_ids(&opened), ["n1"], "部分的な結果で、キャッシュの表示を置き換えない");
        let cached: Vec<String> =
            state.cache.load_cached("c1", 10).await.unwrap().into_iter().map(|n| n.id).collect();
        assert_eq!(cached, ["n1"], "取得できた分も書かない");
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
        assert_eq!(state.cache_metrics.resume_hit(), 1);
        assert_eq!(state.cache_metrics.resume_boundary_refetch(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_returns_an_error_when_the_column_changes_during_the_refetch() {
        let mock = MockServer::start().await;
        let page = serde_json::json!([{
            "id": "n9",
            "createdAt": "2026-07-05T12:00:00.000Z",
            "text": "hello",
            "user": { "id": "u1", "username": "alice", "host": null },
            "visibility": "home"
        }]);
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(page).set_delay(std::time::Duration::from_millis(300)),
            )
            .mount(&mock)
            .await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        state.cache.cache_notes("c1", &[note("n1", 100)]).await.unwrap();
        let app = tauri::test::mock_app();

        let (result, _) = tokio::join!(resume_column_core(app.handle(), &state, "c1"), async {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            state.column_fence.invalidate("c1", || async {}).await; // 取得中に update_column / close_column が走った
        });

        assert!(matches!(result, Err(Error::Invalid(_))), "古ければ、フォールバックせず Err を返す");
        assert_eq!(state.connections.open_count(), 0, "古い定義のストリームを開かない");
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "境界を書かない");
        assert_eq!(state.cache_metrics.resume_boundary_refetch(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_column_core_leaves_a_non_eligible_column_alone() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Tag { tag: "foo".into() }); // Streaming を持たないソース
        state.settings.upsert_column(&column).unwrap();
        state.cache.cache_notes("c1", &[note("n1", 100)]).await.unwrap();
        let app = tauri::test::mock_app();

        let opened = resume_column_core(app.handle(), &state, "c1").await.unwrap();

        assert_eq!(opened_ids(&opened), ["n1"], "対象外のカラムは従来どおりキャッシュを返す");
        assert_eq!(mock.received_requests().await.unwrap().len(), 0, "起動時にRESTを叩かない");
        assert_eq!(state.cache_metrics.resume_hit(), 1);
        assert_eq!(state.cache_metrics.resume_boundary_refetch(), 0);
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib resume_column_core 2>&1 | tail -30`
Expected: Task 5 の 2 件は `ok`。新しい 6 件のうち、次の 4 件が `FAILED`: `re_establishes_*` の 2 件(境界が作られない)、`keeps_the_cache_when_the_refetch_fails`(「取得は試みた」の assert。現状は取得しない)、`returns_an_error_when_the_column_changes_during_the_refetch`(現状は `Ok` を返す)。`keeps_the_cache_when_one_of_several_sources_fails` と `leaves_a_non_eligible_column_alone` は現状でも通る(実装後も通り続けることを守る回帰ガード)。

- [ ] **Step 3: `reestablish_boundary` を実装する**

`commit_initial_fetch` の直後に追加する。

```rust
/// 起動時の復元で、キャッシュ有り・境界なしのカラムの初回取得をやり直し、境界を再確定する(Issue #555)。
///
/// 全ソースの取得が成功したときだけ、キャッシュ・境界・ストリームを反映して `Some(新しいページ)` を返す。
/// 1ソースでも失敗(`SourceOutcome::Failed`)したら、何も書かずに `None` を返す。`fetch_and_filter_multi` は
/// ソース単位の失敗を握りつぶして空の成功を返すので、`Err` だけを見ても、オフラインや一部のソースの
/// 失敗を見分けられない。部分的な結果で、キャッシュの表示を置き換えないためのガード。
/// `Err` は、クライアントを作れない、キャッシュ書き込みに失敗した、世代が古かった、のいずれか。
/// どれもストリームを開く前に失敗するので、呼び出し側はフォールバックしてよい(世代が古い場合を除く)。
#[allow(clippy::too_many_arguments)]
async fn reestablish_boundary<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    column: &Column,
    resolved: &ResolvedSources,
    host: String,
    token: String,
    epoch: &Epoch,
) -> Result<Option<Vec<Note>>> {
    let fetch = fetch_and_filter_multi(state, &column.account_id, resolved, None).await?;
    if fetch.source_outcomes.iter().any(|o| matches!(o, SourceOutcome::Failed)) {
        return Ok(None);
    }
    commit_initial_fetch(app, state, column, resolved, host, token, epoch, fetch).await.map(Some)
}
```

- [ ] **Step 4: `resume_column_core` に組み込む**

`resume_column_core` の、`// 通知以外はキャッシュ優先で即時表示（空なら REST）` から `let (fresh_notes, notifications) = if notes.is_empty() {` の直前までを、次に置き換える。(`let (fresh_notes, notifications) = ...` 以降は変えない。)

```rust
    // 通知以外はキャッシュ優先で即時表示（空なら REST）
    let (mut notes, boundary_unset) = if is_notif {
        (vec![], false)
    } else {
        let cached = state.cache.load_cached(&column.id, INITIAL_LIMIT).await?;
        // ギャップ埋めが打ち切られて境界が引き上げられていると、境界より古い側には穴がありうる。
        // 復元一覧が穴をまたがないよう、有効境界以上に絞る。結果が空なら下の「キャッシュが空」
        // 経路(REST初回取得)に入る(Issue #432)。
        let restore_e = match resolved.as_ref() {
            Some(r) => restore_boundary(state, &column.id, r).await,
            None => None,
        };
        let cache_eligible = resolved.as_ref().is_some_and(backfill_cache_eligible);
        let boundary_unset = needs_boundary_refetch(cache_eligible, restore_e.as_deref());
        (restrict_to_boundary(cached, restore_e.as_deref()), boundary_unset)
    };

    // キャッシュが有っても、境界が確定していないキャッシュ優先対象のカラムは、初回取得をやり直して
    // 境界を再確定する(Issue #555)。境界を新規に作る経路は初回取得だけなので、放っておくと、
    // スクロールバックが常にAPIへ落ちる。取得が全ソース成功にならなければ、従来のキャッシュ優先経路へ戻る
    // (オフラインでも起動できる)。ただし、取得中に update_column / close_column が走った(世代が古い)
    // 場合は、古い定義のストリームを開かないよう、フォールバックせず Err を返す。
    if !notes.is_empty() && boundary_unset {
        let eligible = resolved.as_ref().expect("非通知カラムは resolve_sources 済み");
        match reestablish_boundary(app, state, &column, eligible, host.clone(), token.clone(), &epoch).await {
            Ok(Some(fresh)) => {
                state.cache_metrics.record_resume_boundary_refetch();
                return Ok(OpenedColumn { column, group, notes: fresh, notifications: vec![] });
            }
            Ok(None) => {}
            Err(e) => {
                if !state.column_fence.is_current(&column.id, &epoch) {
                    return Err(e);
                }
                log::warn!("failed to re-establish the backfill boundary of column {}: {e}", column.id);
            }
        }
    }

    if !is_notif {
        state.cache_metrics.record_resume(!notes.is_empty());
        state.strip_muted_replies(&column.account_id, &mut notes);
    }

```

- [ ] **Step 5: 通過を確認する**

Run: `cd src-tauri && cargo test --lib resume_column_core 2>&1 | tail -20`
Expected: 8 件すべて `ok`(Task 5 の 2 件 + 新規 6 件)

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -6`
Expected: `test result: ok.`

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | tail -15`
Expected: 警告なし(Task 4 の `dead_code` も解消している)

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
feat(column): 起動時にキャッシュ有りで境界の無いカラムの境界を再確定する (#555)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: 全体検証と実機確認

**Files:** なし(確認のみ)

- [ ] **Step 1: 全体のテストと静的検査**

Run: `cd src-tauri && cargo test 2>&1 | tail -8`
Expected: `test result: ok.`(`#[ignore]` の実接続・実 DB テストは除く)

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | tail -8`
Expected: 警告なし

Run: `cd frontend && pnpm check 2>&1 | tail -6 && pnpm test 2>&1 | tail -8`
Expected: エラー 0、テスト通過

- [ ] **Step 2: バインディングの差分が項目 1 つだけであることを確認する**

Run: `git diff main --stat -- frontend/src/bindings/tauri.gen.ts && git diff main -- frontend/src/bindings/tauri.gen.ts | grep '^[+-]' | grep -v '^+++\|^---'`
Expected: `resumeBoundaryRefetch: number` の追加 1 行のみ

- [ ] **Step 3: 実機確認の前に、ユーザーへ依頼する**

実データで確認するため、**ユーザーに次を依頼する**(自分で `cargo run` や `target/debug/tsumugi` を起動しない)。

1. 動作中の `cargo tauri dev` を、現在のブランチで再起動してもらう(ホットリビルド済みなら再起動のみ)。再起動時、境界の無いキャッシュ有りカラムの数だけ REST が叩かれ、起動が一時的に遅くなる。
2. 再起動後、Debug Bridge で次を読む(手順は `reference-verify-cache-metrics-via-debug-bridge`)。

Expected:
- `resumeBoundaryRefetch` が、境界の無かった対象カラムの数と一致する(2026-11-03 時点の実データでは、キャッシュ有りの 6〜7 カラムのうち、対象外を除いた数)。
- 読み取り専用コピー(`sqlite3 -readonly ... ".backup"`)の `column_source_boundary` に、対象の全カラム分の行がそろう。
- Hybrid タブ(dev.misskey.omhnc.net, `ffc01f1c`)を上へスクロールすると、`backfillCacheHit` が増え、`notes/hybrid-timeline` のログが増えない。

3. 次にもう一度再起動してもらう。Expected: `resumeBoundaryRefetch` が 0 のまま(再確定が一巡した)。

- [ ] **Step 4: 結果を報告する**

実機確認の結果(メトリクスの値、境界の行数、スクロール時のログ)を、数値つきでユーザーに報告する。期待と違う項目があれば、そのまま報告し、原因の調査に戻る(`superpowers:systematic-debugging`)。Push と PR(本文に `Fixes #555`)は、ユーザーの指示を待つ。
