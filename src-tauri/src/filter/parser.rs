//! TQL の構文解析（再帰下降）＋型検査。docs/design/filter-dsl-design.md §1・§8。
//!
//! 実装上のグラマー（§1 を実装容易に精緻化）:
//!   query   := "from" sources ("where" expr)?
//!   sources := source ("," source)*
//!   source  := IDENT ( "(" STRING ("," STRING)* ")" )?
//!   expr    := or
//!   or      := and ("||" and)*
//!   and     := not ("&&" not)*
//!   not     := "!" not | primary
//!   primary := "(" expr ")" | compare
//!   compare := add (compOp add)?
//!   add     := mul (("+"|"-") mul)*
//!   mul     := unary (("*"|"/") unary)*
//!   unary   := value               (算術の括弧は初期未対応)
//!   value   := field | STRING | NUMBER | set | account
//! ※ 算術の括弧 `(a+b)*c` は初期未対応（`(` は boolean グループとして解釈）。

use super::ast::*;
use super::token::{tokenize, Token};

/// `(` `!` `[` の入れ子の上限(Issue #605)。再帰下降パーサの呼び出し深さを抑える。
pub(crate) const MAX_NEST_DEPTH: usize = 64;
/// 構築する AST の深さの上限(Issue #605)。`a && a && ...` のような平らな連鎖も左に深い木になり、
/// 評価(`eval`)・SQL化(`sql`)・Drop が木の深さだけ再帰するため、入れ子とは別に抑える。
/// tokio ワーカーのスタック(2MB)で、デバッグビルド(1段 1〜2KB)でも十分な余裕が残る値。
pub(crate) const MAX_AST_DEPTH: usize = 256;

pub fn parse(input: &str) -> Result<Query, String> {
    let tokens = tokenize(input)?;
    let mut p = Parser::new(tokens);
    let q = p.parse_query()?;
    if p.pos != p.tokens.len() {
        return Err(format!("unexpected trailing tokens at {}", p.pos));
    }
    Ok(q)
}

/// where 節の述語式だけをパースする（カラムのフィルタ入力用。`from` は付けない）。
pub fn parse_predicate(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input)?;
    let mut p = Parser::new(tokens);
    let e = p.parse_expr()?;
    if p.pos != p.tokens.len() {
        return Err(format!("unexpected trailing tokens at {}", p.pos));
    }
    Ok(e)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// 今いる `(` `!` `[` の入れ子の深さ。`MAX_NEST_DEPTH` を超えたらエラーにする。
    nest: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            nest: 0,
        }
    }

    /// 再帰する構文(`(` `!` `[`)に入る。呼び出し側は、成功した経路では必ず `leave` を呼ぶ
    /// (失敗はパース全体の失敗になるので、エラー経路で戻す必要はない)。
    fn enter(&mut self) -> Result<(), String> {
        self.nest += 1;
        if self.nest > MAX_NEST_DEPTH {
            return Err(format!("TQL is nested too deeply (max {MAX_NEST_DEPTH})"));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.nest -= 1;
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn bump(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: &Token) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn is_ident(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(s)) if s == kw)
    }

    // ---- query / sources ----

    fn parse_query(&mut self) -> Result<Query, String> {
        if !self.eat_ident("from") {
            return Err("query must start with 'from'".into());
        }
        let sources = self.parse_sources()?;
        let predicate = if self.eat_ident("where") {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Query { sources, predicate })
    }

    fn eat_ident(&mut self, kw: &str) -> bool {
        if self.is_ident(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_sources(&mut self) -> Result<Vec<Source>, String> {
        let mut sources = vec![self.parse_source()?];
        while self.eat(&Token::Comma) {
            sources.push(self.parse_source()?);
        }
        Ok(sources)
    }

    fn parse_source(&mut self) -> Result<Source, String> {
        let name = match self.bump() {
            Some(Token::Ident(s)) => s,
            other => return Err(format!("expected source name, got {other:?}")),
        };
        if name == "where" {
            return Err("expected at least one source before 'where'".into());
        }
        // 引数付きソース: name("arg", ...)
        let mut args = Vec::new();
        if self.eat(&Token::LParen) {
            loop {
                match self.bump() {
                    Some(Token::Str(s)) => args.push(s),
                    other => {
                        return Err(format!("source argument must be a string, got {other:?}"))
                    }
                }
                if self.eat(&Token::Comma) {
                    continue;
                }
                if self.eat(&Token::RParen) {
                    break;
                }
                return Err("expected ',' or ')' in source arguments".into());
            }
        }
        map_source(&name, args)
    }

    // ---- expression ----

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_and()?;
        while self.eat(&Token::OrOr) {
            let rhs = self.parse_and()?;
            lhs = check_expr_depth(Expr::Or(Box::new(lhs), Box::new(rhs)))?;
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_not()?;
        while self.eat(&Token::AndAnd) {
            let rhs = self.parse_not()?;
            lhs = check_expr_depth(Expr::And(Box::new(lhs), Box::new(rhs)))?;
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::Not) {
            self.enter()?;
            let inner = self.parse_not()?;
            self.leave();
            check_expr_depth(Expr::Not(Box::new(inner)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::LParen) {
            self.enter()?;
            let e = self.parse_expr()?;
            if !self.eat(&Token::RParen) {
                return Err("expected ')'".into());
            }
            self.leave();
            Ok(e)
        } else {
            self.parse_compare()
        }
    }

    fn parse_compare(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_add()?;
        if let Some(op) = self.peek_compare_op() {
            self.pos += 1;
            let rhs = self.parse_add()?;
            check_compare(&lhs, op, &rhs)?;
            check_expr_depth(Expr::Compare { lhs, op, rhs })
        } else {
            check_bare(&lhs)?;
            Ok(Expr::Bare(lhs))
        }
    }

    fn peek_compare_op(&self) -> Option<CompareOp> {
        Some(match self.peek()? {
            Token::EqEq => CompareOp::Eq,
            Token::Ne => CompareOp::Ne,
            Token::Lt => CompareOp::Lt,
            Token::Gt => CompareOp::Gt,
            Token::Le => CompareOp::Le,
            Token::Ge => CompareOp::Ge,
            Token::Arrow => CompareOp::Contains,
            Token::RArrow => CompareOp::In,
            Token::Ident(s) => match s.as_str() {
                "contains" => CompareOp::Contains,
                "in" => CompareOp::In,
                "startswith" => CompareOp::StartsWith,
                "endswith" => CompareOp::EndsWith,
                "match" => CompareOp::Match,
                _ => return None,
            },
            _ => return None,
        })
    }

    fn parse_add(&mut self) -> Result<Value, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => ArithOp::Add,
                Some(Token::Minus) => ArithOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_mul()?;
            lhs = make_arith(lhs, op, rhs)?;
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Value, String> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => ArithOp::Mul,
                Some(Token::Slash) => ArithOp::Div,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_unary()?;
            lhs = make_arith(lhs, op, rhs)?;
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Value, String> {
        self.parse_value()
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(Token::Str(_)) => {
                let Some(Token::Str(s)) = self.bump() else {
                    unreachable!()
                };
                Ok(Value::Str(s))
            }
            Some(Token::Num(_)) => {
                let Some(Token::Num(n)) = self.bump() else {
                    unreachable!()
                };
                Ok(Value::Num(n))
            }
            Some(Token::At) => {
                self.pos += 1;
                match self.bump() {
                    Some(Token::Ident(s)) => Ok(Value::Account(s)),
                    other => Err(format!("expected account name after '@', got {other:?}")),
                }
            }
            Some(Token::LBracket) => self.parse_set(),
            Some(Token::Ident(_)) => {
                let name = self.parse_field_name()?;
                match Field::from_name(&name) {
                    Some(f) => Ok(Value::Field(f)),
                    None => Err(format!("unknown field: {name}")),
                }
            }
            other => Err(format!("expected a value, got {other:?}")),
        }
    }

    /// IDENT ("." IDENT)* を "a.b" 文字列に組み立てる。
    fn parse_field_name(&mut self) -> Result<String, String> {
        let mut name = match self.bump() {
            Some(Token::Ident(s)) => s,
            other => return Err(format!("expected identifier, got {other:?}")),
        };
        while self.eat(&Token::Dot) {
            match self.bump() {
                Some(Token::Ident(s)) => {
                    name.push('.');
                    name.push_str(&s);
                }
                other => return Err(format!("expected identifier after '.', got {other:?}")),
            }
        }
        Ok(name)
    }

    fn parse_set(&mut self) -> Result<Value, String> {
        // '[' ( value (',' value)* )? ']'
        self.pos += 1; // '['
        self.enter()?;
        let mut items = Vec::new();
        if self.eat(&Token::RBracket) {
            self.leave();
            return Ok(Value::Set(items));
        }
        loop {
            items.push(self.parse_value()?);
            if self.eat(&Token::Comma) {
                continue;
            }
            if self.eat(&Token::RBracket) {
                break;
            }
            return Err("expected ',' or ']' in set literal".into());
        }
        self.leave();
        Ok(Value::Set(items))
    }
}

fn map_source(name: &str, mut args: Vec<String>) -> Result<Source, String> {
    let arg1 = |args: &mut Vec<String>| -> Result<String, String> {
        if args.len() != 1 {
            return Err(format!("source '{name}' takes exactly one argument"));
        }
        Ok(args.remove(0))
    };
    Ok(match name {
        "home" => Source::Home,
        "local" => Source::Local,
        "hybrid" | "social" => Source::Hybrid,
        "global" => Source::Global,
        "mentions" => Source::Mentions,
        "cache" | "local_cache" => Source::Cache,
        "list" => Source::List(arg1(&mut args)?),
        "antenna" => Source::Antenna(arg1(&mut args)?),
        "channel" => Source::Channel(arg1(&mut args)?),
        "user" => Source::User(arg1(&mut args)?),
        "tag" => Source::Tag(arg1(&mut args)?),
        "search" => Source::Search(arg1(&mut args)?),
        _ => return Err(format!("unknown source: {name}")),
    })
}

// ---- 型検査（§8） ----

fn value_types(v: &Value) -> Vec<FilterType> {
    use FilterType::*;
    match v {
        Value::Field(f) => f.supported_types().to_vec(),
        Value::Str(_) | Value::Account(_) => vec![String],
        Value::Num(_) => vec![Numeric],
        Value::Set(_) => vec![Set],
        Value::Arith { .. } => vec![Numeric],
    }
}

fn has_type(v: &Value, t: FilterType) -> bool {
    value_types(v).contains(&t)
}

fn make_arith(lhs: Value, op: ArithOp, rhs: Value) -> Result<Value, String> {
    if !has_type(&lhs, FilterType::Numeric) || !has_type(&rhs, FilterType::Numeric) {
        return Err("arithmetic requires numeric operands".into());
    }
    check_value_depth(Value::Arith {
        lhs: Box::new(lhs),
        op,
        rhs: Box::new(rhs),
    })
}

// ---- AST の深さの上限（Issue #605） ----
//
// `a && a && ...` や `a + a + ...` はループで左に深い木を作るので、パーサの再帰では抑えられない。
// 木を作るたびに深さを測り、上限を超える木は返さずにエラーにする。測る再帰は、直前まで上限以内だった
// 木の上に1段積んだだけなので、深くても `MAX_AST_DEPTH` 段で止まる。

fn check_expr_depth(e: Expr) -> Result<Expr, String> {
    if expr_depth(&e) > MAX_AST_DEPTH {
        return Err(too_deep());
    }
    Ok(e)
}

fn check_value_depth(v: Value) -> Result<Value, String> {
    if value_depth(&v) > MAX_AST_DEPTH {
        return Err(too_deep());
    }
    Ok(v)
}

fn too_deep() -> String {
    format!("TQL expression is too deep (max {MAX_AST_DEPTH})")
}

fn expr_depth(e: &Expr) -> usize {
    1 + match e {
        Expr::Or(a, b) | Expr::And(a, b) => expr_depth(a).max(expr_depth(b)),
        Expr::Not(a) => expr_depth(a),
        Expr::Compare { lhs, rhs, .. } => value_depth(lhs).max(value_depth(rhs)),
        Expr::Bare(v) => value_depth(v),
    }
}

fn value_depth(v: &Value) -> usize {
    1 + match v {
        Value::Arith { lhs, rhs, .. } => value_depth(lhs).max(value_depth(rhs)),
        Value::Set(items) => items.iter().map(value_depth).max().unwrap_or(0),
        _ => 0,
    }
}

fn check_bare(v: &Value) -> Result<(), String> {
    match v {
        Value::Field(f) if f.supports(FilterType::Boolean) => Ok(()),
        _ => Err("bare predicate must be a boolean field".into()),
    }
}

fn check_compare(lhs: &Value, op: CompareOp, rhs: &Value) -> Result<(), String> {
    use CompareOp::*;
    use FilterType::*;
    let ok = match op {
        Eq | Ne => {
            (has_type(lhs, Numeric) && has_type(rhs, Numeric))
                || (has_type(lhs, String) && has_type(rhs, String))
        }
        Lt | Gt | Le | Ge => has_type(lhs, Numeric) && has_type(rhs, Numeric),
        StartsWith | EndsWith | Match => has_type(lhs, String) && has_type(rhs, String),
        // `->`: 集合が要素を含む、または文字列が部分文字列を含む
        Contains => has_type(lhs, Set) || (has_type(lhs, String) && has_type(rhs, String)),
        // `<-`: 要素が集合に含まれる（右辺が集合）
        In => has_type(rhs, Set),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("type mismatch for operator {op:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sources_only() {
        let q = parse("from home").unwrap();
        assert_eq!(q.sources, vec![Source::Home]);
        assert!(q.predicate.is_none());
    }

    #[test]
    fn parses_multi_sources_with_args() {
        let q = parse("from local, hybrid, list(\"abc\")").unwrap();
        assert_eq!(
            q.sources,
            vec![Source::Local, Source::Hybrid, Source::List("abc".into())]
        );
    }

    #[test]
    fn parses_boolean_predicate() {
        let q = parse("from home where has_files && !cw && !bot").unwrap();
        assert!(q.predicate.is_some());
    }

    #[test]
    fn parses_numeric_and_set_compares() {
        parse("from local where reactions >= 10").unwrap();
        parse("from home where \"👍\" <- reactions").unwrap();
        parse("from home where text -> \"Rust\" && remote").unwrap();
        parse("from home where reactions + renotes > 5").unwrap();
    }

    #[test]
    fn parses_grouping_and_account() {
        parse("from home where (renote || quote) && !bot").unwrap();
        parse("from mentions where to_me && !reacted").unwrap();
        parse("from home where @me in mentions").unwrap();
    }

    #[test]
    fn type_errors_are_rejected() {
        // bare 非 boolean
        assert!(parse("from home where reactions").is_err());
        // 数値比較に文字列
        assert!(parse("from home where text >= 10").is_err());
        // in の右辺が集合でない
        assert!(parse("from home where \"x\" <- text").is_err());
        // 算術に文字列
        assert!(parse("from home where text + 1 > 0").is_err());
        // 未知フィールド
        assert!(parse("from home where nope == 1").is_err());
        // 未知ソース
        assert!(parse("from nope").is_err());
    }

    #[test]
    fn string_ops_and_regex() {
        parse("from home where text startswith \"hi\"").unwrap();
        parse("from cache where text match \"(?i)misskey\"").unwrap();
        parse("from home where user.acct endswith \"@example.com\"").unwrap();
    }

    // ---- 深さの上限(Issue #605) ----

    fn pred(expr: &str) -> Result<Query, String> {
        parse(&format!("from home where {expr}"))
    }

    fn chain(term: &str, op: &str, n: usize) -> String {
        vec![term; n].join(&format!(" {op} "))
    }

    #[test]
    fn nested_parens_are_limited() {
        let nest = |n: usize| format!("{}cw{}", "(".repeat(n), ")".repeat(n));
        assert!(pred(&nest(MAX_NEST_DEPTH)).is_ok());
        assert!(pred(&nest(MAX_NEST_DEPTH + 1)).is_err());
    }

    #[test]
    fn nested_nots_are_limited() {
        let nest = |n: usize| format!("{}cw", "!".repeat(n));
        assert!(pred(&nest(MAX_NEST_DEPTH)).is_ok());
        assert!(pred(&nest(MAX_NEST_DEPTH + 1)).is_err());
    }

    #[test]
    fn nested_set_literals_are_limited() {
        let nest = |n: usize| format!("\"x\" <- {}{}", "[".repeat(n), "]".repeat(n));
        assert!(pred(&nest(MAX_NEST_DEPTH)).is_ok());
        assert!(pred(&nest(MAX_NEST_DEPTH + 1)).is_err());
    }

    #[test]
    fn nesting_counts_every_construct_together() {
        // `(!` を33回 = 括弧33 + 否定33 = 66段
        let q = format!("{}cw{}", "(!".repeat(33), ")".repeat(33));
        assert!(pred(&q).is_err());
    }

    #[test]
    fn flat_logical_chains_are_limited() {
        // 項が N 個の連鎖は、葉(Bare)の深さ2 + 連結 N-1 段 = N+1 段
        for op in ["&&", "||"] {
            assert!(pred(&chain("cw", op, MAX_AST_DEPTH - 1)).is_ok(), "{op}");
            assert!(pred(&chain("cw", op, MAX_AST_DEPTH)).is_err(), "{op}");
        }
    }

    #[test]
    fn flat_arithmetic_chains_are_limited() {
        for op in ["+", "-", "*", "/"] {
            let q = |n: usize| format!("{} > 0", chain("reactions", op, n));
            assert!(pred(&q(MAX_AST_DEPTH - 1)).is_ok(), "{op}");
            assert!(pred(&q(MAX_AST_DEPTH)).is_err(), "{op}");
        }
    }

    #[test]
    fn the_depth_limit_covers_chains_inside_nesting() {
        // 入れ子は上限以内でも、各段に連鎖があれば AST は深くなる
        // 入れ子60段(上限以内)、各段で左に5項つなぐ: 5 * 60 = 300 段
        let q = format!(
            "{}cw{}",
            "(".repeat(60),
            " && cw && cw && cw && cw && cw)".repeat(60)
        );
        assert!(pred(&q).is_err());
    }

    #[test]
    fn deep_inputs_are_rejected_without_overflowing_the_stack() {
        // 入力長の上限(`MAX_INPUT_CHARS`)に収まる最大級の入力。修正前は、パース中かドロップ中に
        // スタックが溢れてプロセスごと落ちる(ここでは小さいスタックのスレッドで試す)。
        let inputs = [
            format!("{}cw{}", "(".repeat(5_000), ")".repeat(5_000)),
            format!("{}cw", "!".repeat(16_000)),
            format!("\"x\" <- {}{}", "[".repeat(5_000), "]".repeat(5_000)),
            format!("cw{}", "&&cw".repeat(3_900)),
            format!("cw{}", "||cw".repeat(3_900)),
            format!("reactions{} > 0", "+reactions".repeat(1_500)),
        ];
        for q in inputs {
            assert!(q.chars().count() <= crate::filter::token::MAX_INPUT_CHARS);
            let h = std::thread::Builder::new()
                .stack_size(512 * 1024)
                .spawn(move || {
                    assert!(pred(&q).is_err());
                    assert!(parse_predicate(&q).is_err());
                })
                .unwrap();
            h.join().unwrap();
        }
    }

    #[test]
    fn overlong_input_is_rejected_by_both_entry_points() {
        let q = format!("cw{}", " ".repeat(crate::filter::token::MAX_INPUT_CHARS));
        assert!(pred(&q).is_err());
        assert!(parse_predicate(&q).is_err());
    }

    #[test]
    fn ordinary_deep_filters_still_parse() {
        // 保存済みのカラムが上限で壊れないこと: 実用的な長さの連鎖と入れ子
        assert!(pred(&chain("text -> \"a\"", "||", 100)).is_ok());
        assert!(pred("((renote || quote) && !(bot || cw)) && !reply").is_ok());
    }
}
