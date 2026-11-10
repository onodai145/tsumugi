//! MCPサーバー(読み取り専用、Issue #557)。設計は docs/superpowers/specs/2026-11-10-mcp-server-design.md。
//! 配線(Task 9)が済むまで未使用の項目があるため、dead_code を許可している。配線後に外す。
#![allow(dead_code)]

pub mod settings;
#[cfg(desktop)]
pub mod token;
#[cfg(desktop)]
pub mod guard;
