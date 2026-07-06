//! TQL の構文解析（再帰下降）＋型検査。docs/filter-dsl-design.md §1・§8。
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

pub fn parse(input: &str) -> Result<Query, String> {
    let tokens = tokenize(input)?;
    let mut p = Parser { tokens, pos: 0 };
    let q = p.parse_query()?;
    if p.pos != p.tokens.len() {
        return Err(format!("unexpected trailing tokens at {}", p.pos));
    }
    Ok(q)
}

/// where 節の述語式だけをパースする（カラムのフィルタ入力用。`from` は付けない）。
pub fn parse_predicate(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input)?;
    let mut p = Parser { tokens, pos: 0 };
    let e = p.parse_expr()?;
    if p.pos != p.tokens.len() {
        return Err(format!("unexpected trailing tokens at {}", p.pos));
    }
    Ok(e)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
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
                    other => return Err(format!("source argument must be a string, got {other:?}")),
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
            lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_not()?;
        while self.eat(&Token::AndAnd) {
            let rhs = self.parse_not()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::Not) {
            Ok(Expr::Not(Box::new(self.parse_not()?)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::LParen) {
            let e = self.parse_expr()?;
            if !self.eat(&Token::RParen) {
                return Err("expected ')'".into());
            }
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
            Ok(Expr::Compare { lhs, op, rhs })
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
                let Some(Token::Str(s)) = self.bump() else { unreachable!() };
                Ok(Value::Str(s))
            }
            Some(Token::Num(_)) => {
                let Some(Token::Num(n)) = self.bump() else { unreachable!() };
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
        let mut items = Vec::new();
        if self.eat(&Token::RBracket) {
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
    Ok(Value::Arith {
        lhs: Box::new(lhs),
        op,
        rhs: Box::new(rhs),
    })
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
}
