//! MCPのテスト用の生成関数。

use crate::domain::{Account, Note, User, Visibility};
use std::collections::HashMap;

pub fn user(id: &str, username: &str, host: Option<&str>) -> User {
    User {
        id: id.into(),
        username: username.into(),
        host: host.map(str::to_string),
        name: Some(username.to_uppercase()),
        avatar_url: None,
        is_bot: false,
        is_cat: false,
        followers_count: 0,
        following_count: 0,
        notes_count: 0,
        emojis: HashMap::new(),
        bio: None,
        banner_url: None,
        avatar_blurhash: None,
        instance: None,
    }
}

pub fn note(id: &str, created_at: i64, visibility: Visibility, local_only: bool, text: &str, user: User) -> Note {
    Note {
        id: id.into(),
        created_at,
        text: Some(text.into()),
        cw: None,
        visibility,
        local_only,
        user,
        reply_id: None,
        reply_user_id: None,
        renote_id: None,
        reply: None,
        renote: None,
        files: vec![],
        poll: None,
        tags: vec![],
        mentions: vec![],
        emojis: HashMap::new(),
        channel_id: None,
        via: None,
        lang: None,
        reactions: HashMap::new(),
        reaction_count: 0,
        renote_count: 0,
        reply_count: 0,
        my_reaction: None,
        is_renoted_by_me: false,
        is_favorited_by_me: false,
        is_pinned: false,
    }
}

pub fn public_note(id: &str, created_at: i64, text: &str) -> Note {
    note(id, created_at, Visibility::Public, false, text, user("u1", "alice", None))
}

pub fn account(id: &str, host: &str) -> Account {
    Account {
        id: id.into(),
        host: host.into(),
        username: "me".into(),
        user_id: format!("uid-{id}"),
        display_name: "Me".into(),
        avatar_url: None,
        instance: None,
        is_cat: false,
        avatar_blurhash: None,
    }
}
