use serde::{Deserialize, Serialize};
use specta::Type;

/// MML から展開した 1 音(Issue #75)。時刻は秒で、テンポ変化・繰り返し・タイは解決済み。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoteEvent {
    /// トラック番号(0 始まり)
    pub track: u8,
    pub start_sec: f64,
    pub duration_sec: f64,
    /// MIDI ノート番号(o4 の c が 60)
    pub midi: u8,
    /// 0.0〜1.0
    pub velocity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MmlScore {
    pub notes: Vec<NoteEvent>,
    /// 全トラックのうち最も長い再生時間(秒)
    pub total_sec: f64,
    pub track_count: u8,
}

/// MML の構文エラー。`position` は入力テキストの文字位置(0 始まり、空白を含む)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MmlError {
    pub position: u32,
    pub message: String,
}

/// `parse_mml` コマンドの結果。UI が位置を使うので、構文エラーも `Err` ではなくデータで返す。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum MmlParseResult {
    Parsed { score: MmlScore },
    Invalid { error: MmlError },
}
