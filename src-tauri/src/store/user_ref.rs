//! `user` テーブル(正規化済みユーザー情報)への読み書きと、note payload 内の
//! user 参照(スタブ `{"id": ...}` ⇔ フル `User`)の変換ヘルパー(Issue #263)。

use crate::domain::{InstanceInfo, Note, User};
use crate::error::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;

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

/// 自己修復パス専用のupsert。ペイロードは「そのノートがキャッシュされた時点のスナップショット」
/// であり最新とは限らないため、`upsert_user`(ライブ書き込みパス、常に最新のUserLiteを前提に
/// 常時上書き)と異なり、**全列**を「既存値が無い場合のみ埋める」方針にする(Issue #263 最終レビュー指摘)。
/// これにより、古いノートを読んだだけで直近の name/avatar_url/emojis/*_count が
/// 古いスナップショットで上書きされる回帰を防ぐ。
/// `is_bot`/`is_cat`/`*_count`は0がデフォルト値であり「値が無い」ことを表現できないため、
/// これらは常に既存値を維持する(=スナップショット側の値は無視する)。
/// `emojis`は`NOT NULL DEFAULT '{}'`で明示的なNULLを取れないため、
/// 「既存値が空オブジェクト('{}')なら埋める」という扱いにする(NULLIFで擬似NULL化)。
pub(crate) fn fill_user_from_snapshot(conn: &Connection, user: &User) -> Result<()> {
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
            username = COALESCE(user.username, excluded.username),
            host = COALESCE(user.host, excluded.host),
            name = COALESCE(user.name, excluded.name),
            avatar_url = COALESCE(user.avatar_url, excluded.avatar_url),
            is_bot = user.is_bot,
            is_cat = user.is_cat,
            followers_count = user.followers_count,
            following_count = user.following_count,
            notes_count = user.notes_count,
            emojis = COALESCE(NULLIF(user.emojis, '{}'), excluded.emojis),
            bio = COALESCE(user.bio, excluded.bio),
            banner_url = COALESCE(user.banner_url, excluded.banner_url),
            instance_name = COALESCE(user.instance_name, excluded.instance_name),
            instance_icon_url = COALESCE(user.instance_icon_url, excluded.instance_icon_url),
            instance_theme_color = COALESCE(user.instance_theme_color, excluded.instance_theme_color)",
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

/// note_value の `user.id`(本体+renote分)を出現順にすべて集める(重複可、呼び出し元で
/// dedupする想定)。stub_user_refs 済み・旧形式どちらの形にも対応する(常に `["user"]["id"]`
/// を見るだけなので形式を問わない)。
pub(crate) fn collect_user_id_refs(note_value: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(id) = note_value.get("user").and_then(|u| u.get("id")).and_then(|v| v.as_str()) {
        out.push(id.to_string());
    }
    if let Some(renote) = note_value.get("renote") {
        if renote.is_object() {
            collect_user_id_refs(renote, out);
        }
    }
}

/// note_value の `user` スタブ(本体+renote分)を users から引いてフルオブジェクトへ埋め戻す。
/// 参照先のいずれかが users に無ければ false を返す(このノートは復元不可、呼び出し元でスキップする)。
pub(crate) fn hydrate_user_refs(note_value: &mut serde_json::Value, users: &HashMap<String, User>) -> bool {
    let Some(id) = note_value
        .get("user")
        .and_then(|u| u.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        return false;
    };
    let Some(user) = users.get(&id) else {
        return false;
    };
    note_value["user"] = serde_json::to_value(user).unwrap_or(serde_json::Value::Null);

    if note_value.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        return hydrate_user_refs(&mut note_value["renote"], users);
    }
    true
}

/// `user` テーブルから id 一覧に対応する行をまとめて引く(1クエリ、N+1にしない)。
/// 見つからない id は結果のマップに含まれない(呼び出し元は hydrate_user_refs の false で検知する)。
pub(crate) fn fetch_users_by_ids(conn: &Connection, ids: &[String]) -> Result<HashMap<String, User>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color
         FROM user WHERE id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let bind_params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(bind_params.as_slice(), |r| {
        let emojis_json: String = r.get(10)?;
        let instance_name: Option<String> = r.get(13)?;
        let instance_icon_url: Option<String> = r.get(14)?;
        let instance_theme_color: Option<String> = r.get(15)?;
        let instance = if instance_name.is_some() || instance_icon_url.is_some() || instance_theme_color.is_some() {
            Some(InstanceInfo {
                name: instance_name,
                icon_url: instance_icon_url,
                theme_color: instance_theme_color,
            })
        } else {
            None
        };
        Ok(User {
            id: r.get(0)?,
            username: r.get(1)?,
            host: r.get(2)?,
            name: r.get(3)?,
            avatar_url: r.get(4)?,
            is_bot: r.get::<_, i64>(5)? != 0,
            is_cat: r.get::<_, i64>(6)? != 0,
            followers_count: r.get(7)?,
            following_count: r.get(8)?,
            notes_count: r.get(9)?,
            emojis: serde_json::from_str(&emojis_json).unwrap_or_default(),
            bio: r.get(11)?,
            banner_url: r.get(12)?,
            instance,
        })
    })?;
    for row in rows {
        let user = row?;
        out.insert(user.id.clone(), user);
    }
    Ok(out)
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
    fn fill_user_from_snapshot_does_not_clobber_fresher_live_data() {
        let conn = open_cache_in_memory().unwrap();
        // 直近のライブ書き込み(upsert_note経由を想定)で最新のemojis/nameが入っている
        let mut fresh = user_lite("u1", "Alice (new name)");
        fresh.emojis = HashMap::from([("wave".to_string(), "https://example.com/wave.png".to_string())]);
        upsert_user(&conn, &fresh).unwrap();

        // 数年前にキャッシュされた古いノートを自己修復で読む: 古いname、emojisキー無し(=空マップ)
        let mut stale_snapshot = user_lite("u1", "Alice (old name)");
        stale_snapshot.emojis = HashMap::new();
        fill_user_from_snapshot(&conn, &stale_snapshot).unwrap();

        let (name, emojis_json): (String, String) = conn
            .query_row("SELECT name, emojis FROM user WHERE id = 'u1'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(name, "Alice (new name)", "古いスナップショットが直近のnameを上書きしてはいけない");
        assert!(emojis_json.contains("wave"), "古いスナップショットが直近のemojisを消してはいけない");
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

    #[test]
    fn collect_user_id_refs_collects_top_level_and_renote() {
        let v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" }, "renote": null }
        });
        let mut ids = Vec::new();
        collect_user_id_refs(&v, &mut ids);
        assert_eq!(ids, vec!["u1".to_string(), "u2".to_string()]);
    }

    #[test]
    fn hydrate_user_refs_fills_in_full_user_and_returns_true() {
        let mut v = json!({ "id": "n1", "user": { "id": "u1" } });
        let mut users = HashMap::new();
        users.insert("u1".to_string(), user_lite("u1", "Alice"));

        assert!(hydrate_user_refs(&mut v, &users));
        assert_eq!(v["user"]["username"], json!("alice"));
        assert_eq!(v["user"]["name"], json!("Alice"));
    }

    #[test]
    fn hydrate_user_refs_returns_false_when_user_missing() {
        let mut v = json!({ "id": "n1", "user": { "id": "u1" } });
        let users = HashMap::new();
        assert!(!hydrate_user_refs(&mut v, &users));
    }

    #[test]
    fn hydrate_user_refs_hydrates_renote_author_too() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" }, "renote": null }
        });
        let mut users = HashMap::new();
        users.insert("u1".to_string(), user_lite("u1", "Alice"));
        users.insert("u2".to_string(), user_lite("u2", "Bob"));

        assert!(hydrate_user_refs(&mut v, &users));
        assert_eq!(v["renote"]["user"]["name"], json!("Bob"));
    }

    #[test]
    fn fetch_users_by_ids_returns_stored_instance_info() {
        let conn = open_cache_in_memory().unwrap();
        let mut with_instance = user_lite("u1", "Alice");
        with_instance.instance = Some(InstanceInfo {
            name: Some("Remote".into()),
            icon_url: Some("https://remote.example/icon.png".into()),
            theme_color: Some("#ff8800".into()),
        });
        upsert_user(&conn, &with_instance).unwrap();
        upsert_user(&conn, &user_lite("u2", "Bob")).unwrap(); // instance無し

        let ids = vec!["u1".to_string(), "u2".to_string(), "u3".to_string()];
        let users = fetch_users_by_ids(&conn, &ids).unwrap();

        assert_eq!(users.len(), 2); // u3 は存在しないので含まれない
        let instance = users["u1"].instance.as_ref().unwrap();
        assert_eq!(instance.name.as_deref(), Some("Remote"));
        assert!(users["u2"].instance.is_none());
    }

    #[test]
    fn fetch_users_by_ids_returns_empty_map_for_empty_input() {
        let conn = open_cache_in_memory().unwrap();
        let users = fetch_users_by_ids(&conn, &[]).unwrap();
        assert!(users.is_empty());
    }
}
