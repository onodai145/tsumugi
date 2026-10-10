use super::user::User;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

/// docs/design/filter-dsl-design.md §7 / 設計書§5.1。フィルタ評価の対象そのもの。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// aid/aidx。数値比較しない
    pub id: String,
    /// epoch秒。時間比較はこれ。TS へは number で出す（2^53 に収まり精度損失なし）
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    /// MFM原文。純Renoteは None
    pub text: Option<String>,
    pub cw: Option<String>,
    pub visibility: Visibility,
    pub local_only: bool,
    pub user: User,
    pub reply_id: Option<String>,
    /// 返信先ノートの投稿者 userId（`reply_to_me` 述語用）。返信でない場合は None
    pub reply_user_id: Option<String>,
    /// 返信先ノート。renote と同様に浅く1階層のみ保持する(`reply.reply` は常に None)。
    /// 表示専用でフィルタ評価には使わない。返信でない/返信先が取得できない場合は None
    pub reply: Option<Box<Note>>,
    pub renote_id: Option<String>,
    /// 引用/Renote先（浅く保持）
    pub renote: Option<Box<Note>>,
    pub files: Vec<DriveFile>,
    pub poll: Option<Poll>,
    pub tags: Vec<String>,
    /// メンション先 userId
    pub mentions: Vec<String>,
    /// カスタム絵文字 name -> url（本文 MFM の `:name:` とリアクション絵文字の描画に使う）
    pub emojis: HashMap<String, String>,
    pub channel_id: Option<String>,
    pub via: Option<String>,
    pub lang: Option<String>,

    // 可変集計部（noteUpdated で更新。値は更新するが出入りはしない）
    /// キー=Misskey形式（Unicode生 or :name@host:）
    pub reactions: HashMap<String, u32>,
    pub reaction_count: u32,
    pub renote_count: u32,
    pub reply_count: u32,
    pub my_reaction: Option<String>,
    pub is_renoted_by_me: bool,
    pub is_favorited_by_me: bool,
    pub is_pinned: bool,
}

impl Note {
    /// 自分のリアクション付与をローカル反映する（Misskeyは1ユーザ1リアクション）。
    /// 既に別の絵文字でリアクション済みなら、先にそちらを取り消してから付け直す。
    /// frontend の addReaction (store.svelte.ts) と同じ挙動。
    pub fn apply_my_reaction(&mut self, reaction: &str) {
        if self.my_reaction.is_some() {
            self.clear_my_reaction();
        }
        *self.reactions.entry(reaction.to_string()).or_insert(0) += 1;
        self.my_reaction = Some(reaction.to_string());
        self.reaction_count += 1;
    }

    /// 自分のリアクション取り消しをローカル反映する。
    /// frontend の removeReaction (store.svelte.ts) と同じ挙動。
    pub fn clear_my_reaction(&mut self) {
        let Some(cur) = self.my_reaction.take() else {
            return;
        };
        if let Some(c) = self.reactions.get_mut(&cur) {
            if *c <= 1 {
                self.reactions.remove(&cur);
            } else {
                *c -= 1;
            }
        }
        self.reaction_count = self.reaction_count.saturating_sub(1);
    }

    /// 自分のアカウントの reacted イベントを反映する（他クライアント由来を含む。Issue #28）。
    /// tsumugi 自身の操作は apply_my_reaction で反映済みなので、my_reaction が同じなら何もしない
    /// （二重カウント防止）。frontend の applyOwnReactionEvent と同じ挙動。
    pub fn record_own_reaction_event(&mut self, reaction: &str) {
        if self.my_reaction.as_deref() != Some(reaction) {
            self.apply_my_reaction(reaction);
        }
    }

    /// 自分のアカウントの unreacted イベントを反映する（他クライアント由来を含む。Issue #28）。
    /// my_reaction が一致する時だけ取り消す（自身の取り消し・付け替え後に届く古いイベントは無視）。
    pub fn record_own_unreaction_event(&mut self, reaction: &str) {
        if self.my_reaction.as_deref() == Some(reaction) {
            self.clear_my_reaction();
        }
    }

    /// 他ユーザーのリアクション付与をローカル反映する（my_reactionには触れない）。
    /// frontend の #applyNoteUpdate の "reacted"(isMineでない場合) と同じ挙動。
    pub fn record_others_reaction(&mut self, reaction: &str) {
        *self.reactions.entry(reaction.to_string()).or_insert(0) += 1;
        self.reaction_count += 1;
    }

    /// 他ユーザーのリアクション取り消しをローカル反映する。
    /// frontend の #applyNoteUpdate の "unreacted"(isMineでない場合) と同じ挙動。
    pub fn record_others_unreaction(&mut self, reaction: &str) {
        if let Some(c) = self.reactions.get_mut(reaction) {
            if *c <= 1 {
                self.reactions.remove(reaction);
            } else {
                *c -= 1;
            }
        }
        self.reaction_count = self.reaction_count.saturating_sub(1);
    }
}

/// Specified = direct
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    Public,
    Home,
    Followers,
    Specified,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    /// "image/png" 等。file_types はここから category 化
    pub mime_type: String,
    pub is_sensitive: bool,
    pub url: String,
    pub thumbnail_url: Option<String>,
    /// 元のファイル名（メディア以外はダウンロードリンクの表示名に使う）
    #[serde(default)]
    pub name: String,
    /// バイト数。古いキャッシュや Misskey が返さない場合は None
    #[serde(default)]
    #[specta(type = Option<specta_typescript::Number>)]
    pub size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Poll {
    pub choices: Vec<PollChoice>,
    pub multiple: bool,
    #[specta(type = Option<specta_typescript::Number>)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PollChoice {
    pub text: String,
    pub votes: u32,
    pub is_voted: bool,
}

/// `notes/translate` の翻訳結果（Issue #440）。`source_lang` は検出された翻訳元言語コード。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub source_lang: String,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_note() -> Note {
        Note {
            id: "n1".into(),
            created_at: 0,
            text: None,
            cw: None,
            visibility: Visibility::Public,
            local_only: false,
            user: User {
                id: "u1".into(),
                username: "alice".into(),
                host: None,
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: HashMap::new(),
                bio: None,
                banner_url: None,
                avatar_blurhash: None,
                instance: None,
            },
            reply_id: None,
            reply_user_id: None,
            renote_id: None,
            reply: None,
            renote: None,
            files: vec![],
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: HashMap::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: HashMap::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    #[test]
    fn reply_user_id_round_trips_as_camel_case() {
        let mut n = minimal_note();
        n.reply_id = Some("r1".into());
        n.reply_user_id = Some("target-user".into());

        let v = serde_json::to_value(&n).unwrap();
        assert_eq!(v["replyUserId"], "target-user");

        let back: Note = serde_json::from_value(v).unwrap();
        assert_eq!(back.reply_user_id.as_deref(), Some("target-user"));
    }

    #[test]
    fn reply_round_trips_and_missing_key_deserializes_as_none() {
        let mut n = minimal_note();
        let mut target = minimal_note();
        target.id = "r1".into();
        n.reply_id = Some("r1".into());
        n.reply = Some(Box::new(target));

        let v = serde_json::to_value(&n).unwrap();
        assert_eq!(v["reply"]["id"], "r1");
        let back: Note = serde_json::from_value(v).unwrap();
        assert_eq!(back.reply.as_ref().map(|r| r.id.as_str()), Some("r1"));

        // 旧キャッシュ行(reply キーを持たない payload)は None として読める
        let mut legacy = serde_json::to_value(&minimal_note()).unwrap();
        legacy.as_object_mut().unwrap().remove("reply");
        let back: Note = serde_json::from_value(legacy).unwrap();
        assert!(back.reply.is_none());
    }

    #[test]
    fn apply_my_reaction_from_none_increments_count_and_sets_key() {
        let mut n = minimal_note();
        n.apply_my_reaction("👍");
        assert_eq!(n.reactions.get("👍"), Some(&1));
        assert_eq!(n.reaction_count, 1);
        assert_eq!(n.my_reaction.as_deref(), Some("👍"));
    }

    #[test]
    fn apply_my_reaction_switches_from_existing_reaction() {
        let mut n = minimal_note();
        n.reactions.insert("👍".into(), 1);
        n.reaction_count = 1;
        n.my_reaction = Some("👍".into());

        n.apply_my_reaction("😀");

        // 旧キーは取り消され、新キーが1件だけ付く
        assert_eq!(n.reactions.get("👍"), None);
        assert_eq!(n.reactions.get("😀"), Some(&1));
        assert_eq!(n.reaction_count, 1);
        assert_eq!(n.my_reaction.as_deref(), Some("😀"));
    }

    #[test]
    fn clear_my_reaction_decrements_shared_key_without_removing_it() {
        let mut n = minimal_note();
        // 自分以外にも同じ絵文字でリアクションしている人がいる状態
        n.reactions.insert("👍".into(), 2);
        n.reaction_count = 2;
        n.my_reaction = Some("👍".into());

        n.clear_my_reaction();

        assert_eq!(n.reactions.get("👍"), Some(&1));
        assert_eq!(n.reaction_count, 1);
        assert_eq!(n.my_reaction, None);
    }

    #[test]
    fn clear_my_reaction_removes_key_when_last() {
        let mut n = minimal_note();
        n.reactions.insert("👍".into(), 1);
        n.reaction_count = 1;
        n.my_reaction = Some("👍".into());

        n.clear_my_reaction();

        assert_eq!(n.reactions.get("👍"), None);
        assert_eq!(n.reaction_count, 0);
        assert_eq!(n.my_reaction, None);
    }

    #[test]
    fn clear_my_reaction_is_noop_when_not_reacted() {
        let mut n = minimal_note();
        n.clear_my_reaction();
        assert_eq!(n.reaction_count, 0);
        assert_eq!(n.my_reaction, None);
    }

    #[test]
    fn record_others_reaction_increments_without_touching_my_reaction() {
        let mut n = minimal_note();
        n.my_reaction = Some("😀".into());
        n.reactions.insert("😀".into(), 1);
        n.reaction_count = 1;

        n.record_others_reaction("👍");

        assert_eq!(n.reactions.get("👍"), Some(&1));
        assert_eq!(n.reactions.get("😀"), Some(&1));
        assert_eq!(n.reaction_count, 2);
        assert_eq!(n.my_reaction.as_deref(), Some("😀")); // 自分のリアクションは不変
    }

    #[test]
    fn record_others_unreaction_decrements_and_removes_when_last() {
        let mut n = minimal_note();
        n.reactions.insert("👍".into(), 1);
        n.reaction_count = 1;

        n.record_others_unreaction("👍");

        assert_eq!(n.reactions.get("👍"), None);
        assert_eq!(n.reaction_count, 0);
    }
    #[test]
    fn record_own_reaction_event_sets_my_reaction_and_counts_when_from_other_client() {
        let mut n = minimal_note();
        n.record_own_reaction_event("👍");
        assert_eq!(n.my_reaction.as_deref(), Some("👍"));
        assert_eq!(n.reactions.get("👍"), Some(&1));
        assert_eq!(n.reaction_count, 1);
    }

    #[test]
    fn record_own_reaction_event_is_idempotent_when_already_applied_locally() {
        let mut n = minimal_note();
        n.apply_my_reaction("👍");
        n.record_own_reaction_event("👍");
        assert_eq!(n.my_reaction.as_deref(), Some("👍"));
        assert_eq!(n.reactions.get("👍"), Some(&1));
        assert_eq!(n.reaction_count, 1);
    }

    #[test]
    fn record_own_reaction_event_switches_from_existing_reaction() {
        let mut n = minimal_note();
        n.apply_my_reaction("👍");
        n.record_own_reaction_event("😀");
        assert_eq!(n.my_reaction.as_deref(), Some("😀"));
        assert_eq!(n.reactions.get("👍"), None);
        assert_eq!(n.reactions.get("😀"), Some(&1));
        assert_eq!(n.reaction_count, 1);
    }

    #[test]
    fn record_own_unreaction_event_clears_my_reaction_when_from_other_client() {
        let mut n = minimal_note();
        n.apply_my_reaction("👍");
        n.record_own_unreaction_event("👍");
        assert_eq!(n.my_reaction, None);
        assert_eq!(n.reactions.get("👍"), None);
        assert_eq!(n.reaction_count, 0);
    }

    #[test]
    fn record_own_unreaction_event_is_noop_when_not_reacted_or_reaction_differs() {
        let mut n = minimal_note();
        n.reactions.insert("👍".into(), 1);
        n.reaction_count = 1;
        n.record_own_unreaction_event("👍");
        assert_eq!(n.reactions.get("👍"), Some(&1));
        assert_eq!(n.reaction_count, 1);

        n.my_reaction = Some("😀".into());
        n.reactions.insert("😀".into(), 1);
        n.reaction_count = 2;
        n.record_own_unreaction_event("👍");
        assert_eq!(n.my_reaction.as_deref(), Some("😀"));
        assert_eq!(n.reaction_count, 2);
    }
}
