//! 下書き(未送信の投稿の保存)の永続化。settings.rs と同様、プレーンテキスト(JSON)で
//! 1ファイルに保存する(app_config_dir/drafts.json)。ノートキャッシュ(SQLite, 破棄前提)
//! とは異なり下書きはユーザが書いた再取得不能なデータのため、キャッシュDBには置かない。
//! 自動下書き(2秒デバウンスで書き込まれうる)を手動下書き/設定本体と分けるため、
//! settings.json とは別ファイルに分離する。

use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, atomic::{AtomicI64, Ordering}};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DraftKind {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PollDraftSnapshot {
    pub choices: Vec<String>,
    pub multiple: bool,
    #[specta(type = Option<specta_typescript::Number>)]
    pub expires_at: Option<i64>,
}

/// 返信/引用先ノートの表示用最小スナップショット(ComposeBarのバナー表示に必要な分のみ)。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftNoteSnapshot {
    pub id: String,
    pub username: String,
    pub text: Option<String>,
}

/// 下書き保存/更新の入力(id/account_id/kind/created_at/updated_atはStore側が管理)。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftInput {
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: String,
    pub account_id: String,
    pub kind: DraftKind,
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
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DraftData {
    #[serde(default)]
    drafts: Vec<Draft>,
}

enum Backing {
    File(PathBuf),
    #[cfg(test)]
    Memory,
}

pub struct DraftStore {
    backing: Backing,
    data: Mutex<DraftData>,
}

impl DraftStore {
    pub fn new(path: PathBuf) -> Result<Self> {
        let data = load_json_or_default(&path)?;
        Ok(Self { backing: Backing::File(path), data: Mutex::new(data) })
    }

    #[cfg(test)]
    pub(crate) fn new_in_memory() -> Self {
        Self { backing: Backing::Memory, data: Mutex::new(DraftData::default()) }
    }

    #[allow(irrefutable_let_patterns)]
    fn save(&self, data: &DraftData) -> Result<()> {
        if let Backing::File(path) = &self.backing {
            let json = serde_json::to_string_pretty(data)?;
            let tmp_path = path.with_extension("json.tmp");
            std::fs::write(&tmp_path, json)?;
            std::fs::rename(&tmp_path, path)?;
        }
        Ok(())
    }

    pub fn save_manual(&self, account_id: &str, input: &DraftInput) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        let now = now_millis();
        guard.drafts.push(Draft {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: account_id.to_string(),
            kind: DraftKind::Manual,
            text: input.text.clone(),
            cw: input.cw.clone(),
            visibility: input.visibility,
            local_only: input.local_only,
            reaction_acceptance: input.reaction_acceptance,
            channel_id: input.channel_id.clone(),
            poll: input.poll.clone(),
            file_ids: input.file_ids.clone(),
            reply_note: input.reply_note.clone(),
            quote_note: input.quote_note.clone(),
            created_at: now,
            updated_at: now,
        });
        self.save(&guard)
    }

    pub fn list_manual(&self, account_id: &str) -> Result<Vec<Draft>> {
        let guard = self.data.lock().unwrap();
        let mut list: Vec<Draft> = guard
            .drafts
            .iter()
            .filter(|d| d.account_id == account_id && matches!(d.kind, DraftKind::Manual))
            .cloned()
            .collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(list)
    }

    pub fn delete_manual(&self, account_id: &str, draft_id: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.drafts.retain(|d| {
            !(d.account_id == account_id && d.id == draft_id && matches!(d.kind, DraftKind::Manual))
        });
        self.save(&guard)
    }

    pub fn get_auto(&self, account_id: &str) -> Result<Option<Draft>> {
        let guard = self.data.lock().unwrap();
        Ok(guard
            .drafts
            .iter()
            .find(|d| d.account_id == account_id && matches!(d.kind, DraftKind::Auto))
            .cloned())
    }

    pub fn save_auto(&self, account_id: &str, input: &DraftInput) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard
            .drafts
            .retain(|d| !(d.account_id == account_id && matches!(d.kind, DraftKind::Auto)));
        let now = now_millis();
        guard.drafts.push(Draft {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: account_id.to_string(),
            kind: DraftKind::Auto,
            text: input.text.clone(),
            cw: input.cw.clone(),
            visibility: input.visibility,
            local_only: input.local_only,
            reaction_acceptance: input.reaction_acceptance,
            channel_id: input.channel_id.clone(),
            poll: input.poll.clone(),
            file_ids: input.file_ids.clone(),
            reply_note: input.reply_note.clone(),
            quote_note: input.quote_note.clone(),
            created_at: now,
            updated_at: now,
        });
        self.save(&guard)
    }

    pub fn clear_auto(&self, account_id: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard
            .drafts
            .retain(|d| !(d.account_id == account_id && matches!(d.kind, DraftKind::Auto)));
        self.save(&guard)
    }
}

fn load_json_or_default(path: &Path) -> Result<DraftData> {
    if !path.exists() {
        return Ok(DraftData::default());
    }
    let s = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&s)?)
}

static LAST_TIME: AtomicI64 = AtomicI64::new(0);

fn now_millis() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let mut last = LAST_TIME.load(Ordering::SeqCst);
    loop {
        let new_time = if now > last { now } else { last + 1 };
        match LAST_TIME.compare_exchange(last, new_time, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return new_time,
            Err(new_last) => last = new_last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(text: &str) -> DraftInput {
        DraftInput {
            text: text.into(),
            cw: None,
            visibility: VisibilityInput::Public,
            local_only: false,
            reaction_acceptance: ReactionAcceptanceInput::All,
            channel_id: None,
            poll: None,
            file_ids: vec![],
            reply_note: None,
            quote_note: None,
        }
    }

    #[test]
    fn save_manual_appends_and_list_manual_returns_newest_first() {
        let s = DraftStore::new_in_memory();
        s.save_manual("acc1", &input("first")).unwrap();
        s.save_manual("acc1", &input("second")).unwrap();
        let list = s.list_manual("acc1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].text, "second"); // 新しい順
        assert_eq!(list[1].text, "first");
        assert!(list.iter().all(|d| matches!(d.kind, DraftKind::Manual)));
    }

    #[test]
    fn delete_manual_removes_only_target_draft() {
        let s = DraftStore::new_in_memory();
        s.save_manual("acc1", &input("keep")).unwrap();
        s.save_manual("acc1", &input("drop")).unwrap();
        let id_to_drop = s.list_manual("acc1").unwrap()[0].id.clone(); // "drop"(新しい方)
        s.delete_manual("acc1", &id_to_drop).unwrap();
        let list = s.list_manual("acc1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].text, "keep");
    }

    #[test]
    fn save_auto_upserts_single_draft_per_account() {
        let s = DraftStore::new_in_memory();
        s.save_auto("acc1", &input("v1")).unwrap();
        s.save_auto("acc1", &input("v2")).unwrap();
        let got = s.get_auto("acc1").unwrap().expect("auto draft should exist");
        assert_eq!(got.text, "v2");
        assert!(matches!(got.kind, DraftKind::Auto));
        // 手動下書きの一覧には出ない
        assert!(s.list_manual("acc1").unwrap().is_empty());
    }

    #[test]
    fn get_auto_returns_none_when_unset() {
        let s = DraftStore::new_in_memory();
        assert!(s.get_auto("acc1").unwrap().is_none());
    }

    #[test]
    fn clear_auto_removes_the_auto_draft() {
        let s = DraftStore::new_in_memory();
        s.save_auto("acc1", &input("v1")).unwrap();
        s.clear_auto("acc1").unwrap();
        assert!(s.get_auto("acc1").unwrap().is_none());
    }

    #[test]
    fn auto_drafts_are_isolated_per_account() {
        let s = DraftStore::new_in_memory();
        s.save_auto("acc1", &input("a1")).unwrap();
        s.save_auto("acc2", &input("a2")).unwrap();
        assert_eq!(s.get_auto("acc1").unwrap().unwrap().text, "a1");
        assert_eq!(s.get_auto("acc2").unwrap().unwrap().text, "a2");
    }

    #[test]
    fn persists_to_plain_text_json_file_and_reloads() {
        let path = std::env::temp_dir().join(format!("tsumugi-drafts-test-{}.json", uuid::Uuid::new_v4()));
        {
            let s = DraftStore::new(path.clone()).unwrap();
            s.save_manual("acc1", &input("hello")).unwrap();
        }
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"hello\""));
        assert!(serde_json::from_str::<serde_json::Value>(&raw).is_ok());

        let reloaded = DraftStore::new(path.clone()).unwrap();
        assert_eq!(reloaded.list_manual("acc1").unwrap().len(), 1);

        std::fs::remove_file(&path).ok();
    }
}
