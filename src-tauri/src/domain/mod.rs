//! 正規化済みドメイン型。`serde` + `specta::Type` を付け、`tauri-specta` で TS 型を生成する。
//! 定義は docs/design/phase0-scaffold.md §2 / docs/design/filter-dsl-design.md §7 に対応。
//!
//! 型の一部はTSバインディング生成専用でRust側からは未参照のものがあるため、
//! モジュール全体で dead_code を許容している。
#![allow(dead_code, unused_imports)]

mod account;
mod cache_backend;
mod clip;
mod column;
mod list;
mod mute;
mod note;
mod notification;
mod notify;
mod pane;
mod reaction;
mod schedule;
mod scheduled;
mod search;
mod share;
mod ui;
mod url_preview;
mod user;

pub use account::Account;
pub use cache_backend::CacheBackendConfig;
pub use clip::Clip;
pub use column::{Column, ColumnGroup, ColumnKind, FilterQuery};
pub use list::{SourceItem, UserList};
pub use mute::MuteConfig;
pub use note::{DriveFile, Note, Poll, PollChoice, Translation, Visibility};
pub use notification::Notification;
pub use notify::NotifyConfig;
pub use pane::{Edge, PaneChild, PaneNode, SplitDirection};
pub use reaction::{EmojiDef, ReactionSummary, ReactionUser};
pub use schedule::{schedule_capabilities, ScheduleCapabilities};
pub use scheduled::{LocalScheduleStatus, LocalScheduledNote, ScheduledNote};
pub use search::{parse_misskey_version, search_capabilities, SearchCapabilities};
pub use share::ShareReceived;
pub use ui::{BackgroundKind, UiPrefs};
pub use url_preview::{UrlPlayer, UrlPreview};
pub use user::{InstanceInfo, User};
