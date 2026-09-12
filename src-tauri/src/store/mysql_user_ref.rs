//! `user`テーブル(正規化済みユーザー情報)への読み書き(MySQL版)。
//! ロジックは`store/postgres_user_ref.rs`(Postgres版)と等価。UPSERT構文のみ異なる
//! (`ON CONFLICT ... DO UPDATE SET x = excluded.x` → `ON DUPLICATE KEY UPDATE x = VALUES(x)`)。
//! DB非依存の純粋関数(`stub_user_refs`等)は`user_ref.rs`のものをそのまま再利用する。

use crate::domain::{InstanceInfo, User};
use crate::error::Result;
use std::collections::HashMap;

pub(crate) async fn upsert_user(pool: &sqlx::MySqlPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO `user` (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color,
            avatar_blurhash
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            username = VALUES(username),
            host = VALUES(host),
            name = VALUES(name),
            avatar_url = VALUES(avatar_url),
            is_bot = VALUES(is_bot),
            is_cat = VALUES(is_cat),
            followers_count = VALUES(followers_count),
            following_count = VALUES(following_count),
            notes_count = VALUES(notes_count),
            emojis = VALUES(emojis),
            bio = COALESCE(VALUES(bio), bio),
            banner_url = COALESCE(VALUES(banner_url), banner_url),
            instance_name = COALESCE(VALUES(instance_name), instance_name),
            instance_icon_url = COALESCE(VALUES(instance_icon_url), instance_icon_url),
            instance_theme_color = COALESCE(VALUES(instance_theme_color), instance_theme_color),
            avatar_blurhash = COALESCE(VALUES(avatar_blurhash), avatar_blurhash)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&instance_name)
    .bind(&instance_icon_url)
    .bind(&instance_theme_color)
    .bind(&user.avatar_blurhash)
    .execute(pool)
    .await?;
    Ok(())
}

/// 自己修復パス専用のupsert(`postgres_user_ref.rs::fill_user_from_snapshot`と同じ規約:
/// 全列を「既存値が無い場合のみ埋める」。詳細はそちらのdocコメント参照)。
/// MySQLの`ON DUPLICATE KEY UPDATE`では、挿入直前の既存行の値は列名をそのまま
/// (`col`)、新しく挿入しようとした値は`VALUES(col)`で参照する — Postgresの
/// `"user".col`(既存値)/`excluded.col`(新値)と役割の対応が逆になる点に注意。
pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::MySqlPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO `user` (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color,
            avatar_blurhash
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON DUPLICATE KEY UPDATE
            username = COALESCE(username, VALUES(username)),
            host = COALESCE(host, VALUES(host)),
            name = COALESCE(name, VALUES(name)),
            avatar_url = COALESCE(avatar_url, VALUES(avatar_url)),
            is_bot = is_bot,
            is_cat = is_cat,
            followers_count = followers_count,
            following_count = following_count,
            notes_count = notes_count,
            emojis = COALESCE(NULLIF(emojis, '{}'), VALUES(emojis)),
            bio = COALESCE(bio, VALUES(bio)),
            banner_url = COALESCE(banner_url, VALUES(banner_url)),
            instance_name = COALESCE(instance_name, VALUES(instance_name)),
            instance_icon_url = COALESCE(instance_icon_url, VALUES(instance_icon_url)),
            instance_theme_color = COALESCE(instance_theme_color, VALUES(instance_theme_color)),
            avatar_blurhash = COALESCE(avatar_blurhash, VALUES(avatar_blurhash))",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.host)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(user.is_bot)
    .bind(user.is_cat)
    .bind(user.followers_count as i64)
    .bind(user.following_count as i64)
    .bind(user.notes_count as i64)
    .bind(&emojis_json)
    .bind(&user.bio)
    .bind(&user.banner_url)
    .bind(&instance_name)
    .bind(&instance_icon_url)
    .bind(&instance_theme_color)
    .bind(&user.avatar_blurhash)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn fetch_users_by_ids(pool: &sqlx::MySqlPool, ids: &[String]) -> Result<HashMap<String, User>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    // MySQLのsqlxドライバは配列バインドをサポートしないため、`IN (?,?,...)`を
    // 要素数ぶん動的に組み立てる(Global Constraints参照)。
    // 列数が17でsqlx `FromRow`のタプル実装上限(16)を超えるため、タプルではなく
    // `sqlx::Row`から列名で直接取り出す(`sqlx::Row`トレイトの`try_get`を使う)。
    use sqlx::Row;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color,
                avatar_blurhash
         FROM `user` WHERE id IN ({placeholders})"
    );
    // `placeholders`は`ids.len()`個の`?`を繰り返し連結しただけ(値そのものは含まない)で、
    // 各`id`の値は必ず`.bind()`経由で渡すため、`sqlx::AssertSqlSafe`でのラップは安全(監査済み)。
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;

    for row in rows {
        let id: String = row.try_get("id")?;
        let username: String = row.try_get("username")?;
        let host: Option<String> = row.try_get("host")?;
        let name: Option<String> = row.try_get("name")?;
        let avatar_url: Option<String> = row.try_get("avatar_url")?;
        let is_bot: bool = row.try_get("is_bot")?;
        let is_cat: bool = row.try_get("is_cat")?;
        let followers_count: i64 = row.try_get("followers_count")?;
        let following_count: i64 = row.try_get("following_count")?;
        let notes_count: i64 = row.try_get("notes_count")?;
        let emojis_json: String = row.try_get("emojis")?;
        let bio: Option<String> = row.try_get("bio")?;
        let banner_url: Option<String> = row.try_get("banner_url")?;
        let instance_name: Option<String> = row.try_get("instance_name")?;
        let instance_icon_url: Option<String> = row.try_get("instance_icon_url")?;
        let instance_theme_color: Option<String> = row.try_get("instance_theme_color")?;
        let avatar_blurhash: Option<String> = row.try_get("avatar_blurhash")?;
        let emojis: HashMap<String, String> = serde_json::from_str(&emojis_json).unwrap_or_default();
        let instance = if instance_name.is_some() || instance_icon_url.is_some() || instance_theme_color.is_some() {
            Some(InstanceInfo { name: instance_name, icon_url: instance_icon_url, theme_color: instance_theme_color })
        } else {
            None
        };
        out.insert(
            id.clone(),
            User {
                id, username, host, name, avatar_url,
                is_bot, is_cat,
                followers_count: followers_count as u32,
                following_count: following_count as u32,
                notes_count: notes_count as u32,
                emojis, bio, banner_url, avatar_blurhash, instance,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::User;
    use testcontainers_modules::{mysql::Mysql, testcontainers::runners::AsyncRunner};

    async fn pool() -> sqlx::MySqlPool {
        let container = Mysql::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(3306).await.unwrap();
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect(&format!("mysql://root@127.0.0.1:{port}/test"))
            .await
            .unwrap();
        crate::store::mysql_backend::ensure_schema(&pool).await.unwrap();
        std::mem::forget(container);
        pool
    }

    fn user(id: &str) -> User {
        User {
            id: id.into(), username: "alice".into(), host: None, name: Some("Alice".into()),
            avatar_url: None, is_bot: false, is_cat: false,
            followers_count: 5, following_count: 3, notes_count: 42,
            emojis: HashMap::new(), bio: None, banner_url: None, avatar_blurhash: None, instance: None,
        }
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_roundtrip() {
        let pool = pool().await;
        upsert_user(&pool, &user("u1")).await.unwrap();
        let got = fetch_users_by_ids(&pool, &["u1".to_string()]).await.unwrap();
        assert_eq!(got.get("u1").unwrap().name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_roundtrips_avatar_blurhash() {
        let pool = pool().await;
        let mut u = user("u1");
        u.avatar_blurhash = Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into());
        upsert_user(&pool, &u).await.unwrap();

        let got = fetch_users_by_ids(&pool, &["u1".to_string()]).await.unwrap();
        assert_eq!(
            got.get("u1").unwrap().avatar_blurhash.as_deref(),
            Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj")
        );
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_user_preserves_bio_when_later_write_has_none() {
        let pool = pool().await;
        let mut u = user("u1");
        u.bio = Some("hello".into());
        upsert_user(&pool, &u).await.unwrap();

        u.bio = None;
        upsert_user(&pool, &u).await.unwrap();

        let got = fetch_users_by_ids(&pool, &["u1".to_string()]).await.unwrap();
        assert_eq!(got.get("u1").unwrap().bio.as_deref(), Some("hello"), "bioは既存値を保持すること");
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_users_by_ids_returns_empty_map_for_empty_input() {
        let pool = pool().await;
        let got = fetch_users_by_ids(&pool, &[]).await.unwrap();
        assert!(got.is_empty());
    }
}
