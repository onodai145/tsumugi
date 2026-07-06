//! カラム（Phase 2: home タイムラインのみ）の command。
//! カラム定義は SQLite に永続化し、再起動時に list_columns → resume_column で復元する。

use crate::api::notes::home_timeline;
use crate::domain::{Column, ColumnKind, FilterQuery, Note};
use crate::error::{Error, Result};
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

const INITIAL_LIMIT: u32 = 20;
const DEFAULT_WIDTH: i32 = 360;

/// カラムを開いた結果。フロントは column_id を購読キーにする。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedColumn {
    pub column: Column,
    /// 初期表示用の直近ノート（新しい順）
    pub notes: Vec<Note>,
}

/// home カラムを新規作成する: 定義を永続化し、初期ページ取得＋Streaming 開始。
#[tauri::command]
#[specta::specta]
pub async fn open_home_column(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<OpenedColumn> {
    // account 存在確認を兼ねて host/token を先に引く
    let (host, token) = state.host_token(&account_id)?;

    let order = state.settings.load_columns()?.len() as i32;
    let column = Column {
        id: uuid::Uuid::new_v4().to_string(),
        account_id: account_id.clone(),
        kind: ColumnKind::Home,
        order,
        width: DEFAULT_WIDTH,
        filter: FilterQuery::Keywords(vec![]),
        notify_sound: false,
        notify_desktop: false,
    };
    state.settings.upsert_column(&column)?;

    let notes = fetch_initial(&state, &account_id).await?;
    state.settings.cache_notes(&column.id, &notes)?;
    state
        .connections
        .open_home(app, column.id.clone(), host, token);

    Ok(OpenedColumn { column, notes })
}

/// 永続化済みカラムを再開する（起動時の復元）。Streaming を張り直し初期ページを返す。
#[tauri::command]
#[specta::specta]
pub async fn resume_column(
    app: AppHandle,
    state: State<'_, AppState>,
    column_id: String,
) -> Result<OpenedColumn> {
    let column = state
        .settings
        .load_columns()?
        .into_iter()
        .find(|c| c.id == column_id)
        .ok_or_else(|| Error::Invalid(format!("unknown column: {column_id}")))?;

    let (host, token) = state.host_token(&column.account_id)?;

    // 起動時はまずキャッシュから即時復元。空（初回）のみ REST 取得してキャッシュ。
    let cached = state.settings.load_cached(&column.id, INITIAL_LIMIT)?;
    let notes = if cached.is_empty() {
        let rest = fetch_initial(&state, &column.account_id).await?;
        state.settings.cache_notes(&column.id, &rest)?;
        rest
    } else {
        cached
    };
    state
        .connections
        .open_home(app, column.id.clone(), host, token);

    Ok(OpenedColumn { column, notes })
}

/// 永続化済みカラム定義の一覧（起動時に取得 → resume_column で復元）。
#[tauri::command]
#[specta::specta]
pub async fn list_columns(state: State<'_, AppState>) -> Result<Vec<Column>> {
    state.settings.load_columns()
}

/// 過去ページ（上スクロール）を取得する。`until_id` より古いノートを返し、キャッシュにも保存する。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    account_id: String,
    column_id: String,
    until_id: String,
) -> Result<Vec<Note>> {
    let client = state.client_for(&account_id)?;
    let notes = home_timeline(&client, INITIAL_LIMIT, Some(until_id)).await?;
    state.settings.cache_notes(&column_id, &notes)?;
    Ok(notes)
}

/// カラムを閉じる（Streaming 購読解除＋永続層から削除＋キャッシュの所属も掃除）。
#[tauri::command]
#[specta::specta]
pub async fn close_column(state: State<'_, AppState>, column_id: String) -> Result<()> {
    state.connections.close(&column_id);
    state.settings.delete_column(&column_id)?;
    state.settings.clear_column_notes(&column_id)?;
    Ok(())
}

/// 表示中ノートをキャプチャ購読する（他者のリアクション等を追う）。
/// 初期ページ（REST 取得分）はフロントがこれで登録する。Streaming 受信分は Rust が自動登録。
#[tauri::command]
#[specta::specta]
pub async fn capture_notes(
    state: State<'_, AppState>,
    column_id: String,
    note_ids: Vec<String>,
) -> Result<()> {
    state.connections.capture(&column_id, note_ids);
    Ok(())
}

/// キャプチャ解除（表示領域外に出たノート）。
#[tauri::command]
#[specta::specta]
pub async fn uncapture_notes(
    state: State<'_, AppState>,
    column_id: String,
    note_ids: Vec<String>,
) -> Result<()> {
    state.connections.uncapture(&column_id, note_ids);
    Ok(())
}

async fn fetch_initial(state: &AppState, account_id: &str) -> Result<Vec<Note>> {
    let client = state.client_for(account_id)?;
    home_timeline(&client, INITIAL_LIMIT, None).await
}
