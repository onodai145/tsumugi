use serde::{Deserialize, Serialize};
use specta::Type;

/// 他アプリの共有シート(Android の ACTION_SEND/ACTION_SEND_MULTIPLE)から受け取った内容
/// (Issue #116)。`text` はテキスト共有時のみ、`file_paths` は画像/動画共有時のみ埋まる
/// (アプリの一時キャッシュディレクトリへコピー済みの絶対パス)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShareReceived {
    pub text: Option<String>,
    pub file_paths: Vec<String>,
}
