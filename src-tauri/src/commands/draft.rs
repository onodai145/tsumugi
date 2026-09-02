//! 下書き(未送信の投稿の保存/復元)系 command。

use crate::state::AppState;
use crate::store::draft::{Draft, DraftInput};
use crate::error::Result;
use tauri::State;

/// アカウントの手動保存下書き一覧(更新日時降順)。
#[tauri::command]
#[specta::specta]
pub async fn list_drafts(state: State<'_, AppState>, account_id: String) -> Result<Vec<Draft>> {
    list_drafts_core(state.inner(), &account_id)
}

fn list_drafts_core(state: &AppState, account_id: &str) -> Result<Vec<Draft>> {
    state.drafts.list_manual(account_id)
}

/// 手動下書きを新規保存する(既存の上書きはしない)。
#[tauri::command]
#[specta::specta]
pub async fn save_draft(state: State<'_, AppState>, account_id: String, input: DraftInput) -> Result<()> {
    save_draft_core(state.inner(), &account_id, &input)
}

fn save_draft_core(state: &AppState, account_id: &str, input: &DraftInput) -> Result<()> {
    state.drafts.save_manual(account_id, input)
}

/// 手動下書きを削除する。
#[tauri::command]
#[specta::specta]
pub async fn delete_draft(state: State<'_, AppState>, account_id: String, draft_id: String) -> Result<()> {
    delete_draft_core(state.inner(), &account_id, &draft_id)
}

fn delete_draft_core(state: &AppState, account_id: &str, draft_id: &str) -> Result<()> {
    state.drafts.delete_manual(account_id, draft_id)
}

/// アカウントの自動一時下書き(あれば1件)。
#[tauri::command]
#[specta::specta]
pub async fn get_auto_draft(state: State<'_, AppState>, account_id: String) -> Result<Option<Draft>> {
    get_auto_draft_core(state.inner(), &account_id)
}

fn get_auto_draft_core(state: &AppState, account_id: &str) -> Result<Option<Draft>> {
    state.drafts.get_auto(account_id)
}

/// 自動一時下書きをupsertする(アカウントにつき1件)。
#[tauri::command]
#[specta::specta]
pub async fn save_auto_draft(state: State<'_, AppState>, account_id: String, input: DraftInput) -> Result<()> {
    save_auto_draft_core(state.inner(), &account_id, &input)
}

fn save_auto_draft_core(state: &AppState, account_id: &str, input: &DraftInput) -> Result<()> {
    state.drafts.save_auto(account_id, input)
}

/// 自動一時下書きを消す。
#[tauri::command]
#[specta::specta]
pub async fn clear_auto_draft(state: State<'_, AppState>, account_id: String) -> Result<()> {
    clear_auto_draft_core(state.inner(), &account_id)
}

fn clear_auto_draft_core(state: &AppState, account_id: &str) -> Result<()> {
    state.drafts.clear_auto(account_id)
}

// このコードベースには `#[tauri::command]` fn を `State<'_, AppState>` 込みで直接呼ぶ
// ユニットテストの前例がない(`tauri::State` はプライベートフィールドの tuple struct で
// 公開コンストラクタが無く、`tauri::test::mock_builder()` 経由で組み立てるには
// app.manage() + WebviewWindow + generate_context! 相当のフィクスチャが要る)。代わりに、
// commands/column.rs が確立している前例(`search_cache_core` 等、command 本体を `&AppState`
// 等を取る素の関数へ切り出してテストする形)に倣い、各 command を `state.inner()` を渡すだけの
// 一行委譲にし、実体(`*_core`)を直接テストする。これにより command が正しい
// account_id/draft_id を正しい `*_core` へ渡していること自体もコンパイラ経由で保証される。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::notes::{ReactionAcceptanceInput, VisibilityInput};
    use crate::store::SettingsStore;

    fn test_state() -> AppState {
        AppState::new_for_test(SettingsStore::new_in_memory())
    }

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
    fn manual_draft_save_list_delete_round_trip() {
        let state = test_state();
        save_draft_core(&state, "acc1", &input("下書き本文")).unwrap();
        let list = list_drafts_core(&state, "acc1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].text, "下書き本文");

        delete_draft_core(&state, "acc1", &list[0].id).unwrap();
        assert!(list_drafts_core(&state, "acc1").unwrap().is_empty());
    }

    #[test]
    fn manual_drafts_are_not_visible_to_other_accounts() {
        let state = test_state();
        save_draft_core(&state, "acc1", &input("acc1の下書き")).unwrap();
        save_draft_core(&state, "acc2", &input("acc2の下書き")).unwrap();

        let acc1_list = list_drafts_core(&state, "acc1").unwrap();
        assert_eq!(acc1_list.len(), 1);
        assert_eq!(acc1_list[0].text, "acc1の下書き");

        let acc2_list = list_drafts_core(&state, "acc2").unwrap();
        assert_eq!(acc2_list.len(), 1);
        assert_eq!(acc2_list[0].text, "acc2の下書き");
    }

    #[test]
    fn delete_draft_core_is_scoped_to_the_given_account() {
        let state = test_state();
        save_draft_core(&state, "acc1", &input("acc1の下書き")).unwrap();
        let id = list_drafts_core(&state, "acc1").unwrap()[0].id.clone();

        // 他アカウントのaccount_idでは削除されない
        delete_draft_core(&state, "acc2", &id).unwrap();
        assert_eq!(list_drafts_core(&state, "acc1").unwrap().len(), 1);

        delete_draft_core(&state, "acc1", &id).unwrap();
        assert!(list_drafts_core(&state, "acc1").unwrap().is_empty());
    }

    #[test]
    fn auto_draft_save_get_clear_round_trip() {
        let state = test_state();
        assert!(get_auto_draft_core(&state, "acc1").unwrap().is_none());

        save_auto_draft_core(&state, "acc1", &input("自動保存")).unwrap();
        let got = get_auto_draft_core(&state, "acc1").unwrap().expect("saved");
        assert_eq!(got.text, "自動保存");

        clear_auto_draft_core(&state, "acc1").unwrap();
        assert!(get_auto_draft_core(&state, "acc1").unwrap().is_none());
    }

    #[test]
    fn auto_drafts_are_not_visible_to_other_accounts() {
        let state = test_state();
        save_auto_draft_core(&state, "acc1", &input("acc1の自動下書き")).unwrap();
        assert!(get_auto_draft_core(&state, "acc2").unwrap().is_none());
        assert_eq!(get_auto_draft_core(&state, "acc1").unwrap().unwrap().text, "acc1の自動下書き");
    }
}
