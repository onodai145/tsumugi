//! TQL(Tsumugi Query Language) フィルタ評価。docs/filter-dsl-design.md。
//! 字句解析(token) → 構文解析+型検査(parser) → AST(ast) → インメモリ評価(eval) →
//! SQL 射影(sql) の分割構成。
//!
//! まだ sql/一部は未接続のため dead_code を許容している。

#![allow(dead_code)]

pub mod ast;
pub mod eval;
pub mod mute;
pub mod parser;
pub mod sql;
pub mod token;

use crate::domain::{FilterQuery, Note};
use eval::EvalContext;

/// カラム生成時に一度だけコンパイルしておくフィルタ（毎ノートのパースを避ける）。
#[derive(Clone)]
pub enum CompiledFilter {
    /// 素通し（フィルタなし）
    PassAll,
    /// 部分一致キーワード（OR、小文字化して比較）
    Keywords(Vec<String>),
    /// TQL の where 述語
    Tql(ast::Expr),
}

impl CompiledFilter {
    /// `FilterQuery` をコンパイルする。TQL のパース失敗はエラー文字列で返す。
    pub fn compile(fq: &FilterQuery) -> Result<Self, String> {
        Ok(match fq {
            FilterQuery::Keywords(ks) if ks.iter().all(|k| k.trim().is_empty()) => {
                CompiledFilter::PassAll
            }
            FilterQuery::Keywords(ks) => CompiledFilter::Keywords(
                ks.iter()
                    .map(|k| k.to_lowercase())
                    .filter(|k| !k.is_empty())
                    .collect(),
            ),
            FilterQuery::Tql(q) if q.trim().is_empty() => CompiledFilter::PassAll,
            FilterQuery::Tql(q) => CompiledFilter::Tql(parser::parse_predicate(q)?),
        })
    }

    /// このノートがフィルタを通過するか。
    pub fn matches(&self, note: &Note, ctx: &EvalContext) -> bool {
        match self {
            CompiledFilter::PassAll => true,
            CompiledFilter::Keywords(ks) => {
                let text = note.text.as_deref().unwrap_or("").to_lowercase();
                ks.iter().any(|k| text.contains(k))
            }
            CompiledFilter::Tql(expr) => eval::evaluate(expr, note, ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{User, Visibility};
    use std::collections::HashMap;

    fn note(text: &str, files: usize) -> Note {
        Note {
            id: "n".into(),
            created_at: 0,
            text: Some(text.into()),
            cw: None,
            visibility: Visibility::Public,
            local_only: false,
            user: User {
                id: "u".into(),
                username: "a".into(),
                host: None,
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
            },
            reply_id: None,
            renote_id: None,
            renote: None,
            files: (0..files)
                .map(|i| crate::domain::DriveFile {
                    id: i.to_string(),
                    mime_type: "image/png".into(),
                    is_sensitive: false,
                    url: "u".into(),
                    thumbnail_url: None,
                })
                .collect(),
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: vec![],
            channel_id: None,
            via: None,
            lang: None,
            reactions: HashMap::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    #[test]
    fn passall_and_keywords() {
        let ctx = EvalContext::default();
        assert!(CompiledFilter::compile(&FilterQuery::Keywords(vec![]))
            .unwrap()
            .matches(&note("anything", 0), &ctx));
        let kw = CompiledFilter::compile(&FilterQuery::Keywords(vec!["Rust".into()])).unwrap();
        assert!(kw.matches(&note("I love rust", 0), &ctx));
        assert!(!kw.matches(&note("hello", 0), &ctx));
    }

    #[test]
    fn tql_predicate() {
        let ctx = EvalContext::default();
        let f = CompiledFilter::compile(&FilterQuery::Tql("has_files && !cw".into())).unwrap();
        assert!(f.matches(&note("x", 1), &ctx));
        assert!(!f.matches(&note("x", 0), &ctx));
    }

    #[test]
    fn tql_parse_error_is_reported() {
        assert!(CompiledFilter::compile(&FilterQuery::Tql("reactions".into())).is_err()); // 非boolean
        assert!(CompiledFilter::compile(&FilterQuery::Tql("&&".into())).is_err());
    }
}
