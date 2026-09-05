//! `user`テーブル(正規化済みユーザー情報)への読み書き(Postgres版)。
//! ロジックは`store/user_ref.rs`の同名関数(SQLite版)と等価。
//! DB非依存の純粋関数(`stub_user_refs`等)は`user_ref.rs`のものをそのまま再利用する。

use crate::domain::{InstanceInfo, User};
use crate::error::Result;
use std::collections::HashMap;

pub(crate) async fn upsert_user(pool: &sqlx::PgPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO \"user\" (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        ON CONFLICT (id) DO UPDATE SET
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
            bio = COALESCE(excluded.bio, \"user\".bio),
            banner_url = COALESCE(excluded.banner_url, \"user\".banner_url),
            instance_name = COALESCE(excluded.instance_name, \"user\".instance_name),
            instance_icon_url = COALESCE(excluded.instance_icon_url, \"user\".instance_icon_url),
            instance_theme_color = COALESCE(excluded.instance_theme_color, \"user\".instance_theme_color)",
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

/// 自己修復パス専用のupsert(`user_ref.rs::fill_user_from_snapshot`と同じ規約:
/// 全列を「既存値が無い場合のみ埋める」。詳細は`user_ref.rs`のdocコメント参照)。
pub(crate) async fn fill_user_from_snapshot(pool: &sqlx::PgPool, user: &User) -> Result<()> {
    let emojis_json = serde_json::to_string(&user.emojis)?;
    let (instance_name, instance_icon_url, instance_theme_color) = match &user.instance {
        Some(i) => (i.name.clone(), i.icon_url.clone(), i.theme_color.clone()),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO \"user\" (
            id, username, host, name, avatar_url, is_bot, is_cat,
            followers_count, following_count, notes_count, emojis,
            bio, banner_url, instance_name, instance_icon_url, instance_theme_color
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        ON CONFLICT (id) DO UPDATE SET
            username = COALESCE(\"user\".username, excluded.username),
            host = COALESCE(\"user\".host, excluded.host),
            name = COALESCE(\"user\".name, excluded.name),
            avatar_url = COALESCE(\"user\".avatar_url, excluded.avatar_url),
            is_bot = \"user\".is_bot,
            is_cat = \"user\".is_cat,
            followers_count = \"user\".followers_count,
            following_count = \"user\".following_count,
            notes_count = \"user\".notes_count,
            emojis = COALESCE(NULLIF(\"user\".emojis, '{}'), excluded.emojis),
            bio = COALESCE(\"user\".bio, excluded.bio),
            banner_url = COALESCE(\"user\".banner_url, excluded.banner_url),
            instance_name = COALESCE(\"user\".instance_name, excluded.instance_name),
            instance_icon_url = COALESCE(\"user\".instance_icon_url, excluded.instance_icon_url),
            instance_theme_color = COALESCE(\"user\".instance_theme_color, excluded.instance_theme_color)",
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

pub(crate) async fn fetch_users_by_ids(pool: &sqlx::PgPool, ids: &[String]) -> Result<HashMap<String, User>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, bool, bool, i64, i64, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT id, username, host, name, avatar_url, is_bot, is_cat,
                followers_count, following_count, notes_count, emojis,
                bio, banner_url, instance_name, instance_icon_url, instance_theme_color
         FROM \"user\" WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

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
                id,
                username,
                host,
                name,
                avatar_url,
                is_bot,
                is_cat,
                followers_count: followers_count as u32,
                following_count: following_count as u32,
                notes_count: notes_count as u32,
                emojis,
                bio,
                banner_url,
                instance,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::User;
    use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

    async fn pool() -> sqlx::PgPool {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"))
            .await
            .unwrap();
        crate::store::postgres_backend::ensure_schema(&pool).await.unwrap();
        // testcontainersのcontainerハンドルはpoolが生きている間ドロップされると
        // コンテナが止まるため、リークさせてテストプロセス終了まで保持する。
        std::mem::forget(container);
        pool
    }

    fn user(id: &str) -> User {
        User {
            id: id.into(),
            username: "alice".into(),
            host: None,
            name: Some("Alice".into()),
            avatar_url: None,
            is_bot: false,
            is_cat: false,
            followers_count: 5,
            following_count: 3,
            notes_count: 42,
            emojis: HashMap::new(),
            bio: None,
            banner_url: None,
            instance: None,
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

        u.bio = None; // ライブ書き込み(UserLiteのみ)を模す
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
