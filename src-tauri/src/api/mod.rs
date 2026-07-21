//! Misskey REST クライアント（薄い型付きラッパ）。
//!
//! progenitor による OpenAPI 自動生成は Misskey が OpenAPI 3.1.0 を出力するため不可
//! （`openapiv3` crate が 3.1 を parse 不可。docs/phase0-scaffold §「未確定」と検証結果参照）。
//! よって設計書§6.1 のフォールバックである手書きラッパを採用する。Misskey REST は
//! 「全 POST・JSONボディに `i`(token) 同梱」で均一なので、共通処理を [`client`] に集約する。

pub mod client;
pub mod clips;
pub mod drive;
pub mod meta;
pub mod mutes;
pub mod notes;
pub mod notifications;
pub mod normalize;

pub use client::MisskeyClient;
