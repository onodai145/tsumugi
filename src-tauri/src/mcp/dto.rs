//! MCP が返すノートの形(Issue #557)。`domain::Note` を直接シリアライズせず、ここで許可した
//! フィールドだけに**明示的に詰め替える**(将来 `Note` にフィールドが増えても、自動では外に出ない)。
//!
//! 返さないもの: 返信先・引用元の本文(IDのみ)、アカウント視点のフィールド(`my_reaction` /
//! `is_*_by_me` はアカウント間で混ざる)、`is_pinned`、メンション先のID、ファイルのURL。

use crate::domain::{Note, Visibility};
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    pub id: String,
    /// そのアカウントのホストの `/notes/<id>`。引用元として示せる。
    pub url: String,
    /// ISO 8601(UTC)
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cw: Option<String>,
    pub visibility: &'static str,
    pub user: UserDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renote_id: Option<String>,
    pub tags: Vec<String>,
    pub files: Vec<FileDto>,
    pub reaction_count: u32,
    pub renote_count: u32,
    pub reply_count: u32,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: String,
    /// `@name` または `@name@host`
    pub acct: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub is_bot: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileDto {
    pub name: String,
    pub mime_type: String,
    pub is_sensitive: bool,
}

/// 公開(Public / Home)で、ローカル限定でないノートだけを DTO にする。それ以外は `None`。
/// SQL のスコープ条件に加えた二重の安全(payload と列が食い違っても外に出さない)。
pub fn from_note(note: &Note, account_host: &str) -> Option<NoteDto> {
    let visibility = match note.visibility {
        Visibility::Public => "public",
        Visibility::Home => "home",
        Visibility::Followers | Visibility::Specified => return None,
    };
    if note.local_only {
        return None;
    }
    let created_at = chrono::DateTime::from_timestamp(note.created_at, 0)?.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let acct = match &note.user.host {
        Some(h) => format!("@{}@{}", note.user.username, h),
        None => format!("@{}", note.user.username),
    };
    Some(NoteDto {
        id: note.id.clone(),
        url: format!("https://{}/notes/{}", account_host, note.id),
        created_at,
        text: note.text.clone(),
        cw: note.cw.clone(),
        visibility,
        user: UserDto { id: note.user.id.clone(), acct, name: note.user.name.clone(), is_bot: note.user.is_bot },
        reply_id: note.reply_id.clone(),
        renote_id: note.renote_id.clone(),
        tags: note.tags.clone(),
        files: note
            .files
            .iter()
            .map(|f| FileDto { name: f.name.clone(), mime_type: f.mime_type.clone(), is_sensitive: f.is_sensitive })
            .collect(),
        reaction_count: note.reaction_count,
        renote_count: note.renote_count,
        reply_count: note.reply_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DriveFile, Visibility};
    use crate::mcp::testutil::{note, user};
    use std::collections::BTreeSet;

    /// 許可するキーの全体。ここに無いキーが出力に現れたらテストが落ちる(新しいフィールドが
    /// 自動で外に出ないことの固定)。
    const ALLOWED_KEYS: &[&str] = &[
        "id", "url", "createdAt", "text", "cw", "visibility", "user", "replyId", "renoteId", "tags", "files",
        "reactionCount", "renoteCount", "replyCount",
    ];

    fn full_note() -> crate::domain::Note {
        let mut n = note("n1", 1_793_491_200, Visibility::Public, false, "hello #rust", user("u1", "alice", Some("example.com")));
        n.cw = Some("cw".into());
        n.reply_id = Some("r1".into());
        n.renote_id = Some("q1".into());
        // 返信先・引用元の本文は、IDだけ返して本文は返さない。
        n.reply = Some(Box::new(note("r1", 1, Visibility::Followers, false, "SECRET reply body", user("u2", "bob", None))));
        n.renote = Some(Box::new(note("q1", 1, Visibility::Followers, false, "SECRET quote body", user("u3", "carol", None))));
        n.tags = vec!["rust".into()];
        n.files = vec![DriveFile {
            id: "f1".into(),
            mime_type: "image/png".into(),
            is_sensitive: true,
            url: "https://example.com/secret-file-url.png".into(),
            thumbnail_url: Some("https://example.com/thumb.png".into()),
            name: "pic.png".into(),
            size: Some(10),
        }];
        n.mentions = vec!["u9".into()];
        n.reaction_count = 3;
        n.renote_count = 2;
        n.reply_count = 1;
        // アカウント視点のフィールド(共有の note 行に入っていて、アカウント間で混ざる)。
        n.my_reaction = Some("👍".into());
        n.is_renoted_by_me = true;
        n.is_favorited_by_me = true;
        n.is_pinned = true;
        n
    }

    #[test]
    fn output_keys_are_exactly_the_allowed_set() {
        let dto = from_note(&full_note(), "example.com").unwrap();
        let v = serde_json::to_value(&dto).unwrap();
        let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        let allowed: BTreeSet<&str> = ALLOWED_KEYS.iter().copied().collect();
        assert_eq!(keys, allowed);
    }

    #[test]
    fn nothing_sensitive_leaks_into_the_serialized_text() {
        let json = serde_json::to_string(&from_note(&full_note(), "example.com").unwrap()).unwrap();
        for forbidden in ["SECRET", "myReaction", "isRenotedByMe", "isFavoritedByMe", "isPinned", "secret-file-url", "thumb.png", "u9", "👍"] {
            assert!(!json.contains(forbidden), "出力に {forbidden} が含まれている: {json}");
        }
    }

    #[test]
    fn fields_are_mapped_as_specified() {
        let v = serde_json::to_value(from_note(&full_note(), "viewer.example").unwrap()).unwrap();
        assert_eq!(v["id"], "n1");
        assert_eq!(v["url"], "https://viewer.example/notes/n1", "アカウントのホストから組み立てる");
        assert_eq!(v["createdAt"], "2026-11-01T00:00:00Z");
        assert_eq!(v["visibility"], "public");
        assert_eq!(v["user"]["acct"], "@alice@example.com");
        assert_eq!(v["user"]["name"], "ALICE");
        assert_eq!(v["user"]["isBot"], false);
        assert_eq!(v["replyId"], "r1");
        assert_eq!(v["renoteId"], "q1");
        assert_eq!(v["files"][0]["name"], "pic.png");
        assert_eq!(v["files"][0]["mimeType"], "image/png");
        assert_eq!(v["files"][0]["isSensitive"], true);
        assert_eq!(v["reactionCount"], 3);
    }

    #[test]
    fn local_user_acct_has_no_host_part_and_none_fields_are_omitted() {
        let mut n = note("n2", 0, Visibility::Home, false, "x", user("u1", "alice", None));
        n.text = None;
        let v = serde_json::to_value(from_note(&n, "example.com").unwrap()).unwrap();
        assert_eq!(v["user"]["acct"], "@alice");
        assert_eq!(v["visibility"], "home");
        for absent in ["text", "cw", "replyId", "renoteId"] {
            assert!(v.get(absent).is_none(), "{absent} は None のとき省略する");
        }
    }

    #[test]
    fn notes_outside_public_or_home_or_local_only_are_never_converted() {
        // SQLのスコープ条件に加えた、二重の安全(payload と列が食い違っても外に出さない)。
        for vis in [Visibility::Followers, Visibility::Specified] {
            assert!(from_note(&note("n", 0, vis, false, "x", user("u1", "a", None)), "h").is_none());
        }
        assert!(from_note(&note("n", 0, Visibility::Public, true, "x", user("u1", "a", None)), "h").is_none());
    }
}
