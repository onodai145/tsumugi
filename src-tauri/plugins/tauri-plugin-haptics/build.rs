const COMMANDS: &[&str] = &["vibrate"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
