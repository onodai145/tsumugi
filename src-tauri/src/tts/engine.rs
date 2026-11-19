//! 音声合成エンジンの境界(Issue #625)。実体は `external.rs`(外部の VOICEVOX 互換エンジン)で、
//! 段階2で内蔵 CORE を同じ trait で足す。テストでは偽の実装に差し替える。

use crate::error::Result;
use async_trait::async_trait;
use serde::Serialize;
use specta::Type;

/// 選べる声。`id` はエンジン固有の不透明な文字列(VOICEVOX ではスタイルの数値 ID)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    /// 表示名(例 `ずんだもん（ノーマル）`)。
    pub label: String,
}

/// 合成のパラメーター。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthParams {
    pub speed_scale: f32,
    pub volume: f32,
}

#[async_trait]
pub trait SpeechEngine: Send + Sync + 'static {
    /// 選べる声の一覧(設定画面用)。
    async fn voices(&self) -> Result<Vec<Voice>>;
    /// `text` を `voice_id` の声で合成し、WAV のバイト列を返す。
    async fn synthesize(&self, text: &str, voice_id: &str, p: &SynthParams) -> Result<Vec<u8>>;
}
