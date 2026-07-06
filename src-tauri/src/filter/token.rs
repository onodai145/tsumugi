//! TQL の字句解析。docs/filter-dsl-design.md §1(EBNF)・§3(演算子)。
//! 単語演算子(contains/in/startswith/endswith/match)は Ident のままにし、パーサが解釈する。

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // 論理
    AndAnd, // &&
    OrOr,   // ||
    Not,    // !
    // 比較
    EqEq, // ==
    Ne,   // !=
    Lt,   // <
    Gt,   // >
    Le,   // <=
    Ge,   // >=
    Arrow,  // ->  (contains)
    RArrow, // <-  (in)
    // 算術
    Plus,
    Minus,
    Star,
    Slash,
    // 区切り
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    At, // @
    // リテラル/識別子
    Ident(String),
    Str(String),
    Num(f64),
}

/// 入力を字句列へ。失敗時はメッセージを返す。
pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let n = chars.len();
    let mut out = Vec::new();

    while i < n {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                i += 1;
            }
            '&' => {
                if i + 1 < n && chars[i + 1] == '&' {
                    out.push(Token::AndAnd);
                    i += 2;
                } else {
                    return Err("expected '&&'".into());
                }
            }
            '|' => {
                if i + 1 < n && chars[i + 1] == '|' {
                    out.push(Token::OrOr);
                    i += 2;
                } else {
                    return Err("expected '||'".into());
                }
            }
            '!' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    out.push(Token::Ne);
                    i += 2;
                } else {
                    out.push(Token::Not);
                    i += 1;
                }
            }
            '=' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    out.push(Token::EqEq);
                    i += 2;
                } else {
                    return Err("expected '==' (single '=' is not valid)".into());
                }
            }
            '<' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    out.push(Token::Le);
                    i += 2;
                } else if i + 1 < n && chars[i + 1] == '-' {
                    out.push(Token::RArrow);
                    i += 2;
                } else {
                    out.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    out.push(Token::Ge);
                    i += 2;
                } else {
                    out.push(Token::Gt);
                    i += 1;
                }
            }
            '-' => {
                if i + 1 < n && chars[i + 1] == '>' {
                    out.push(Token::Arrow);
                    i += 2;
                } else {
                    out.push(Token::Minus);
                    i += 1;
                }
            }
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '*' => {
                out.push(Token::Star);
                i += 1;
            }
            '/' => {
                out.push(Token::Slash);
                i += 1;
            }
            '(' => {
                out.push(Token::LParen);
                i += 1;
            }
            ')' => {
                out.push(Token::RParen);
                i += 1;
            }
            '[' => {
                out.push(Token::LBracket);
                i += 1;
            }
            ']' => {
                out.push(Token::RBracket);
                i += 1;
            }
            ',' => {
                out.push(Token::Comma);
                i += 1;
            }
            '.' => {
                out.push(Token::Dot);
                i += 1;
            }
            '@' => {
                out.push(Token::At);
                i += 1;
            }
            '"' => {
                // 文字列リテラル（\" と \\ をエスケープとして扱う）
                let mut s = String::new();
                i += 1;
                let mut closed = false;
                while i < n {
                    let ch = chars[i];
                    if ch == '\\' && i + 1 < n {
                        let next = chars[i + 1];
                        s.push(match next {
                            'n' => '\n',
                            't' => '\t',
                            other => other, // \" \\ など
                        });
                        i += 2;
                    } else if ch == '"' {
                        i += 1;
                        closed = true;
                        break;
                    } else {
                        s.push(ch);
                        i += 1;
                    }
                }
                if !closed {
                    return Err("unterminated string literal".into());
                }
                out.push(Token::Str(s));
            }
            '0'..='9' => {
                let start = i;
                while i < n && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    // 数値内の '.' は小数点。ただし後続が数字でなければ Dot として扱うため止める
                    if chars[i] == '.' && !(i + 1 < n && chars[i + 1].is_ascii_digit()) {
                        break;
                    }
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let num = num_str
                    .parse::<f64>()
                    .map_err(|_| format!("invalid number: {num_str}"))?;
                out.push(Token::Num(num));
            }
            c if is_ident_start(c) => {
                let start = i;
                while i < n && is_ident_part(chars[i]) {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                out.push(Token::Ident(s));
            }
            other => {
                return Err(format!("unexpected character: {other:?}"));
            }
        }
    }
    Ok(out)
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn is_ident_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::Token::*;
    use super::*;

    #[test]
    fn tokenizes_basic_query() {
        let t = tokenize("from home where has_files && !cw").unwrap();
        assert_eq!(
            t,
            vec![
                Ident("from".into()),
                Ident("home".into()),
                Ident("where".into()),
                Ident("has_files".into()),
                AndAnd,
                Not,
                Ident("cw".into()),
            ]
        );
    }

    #[test]
    fn operators_and_arrows() {
        let t = tokenize("reactions >= 10 || text -> \"Rust\" || \"a\" <- tags").unwrap();
        assert_eq!(
            t,
            vec![
                Ident("reactions".into()),
                Ge,
                Num(10.0),
                OrOr,
                Ident("text".into()),
                Arrow,
                Str("Rust".into()),
                OrOr,
                Str("a".into()),
                RArrow,
                Ident("tags".into()),
            ]
        );
    }

    #[test]
    fn dotted_field_and_account_and_set() {
        let t = tokenize("user.followers > 100 && @alice in [ \"x\" , \"y\" ]").unwrap();
        assert_eq!(
            t,
            vec![
                Ident("user".into()),
                Dot,
                Ident("followers".into()),
                Gt,
                Num(100.0),
                AndAnd,
                At,
                Ident("alice".into()),
                Ident("in".into()),
                LBracket,
                Str("x".into()),
                Comma,
                Str("y".into()),
                RBracket,
            ]
        );
    }

    #[test]
    fn ne_vs_not_and_decimal() {
        assert_eq!(tokenize("a != 1.5").unwrap(), vec![Ident("a".into()), Ne, Num(1.5)]);
        assert_eq!(tokenize("!renote").unwrap(), vec![Not, Ident("renote".into())]);
    }

    #[test]
    fn string_escapes_and_errors() {
        assert_eq!(tokenize(r#""a\"b""#).unwrap(), vec![Str("a\"b".into())]);
        assert!(tokenize(r#""unterminated"#).is_err());
        assert!(tokenize("=").is_err()); // 単独 '=' は不可
        assert!(tokenize("a & b").is_err()); // 単独 '&' は不可
    }
}
