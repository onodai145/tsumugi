//! Misskey の生 JSON レスポンスを domain 型へ正規化する。

use crate::domain::{DriveFile, Note, Notification, Poll, PollChoice, ReactionUser, User, Visibility};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

/// followers/following/notes の各カウント用デシリアライザ。リモートユーザーで未解決の場合、
/// Misskey は `-1` を返すことがある（`u32` では表現できずデシリアライズが失敗していた）。
/// 負値は「不明」として 0 に丸める。
fn deserialize_count<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(i64::deserialize(deserializer)?.max(0) as u32)
}

/// `UserLite.instance`（リモートユーザーのみ）。本家 `MkInstanceTicker.vue` はリモートの
/// アイコンに `iconUrl` ではなく `faviconUrl` を使うため、`iconUrl` は読まない。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawInstanceInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub favicon_url: Option<String>,
    #[serde(default)]
    pub theme_color: Option<String>,
}

impl RawInstanceInfo {
    /// `host` はリモートユーザーの所属ホスト。`favicon_url` が無ければ `/favicon.ico` を補う。
    fn into_domain(self, host: &str) -> crate::domain::InstanceInfo {
        crate::domain::InstanceInfo {
            name: self.name,
            icon_url: self.favicon_url,
            theme_color: self.theme_color,
        }
        .with_favicon_fallback(host)
        .with_theme_color_fallback()
    }
}

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
    #[serde(default, deserialize_with = "deserialize_count")]
    pub followers_count: u32,
    #[serde(default, deserialize_with = "deserialize_count")]
    pub following_count: u32,
    #[serde(default, deserialize_with = "deserialize_count")]
    pub notes_count: u32,
    /// 表示名(`name`)中のカスタム絵文字 {name: url}。
    #[serde(default)]
    pub emojis: HashMap<String, String>,
    /// Misskey側のフィールド名は `description`。UserDetailed系レスポンスにのみ存在。
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub instance: Option<RawInstanceInfo>,
    #[serde(default)]
    pub avatar_blurhash: Option<String>,
}

impl From<RawUser> for User {
    fn from(r: RawUser) -> Self {
        // Misskeyはローカルユーザーに `instance` を付与しないため、`host` が無ければ None。
        let instance = match (&r.host, r.instance) {
            (Some(h), Some(i)) => Some(i.into_domain(h)),
            _ => None,
        };
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
            emojis: r.emojis,
            bio: r.description,
            banner_url: r.banner_url,
            avatar_blurhash: r.avatar_blurhash.clone(),
            instance,
        }
    }
}

/// Misskey の NoteReaction オブジェクト（`notes/reactions` のレスポンス要素）を受ける生型。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawReactionUser {
    pub user: RawUser,
    /// Misskey形式キー（Unicode生 or :name@host:）。JSON上のフィールド名は `type`。
    #[serde(rename = "type")]
    pub reaction: String,
}

impl From<RawReactionUser> for ReactionUser {
    fn from(r: RawReactionUser) -> Self {
        ReactionUser {
            user: r.user.into(),
            reaction: r.reaction,
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
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: Option<i64>,
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
    /// ネストされた返信元ノート（reply_user_id 抽出用。浅く1階層のみ使う）
    #[serde(default)]
    pub reply: Option<Box<RawNote>>,
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
    /// Misskey は {name: url} のオブジェクトで返す（本文中のカスタム絵文字）。
    #[serde(default)]
    pub emojis: HashMap<String, String>,
    /// リアクションのカスタム絵文字 {name: url}（新しめの Misskey）。
    #[serde(default)]
    pub reaction_emojis: HashMap<String, String>,
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
            name: f.name,
            size: f.size,
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
            reply_user_id: r.reply.as_ref().map(|reply| reply.user.id.clone()),
            reply: r.reply.map(|n| {
                let mut reply: Note = (*n).into();
                // 浅く1階層のみ保持する
                reply.reply = None;
                Box::new(reply)
            }),
            renote_id: r.renote_id,
            renote: r.renote.map(|n| Box::new((*n).into())),
            files: r.files.into_iter().map(Into::into).collect(),
            poll: r.poll.map(Into::into),
            tags: r.tags,
            mentions: r.mentions,
            emojis: {
                // 本文絵文字とリアクション絵文字を1つの name->url マップに統合
                let mut m = r.emojis;
                m.extend(r.reaction_emojis);
                m
            },
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
        assert_eq!(n.emojis.get("blobcat").map(String::as_str), Some("http://x/e.png"));
        assert_eq!(n.tags, vec!["rust".to_string()]);
    }

    #[test]
    fn extracts_reply_user_id_from_nested_reply() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n2","createdAt":"2026-07-05T12:00:00.000Z","text":"@bob hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public","replyId":"r1",
              "reply":{
                "id":"r1","createdAt":"2026-07-05T11:00:00.000Z","text":"original",
                "user":{"id":"bob-id","username":"bob","host":null},
                "visibility":"public"
              }
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert_eq!(n.reply_id.as_deref(), Some("r1"));
        assert_eq!(n.reply_user_id.as_deref(), Some("bob-id"));
    }

    #[test]
    fn reply_user_id_is_none_when_not_a_reply() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n3","createdAt":"2026-07-05T12:00:00.000Z","text":"hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public"
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert_eq!(n.reply_user_id, None);
    }

    #[test]
    fn keeps_nested_reply_as_shallow_note() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n2","createdAt":"2026-07-05T12:00:00.000Z","text":"@bob hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public","replyId":"r1",
              "reply":{
                "id":"r1","createdAt":"2026-07-05T11:00:00.000Z","text":"original",
                "user":{"id":"bob-id","username":"bob","host":null,"name":"Bob"},
                "visibility":"public","replyId":"r0",
                "reply":{
                  "id":"r0","createdAt":"2026-07-05T10:00:00.000Z","text":"grandparent",
                  "user":{"id":"carol-id","username":"carol","host":null},
                  "visibility":"public"
                }
              }
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        let reply = n.reply.expect("reply should be kept");
        assert_eq!(reply.id, "r1");
        assert_eq!(reply.text.as_deref(), Some("original"));
        assert_eq!(reply.user.name.as_deref(), Some("Bob"));
        // 浅く1階層のみ: 返信先の返信先は落とす(reply_id は残る)
        assert!(reply.reply.is_none());
        assert_eq!(reply.reply_id.as_deref(), Some("r0"));
    }

    #[test]
    fn reply_is_none_when_not_a_reply() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n3","createdAt":"2026-07-05T12:00:00.000Z","text":"hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public"
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert!(n.reply.is_none());
    }

    #[test]
    fn parses_user_emojis_for_display_name() {
        let raw: RawUser = serde_json::from_str(
            r#"{"id":"u1","username":"alice","name":"Alice :blobcat:",
                "emojis":{"blobcat":"http://x/e.png"}}"#,
        )
        .unwrap();
        let u: User = raw.into();
        assert_eq!(u.name.as_deref(), Some("Alice :blobcat:"));
        assert_eq!(u.emojis.get("blobcat").map(String::as_str), Some("http://x/e.png"));
    }

    #[test]
    fn parses_reaction_user() {
        let raw: RawReactionUser = serde_json::from_str(
            r#"{"id":"r1","createdAt":"2026-07-05T00:00:00Z","type":"👍",
                "user":{"id":"u1","username":"alice"}}"#,
        )
        .unwrap();
        let ru: ReactionUser = raw.into();
        assert_eq!(ru.reaction, "👍");
        assert_eq!(ru.user.id, "u1");
        assert_eq!(ru.user.username, "alice");
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

    #[test]
    fn raw_user_maps_description_to_bio_and_carries_banner_url() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0,
            "description":"hello world","bannerUrl":"https://example.com/banner.png"
        }"#;
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        assert_eq!(user.bio, Some("hello world".to_string()));
        assert_eq!(user.banner_url, Some("https://example.com/banner.png".to_string()));
    }

    #[test]
    fn raw_user_without_description_or_banner_defaults_to_none() {
        let json = r#"{
            "id":"u1","username":"alice","host":null,"name":"Alice",
            "avatarUrl":null,"isBot":false,"isCat":false,
            "followersCount":0,"followingCount":0,"notesCount":0
        }"#;
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        assert_eq!(user.bio, None);
        assert_eq!(user.banner_url, None);
    }

    /// リモートユーザーで未解決の場合、Misskeyはfollowers/following/notesCountに-1を返すことがある。
    /// u32では負値をそのままデシリアライズできないため、0に丸めて受け付けられることを確認する。
    #[test]
    fn raw_user_clamps_negative_counts_to_zero() {
        let json = r#"{
            "id":"u1","username":"alice","host":"remote.example",
            "followersCount":-1,"followingCount":-1,"notesCount":-1
        }"#;
        let raw: RawUser = serde_json::from_str(json).unwrap();
        assert_eq!(raw.followers_count, 0);
        assert_eq!(raw.following_count, 0);
        assert_eq!(raw.notes_count, 0);
    }

    #[test]
    fn raw_user_maps_instance_for_remote_user() {
        let json = "{\"id\":\"u1\",\"username\":\"alice\",\"host\":\"remote.example\",\"instance\":{\"name\":\"Remote Instance\",\"iconUrl\":\"https://remote.example/icon.png\",\"faviconUrl\":\"https://remote.example/favicon.png\",\"themeColor\":\"#ff8800\"}}";
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        let instance = user.instance.expect("instance should be present for remote user");
        assert_eq!(instance.name, Some("Remote Instance".to_string()));
        // 本家MkInstanceTicker.vueと同様、リモートはiconUrlではなくfaviconUrlを表示する。
        assert_eq!(instance.icon_url, Some("https://remote.example/favicon.png".to_string()));
        assert_eq!(instance.theme_color, Some("#ff8800".to_string()));
    }

    #[test]
    fn raw_user_has_no_instance_for_local_user() {
        let raw: RawUser =
            serde_json::from_str(r#"{"id":"u1","username":"alice","host":null}"#).unwrap();
        let user: User = raw.into();
        assert_eq!(user.instance, None);
    }

    #[test]
    fn raw_user_instance_falls_back_to_host_favicon_when_favicon_url_missing() {
        // 本家Misskeyと同様、faviconUrl/themeColorが無いインスタンス(管理者が未設定)では
        // ホストの/favicon.icoと既定グレー(#777777)にそれぞれフォールバックする。
        // iconUrlが設定されていても、faviconとは別物になりうるため使わない。
        let json = "{\"id\":\"u1\",\"username\":\"alice\",\"host\":\"remote.example\",\"instance\":{\"name\":\"Remote Instance\",\"iconUrl\":\"https://remote.example/icon.png\",\"faviconUrl\":null,\"themeColor\":null}}";
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        let instance = user.instance.expect("instance should be present for remote user");
        assert_eq!(instance.icon_url, Some("https://remote.example/favicon.ico".to_string()));
        assert_eq!(instance.theme_color, Some("#777777".to_string()));
    }

    #[test]
    fn raw_user_maps_avatar_blurhash_into_user() {
        let json = r#"{"id":"u1","username":"alice","avatarBlurhash":"LEHV6nWB2yk8pyo0adR*.7kCMdnj"}"#;
        let raw: RawUser = serde_json::from_str(json).unwrap();
        let user: User = raw.into();
        assert_eq!(user.avatar_blurhash.as_deref(), Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj"));
    }

    #[test]
    fn raw_file_size_is_mapped() {
        let raw: RawFile = serde_json::from_str(
            r#"{"id":"f1","type":"application/pdf","url":"http://x/f1","name":"a.pdf","size":1234}"#,
        )
        .unwrap();
        let f: DriveFile = raw.into();
        assert_eq!(f.size, Some(1234));

        let raw: RawFile = serde_json::from_str(r#"{"id":"f2"}"#).unwrap();
        let f: DriveFile = raw.into();
        assert_eq!(f.size, None);
    }
}
