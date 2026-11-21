//! 読み上げ(Issue #625)。エンジン(`engine` / `external`)・キュー(`queue`)・ワーカー(`manager` / `runtime`)。
//! 再生は `crate::sound::SoundPlayer` の読み上げレーンが担う。Tauri への依存は `runtime` と
//! `commands::speech` に閉じる(`translate/` と同じ流儀)。

pub mod engine;
pub mod external;
pub mod manager;
pub mod queue;
pub mod runtime;
pub mod voicevox;

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
/// 内蔵エンジンが使えない状態: エンジン一式、または選んだ声の VVM が未取得。
pub const ERR_NOT_READY: &str = "SPEECH_CORE_NOT_READY";
/// 内蔵エンジンのロード・起動に失敗した(次の合成で再試行する)。
pub const ERR_ENGINE_LOAD: &str = "SPEECH_CORE_LOAD_FAILED";
/// この環境では内蔵エンジンを使えない(Android・対応外の CPU など)。
pub const ERR_UNSUPPORTED: &str = "SPEECH_CORE_UNSUPPORTED";

/// 識別コードつきの `Error::Invalid`(翻訳の `LOCAL_*` と同じ流儀)。
pub fn invalid(code: impl Into<String>) -> Error {
    Error::Invalid(code.into())
}
