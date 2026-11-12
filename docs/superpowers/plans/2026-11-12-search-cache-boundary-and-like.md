# ノートキャッシュ検索のページ境界と部分一致 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `search_cache` のページ境界を ID 順に揃え、TQL の部分一致(`->` / `startswith` / `endswith`)を、3 backend で同じ結果(実文字・大文字小文字を区別)にする。

**Architecture:** `filter/sql.rs` の `build_where(expr, ctx)` は同じシグネチャのまま、SQLite / PostgreSQL / MySQL の3方言ぶんの SQL とバインド値(`Rendered`)をまとめた `SqlWhere` を返す。各 backend は `render(Dialect)` で自分の分を選ぶ(PostgreSQL の `to_postgres_sql` は廃止)。部分一致は `LIKE` をやめ、方言ごとの関数で書く。`search_cache` は `ORDER BY n.id DESC` / `n.id < ?` に変え、`search_cache_core` は並べ替えず ID 降順のまま返す。3 backend 共通の「契約テスト」(`store/search_contract.rs`)を SQLite の通常テストと、PostgreSQL / MySQL の `#[ignore]` テストの両方から呼ぶ。

**Tech Stack:** Rust(rusqlite / sqlx)、cargo test、Docker(testcontainers、実 DB テスト)

**Spec:** `docs/superpowers/specs/2026-11-12-search-cache-boundary-and-like-design.md`(Issue #583)

## Global Constraints

- `search_cache` / `NoteCacheBackend::search_cache` のシグネチャは**変えない**(`where_sql: &SqlWhere, until_id: Option<&str>, limit: u32`)。
- `build_where(expr: &Expr, ctx: &SqlCtx) -> Result<SqlWhere, String>` のシグネチャは**変えない**。
- 部分一致は大文字小文字を**区別する**(インメモリ評価 `filter/eval.rs` の Rust の `contains` / `starts_with` / `ends_with` と一致。eval 側は変更しない)。
- 文字列マッチの検索語は SQL に埋め込まず、必ずバインドする(`AssertSqlSafe` の前提: `where_sql` の SQL 文字列に値を入れない)。
- `REGEXP` の出力は現状と同じ(SQLite / MySQL は `col REGEXP ?`、PostgreSQL は `col ~ ?`)。意味の差は #598 で扱い、今回は触らない。
- `load_cached` / `load_cached_before`(`column_note` 経由)は触らない。
- コミットメッセージは Conventional Commits の**件名1行のみ**(本文・箇条書きなし)。Co-Authored-By トレーラーは別途付ける。`--no-verify` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告し、リトライしない。
- 作業ブランチ: `fix/search-cache-boundary-and-like-583`(作成済み)。`main` では編集しない。
- Rust のテストは `cd src-tauri && cargo test ...`。実 DB テストは Docker が必要で、MySQL は `--test-threads=2`(並列を上げるとコンテナが一斉に立ち上がって OOM になる)。
- `cargo tauri dev` / `./target/debug/tsumugi` は起動しない(今回のテストは `cargo test` だけで完結する)。

## Review Focus

仕様が暗黙に求めているが、通常のテストでは見落としやすい入力。各行のテストは、担当タスクに入っている。

1. **検索語が `%` / `_` / `\` / `!` を含む**: 実文字として一致する(ワイルドカードにならない)。→ Task 1 の契約テスト
2. **空の検索語、列より長い検索語**: 空は全件に一致、長い検索語の `startswith` / `endswith` は偽。SQLite の `substr(X, -0)` は全体を返すので末尾一致の開始位置は `length(col) - length(?) + 1` で計算する。→ Task 1 の契約テスト
3. **多バイト文字・絵文字・大文字小文字**: 日本語と絵文字の部分一致、`Hello` と `hello` は別物。→ Task 1 の契約テスト
4. **ID と `created_at` の順序が逆転したノート**: ページ送りで欠落も重複もしない。存在しない ID をカーソルにしても、その ID より小さいものが続く。→ Task 2 の契約テスト
5. **PostgreSQL の `$n` 番号**: 検索語を2回バインドする式(`startswith` / `endswith`)と、`and_raw` で後から足した条件(MCP のスコープ)の後でも番号がずれない。→ Task 1 の単体テスト、Task 3 の実 DB テスト

---

## File Structure

| ファイル | 役割 |
| --- | --- |
| `src-tauri/src/filter/sql.rs` | **変更**。`Dialect` / `Rendered` / `SqlWhere`(3方言ぶんを保持)、方言ごとの `Builder`、文字列マッチの生成 |
| `src-tauri/src/store/search_contract.rs` | **新規**(`#[cfg(test)]`)。3 backend 共通の契約テスト(`text_match_contract` / `paging_contract`) |
| `src-tauri/src/store/mod.rs` | `search_contract` の登録 |
| `src-tauri/src/store/note_cache.rs` | `search_cache_sync`(SQLite)を `render` + ID 順に。`search_cache` のドキュメント更新 |
| `src-tauri/src/store/sqlite_backend.rs` | `SqlWhere` の複製を `clone()` に。契約テストの呼び出し |
| `src-tauri/src/store/postgres_backend.rs` | `to_postgres_sql` 廃止、`render(Postgres)` + ID 順。契約テストの呼び出し(`#[ignore]`) |
| `src-tauri/src/store/mysql_backend.rs` | `render(MySql)` + ID 順。契約テストの呼び出し(`#[ignore]`) |
| `src-tauri/src/commands/column.rs` | 手組みの `SqlWhere` を `always_true()` に。`search_cache_core` の並べ替えをやめる |
| `src-tauri/src/mcp/query.rs` | `with_scope` / `build_get` / 手組みの `SqlWhere` を新 API に。テスト追従、冒頭コメント更新 |
| `src-tauri/src/mcp/tools.rs` | `search_notes` の説明文の `newest first` を実態に合わせる |
| `docs/design/filter-dsl-design.md` | §10 の表と LIKE のエスケープの記述 |
| `CLAUDE.md` | `filter/` の項に方言ごとの生成を1文 |

---

### Task 1: SQL の方言ごとの生成と部分一致(契約テスト付き)

`build_where` が3方言ぶんを生成し、各 backend が `render` で選ぶようにする。部分一致を `LIKE` から方言ごとの関数に置き換える。この Task では並び順・境界は従来のまま(`created_at` 順)。

**Files:**
- Modify: `src-tauri/src/filter/sql.rs`
- Create: `src-tauri/src/store/search_contract.rs`
- Modify: `src-tauri/src/store/mod.rs`
- Modify: `src-tauri/src/store/note_cache.rs:602-640`(`search_cache_sync`)
- Modify: `src-tauri/src/store/sqlite_backend.rs:354-370`(`search_cache`)と、tests に契約テストを追加
- Modify: `src-tauri/src/store/postgres_backend.rs:1144-1198`(`to_postgres_sql` / `search_cache_impl`)
- Modify: `src-tauri/src/store/mysql_backend.rs:1139-1170`(`search_cache_impl`)
- Modify: `src-tauri/src/commands/column.rs:1950`, `:2276`
- Modify: `src-tauri/src/mcp/query.rs`(`with_scope`、`:142`、`build_get`、冒頭コメント、tests)

**Interfaces:**
- Produces(`filter::sql`):
  - `pub enum Dialect { Sqlite, Postgres, MySql }`(`Debug, Clone, Copy, PartialEq, Eq`)
  - `pub struct Rendered { pub sql: String, pub params: Vec<SqlParam> }`(`Debug, Clone, PartialEq`)
  - `pub struct SqlWhere`(`Debug, Clone, PartialEq`。フィールドは非公開)
  - `SqlWhere::render(&self, d: Dialect) -> &Rendered`
  - `SqlWhere::raw(sql: &str, params: Vec<SqlParam>) -> SqlWhere`(3方言で同じ意味になる SQL。`?` プレースホルダ。リテラル内に `?` を書かない)
  - `SqlWhere::always_true() -> SqlWhere`(`raw("1=1", vec![])`)
  - `SqlWhere::and_raw(self, sql: &str, params: Vec<SqlParam>) -> SqlWhere`(`({base}) AND ({sql})` に連結。PostgreSQL は `?` を既存のバインド数の続きの `$n` に振り直す)
- Produces(`store::search_contract`、`#[cfg(test)]`): `pub(crate) async fn text_match_contract(b: &dyn NoteCacheBackend)`
- Consumes: `NoteCacheBackend::search_cache(&self, &SqlWhere, Option<&str>, u32) -> Result<Vec<Note>>`(既存)、`crate::mcp::testutil::public_note(id: &str, created_at: i64, text: &str) -> Note`(既存)

- [ ] **Step 1: `sql.rs` の既存テストを新 API の形に書き換え、新しい単体テストを足す(失敗するテスト)**

`src-tauri/src/filter/sql.rs` の `mod tests` を次のように置き換える(`ctx()` と `where_of()` は残す)。`w.sql` / `w.params` の直接参照をやめ、`sq(&w)`(SQLite 方言の `Rendered`)経由にする。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::parser::parse;

    fn ctx() -> SqlCtx {
        SqlCtx {
            my_ids: vec!["me1".into()],
            following_ids: Some(vec!["f1".into()]),
        }
    }

    fn where_of(query: &str) -> SqlWhere {
        let q = parse(query).unwrap();
        build_where(q.predicate.as_ref().unwrap(), &ctx()).unwrap()
    }

    /// SQLite 方言の出力(既存テストの比較対象)。
    fn sq(w: &SqlWhere) -> &Rendered {
        w.render(Dialect::Sqlite)
    }

    fn text(s: &str) -> SqlParam {
        SqlParam::Text(s.into())
    }

    fn cmp(lhs: Value, op: CompareOp, rhs: Value) -> Expr {
        Expr::Compare { lhs, op, rhs }
    }

    #[test]
    fn boolean_and_numeric() {
        let w = where_of("from cache where has_files && !cw");
        assert_eq!(sq(&w).sql, "(n.files_count > 0 AND (NOT n.cw IS NOT NULL))");
        assert!(sq(&w).params.is_empty());

        let w = where_of("from cache where reactions >= 10");
        assert_eq!(sq(&w).sql, "(n.reaction_count >= ?)");
        assert_eq!(sq(&w).params, vec![SqlParam::Real(10.0)]);
    }

    #[test]
    fn arith_and_string() {
        let w = where_of("from cache where reactions + renotes > 5");
        assert_eq!(sq(&w).sql, "((n.reaction_count + n.renote_count) > ?)");
    }

    #[test]
    fn string_match_contains_is_rendered_per_dialect_without_like() {
        let w = where_of("from cache where text -> \"Rust\"");
        let s = w.render(Dialect::Sqlite);
        assert_eq!(s.sql, "(instr(COALESCE(n.text,''), ?) > 0)");
        assert_eq!(s.params, vec![text("Rust")]);
        let p = w.render(Dialect::Postgres);
        assert_eq!(p.sql, "(strpos(COALESCE(n.text,''), $1) > 0)");
        assert_eq!(p.params, vec![text("Rust")]);
        let m = w.render(Dialect::MySql);
        assert_eq!(m.sql, "(LOCATE(BINARY ?, COALESCE(n.text,'')) > 0)");
        assert_eq!(m.params, vec![text("Rust")]);
    }

    #[test]
    fn string_match_starts_with_binds_the_needle_twice() {
        let w = where_of("from cache where text startswith \"hi\"");
        let s = w.render(Dialect::Sqlite);
        assert_eq!(s.sql, "(substr(COALESCE(n.text,''), 1, length(?)) = ?)");
        assert_eq!(s.params, vec![text("hi"), text("hi")]);
        let p = w.render(Dialect::Postgres);
        assert_eq!(p.sql, "(left(COALESCE(n.text,''), length($1)) = $2)");
        assert_eq!(p.params, vec![text("hi"), text("hi")]);
        let m = w.render(Dialect::MySql);
        assert_eq!(m.sql, "(LEFT(COALESCE(n.text,''), CHAR_LENGTH(?)) = BINARY ?)");
        assert_eq!(m.params, vec![text("hi"), text("hi")]);
    }

    #[test]
    fn string_match_ends_with_computes_the_start_without_negative_substr() {
        let w = where_of("from cache where text endswith \"hi\"");
        let s = w.render(Dialect::Sqlite);
        // `substr(X, -N)` は N=0 で全体を返してしまうので、開始位置を length から計算する。
        assert_eq!(
            s.sql,
            "(substr(COALESCE(n.text,''), length(COALESCE(n.text,'')) - length(?) + 1) = ?)"
        );
        assert_eq!(s.params, vec![text("hi"), text("hi")]);
        let p = w.render(Dialect::Postgres);
        assert_eq!(p.sql, "(right(COALESCE(n.text,''), length($1)) = $2)");
        let m = w.render(Dialect::MySql);
        assert_eq!(m.sql, "(RIGHT(COALESCE(n.text,''), CHAR_LENGTH(?)) = BINARY ?)");
    }

    #[test]
    fn string_match_needle_is_bound_raw_never_escaped_or_inlined() {
        let w = where_of("from cache where text -> \"100%_!\"");
        for d in [Dialect::Sqlite, Dialect::Postgres, Dialect::MySql] {
            let r = w.render(d);
            assert_eq!(r.params, vec![text("100%_!")], "{d:?}");
            assert!(!r.sql.contains("100"), "検索語がSQLに入っている: {}", r.sql);
        }
    }

    #[test]
    fn string_match_placeholder_order_follows_the_text_when_the_lhs_is_a_literal() {
        // 左辺がリテラルだと、列の式も `?` になる。バインド値の並びは、SQL 中の `?` の並びと一致させる。
        let e = cmp(Value::Str("abc".into()), CompareOp::EndsWith, Value::Str("c".into()));
        let w = build_where(&e, &ctx()).unwrap();
        let s = w.render(Dialect::Sqlite);
        assert_eq!(s.sql, "(substr(?, length(?) - length(?) + 1) = ?)");
        assert_eq!(s.params, vec![text("abc"), text("abc"), text("c"), text("c")]);
        let p = w.render(Dialect::Postgres);
        assert_eq!(p.sql, "(right($1, length($2)) = $3)");
        assert_eq!(p.params, vec![text("abc"), text("c"), text("c")]);

        // MySQL の `LOCATE(BINARY ?, col)` は検索語が先。
        let e = cmp(Value::Str("abc".into()), CompareOp::Contains, Value::Str("b".into()));
        let w = build_where(&e, &ctx()).unwrap();
        let m = w.render(Dialect::MySql);
        assert_eq!(m.sql, "(LOCATE(BINARY ?, ?) > 0)");
        assert_eq!(m.params, vec![text("b"), text("abc")]);
    }

    #[test]
    fn string_match_rejects_a_non_literal_needle() {
        let e = cmp(Value::Field(Field::Text), CompareOp::Contains, Value::Field(Field::CwText));
        assert!(build_where(&e, &ctx()).is_err());
    }

    #[test]
    fn set_membership_and_regex() {
        let w = where_of("from cache where \"👍\" <- reactions");
        assert_eq!(
            sq(&w).sql,
            "(? IN (SELECT emoji_key FROM note_reaction r WHERE r.note_id=n.id))"
        );
        assert_eq!(sq(&w).params, vec![text("👍")]);

        let w = where_of("from cache where text match \"(?i)mi\"");
        assert_eq!(sq(&w).sql, "(COALESCE(n.text,'') REGEXP ?)");
        assert_eq!(w.render(Dialect::MySql).sql, "(COALESCE(n.text,'') REGEXP ?)");
        assert_eq!(w.render(Dialect::Postgres).sql, "(COALESCE(n.text,'') ~ $1)");
    }

    #[test]
    fn context_fields_expand_ids() {
        let w = where_of("from cache where mine");
        assert_eq!(sq(&w).sql, "n.user_id IN (?)");
        assert_eq!(sq(&w).params, vec![text("me1")]);
        assert_eq!(w.render(Dialect::Postgres).sql, "n.user_id IN ($1)");

        let w = where_of("from cache where following");
        assert_eq!(sq(&w).sql, "n.user_id IN (?)");
        assert_eq!(sq(&w).params, vec![text("f1")]);

        let w = where_of("from cache where to_me");
        assert!(sq(&w).sql.contains("note_mention m WHERE m.note_id=n.id AND m.user_id IN (?)"));
    }

    #[test]
    fn and_raw_appends_a_condition_and_renumbers_postgres_placeholders() {
        let w = where_of("from cache where reactions >= 10").and_raw(
            "n.id IN (SELECT note_id FROM note_account WHERE account_id = ?)",
            vec![text("a1")],
        );
        let s = w.render(Dialect::Sqlite);
        assert_eq!(
            s.sql,
            "((n.reaction_count >= ?)) AND (n.id IN (SELECT note_id FROM note_account WHERE account_id = ?))"
        );
        assert_eq!(s.params, vec![SqlParam::Real(10.0), text("a1")]);
        let p = w.render(Dialect::Postgres);
        assert_eq!(
            p.sql,
            "((n.reaction_count >= $1)) AND (n.id IN (SELECT note_id FROM note_account WHERE account_id = $2))"
        );
        assert_eq!(p.params, s.params);
    }

    #[test]
    fn and_raw_after_a_two_bind_match_keeps_postgres_numbers_in_order() {
        let w = where_of("from cache where text startswith \"hi\"").and_raw("n.id = ?", vec![text("x")]);
        let p = w.render(Dialect::Postgres);
        assert_eq!(p.sql, "((left(COALESCE(n.text,''), length($1)) = $2)) AND (n.id = $3)");
        assert_eq!(p.params, vec![text("hi"), text("hi"), text("x")]);
    }

    #[test]
    fn raw_and_always_true_render_for_every_dialect() {
        for d in [Dialect::Sqlite, Dialect::Postgres, Dialect::MySql] {
            assert_eq!(SqlWhere::always_true().render(d).sql, "1=1");
            assert!(SqlWhere::always_true().render(d).params.is_empty());
        }
        let w = SqlWhere::raw("n.id = ?", vec![text("a")]);
        assert_eq!(w.render(Dialect::Sqlite).sql, "n.id = ?");
        assert_eq!(w.render(Dialect::MySql).sql, "n.id = ?");
        assert_eq!(w.render(Dialect::Postgres).sql, "n.id = $1");
        assert_eq!(w.render(Dialect::Postgres).params, vec![text("a")]);
    }
}
```

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認する**

Run: `cd src-tauri && cargo test --lib filter::sql 2>&1 | tail -20`
Expected: FAIL。`cannot find type 'Dialect' in this scope` などのコンパイルエラー。

- [ ] **Step 3: `sql.rs` の本体を実装する**

`src-tauri/src/filter/sql.rs` の冒頭(1〜68行目、`Builder` の `id_list` まで)を次に置き換える。

```rust
//! TQL の SQL 射影。docs/design/filter-dsl-design.md §10・§11。
//! AST → WHERE 句（`cache` ソースの検索用）。`note n JOIN user u` を前提とする。
//!
//! 生成物は、SQLite / PostgreSQL / MySQL の3方言ぶんの [`Rendered`](SQL とバインド値の列)を
//! まとめた [`SqlWhere`]。呼び出し側は backend を知らなくてよい(backend は実行時に差し替わる)ので、
//! 組み立ては方言なしで行い、backend が自分の方言を [`SqlWhere::render`] で選ぶ。
//! 方言で変わるのは、プレースホルダ(`?` / `$n`)・文字列の部分一致・正規表現。
//! 部分一致は `LIKE` を使わず、方言ごとの関数で書く(検索語のエスケープが要らず、大文字小文字を区別する
//! = インメモリ評価 `filter/eval.rs` と一致する)。
//! context 依存(mine/following/to_me/@account) は my_ids / following_ids をプレースホルダとして展開する。
//! 注意(v1): リアクションのホスト吸収正規化は未対応。

use super::ast::*;

#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    Text(String),
    Real(f64),
}

pub struct SqlCtx {
    pub my_ids: Vec<String>,
    pub following_ids: Option<Vec<String>>,
}

/// SQL の方言。`SqlWhere::render` で、backend が自分の分を選ぶ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
    MySql,
}

/// 1方言ぶんの WHERE 句。プレースホルダは SQLite / MySQL が `?`、PostgreSQL が `$n`。
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub sql: String,
    pub params: Vec<SqlParam>,
}

/// 3方言ぶんの WHERE 句。値は SQL に埋め込まず、必ず `params` でバインドする。
#[derive(Debug, Clone, PartialEq)]
pub struct SqlWhere {
    sqlite: Rendered,
    postgres: Rendered,
    mysql: Rendered,
}

impl SqlWhere {
    pub fn render(&self, dialect: Dialect) -> &Rendered {
        match dialect {
            Dialect::Sqlite => &self.sqlite,
            Dialect::Postgres => &self.postgres,
            Dialect::MySql => &self.mysql,
        }
    }

    /// 3方言で同じ意味になる固定の SQL(`?` プレースホルダ)から作る。リテラルの中に `?` を書かない。
    pub fn raw(sql: &str, params: Vec<SqlParam>) -> Self {
        let one = |d: Dialect| Rendered { sql: convert_placeholders(sql, d, 0), params: params.clone() };
        SqlWhere { sqlite: one(Dialect::Sqlite), postgres: one(Dialect::Postgres), mysql: one(Dialect::MySql) }
    }

    pub fn always_true() -> Self {
        Self::raw("1=1", vec![])
    }

    /// `({既存}) AND ({sql})` に連結する。`sql` は [`SqlWhere::raw`] と同じ書き方(`?`)。
    /// PostgreSQL は、既存のバインド数の続きに `$n` を振り直す。
    pub fn and_raw(self, sql: &str, params: Vec<SqlParam>) -> Self {
        let join = |d: Dialect, base: Rendered| {
            let extra = convert_placeholders(sql, d, base.params.len());
            let mut all = base.params;
            all.extend(params.iter().cloned());
            Rendered { sql: format!("({}) AND ({})", base.sql, extra), params: all }
        };
        SqlWhere {
            sqlite: join(Dialect::Sqlite, self.sqlite),
            postgres: join(Dialect::Postgres, self.postgres),
            mysql: join(Dialect::MySql, self.mysql),
        }
    }
}

/// PostgreSQL のときだけ、`?` を `already_bound` の続きの `$n` に振り直す。
fn convert_placeholders(sql: &str, dialect: Dialect, already_bound: usize) -> String {
    if dialect != Dialect::Postgres {
        return sql.to_string();
    }
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n = already_bound;
    for ch in sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn build_where(expr: &Expr, ctx: &SqlCtx) -> Result<SqlWhere, String> {
    let render = |dialect: Dialect| -> Result<Rendered, String> {
        let mut b = Builder { ctx, dialect, params: Vec::new() };
        let sql = b.expr(expr)?;
        Ok(Rendered { sql, params: b.params })
    };
    Ok(SqlWhere {
        sqlite: render(Dialect::Sqlite)?,
        postgres: render(Dialect::Postgres)?,
        mysql: render(Dialect::MySql)?,
    })
}

#[derive(Clone, Copy)]
enum TextMatch {
    Contains,
    StartsWith,
    EndsWith,
}

struct Builder<'a> {
    ctx: &'a SqlCtx,
    dialect: Dialect,
    params: Vec<SqlParam>,
}

impl Builder<'_> {
    /// 値を積み、その方言のプレースホルダ(`?` / `$n`)を返す。
    fn bind(&mut self, p: SqlParam) -> String {
        self.params.push(p);
        match self.dialect {
            Dialect::Postgres => format!("${}", self.params.len()),
            _ => "?".to_string(),
        }
    }
    fn push_text(&mut self, s: String) -> String {
        self.bind(SqlParam::Text(s))
    }
    fn push_real(&mut self, x: f64) -> String {
        self.bind(SqlParam::Real(x))
    }

    /// id リストを `(?, ?, ...)` に展開。空なら `(NULL)`（= 常に偽）。
    fn id_list(&mut self, ids: &[String]) -> String {
        if ids.is_empty() {
            return "(NULL)".to_string();
        }
        let mut s = String::from("(");
        for (i, id) in ids.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let ph = self.push_text(id.clone());
            s.push_str(&ph);
        }
        s.push(')');
        s
    }
```

続けて、`Builder` の他のメソッドを次のとおり変更する(それ以外は現状のまま)。

`compare` の `StartsWith` / `EndsWith` / `Match` / `Contains` の4つの腕(現状の108〜138行目)を次に置き換える。

```rust
            StartsWith => self.text_match(TextMatch::StartsWith, lhs, rhs)?,
            EndsWith => self.text_match(TextMatch::EndsWith, lhs, rhs)?,
            Match => {
                let col = self.str(lhs)?;
                let pat = self.str(rhs)?;
                let op = if self.dialect == Dialect::Postgres { "~" } else { "REGEXP" };
                format!("({col} {op} {pat})")
            }
            Contains => {
                if set_capable(lhs) {
                    // 集合が要素を含む: rhs IN (集合サブクエリ)
                    let elem = self.str(rhs)?;
                    let sub = self.set_subquery(lhs)?;
                    format!("({elem} IN {sub})")
                } else {
                    // 文字列部分一致
                    self.text_match(TextMatch::Contains, lhs, rhs)?
                }
            }
```

`compare` メソッドの直後に、`text_match` を追加する。

```rust
    /// 文字列の部分一致。大文字小文字を区別し、`%` / `_` / `\` などを実文字として扱う。
    /// 検索語はエスケープせずバインドする。`?` の並び(= バインド値の並び)が SQL の文字の並びと
    /// 一致するよう、式の中に現れる順に積む(PostgreSQL は `$n` の番号が積んだ順になる)。
    fn text_match(&mut self, kind: TextMatch, lhs: &Value, rhs: &Value) -> Result<String, String> {
        let needle = needle_value(rhs)?;
        Ok(match (self.dialect, kind) {
            (Dialect::Sqlite, TextMatch::Contains) => {
                let col = self.str(lhs)?;
                let n = self.push_text(needle);
                format!("(instr({col}, {n}) > 0)")
            }
            (Dialect::Sqlite, TextMatch::StartsWith) => {
                let col = self.str(lhs)?;
                let n1 = self.push_text(needle.clone());
                let n2 = self.push_text(needle);
                format!("(substr({col}, 1, length({n1})) = {n2})")
            }
            (Dialect::Sqlite, TextMatch::EndsWith) => {
                // `substr(X, -N)` は N=0 で全体を返すので、開始位置を length から計算する。
                let col1 = self.str(lhs)?;
                let col2 = self.str(lhs)?;
                let n1 = self.push_text(needle.clone());
                let n2 = self.push_text(needle);
                format!("(substr({col1}, length({col2}) - length({n1}) + 1) = {n2})")
            }
            (Dialect::Postgres, TextMatch::Contains) => {
                let col = self.str(lhs)?;
                let n = self.push_text(needle);
                format!("(strpos({col}, {n}) > 0)")
            }
            (Dialect::Postgres, TextMatch::StartsWith) => {
                let col = self.str(lhs)?;
                let n1 = self.push_text(needle.clone());
                let n2 = self.push_text(needle);
                format!("(left({col}, length({n1})) = {n2})")
            }
            (Dialect::Postgres, TextMatch::EndsWith) => {
                let col = self.str(lhs)?;
                let n1 = self.push_text(needle.clone());
                let n2 = self.push_text(needle);
                format!("(right({col}, length({n1})) = {n2})")
            }
            (Dialect::MySql, TextMatch::Contains) => {
                // `LOCATE` は、引数のどちらかがバイナリ文字列なら大文字小文字を区別する。検索語が先。
                let n = self.push_text(needle);
                let col = self.str(lhs)?;
                format!("(LOCATE(BINARY {n}, {col}) > 0)")
            }
            (Dialect::MySql, TextMatch::StartsWith) => {
                let col = self.str(lhs)?;
                let n1 = self.push_text(needle.clone());
                let n2 = self.push_text(needle);
                format!("(LEFT({col}, CHAR_LENGTH({n1})) = BINARY {n2})")
            }
            (Dialect::MySql, TextMatch::EndsWith) => {
                let col = self.str(lhs)?;
                let n1 = self.push_text(needle.clone());
                let n2 = self.push_text(needle);
                format!("(RIGHT({col}, CHAR_LENGTH({n1})) = BINARY {n2})")
            }
        })
    }
```

`num` と `str` の `push_real` / `push_text` の呼び出しから `.to_string()` を外す(戻り値が `String` になったため)。

```rust
            Value::Num(x) => self.push_real(*x),
```
```rust
            Value::Str(s) => self.push_text(s.clone()),
            Value::Account(a) => self.push_text(a.clone()),
```

ファイル下部の `like_value` を次に置き換える(関数名と中身)。

```rust
fn needle_value(v: &Value) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        _ => Err("string match operand must be a string literal".into()),
    }
}
```

- [ ] **Step 4: `sql.rs` の単体テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib filter::sql 2>&1 | tail -30`
Expected: `filter::sql` のテストだけが対象。ただし他モジュール(`note_cache.rs` など)が `where_sql.sql` を参照していてコンパイルエラーになる。Step 5〜7 で呼び出し側を直してから、もう一度実行する。

- [ ] **Step 5: 呼び出し側を新 API に追従させる**

**`src-tauri/src/store/note_cache.rs` の `search_cache_sync`**(602〜640行目付近)で、`where_sql.sql` と `where_sql.params` の参照を `render` 経由にする。

```rust
    use crate::filter::sql::{Dialect, SqlParam};

    let w = where_sql.render(Dialect::Sqlite);
    let mut sql = String::from("SELECT n.id, n.payload FROM note n JOIN user u ON u.id = n.user_id WHERE (");
    sql.push_str(&w.sql);
    sql.push(')');
    // …(until_id と ORDER BY は、この Task では変えない)…
    for p in &w.params {
```
(元の `use crate::filter::sql::SqlParam;` は上の `use` に置き換える。)

**`src-tauri/src/store/sqlite_backend.rs`** の `search_cache`(354〜370行目)で、手動の複製を `clone()` にする。

```rust
        let conn = Arc::clone(&self.conn);
        let where_sql = where_sql.clone();
```
(「SqlWhereはCloneをderiveしていないため…」のコメント行は削除する。)

**`src-tauri/src/store/postgres_backend.rs`**: `to_postgres_sql`(1144〜1160行目、ドキュメントコメントごと)を削除し、`search_cache_impl` を次に置き換える。

```rust
async fn search_cache_impl(
    pool: &sqlx::PgPool,
    where_sql: &crate::filter::sql::SqlWhere,
    until_id: Option<&str>,
    limit: u32,
) -> Result<Vec<Note>> {
    use crate::filter::sql::{Dialect, SqlParam};

    // `w`はTQLコンパイラ(`filter/sql.rs`)が生成した固定文字列(`$n`番号付き)で、値は
    // 一切埋め込まず`w.params`(下でbind)経由のみで渡す設計のため、
    // `sqlx::AssertSqlSafe`でのラップは安全(監査済み)。
    let w = where_sql.render(Dialect::Postgres);
    let mut sql = format!("SELECT n.id, n.payload FROM note n JOIN \"user\" u ON u.id = n.user_id WHERE ({})", w.sql);
    let mut next_placeholder = w.params.len() + 1;
    if until_id.is_some() {
        sql.push_str(&format!(" AND n.id < ${next_placeholder}"));
        next_placeholder += 1;
    }
    sql.push_str(&format!(" ORDER BY n.created_at DESC, n.id DESC LIMIT ${next_placeholder}"));

    let mut query = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql));
    for p in &w.params {
        query = match p {
            SqlParam::Text(s) => query.bind(s.clone()),
            SqlParam::Real(x) => query.bind(*x),
        };
    }
    if let Some(u) = until_id {
        query = query.bind(u.to_string());
    }
    query = query.bind(limit as i64);

    let rows = query.fetch_all(pool).await?;
    resolve_payload_rows(pool, rows).await
}
```

**`src-tauri/src/store/mysql_backend.rs`** の `search_cache_impl`(1139〜1170行目)で、先頭の `use` とコメントと SQL 組み立てを変える(それ以外は現状のまま)。

```rust
    use crate::filter::sql::{Dialect, SqlParam};

    // `w`はTQLコンパイラ(`filter/sql.rs`)が生成する固定文字列で、値は一切埋め込まず
    // `w.params`(下でbind)経由のみで渡す設計のため、`sqlx::AssertSqlSafe`でのラップは安全(監査済み)。
    let w = where_sql.render(Dialect::MySql);
    let mut sql = format!("SELECT n.id, n.payload FROM note n JOIN `user` u ON u.id = n.user_id WHERE ({})", w.sql);
```
続く `for p in &where_sql.params {` を `for p in &w.params {` にする。

**`src-tauri/src/commands/column.rs`** の2箇所(`:1950` と `:2276`)の手組みを置き換える。

```rust
        _ => sql::SqlWhere::always_true(),
```
```rust
            None => sql::SqlWhere::always_true(),
```

**`src-tauri/src/mcp/query.rs`**: 冒頭の `use`、`with_scope`、手組み2箇所を置き換える。

```rust
use crate::filter::sql::{build_where, SqlCtx, SqlParam, SqlWhere};
```
(`Dialect` は、テストモジュールでだけ使う。)

```rust
fn with_scope(base: SqlWhere, account_id: &str) -> SqlWhere {
    base.and_raw(SCOPE_SQL, vec![SqlParam::Text(account_id.to_string())])
}
```
```rust
        None => SqlWhere::always_true(),
```
```rust
    let base = SqlWhere::raw("n.id = ?", vec![SqlParam::Text(note_id.to_string())]);
```

冒頭コメント(7〜8行目)を次に置き換える。

```rust
//! 注意: TQL の部分一致(`Contains`)は、`%` / `_` / `\` を実文字として扱い、大文字小文字を区別する
//! (方言ごとの関数で書くので、検索語のエスケープは要らない)。`text` の検索語はバインドされる。
```

`mcp/query.rs` の `mod tests` を追従させる。`use` に `Dialect` を足す。

```rust
    use crate::filter::sql::{Dialect, SqlParam};
```
次の4つを置き換える。

`texts` ヘルパー:
```rust
    fn texts(q: &Query) -> Vec<String> {
        q.where_sql
            .render(Dialect::Sqlite)
            .params
            .iter()
            .filter_map(|p| match p {
                SqlParam::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }
```
`every_query_carries_the_scope_with_the_account_id_bound_last` のループ内:
```rust
            let sql = &q.where_sql.render(Dialect::Sqlite).sql;
            assert!(sql.contains(SCOPE_SQL), "スコープ条件が付いていない: {sql}");
            assert_eq!(texts(&q).last().map(String::as_str), Some("a1"), "最後のパラメータはアカウントID");
```
`text_words_become_bound_like_patterns_and_never_enter_the_sql`(名前も `text_words_become_bound_needles_and_never_enter_the_sql` に変える):
```rust
    #[test]
    fn text_words_become_bound_needles_and_never_enter_the_sql() {
        let p = SearchParams { text: Some("rust  '; DROP TABLE note;--".into()), ..Default::default() };
        let q = build_search("a1", HOST, &p).unwrap();
        for d in [Dialect::Sqlite, Dialect::Postgres, Dialect::MySql] {
            let sql = &q.where_sql.render(d).sql;
            assert!(!sql.contains("DROP"), "入力がSQLに入っている: {sql}");
            assert!(!sql.contains("rust"));
        }
        let t = texts(&q);
        assert_eq!(t[0], "rust");
        assert_eq!(t[1], "';");
        assert!(t.contains(&"DROP".to_string()));
    }
```
`blank_text_is_treated_as_absent` の最終行:
```rust
        assert_eq!(a.where_sql.render(Dialect::Sqlite).sql, b.where_sql.render(Dialect::Sqlite).sql);
```
`since_and_until_become_epoch_second_bounds` と `date_only_since_and_until_cover_the_whole_day` の `.where_sql\n.params` を `.where_sql\n.render(Dialect::Sqlite)\n.params` にする(2箇所)。

`percent_in_text_acts_as_a_wildcard_but_stays_inside_the_scope`(`:293-300`)を、次に置き換える。

```rust
    #[tokio::test]
    async fn percent_and_underscore_in_text_are_literal_and_stay_inside_the_scope() {
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()));
        let notes = vec![
            public_note("n1", 400, "50% off"),
            public_note("n2", 300, "50 off"),
            public_note("n3", 200, "a_b"),
            public_note("n4", 100, "aXb"),
        ];
        cache.cache_notes("col1", &notes).await.unwrap();
        cache.record_note_accounts("a1", &notes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()).await.unwrap();

        for (text, expected) in [("%", vec!["n1"]), ("50%", vec!["n1"]), ("_", vec!["n3"]), ("a_b", vec!["n3"])] {
            let q = build_search("a1", HOST, &SearchParams { text: Some(text.into()), ..Default::default() }).unwrap();
            assert_eq!(ids_of(&cache, &q).await, expected, "text = {text:?}");
        }
        // スコープ条件のANDは外れない: 帰属の無いアカウントでは、何も出ない。
        let q = build_search("never-seen", HOST, &SearchParams { text: Some("%".into()), ..Default::default() }).unwrap();
        assert!(ids_of(&cache, &q).await.is_empty());
        // seeded() の範囲外のノートも、`%` では出ない。
        let seeded = seeded().await;
        let q = build_search("a1", HOST, &SearchParams { text: Some("%".into()), ..Default::default() }).unwrap();
        assert!(ids_of(&seeded, &q).await.is_empty());
    }
```

- [ ] **Step 6: 契約テスト(文字列マッチ)を書く**

`src-tauri/src/store/mod.rs` に、`mod sqlite_backend;` の次の行として追加する。

```rust
#[cfg(test)]
pub(crate) mod search_contract;
```

`src-tauri/src/store/search_contract.rs` を新規作成する。

```rust
//! 3 backend(SQLite / PostgreSQL / MySQL)の `search_cache` が同じ結果を返すことを固定する共有ケース
//! (Issue #583)。SQLite は通常のテストから、PostgreSQL / MySQL は `#[ignore]` の実 DB テストから呼ぶ。
//! 同じケースを、ライブ受信のインメモリ評価(`filter/eval.rs`)にも流して、SQL と一致することを確かめる。

use crate::domain::Note;
use crate::filter::ast::{CompareOp, Expr, Field, Value};
use crate::filter::eval::{evaluate, EvalContext};
use crate::filter::sql::{build_where, SqlCtx};
use crate::mcp::testutil::public_note;
use crate::store::note_cache::NoteCacheBackend;

const ALL: &[&str] = &["t01", "t02", "t03", "t04", "t05", "t06", "t07", "t08", "t09", "t10", "t11"];

fn fixtures() -> Vec<Note> {
    let mut notes = vec![
        public_note("t01", 100, "100% sure"),
        public_note("t02", 101, "100 percent"),
        public_note("t03", 102, "a_b"),
        public_note("t04", 103, "aXb"),
        public_note("t05", 104, "Hello World"),
        public_note("t06", 105, "hello world"),
        public_note("t07", 106, "path\\to"),
        public_note("t08", 107, "wow!"),
        public_note("t09", 108, "日本語のテキスト😀"),
        public_note("t10", 109, "placeholder"),
        public_note("t11", 110, "x"),
    ];
    notes[9].text = None; // t10: 本文なし。空文字列として扱われる。
    notes[10].cw = Some("Spoiler".into()); // t11: CW あり。他のノートは CW なし(空文字列として扱われる)。
    notes
}

struct Case {
    field: Field,
    op: CompareOp,
    needle: &'static str,
    expected: &'static [&'static str],
}

fn c(field: Field, op: CompareOp, needle: &'static str, expected: &'static [&'static str]) -> Case {
    Case { field, op, needle, expected }
}

fn cases() -> Vec<Case> {
    use CompareOp::{Contains, EndsWith, StartsWith};
    use Field::{CwText, Text};
    vec![
        // --- Contains: `%` `_` `\` `!` は実文字。大文字小文字を区別する。
        c(Text, Contains, "100%", &["t01"]),
        c(Text, Contains, "%", &["t01"]),
        c(Text, Contains, "_", &["t03"]),
        c(Text, Contains, "a_b", &["t03"]),
        c(Text, Contains, "\\", &["t07"]),
        c(Text, Contains, "!", &["t08"]),
        c(Text, Contains, "Hello", &["t05"]),
        c(Text, Contains, "hello", &["t06"]),
        c(Text, Contains, "HELLO", &[]),
        c(Text, Contains, "日本語", &["t09"]),
        c(Text, Contains, "😀", &["t09"]),
        c(Text, Contains, "", ALL),
        c(Text, Contains, "100% sure and more", &[]),
        // --- StartsWith
        c(Text, StartsWith, "100", &["t01", "t02"]),
        c(Text, StartsWith, "100%", &["t01"]),
        c(Text, StartsWith, "a_", &["t03"]),
        c(Text, StartsWith, "Hello", &["t05"]),
        c(Text, StartsWith, "hello", &["t06"]),
        c(Text, StartsWith, "path\\", &["t07"]),
        c(Text, StartsWith, "日本", &["t09"]),
        c(Text, StartsWith, "", ALL),
        c(Text, StartsWith, "100% sure and more", &[]),
        // --- EndsWith
        c(Text, EndsWith, "sure", &["t01"]),
        c(Text, EndsWith, "percent", &["t02"]),
        c(Text, EndsWith, "to", &["t07"]),
        c(Text, EndsWith, "b", &["t03", "t04"]),
        c(Text, EndsWith, "_b", &["t03"]),
        c(Text, EndsWith, "!", &["t08"]),
        c(Text, EndsWith, "😀", &["t09"]),
        c(Text, EndsWith, "World", &["t05"]),
        c(Text, EndsWith, "", ALL),
        c(Text, EndsWith, "100% sure and more", &[]),
        // --- NULL の cw / text は空文字列として扱われる(空の検索語にだけ一致)。
        c(CwText, Contains, "Spoil", &["t11"]),
        c(CwText, Contains, "spoil", &[]),
        c(CwText, Contains, "", ALL),
        c(CwText, StartsWith, "Spoil", &["t11"]),
        c(CwText, EndsWith, "ler", &["t11"]),
        c(Text, Contains, "placeholder", &[]),
    ]
}

/// 部分一致(`->` / `startswith` / `endswith`)が、3 backend で同じ結果になること。
pub(crate) async fn text_match_contract(b: &dyn NoteCacheBackend) {
    let notes = fixtures();
    b.cache_notes("col1", &notes).await.unwrap();
    let ctx = SqlCtx { my_ids: vec![], following_ids: None };
    let eval_ctx = EvalContext::default();

    for case in cases() {
        let expr = Expr::Compare {
            lhs: Value::Field(case.field),
            op: case.op,
            rhs: Value::Str(case.needle.to_string()),
        };
        let want: Vec<String> = case.expected.iter().map(|s| s.to_string()).collect();
        let label = format!("{:?} {:?} {:?}", case.field, case.op, case.needle);

        let w = build_where(&expr, &ctx).unwrap();
        let mut got: Vec<String> = b.search_cache(&w, None, 100).await.unwrap().into_iter().map(|n| n.id).collect();
        got.sort();
        assert_eq!(got, want, "SQL: {label}");

        // ライブ受信のインメモリ評価とも一致する。
        let mut mem: Vec<String> = notes.iter().filter(|n| evaluate(&expr, n, &eval_ctx)).map(|n| n.id.clone()).collect();
        mem.sort();
        assert_eq!(mem, want, "eval: {label}");
    }
}
```

`src-tauri/src/store/sqlite_backend.rs` の `mod tests` に、`search_cache_applies_predicate_and_until_id_boundary` の直後へ追加する。

```rust
    /// 部分一致の契約(Issue #583)。PostgreSQL / MySQL も同じケースを通る(`search_contract.rs`)。
    #[tokio::test]
    async fn search_cache_text_match_contract() {
        crate::store::search_contract::text_match_contract(&store()).await;
    }
```

- [ ] **Step 7: Task 1 のテストを実行する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -40`
Expected: PASS(`filter::sql`、`store::sqlite_backend`(契約テスト含む)、`mcp::query`、`commands::column` のテスト)。

契約テストが落ちた場合は、メッセージの `SQL: <field> <op> <needle>` で原因の式を特定し、`text_match` の SQLite の腕だけを直す。想定される落ち方と直し方:
- 空の検索語の `Contains` が偽になる(`instr(X, '')` が 0 を返す場合): SQLite の `Contains` を `(({n}) = '' OR instr({col}, {n}) > 0)` のように、検索語を2回バインドする式に変え、`filter::sql` の `string_match_contains_is_rendered_per_dialect_without_like` の期待値(SQLite)も合わせる。
- それ以外: 落ちたケースの式を `sqlite3` で手で確かめて、腕を直す。

次を確認する。

Run: `cd src-tauri && grep -rn "to_postgres_sql\|like_value\|LIKE operand" src/ | head`
Expected: 出力なし。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src
git commit -m "feat(filter): TQLのSQL生成を方言ごとに分け、部分一致を実文字・大文字小文字区別にする (#583)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: ページ境界を ID 順にする

`search_cache` の並びと境界を ID 基準にし、`search_cache_core` は並べ替えず ID 降順のまま返す。

**Files:**
- Modify: `src-tauri/src/store/search_contract.rs`(`paging_contract` を追加)
- Modify: `src-tauri/src/store/note_cache.rs`(`search_cache_sync`、`search_cache` のドキュメント)
- Modify: `src-tauri/src/store/postgres_backend.rs`(`search_cache_impl`)
- Modify: `src-tauri/src/store/mysql_backend.rs`(`search_cache_impl`)
- Modify: `src-tauri/src/store/sqlite_backend.rs`(tests)
- Modify: `src-tauri/src/commands/column.rs`(`search_cache_core` と tests)
- Modify: `src-tauri/src/mcp/tools.rs:90`(説明文)

**Interfaces:**
- Consumes: `SqlWhere::always_true()`(Task 1)、`NoteCacheBackend::search_cache`
- Produces: `search_contract::paging_contract(b: &dyn NoteCacheBackend)`(`#[cfg(test)]`、`pub(crate)`)。`search_cache` は **ID 降順**で返す。

- [ ] **Step 1: ページ境界の契約テストを書く(失敗するテスト)**

`src-tauri/src/store/search_contract.rs` の先頭の `use` に `SqlWhere` を足し(`use crate::filter::sql::{build_where, SqlCtx, SqlWhere};`)、末尾に追加する。

```rust
/// ページ境界が ID 基準で、`created_at` と ID の順序が食い違うノートでも欠落も重複も起きないこと。
pub(crate) async fn paging_contract(b: &dyn NoteCacheBackend) {
    // ID の昇順: a0 < a1 < a2 < a3 < z9。`created_at` の新しい順: a0(999) > a3 > a2 > a1 > z9(10)。
    let notes = vec![
        public_note("a0", 999, "newest by time, smallest id"),
        public_note("a1", 100, "x"),
        public_note("a2", 200, "x"),
        public_note("a3", 300, "x"),
        public_note("z9", 10, "oldest by time, largest id"),
    ];
    b.cache_notes("col1", &notes).await.unwrap();
    let w = SqlWhere::always_true();
    let ids = |notes: Vec<Note>| -> Vec<String> { notes.into_iter().map(|n| n.id).collect() };

    // 先頭ページは ID の大きい順。
    assert_eq!(ids(b.search_cache(&w, None, 2).await.unwrap()), ["z9", "a3"]);

    // 各ページの最後の ID をカーソルにしてたどると、全ノートが1回ずつ現れる。
    let mut seen: Vec<String> = vec![];
    let mut cursor: Option<String> = None;
    for _ in 0..10 {
        let page = b.search_cache(&w, cursor.as_deref(), 2).await.unwrap();
        let Some(last) = page.last() else { break };
        cursor = Some(last.id.clone());
        seen.extend(ids(page));
    }
    assert_eq!(seen, ["z9", "a3", "a2", "a1", "a0"]);

    // 存在しない ID をカーソルにしても、その ID より小さいものが続く("a2" < "a25" < "a3")。
    assert_eq!(ids(b.search_cache(&w, Some("a25"), 10).await.unwrap()), ["a2", "a1", "a0"]);
}
```

`src-tauri/src/store/sqlite_backend.rs` の `mod tests` の、Task 1 で足した契約テストの直後に追加する。

```rust
    #[tokio::test]
    async fn search_cache_paging_contract() {
        crate::store::search_contract::paging_contract(&store()).await;
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib search_cache_paging_contract 2>&1 | tail -15`
Expected: FAIL。先頭ページが `["a0", "a3"]`(`created_at` 順)になり、`left: ["a0", "a3"] right: ["z9", "a3"]` のような差分が出る。

- [ ] **Step 3: 3 backend の `search_cache` を ID 順にする**

3箇所とも、`ORDER BY n.created_at DESC, n.id DESC LIMIT` を `ORDER BY n.id DESC LIMIT` に変える(`id` は主キーで一意なので副キーは要らない)。

`src-tauri/src/store/note_cache.rs` の `search_cache_sync`:
```rust
    sql.push_str(" ORDER BY n.id DESC LIMIT ?");
```
`src-tauri/src/store/postgres_backend.rs` の `search_cache_impl`:
```rust
    sql.push_str(&format!(" ORDER BY n.id DESC LIMIT ${next_placeholder}"));
```
`src-tauri/src/store/mysql_backend.rs` の `search_cache_impl`:
```rust
    sql.push_str(" ORDER BY n.id DESC LIMIT ?");
```

`src-tauri/src/store/note_cache.rs` の `NoteCacheStore::search_cache` のドキュメント(207〜208行目)を置き換える。

```rust
    /// TQL `cache` ソース: ローカルキャッシュ全体を where 句で検索する（受信せず検索のみ）。
    /// 並びと境界は **ID 順**(`ORDER BY n.id DESC`、`until_id` はそれより小さい ID)。バックフィルの透かし・
    /// REST の `untilId`・MCP の `cursor` と同じ基準で、返したページの最後のノートがその最小 ID になる。
    /// 表示用に `created_at` で並べ直すのは呼び出し側の責務(カーソルには使わない)。
```

- [ ] **Step 4: `search_cache_core` を ID 降順のまま返すようにする**

`src-tauri/src/commands/column.rs` の `search_cache_core`(1952行目付近)の、`filtered` の並べ替えを消す。

```rust
    let mut filtered: Vec<Note> = raw
        .into_iter()
        .filter(|n| {
            compiled.matches(n, eval_ctx)
                && !crate::filter::mute::is_muted(n, mute)
                && !is_server_muted(n)
                && !is_word_muted(n)
        })
        .collect();
    // `search_cache` が返した ID 降順のまま返す。検索モーダルは「表示中の最後のノートの ID」を次の
    // `until_id` にするため、ここで `created_at` に並べ直すと最後が最小 ID でなくなり、ページ送りで
    // 取りこぼしや重複が起きる(Issue #583)。
    filtered.truncate(limit as usize);
    Ok(filtered)
```
関数のドキュメントコメント(`/// キャッシュDB検索(Issue #248)の中核ロジック。…`)に、1行足す: `/// 返す順序は ID 降順(`search_cache` と同じ。カーソルは最後のノートの ID)。`

`mod tests` の2つを直す。`search_cache_core_with_empty_predicate_returns_all_desc_order` は、名前と期待値を変える(データは `n1`(100)・`n2`(300)・`n3`(200) のまま)。

```rust
    #[tokio::test]
    async fn search_cache_core_with_empty_predicate_returns_all_in_id_desc_order() {
        let cache = cache_with(&[note("n1", 100), note("n2", 300), note("n3", 200)]).await;
        // …(filter と呼び出しは現状のまま)…
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n3", "n2", "n1"]);
    }
```

その直後に、ページ送りのテストを追加する。

```rust
    /// `created_at` と ID の順序が食い違っても、最後のノートの ID をカーソルにしたページ送りで
    /// 欠落も重複も起きない(Issue #583)。
    #[tokio::test]
    async fn search_cache_core_pages_by_id_even_when_created_at_order_differs() {
        let cache = cache_with(&[note("a0", 999), note("a1", 100), note("z9", 10)]).await;
        let filter = FilterQuery::Tql(String::new());
        let mut seen: Vec<String> = vec![];
        let mut cursor: Option<String> = None;
        for _ in 0..5 {
            let page = search_cache_core(
                &cache,
                &filter,
                &EvalContext::default(),
                &MuteConfig::default(),
                cursor.as_deref(),
                1,
                |_| false,
                |_| false,
            )
            .await
            .unwrap();
            let Some(last) = page.last() else { break };
            cursor = Some(last.id.clone());
            seen.extend(page.iter().map(|n| n.id.clone()));
        }
        assert_eq!(seen, ["z9", "a1", "a0"]);
    }
```

- [ ] **Step 5: MCP の説明文を実態に合わせる**

`src-tauri/src/mcp/tools.rs:90` の `description` の `newest first` を `newest note ID first` に変える。

```rust
        description = "Search the locally cached Public/Home notes of one account, newest note ID first. Returns {notes, nextCursor?}. …"
```
(以降は現状のまま。)

Run: `cd src-tauri && grep -rn "newest first" src/ | head`
Expected: 出力なし(テストが説明文を参照していないことの確認)。

- [ ] **Step 6: テストを実行する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -40`
Expected: PASS(`search_cache_paging_contract`、`search_cache_core_*`、`mcp::query` の `cursor_pages_through_older_notes`、`mcp::service` の `next_cursor_is_the_last_id_when_the_page_is_full` を含む)。

落ちたテストが ID と `created_at` の食い違いを前提にしていないか確認する(データの ID が `created_at` と同じ昇順なら、結果は変わらない)。期待値が `created_at` 順を前提に書かれていたものだけ、ID 降順に直す。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src
git commit -m "fix(store): search_cacheのページ境界をID順に揃える (#583)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: PostgreSQL / MySQL の実 DB テスト

Task 1・2 の契約テストを、実 DB(Docker)で通す。関数の挙動(空文字列・多バイト・末尾空白など)は、ここで初めて実 DB で確かめられる。

**Files:**
- Modify: `src-tauri/src/store/postgres_backend.rs`(tests)
- Modify: `src-tauri/src/store/mysql_backend.rs`(tests)
- Modify(落ちた場合のみ): `src-tauri/src/filter/sql.rs` の `text_match` の該当方言の腕と、その単体テストの期待値

**Interfaces:**
- Consumes: `search_contract::text_match_contract` / `paging_contract`(Task 1・2)、各 backend の `backend()`(テスト内の既存ヘルパー)

- [ ] **Step 1: 実 DB テストを追加する**

`src-tauri/src/store/postgres_backend.rs` の `mod tests` の、`search_cache_translates_regexp_to_postgres_tilde_operator` の直後に追加する。

```rust
    /// 部分一致の契約(Issue #583)。SQLite と同じケースを通る(`search_contract.rs`)。
    #[tokio::test]
    #[ignore]
    async fn search_cache_text_match_contract() {
        let s = backend().await;
        crate::store::search_contract::text_match_contract(&*s).await;
    }

    #[tokio::test]
    #[ignore]
    async fn search_cache_paging_contract() {
        let s = backend().await;
        crate::store::search_contract::paging_contract(&*s).await;
    }
```

`src-tauri/src/store/mysql_backend.rs` の `mod tests` の、`search_cache_applies_predicate_and_until_id_boundary` の直後に、同じ2つを追加する(`backend()` は同名のヘルパーが MySQL 用に既にある)。

```rust
    /// 部分一致の契約(Issue #583)。SQLite と同じケースを通る(`search_contract.rs`)。
    #[tokio::test]
    #[ignore]
    async fn search_cache_text_match_contract() {
        let s = backend().await;
        crate::store::search_contract::text_match_contract(&*s).await;
    }

    #[tokio::test]
    #[ignore]
    async fn search_cache_paging_contract() {
        let s = backend().await;
        crate::store::search_contract::paging_contract(&*s).await;
    }
```

- [ ] **Step 2: Docker が使えることを確認し、PostgreSQL の実 DB テストを実行する**

Run: `docker ps >/dev/null 2>&1 && echo ok || echo "docker unavailable"`
Expected: `ok`。`docker unavailable` なら、ここで止めてユーザーに報告する(実 DB テストを省略して先へ進まない)。

Run: `cd src-tauri && cargo test --lib postgres_backend::tests::search_cache_ -- --ignored 2>&1 | tail -30`
Expected: PASS(`search_cache_text_match_contract`、`search_cache_paging_contract`、`search_cache_applies_predicate_and_until_id_boundary`、`search_cache_translates_regexp_to_postgres_tilde_operator`、`search_cache_applies_boolean_predicates_without_type_error`)。

- [ ] **Step 3: MySQL の実 DB テストを実行する**

Run: `cd src-tauri && cargo test --lib mysql_backend::tests::search_cache_ -- --ignored --test-threads=2 2>&1 | tail -30`
Expected: PASS。

- [ ] **Step 4: 実 DB テストが落ちた場合の対応**

メッセージの `SQL: <field> <op> <needle>` で、落ちた方言とケースを特定する。`filter/sql.rs` の `text_match` の該当方言の腕を直し、`filter::sql` の単体テスト(`string_match_*`)の期待値も同じ式に合わせる。直したら、SQLite の契約テスト(`cargo test --lib search_cache_text_match_contract`)と、落ちた方言の実 DB テストを再実行する。想定される落ち方:
- MySQL の `Contains` で、検索語が `BINARY` でも大文字小文字を区別しない: `LOCATE(BINARY {n}, {col})` の `{col}` 側も `BINARY {col}` にする。
- MySQL の `StartsWith` / `EndsWith` が、検索語の末尾空白で誤一致・不一致になる(PAD SPACE 照合順序の影響): 落ちたケースを `mysql` クライアントか `sqlx` の一時テストで再現し、`BINARY` を付ける位置(比較の両辺)を変えて直る式を確定する。推測で条件を足さない。
- PostgreSQL の `left` / `right` / `strpos` が型エラー(`could not determine data type of parameter`): 該当の `{n1}` / `{n2}` に `::text` を付ける(`${n}::text`)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src
git commit -m "test(store): 部分一致とページ境界の契約をPostgreSQL・MySQLの実DBで固定する (#583)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
(Step 4 で `filter/sql.rs` を直した場合は、件名を `fix(filter): …` に分けて別コミットにする。)

---

### Task 4: ドキュメントの更新と全体の確認

**Files:**
- Modify: `docs/design/filter-dsl-design.md`(§10 の表と LIKE のエスケープの記述)
- Modify: `CLAUDE.md`(`filter/` の項)

- [ ] **Step 1: `filter-dsl-design.md` を更新する**

Run: `grep -n "LIKE\|startswith\|endswith\|contains" docs/design/filter-dsl-design.md`
該当する表(§10 の507〜514行目付近)と、直後の「LIKE の右辺はエスケープして…」の箇条書きを読み、次のように直す。

- `startswith` / `endswith` / `contains` の3行の「SQL 射影」列を、`LIKE` の式から「方言ごとの関数(下記)」に置き換える。
- 「LIKE の右辺はエスケープしてワイルドカード誤爆を防ぐ」の箇条書きを、次の表と説明に置き換える。

```markdown
文字列の部分一致は `LIKE` を使わず、方言ごとの関数で書く(検索語はエスケープせずバインド。大文字小文字を区別し、`%` `_` `\` は実文字。インメモリ評価と一致する)。`build_where` は3方言ぶんを同時に作り、backend が `SqlWhere::render(Dialect)` で選ぶ。

| 種類 | SQLite | PostgreSQL | MySQL |
| --- | --- | --- | --- |
| `contains` / `->` | `instr(col, ?) > 0` | `strpos(col, ?) > 0` | `LOCATE(BINARY ?, col) > 0` |
| `startswith` | `substr(col, 1, length(?)) = ?` | `left(col, length(?)) = ?` | `LEFT(col, CHAR_LENGTH(?)) = BINARY ?` |
| `endswith` | `substr(col, length(col) - length(?) + 1) = ?` | `right(col, length(?)) = ?` | `RIGHT(col, CHAR_LENGTH(?)) = BINARY ?` |

`search_cache` の並びと `until_id` の境界は ID 順(`ORDER BY n.id DESC` / `n.id < ?`)。
```
(式は、Task 3 で確定したものに合わせる。)

- [ ] **Step 2: `CLAUDE.md` の `filter/` の項に1文足す**

`CLAUDE.md` の `filter/` の箇条書きの、「`Cache queries project TQL filters to SQL with sql::build_where …`」の文の直後に追加する。

```markdown
`build_where` は SQLite / PostgreSQL / MySQL の3方言ぶんの SQL を同時に作り(`SqlWhere`)、backend が `render(Dialect)` で自分の分を選ぶ。部分一致は `LIKE` ではなく方言ごとの関数で、大文字小文字を区別する。`search_cache` の並びと `until_id` の境界は ID 順(契約テストは `store/search_contract.rs`、PostgreSQL / MySQL は `#[ignore]`)。
```

- [ ] **Step 3: 全体のテストを実行する**

Run: `cd src-tauri && cargo test 2>&1 | tail -30`
Expected: PASS(`#[ignore]` は除く)。`generates_frontend_bindings` を含めて落ちないこと。

Run: `cd src-tauri && cargo build 2>&1 | tail -5`
Expected: 警告なし・エラーなし(`SqlWhere` の未使用メソッドの警告が出た場合は、`#[allow(dead_code)]` ではなく、使われていないメソッドを削る)。

Run: `git status --short`
Expected: `frontend/src/bindings/tauri.gen.ts` が変わっていないこと(コマンドの型は変えていない)。

- [ ] **Step 4: Commit**

```bash
git add docs/design/filter-dsl-design.md CLAUDE.md
git commit -m "docs: 部分一致の方言ごとの生成とID順の境界を設計書に反映する (#583)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: PR(ユーザーに確認してから)

- [ ] **Step 1: ユーザーに、push と PR の作成の確認をとる**

push と PR は外向きの操作なので、実行前に確認する。確認が取れたら進める。

- [ ] **Step 2: push と PR**

```bash
git push -u origin fix/search-cache-boundary-and-like-583
gh pr create --title "fix: ノートキャッシュ検索のページ境界と部分一致をbackend間で揃える" --body-file <スクラッチパッド内の本文ファイルの絶対パス>
```
PR 本文は、先に `.github/pull_request_template.md` を読み、その構成に従ってスクラッチパッド内のファイルへ書く(`--body` / `--body-file` はテンプレートを自動適用しないため)。本文に `Fixes #583` を入れ、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。実 DB テスト(PostgreSQL / MySQL)を実行した結果と、#598(別 Issue)への言及を書く。

- [ ] **Step 3: マージ方針**

マージは `gh pr merge <N> --merge`(通常のマージコミット、squash しない)。CI の結果は Monitor で待たず、ユーザーが確認する。

---

## Self-Review(plan 作成時)

- **Spec coverage**:
  - 方言ごとの生成(spec §1、§2)→ Task 1。
  - ページ境界 ID 順(spec §3)→ Task 2。
  - 呼び出し側(spec §4: `search_cache_core`・MCP・バックフィル無変更)→ Task 1(追従)・Task 2(`search_cache_core`、`mcp/tools.rs`)。
  - ドキュメント(spec §5)→ Task 1(`mcp/query.rs` 冒頭、`note_cache.rs`)・Task 2・Task 4。
  - テスト(文字列マッチ・ページ境界・単体・既存追従)→ Task 1・2・3。
  - 実装順序(spec 末尾)→ Task 1〜4 と対応。
- **Placeholder**: Task 3 Step 4 の「落ちた場合」は、実 DB でしか確定できない箇所の対応方針で、式は具体的に書いた。Task 4 Step 1 の表の「式は Task 3 で確定したものに合わせる」も同様(確定前の式は plan 内に書いてある)。
- **Type consistency**: `Dialect::{Sqlite, Postgres, MySql}`、`Rendered { sql, params }`、`SqlWhere::{render, raw, always_true, and_raw}`、`text_match_contract` / `paging_contract(&dyn NoteCacheBackend)` を全 Task で同じ名前で使っている。
