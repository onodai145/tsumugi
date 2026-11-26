//! MCPサーバー(読み取り専用、Issue #557)。設計は docs/superpowers/specs/2026-11-10-mcp-server-design.md。

#[cfg(desktop)]
pub mod dto;
#[cfg(desktop)]
pub mod guard;
#[cfg(desktop)]
pub mod query;
#[cfg(desktop)]
pub mod server;
#[cfg(desktop)]
pub mod service;
pub mod settings;
#[cfg(test)]
pub(crate) mod testutil;
#[cfg(desktop)]
pub mod token;
#[cfg(desktop)]
pub mod tools;
