//! ローカル翻訳（PLaMo 3 Translate、Issue #603）。設計は
//! `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`。

use crate::error::Error;

pub mod download;
pub mod engine;
pub mod lang;
#[cfg(llama_target)]
pub mod llama;
pub mod local_models;
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

    #[cfg(target_os = "android")]
    #[test]
    fn idle_unload_is_three_minutes_on_android() {
        assert_eq!(IDLE_UNLOAD, std::time::Duration::from_secs(180));
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn idle_unload_is_ten_minutes_on_desktop() {
        assert_eq!(IDLE_UNLOAD, std::time::Duration::from_secs(600));
    }

    // `build.rs` が cfg `llama_target` を立てる条件(`llama_target.rs`)。build.rs 自身はテストを走らせられないので、
    // 同じファイルをここにも読み込んで、ターゲットごとの表で確かめる。
    mod llama_target_rule {
        include!("../../llama_target.rs");

        #[test]
        fn is_on_for_the_desktop_oses_and_every_android_abi_with_the_feature() {
            for os in ["linux", "macos", "windows"] {
                assert!(llama_target(true, os, "x86_64"), "{os}");
            }
            for arch in ["aarch64", "x86_64", "arm", "x86"] {
                assert!(llama_target(true, "android", arch), "android {arch}");
            }
        }

        #[test]
        fn is_off_without_the_feature_on_any_target() {
            for (os, arch) in [
                ("linux", "x86_64"),
                ("macos", "aarch64"),
                ("windows", "x86_64"),
                ("android", "aarch64"),
            ] {
                assert!(!llama_target(false, os, arch), "{os} {arch}");
            }
        }

        #[test]
        fn is_off_for_unsupported_targets_even_with_the_feature() {
            // 配布しない OS(BSD など)と iOS、Android でも未確認の ABI(riscv64 など)。
            for (os, arch) in [
                ("freebsd", "x86_64"),
                ("openbsd", "x86_64"),
                ("ios", "aarch64"),
                ("android", "riscv64"),
            ] {
                assert!(!llama_target(true, os, arch), "{os} {arch}");
            }
        }
    }

    #[test]
    fn is_supported_matches_the_rule_for_the_target_this_test_runs_on() {
        // build.rs が実際に出した cfg が、同じ規則を今のターゲットに当てた結果と一致する(配線の確認)。
        let want = llama_target_rule::llama_target(
            cfg!(feature = "local-translate"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        assert_eq!(is_supported(), want);
    }
}
