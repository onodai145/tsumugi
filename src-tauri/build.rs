/// llama.cpp（ローカル翻訳）をビルドするターゲットか。`local-translate` feature が有効で、
/// デスクトップ、または Android でビルドできることを確認した ABI のとき。コード側は `#[cfg(llama_target)]` で判定する。
/// Android の ABI を絞る・外すときは、`Cargo.toml` の Android 用 `llama-cpp-2` の条件も同じにする。
fn llama_target(feature: bool, os: &str, arch: &str) -> bool {
  feature
    && (matches!(os, "linux" | "macos" | "windows")
      || (os == "android" && matches!(arch, "aarch64" | "x86_64" | "arm" | "x86")))
}

fn main() {
  println!("cargo:rustc-check-cfg=cfg(llama_target)");
  let feature = std::env::var_os("CARGO_FEATURE_LOCAL_TRANSLATE").is_some();
  let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
  if llama_target(feature, &os, &arch) {
    println!("cargo:rustc-cfg=llama_target");
  }

  let git_hash = std::process::Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .output()
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| s.trim().to_string())
    .unwrap_or_else(|| "unknown".to_string());
  println!("cargo:rustc-env=TSUMUGI_GIT_HASH={git_hash}");

  tauri_build::build()
}
