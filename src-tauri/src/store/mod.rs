//! 永続化。設定（Account/Column, settings.rs）と下書き（draft.rs）はプレーンテキスト(JSON)の
//! 1ファイル。ノートキャッシュ（note_cache.rs, 破棄前提）はデフォルトでrusqlite経由のSQLiteだが、
//! `NoteCacheBackend`トレイトにより差し替え可能で、PostgreSQL/MySQLバックエンドも選択できる
//! （postgres_backend.rs / mysql_backend.rs, 設定画面から切替。Issue #115 Phase 2/3）。
//! いずれも再起動時に復元する。

pub mod db;
pub mod draft;
pub mod note_cache;
pub mod settings;
mod sqlite_backend;
pub(crate) mod postgres_backend;
pub(crate) mod postgres_user_ref;
pub(crate) mod mysql_backend;
pub(crate) mod mysql_user_ref;
pub mod user_ref;

pub use draft::DraftStore;
pub use note_cache::NoteCacheStore;
pub use settings::SettingsStore;
pub(crate) use sqlite_backend::SqliteBackend;
