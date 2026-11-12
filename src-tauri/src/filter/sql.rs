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

    fn expr(&mut self, e: &Expr) -> Result<String, String> {
        Ok(match e {
            Expr::Or(a, b) => format!("({} OR {})", self.expr(a)?, self.expr(b)?),
            Expr::And(a, b) => format!("({} AND {})", self.expr(a)?, self.expr(b)?),
            Expr::Not(a) => format!("(NOT {})", self.expr(a)?),
            Expr::Bare(v) => self.bare(v)?,
            Expr::Compare { lhs, op, rhs } => self.compare(lhs, *op, rhs)?,
        })
    }

    fn bare(&mut self, v: &Value) -> Result<String, String> {
        match v {
            Value::Field(f) => self.bool_field(*f),
            _ => Err("bare predicate must be a boolean field".into()),
        }
    }

    fn compare(&mut self, lhs: &Value, op: CompareOp, rhs: &Value) -> Result<String, String> {
        use CompareOp::*;
        Ok(match op {
            Eq | Ne => {
                let sym = if op == Eq { "=" } else { "<>" };
                if numeric(lhs) && numeric(rhs) {
                    format!("({} {} {})", self.num(lhs)?, sym, self.num(rhs)?)
                } else {
                    format!("({} {} {})", self.str(lhs)?, sym, self.str(rhs)?)
                }
            }
            Lt | Gt | Le | Ge => {
                let sym = match op {
                    Lt => "<",
                    Gt => ">",
                    Le => "<=",
                    Ge => ">=",
                    _ => unreachable!(),
                };
                format!("({} {} {})", self.num(lhs)?, sym, self.num(rhs)?)
            }
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
            In => {
                if let Value::Account(_) = lhs {
                    // @me <- mentions : 自分の id が mentions 集合に含まれるか
                    format!(
                        "EXISTS (SELECT 1 FROM note_mention m WHERE m.note_id=n.id AND m.user_id IN {})",
                        self.id_list(&self.ctx.my_ids.clone())
                    )
                } else {
                    // 要素 ∈ 集合: lhs IN (集合サブクエリ)
                    let elem = self.str(lhs)?;
                    let sub = self.set_subquery(rhs)?;
                    format!("({elem} IN {sub})")
                }
            }
        })
    }

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

    fn num(&mut self, v: &Value) -> Result<String, String> {
        Ok(match v {
            Value::Num(x) => self.push_real(*x),
            Value::Arith { lhs, op, rhs } => {
                let sym = match op {
                    ArithOp::Add => "+",
                    ArithOp::Sub => "-",
                    ArithOp::Mul => "*",
                    ArithOp::Div => "/",
                };
                format!("({} {} {})", self.num(lhs)?, sym, self.num(rhs)?)
            }
            Value::Field(f) => num_field(*f)?.to_string(),
            _ => return Err("expected numeric value".into()),
        })
    }

    fn str(&mut self, v: &Value) -> Result<String, String> {
        Ok(match v {
            Value::Str(s) => self.push_text(s.clone()),
            Value::Account(a) => self.push_text(a.clone()),
            Value::Field(f) => str_field(*f)?.to_string(),
            _ => return Err("expected string value".into()),
        })
    }

    fn set_subquery(&mut self, v: &Value) -> Result<String, String> {
        Ok(match v {
            Value::Field(f) => set_field_subquery(*f)?.to_string(),
            Value::Set(items) => {
                // リテラル集合 → VALUES 相当。(?, ?, ...) を IN で使えるよう括弧展開
                let mut parts = Vec::new();
                for it in items {
                    parts.push(self.str(it)?);
                }
                format!("({})", parts.join(","))
            }
            _ => return Err("expected a set".into()),
        })
    }

    fn bool_field(&mut self, f: Field) -> Result<String, String> {
        use Field::*;
        Ok(match f {
            Renote => "(n.renote_id IS NOT NULL AND n.text IS NULL)".into(),
            Quote => "(n.renote_id IS NOT NULL AND n.text IS NOT NULL)".into(),
            Reply => "n.reply_id IS NOT NULL".into(),
            HasFiles => "n.files_count > 0".into(),
            HasPoll => "n.has_poll = 1".into(),
            Cw => "n.cw IS NOT NULL".into(),
            Sensitive => {
                "EXISTS (SELECT 1 FROM note_file f WHERE f.note_id=n.id AND f.is_sensitive=1)".into()
            }
            Local => "u.host IS NULL".into(),
            Remote => "u.host IS NOT NULL".into(),
            Bot => "u.is_bot = 1".into(),
            Cat => "u.is_cat = 1".into(),
            Direct => "n.visibility = 'specified'".into(),
            ToMe => format!(
                "EXISTS (SELECT 1 FROM note_mention m WHERE m.note_id=n.id AND m.user_id IN {})",
                self.id_list(&self.ctx.my_ids.clone())
            ),
            ReplyToMe => format!(
                "n.reply_user_id IN {}",
                self.id_list(&self.ctx.my_ids.clone())
            ),
            HasMention => "EXISTS (SELECT 1 FROM note_mention m WHERE m.note_id=n.id)".into(),
            HasLink => "n.has_link = 1".into(),
            Pinned => "n.is_pinned = 1".into(),
            Reacted => "n.my_reaction IS NOT NULL".into(),
            Renoted => "n.is_renoted_by_me = 1".into(),
            Favorited => "n.is_favorited_by_me = 1".into(),
            Mine => format!("n.user_id IN {}", self.id_list(&self.ctx.my_ids.clone())),
            Following => match self.ctx.following_ids.clone() {
                Some(ids) => format!("n.user_id IN {}", self.id_list(&ids)),
                None => "0".into(), // 未取得なら矛盾（常に偽）
            },
            _ => return Err(format!("field is not boolean: {f:?}")),
        })
    }
}

fn numeric(v: &Value) -> bool {
    matches!(v, Value::Num(_) | Value::Arith { .. })
        || matches!(v, Value::Field(f) if f.supports(FilterType::Numeric))
}
fn set_capable(v: &Value) -> bool {
    matches!(v, Value::Set(_)) || matches!(v, Value::Field(f) if f.supports(FilterType::Set))
}

fn needle_value(v: &Value) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        _ => Err("string match operand must be a string literal".into()),
    }
}

fn num_field(f: Field) -> Result<&'static str, String> {
    use Field::*;
    Ok(match f {
        Reactions => "n.reaction_count",
        Renotes => "n.renote_count",
        Replies => "n.reply_count",
        Files => "n.files_count",
        Length => "n.text_length",
        CreatedAt => "n.created_at",
        UserFollowers => "u.followers_count",
        UserFollowing => "u.following_count",
        UserNotes => "u.notes_count",
        _ => return Err(format!("field is not numeric: {f:?}")),
    })
}

fn str_field(f: Field) -> Result<&'static str, String> {
    use Field::*;
    Ok(match f {
        Text => "COALESCE(n.text,'')",
        CwText => "COALESCE(n.cw,'')",
        Via => "COALESCE(n.via,'')",
        Host => "COALESCE(u.host,'')",
        VisibilityStr => "n.visibility",
        Channel => "COALESCE(n.channel_id,'')",
        Lang => "COALESCE(n.lang,'')",
        ReplyId => "COALESCE(n.reply_id,'')",
        RenoteId => "COALESCE(n.renote_id,'')",
        UserUsername => "u.username",
        UserAcct => "('@'||u.username||CASE WHEN u.host IS NULL THEN '' ELSE '@'||u.host END)",
        UserName => "COALESCE(u.name,'')",
        UserId => "u.id",
        _ => return Err(format!("field is not string: {f:?}")),
    })
}

fn set_field_subquery(f: Field) -> Result<&'static str, String> {
    use Field::*;
    Ok(match f {
        Reactions => "(SELECT emoji_key FROM note_reaction r WHERE r.note_id=n.id)",
        Tags => "(SELECT tag FROM note_tag t WHERE t.note_id=n.id)",
        Mentions => "(SELECT user_id FROM note_mention m WHERE m.note_id=n.id)",
        Emojis => "(SELECT emoji FROM note_emoji e WHERE e.note_id=n.id)",
        FileTypes => "(SELECT mime_category FROM note_file f WHERE f.note_id=n.id)",
        _ => return Err(format!("field is not a set: {f:?}")),
    })
}

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
