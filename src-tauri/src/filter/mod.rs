//! TQL(Tsumugi Query Language) フィルタ評価。docs/filter-dsl-design.md。
//! 字句解析(token) → 構文解析+型検査(parser) → AST(ast) → インメモリ評価(eval) →
//! SQL 射影(sql) の分割構成。Phase 4 で段階実装中。
//!
//! まだカラム評価には未接続のため、全体を dead_code 許容にしている（接続時に外す）。
#![allow(dead_code)]

pub mod ast;
pub mod eval;
pub mod parser;
pub mod sql;
pub mod token;
