//! 永続化（SQLite）。設定（Account/Column）を保存し、再起動時に復元する。

pub mod db;
pub mod settings;

pub use settings::SettingsStore;
