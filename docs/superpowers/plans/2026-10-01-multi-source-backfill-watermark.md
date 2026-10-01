# 複数ソースbackfillの透かし方式 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 複数ソース+強いフィルタのカラムで、backfill(上スクロール・初回取得)が浅いソースのノートを読み飛ばす不具合(Issue #428)を、各ソースの生ページの最古idのうち最大値(透かし)以上のノートだけを画面へ返す方式で直す。

**Architecture:** `fetch_and_filter_multi` の取得を「1パス分の取得(`fetch_backfill_pass`)」と「パスを回す純粋寄りのループ(`collect_backfill_pages`)」に分ける。ループは、1パスの結果から透かし `c` を計算(`backfill_watermark`)し、`split_display_and_cacheable` に渡して `id >= c` のノートだけを画面用にする。画面用が0件で透かしがあれば `until_id = c` で再取得する(上限5パス、上限到達時は最後の1回だけ透かしなしで返す)。ソース別の取得結果はパス間で `merge_source_outcome` で統合する。フロント・TSバインディング・DBスキーマ・trait は変更しない。

**Tech Stack:** Rust (Tauri v2, tokio)

**Spec:** `docs/superpowers/specs/2026-10-01-multi-source-backfill-watermark-design.md`

## Global Constraints

- ブランチは `fix/428-multi-source-backfill-watermark`(作成済み)。`main` へ直接コミットしない。
- コミットメッセージは件名1行のみ。本文・箇条書きを書かない。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` トレーラーを付ける(`git commit -m "$(cat <<'EOF' ... EOF)"` で件名の後に空行を挟んで書く)。`--no-verify` / `--no-gpg-sign` 禁止。`git commit` が失敗/タイムアウトしたら止めてユーザーに報告し、リトライしない。
- 変更は `src-tauri/src/commands/column.rs` だけ。`tauri.gen.ts`、フロント、DBスキーマ、`NoteCacheBackend` trait は変更しない。`cargo test` 後に `frontend/src/bindings/tauri.gen.ts` に差分が出たら原因を調べる。
- ID比較は辞書順(既存の境界の規約)。`until_id` は「それより古いノートを返す」(Misskey の `untilId`)。
- 定数 `BACKFILL_MAX_PASSES = 5`。
- 実機確認をする場合は Xvfb 越し、`WAYLAND_DISPLAY` を unset、`dbus-run-session` 必須。`cargo tauri dev` はリポジトリルートから実行する。自分で起動したプロセスは正確な PID で kill する(`pkill`/`killall` 禁止)。

## Review Focus

- 再取得(2パス目以降)の `until_id` が前パスの透かし `c` であること。画面用ノートが全て `until_id` より古い(=フロントの追記のみの `loadMore` と整合する)こと。Task 2 のテストで固定。
- パス間の `source_outcomes` のマージ: 最初のパスで `Failed` のソースは `Failed` のまま(先頭から連続して取得できていないので境界に使えない)。Task 1 / Task 2 のテストで固定。
- `fetch_backfill` の `plan_boundary_extend`(元の `until_id` との連続性)と `open_stream_and_fetch` の `plan_boundary_initial` に、再取得を挟んだ後のソース別最古idが渡ること。Task 3 の配線(戻り値の `source_outcomes`)で担保し、レビューで確認する。
- 初回取得(`until_id = None`)でも透かしが効き、初回の画面用ノートが従来より少なくなりうること。仕様どおりで、境界(`E = max(b_i)` 以上は全部キャッシュ済み)と整合する。
- `from cache` を含むカラム: `use_cache` では `cacheable` が画面用と同じ。キャッシュ検索結果が `INITIAL_LIMIT` 件に達したときだけ透かしに含める。Task 1 / Task 2 のテストで固定。
- 上限パス数に達して画面用が0件のときの「最後の1回だけ透かしなし」。Task 2 のテストで固定。
- API呼び出し回数が最悪 `BACKFILL_MAX_PASSES × ソース数` に増える。通常は1パス。レビューで確認する(テストでは固定しない)。

---

### Task 1: 純粋関数(透かし・パス間マージ・画面用の選択)

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`split_display_and_cacheable` 約 1399 行、`SourceOutcome` の近くに追加、`mod tests` 内)

**Interfaces:**
- Consumes: 既存の `SourceOutcome`、`INITIAL_LIMIT`、`Note`
- Produces:
  - `fn backfill_watermark(outcomes: &[SourceOutcome], cache_min: Option<&str>) -> Option<String>`
  - `fn merge_source_outcome(prev: SourceOutcome, next: SourceOutcome) -> SourceOutcome`
  - `fn split_display_and_cacheable(filtered: Vec<Note>, use_cache: bool, watermark: Option<&str>) -> (Vec<Note>, Vec<Note>)`(引数を1つ追加)

- [ ] **Step 1: 失敗するテストを書く**

(a) 既存テスト4か所の呼び出しに第3引数 `None` を足す。`mod tests` 内の次の呼び出しをすべて置換する(`split_display_and_cacheable(X, Y)` → `split_display_and_cacheable(X, Y, None)`)。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
sed -i -E 's/split_display_and_cacheable\((with_dup\.clone\(\), false|with_dup, true|all, false)\)/split_display_and_cacheable(\1, None)/' src-tauri/src/commands/column.rs
grep -n "split_display_and_cacheable(" src-tauri/src/commands/column.rs
```
Expected: `mod tests` 内の4か所が `, None)` で終わる(本体の定義は `fn split_display_and_cacheable(mut filtered: ...` のまま、この時点ではまだ2引数)。

(b) `mod tests` 内、`split_display_and_cacheable_dedupes_sorts_and_truncates_only_the_display` の直後に追加する。

```rust
    #[test]
    fn split_display_and_cacheable_hides_notes_older_than_the_watermark_from_the_display_only() {
        let all: Vec<Note> = (1..=10).map(|i| note(&format!("n{i:02}"), i as i64)).collect();

        // 透かし n06: 画面用は n06 以上だけ(n10..n06)。cacheable は全件(10件)。
        let (display, cacheable) = split_display_and_cacheable(all.clone(), false, Some("n06"));
        assert_eq!(display.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n10", "n09", "n08", "n07", "n06"]);
        assert_eq!(cacheable.len(), 10);

        // `from cache` を含むカラムは cacheable が画面用と同じ
        let (display, cacheable) = split_display_and_cacheable(all.clone(), true, Some("n06"));
        assert_eq!(cacheable.len(), display.len());
        assert_eq!(display.len(), 5);

        // 透かしより新しいノートが無ければ画面用は空
        assert!(split_display_and_cacheable(all, false, Some("n99")).0.is_empty());
    }

    #[test]
    fn backfill_watermark_is_the_newest_of_the_per_source_oldest_ids() {
        // 最も浅い(=最古idが最も新しい)ソースの深さ
        assert_eq!(
            backfill_watermark(&[fetched("n500"), fetched("n200")], None),
            Some("n500".to_string())
        );
        // 枯渇済み・失敗したソースは制約しない
        assert_eq!(
            backfill_watermark(&[SourceOutcome::Exhausted, fetched("n200"), SourceOutcome::Failed], None),
            Some("n200".to_string())
        );
        // 制約するソースが無ければ None
        assert_eq!(backfill_watermark(&[SourceOutcome::Exhausted, SourceOutcome::Failed], None), None);
        assert_eq!(backfill_watermark(&[], None), None);
        // `from cache` の検索結果(INITIAL_LIMIT件に達したときの最小id)も1ソースとして含める
        assert_eq!(backfill_watermark(&[fetched("n200")], Some("n600")), Some("n600".to_string()));
        assert_eq!(backfill_watermark(&[], Some("n600")), Some("n600".to_string()));
    }

    #[test]
    fn merge_source_outcome_keeps_the_contiguous_per_source_coverage() {
        use SourceOutcome::{Exhausted, Failed};
        // 最初のパスで失敗したソースは、先頭から連続して取得できていないので Failed のまま
        assert_eq!(merge_source_outcome(Failed, fetched("n100")), Failed);
        assert_eq!(merge_source_outcome(Failed, Exhausted), Failed);
        // 枯渇済みはそれ以降も枯渇済み
        assert_eq!(merge_source_outcome(Exhausted, fetched("n100")), Exhausted);
        // 取得成功同士は、より古い(小さい)idへ
        assert_eq!(merge_source_outcome(fetched("n500"), fetched("n400")), fetched("n400"));
        assert_eq!(merge_source_outcome(fetched("n300"), fetched("n400")), fetched("n300"));
        // 後続パスで枯渇したら枯渇済み
        assert_eq!(merge_source_outcome(fetched("n500"), Exhausted), Exhausted);
        // 後続パスの失敗は、それまでに取れた範囲を保つ
        assert_eq!(merge_source_outcome(fetched("n500"), Failed), fetched("n500"));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error" | sort | uniq -c`
Expected: コンパイルエラー `cannot find function backfill_watermark` / `merge_source_outcome`、`split_display_and_cacheable` の引数の数が合わない(E0061)

- [ ] **Step 3: 実装する**

(a) `SourceOutcome` の定義の直後に追加する。

```rust
/// 複数パスにまたがる1ソースの取得結果を統合する(Issue #428)。ソースごとの結果は、
/// 最初のパスから連続して取得できた範囲を表す必要がある(backfill境界の前提)。
/// - 最初のパスで失敗(`Failed`)したソースは、その後取得できても先頭から連続していないので `Failed`。
/// - `Exhausted` は以降も `Exhausted`。
/// - 取得成功(`Fetched`)同士はより古い(小さい)idへ。後続で `Exhausted` なら `Exhausted`。
/// - 後続パスの失敗は、それまでに取れた範囲を保つ。
fn merge_source_outcome(prev: SourceOutcome, next: SourceOutcome) -> SourceOutcome {
    use SourceOutcome::{Exhausted, Failed, Fetched};
    match (prev, next) {
        (Failed, _) => Failed,
        (Exhausted, _) => Exhausted,
        (Fetched(_), Exhausted) => Exhausted,
        (Fetched(a), Fetched(b)) => Fetched(a.min(b)),
        (Fetched(a), Failed) => Fetched(a),
    }
}

/// 1パス分の取得結果から、画面へ返してよい最古のid(透かし)を決める(Issue #428)。
/// 各ソースの生ページの最古idのうち最大(=最も浅いソースの深さ)。`Exhausted`/`Failed` の
/// ソースは制約しない。`cache_min`(`from cache` の検索結果が `INITIAL_LIMIT` 件に達したときの
/// その最小id)も1ソースとして含める。制約するソースが無ければ None(全ノートを返してよい)。
///
/// 画面へ返すノートを透かし以上に限ると、フロントが次の `until_id` に使う「表示中の最古」は
/// 透かし以上になる。次回は全ソースが `until_id` から取り直すので、浅いソースの区間が飛ばされない。
fn backfill_watermark(outcomes: &[SourceOutcome], cache_min: Option<&str>) -> Option<String> {
    outcomes
        .iter()
        .filter_map(|o| match o {
            SourceOutcome::Fetched(id) => Some(id.as_str()),
            _ => None,
        })
        .chain(cache_min)
        .max()
        .map(str::to_string)
}
```

(b) `split_display_and_cacheable` を置き換える。

old:
```rust
/// 重複除去・created_at降順ソート済みのフィルタ通過ノートから、画面へ返す分と
/// キャッシュする分を決める。詳細は `FilteredFetch::cacheable` を参照。
fn split_display_and_cacheable(mut filtered: Vec<Note>, use_cache: bool) -> (Vec<Note>, Vec<Note>) {
    // 複数ソースに同じノートが跨る場合の重複除去 + created_at 降順ソート
    let mut seen = std::collections::HashSet::new();
    filtered.retain(|n| seen.insert(n.id.clone()));
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    let display: Vec<Note> = filtered.iter().take(INITIAL_LIMIT as usize).cloned().collect();
```
new:
```rust
/// 重複除去・created_at降順ソート済みのフィルタ通過ノートから、画面へ返す分と
/// キャッシュする分を決める。詳細は `FilteredFetch::cacheable` を参照。
/// `watermark`(Some)のときは、画面へ返す分を `id >= watermark` のノートだけに限る
/// (`backfill_watermark`)。`cacheable` には透かしより古いノートも入る(Issue #428)。
fn split_display_and_cacheable(
    mut filtered: Vec<Note>,
    use_cache: bool,
    watermark: Option<&str>,
) -> (Vec<Note>, Vec<Note>) {
    // 複数ソースに同じノートが跨る場合の重複除去 + created_at 降順ソート
    let mut seen = std::collections::HashSet::new();
    filtered.retain(|n| seen.insert(n.id.clone()));
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    let display: Vec<Note> = filtered
        .iter()
        .filter(|n| watermark.is_none_or(|w| n.id.as_str() >= w))
        .take(INITIAL_LIMIT as usize)
        .cloned()
        .collect();
```
(`let cacheable = ...` 以降はそのまま。)

(c) 唯一の本体の呼び出し元 `fetch_and_filter_multi` 内を、この Task の間だけコンパイルが通るように直す。

old:
```rust
    let (notes, cacheable) = split_display_and_cacheable(filtered, resolved.use_cache);
```
new:
```rust
    let (notes, cacheable) = split_display_and_cacheable(filtered, resolved.use_cache, None);
```
(Task 3 で `fetch_and_filter_multi` ごと書き換わる。)

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|test result|FAILED|watermark|merge_source_outcome"`
Expected: 全テスト PASS(追加3件が `ok`)。`backfill_watermark` / `merge_source_outcome` は未使用の警告が出てよい(Task 2 で使う)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
feat: backfillの透かし計算とパス間の取得結果マージ関数を追加 (#428)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: パスを回す `collect_backfill_pages`

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(定数、`BackfillPass`、`collect_backfill_pages` を `FilteredFetch` の近くに追加、`mod tests` 内)

**Interfaces:**
- Consumes: Task 1 の `backfill_watermark`、`merge_source_outcome`、`split_display_and_cacheable(.., watermark)`、既存の `FilteredFetch`、`SourceOutcome`
- Produces:
  - `const BACKFILL_MAX_PASSES: u32 = 5;`
  - `struct BackfillPass { notes: Vec<Note>, outcomes: Vec<SourceOutcome>, cache_min: Option<String> }`
  - `async fn collect_backfill_pages<F, Fut>(until_id: Option<&str>, use_cache: bool, fetch_pass: F) -> Result<FilteredFetch> where F: FnMut(Option<String>) -> Fut, Fut: std::future::Future<Output = Result<BackfillPass>>`

- [ ] **Step 1: 失敗するテストを書く**

`mod tests` 内、Task 1 のテストの後に追加する。

```rust
    fn bp(notes: &[(&str, i64)], outcomes: Vec<SourceOutcome>, cache_min: Option<&str>) -> BackfillPass {
        BackfillPass {
            notes: notes.iter().map(|(id, c)| note(id, *c)).collect(),
            outcomes,
            cache_min: cache_min.map(str::to_string),
        }
    }

    fn ids(notes: &[Note]) -> Vec<&str> {
        notes.iter().map(|n| n.id.as_str()).collect()
    }

    /// issue #428 のシナリオ。密なソース A はフィルタで全て落ち、疎なソース B のノートだけが残る。
    /// A の生ページは n500 まで、B の生ページは n200 まで届く。透かし n500 より古い B のノートは
    /// 画面へ返さない(返すと次の until_id が n500 より古くなり、A の (until_id, n500) が飛ばされる)。
    #[tokio::test]
    async fn collect_backfill_pages_hides_notes_older_than_the_shallowest_source_depth() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let mut script = std::collections::VecDeque::from(vec![bp(
            &[("n600", 600), ("n550", 550), ("n300", 300), ("n250", 250)],
            vec![fetched("n500"), fetched("n200")],
            None,
        )]);

        let fetch = collect_backfill_pages(Some("n900"), false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(ids(&fetch.notes), vec!["n600", "n550"]);
        // 透かしより古いノートもキャッシュには入る(境界は生ページ全体が column_note にある前提)
        assert_eq!(fetch.cacheable.len(), 4);
        assert_eq!(*calls.borrow(), vec![Some("n900".to_string())]);
        assert_eq!(fetch.source_outcomes, vec![fetched("n500"), fetched("n200")]);
    }

    #[tokio::test]
    async fn collect_backfill_pages_refetches_from_the_watermark_when_nothing_is_left_to_show() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let mut script = std::collections::VecDeque::from(vec![
            // 1パス目: 返せるノートは全て透かし n500 より古い → 画面用は空
            bp(&[("n300", 300), ("n250", 250)], vec![fetched("n500"), fetched("n250")], None),
            // 2パス目(until_id = n500): 透かし n400 以上のノートがある
            bp(&[("n480", 480), ("n470", 470)], vec![fetched("n400"), fetched("n300")], None),
        ]);

        let fetch = collect_backfill_pages(Some("n900"), false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(*calls.borrow(), vec![Some("n900".to_string()), Some("n500".to_string())]);
        assert_eq!(ids(&fetch.notes), vec!["n480", "n470"]);
        // cacheable は全パスの和集合
        assert_eq!(fetch.cacheable.len(), 4);
        // ソース別の取得結果はパス間で統合される(より古い方、先頭から連続)
        assert_eq!(fetch.source_outcomes, vec![fetched("n400"), fetched("n250")]);
    }

    #[tokio::test]
    async fn collect_backfill_pages_does_not_refetch_when_every_source_is_exhausted() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let mut script = std::collections::VecDeque::from(vec![bp(
            &[],
            vec![SourceOutcome::Exhausted, SourceOutcome::Exhausted],
            None,
        )]);

        let fetch = collect_backfill_pages(None, false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert!(fetch.notes.is_empty());
        assert_eq!(calls.borrow().len(), 1);
    }

    #[tokio::test]
    async fn collect_backfill_pages_gives_up_at_the_pass_limit_and_returns_the_last_pass_without_a_watermark() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        // どのパスも透かし n500 より古いノートしか返らない
        let mut script: std::collections::VecDeque<BackfillPass> = (0..BACKFILL_MAX_PASSES)
            .map(|_| bp(&[("n100", 100)], vec![fetched("n500"), fetched("n100")], None))
            .collect();

        let fetch = collect_backfill_pages(Some("n900"), false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(calls.borrow().len(), BACKFILL_MAX_PASSES as usize);
        // スクロールが止まらないよう、最後の1回だけ透かしなしで返す(従来の挙動)
        assert_eq!(ids(&fetch.notes), vec!["n100"]);
    }

    #[tokio::test]
    async fn collect_backfill_pages_keeps_a_source_that_failed_on_the_first_pass_failed() {
        let mut script = std::collections::VecDeque::from(vec![
            bp(&[("n300", 300)], vec![SourceOutcome::Failed, fetched("n500")], None),
            bp(&[("n460", 460)], vec![fetched("n450"), fetched("n400")], None),
        ]);

        let fetch = collect_backfill_pages(Some("n900"), false, |_| {
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(ids(&fetch.notes), vec!["n460"]);
        // 先頭から連続して取得できていないソースは境界に使わせない
        assert_eq!(fetch.source_outcomes, vec![SourceOutcome::Failed, fetched("n400")]);
    }

    #[tokio::test]
    async fn collect_backfill_pages_treats_a_full_cache_search_page_as_a_source_for_the_watermark() {
        let mut script = std::collections::VecDeque::from(vec![bp(
            &[("n700", 700), ("n300", 300)],
            vec![fetched("n200")],
            Some("n600"),
        )]);

        let fetch = collect_backfill_pages(None, true, |_| {
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(ids(&fetch.notes), vec!["n700"]);
        // `from cache` を含むカラムは cacheable が画面用と同じ
        assert_eq!(ids(&fetch.cacheable), vec!["n700"]);
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error" | sort | uniq -c`
Expected: コンパイルエラー `cannot find struct BackfillPass` / `cannot find function collect_backfill_pages` / `cannot find value BACKFILL_MAX_PASSES`

- [ ] **Step 3: 実装する**

(a) 定数。`const GAP_FILL_MAX_PAGES: u32 = 10;` の直後に追加する。

```rust
/// `collect_backfill_pages` の最大パス数(Issue #428)。画面へ返すノートが0件のときの内部再取得の上限。
const BACKFILL_MAX_PASSES: u32 = 5;
```

(b) `FilteredFetch` の定義の直後に型と関数を追加する。

```rust
/// `collect_backfill_pages` が1パス分の取得として受け取る結果(Issue #428)。
struct BackfillPass {
    /// フィルタ/ミュート適用済みのノート(重複除去前)。
    notes: Vec<Note>,
    /// `resolved.kinds` と同じ並びの、ソースごとの取得結果。
    outcomes: Vec<SourceOutcome>,
    /// `from cache` の検索結果が `INITIAL_LIMIT` 件に達したときのその最小id。
    cache_min: Option<String>,
}

/// 1パス分の取得を `fetch_pass` で繰り返し、透かし以上のノートだけを画面用として集める(Issue #428)。
///
/// 各パスで `backfill_watermark` を計算し、`split_display_and_cacheable` に渡す。画面用が0件で
/// 透かしがあれば(=まだ深く取れるソースがある)、`until_id = 透かし` で取り直す。これが無いと、
/// 浅いソースの生ページが全てフィルタで落ち他ソースのノートが全て透かしより古い場合に、
/// フロントが同じ `until_id` を繰り返してそのカラムが遡れなくなる。
/// `BACKFILL_MAX_PASSES` に達しても0件なら、スクロールが止まらないよう最後の1パスだけ透かしなしで返す。
/// `cacheable` は全パスの和集合、`source_outcomes` はパス間で `merge_source_outcome` により統合する。
async fn collect_backfill_pages<F, Fut>(
    until_id: Option<&str>,
    use_cache: bool,
    mut fetch_pass: F,
) -> Result<FilteredFetch>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<BackfillPass>>,
{
    let mut until = until_id.map(str::to_string);
    let mut all_cacheable: Vec<Note> = Vec::new();
    let mut merged_outcomes: Option<Vec<SourceOutcome>> = None;
    let mut pass_no: u32 = 0;
    let notes = loop {
        pass_no += 1;
        let pass = fetch_pass(until.clone()).await?;
        merged_outcomes = Some(match merged_outcomes.take() {
            None => pass.outcomes.clone(),
            Some(prev) => prev
                .into_iter()
                .zip(pass.outcomes.iter().cloned())
                .map(|(p, n)| merge_source_outcome(p, n))
                .collect(),
        });
        let watermark = backfill_watermark(&pass.outcomes, pass.cache_min.as_deref());
        let is_last = pass_no >= BACKFILL_MAX_PASSES;
        let fallback_notes = if is_last { Some(pass.notes.clone()) } else { None };
        let (mut display, mut cacheable) = split_display_and_cacheable(pass.notes, use_cache, watermark.as_deref());
        if display.is_empty() && watermark.is_some() && is_last {
            if let Some(notes) = fallback_notes {
                (display, cacheable) = split_display_and_cacheable(notes, use_cache, None);
            }
        }
        all_cacheable.extend(cacheable);
        if !display.is_empty() || watermark.is_none() || is_last {
            break display;
        }
        until = watermark;
    };

    // 全パスの和集合(id 重複除去、created_at 降順)。1パスで終わった場合は split の結果と同じ。
    let mut seen = std::collections::HashSet::new();
    all_cacheable.retain(|n| seen.insert(n.id.clone()));
    all_cacheable.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    Ok(FilteredFetch {
        notes,
        cacheable: all_cacheable,
        source_outcomes: merged_outcomes.unwrap_or_default(),
    })
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib collect_backfill_pages 2>&1 | tail -15`
Expected: 追加6件が PASS(`collect_backfill_pages_*`)。

Run: `cd src-tauri && cargo test --lib 2>&1 | grep -E "^error|test result|FAILED"`
Expected: 全テスト PASS。`collect_backfill_pages` / `BackfillPass` は未使用の警告が出てよい(Task 3 で使う)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
feat: 透かし以上のノートだけを返すbackfillのパスループを追加 (#428)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `fetch_and_filter_multi` の配線

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`fetch_and_filter_multi` 約 1481–1540 行)

**Interfaces:**
- Consumes: Task 2 の `collect_backfill_pages`、`BackfillPass`
- Produces:
  - `async fn fetch_backfill_pass(state: &AppState, account_id: &str, resolved: &ResolvedSources, until_id: Option<&str>) -> Result<BackfillPass>`(1パス分の取得: 各ソースの REST 取得、`from cache` 検索、フィルタ/ミュート適用)
  - `fetch_and_filter_multi` の署名・戻り値は変更しない(`fetch_backfill` / `open_stream_and_fetch` は無変更)

- [ ] **Step 1: 配線する**

`fetch_and_filter_multi` の本体を次のように置き換える(ドキュメントコメントは更新し、署名はそのまま)。

old(関数全体、`async fn fetch_and_filter_multi(` から閉じ括弧まで)を、次に置き換える。

```rust
/// 解決済みソース群から REST 初期/過去ページを取得し、id重複除去+created_at降順マージの上、
/// フィルタ/ミュートを適用する。`cache` ソースが含まれる場合はローカルSQLite検索も合成する。
/// 個別ソースの取得失敗は他ソースの結果を活かすため無視する（TQL§複数ソースは OR 合成のため）。
/// ただし失敗は `source_outcomes` に `Failed` として残し、backfill境界を進めないようにする。
/// 画面へ返すのは、各ソースの生ページの最古idのうち最大(透かし)以上のノートだけで、必要なら
/// 内部で取り直す(`collect_backfill_pages`, Issue #428)。
async fn fetch_and_filter_multi(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<FilteredFetch> {
    collect_backfill_pages(until_id, resolved.use_cache, |until| async move {
        fetch_backfill_pass(state, account_id, resolved, until.as_deref()).await
    })
    .await
}

/// `fetch_and_filter_multi` の1パス分の取得(Issue #428)。各ソースから `until_id` より古い
/// 生ページを取得し、`cache` ソースがあればローカル検索も合成して、フィルタ/ミュートを適用する。
async fn fetch_backfill_pass(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<BackfillPass> {
    let mut all: Vec<Note> = Vec::new();
    let mut outcomes: Vec<SourceOutcome> = Vec::with_capacity(resolved.kinds.len());

    if !resolved.kinds.is_empty() {
        let client = state.client_for(account_id)?;
        for k in &resolved.kinds {
            let outcome = match k.rest_request(INITIAL_LIMIT, until_id) {
                None => SourceOutcome::Failed,
                Some((endpoint, body)) => match fetch_notes(&client, endpoint, &body).await {
                    Ok(raw) => {
                        let outcome = source_outcome_from_page(&raw);
                        all.extend(raw);
                        outcome
                    }
                    Err(_) => SourceOutcome::Failed,
                },
            };
            outcomes.push(outcome);
        }
    }

    let mut cache_min: Option<String> = None;
    if resolved.use_cache {
        let sql_ctx = sql::SqlCtx {
            my_ids: state.eval_context().my_user_ids.into_iter().collect(),
            following_ids: None,
        };
        let expr = match &resolved.filter {
            CompiledFilter::Tql(e) => Some(e),
            _ => None,
        };
        let where_sql = match expr {
            Some(e) => sql::build_where(e, &sql_ctx).map_err(Error::Invalid)?,
            None => sql::SqlWhere { sql: "1=1".into(), params: vec![] },
        };
        if let Ok(cached) = state.cache.search_cache(&where_sql, until_id, INITIAL_LIMIT).await {
            // 検索結果が上限に達したときだけ、その最小idを透かしの候補にする(未満なら枯渇扱い)。
            if cached.len() as u32 >= INITIAL_LIMIT {
                cache_min = cached.iter().map(|n| n.id.clone()).min();
            }
            all.extend(cached);
        }
    }

    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let notes: Vec<Note> = all
        .into_iter()
        .filter(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
                && !state.is_word_muted(account_id, n)
        })
        .collect();

    Ok(BackfillPass { notes, outcomes, cache_min })
}
```

Task 1 で入れた一時的な `split_display_and_cacheable(filtered, resolved.use_cache, None)` は、この置き換えで消える。

- [ ] **Step 2: ビルドと全テストを確認する**

Run: `cd src-tauri && cargo test 2>&1 | tee /tmp/claude-1000/-home-onodai145-repos-github-com-onodai145-tsumugi/fd89c674-63d3-4460-99d4-480bf4f29efb/scratchpad/t428.log | grep -E "^error|test result|FAILED"`
Expected: コンパイルが通り、全テスト PASS(Postgres/MySQL/実 Misskey の `#[ignore]` は対象外)。`BackfillPass` / `collect_backfill_pages` の未使用警告が消えている。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi && git status --short`
Expected: 変更は `src-tauri/src/commands/column.rs` のみ(`tauri.gen.ts` に差分が無い)。

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -B1 -A6 "column.rs" | head -30`
Expected: 今回の追加箇所に関する新しい警告なし(既存の `vec![...; 0]` の警告は無視してよい)。

- [ ] **Step 3: コミットする**

```bash
git add src-tauri/src/commands/column.rs
git commit -m "$(cat <<'EOF'
fix: 複数ソースbackfillで透かし以上のノートだけを返し読み飛ばしを防ぐ (#428)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: 実機確認(可能なら)と PR

**Files:** なし(確認と PR 作成のみ)

- [ ] **Step 1: 実機確認の可否を判断する**

密なソース(例: ローカル TL)と疎なソース(例: 特定ユーザー/ハッシュタグ)を `from` に並べ、強いフィルタを掛けたカラムで遡り、抜けが無いことを確認できるか検討する。Xvfb + `dbus-run-session` + 使い捨てプロファイル、ユーザーの実データには触れない。実Misskeyの用意が難しければ省略し、その旨を PR 本文に書く(確認済みと書かない)。

- [ ] **Step 2: PR を作成する**

`git push -u origin fix/428-multi-source-backfill-watermark` のあと、`.github/pull_request_template.md` の構成に沿って `gh pr create --body-file` で作る。本文に `Fixes #428` を入れ、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。影響範囲は全て未チェック(バインディング・スキーマ・E2E に影響なし)。検証欄に `cargo test` の結果と実機確認の有無を書く。限界(最悪のAPI呼び出し回数、上限到達時の従来挙動、`Failed` ソース)も書く。マージは指示があるまで行わない。push 後に CI を待たない。
