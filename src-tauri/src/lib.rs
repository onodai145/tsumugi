mod api;
mod commands;
mod domain;
mod error;
mod events;
mod filter;
mod session;
mod state;
mod store;
mod stream;

use session::KeyringStore;
use state::AppState;
use store::{db, SettingsStore};
use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder};

/// tauri-specta の command / event 集合を構築する。TS バインディング export と
/// invoke_handler の両方でこの定義を使うため関数に切り出す。
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::account::start_miauth,
            commands::account::complete_miauth,
            commands::account::list_accounts,
            commands::account::switch_account,
            commands::account::remove_account,
            commands::account::logout,
            commands::account::whoami,
            commands::column::add_column,
            commands::column::resume_column,
            commands::column::list_groups,
            commands::column::list_columns,
            commands::column::fetch_backfill,
            commands::column::fetch_notifications_backfill,
            commands::column::close_column,
            commands::column::set_group_width,
            commands::column::reorder_groups,
            commands::column::move_tab,
            commands::column::capture_notes,
            commands::column::uncapture_notes,
            commands::column::validate_filter,
            commands::column::list_user_lists,
            commands::note::post_note,
            commands::note::renote,
            commands::note::delete_note_cmd,
            commands::note::react,
            commands::note::unreact,
            commands::note::list_custom_emojis,
            commands::note::upload_file,
            commands::mute::get_mute,
            commands::mute::set_mute,
            commands::mute::get_notify,
            commands::mute::set_notify,
            commands::mute::get_ui_prefs,
            commands::mute::set_ui_prefs,
            commands::mute::sync_server_mutes,
        ])
        .events(collect_events![
            events::ColumnNote,
            events::ColumnNoteUpdated,
            events::ColumnNotification,
            events::ColumnConnectionState,
        ])
}

/// TS 生成の設定。i64 の扱いはフィールド単位で `#[specta(type = specta_typescript::Number)]`
/// を付けて JS `number` へ（epoch 秒は 2^53 に十分収まるため精度損失なし。domain/note.rs 参照）。
fn ts_config() -> specta_typescript::Typescript {
    specta_typescript::Typescript::default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // 開発ビルド時のみ、Rust→TS 型・invoke ラッパをフロントへ生成（二重管理回避）
    #[cfg(debug_assertions)]
    builder
        .export(ts_config(), "../frontend/src/bindings/tauri.gen.ts")
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // 設定 DB を app data dir に開く（無ければ作成）
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("no app data dir");
            std::fs::create_dir_all(&data_dir).expect("failed to create app data dir");
            let conn = db::open(&data_dir.join("tsumugi.db")).expect("failed to open settings db");
            let settings = SettingsStore::new(conn);
            app.manage(AppState::new(Box::new(KeyringStore), settings));

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
            .export(ts_config(), path)
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
        // Phase 2: command と event が生成されていること
        assert!(ts.contains("addColumn"), "missing addColumn command");
        assert!(ts.contains("ColumnNote"), "missing ColumnNote event type");
        assert!(ts.contains("createdAt"), "Note.created_at should be camelCase");
        // i64(created_at) が number として出ていること（BigInt ではない）
        assert!(
            ts.contains("createdAt: number") || ts.contains("createdAt:number"),
            "created_at should export as number:\n{ts}"
        );
    }
}
