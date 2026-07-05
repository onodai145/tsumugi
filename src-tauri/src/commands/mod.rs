//! Tauri command ハンドラ（invoke 対象）。

pub mod account;
pub mod column;

// lib.rs は commands::<mod>::* をフルパス参照するため、この再エクスポートは今は未使用。
#[allow(unused_imports)]
pub use account::{
    complete_miauth, list_accounts, logout, remove_account, start_miauth, switch_account, whoami,
    MiAuthSession,
};
#[allow(unused_imports)]
pub use column::{close_column, fetch_backfill, open_home_column, OpenedColumn};
