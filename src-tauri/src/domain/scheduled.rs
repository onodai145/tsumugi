//! サーバー側の予約投稿(Issue #60)。本家 Misskey の `NoteDraft` のうち、予約中のもの。
//! 作成欄に戻せるよう、フィールドは `store::draft::Draft` と揃えている。

use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
use crate::store::draft::{DraftNoteSnapshot, PollDraftSnapshot};
use serde::{Deserialize, Serialize};
use specta::Type;

// PartialEq は付けない(PollDraftSnapshot / DraftNoteSnapshot が持たないため。テストはフィールド単位で比較する)。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledNote {
    /// サーバー側の下書き ID(取り消しに使う)。
    pub id: String,
    /// 予約日時(epoch 秒)。現在より過去なら、投稿に失敗して残っているもの。
    #[specta(type = specta_typescript::Number)]
    pub scheduled_at: i64,
    pub text: String,
    pub cw: Option<String>,
    pub visibility: VisibilityInput,
    pub local_only: bool,
    pub reaction_acceptance: ReactionAcceptanceInput,
    pub channel_id: Option<String>,
    pub poll: Option<PollDraftSnapshot>,
    pub file_ids: Vec<String>,
    pub reply_note: Option<DraftNoteSnapshot>,
    pub quote_note: Option<DraftNoteSnapshot>,
}

/// クライアント側(ローカル)予約の状態(Issue #60 B)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LocalScheduleStatus {
    /// 予約時刻(または再試行の予定時刻)を待っている。
    Pending,
    /// 送信中。結果が分かるまで、取り消しも再送もできない。
    Posting,
    /// アプリが起動していない間に予約時刻を過ぎた(猶予超過)。自動では投稿しない。
    Expired,
    /// 投稿に失敗した。理由は `error`。
    Failed,
}

/// ローカル予約 1 件。中身は A の `ScheduledNote` を再利用する(`id` はローカル予約の ID)。
/// これにより、A の「作成欄に戻す」の処理をそのまま使える。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalScheduledNote {
    pub note: ScheduledNote,
    pub status: LocalScheduleStatus,
    pub error: Option<String>,
}
