//! 永続化。設定（Account/Column, settings.rs）と下書き（draft.rs）はプレーンテキスト(JSON)の
//! 1ファイル、ノートキャッシュ（note_cache.rs, 破棄前提）のみ rusqlite 経由の SQLite。
//! いずれも再起動時に復元する。

pub mod db;
pub mod draft;
pub mod note_cache;
pub mod settings;
pub mod user_ref;

pub use draft::DraftStore;
pub use note_cache::NoteCacheStore;
pub use settings::SettingsStore;
