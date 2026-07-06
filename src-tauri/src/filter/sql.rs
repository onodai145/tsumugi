//! TQL の SQL 射影。docs/filter-dsl-design.md §10・§11。
//! AST → SQLite WHERE 句（`cache` ソースの検索用）。`note n JOIN user u` を前提とする。
//!
//! 生成物はプレースホルダ `?` とバインド値 [`SqlParam`] の列。context 依存(mine/following/
//! to_me/@account) は my_ids / following_ids を `?` として展開する。
//! 注意(v1): リアクションのホスト吸収正規化・LIKE のワイルドカードエスケープは未対応。

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

pub struct SqlWhere {
    pub sql: String,
    pub params: Vec<SqlParam>,
}

pub fn build_where(expr: &Expr, ctx: &SqlCtx) -> Result<SqlWhere, String> {
    let mut b = Builder {
        ctx,
        params: Vec::new(),
    };
    let sql = b.expr(expr)?;
    Ok(SqlWhere {
        sql,
        params: b.params,
    })
}

struct Builder<'a> {
    ctx: &'a SqlCtx,
    params: Vec<SqlParam>,
}

impl Builder<'_> {
    fn push_text(&mut self, s: String) -> &'static str {
        self.params.push(SqlParam::Text(s));
        "?"
    }
    fn push_real(&mut self, x: f64) -> &'static str {
        self.params.push(SqlParam::Real(x));
        "?"
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
            self.params.push(SqlParam::Text(id.clone()));
            s.push('?');
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
            StartsWith => {
                let col = self.str(lhs)?;
                let pat = like_value(rhs)? + "%";
                let ph = self.push_text(pat);
                format!("({col} LIKE {ph})")
            }
            EndsWith => {
                let col = self.str(lhs)?;
                let pat = format!("%{}", like_value(rhs)?);
                let ph = self.push_text(pat);
                format!("({col} LIKE {ph})")
            }
            Match => {
                let col = self.str(lhs)?;
                let ph = self.str(rhs)?;
                format!("({col} REGEXP {ph})")
            }
            Contains => {
                if set_capable(lhs) {
                    // 集合が要素を含む: rhs IN (集合サブクエリ)
                    let elem = self.str(rhs)?;
                    let sub = self.set_subquery(lhs)?;
                    format!("({elem} IN {sub})")
                } else {
                    // 文字列部分一致
                    let col = self.str(lhs)?;
                    let pat = format!("%{}%", like_value(rhs)?);
                    let ph = self.push_text(pat);
                    format!("({col} LIKE {ph})")
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

    fn num(&mut self, v: &Value) -> Result<String, String> {
        Ok(match v {
            Value::Num(x) => self.push_real(*x).to_string(),
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
            Value::Str(s) => self.push_text(s.clone()).to_string(),
            Value::Account(a) => self.push_text(a.clone()).to_string(),
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

fn like_value(v: &Value) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        _ => Err("LIKE operand must be a string literal".into()),
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

    #[test]
    fn boolean_and_numeric() {
        let w = where_of("from cache where has_files && !cw");
        assert_eq!(w.sql, "(n.files_count > 0 AND (NOT n.cw IS NOT NULL))");
        assert!(w.params.is_empty());

        let w = where_of("from cache where reactions >= 10");
        assert_eq!(w.sql, "(n.reaction_count >= ?)");
        assert_eq!(w.params, vec![SqlParam::Real(10.0)]);
    }

    #[test]
    fn arith_and_string() {
        let w = where_of("from cache where reactions + renotes > 5");
        assert_eq!(w.sql, "((n.reaction_count + n.renote_count) > ?)");

        let w = where_of("from cache where text -> \"Rust\"");
        assert_eq!(w.sql, "(COALESCE(n.text,'') LIKE ?)");
        assert_eq!(w.params, vec![SqlParam::Text("%Rust%".into())]);

        let w = where_of("from cache where text startswith \"hi\"");
        assert_eq!(w.params, vec![SqlParam::Text("hi%".into())]);
    }

    #[test]
    fn set_membership_and_regex() {
        let w = where_of("from cache where \"👍\" <- reactions");
        assert_eq!(
            w.sql,
            "(? IN (SELECT emoji_key FROM note_reaction r WHERE r.note_id=n.id))"
        );
        assert_eq!(w.params, vec![SqlParam::Text("👍".into())]);

        let w = where_of("from cache where text match \"(?i)mi\"");
        assert_eq!(w.sql, "(COALESCE(n.text,'') REGEXP ?)");
    }

    #[test]
    fn context_fields_expand_ids() {
        let w = where_of("from cache where mine");
        assert_eq!(w.sql, "n.user_id IN (?)");
        assert_eq!(w.params, vec![SqlParam::Text("me1".into())]);

        let w = where_of("from cache where following");
        assert_eq!(w.sql, "n.user_id IN (?)");
        assert_eq!(w.params, vec![SqlParam::Text("f1".into())]);

        let w = where_of("from cache where to_me");
        assert!(w.sql.contains("note_mention m WHERE m.note_id=n.id AND m.user_id IN (?)"));
    }
}
