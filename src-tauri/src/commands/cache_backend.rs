//! バックエンド切替コマンド(Issue #115 Phase 2)。設定画面からの手動切替を扱う。
//! 起動時のフォールバック(lib.rs::run()内、接続失敗時は無言でSQLiteへ)とは扱いが異なる:
//! ここでの接続失敗はフロントエンドへそのままエラーを返し、切替前のバックエンドを維持する
//! (設定画面が実体と食い違う表示になることを防ぐため)。

use crate::domain::CacheBackendConfig;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn get_cache_backend(state: State<'_, AppState>) -> Result<CacheBackendConfig, String> {
    state.settings.load_cache_backend().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_cache_backend(
    state: State<'_, AppState>,
    config: CacheBackendConfig,
    password: Option<String>,
) -> Result<(), String> {
    let new_backend: std::sync::Arc<dyn crate::store::note_cache::NoteCacheBackend> = match &config {
        CacheBackendConfig::Sqlite => {
            let conn = crate::store::db::open_cache(&state.cache_dir.join("cache.db")).map_err(|e| e.to_string())?;
            std::sync::Arc::new(crate::store::SqliteBackend::new(conn))
        }
        CacheBackendConfig::Postgres { host, port, database, user } => {
            let password = password.ok_or("password is required for Postgres backend")?;
            let params = crate::store::postgres_backend::PostgresConnectParams {
                host: host.clone(),
                port: *port,
                database: database.clone(),
                user: user.clone(),
                password: password.clone(),
            };
            // 接続確認に失敗したらここでErrを返す(切替前のバックエンドはまだ差し替えていない)。
            let backend = crate::store::postgres_backend::PostgresBackend::connect(&params)
                .await
                .map_err(|e| format!("failed to connect to Postgres: {e}"))?;
            crate::session::save_cache_backend_password(&password).map_err(|e| e.to_string())?;
            std::sync::Arc::new(backend)
        }
    };

    // ここまで来て初めて実際に差し替える(接続確認済みのバックエンドのみをswapする)。
    // 設定の永続化を先に行い、それが成功して初めてswap_backend(infallible)を呼ぶ。
    // 逆順にすると、swap成功直後にsave_cache_backendが失敗した場合に「プロセスは新
    // バックエンドで動いているが、永続化された設定は旧バックエンドのまま」という
    // 不整合window(再起動すると旧バックエンドに戻ってしまう)が生じるため。
    state.settings.save_cache_backend(&config).map_err(|e| e.to_string())?;
    state.cache.swap_backend(new_backend);
    Ok(())
}
