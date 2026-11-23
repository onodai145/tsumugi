# TQL `match` の正規表現の事前コンパイル Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** TQL の `match` の右辺がリテラルのとき、正規表現を `CompiledFilter` の生成時に一度だけコンパイルし、ライブ受信のノートごとの `Regex::new` をなくす(Issue #609)。

**Architecture:** `filter/eval.rs` に、リテラルのパターンを集めてコンパイルする `compile_regexes` と、その表を使う `evaluate_with` を足す(`evaluate` は表なしで残す)。`CompiledFilter::Tql` を `{ expr, regexes }` にし、生成を `CompiledFilter::from_expr` の一箇所に集める。`commands/column.rs` の直接構築(921 行)は `from_expr` に、分解 2 箇所(1953・2277 行)は `Tql { expr, .. }` に直す。

**Tech Stack:** Rust(`regex` crate 1.12、既存の依存)。新しい依存は無し。

**Spec:** `docs/superpowers/specs/2026-11-23-issue-609-tql-match-precompile-design.md`

## Global Constraints

- ブランチ `perf/issue-609-tql-match-precompile-regex` の上で作業する(作成済み。`main` では編集・コミットしない)。
- コミットメッセージは**件名 1 行のみ**(本文・箇条書きなし)。形式は `type(scope): 件名`。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら**再試行せず**止めて報告する。
- 評価の結果を変えない。不正なリテラルのパターンは常に不一致(`!` を付けると一致)。
- `evaluate(expr, note, ctx)` のシグネチャは変えない。
- 右辺が動的な `match`(フィールドなど)は、これまでどおりノートごとにコンパイルする。LRU は入れない。
- `filter/sql.rs`・`store/db.rs` の `regexp()`・`filter/mute.rs` は触らない。AST(`filter/ast.rs`)も変えない。
- テストは `cd src-tauri && cargo test --lib <パス>`。`llama.cpp` のビルドが重いので、`CARGO_BUILD_JOBS=4` を付け、`CARGO_TARGET_DIR` を `/tmp` に置かない。
- PR は `Fixes #609` を本文に書く。`.github/pull_request_template.md` の構成に手で合わせる。push 後に CI を待たない。

## Review Focus

- 不正なパターン(`"("`)を `!` で否定したとき、一致すること(事前コンパイルが `None` を「不一致」でなく「エラー」にしてしまわないこと)。Task 2 のテストで固定する。
- 空文字列のパターン(`match ""`)が、これまでどおりすべてのノートに一致すること。Task 1 のテストで固定する。
- 大文字小文字だけが違うパターン(`"A"` と `"a"`)が、表の中で別のエントリになること(キーの正規化をしない)。Task 1 のテストで固定する。
- 表に無いパターンは、その場でコンパイルして同じ結果になること(`evaluate` が表なしで動くこと)。Task 1 のテストで固定する。
- `from … where …` 形式の TQL カラム(`column.rs:921`)でも表が作られること。Task 2 の検証手順(`grep`)と `from_expr` のテストで固定する。

---

### Task 1: `eval.rs` の事前コンパイルと表つき評価

**Files:**
- Modify: `src-tauri/src/filter/eval.rs`(先頭の `use`、`evaluate` まわり、`Match` の分岐、`mod tests`)

**Interfaces:**
- Consumes: 既存の `Expr` / `Value::Str` / `CompareOp::Match`(`filter/ast.rs`)。
- Produces(Task 2 が使う):
  - `pub type RegexTable = HashMap<String, Option<regex::Regex>>;`
  - `pub fn compile_regexes(expr: &Expr) -> RegexTable`
  - `pub fn evaluate_with(expr: &Expr, note: &Note, ctx: &EvalContext, re: &RegexTable) -> bool`
  - `pub fn evaluate(expr: &Expr, note: &Note, ctx: &EvalContext) -> bool`(既存。表なしで評価)

- [ ] **Step 0: 作業ツリーを整理して、spec と plan をコミットする**

作業ツリーには、spec 承認前に書いた試作が残っている(`filter/eval.rs`・`filter/mod.rs`。ビルドが通らない)。plan どおりにテストから書き直すため、捨てる。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git checkout -- src-tauri/src/filter/eval.rs src-tauri/src/filter/mod.rs
git status --short
```

Expected: `docs/superpowers/specs/2026-11-23-issue-609-tql-match-precompile-design.md` と `docs/superpowers/plans/2026-11-23-issue-609-tql-match-precompile.md` の 2 つだけが `??` で出る。

```bash
git add docs/superpowers/specs/2026-11-23-issue-609-tql-match-precompile-design.md docs/superpowers/plans/2026-11-23-issue-609-tql-match-precompile.md
git commit -m "docs(filter): match の正規表現の事前コンパイルの spec と plan を追加する(Issue #609)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

(件名のみ。`-m` の 2 つ目はトレーラーで、本文ではない。)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/filter/eval.rs` の `mod tests` の末尾(`expressions_at_the_depth_limit_evaluate_within_a_small_stack` の後ろ)に追加する。

```rust
    #[test]
    fn compile_regexes_collects_literal_patterns_once() {
        let q = parse(
            "from home where text match \"^a\" || (!(text match \"^a\") && cw_text match \"b$\")",
        )
        .unwrap();
        let table = compile_regexes(q.predicate.as_ref().unwrap());
        // 同じパターンは一つにまとまり、Not / And / Or の下も走査される
        assert_eq!(table.len(), 2);
        assert!(matches!(table.get("^a"), Some(Some(_))));
        assert!(matches!(table.get("b$"), Some(Some(_))));
    }

    #[test]
    fn compile_regexes_keeps_invalid_patterns_as_none_and_case_variants_apart() {
        let q = parse(
            "from home where text match \"(\" || text match \"A\" || text match \"a\"",
        )
        .unwrap();
        let table = compile_regexes(q.predicate.as_ref().unwrap());
        assert_eq!(table.len(), 3);
        assert!(matches!(table.get("("), Some(None))); // 不正なパターンは None
        assert!(table.contains_key("A") && table.contains_key("a")); // 正規化しない
    }

    #[test]
    fn compile_regexes_skips_dynamic_right_hand_sides() {
        let q = parse("from home where text match user.username").unwrap();
        assert!(compile_regexes(q.predicate.as_ref().unwrap()).is_empty());
    }

    #[test]
    fn evaluate_with_uses_the_table_and_falls_back_when_the_pattern_is_missing() {
        let n = base_note(); // text = "hello https://ex.com #rust @bob"
        let q = parse("from home where text match \"^hello\"").unwrap();
        let expr = q.predicate.as_ref().unwrap();

        // 表の値が使われる: 同じパターンを None にした表では不一致になる
        let mut poisoned = RegexTable::new();
        poisoned.insert("^hello".to_string(), None);
        assert!(!evaluate_with(expr, &n, &ctx(), &poisoned));

        // 事前コンパイルした表でも、空の表(その場でコンパイル)でも、`evaluate` と同じ結果
        let table = compile_regexes(expr);
        assert!(evaluate_with(expr, &n, &ctx(), &table));
        assert!(evaluate_with(expr, &n, &ctx(), &RegexTable::new()));
        assert!(evaluate(expr, &n, &ctx()));
    }

    #[test]
    fn empty_literal_pattern_matches_every_note() {
        let n = base_note();
        let q = parse("from home where text match \"\"").unwrap();
        let expr = q.predicate.as_ref().unwrap();
        assert!(evaluate_with(expr, &n, &ctx(), &compile_regexes(expr)));
        assert!(evaluate(expr, &n, &ctx()));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib filter::eval::tests::compile_regexes`
Expected: コンパイルエラー(`cannot find function compile_regexes` / `cannot find type RegexTable`)。

- [ ] **Step 3: 最小の実装を書く**

`src-tauri/src/filter/eval.rs` の先頭の `use std::collections::HashSet;` を置き換える。

```rust
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// `match` の右辺がリテラルのパターンを、事前にコンパイルした結果(Issue #609)。
/// 不正なパターンは `None`(評価は常に不一致)。
pub type RegexTable = HashMap<String, Option<Regex>>;
```

`evaluate` 関数を、次の 4 つに置き換える(`eval_compare` のシグネチャも変わる)。

```rust
/// 式木のうち、右辺がリテラルの `match` のパターンを一度だけコンパイルする。
/// 右辺がフィールドなどの動的な `match` は対象外(ノートごとに値が変わるため)。
pub fn compile_regexes(expr: &Expr) -> RegexTable {
    fn walk(expr: &Expr, table: &mut RegexTable) {
        match expr {
            Expr::Or(a, b) | Expr::And(a, b) => {
                walk(a, table);
                walk(b, table);
            }
            Expr::Not(a) => walk(a, table),
            Expr::Bare(_) => {}
            Expr::Compare { op, rhs, .. } => {
                if let (CompareOp::Match, Value::Str(p)) = (op, rhs) {
                    table
                        .entry(p.clone())
                        .or_insert_with(|| Regex::new(p).ok());
                }
            }
        }
    }
    let mut table = RegexTable::new();
    walk(expr, &mut table);
    table
}

/// predicate が None のクエリは常に true（呼び出し側で扱う）。ここでは Expr を評価。
/// 事前コンパイルなしで評価する(`match` はノートごとにコンパイルする)。
pub fn evaluate(expr: &Expr, note: &Note, ctx: &EvalContext) -> bool {
    evaluate_with(expr, note, ctx, &RegexTable::new())
}

/// `compile_regexes` で作った表を使って評価する。表に無いパターンは、その場でコンパイルする。
pub fn evaluate_with(expr: &Expr, note: &Note, ctx: &EvalContext, re: &RegexTable) -> bool {
    match expr {
        Expr::Or(a, b) => evaluate_with(a, note, ctx, re) || evaluate_with(b, note, ctx, re),
        Expr::And(a, b) => evaluate_with(a, note, ctx, re) && evaluate_with(b, note, ctx, re),
        Expr::Not(a) => !evaluate_with(a, note, ctx, re),
        Expr::Bare(v) => eval_bool(v, note, ctx),
        Expr::Compare { lhs, op, rhs } => eval_compare(lhs, *op, rhs, note, ctx, re),
    }
}

fn eval_match(lhs: &Value, rhs: &Value, n: &Note, ctx: &EvalContext, table: &RegexTable) -> bool {
    if let Value::Str(p) = rhs {
        if let Some(re) = table.get(p) {
            return re
                .as_ref()
                .is_some_and(|re| re.is_match(&eval_str(lhs, n, ctx)));
        }
    }
    Regex::new(&eval_str(rhs, n, ctx))
        .map(|re| re.is_match(&eval_str(lhs, n, ctx)))
        .unwrap_or(false)
}
```

`eval_compare` のシグネチャと `Match` の分岐を変える。

```rust
fn eval_compare(
    lhs: &Value,
    op: CompareOp,
    rhs: &Value,
    n: &Note,
    ctx: &EvalContext,
    re: &RegexTable,
) -> bool {
```

```rust
        Match => eval_match(lhs, rhs, n, ctx, re),
```

(旧: `Match => regex::Regex::new(&eval_str(rhs, n, ctx)).map(...).unwrap_or(false),` の 3 行を置き換える。)

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib filter::`
Expected: PASS(追加の 5 件と、既存の `filter::eval` の `match` のテスト(`(?i)` を含む)・`filter::sql`・`filter::parser` がすべて通る)。`filter/mod.rs` は未変更で、`CompiledFilter::Tql(expr)` が `eval::evaluate` を呼ぶままなのでビルドも通る。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/filter/eval.rs
git commit -m "perf(filter): リテラルの match パターンを事前コンパイルして評価に使えるようにする(Issue #609)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `CompiledFilter::from_expr` と `column.rs` の追従

**Files:**
- Modify: `src-tauri/src/filter/mod.rs`(`CompiledFilter` の定義・`compile`・`matches`・`mod tests`)
- Modify: `src-tauri/src/commands/column.rs:921`・`:1953`・`:2277`

**Interfaces:**
- Consumes(Task 1): `eval::RegexTable`、`eval::compile_regexes(&Expr) -> RegexTable`、`eval::evaluate_with(&Expr, &Note, &EvalContext, &RegexTable) -> bool`。
- Produces:
  - `CompiledFilter::Tql { expr: ast::Expr, regexes: eval::RegexTable }`
  - `CompiledFilter::from_expr(expr: ast::Expr) -> CompiledFilter`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/filter/mod.rs` の `mod tests` の `tql_parse_error_is_reported` の直前に追加する。

```rust
    fn tql_regexes(f: &CompiledFilter) -> &eval::RegexTable {
        match f {
            CompiledFilter::Tql { regexes, .. } => regexes,
            _ => panic!("TQL ではない"),
        }
    }

    #[test]
    fn tql_literal_match_pattern_is_precompiled_once() {
        let ctx = EvalContext::default();
        let f = CompiledFilter::compile(&FilterQuery::Tql(
            "text match \"^ab+c$\" || text match \"^ab+c$\" || text match \"zzz\"".into(),
        ))
        .unwrap();
        // 同じパターンは1つにまとまり、リテラルごとに1回だけコンパイルされる(Issue #609)
        assert_eq!(tql_regexes(&f).len(), 2);
        assert!(f.matches(&note("abbc", 0), &ctx));
        assert!(!f.matches(&note("xyz", 0), &ctx));
    }

    #[test]
    fn tql_invalid_literal_pattern_never_matches_and_negation_does() {
        let ctx = EvalContext::default();
        let f = CompiledFilter::compile(&FilterQuery::Tql("text match \"(\"".into())).unwrap();
        assert!(matches!(tql_regexes(&f).get("("), Some(None)));
        assert!(!f.matches(&note("(", 0), &ctx));
        let not =
            CompiledFilter::compile(&FilterQuery::Tql("!(text match \"(\")".into())).unwrap();
        assert!(not.matches(&note("(", 0), &ctx));
    }

    #[test]
    fn tql_dynamic_match_pattern_is_still_evaluated_per_note() {
        let ctx = EvalContext::default();
        let f = CompiledFilter::compile(&FilterQuery::Tql("text match user.username".into()))
            .unwrap();
        assert!(tql_regexes(&f).is_empty());
        assert!(f.matches(&note("xax", 0), &ctx)); // username は "a"
        assert!(!f.matches(&note("xyz", 0), &ctx));
    }

    #[test]
    fn from_expr_precompiles_like_compile() {
        // `commands/column.rs` の `from … where …` 経路は `compile` を通らず `from_expr` で作る
        let ctx = EvalContext::default();
        let q = parser::parse("from home where text match \"^ab\"").unwrap();
        let f = CompiledFilter::from_expr(q.predicate.unwrap());
        assert_eq!(tql_regexes(&f).len(), 1);
        assert!(f.matches(&note("abc", 0), &ctx));
        assert!(!f.matches(&note("xab", 0), &ctx));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib filter::tests`
Expected: コンパイルエラー(`variant CompiledFilter::Tql does not have a field named regexes`、`no function or associated item named from_expr`)。

- [ ] **Step 3: `mod.rs` を実装する**

`CompiledFilter` の `Tql` バリアントを置き換える。

```rust
    /// TQL の where 述語。`regexes` は右辺がリテラルの `match` を事前コンパイルした表(Issue #609)。
    /// 直接構築せず、`CompiledFilter::from_expr` を使う(事前コンパイルを一箇所に集めるため)。
    Tql {
        expr: ast::Expr,
        regexes: eval::RegexTable,
    },
```

`impl CompiledFilter` に `from_expr` を追加し、`compile` の TQL 分岐と `matches` を直す。

```rust
    /// AST から TQL のフィルタを作る。正規表現の事前コンパイルはここだけで行う。
    pub fn from_expr(expr: ast::Expr) -> Self {
        let regexes = eval::compile_regexes(&expr);
        CompiledFilter::Tql { expr, regexes }
    }
```

```rust
            FilterQuery::Tql(q) => CompiledFilter::from_expr(parser::parse_predicate(q)?),
```

```rust
            CompiledFilter::Tql { expr, regexes } => eval::evaluate_with(expr, note, ctx, regexes),
```

- [ ] **Step 4: `commands/column.rs` を直す**

`column.rs:921`:

```rust
        Some(expr) => CompiledFilter::from_expr(expr),
```

`column.rs:1953`:

```rust
        CompiledFilter::Tql { expr, .. } => sql::build_where(expr, &sql_ctx).map_err(Error::Invalid)?,
```

`column.rs:2277`:

```rust
            CompiledFilter::Tql { expr: e, .. } => Some(e),
```

- [ ] **Step 5: テストが通ることを確認する**

Run: `cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib filter:: commands::`
Expected: PASS。`error[E0164]` / `E0533` が出ないこと(`CompiledFilter::Tql` の取りこぼしが無いこと)。

あわせて、直接構築が残っていないことを確認する。

Run: `grep -rn "CompiledFilter::Tql" src-tauri/src`
Expected: `filter/mod.rs` の定義・`from_expr`・`matches`・テストのヘルパーと、`column.rs` の `Tql { expr, .. }` / `Tql { expr: e, .. }` の 2 箇所だけ。`CompiledFilter::Tql(` や `CompiledFilter::Tql { expr, regexes }` を**構築**している行が `from_expr` 以外に無い。

- [ ] **Step 6: コミットする**

```bash
git add src-tauri/src/filter/mod.rs src-tauri/src/commands/column.rs
git commit -m "perf(filter): CompiledFilter の生成時にリテラルの match を事前コンパイルする(Issue #609)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ドキュメント・全体の検証・PR

**Files:**
- Modify: `CLAUDE.md`(`filter/` の説明の段落)
- Modify: `docs/design/filter-dsl-design.md`(インメモリ評価の節)

**Interfaces:**
- Consumes: Task 1・2 の実装。
- Produces: なし。

- [ ] **Step 1: `CLAUDE.md` に追記する**

`### src-tauri/src layout` の `filter/` の項で、`CompiledFilter`(`filter/mod.rs`)の説明の文 `` `CompiledFilter` (in `filter/mod.rs`) is compiled once per column at creation time to avoid re-parsing per note. `` の直後に、次の英文を足す(この項は英語で書かれている)。

```
Literal `match` patterns are precompiled into `CompiledFilter::Tql { regexes }` at that time (Issue #609), so live evaluation doesn't call `Regex::new` per note; a `match` whose right-hand side is a field is still compiled per note. Always build a TQL filter with `CompiledFilter::from_expr` (never construct `CompiledFilter::Tql` directly) so the table is never skipped — `commands/column.rs` builds `from … where …` columns that way.
```

- [ ] **Step 2: `docs/design/filter-dsl-design.md` に追記する**

インメモリ評価(§10 付近。`eval` の説明の節)の末尾に、一文を足す。

```
右辺がリテラルの `match` は、カラム生成時(`CompiledFilter::from_expr`)に正規表現を一度だけコンパイルして保持する(Issue #609)。右辺がフィールドのときは、ノートごとにコンパイルする。不正なパターンは、どちらの場合も不一致になる。
```

節の位置は `grep -n "インメモリ\|§10\|## 10" docs/design/filter-dsl-design.md` で確かめてから入れる。該当する節が見つからなければ、`match` の評価に触れている箇所(397 行付近の「正規表現(`Match`)」の行)の直後に入れる。

- [ ] **Step 3: 全体を検証する**

```bash
cd src-tauri && CARGO_BUILD_JOBS=4 cargo test --lib
cd ../frontend && pnpm check
```

Expected: `cargo test --lib` がすべて通る(`#[ignore]` は除く)。TS の bindings は変えていないので `pnpm check` は影響を受けないが、念のため通す。`cargo test` が `generates_frontend_bindings` で `frontend/src/bindings/tauri.gen.ts` を書き換えた場合は、差分が無いことを `git status` で確かめる(`CompiledFilter` は specta の型ではないので、差分は出ないはず)。

- [ ] **Step 4: ドキュメントをコミットする**

```bash
git add CLAUDE.md docs/design/filter-dsl-design.md
git commit -m "docs(filter): リテラルの match の事前コンパイルを CLAUDE.md と設計書に反映する(Issue #609)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: push して PR を作る**

```bash
git push -u origin perf/issue-609-tql-match-precompile-regex
```

`gh pr create` は `--body` を使うので、`.github/pull_request_template.md` を読んで、その見出しの構成に手で合わせた本文を作る。本文に `Fixes #609` を入れ、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。CI は待たない(利用者が自分で見る)。
