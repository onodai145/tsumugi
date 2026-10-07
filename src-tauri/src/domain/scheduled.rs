//! サーバー側の予約投稿(Issue #60)。本家 Misskey の `NoteDraft` のうち、予約中のもの。
//! 作成欄に戻せるよう、フィールドは `store::draft::Draft` と揃えている。

use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
use crate::store::draft::{DraftNoteSnapshot, PollDraftSnapshot};
use serde::Serialize;
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
