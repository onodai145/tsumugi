use serde::{Deserialize, Serialize};
use specta::Type;

/// 設計書§5。**token 本体は含めない**（keyring に保管し Core 内のみで扱う）。
/// フロントへ token を渡す command は設けない。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// アプリ内部で発行する UUID（keyring のキーにも使う）
    pub id: String,
    /// 例 "misskey.io"
    pub host: String,
    /// ログインユーザ名（@なし）
    pub username: String,
    /// インスタンス上の userId（mine / to_me 判定に使う）
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}
