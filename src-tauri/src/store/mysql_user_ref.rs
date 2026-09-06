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
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            instance_theme_color = COALESCE(VALUES(instance_theme_color), instance_theme_color)",
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
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            instance_theme_color = COALESCE(instance_theme_color, VALUES(instance_theme_color))",
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
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color
         FROM `user` WHERE id IN ({placeholders})"
    );
    let mut query = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, bool, bool, i64, i64, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;

    for (id, username, host, name, avatar_url, is_bot, is_cat, followers_count, following_count, notes_count, emojis_json, bio, banner_url, instance_name, instance_icon_url, instance_theme_color) in rows {
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
                emojis, bio, banner_url, instance,
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
            emojis: HashMap::new(), bio: None, banner_url: None, instance: None,
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
