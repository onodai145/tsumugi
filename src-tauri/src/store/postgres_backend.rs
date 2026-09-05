//! note cacheのPostgresBackend(Issue #115 Phase 2)。`sqlx::PgPool`を使い、
//! `NoteCacheBackend`トレイトの非同期メソッドをネイティブに(spawn_blockingなしで)実装する。
//! DDLは`sea-query`の`Table::create()`で書く。CRUD文は既存の`SqliteBackend`と同じ、
//! 手書きSQL文字列 + `$N`プレースホルダのバインド方式で書く(設計書「Global Constraints」参照)。

use crate::error::Result;
use sea_query::{ColumnDef, Index, IndexOrder, PostgresQueryBuilder, Table};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::Executor;

pub(crate) struct PostgresConnectParams {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

pub(crate) struct PostgresBackend {
    pool: sqlx::PgPool,
}

impl PostgresBackend {
    /// Postgresへ接続する。TLSモードは`PgSslMode::Prefer`(sqlxのデフォルトと同じ)を
    /// 明示的に指定している — 既定値に暗黙に頼るのではなく、選択を監査可能にするため。
    /// `Require`/`VerifyFull`は使わない: このアプリはユーザーが自由に設定した任意の
    /// Postgresインスタンス(TLS未設定のLAN/ホームラボ環境を含む)へ接続するため、TLSを
    /// 強制すると正当な構成が接続できなくなる。
    pub(crate) async fn connect(params: &PostgresConnectParams) -> Result<Self> {
        let opts = PgConnectOptions::new()
            .host(&params.host)
            .port(params.port)
            .database(&params.database)
            .username(&params.user)
            .password(&params.password)
            .ssl_mode(PgSslMode::Prefer);
        let pool = PgPoolOptions::new().max_connections(5).connect_with(opts).await?;
        ensure_schema(&pool).await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }
}

/// キャッシュDBのテーブルをすべて作成する(`CREATE TABLE IF NOT EXISTS`相当、冪等)。
pub(crate) async fn ensure_schema(pool: &sqlx::PgPool) -> Result<()> {
    let note = Table::create()
        .table(NoteTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTable::Id).text().primary_key())
        .col(ColumnDef::new(NoteTable::CreatedAt).big_integer().not_null())
        .col(ColumnDef::new(NoteTable::Text).text())
        .col(ColumnDef::new(NoteTable::TextLength).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::Cw).text())
        .col(ColumnDef::new(NoteTable::Visibility).text().not_null())
        .col(ColumnDef::new(NoteTable::LocalOnly).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::UserId).text().not_null())
        .col(ColumnDef::new(NoteTable::ReplyId).text())
        .col(ColumnDef::new(NoteTable::ReplyUserId).text())
        .col(ColumnDef::new(NoteTable::RenoteId).text())
        .col(ColumnDef::new(NoteTable::ChannelId).text())
        .col(ColumnDef::new(NoteTable::Via).text())
        .col(ColumnDef::new(NoteTable::Lang).text())
        .col(ColumnDef::new(NoteTable::FilesCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::HasPoll).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::HasLink).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::IsPinned).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::ReactionCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::RenoteCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::ReplyCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(NoteTable::MyReaction).text())
        .col(ColumnDef::new(NoteTable::IsRenotedByMe).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::IsFavoritedByMe).boolean().not_null().default(false))
        .col(ColumnDef::new(NoteTable::Payload).text().not_null())
        .build(PostgresQueryBuilder);
    pool.execute(note.as_str()).await?;

    let idx_note_created = Index::create()
        .if_not_exists()
        .name("idx_note_created")
        .table(NoteTable::Table)
        .col(NoteTable::CreatedAt)
        .build(PostgresQueryBuilder);
    pool.execute(idx_note_created.as_str()).await?;

    let idx_note_user = Index::create()
        .if_not_exists()
        .name("idx_note_user")
        .table(NoteTable::Table)
        .col(NoteTable::UserId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_note_user.as_str()).await?;

    let user = Table::create()
        .table(UserTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(UserTable::Id).text().primary_key())
        .col(ColumnDef::new(UserTable::Username).text().not_null())
        .col(ColumnDef::new(UserTable::Host).text())
        .col(ColumnDef::new(UserTable::Name).text())
        .col(ColumnDef::new(UserTable::AvatarUrl).text())
        .col(ColumnDef::new(UserTable::IsBot).boolean().not_null().default(false))
        .col(ColumnDef::new(UserTable::IsCat).boolean().not_null().default(false))
        .col(ColumnDef::new(UserTable::FollowersCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::FollowingCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::NotesCount).big_integer().not_null().default(0))
        .col(ColumnDef::new(UserTable::Emojis).text().not_null().default("{}"))
        .col(ColumnDef::new(UserTable::Bio).text())
        .col(ColumnDef::new(UserTable::BannerUrl).text())
        .col(ColumnDef::new(UserTable::InstanceName).text())
        .col(ColumnDef::new(UserTable::InstanceIconUrl).text())
        .col(ColumnDef::new(UserTable::InstanceThemeColor).text())
        .build(PostgresQueryBuilder);
    pool.execute(user.as_str()).await?;

    let note_reaction = Table::create()
        .table(NoteReactionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteReactionTable::NoteId).text())
        .col(ColumnDef::new(NoteReactionTable::EmojiKey).text())
        .col(ColumnDef::new(NoteReactionTable::Count).big_integer())
        .build(PostgresQueryBuilder);
    pool.execute(note_reaction.as_str()).await?;

    let note_tag = Table::create()
        .table(NoteTagTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteTagTable::NoteId).text())
        .col(ColumnDef::new(NoteTagTable::Tag).text())
        .build(PostgresQueryBuilder);
    pool.execute(note_tag.as_str()).await?;

    let note_mention = Table::create()
        .table(NoteMentionTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteMentionTable::NoteId).text())
        .col(ColumnDef::new(NoteMentionTable::UserId).text())
        .build(PostgresQueryBuilder);
    pool.execute(note_mention.as_str()).await?;

    let note_emoji = Table::create()
        .table(NoteEmojiTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteEmojiTable::NoteId).text())
        .col(ColumnDef::new(NoteEmojiTable::Emoji).text())
        .build(PostgresQueryBuilder);
    pool.execute(note_emoji.as_str()).await?;

    let note_file = Table::create()
        .table(NoteFileTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(NoteFileTable::NoteId).text())
        .col(ColumnDef::new(NoteFileTable::MimeType).text())
        .col(ColumnDef::new(NoteFileTable::MimeCategory).text())
        .col(ColumnDef::new(NoteFileTable::IsSensitive).boolean())
        .build(PostgresQueryBuilder);
    pool.execute(note_file.as_str()).await?;

    let idx_nr_note = Index::create()
        .if_not_exists()
        .name("idx_nr_note")
        .table(NoteReactionTable::Table)
        .col(NoteReactionTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nr_note.as_str()).await?;

    let idx_nt_note = Index::create()
        .if_not_exists()
        .name("idx_nt_note")
        .table(NoteTagTable::Table)
        .col(NoteTagTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nt_note.as_str()).await?;

    let idx_nm_note = Index::create()
        .if_not_exists()
        .name("idx_nm_note")
        .table(NoteMentionTable::Table)
        .col(NoteMentionTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nm_note.as_str()).await?;

    let idx_ne_note = Index::create()
        .if_not_exists()
        .name("idx_ne_note")
        .table(NoteEmojiTable::Table)
        .col(NoteEmojiTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_ne_note.as_str()).await?;

    let idx_nf_note = Index::create()
        .if_not_exists()
        .name("idx_nf_note")
        .table(NoteFileTable::Table)
        .col(NoteFileTable::NoteId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nf_note.as_str()).await?;

    let idx_nr_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nr_unique")
        .table(NoteReactionTable::Table)
        .col(NoteReactionTable::NoteId)
        .col(NoteReactionTable::EmojiKey)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nr_unique.as_str()).await?;

    let idx_nt_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nt_unique")
        .table(NoteTagTable::Table)
        .col(NoteTagTable::NoteId)
        .col(NoteTagTable::Tag)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nt_unique.as_str()).await?;

    let idx_nm_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nm_unique")
        .table(NoteMentionTable::Table)
        .col(NoteMentionTable::NoteId)
        .col(NoteMentionTable::UserId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nm_unique.as_str()).await?;

    let idx_ne_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_ne_unique")
        .table(NoteEmojiTable::Table)
        .col(NoteEmojiTable::NoteId)
        .col(NoteEmojiTable::Emoji)
        .build(PostgresQueryBuilder);
    pool.execute(idx_ne_unique.as_str()).await?;

    let idx_nf_unique = Index::create()
        .if_not_exists()
        .unique()
        .name("idx_nf_unique")
        .table(NoteFileTable::Table)
        .col(NoteFileTable::NoteId)
        .col(NoteFileTable::MimeType)
        .col(NoteFileTable::MimeCategory)
        .col(NoteFileTable::IsSensitive)
        .build(PostgresQueryBuilder);
    pool.execute(idx_nf_unique.as_str()).await?;

    let column_note = Table::create()
        .table(ColumnNoteTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnNoteTable::ColumnId).text().not_null())
        .col(ColumnDef::new(ColumnNoteTable::NoteId).text().not_null())
        .col(ColumnDef::new(ColumnNoteTable::ReceivedAt).big_integer().not_null())
        .col(ColumnDef::new(ColumnNoteTable::CreatedAt).big_integer().not_null().default(0))
        .primary_key(
            Index::create()
                .col(ColumnNoteTable::ColumnId)
                .col(ColumnNoteTable::NoteId),
        )
        .build(PostgresQueryBuilder);
    pool.execute(column_note.as_str()).await?;

    let idx_cn_column = Index::create()
        .if_not_exists()
        .name("idx_cn_column")
        .table(ColumnNoteTable::Table)
        .col(ColumnNoteTable::ColumnId)
        .build(PostgresQueryBuilder);
    pool.execute(idx_cn_column.as_str()).await?;

    let idx_cn_column_created = Index::create()
        .if_not_exists()
        .name("idx_cn_column_created")
        .table(ColumnNoteTable::Table)
        .col(ColumnNoteTable::ColumnId)
        .col((ColumnNoteTable::CreatedAt, IndexOrder::Desc))
        .col((ColumnNoteTable::NoteId, IndexOrder::Desc))
        .build(PostgresQueryBuilder);
    pool.execute(idx_cn_column_created.as_str()).await?;

    let column_fetch_boundary = Table::create()
        .table(ColumnFetchBoundaryTable::Table)
        .if_not_exists()
        .col(ColumnDef::new(ColumnFetchBoundaryTable::ColumnId).text().primary_key())
        .col(ColumnDef::new(ColumnFetchBoundaryTable::OldestFetchedId).text().not_null())
        .build(PostgresQueryBuilder);
    pool.execute(column_fetch_boundary.as_str()).await?;

    Ok(())
}

#[derive(sea_query::Iden)]
enum NoteTable {
    #[iden = "note"]
    Table,
    Id,
    CreatedAt,
    Text,
    TextLength,
    Cw,
    Visibility,
    LocalOnly,
    UserId,
    ReplyId,
    ReplyUserId,
    RenoteId,
    ChannelId,
    Via,
    Lang,
    FilesCount,
    HasPoll,
    HasLink,
    IsPinned,
    ReactionCount,
    RenoteCount,
    ReplyCount,
    MyReaction,
    IsRenotedByMe,
    IsFavoritedByMe,
    Payload,
}

#[derive(sea_query::Iden)]
enum UserTable {
    #[iden = "user"]
    Table,
    Id,
    Username,
    Host,
    Name,
    AvatarUrl,
    IsBot,
    IsCat,
    FollowersCount,
    FollowingCount,
    NotesCount,
    Emojis,
    Bio,
    BannerUrl,
    InstanceName,
    InstanceIconUrl,
    InstanceThemeColor,
}

#[derive(sea_query::Iden)]
enum NoteReactionTable {
    #[iden = "note_reaction"]
    Table,
    NoteId,
    EmojiKey,
    Count,
}

#[derive(sea_query::Iden)]
enum NoteTagTable {
    #[iden = "note_tag"]
    Table,
    NoteId,
    Tag,
}

#[derive(sea_query::Iden)]
enum NoteMentionTable {
    #[iden = "note_mention"]
    Table,
    NoteId,
    UserId,
}

#[derive(sea_query::Iden)]
enum NoteEmojiTable {
    #[iden = "note_emoji"]
    Table,
    NoteId,
    Emoji,
}

#[derive(sea_query::Iden)]
enum NoteFileTable {
    #[iden = "note_file"]
    Table,
    NoteId,
    MimeType,
    MimeCategory,
    IsSensitive,
}

#[derive(sea_query::Iden)]
enum ColumnNoteTable {
    #[iden = "column_note"]
    Table,
    ColumnId,
    NoteId,
    ReceivedAt,
    CreatedAt,
}

#[derive(sea_query::Iden)]
enum ColumnFetchBoundaryTable {
    #[iden = "column_fetch_boundary"]
    Table,
    ColumnId,
    OldestFetchedId,
}

use crate::domain::Note;
use crate::store::note_cache::NoteCacheBackend;

fn visibility_str(v: crate::domain::Visibility) -> &'static str {
    use crate::domain::Visibility::*;
    match v {
        Public => "public",
        Home => "home",
        Followers => "followers",
        Specified => "specified",
    }
}

fn mime_category(mime: &str) -> &str {
    mime.split('/').next().unwrap_or("other")
}

fn has_url(text: &str) -> bool {
    text.contains("http://") || text.contains("https://")
}

/// noteが参照するuserをすべてupsertする。プールの別コネクションを都度取得して
/// 即座に返却するため、呼び出し元がトランザクションを保持している間に呼んでは
/// ならない — コネクションプールのサイズが小さい場合、トランザクション用の
/// コネクションを保持したまま追加のコネクションを要求するとデッドロック
/// (もしくはacquireタイムアウト)を招く。必ずトランザクション開始前に呼ぶこと。
async fn upsert_note_users(pool: &sqlx::PgPool, n: &Note) -> Result<()> {
    // note行より先にuserをupsertする(SQLite版と同じ理由: user行が無いnote行が
    // 永久に読めなくなる事態を避ける)。
    for user in crate::store::user_ref::collect_users(n) {
        crate::store::postgres_user_ref::upsert_user(pool, user).await?;
    }
    Ok(())
}

/// `note`行 + 側テーブル(reaction/tag/mention/emoji/file)をUPSERTする。
/// `store/note_cache.rs::upsert_note`(SQLite版)と等価。呼び出し元が開始した
/// トランザクション`tx`の中で実行する(コミットは呼び出し元の責務)。
/// user upsertはこの関数に含まれない — 呼び出し元が`upsert_note_users`を
/// トランザクション開始前に済ませておくこと(`upsert_note_users`のdocコメント参照)。
async fn upsert_note_tx(tx: &mut sqlx::PgTransaction<'_>, n: &Note) -> Result<()> {
    let mut payload_value = serde_json::to_value(n)?;
    crate::store::user_ref::stub_user_refs(&mut payload_value);
    let payload = serde_json::to_string(&payload_value)?;
    let text_length = n.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) as i64;
    let has_link = n.text.as_deref().map(has_url).unwrap_or(false);

    sqlx::query(
        "INSERT INTO note (
            id, created_at, text, text_length, cw, visibility, local_only, user_id,
            reply_id, reply_user_id, renote_id, channel_id, via, lang,
            files_count, has_poll, has_link, is_pinned,
            reaction_count, renote_count, reply_count, my_reaction,
            is_renoted_by_me, is_favorited_by_me, payload
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)
        ON CONFLICT (id) DO UPDATE SET
            created_at = excluded.created_at, text = excluded.text, text_length = excluded.text_length,
            cw = excluded.cw, visibility = excluded.visibility, local_only = excluded.local_only,
            user_id = excluded.user_id, reply_id = excluded.reply_id, reply_user_id = excluded.reply_user_id,
            renote_id = excluded.renote_id, channel_id = excluded.channel_id, via = excluded.via,
            lang = excluded.lang, files_count = excluded.files_count, has_poll = excluded.has_poll,
            has_link = excluded.has_link, is_pinned = excluded.is_pinned,
            reaction_count = excluded.reaction_count, renote_count = excluded.renote_count,
            reply_count = excluded.reply_count, my_reaction = excluded.my_reaction,
            is_renoted_by_me = excluded.is_renoted_by_me, is_favorited_by_me = excluded.is_favorited_by_me,
            payload = excluded.payload",
    )
    .bind(&n.id)
    .bind(n.created_at)
    .bind(&n.text)
    .bind(text_length)
    .bind(&n.cw)
    .bind(visibility_str(n.visibility))
    .bind(n.local_only)
    .bind(&n.user.id)
    .bind(&n.reply_id)
    .bind(Option::<String>::None)
    .bind(&n.renote_id)
    .bind(&n.channel_id)
    .bind(&n.via)
    .bind(&n.lang)
    .bind(n.files.len() as i64)
    .bind(n.poll.is_some())
    .bind(has_link)
    .bind(n.is_pinned)
    .bind(n.reaction_count as i64)
    .bind(n.renote_count as i64)
    .bind(n.reply_count as i64)
    .bind(&n.my_reaction)
    .bind(n.is_renoted_by_me)
    .bind(n.is_favorited_by_me)
    .bind(&payload)
    .execute(&mut **tx)
    .await?;

    for (emoji, count) in &n.reactions {
        sqlx::query(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES ($1,$2,$3)
             ON CONFLICT (note_id, emoji_key) DO UPDATE SET count = excluded.count",
        )
        .bind(&n.id)
        .bind(emoji)
        .bind(*count as i64)
        .execute(&mut **tx)
        .await?;
    }
    for tag in &n.tags {
        sqlx::query(
            "INSERT INTO note_tag (note_id, tag) VALUES ($1,$2) ON CONFLICT (note_id, tag) DO NOTHING",
        )
        .bind(&n.id)
        .bind(tag)
        .execute(&mut **tx)
        .await?;
    }
    for uid in &n.mentions {
        sqlx::query(
            "INSERT INTO note_mention (note_id, user_id) VALUES ($1,$2) ON CONFLICT (note_id, user_id) DO NOTHING",
        )
        .bind(&n.id)
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    }
    for e in n.emojis.keys() {
        sqlx::query(
            "INSERT INTO note_emoji (note_id, emoji) VALUES ($1,$2) ON CONFLICT (note_id, emoji) DO NOTHING",
        )
        .bind(&n.id)
        .bind(e)
        .execute(&mut **tx)
        .await?;
    }
    for f in &n.files {
        sqlx::query(
            "INSERT INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES ($1,$2,$3,$4)
             ON CONFLICT (note_id, mime_type, mime_category, is_sensitive) DO NOTHING",
        )
        .bind(&n.id)
        .bind(&f.mime_type)
        .bind(mime_category(&f.mime_type))
        .bind(f.is_sensitive)
        .execute(&mut **tx)
        .await?;
    }

    // 旧行の掃除(SQLiteのjson_eachの代わりにPostgresは`= ANY($N)`配列バインドを使う)。
    let reaction_keys: Vec<&String> = n.reactions.keys().collect();
    sqlx::query("DELETE FROM note_reaction WHERE note_id = $1 AND NOT (emoji_key = ANY($2))")
        .bind(&n.id)
        .bind(&reaction_keys)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM note_tag WHERE note_id = $1 AND NOT (tag = ANY($2))")
        .bind(&n.id)
        .bind(&n.tags)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM note_mention WHERE note_id = $1 AND NOT (user_id = ANY($2))")
        .bind(&n.id)
        .bind(&n.mentions)
        .execute(&mut **tx)
        .await?;
    let emoji_keys: Vec<&String> = n.emojis.keys().collect();
    sqlx::query("DELETE FROM note_emoji WHERE note_id = $1 AND NOT (emoji = ANY($2))")
        .bind(&n.id)
        .bind(&emoji_keys)
        .execute(&mut **tx)
        .await?;
    // note_fileは複合キーで`= ANY`が使えないため、SQLite版(rowidベース)と同様に
    // 既存行を取得してRust側で差分判定し、現存しないキーの組だけ個別にDELETEする。
    let current_file_keys: std::collections::HashSet<(String, String, bool)> = n
        .files
        .iter()
        .map(|f| (f.mime_type.clone(), mime_category(&f.mime_type).to_string(), f.is_sensitive))
        .collect();
    let existing_files: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT mime_type, mime_category, is_sensitive FROM note_file WHERE note_id = $1",
    )
    .bind(&n.id)
    .fetch_all(&mut **tx)
    .await?;
    for (mime_type, mime_category_val, is_sensitive) in existing_files {
        let key = (mime_type.clone(), mime_category_val.clone(), is_sensitive);
        if !current_file_keys.contains(&key) {
            sqlx::query(
                "DELETE FROM note_file WHERE note_id = $1 AND mime_type = $2 AND mime_category = $3 AND is_sensitive = $4",
            )
            .bind(&n.id)
            .bind(&mime_type)
            .bind(&mime_category_val)
            .bind(is_sensitive)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}

/// 単発呼び出し用の薄いラッパー(既存の呼び出し箇所、例: `update_note`向け)。
/// user upsert(トランザクション外)→1トランザクションを開始し`upsert_note_tx`を
/// 呼んでコミットする、の順で行う。
async fn upsert_note(pool: &sqlx::PgPool, n: &Note) -> Result<()> {
    upsert_note_users(pool, n).await?;
    let mut tx = pool.begin().await?;
    upsert_note_tx(&mut tx, n).await?;
    tx.commit().await?;
    Ok(())
}

async fn self_heal_node(pool: &sqlx::PgPool, node: &mut serde_json::Value) -> Result<bool> {
    let mut changed = false;
    if let Some(user_value) = node.get("user").cloned() {
        if crate::store::user_ref::is_legacy_full_user(&user_value) {
            if let Ok(user) = serde_json::from_value::<crate::domain::User>(user_value.clone()) {
                crate::store::postgres_user_ref::fill_user_from_snapshot(pool, &user).await?;
                if let Some(id) = user_value.get("id").cloned() {
                    node["user"] = serde_json::json!({ "id": id });
                    changed = true;
                }
            }
        }
    }
    if node.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        changed |= Box::pin(self_heal_node(pool, &mut node["renote"])).await?;
    }
    Ok(changed)
}

async fn self_heal_legacy_row(pool: &sqlx::PgPool, note_id: &str, value: &mut serde_json::Value) -> Result<()> {
    let changed = self_heal_node(pool, value).await?;
    if changed {
        let new_payload = serde_json::to_string(value)?;
        sqlx::query("UPDATE note SET payload = $1 WHERE id = $2").bind(new_payload).bind(note_id).execute(pool).await?;
    }
    Ok(())
}

async fn resolve_payload_rows(pool: &sqlx::PgPool, rows: Vec<(String, String)>) -> Result<Vec<Note>> {
    let mut values: Vec<(String, serde_json::Value)> = Vec::with_capacity(rows.len());
    for (id, payload) in rows {
        match serde_json::from_str::<serde_json::Value>(&payload) {
            Ok(mut v) => {
                if crate::store::user_ref::has_legacy_full_user(&v) {
                    self_heal_legacy_row(pool, &id, &mut v).await?;
                }
                values.push((id, v));
            }
            Err(e) => log::warn!("skipping note cache row {id} with unparsable payload: {e}"),
        }
    }

    let mut ids = Vec::new();
    for (_, v) in &values {
        crate::store::user_ref::collect_user_id_refs(v, &mut ids);
    }
    ids.sort();
    ids.dedup();
    let users = crate::store::postgres_user_ref::fetch_users_by_ids(pool, &ids).await?;

    let mut out = Vec::with_capacity(values.len());
    for (id, mut v) in values {
        if !crate::store::user_ref::hydrate_user_refs(&mut v, &users) {
            log::warn!("skipping note cache row {id}: referenced user not found in user table");
            continue;
        }
        match serde_json::from_value::<Note>(v) {
            Ok(note) => out.push(note),
            Err(e) => log::warn!("skipping note cache row {id} with undeserializable payload: {e}"),
        }
    }
    Ok(out)
}

#[async_trait::async_trait]
impl NoteCacheBackend for PostgresBackend {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let now = crate::store::note_cache::now_epoch();
        // userのupsertはすべてトランザクション開始前に行う(`upsert_note_users`のdoc
        // コメント参照 — トランザクション保持中にプールから別コネクションを取得すると
        // プールが小さい場合にデッドロック/acquireタイムアウトを招くため)。
        for n in notes {
            upsert_note_users(&self.pool, n).await?;
        }
        let mut tx = self.pool.begin().await?;
        for n in notes {
            upsert_note_tx(&mut tx, n).await?;
            sqlx::query(
                "INSERT INTO column_note (column_id, note_id, received_at, created_at) VALUES ($1,$2,$3,$4)
                 ON CONFLICT DO NOTHING",
            )
            .bind(column_id)
            .bind(&n.id)
            .bind(now)
            .bind(n.created_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.cache_notes(column_id, std::slice::from_ref(note)).await
    }

    async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = $1
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT $2",
        )
        .bind(column_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        resolve_payload_rows(&self.pool, rows).await
    }

    async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = $1 AND cn.note_id < $2
             ORDER BY cn.note_id DESC
             LIMIT $3",
        )
        .bind(column_id)
        .bind(until_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        let mut out = resolve_payload_rows(&self.pool, rows).await?;
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(out)
    }

    async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT id, payload FROM note WHERE id = $1").bind(note_id).fetch_optional(&self.pool).await?;
        Ok(match row {
            Some(r) => resolve_payload_rows(&self.pool, vec![r]).await?.into_iter().next(),
            None => None,
        })
    }

    async fn update_note(&self, note: &Note) -> Result<()> {
        upsert_note(&self.pool, note).await
    }

    async fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM column_note WHERE column_id = $1").bind(column_id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = $1").bind(column_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        let v: Option<(String,)> = sqlx::query_as(
            "SELECT oldest_fetched_id FROM column_fetch_boundary WHERE column_id = $1",
        )
        .bind(column_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(v.map(|(s,)| s))
    }

    async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES ($1,$2)
             ON CONFLICT (column_id) DO UPDATE SET oldest_fetched_id = excluded.oldest_fetched_id",
        )
        .bind(column_id)
        .bind(new_oldest_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES ($1,$2)
             ON CONFLICT (column_id) DO UPDATE SET
                oldest_fetched_id = LEAST(column_fetch_boundary.oldest_fetched_id, excluded.oldest_fetched_id)",
        )
        .bind(column_id)
        .bind(new_oldest_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_all_fetch_boundaries(&self) -> Result<()> {
        sqlx::query("DELETE FROM column_fetch_boundary").execute(&self.pool).await?;
        Ok(())
    }

    async fn note_count(&self) -> Result<i32> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note").fetch_one(&self.pool).await?;
        Ok(count as i32)
    }

    async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM note WHERE created_at >= $1").bind(since_epoch_secs as i64).fetch_one(&self.pool).await?;
        Ok(count as i32)
    }

    async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        prune_impl(&self.pool, keep, max_age_days, max_size_mb).await
    }

    async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        search_cache_impl(&self.pool, where_sql, until_id, limit).await
    }
}

/// SQLite版`delete_matching`と等価だが、TEMP TABLEを使わずRust側で対象IDを
/// `Vec<String>`として確定してから`= ANY($1)`でDELETEする(プールの別コネクションに
/// 割り当てられてもTEMP TABLEが「無い」扱いになる問題を回避するため。Global Constraints参照)。
/// `tx`は呼び出し元が開始したトランザクション。
async fn delete_matching_ids(tx: &mut sqlx::PgTransaction<'_>, ids: &[String]) -> Result<i64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let deleted = sqlx::query("DELETE FROM note WHERE id = ANY($1)").bind(ids).execute(&mut **tx).await?.rows_affected() as i64;

    let affected_columns: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT column_id FROM column_note WHERE note_id = ANY($1)").bind(ids).fetch_all(&mut **tx).await?;
    let max_deleted_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT column_id, MAX(note_id) FROM column_note WHERE note_id = ANY($1) GROUP BY column_id",
    )
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;
    let max_deleted_by_column: std::collections::HashMap<String, String> = max_deleted_rows.into_iter().collect();

    for table in ["column_note", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE note_id = ANY($1)")).bind(ids).execute(&mut **tx).await?;
    }

    for (column_id,) in &affected_columns {
        let survivor: Option<(String,)> =
            sqlx::query_as("SELECT MIN(note_id) FROM column_note WHERE column_id = $1").bind(column_id).fetch_optional(&mut **tx).await?;
        match survivor.map(|(s,)| s) {
            Some(oldest) => {
                let candidate = match max_deleted_by_column.get(column_id) {
                    Some(max_deleted) if max_deleted.as_str() > oldest.as_str() => max_deleted.clone(),
                    _ => oldest,
                };
                sqlx::query(
                    "UPDATE column_fetch_boundary SET oldest_fetched_id = $2 WHERE column_id = $1 AND oldest_fetched_id < $2",
                )
                .bind(column_id)
                .bind(&candidate)
                .execute(&mut **tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM column_fetch_boundary WHERE column_id = $1").bind(column_id).execute(&mut **tx).await?;
            }
        }
    }
    Ok(deleted)
}

async fn prune_impl(pool: &sqlx::PgPool, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
    let mut deleted: i64 = 0;
    let mut tx = pool.begin().await?;

    if max_age_days > 0 {
        let cutoff = crate::store::note_cache::now_epoch() - max_age_days as i64 * 86_400;
        let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM note WHERE created_at < $1").bind(cutoff).fetch_all(&mut *tx).await?;
        let ids: Vec<String> = ids.into_iter().map(|(id,)| id).collect();
        deleted += delete_matching_ids(&mut tx, &ids).await?;
    }
    if keep > 0 {
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note").fetch_one(&mut *tx).await?;
        let overflow = total - keep as i64;
        if overflow > 0 {
            let ids: Vec<(String,)> =
                sqlx::query_as("SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT $1").bind(overflow).fetch_all(&mut *tx).await?;
            let ids: Vec<String> = ids.into_iter().map(|(id,)| id).collect();
            deleted += delete_matching_ids(&mut tx, &ids).await?;
        }
    }
    tx.commit().await?;

    if max_size_mb > 0 {
        log::warn!(
            "max_size_mb is configured but has no effect on the Postgres cache backend \
             (byte-budget pruning is not supported here); use keep/max_age_days instead"
        );
    }
    Ok(deleted as usize)
}

/// `SqlWhere.sql`(`?`プレースホルダ、SQLite方言)をPostgres用に変換する。
/// `?`を出現順に`$1`,`$2`,...へ振り直し、` REGEXP `を` ~ `へ置換する。
/// 設計書「filter/sql.rs(TQLの扱い)」参照(build_where自体は変更しない)。
fn to_postgres_sql(where_sql: &crate::filter::sql::SqlWhere) -> String {
    let mut out = String::with_capacity(where_sql.sql.len());
    let mut n = 0usize;
    for ch in where_sql.sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out.replace(" REGEXP ", " ~ ")
}

async fn search_cache_impl(
    pool: &sqlx::PgPool,
    where_sql: &crate::filter::sql::SqlWhere,
    until_id: Option<&str>,
    limit: u32,
) -> Result<Vec<Note>> {
    use crate::filter::sql::SqlParam;

    let converted_where = to_postgres_sql(where_sql);
    let mut sql = format!("SELECT n.id, n.payload FROM note n JOIN \"user\" u ON u.id = n.user_id WHERE ({converted_where})");
    let mut next_placeholder = where_sql.params.len() + 1;
    if until_id.is_some() {
        sql.push_str(&format!(" AND n.id < ${next_placeholder}"));
        next_placeholder += 1;
    }
    sql.push_str(&format!(" ORDER BY n.created_at DESC, n.id DESC LIMIT ${next_placeholder}"));

    let mut query = sqlx::query_as::<_, (String, String)>(&sql);
    for p in &where_sql.params {
        query = match p {
            SqlParam::Text(s) => query.bind(s.clone()),
            SqlParam::Real(x) => query.bind(*x),
        };
    }
    if let Some(u) = until_id {
        query = query.bind(u.to_string());
    }
    query = query.bind(limit as i64);

    let rows = query.fetch_all(pool).await?;
    resolve_payload_rows(pool, rows).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

    /// Docker上の使い捨てPostgresへ接続し、スキーマが2回適用してもエラーにならず
    /// (冪等)、テーブルが実際に作成されることを確認する。CI常時実行はしない方針
    /// (`#[ignore]`、既存の実Misskey接続テストと同様)。
    #[tokio::test]
    #[ignore]
    async fn ensure_schema_is_idempotent_and_creates_tables() {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(&format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"))
            .await
            .unwrap();

        ensure_schema(&pool).await.unwrap();
        ensure_schema(&pool).await.unwrap(); // 2回目も成功する(冪等)

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'note'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    /// Postgres接続の組み立て(host/port/database/user/password)が実際に使えることの確認。
    #[tokio::test]
    #[ignore]
    async fn connect_succeeds_against_running_postgres() {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let params = PostgresConnectParams {
            host: "127.0.0.1".into(),
            port,
            database: "postgres".into(),
            user: "postgres".into(),
            password: "postgres".into(),
        };
        let backend = PostgresBackend::connect(&params).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note").fetch_one(backend.pool()).await.unwrap();
        assert_eq!(count, 0);
    }

    fn note(id: &str, created_at: i64) -> Note {
        use crate::domain::{DriveFile, User, Visibility};
        Note {
            id: id.into(),
            created_at,
            text: Some("hello https://example.com #rust".into()),
            cw: None,
            visibility: Visibility::Home,
            local_only: false,
            user: User {
                id: "u1".into(), username: "alice".into(), host: None, name: Some("Alice".into()),
                avatar_url: None, is_bot: false, is_cat: false,
                followers_count: 5, following_count: 3, notes_count: 42,
                emojis: std::collections::HashMap::new(), bio: None, banner_url: None, instance: None,
            },
            reply_id: None, renote_id: None, renote: None,
            files: vec![DriveFile { id: "f1".into(), mime_type: "image/png".into(), is_sensitive: false, url: "http://x/f1".into(), thumbnail_url: None, name: "f1.png".into() }],
            poll: None, tags: vec!["rust".into()], mentions: vec![],
            emojis: std::collections::HashMap::new(), channel_id: None, via: None, lang: None,
            reactions: std::collections::HashMap::from([("👍".into(), 3u32)]),
            reaction_count: 3, renote_count: 1, reply_count: 0,
            my_reaction: Some("👍".into()), is_renoted_by_me: false, is_favorited_by_me: false, is_pinned: false,
        }
    }

    async fn backend() -> PostgresBackend {
        let container = Postgres::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let params = PostgresConnectParams { host: "127.0.0.1".into(), port, database: "postgres".into(), user: "postgres".into(), password: "postgres".into() };
        let backend = PostgresBackend::connect(&params).await.unwrap();
        std::mem::forget(container);
        backend
    }

    #[tokio::test]
    #[ignore]
    async fn cache_roundtrip_preserves_note_and_order() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 150)]).await.unwrap();
        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n3", "n1"]);
        assert_eq!(got[0].reactions.get("👍"), Some(&3));
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_note_removes_stale_reaction_after_unreact() {
        let s = backend().await;
        let mut n = note("n1", 100);
        n.reactions = std::collections::HashMap::from([("👍".into(), 3u32)]);
        s.cache_note("col1", &n).await.unwrap();

        n.reactions = std::collections::HashMap::new();
        n.reaction_count = 0;
        n.my_reaction = None;
        s.update_note(&n).await.unwrap();

        let (rc,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'").fetch_one(s.pool()).await.unwrap();
        assert_eq!(rc, 0);
    }

    #[tokio::test]
    #[ignore]
    async fn prune_removes_oldest_beyond_keep_and_related_rows() {
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        let deleted = s.prune(2, 0, 0).await.unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(s.note_count().await.unwrap(), 2);
        let (rc,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'").fetch_one(s.pool()).await.unwrap();
        assert_eq!(rc, 0);
    }

    #[tokio::test]
    #[ignore]
    async fn search_cache_applies_predicate_and_until_id_boundary() {
        use crate::filter::{parser, sql};
        let s = backend().await;
        s.cache_notes("col1", &[note("a1", 300), note("a2", 200), note("a3", 100)]).await.unwrap();
        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };
        let expr = parser::parse_predicate("has_files").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, Some("a3"), 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["a1", "a2"]);
    }

    #[tokio::test]
    #[ignore]
    async fn search_cache_translates_regexp_to_postgres_tilde_operator() {
        use crate::filter::{parser, sql};
        let s = backend().await;
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();
        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };
        let expr = parser::parse_predicate("text match \"rust\"").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, None, 10).await.unwrap();
        assert_eq!(got.len(), 1);
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_boundary_roundtrip_and_extend_only_moves_older() {
        let s = backend().await;
        assert!(s.get_fetch_boundary("col1").await.unwrap().is_none());
        s.set_fetch_boundary("col1", "n500").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n500"));
        s.extend_fetch_boundary("col1", "n300").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n300"));
        s.extend_fetch_boundary("col1", "n800").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n300"));
    }
}
