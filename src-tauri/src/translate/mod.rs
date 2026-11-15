//! ローカル翻訳（PLaMo 3 Translate、Issue #603）。設計は
//! `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`。

use crate::error::Error;

pub mod download;
pub mod engine;
pub mod lang;
pub mod local_models;
#[cfg(llama_target)]
pub mod llama;
pub mod manager;
pub mod model_store;
pub mod prompt;

pub use local_models::LocalModels;
pub use manager::LocalTranslate;

/// アイドルがこれだけ続いたらモデルを解放する。Android はメモリが厳しいので短くする（実機の実測で見直す）。
#[cfg(target_os = "android")]
pub const IDLE_UNLOAD: std::time::Duration = std::time::Duration::from_secs(180);
#[cfg(not(target_os = "android"))]
pub const IDLE_UNLOAD: std::time::Duration = std::time::Duration::from_secs(600);
/// アイドル解放の確認間隔。
pub const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

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
    cfg!(llama_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_unload_is_three_minutes_on_android_and_ten_elsewhere() {
        let want = if cfg!(target_os = "android") { 180 } else { 600 };
        assert_eq!(IDLE_UNLOAD, std::time::Duration::from_secs(want));
    }

    #[test]
    fn is_supported_follows_the_llama_target_cfg() {
        assert_eq!(is_supported(), cfg!(llama_target));
    }
}
