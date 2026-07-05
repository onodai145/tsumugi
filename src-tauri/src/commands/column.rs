//! カラム（Phase 2: home タイムラインのみ）の command。
//! 初期ページを REST で取得しつつ Streaming を開き、以降は `ColumnNote` イベントで追記する。

use crate::api::notes::home_timeline;
use crate::domain::Note;
use crate::error::Result;
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

const INITIAL_LIMIT: u32 = 20;

/// `open_home_column` の戻り値。フロントは column_id を購読キーにする。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedColumn {
    pub column_id: String,
    /// 初期表示用の直近ノート（新しい順）
    pub notes: Vec<Note>,
}

/// home カラムを開く: 初期ページを取得し、homeTimeline の Streaming を開始する。
#[tauri::command]
#[specta::specta]
pub async fn open_home_column(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<OpenedColumn> {
    let column_id = uuid::Uuid::new_v4().to_string();

    // 初期ページ（REST）
    let client = state.client_for(&account_id)?;
    let notes = home_timeline(&client, INITIAL_LIMIT, None).await?;

    // Streaming 開始（token は host_token から取り、フロントには渡さない）
    let (host, token) = state.host_token(&account_id)?;
    state
        .connections
        .open_home(app, column_id.clone(), host, token);

    Ok(OpenedColumn { column_id, notes })
}

/// 過去ページ（上スクロール）を取得する。`until_id` より古いノートを返す。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    account_id: String,
    until_id: String,
) -> Result<Vec<Note>> {
    let client = state.client_for(&account_id)?;
    home_timeline(&client, INITIAL_LIMIT, Some(until_id)).await
}

/// カラムを閉じる（Streaming 購読解除）。
#[tauri::command]
#[specta::specta]
pub async fn close_column(state: State<'_, AppState>, column_id: String) -> Result<()> {
    state.connections.close(&column_id);
    Ok(())
}
