//! カラムの command。ソース（Home/Local/Hybrid/Global）別にチャンネル購読＋REST初期取得し、
//! フィルタ（TQL述語 or キーワード）を適用して通過分のみ表示する。
//! カラム定義は SQLite に永続化し、再起動時に list_columns → resume_column で復元する。

use crate::api::meta::fetch_user_lists;
use crate::api::notes::fetch_notes;
use crate::api::notifications::fetch_notifications;
use crate::domain::{Column, ColumnKind, FilterQuery, Note, Notification, UserList};
use crate::error::{Error, Result};
use crate::filter::CompiledFilter;
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

const INITIAL_LIMIT: u32 = 20;
const DEFAULT_WIDTH: i32 = 300;

/// カラムを開いた結果。ノートカラムは `notes`、通知カラムは `notifications` が入る。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedColumn {
    pub column: Column,
    /// 初期表示用の直近ノート（フィルタ通過済み・新しい順）
    pub notes: Vec<Note>,
    /// 通知カラムの初期通知（新しい順）
    pub notifications: Vec<Notification>,
}

/// カラムを新規作成する。ソース種別＋フィルタを受け、購読を開始する。
#[tauri::command]
#[specta::specta]
pub async fn add_column(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    kind: ColumnKind,
    filter: FilterQuery,
) -> Result<OpenedColumn> {
    let (host, token) = state.host_token(&account_id)?;
    let is_notif = matches!(kind, ColumnKind::Notifications);
    // REST 取得できないソースはまだ未対応（Notifications は別経路なので除外）
    if !is_notif && kind.rest_request(1, None).is_none() {
        return Err(Error::Invalid("このソースはまだ未対応です".into()));
    }
    // フィルタをコンパイル（TQL のパースエラーはここで弾く）
    let compiled = CompiledFilter::compile(&filter).map_err(Error::Invalid)?;

    let order = state.settings.load_columns()?.len() as i32;
    let column = Column {
        id: uuid::Uuid::new_v4().to_string(),
        account_id: account_id.clone(),
        kind,
        order,
        width: DEFAULT_WIDTH,
        filter,
        notify_sound: false,
        notify_desktop: false,
    };
    state.settings.upsert_column(&column)?;

    if is_notif {
        let client = state.client_for(&account_id)?;
        let notifications = fetch_notifications(&client, INITIAL_LIMIT, None).await?;
        state
            .connections
            .open_notifications(app, column.id.clone(), host, token);
        return Ok(OpenedColumn {
            column,
            notes: vec![],
            notifications,
        });
    }

    let notes = fetch_and_filter(&state, &account_id, &column, &compiled, None).await?;
    state.settings.cache_notes(&column.id, &notes)?;
    // ストリーミングを持つソースのみ購読を開く（Search は REST のみ）
    if let Some((channel, params)) = column.kind.stream_request() {
        state.connections.open_channel(
            app,
            column.id.clone(),
            host,
            token,
            channel,
            params,
            compiled,
            state.eval_context(),
        );
    }

    Ok(OpenedColumn {
        column,
        notes,
        notifications: vec![],
    })
}

/// 永続化済みカラムを再開する（起動時の復元）。キャッシュ優先で即時表示し購読を張り直す。
#[tauri::command]
#[specta::specta]
pub async fn resume_column(
    app: AppHandle,
    state: State<'_, AppState>,
    column_id: String,
) -> Result<OpenedColumn> {
    let column = load_column(&state, &column_id)?;
    let (host, token) = state.host_token(&column.account_id)?;

    // 通知カラムはキャッシュせず毎回 i/notifications を取得
    if matches!(column.kind, ColumnKind::Notifications) {
        let client = state.client_for(&column.account_id)?;
        let notifications = fetch_notifications(&client, INITIAL_LIMIT, None).await?;
        state
            .connections
            .open_notifications(app, column.id.clone(), host, token);
        return Ok(OpenedColumn {
            column,
            notes: vec![],
            notifications,
        });
    }

    if column.kind.rest_request(1, None).is_none() {
        return Err(Error::Invalid("このソースはまだ未対応です".into()));
    }
    let compiled = CompiledFilter::compile(&column.filter).map_err(Error::Invalid)?;

    // キャッシュ（既にフィルタ通過済み）を即時表示。空なら REST 取得。
    let cached = state.settings.load_cached(&column.id, INITIAL_LIMIT)?;
    let notes = if cached.is_empty() {
        let fresh = fetch_and_filter(&state, &column.account_id, &column, &compiled, None).await?;
        state.settings.cache_notes(&column.id, &fresh)?;
        fresh
    } else {
        cached
    };
    if let Some((channel, params)) = column.kind.stream_request() {
        state.connections.open_channel(
            app,
            column.id.clone(),
            host,
            token,
            channel,
            params,
            compiled,
            state.eval_context(),
        );
    }

    Ok(OpenedColumn {
        column,
        notes,
        notifications: vec![],
    })
}

/// 永続化済みカラム定義の一覧（起動時に取得 → resume_column で復元）。
#[tauri::command]
#[specta::specta]
pub async fn list_columns(state: State<'_, AppState>) -> Result<Vec<Column>> {
    state.settings.load_columns()
}

/// 過去ページ（上スクロール）。カラムのソースから取得し、フィルタ適用＆キャッシュする。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
) -> Result<Vec<Note>> {
    let column = load_column(&state, &column_id)?;
    let compiled = CompiledFilter::compile(&column.filter).map_err(Error::Invalid)?;
    let notes = fetch_and_filter(&state, &column.account_id, &column, &compiled, Some(&until_id)).await?;
    state.settings.cache_notes(&column.id, &notes)?;
    Ok(notes)
}

/// 通知カラムの過去ページ。`until_id` より古い通知を返す。
#[tauri::command]
#[specta::specta]
pub async fn fetch_notifications_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
) -> Result<Vec<Notification>> {
    let column = load_column(&state, &column_id)?;
    let client = state.client_for(&column.account_id)?;
    fetch_notifications(&client, INITIAL_LIMIT, Some(&until_id)).await
}

/// カラム幅を更新（永続化）。
#[tauri::command]
#[specta::specta]
pub async fn set_column_width(
    state: State<'_, AppState>,
    column_id: String,
    width: i32,
) -> Result<()> {
    state.settings.set_column_width(&column_id, width.clamp(220, 720))
}

/// カラムの並び順を更新（与えた id 順に振り直す・永続化）。
#[tauri::command]
#[specta::specta]
pub async fn reorder_columns(state: State<'_, AppState>, ordered_ids: Vec<String>) -> Result<()> {
    state.settings.reorder_columns(&ordered_ids)
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

/// 表示中ノートをキャプチャ購読する（他者のリアクション等を追う。初期ページ分をフロントが登録）。
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

/// フィルタ（TQL/キーワード）の妥当性を検証する（UI の入力チェック用）。
#[tauri::command]
#[specta::specta]
pub async fn validate_filter(filter: FilterQuery) -> Result<()> {
    CompiledFilter::compile(&filter).map(|_| ()).map_err(Error::Invalid)
}

/// 自分のユーザリスト一覧（List カラム作成時の選択用）。
#[tauri::command]
#[specta::specta]
pub async fn list_user_lists(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<UserList>> {
    let client = state.client_for(&account_id)?;
    fetch_user_lists(&client).await
}

fn load_column(state: &AppState, column_id: &str) -> Result<Column> {
    state
        .settings
        .load_columns()?
        .into_iter()
        .find(|c| c.id == column_id)
        .ok_or_else(|| Error::Invalid(format!("unknown column: {column_id}")))
}

/// カラムのソースから 1 ページ取得し、フィルタ通過分のみ返す（until_id で過去ページ）。
async fn fetch_and_filter(
    state: &AppState,
    account_id: &str,
    column: &Column,
    compiled: &CompiledFilter,
    until_id: Option<&str>,
) -> Result<Vec<Note>> {
    let (endpoint, body) = column
        .kind
        .rest_request(INITIAL_LIMIT, until_id)
        .ok_or_else(|| Error::Invalid("このソースはまだ未対応です".into()))?;
    let client = state.client_for(account_id)?;
    let raw = fetch_notes(&client, endpoint, &body).await?;
    let ctx = state.eval_context();
    Ok(raw.into_iter().filter(|n| compiled.matches(n, &ctx)).collect())
}
