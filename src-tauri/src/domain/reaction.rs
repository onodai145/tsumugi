use serde::{Deserialize, Serialize};
use specta::Type;

/// リアクションピッカー用の絵文字定義（インスタンス単位でキャッシュ）。
/// 注意: `Note.reactions` は「絵文字キー→集計数」であり、誰がリアクションしたかは
/// Misskey は返さない（docs/filter-dsl-design.md §3.4）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmojiDef {
    /// "blobcat"（: なし）
    pub name: String,
    /// None=ローカル
    pub host: Option<String>,
    pub url: String,
    pub category: Option<String>,
    pub aliases: Vec<String>,
}

/// NoteCard 表示用の集計エントリ（HashMap を並び順付きに展開したもの）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReactionSummary {
    /// Misskey形式キー（Unicode生 or :name@host:）
    pub key: String,
    pub count: u32,
    /// my_reaction == key
    pub reacted_by_me: bool,
    /// カスタム絵文字なら解決した URL
    pub emoji_url: Option<String>,
}
