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
    Drumkit,
}

impl Timbre {
    /// 番号順の一覧。
    pub const ALL: [Timbre; 15] = [
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
        Timbre::Drumkit,
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
            Timbre::Drumkit => "drumkit",
        }
    }

    /// 打楽器(音の高さを持たない `drum` `noise` `drumkit`)か。`y` `m` `k` `p` はこれらに効かない。
    pub fn is_percussive(self) -> bool {
        matches!(self, Timbre::Drum | Timbre::Noise | Timbre::Drumkit)
    }
}

/// 音色のパラメーター(Issue #499)。MML の `y a,d,s,r` のエンベロープ。時間はミリ秒、`sustain` は 0〜100(%)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnvelopeParams {
    pub attack_ms: u32,
    pub decay_ms: u32,
    pub sustain: u8,
    pub release_ms: u32,
}

/// `m 速さ,深さ` のビブラート。`rate_hz` は 1〜20(Hz)、`depth` は 1〜100(音程の揺れの片側の幅、おおよそのセント。遅い速さでは頭打ち)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VibratoParams {
    pub rate_hz: u8,
    pub depth: u8,
}

/// 音に付くパラメーター。`y` `m` `k` のどれも書かれていなければ、`NoteEvent.params` は `None`。
/// `detune` は `k` のセント(−100〜100。0 は指定なし)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TimbreParams {
    pub envelope: Option<EnvelopeParams>,
    pub vibrato: Option<VibratoParams>,
    pub detune: i16,
}

/// ポルタメント(Issue #499)。この音は、`from_midi` の高さから目的の高さまで、`ms` ミリ秒(音の長さより長ければ音の長さ)で滑って入る。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Glide {
    pub from_midi: u8,
    pub ms: u16,
}

/// リバーブ(Issue #499)。`level` は送る量(1〜100 %)、`decay_ms` は残響の長さ(100〜5000 ミリ秒)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReverbParams {
    pub level: u8,
    pub decay_ms: u16,
}

/// ディレイ(Issue #499)。`level` は送る量(1〜100 %)、`time_ms` は間隔(50〜1000 ミリ秒)、`feedback` は繰り返し(0〜90 %)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DelayParams {
    pub level: u8,
    pub time_ms: u16,
    pub feedback: u8,
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
    /// `y` `m` `k` のパラメーター(どれも書かれていなければ `None`)
    pub params: Option<TimbreParams>,
    /// `p` のポルタメント(滑らない音は `None`)
    pub glide: Option<Glide>,
    /// `x` のリバーブ(効かせない音は `None`)
    pub reverb: Option<ReverbParams>,
    /// `z` のディレイ(効かせない音は `None`)
    pub delay: Option<DelayParams>,
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
