//! TQL入力補完。docs/design/filter-dsl-design.md の文法に基づき、カーソル位置までの
//! 部分入力を文脈分類し、キーワード/ソース名/フィールド名/演算子の候補を返す。
//! list/antenna/channel の引数(実ID)はここでは扱わない(フロント側で別途解決、
//! docs/superpowers/specs/2026-08-05-tql-autocomplete-design.md §2)。

use super::ast::Field;
use super::token::{tokenize, Token};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TqlEditMode {
    /// from ... where ... のフルクエリ(エキスパートモードのtextarea)
    Query,
    /// where 述語のみ(簡単モードのfilter input)
    Predicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TqlCompletionKind {
    Keyword,
    Source,
    Field,
    Operator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TqlCompletionItem {
    pub label: String,
    pub insert: String,
    pub kind: TqlCompletionKind,
}

const ARGLESS_SOURCES: &[&str] = &["home", "local", "hybrid", "global", "mentions", "cache"];
const ARG_SOURCES: &[&str] = &["list", "antenna", "channel", "user", "tag", "search"];

const FIELD_NAMES: &[&str] = &[
    "renote", "quote", "reply", "has_files", "has_poll", "cw", "sensitive", "local", "remote",
    "bot", "cat", "direct", "to_me", "reply_to_me", "has_mention", "has_link", "pinned",
    "reacted", "renoted", "favorited", "mine", "following", "reactions", "renotes", "replies",
    "files", "length", "created_at", "user.followers", "user.following", "user.notes", "text",
    "cw_text", "via", "host", "visibility", "channel", "lang", "reply_id", "renote_id",
    "user.username", "user.acct", "user.name", "user.id", "tags", "mentions", "emojis",
    "file_types",
];

const WORD_OPERATORS: &[&str] = &["contains", "in", "startswith", "endswith", "match"];
const SYMBOL_OPERATORS: &[&str] = &["==", "!=", "<", ">", "<=", ">=", "->", "<-"];
const LOGIC_OPERATORS: &[&str] = &["&&", "||"];

/// `text` の先頭から `cursor_chars`（Unicodeコードポイント数。フロントがJSのUTF-16
/// カーソル位置から変換して渡す）文字目までを文脈分類し、前方一致する補完候補を返す。
/// tokenize失敗時は空配列を返す(入力途中の不正な文字列でも落とさない)。
pub fn complete(text: &str, cursor_chars: usize, mode: TqlEditMode) -> Vec<TqlCompletionItem> {
    let cursor = char_offset_to_byte(text, cursor_chars);
    let (word_start, partial) = current_word(text, cursor);
    let prefix = &text[..word_start];
    let Ok(tokens) = tokenize(prefix) else {
        return Vec::new();
    };

    let partial_lower = partial.to_lowercase();
    classify(&tokens, mode)
        .into_iter()
        .filter(|item| item.label.to_lowercase().starts_with(&partial_lower))
        .collect()
}

fn char_offset_to_byte(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// カーソル直前の識別子文字列(ASCII英数字と'_')を後方スキャンで切り出す。
/// ASCIIバイトだけを見て止まるため、常にUTF-8の文字境界上で止まる
/// (マルチバイト文字のバイトは is_ascii_alphanumeric() が常にfalseなので跨がない)。
fn current_word(text: &str, cursor: usize) -> (usize, String) {
    let bytes = text.as_bytes();
    let mut start = cursor;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }
    (start, text[start..cursor].to_string())
}

fn classify(tokens: &[Token], mode: TqlEditMode) -> Vec<TqlCompletionItem> {
    if mode == TqlEditMode::Predicate {
        return predicate_context(tokens);
    }
    let has_from = tokens.iter().any(|t| matches!(t, Token::Ident(s) if s == "from"));
    if !has_from {
        return if tokens.is_empty() {
            vec![keyword_item("from")]
        } else {
            Vec::new()
        };
    }
    let has_where = tokens.iter().any(|t| matches!(t, Token::Ident(s) if s == "where"));
    if !has_where {
        return source_context(tokens);
    }
    predicate_context(tokens)
}

fn source_context(tokens: &[Token]) -> Vec<TqlCompletionItem> {
    match tokens.last() {
        Some(Token::Ident(s)) if s == "from" => source_items(),
        Some(Token::Comma) => source_items(),
        Some(Token::Ident(s)) if ARGLESS_SOURCES.contains(&s.as_str()) => {
            let mut items = source_items();
            items.push(keyword_item("where"));
            items
        }
        Some(Token::RParen) => {
            let mut items = source_items();
            items.push(keyword_item("where"));
            items
        }
        _ => Vec::new(),
    }
}

fn predicate_context(tokens: &[Token]) -> Vec<TqlCompletionItem> {
    match tokens.last() {
        None => field_items(),
        Some(Token::AndAnd) | Some(Token::OrOr) | Some(Token::Not) | Some(Token::LParen) => {
            field_items()
        }
        Some(Token::Ident(s)) if s == "where" => field_items(),
        Some(Token::Ident(s)) if Field::from_name(s).is_some() => operator_items(),
        Some(Token::Str(_)) | Some(Token::Num(_)) | Some(Token::RBracket) | Some(Token::RParen) => {
            logic_items()
        }
        _ => Vec::new(),
    }
}

fn keyword_item(name: &str) -> TqlCompletionItem {
    TqlCompletionItem {
        label: name.to_string(),
        insert: format!("{name} "),
        kind: TqlCompletionKind::Keyword,
    }
}

fn source_items() -> Vec<TqlCompletionItem> {
    let mut items: Vec<TqlCompletionItem> = ARGLESS_SOURCES
        .iter()
        .map(|s| TqlCompletionItem {
            label: s.to_string(),
            insert: format!("{s} "),
            kind: TqlCompletionKind::Source,
        })
        .collect();
    items.extend(ARG_SOURCES.iter().map(|s| TqlCompletionItem {
        label: s.to_string(),
        insert: format!("{s}(\""),
        kind: TqlCompletionKind::Source,
    }));
    items
}

fn field_items() -> Vec<TqlCompletionItem> {
    FIELD_NAMES
        .iter()
        .map(|f| TqlCompletionItem {
            label: f.to_string(),
            insert: format!("{f} "),
            kind: TqlCompletionKind::Field,
        })
        .collect()
}

fn operator_items() -> Vec<TqlCompletionItem> {
    WORD_OPERATORS
        .iter()
        .chain(SYMBOL_OPERATORS.iter())
        .chain(LOGIC_OPERATORS.iter())
        .map(|op| TqlCompletionItem {
            label: op.to_string(),
            insert: format!("{op} "),
            kind: TqlCompletionKind::Operator,
        })
        .collect()
}

fn logic_items() -> Vec<TqlCompletionItem> {
    LOGIC_OPERATORS
        .iter()
        .map(|op| TqlCompletionItem {
            label: op.to_string(),
            insert: format!("{op} "),
            kind: TqlCompletionKind::Operator,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[TqlCompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn suggests_from_at_empty_query() {
        let items = complete("", 0, TqlEditMode::Query);
        assert_eq!(labels(&items), vec!["from"]);
    }

    #[test]
    fn suggests_sources_after_from() {
        let items = complete("from ", 5, TqlEditMode::Query);
        assert!(labels(&items).contains(&"home"));
        assert!(labels(&items).contains(&"list"));
        assert!(!labels(&items).contains(&"where"));
    }

    #[test]
    fn suggests_where_after_a_bare_source() {
        let items = complete("from home ", 10, TqlEditMode::Query);
        assert!(labels(&items).contains(&"where"));
        assert!(labels(&items).contains(&"local"));
    }

    #[test]
    fn suggests_sources_after_comma() {
        let items = complete("from home, ", 11, TqlEditMode::Query);
        assert!(labels(&items).contains(&"list"));
        assert!(!labels(&items).contains(&"where"));
    }

    #[test]
    fn suggests_fields_after_where() {
        let items = complete("from home where ", 16, TqlEditMode::Query);
        assert!(labels(&items).contains(&"has_files"));
        assert!(labels(&items).contains(&"reactions"));
    }

    #[test]
    fn filters_fields_by_partial_prefix() {
        let items = complete("from home where has_fi", 22, TqlEditMode::Query);
        assert_eq!(labels(&items), vec!["has_files"]);
    }

    #[test]
    fn suggests_operators_after_a_field_name() {
        let items = complete("from home where reactions ", 26, TqlEditMode::Query);
        assert!(labels(&items).contains(&"contains"));
        assert!(labels(&items).contains(&">="));
        assert!(labels(&items).contains(&"&&"));
    }

    #[test]
    fn suggests_logic_operators_after_a_value() {
        let items = complete("from home where reactions >= 10 ", 32, TqlEditMode::Query);
        assert_eq!(labels(&items), vec!["&&", "||"]);
    }

    #[test]
    fn predicate_mode_suggests_fields_from_the_start() {
        let items = complete("has_fi", 6, TqlEditMode::Predicate);
        assert_eq!(labels(&items), vec!["has_files"]);
    }

    #[test]
    fn predicate_mode_suggests_fields_after_and_and() {
        let items = complete("has_files && ", 13, TqlEditMode::Predicate);
        assert!(labels(&items).contains(&"cw"));
    }

    #[test]
    fn returns_empty_on_broken_input() {
        let items = complete("from list(\"", 11, TqlEditMode::Query);
        assert!(items.is_empty());
    }

    #[test]
    fn handles_multibyte_content_before_cursor_safely() {
        // 日本語文字列リテラルなど、cursor手前にマルチバイト文字が含まれていても
        // バイトオフセット変換でパニックせず文脈分類が正しく続くことを確認する。
        let items = complete("text -> \"日本語\" && has_fi", 23, TqlEditMode::Predicate);
        assert_eq!(labels(&items), vec!["has_files"]);
    }
}
