//! Misskey の生 JSON レスポンスを domain 型へ正規化する。

use crate::domain::{DriveFile, Note, Notification, Poll, PollChoice, User, Visibility};
use serde::Deserialize;
use std::collections::HashMap;

/// Misskey の User オブジェクト（`i` / `me` / miauth の user）を受ける生型。
/// UserLite にしか無いフィールドもあるため、集計値は `#[serde(default)]`。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUser {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub is_cat: bool,
    #[serde(default)]
    pub followers_count: u32,
    #[serde(default)]
    pub following_count: u32,
    #[serde(default)]
    pub notes_count: u32,
}

impl From<RawUser> for User {
    fn from(r: RawUser) -> Self {
        User {
            id: r.id,
            username: r.username,
            host: r.host,
            name: r.name,
            avatar_url: r.avatar_url,
            is_bot: r.is_bot,
            is_cat: r.is_cat,
            followers_count: r.followers_count,
            following_count: r.following_count,
            notes_count: r.notes_count,
        }
    }
}

/// Misskey の Notification オブジェクト（i/notifications / main channel）を受ける生型。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawNotification {
    pub id: String,
    pub created_at: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub user: Option<RawUser>,
    #[serde(default)]
    pub note: Option<RawNote>,
    #[serde(default)]
    pub reaction: Option<String>,
}

impl From<RawNotification> for Notification {
    fn from(r: RawNotification) -> Self {
        Notification {
            id: r.id,
            created_at: to_epoch(&r.created_at),
            kind: r.kind,
            user: r.user.map(Into::into),
            note: r.note.map(Into::into),
            reaction: r.reaction,
        }
    }
}

/// Misskey の ISO8601(RFC3339) 文字列を epoch 秒へ。パース不能なら 0。
fn to_epoch(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFile {
    pub id: String,
    #[serde(rename = "type", default)]
    pub mime_type: String,
    #[serde(default)]
    pub is_sensitive: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPollChoice {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub votes: u32,
    #[serde(default)]
    pub is_voted: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPoll {
    #[serde(default)]
    pub choices: Vec<RawPollChoice>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Misskey の Note オブジェクト（timeline / streaming のペイロード）を受ける生型。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawNote {
    pub id: String,
    pub created_at: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub cw: Option<String>,
    pub user: RawUser,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default)]
    pub local_only: bool,
    #[serde(default)]
    pub reply_id: Option<String>,
    #[serde(default)]
    pub renote_id: Option<String>,
    #[serde(default)]
    pub renote: Option<Box<RawNote>>,
    #[serde(default)]
    pub files: Vec<RawFile>,
    #[serde(default)]
    pub poll: Option<RawPoll>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub mentions: Vec<String>,
    /// Misskey は {name: url} のオブジェクトで返す。キーだけ使う。
    #[serde(default)]
    pub emojis: HashMap<String, String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    /// emoji キー -> 集計数
    #[serde(default)]
    pub reactions: HashMap<String, u32>,
    #[serde(default)]
    pub renote_count: u32,
    #[serde(default)]
    pub replies_count: u32,
    #[serde(default)]
    pub my_reaction: Option<String>,
}

fn default_visibility() -> String {
    "public".to_string()
}

fn parse_visibility(s: &str) -> Visibility {
    match s {
        "home" => Visibility::Home,
        "followers" => Visibility::Followers,
        "specified" => Visibility::Specified,
        _ => Visibility::Public,
    }
}

impl From<RawFile> for DriveFile {
    fn from(f: RawFile) -> Self {
        DriveFile {
            id: f.id,
            mime_type: f.mime_type,
            is_sensitive: f.is_sensitive,
            url: f.url,
            thumbnail_url: f.thumbnail_url,
        }
    }
}

impl From<RawPoll> for Poll {
    fn from(p: RawPoll) -> Self {
        Poll {
            choices: p
                .choices
                .into_iter()
                .map(|c| PollChoice {
                    text: c.text,
                    votes: c.votes,
                    is_voted: c.is_voted,
                })
                .collect(),
            multiple: p.multiple,
            expires_at: p.expires_at.as_deref().map(to_epoch),
        }
    }
}

impl From<RawNote> for Note {
    fn from(r: RawNote) -> Self {
        let reaction_count = r.reactions.values().copied().sum();
        Note {
            id: r.id,
            created_at: to_epoch(&r.created_at),
            text: r.text,
            cw: r.cw,
            visibility: parse_visibility(&r.visibility),
            local_only: r.local_only,
            user: r.user.into(),
            reply_id: r.reply_id,
            renote_id: r.renote_id,
            renote: r.renote.map(|n| Box::new((*n).into())),
            files: r.files.into_iter().map(Into::into).collect(),
            poll: r.poll.map(Into::into),
            tags: r.tags,
            mentions: r.mentions,
            emojis: r.emojis.into_keys().collect(),
            channel_id: r.channel_id,
            via: None,
            lang: r.lang,
            reactions: r.reactions,
            reaction_count,
            renote_count: r.renote_count,
            reply_count: r.replies_count,
            my_reaction: r.my_reaction,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_note_with_reactions_and_epoch() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n1","createdAt":"2026-07-05T12:00:00.000Z","text":"hello",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"home","renoteCount":2,"repliesCount":1,
              "reactions":{"👍":3,":blobcat@.:":1},"myReaction":"👍",
              "files":[{"id":"f1","type":"image/png","isSensitive":false,"url":"http://x/f1"}],
              "emojis":{"blobcat":"http://x/e.png"},"tags":["rust"]
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert_eq!(n.id, "n1");
        assert_eq!(n.created_at, 1783252800); // 2026-07-05T12:00:00Z
        assert_eq!(n.visibility, Visibility::Home);
        assert_eq!(n.reaction_count, 4); // 3 + 1
        assert_eq!(n.renote_count, 2);
        assert_eq!(n.reply_count, 1);
        assert_eq!(n.my_reaction.as_deref(), Some("👍"));
        assert_eq!(n.files.len(), 1);
        assert_eq!(n.files[0].mime_type, "image/png");
        assert_eq!(n.emojis, vec!["blobcat".to_string()]);
        assert_eq!(n.tags, vec!["rust".to_string()]);
    }

    #[test]
    fn defaults_for_minimal_note() {
        let raw: RawNote = serde_json::from_str(
            r#"{"id":"n2","createdAt":"2026-07-05T00:00:00Z",
                "user":{"id":"u2","username":"bob"}}"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert_eq!(n.visibility, Visibility::Public);
        assert_eq!(n.reaction_count, 0);
        assert!(n.text.is_none());
        assert!(!n.is_pinned);
    }

    #[test]
    fn bad_date_falls_back_to_zero() {
        let raw: RawNote = serde_json::from_str(
            r#"{"id":"n3","createdAt":"not-a-date","user":{"id":"u","username":"x"}}"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert_eq!(n.created_at, 0);
    }

    #[test]
    fn parses_local_user_and_defaults_missing_counts() {
        let raw: RawUser = serde_json::from_str(
            r#"{"id":"a1","username":"alice","host":null,"name":"Alice","isBot":false}"#,
        )
        .unwrap();
        let u: User = raw.into();
        assert_eq!(u.id, "a1");
        assert_eq!(u.username, "alice");
        assert!(u.host.is_none());
        assert_eq!(u.acct(), "@alice");
        assert_eq!(u.notes_count, 0); // 欠損は 0
    }

    #[test]
    fn parses_remote_user_with_counts() {
        let raw: RawUser = serde_json::from_str(
            r#"{"id":"b2","username":"bob","host":"remote.example","name":null,
                "followersCount":12,"followingCount":3,"notesCount":45,"isCat":true}"#,
        )
        .unwrap();
        let u: User = raw.into();
        assert_eq!(u.acct(), "@bob@remote.example");
        assert_eq!(u.followers_count, 12);
        assert!(u.is_cat);
    }
}
