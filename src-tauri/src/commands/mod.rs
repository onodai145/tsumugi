//! Tauri command ハンドラ（invoke 対象）。

pub mod account;
pub mod column;
pub mod note;

// lib.rs は commands::<mod>::* をフルパス参照するため、この再エクスポートは今は未使用。
#[allow(unused_imports)]
pub use account::{
    complete_miauth, list_accounts, logout, remove_account, start_miauth, switch_account, whoami,
    MiAuthSession,
};
#[allow(unused_imports)]
pub use column::{
    capture_notes, close_column, fetch_backfill, list_columns, open_home_column, resume_column,
    uncapture_notes, OpenedColumn,
};
#[allow(unused_imports)]
pub use note::{delete_note_cmd, list_custom_emojis, post_note, react, renote, unreact};
