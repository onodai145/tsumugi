//! ローカル翻訳の推論エンジンの境界（Issue #603）。実体は `llama.rs`（`llama-cpp-2`）で、テストでは偽の実装に
//! 差し替える（CI にはモデルが無い）。

use crate::error::Result;

/// 1つの文を翻訳する。ブロッキングで時間がかかるので、呼び出し側は `spawn_blocking` で呼ぶ。
pub trait LocalTranslator: Send + Sync + 'static {
    /// `plamo_lang` は `lang::to_plamo_lang` が返す PLaMo の言語コード。
    /// 入力が長すぎるときは `LOCAL_TEXT_TOO_LONG` の `Error::Invalid` を返す。
    fn translate(&self, text: &str, plamo_lang: &str) -> Result<String>;
}
