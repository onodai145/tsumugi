//! MCPサーバー(読み取り専用、Issue #557)。設計は docs/superpowers/specs/2026-11-10-mcp-server-design.md。
//! 配線(Task 9)が済むまで未使用の項目があるため、dead_code を許可している。配線後に外す。
#![allow(dead_code)]

pub mod settings;
#[cfg(desktop)]
pub mod token;
#[cfg(desktop)]
pub mod guard;
#[cfg(desktop)]
pub mod query;
#[cfg(test)]
pub(crate) mod testutil;
#[cfg(desktop)]
pub mod dto;
#[cfg(desktop)]
pub mod service;
#[cfg(desktop)]
pub mod tools;
#[cfg(desktop)]
pub mod server;
