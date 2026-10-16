use serde::{Deserialize, Serialize};
use specta::Type;

/// 音色(Issue #494)。MML の `@<番号>` / `@<名前>` で指定する。番号は `ALL` の並び順(0 始まり)。
/// 番号と名前は、フロントの `lib/mmlTimbres.ts` と同じにすること(両側のテストが同じ表を持つ)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Timbre {
    Fm,
    Square,
    Pulse,
    Triangle,
    Saw,
    Sine,
    Organ,
    Epiano,
    Bell,
    Am,
    Lead,
    Bass,
    Drum,
    Noise,
}

impl Timbre {
    /// 番号順の一覧。
    pub const ALL: [Timbre; 14] = [
        Timbre::Fm,
        Timbre::Square,
        Timbre::Pulse,
        Timbre::Triangle,
        Timbre::Saw,
        Timbre::Sine,
        Timbre::Organ,
        Timbre::Epiano,
        Timbre::Bell,
        Timbre::Am,
        Timbre::Lead,
        Timbre::Bass,
        Timbre::Drum,
        Timbre::Noise,
    ];

    pub fn from_number(n: u32) -> Option<Timbre> {
        Self::ALL.get(n as usize).copied()
    }

    /// MML の `@<名前>` で使う名前(小文字)。serde の出力と同じ。
    pub fn name(self) -> &'static str {
        match self {
            Timbre::Fm => "fm",
            Timbre::Square => "square",
            Timbre::Pulse => "pulse",
            Timbre::Triangle => "triangle",
            Timbre::Saw => "saw",
            Timbre::Sine => "sine",
            Timbre::Organ => "organ",
            Timbre::Epiano => "epiano",
            Timbre::Bell => "bell",
            Timbre::Am => "am",
            Timbre::Lead => "lead",
            Timbre::Bass => "bass",
            Timbre::Drum => "drum",
            Timbre::Noise => "noise",
        }
    }
}

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
    /// 音色(`@` で指定。指定が無ければ `fm`)
    pub timbre: Timbre,
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
