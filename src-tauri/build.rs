fn main() {
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
