//! サーバ側ミュート/ブロックの取得（mute/list・blocking/list）。
//! Krile の MuteBlockManager 相当。返るのは対象ユーザの userId 集合。

use crate::api::MisskeyClient;
use crate::error::Result;
use serde_json::json;
use std::collections::HashSet;

const PAGE: u32 = 100;
const MAX_PAGES: usize = 20; // 安全弁（最大 2000 件）

/// サーバ側でミュート＋ブロックしているユーザの userId 集合を取得する。
/// どちらも「表示を抑制する」用途なので和集合で返す。
pub async fn fetch_muted_and_blocked(client: &MisskeyClient) -> Result<HashSet<String>> {
    let mut ids = HashSet::new();
    collect(client, "mute/list", "muteeId", &mut ids).await?;
    collect(client, "blocking/list", "blockeeId", &mut ids).await?;
    Ok(ids)
}

/// ページングしながら各レコードの `id_field`（対象 userId）を集める。
/// レコード自身の `id` を untilId に使って過去方向へ辿る。
async fn collect(
    client: &MisskeyClient,
    endpoint: &str,
    id_field: &str,
    out: &mut HashSet<String>,
) -> Result<()> {
    let mut until: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut body = json!({ "limit": PAGE });
        if let Some(u) = &until {
            body["untilId"] = json!(u);
        }
        let page: Vec<serde_json::Value> = client.post(endpoint, &body).await?;
        if page.is_empty() {
            break;
        }
        for rec in &page {
            if let Some(uid) = rec.get(id_field).and_then(|v| v.as_str()) {
                out.insert(uid.to_string());
            }
        }
        // 次ページの until はレコードの id
        until = page
            .last()
            .and_then(|r| r.get("id").and_then(|v| v.as_str()))
            .map(str::to_string);
        if until.is_none() || page.len() < PAGE as usize {
            break;
        }
    }
    Ok(())
}
