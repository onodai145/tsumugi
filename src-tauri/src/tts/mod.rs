//! 読み上げ(Issue #625)。エンジン(`engine` / `external`)・キュー(`queue`)・ワーカー(`manager` / `runtime`)。
//! 再生は `crate::sound::SoundPlayer` の読み上げレーンが担う。Tauri への依存は `runtime` と
//! `commands::speech` に閉じる(`translate/` と同じ流儀)。

pub mod engine;
pub mod external;
pub mod manager;
pub mod queue;
pub mod runtime;

use crate::error::Error;

/// エンジンの URL が未設定。
pub const ERR_NOT_CONFIGURED: &str = "SPEECH_ENGINE_NOT_CONFIGURED";
/// エンジンに接続できない・タイムアウトした。
pub const ERR_UNREACHABLE: &str = "SPEECH_ENGINE_UNREACHABLE";
/// エンジンの応答が想定外(HTTP エラー・WAV でない・JSON の形が違う)。
pub const ERR_BAD_RESPONSE: &str = "SPEECH_ENGINE_BAD_RESPONSE";
/// 声が選ばれていない(カラムの声も既定の声も空)。
pub const ERR_NO_VOICE: &str = "SPEECH_NO_VOICE";
/// 読み上げる文が長すぎる。
pub const ERR_TEXT_TOO_LONG: &str = "SPEECH_TEXT_TOO_LONG";

/// 識別コードつきの `Error::Invalid`(翻訳の `LOCAL_*` と同じ流儀)。
pub fn invalid(code: impl Into<String>) -> Error {
    Error::Invalid(code.into())
}
