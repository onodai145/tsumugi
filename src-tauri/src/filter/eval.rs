//! TQL のインメモリ評価。docs/filter-dsl-design.md §10。
//! Streaming 受信時に `evaluate(&Expr, &Note, &EvalContext) -> bool` で判定する。

use super::ast::*;
use crate::domain::{Note, Visibility};
use std::collections::HashSet;

/// `mine` / `following` / `to_me` 等の解決に要る文脈（§7）。
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    /// 全ログインアカウントの userId
    pub my_user_ids: HashSet<String>,
    /// フォロー中 userId（取得済みなら Some）
    pub following_ids: Option<HashSet<String>>,
    /// 受信アカウントのインスタンス host（host 比較用）
    pub local_host: Option<String>,
}

/// predicate が None のクエリは常に true（呼び出し側で扱う）。ここでは Expr を評価。
pub fn evaluate(expr: &Expr, note: &Note, ctx: &EvalContext) -> bool {
    match expr {
        Expr::Or(a, b) => evaluate(a, note, ctx) || evaluate(b, note, ctx),
        Expr::And(a, b) => evaluate(a, note, ctx) && evaluate(b, note, ctx),
        Expr::Not(a) => !evaluate(a, note, ctx),
        Expr::Bare(v) => eval_bool(v, note, ctx),
        Expr::Compare { lhs, op, rhs } => eval_compare(lhs, *op, rhs, note, ctx),
    }
}

fn eval_compare(lhs: &Value, op: CompareOp, rhs: &Value, n: &Note, ctx: &EvalContext) -> bool {
    use CompareOp::*;
    match op {
        Lt | Gt | Le | Ge => {
            let (l, r) = (eval_num(lhs, n), eval_num(rhs, n));
            match op {
                Lt => l < r,
                Gt => l > r,
                Le => l <= r,
                Ge => l >= r,
                _ => unreachable!(),
            }
        }
        Eq | Ne => {
            let eq = if numeric(lhs) && numeric(rhs) {
                eval_num(lhs, n) == eval_num(rhs, n)
            } else if let Some(m) = account_eq(lhs, rhs, n, ctx) {
                m
            } else {
                eval_str(lhs, n, ctx) == eval_str(rhs, n, ctx)
            };
            if op == Eq {
                eq
            } else {
                !eq
            }
        }
        StartsWith => eval_str(lhs, n, ctx).starts_with(&eval_str(rhs, n, ctx)),
        EndsWith => eval_str(lhs, n, ctx).ends_with(&eval_str(rhs, n, ctx)),
        Match => regex::Regex::new(&eval_str(rhs, n, ctx))
            .map(|re| re.is_match(&eval_str(lhs, n, ctx)))
            .unwrap_or(false),
        // `->`: 集合が要素を含む、または文字列が部分文字列を含む
        Contains => {
            if set_capable(lhs) {
                let set = eval_set(lhs, n, ctx);
                set.contains(&normalize_reaction(&eval_str(rhs, n, ctx)))
            } else {
                eval_str(lhs, n, ctx).contains(&eval_str(rhs, n, ctx))
            }
        }
        // `<-`: 要素が集合に含まれる（右辺が集合）
        In => {
            let set = eval_set(rhs, n, ctx);
            if let Value::Account(_) = lhs {
                // @me <- mentions : 自分のいずれかが集合に含まれるか
                !ctx.my_user_ids.is_disjoint(&set)
            } else {
                set.contains(&normalize_reaction(&eval_str(lhs, n, ctx)))
            }
        }
    }
}

fn numeric(v: &Value) -> bool {
    matches!(v, Value::Num(_) | Value::Arith { .. })
        || matches!(v, Value::Field(f) if f.supports(FilterType::Numeric))
}

fn set_capable(v: &Value) -> bool {
    matches!(v, Value::Set(_))
        || matches!(v, Value::Field(f) if f.supports(FilterType::Set))
}

/// Account を含む Eq/Ne の特別扱い（`user.id == @me` 等）。該当しなければ None。
fn account_eq(lhs: &Value, rhs: &Value, n: &Note, ctx: &EvalContext) -> Option<bool> {
    let (other, is_account) = match (lhs, rhs) {
        (Value::Account(_), _) => (rhs, true),
        (_, Value::Account(_)) => (lhs, true),
        _ => (lhs, false),
    };
    if !is_account {
        return None;
    }
    let s = eval_str(other, n, ctx);
    Some(ctx.my_user_ids.contains(&s))
}

fn eval_bool(v: &Value, n: &Note, ctx: &EvalContext) -> bool {
    let Value::Field(f) = v else { return false };
    use Field::*;
    match f {
        Renote => n.renote_id.is_some() && n.text.is_none(),
        Quote => n.renote_id.is_some() && n.text.is_some(),
        Reply => n.reply_id.is_some(),
        HasFiles => !n.files.is_empty(),
        HasPoll => n.poll.is_some(),
        Cw => n.cw.is_some(),
        Sensitive => n.files.iter().any(|f| f.is_sensitive),
        Local => n.user.host.is_none(),
        Remote => n.user.host.is_some(),
        Bot => n.user.is_bot,
        Cat => n.user.is_cat,
        Direct => n.visibility == Visibility::Specified,
        ToMe => n.mentions.iter().any(|m| ctx.my_user_ids.contains(m)),
        // reply_user_id は domain::Note に無いため未対応（常に false）
        ReplyToMe => false,
        HasMention => !n.mentions.is_empty(),
        HasLink => n.text.as_deref().map(has_url).unwrap_or(false),
        Pinned => n.is_pinned,
        Reacted => n.my_reaction.is_some(),
        Renoted => n.is_renoted_by_me,
        Favorited => n.is_favorited_by_me,
        Mine => ctx.my_user_ids.contains(&n.user.id),
        Following => ctx
            .following_ids
            .as_ref()
            .map(|s| s.contains(&n.user.id))
            .unwrap_or(false),
        _ => false,
    }
}

fn eval_num(v: &Value, n: &Note) -> f64 {
    match v {
        Value::Num(x) => *x,
        Value::Arith { lhs, op, rhs } => {
            let (l, r) = (eval_num(lhs, n), eval_num(rhs, n));
            match op {
                ArithOp::Add => l + r,
                ArithOp::Sub => l - r,
                ArithOp::Mul => l * r,
                ArithOp::Div => {
                    if r == 0.0 {
                        0.0
                    } else {
                        l / r
                    }
                }
            }
        }
        Value::Field(f) => {
            use Field::*;
            match f {
                Reactions => n.reaction_count as f64,
                Renotes => n.renote_count as f64,
                Replies => n.reply_count as f64,
                Files => n.files.len() as f64,
                Length => n.text.as_deref().unwrap_or("").chars().count() as f64,
                CreatedAt => n.created_at as f64,
                UserFollowers => n.user.followers_count as f64,
                UserFollowing => n.user.following_count as f64,
                UserNotes => n.user.notes_count as f64,
                _ => 0.0,
            }
        }
        _ => 0.0,
    }
}

fn eval_str(v: &Value, n: &Note, _ctx: &EvalContext) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Account(a) => a.clone(),
        Value::Field(f) => {
            use Field::*;
            match f {
                Text => n.text.clone().unwrap_or_default(),
                CwText => n.cw.clone().unwrap_or_default(),
                Via => n.via.clone().unwrap_or_default(),
                Host => n.user.host.clone().unwrap_or_default(),
                VisibilityStr => visibility_str(n.visibility).to_string(),
                Channel => n.channel_id.clone().unwrap_or_default(),
                Lang => n.lang.clone().unwrap_or_default(),
                ReplyId => n.reply_id.clone().unwrap_or_default(),
                RenoteId => n.renote_id.clone().unwrap_or_default(),
                UserUsername => n.user.username.clone(),
                UserAcct => n.user.acct(),
                UserName => n.user.name.clone().unwrap_or_default(),
                UserId => n.user.id.clone(),
                _ => String::new(),
            }
        }
        _ => String::new(),
    }
}

fn eval_set(v: &Value, n: &Note, ctx: &EvalContext) -> HashSet<String> {
    match v {
        Value::Set(items) => items
            .iter()
            .map(|it| normalize_reaction(&eval_str(it, n, ctx)))
            .collect(),
        Value::Field(f) => {
            use Field::*;
            match f {
                Reactions => n.reactions.keys().map(|k| normalize_reaction(k)).collect(),
                Tags => n.tags.iter().cloned().collect(),
                Mentions => n.mentions.iter().cloned().collect(),
                Emojis => n.emojis.keys().cloned().collect(),
                FileTypes => n
                    .files
                    .iter()
                    .map(|f| mime_category(&f.mime_type).to_string())
                    .collect(),
                _ => HashSet::new(),
            }
        }
        _ => HashSet::new(),
    }
}

fn visibility_str(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "public",
        Visibility::Home => "home",
        Visibility::Followers => "followers",
        Visibility::Specified => "specified",
    }
}

fn mime_category(mime: &str) -> &str {
    mime.split('/').next().unwrap_or("other")
}

fn has_url(text: &str) -> bool {
    text.contains("http://") || text.contains("https://")
}

/// カスタム絵文字 `:name@host:` はホスト部を吸収して `:name:` にする（§3.4）。
/// Unicode 絵文字やそれ以外の文字列はそのまま。
fn normalize_reaction(key: &str) -> String {
    if key.starts_with(':') && key.ends_with(':') {
        if let Some(at) = key.find('@') {
            return format!("{}:", &key[..at]);
        }
    }
    key.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DriveFile, User};
    use crate::filter::parser::parse;
    use std::collections::HashMap;

    fn ctx() -> EvalContext {
        EvalContext {
            my_user_ids: HashSet::from(["me1".to_string()]),
            following_ids: Some(HashSet::from(["friend".to_string()])),
            local_host: None,
        }
    }

    fn base_note() -> Note {
        Note {
            id: "n1".into(),
            created_at: 1000,
            text: Some("hello https://ex.com #rust @bob".into()),
            cw: None,
            visibility: Visibility::Public,
            local_only: false,
            user: User {
                id: "u1".into(),
                username: "alice".into(),
                host: None,
                name: Some("Alice".into()),
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 100,
                following_count: 10,
                notes_count: 5,
            },
            reply_id: None,
            renote_id: None,
            renote: None,
            files: vec![DriveFile {
                id: "f".into(),
                mime_type: "image/png".into(),
                is_sensitive: false,
                url: "u".into(),
                thumbnail_url: None,
            }],
            poll: None,
            tags: vec!["rust".into()],
            mentions: vec!["bob".into()],
            emojis: std::collections::HashMap::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: HashMap::from([("👍".into(), 12u32), (":blobcat@misskey.io:".into(), 3)]),
            reaction_count: 15,
            renote_count: 2,
            reply_count: 1,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    fn matches(query: &str, note: &Note) -> bool {
        let q = parse(query).unwrap();
        evaluate(q.predicate.as_ref().unwrap(), note, &ctx())
    }

    #[test]
    fn boolean_predicates() {
        let n = base_note();
        assert!(matches("from home where has_files && !cw && !bot", &n));
        assert!(matches("from home where local && has_link && has_mention", &n));
        assert!(!matches("from home where renote", &n));
        assert!(matches("from home where !renote", &n));
    }

    #[test]
    fn numeric_and_arith() {
        let n = base_note();
        assert!(matches("from home where reactions >= 10", &n));
        assert!(!matches("from home where reactions > 100", &n));
        assert!(matches("from home where reactions + renotes > 16", &n)); // 15+2=17
        assert!(matches("from home where user.followers >= 100", &n));
        assert!(matches("from home where created_at < 2000", &n));
    }

    #[test]
    fn string_ops() {
        let n = base_note();
        assert!(matches("from home where text -> \"hello\"", &n));
        assert!(matches("from home where text startswith \"hello\"", &n));
        assert!(matches("from home where text match \"(?i)RUST\"", &n));
        assert!(matches("from home where user.username == \"alice\"", &n));
        assert!(matches("from home where visibility == \"public\"", &n));
        assert!(!matches("from home where host == \"misskey.io\"", &n)); // local → host ""
    }

    #[test]
    fn set_membership_and_reaction_normalize() {
        let n = base_note();
        assert!(matches("from home where \"👍\" <- reactions", &n));
        assert!(matches("from home where \"rust\" <- tags", &n));
        assert!(matches("from home where \"bob\" <- mentions", &n));
        assert!(matches("from home where \"image\" <- file_types", &n));
        // カスタム絵文字はホスト吸収して :blobcat: でマッチ
        assert!(matches("from home where \":blobcat:\" <- reactions", &n));
        assert!(!matches("from home where \":nope:\" <- reactions", &n));
    }

    #[test]
    fn context_dependent() {
        let mut n = base_note();
        n.user.id = "me1".into(); // 自分の投稿
        assert!(matches("from home where mine", &n));
        n.user.id = "friend".into();
        assert!(matches("from home where following", &n));
        n.mentions = vec!["me1".into()];
        assert!(matches("from mentions where to_me", &n));
        assert!(matches("from home where @me in mentions", &n));
    }

    #[test]
    fn grouping_and_not() {
        let n = base_note();
        assert!(matches("from home where (renote || has_files) && !bot", &n));
        assert!(!matches("from home where (bot || cat) && has_files", &n));
    }
}
