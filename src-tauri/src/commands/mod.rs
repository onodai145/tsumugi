//! Tauri command ハンドラ（invoke 対象）。

pub mod account;
pub mod column;
pub mod mute;
pub mod note;

// lib.rs は commands::<mod>::* をフルパス参照するため、この再エクスポートは今は未使用。
#[allow(unused_imports)]
pub use account::{
    complete_miauth, list_accounts, logout, remove_account, start_miauth, switch_account, whoami,
    MiAuthSession,
};
#[allow(unused_imports)]
pub use column::{
    add_column, capture_notes, close_column, fetch_backfill, fetch_notifications_backfill,
    list_columns, list_groups, list_user_lists, move_tab, reorder_groups, resume_column,
    set_group_width, uncapture_notes, validate_filter, OpenedColumn,
};
#[allow(unused_imports)]
pub use note::{
    delete_note_cmd, list_custom_emojis, post_note, react, renote, unreact, upload_file,
};
#[allow(unused_imports)]
pub use mute::{get_mute, set_mute};
