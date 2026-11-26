// `llama_target`(cfg `llama_target` を立てる条件)。単体テストのために別ファイルにしてある。
// `include!` ではなく `mod` で読む: `cargo fmt` は `mod` をたどって対象を集める(Issue #672)。
mod llama_target;

fn main() {
    println!("cargo:rerun-if-changed=llama_target.rs");
    println!("cargo:rustc-check-cfg=cfg(llama_target)");
    let feature = std::env::var_os("CARGO_FEATURE_LOCAL_TRANSLATE").is_some();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if llama_target::llama_target(feature, &os, &arch) {
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
