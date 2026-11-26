// `build.rs`(`mod`)と、`src/translate/mod.rs` のテスト(`include!`)の両方が読み込む(build.rs はテストを走らせられないため)。
//
// llama.cpp（ローカル翻訳）をビルドするターゲットか。`local-translate` feature が有効で、
// Linux / macOS / Windows、または Android でビルドできることを確認した ABI のとき。
// BSD などは配布しないので対象外。コード側は `#[cfg(llama_target)]` で判定する。
// Android の ABI を絞る・外すときは、`Cargo.toml` の Android 用 `llama-cpp-2` の条件も同じにする。
pub fn llama_target(feature: bool, os: &str, arch: &str) -> bool {
    feature
        && (matches!(os, "linux" | "macos" | "windows")
            || (os == "android" && matches!(arch, "aarch64" | "x86_64" | "arm" | "x86")))
}
