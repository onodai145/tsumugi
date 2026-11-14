//! ローカル翻訳（PLaMo 3 Translate、Issue #603）。設計は
//! `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`。

use crate::error::Error;

pub mod lang;
pub mod prompt;

/// ローカル翻訳のエラー文言。専用の `Error` variant は足さず、#440 の `UNAVAILABLE` と同じく
/// `Error::Invalid` の文言で区別する（フロントは文言で判別する）。
pub const ERR_MODEL_NOT_READY: &str = "LOCAL_MODEL_NOT_READY";
pub const ERR_TEXT_TOO_LONG: &str = "LOCAL_TEXT_TOO_LONG";
pub const ERR_LANG_UNSUPPORTED: &str = "LOCAL_LANG_UNSUPPORTED";
/// このビルド・プラットフォームがローカル翻訳に対応していない。
pub const ERR_UNSUPPORTED: &str = "LOCAL_UNSUPPORTED";

pub fn invalid(code: &str) -> Error {
    Error::Invalid(code.to_string())
}

/// このビルド・プラットフォームでローカル翻訳が使えるか。
pub const fn is_supported() -> bool {
    cfg!(all(feature = "local-translate", desktop))
}
