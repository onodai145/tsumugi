mod api;
mod commands;
mod domain;
mod error;
mod session;
mod state;

use session::KeyringStore;
use state::AppState;
use tauri::Manager;
use tauri_specta::{collect_commands, Builder};

/// tauri-specta の command 集合を構築する。TS バインディング export と
/// invoke_handler の両方でこの定義を使うため関数に切り出す。
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::account::start_miauth,
        commands::account::complete_miauth,
        commands::account::list_accounts,
        commands::account::switch_account,
        commands::account::remove_account,
        commands::account::logout,
        commands::account::whoami,
    ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // 開発ビルド時のみ、Rust→TS 型・invoke ラッパをフロントへ生成（二重管理回避）
    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../frontend/src/bindings/tauri.gen.ts",
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(AppState::new(Box::new(KeyringStore)));

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod specta_export {
    use super::*;

    /// tauri-specta 推奨パターン: TS バインディング生成をテストで行う。
    /// `cargo test` で frontend/src/bindings/tauri.gen.ts を再生成し、
    /// 生成が成功すること + serde の camelCase が TS へ反映されていることを検証する。
    #[test]
    fn generates_frontend_bindings() {
        let path = "../frontend/src/bindings/tauri.gen.ts";
        specta_builder()
            .export(specta_typescript::Typescript::default(), path)
            .expect("failed to export typescript bindings");

        let ts = std::fs::read_to_string(path).expect("bindings file not written");
        // command が生成されている（camelCase 化される）
        assert!(ts.contains("startMiauth"), "missing startMiauth in:\n{ts}");
        assert!(ts.contains("completeMiauth"));
        assert!(ts.contains("whoami"));
        // serde(rename_all="camelCase") が specta 経由で TS に反映されていること
        assert!(
            ts.contains("displayName"),
            "Account.display_name should be camelCase (displayName). serde rename not applied:\n{ts}"
        );
        assert!(ts.contains("followersCount"), "User.followers_count should be camelCase");
        // token を戻り値に含めていないこと（型に token フィールドが無い）
        assert!(!ts.contains("token:"), "bindings unexpectedly expose a token field:\n{ts}");
    }
}
