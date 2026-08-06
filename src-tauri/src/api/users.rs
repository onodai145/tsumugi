//! ユーザー検索・プロフィール取得・フォロー操作 REST。

use crate::api::normalize::RawUser;
use crate::api::MisskeyClient;
use crate::domain::User;
use crate::error::Result;
use serde::Deserialize;
use serde_json::json;

pub async fn search_users(client: &MisskeyClient, query: &str, limit: u32) -> Result<Vec<User>> {
    let body = json!({
        "query": query,
        "limit": limit,
        "origin": "combined",
        "detail": false,
    });
    let raw: Vec<RawUser> = client.post("users/search", &body).await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

/// `users/show` のレスポンス。UserDetailedNotMe は `RawUser` にないフォロー関係フラグを
/// 追加で持つため、`#[serde(flatten)]` で `RawUser` の全フィールド + 関係フラグを一度に受ける。
/// 自分自身を対象にした場合（`MeDetailed`）は関係フラグが存在せず `None` になる。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUserShow {
    #[serde(flatten)]
    pub user: RawUser,
    #[serde(default)]
    pub is_following: Option<bool>,
}

/// ユーザーIDからプロフィール詳細（フォロー関係フラグ込み）を取得する。
pub async fn show(client: &MisskeyClient, user_id: &str) -> Result<RawUserShow> {
    client.post("users/show", &json!({ "userId": user_id })).await
}

/// フォローする。
pub async fn follow(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client.post("following/create", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// フォロー解除する。
pub async fn unfollow(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: serde_json::Value = client.post("following/delete", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// `users/followers` / `users/following` の1件（Followingオブジェクト）。
/// `id` はこのFollowingレコード自身のIDで、`until_id` によるページングのカーソルに使う
/// （ユーザーIDではない）。一覧の主体（followers なら相手=follower、following なら相手=followee）
/// を合わせて保持する。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFollowing {
    id: String,
    #[serde(default)]
    followee: Option<RawUser>,
    #[serde(default)]
    follower: Option<RawUser>,
}

const FOLLOW_LIST_PAGE_SIZE: u32 = 20;

/// `RawFollowing` の配列から、ユーザーがhydrateされている分だけ `(User, cursor)` を組み立てる。
/// ユーザーが取得できなかったレコード（`extract` が `None` を返す）は結果から除外する。
/// ただし末尾のレコードがhydrateされていない場合、その分のカーソルが失われると次ページ取得時に
/// 迷子になるため、返す一覧の最後の要素の cursor は「取得したレコード全体（ユーザー有無問わず）の
/// 最後のレコードのid」で上書きする。これにより最後の要素のcursorを次回の `until_id` に使えば
/// 常に安全にページングできる。
fn build_follow_list_entries(
    raw: Vec<RawFollowing>,
    extract: impl Fn(&RawFollowing) -> Option<RawUser>,
) -> Vec<(User, String)> {
    let last_raw_id = raw.last().map(|f| f.id.clone());
    let mut entries: Vec<(User, String)> =
        raw.iter().filter_map(|f| Some((extract(f)?.into(), f.id.clone()))).collect();
    if let (Some(last_entry), Some(last_id)) = (entries.last_mut(), last_raw_id) {
        last_entry.1 = last_id;
    }
    entries
}

/// フォロワー一覧（新しい順、`until_id` でページング）。戻り値は `(ユーザー, カーソル)` の配列で、
/// カーソルは次ページの `until_id` に使うFollowingレコードのID（ユーザーIDではない）。
pub async fn followers(client: &MisskeyClient, user_id: &str, until_id: Option<&str>) -> Result<Vec<(User, String)>> {
    let mut body = json!({ "userId": user_id, "limit": FOLLOW_LIST_PAGE_SIZE });
    if let Some(u) = until_id {
        body["untilId"] = json!(u);
    }
    let raw: Vec<RawFollowing> = client.post("users/followers", &body).await?;
    Ok(build_follow_list_entries(raw, |f| f.follower.clone()))
}

/// フォロー中一覧（新しい順、`until_id` でページング）。戻り値は `(ユーザー, カーソル)` の配列で、
/// カーソルは次ページの `until_id` に使うFollowingレコードのID（ユーザーIDではない）。
pub async fn following(client: &MisskeyClient, user_id: &str, until_id: Option<&str>) -> Result<Vec<(User, String)>> {
    let mut body = json!({ "userId": user_id, "limit": FOLLOW_LIST_PAGE_SIZE });
    if let Some(u) = until_id {
        body["untilId"] = json!(u);
    }
    let raw: Vec<RawFollowing> = client.post("users/following", &body).await?;
    Ok(build_follow_list_entries(raw, |f| f.followee.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_following(json_val: serde_json::Value) -> Vec<RawFollowing> {
        serde_json::from_value(json_val).unwrap()
    }

    #[test]
    fn cursor_is_own_record_id_when_all_records_hydrated() {
        let raw = raw_following(json!([
            { "id": "f1", "follower": { "id": "u1", "username": "alice" } },
            { "id": "f2", "follower": { "id": "u2", "username": "bob" } },
        ]));
        let entries = build_follow_list_entries(raw, |f| f.follower.clone());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.id, "u1");
        assert_eq!(entries[0].1, "f1");
        assert_eq!(entries[1].0.id, "u2");
        assert_eq!(entries[1].1, "f2");
    }

    #[test]
    fn unhydrated_records_are_dropped_but_last_entry_cursor_still_reaches_final_raw_id() {
        // 3件中2件目・3件目はfollowerがhydrateされていない（削除済みユーザー等）。
        // それでも最後に残るentry（u1）のcursorは、生レコード全体の最後(f3)を指す必要がある。
        // そうしないと次回のuntil_id=f1のままだと同じレコードを再取得してしまう。
        let raw = raw_following(json!([
            { "id": "f1", "follower": { "id": "u1", "username": "alice" } },
            { "id": "f2", "follower": null },
            { "id": "f3", "follower": null },
        ]));
        let entries = build_follow_list_entries(raw, |f| f.follower.clone());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0.id, "u1");
        assert_eq!(entries[0].1, "f3");
    }

    #[test]
    fn all_unhydrated_records_yield_empty_entries() {
        let raw = raw_following(json!([
            { "id": "f1", "follower": null },
            { "id": "f2", "follower": null },
        ]));
        let entries = build_follow_list_entries(raw, |f| f.follower.clone());
        assert!(entries.is_empty());
    }
}
