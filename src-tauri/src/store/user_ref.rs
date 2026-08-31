//! `user` テーブル(正規化済みユーザー情報)への読み書きと、note payload 内の
//! user 参照(スタブ `{"id": ...}` ⇔ フル `User`)の変換ヘルパー(Issue #263)。

use crate::domain::{Note, User};
use crate::error::Result;
use rusqlite::{params, Connection};

/// `user` テーブルへ upsert する。UserLite に常に含まれる列は常に最新値で上書きし、
/// UserLite では省略されうる列(`bio`/`banner_url`/`instance_*`)は、新しい値が
/// `NULL` のときは既存値を保持する(COALESCE)。これにより:
/// - フルユーザー取得(bio/banner込み)の後、ノート受信(UserLiteのみ)の `NULL` で
///   既存の bio/banner_url を踏み潰さない。
/// - `instance` フェッチが一時的に失敗した投稿(`"instance":null`)が、既に分かっている
///   instance を消さない。
pub(crate) fn upsert_user(conn: &Connection, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    conn.execute(
        "INSERT INTO user (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            host = excluded.host,
            name = excluded.name,
            avatar_url = excluded.avatar_url,
            is_bot = excluded.is_bot,
            is_cat = excluded.is_cat,
            followers_count = excluded.followers_count,
            following_count = excluded.following_count,
            notes_count = excluded.notes_count,
            emojis = excluded.emojis,
            bio = COALESCE(excluded.bio, user.bio),
            banner_url = COALESCE(excluded.banner_url, user.banner_url),
            instance_name = COALESCE(excluded.instance_name, user.instance_name),
            instance_icon_url = COALESCE(excluded.instance_icon_url, user.instance_icon_url),
            instance_theme_color = COALESCE(excluded.instance_theme_color, user.instance_theme_color)",
        params![
            user.id,
            user.username,
            user.host,
            user.name,
            user.avatar_url,
            user.is_bot as i64,
            user.is_cat as i64,
            user.followers_count,
            user.following_count,
            user.notes_count,
            emojis_json,
            user.bio,
            user.banner_url,
            instance_name,
            instance_icon_url,
            instance_theme_color,
        ],
    )?;
    Ok(())
}

/// ノート本体+renote(入れ子)分の User をすべて集める(重複排除はしない)。
/// upsert_note が「note.payload に埋め込まれる全ユーザー」をキャッシュへ反映するために使う。
pub(crate) fn collect_users(note: &Note) -> Vec<&User> {
    let mut out = vec![&note.user];
    if let Some(renote) = &note.renote {
        out.extend(collect_users(renote));
    }
    out
}

/// note_value の `user`(本体+renote分)を `{"id": ...}` スタブへ差し替える。
/// upsert_note の保存直前に呼び、payload に生ユーザー情報を持たせない。
pub(crate) fn stub_user_refs(note_value: &mut serde_json::Value) {
    if let Some(id) = note_value.get("user").and_then(|u| u.get("id")).cloned() {
        note_value["user"] = serde_json::json!({ "id": id });
    }
    if note_value.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        stub_user_refs(&mut note_value["renote"]);
    }
}

/// user_value が旧形式(フルオブジェクト埋め込み)かどうか。スタブは `id` のみなので
/// `username` の有無で判定する(UserLiteは常にusernameを含む)。
pub(crate) fn is_legacy_full_user(user_value: &serde_json::Value) -> bool {
    user_value.get("username").is_some()
}

/// note_value の `user`(本体+renote分)のいずれかが旧形式かどうかを再帰的に判定する。
pub(crate) fn has_legacy_full_user(note_value: &serde_json::Value) -> bool {
    if note_value.get("user").map(is_legacy_full_user).unwrap_or(false) {
        return true;
    }
    note_value.get("renote").map(has_legacy_full_user).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DriveFile, InstanceInfo, Visibility};
    use crate::store::db::open_cache_in_memory;
    use std::collections::HashMap;
    use std::collections::HashMap as Map;

    fn user_lite(id: &str, name: &str) -> User {
        User {
            id: id.into(),
            username: "alice".into(),
            host: Some("remote.example".into()),
            name: Some(name.into()),
            avatar_url: Some("https://remote.example/a.png".into()),
            is_bot: false,
            is_cat: false,
            followers_count: 1,
            following_count: 2,
            notes_count: 3,
            emojis: HashMap::new(),
            bio: None,
            banner_url: None,
            instance: None,
        }
    }

    fn bare_note(id: &str, user: User) -> Note {
        Note {
            id: id.into(),
            created_at: 100,
            text: Some("hi".into()),
            cw: None,
            visibility: Visibility::Home,
            local_only: false,
            user,
            reply_id: None,
            renote_id: None,
            renote: None,
            files: Vec::<DriveFile>::new(),
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: Map::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: Map::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    fn row(conn: &Connection, id: &str) -> (Option<String>, Option<String>, Option<String>) {
        conn.query_row(
            "SELECT bio, banner_url, instance_name FROM user WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    }

    #[test]
    fn upsert_user_preserves_bio_when_later_write_has_none() {
        let conn = open_cache_in_memory().unwrap();
        let mut full = user_lite("u1", "Alice");
        full.bio = Some("hello".into());
        upsert_user(&conn, &full).unwrap();

        // 後続の UserLite 由来の書き込み(bio=None)
        let lite = user_lite("u1", "Alice (updated)");
        upsert_user(&conn, &lite).unwrap();

        let (bio, _, _) = row(&conn, "u1");
        assert_eq!(bio, Some("hello".to_string()));
    }

    #[test]
    fn upsert_user_overwrites_always_present_fields() {
        let conn = open_cache_in_memory().unwrap();
        upsert_user(&conn, &user_lite("u1", "Alice")).unwrap();
        upsert_user(&conn, &user_lite("u1", "Alice2")).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM user WHERE id = 'u1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "Alice2");
    }

    #[test]
    fn upsert_user_preserves_instance_when_later_fetch_fails() {
        let conn = open_cache_in_memory().unwrap();
        let mut with_instance = user_lite("u1", "Alice");
        with_instance.instance = Some(InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/icon.png".into()),
            theme_color: Some("#ff8800".into()),
        });
        upsert_user(&conn, &with_instance).unwrap();

        // instance フェッチ失敗(null)の投稿を後から受信
        let mut failed_fetch = user_lite("u1", "Alice");
        failed_fetch.instance = None;
        upsert_user(&conn, &failed_fetch).unwrap();

        let (_, _, instance_name) = row(&conn, "u1");
        assert_eq!(instance_name, Some("Remote".to_string()));
    }

    #[test]
    fn collect_users_returns_just_author_when_no_renote() {
        let n = bare_note("n1", user_lite("u1", "Alice"));
        let users = collect_users(&n);
        assert_eq!(users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1"]);
    }

    #[test]
    fn collect_users_includes_renote_author() {
        let mut n = bare_note("n1", user_lite("u1", "Alice"));
        n.renote = Some(Box::new(bare_note("n0", user_lite("u2", "Bob"))));
        let users = collect_users(&n);
        assert_eq!(users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1", "u2"]);
    }

    use serde_json::json;

    #[test]
    fn stub_user_refs_replaces_top_level_user_with_id_only() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1", "username": "alice", "isBot": false }
        });
        stub_user_refs(&mut v);
        assert_eq!(v["user"], json!({ "id": "u1" }));
    }

    #[test]
    fn stub_user_refs_recurses_into_renote() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1", "username": "alice" },
            "renote": {
                "id": "n0",
                "user": { "id": "u2", "username": "bob" },
                "renote": null
            }
        });
        stub_user_refs(&mut v);
        assert_eq!(v["user"], json!({ "id": "u1" }));
        assert_eq!(v["renote"]["user"], json!({ "id": "u2" }));
    }

    #[test]
    fn is_legacy_full_user_true_for_object_with_username_false_for_stub() {
        assert!(is_legacy_full_user(&json!({ "id": "u1", "username": "alice" })));
        assert!(!is_legacy_full_user(&json!({ "id": "u1" })));
    }

    #[test]
    fn has_legacy_full_user_detects_legacy_shape_in_renote_only() {
        let v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2", "username": "bob" } }
        });
        assert!(has_legacy_full_user(&v));

        let already_stubbed = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" } }
        });
        assert!(!has_legacy_full_user(&already_stubbed));
    }
}
