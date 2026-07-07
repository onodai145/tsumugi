//! 正規化済みドメイン型。`serde` + `specta::Type` を付け、`tauri-specta` で TS 型を生成する。
//! 定義は docs/phase0-scaffold.md §2 / docs/filter-dsl-design.md §7 に対応。
//!
//! Phase 1 では Account/User のみを command が使う。Note/Column/Reaction 系は Phase 2 以降で
//! 参照するため型定義だけ先行して置く（dead_code を許容）。
#![allow(dead_code, unused_imports)]

mod account;
mod column;
mod list;
mod mute;
mod note;
mod notification;
mod reaction;
mod user;

pub use account::Account;
pub use column::{Column, ColumnGroup, ColumnKind, FilterQuery};
pub use list::UserList;
pub use mute::MuteConfig;
pub use note::{DriveFile, Note, Poll, PollChoice, Visibility};
pub use notification::Notification;
pub use reaction::{EmojiDef, ReactionSummary};
pub use user::User;
