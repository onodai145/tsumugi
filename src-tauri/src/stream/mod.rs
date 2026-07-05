//! Streaming 受信（WebSocket 自前実装）。設計書§6。

pub mod connection;
pub mod inbox;
pub mod protocol;

pub use connection::ConnectionManager;
