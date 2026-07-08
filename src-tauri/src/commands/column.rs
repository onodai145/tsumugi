//! カラム(視覚グループ)とタブ(1タイムライン)の command。
//! タブはソース種別＋フィルタを持ち、購読＋REST取得しフィルタ適用して表示する。
//! 定義は SQLite に永続化し、起動時に list_groups/list_columns → resume_column で復元する。

use crate::api::meta::{fetch_antennas, fetch_followed_channels, fetch_user_lists, resolve_user};
use crate::api::notes::fetch_notes;
use crate::api::notifications::fetch_notifications;
use crate::domain::{
    Column, ColumnGroup, ColumnKind, FilterQuery, Note, Notification, SourceItem, User, UserList,
};
use crate::error::{Error, Result};
use crate::filter::CompiledFilter;
use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

const INITIAL_LIMIT: u32 = 20;
const DEFAULT_WIDTH: i32 = 300;

/// タブを開いた結果。所属グループも返す（新規グループの幅などをフロントへ）。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedColumn {
    pub column: Column,
    pub group: ColumnGroup,
    pub notes: Vec<Note>,
    pub notifications: Vec<Notification>,
}

/// タブを新規作成する。`group_id` が None なら新しい視覚カラム(グループ)を作る。
#[tauri::command]
#[specta::specta]
pub async fn add_column(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    kind: ColumnKind,
    filter: FilterQuery,
    group_id: Option<String>,
) -> Result<OpenedColumn> {
    let (host, token) = state.host_token(&account_id)?;
    let is_notif = matches!(kind, ColumnKind::Notifications);
    if !is_notif && kind.rest_request(1, None).is_none() {
        return Err(Error::Invalid("このソースはまだ未対応です".into()));
    }
    let compiled = CompiledFilter::compile(&filter).map_err(Error::Invalid)?;

    // 所属グループを決める（既存 or 新規）
    let (group, tab_order) = match group_id {
        Some(gid) => {
            let group = state
                .settings
                .load_groups()?
                .into_iter()
                .find(|g| g.id == gid)
                .ok_or_else(|| Error::Invalid(format!("unknown group: {gid}")))?;
            let tab_order =
                state.settings.load_columns()?.iter().filter(|c| c.group_id == gid).count() as i32;
            (group, tab_order)
        }
        None => {
            let order = state.settings.load_groups()?.len() as i32;
            let width = state
                .settings
                .load_ui()
                .map(|p| p.default_column_width)
                .unwrap_or(DEFAULT_WIDTH)
                .clamp(220, 720);
            let group = ColumnGroup {
                id: uuid::Uuid::new_v4().to_string(),
                order,
                width,
            };
            state.settings.upsert_group(&group)?;
            (group, 0)
        }
    };

    let column = Column {
        id: uuid::Uuid::new_v4().to_string(),
        account_id: account_id.clone(),
        kind,
        order: tab_order,
        filter,
        notify_sound: false,
        notify_desktop: false,
        group_id: group.id.clone(),
        title: None,
    };
    state.settings.upsert_column(&column)?;

    let (notes, notifications) = open_stream_and_fetch(&app, &state, &column, compiled, host, token).await?;
    Ok(OpenedColumn {
        column,
        group,
        notes,
        notifications,
    })
}

/// 既存タブのソース種別・フィルタ・名前を変更し、ストリームを張り直す。
/// アカウントは変更しない。フィルタ変更でキャッシュが不整合になるためクリアして再取得する。
#[tauri::command]
#[specta::specta]
pub async fn update_column(
    app: AppHandle,
    state: State<'_, AppState>,
    column_id: String,
    kind: ColumnKind,
    filter: FilterQuery,
    title: Option<String>,
) -> Result<OpenedColumn> {
    let mut column = load_column(&state, &column_id)?;
    let is_notif = matches!(kind, ColumnKind::Notifications);
    if !is_notif && kind.rest_request(1, None).is_none() {
        return Err(Error::Invalid("このソースはまだ未対応です".into()));
    }
    let compiled = CompiledFilter::compile(&filter).map_err(Error::Invalid)?;

    column.kind = kind;
    column.filter = filter;
    column.title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    state.settings.upsert_column(&column)?;

    // 既存ストリームを閉じ、旧フィルタで貯めたキャッシュを捨てる
    state.connections.close(&column_id);
    state.settings.clear_column_notes(&column_id)?;

    let group = state
        .settings
        .load_groups()?
        .into_iter()
        .find(|g| g.id == column.group_id)
        .ok_or_else(|| Error::Invalid(format!("unknown group: {}", column.group_id)))?;
    let (host, token) = state.host_token(&column.account_id)?;
    let (notes, notifications) =
        open_stream_and_fetch(&app, &state, &column, compiled, host, token).await?;

    Ok(OpenedColumn {
        column,
        group,
        notes,
        notifications,
    })
}

/// 永続化済みタブを再開する（起動時の復元）。
#[tauri::command]
#[specta::specta]
pub async fn resume_column(
    app: AppHandle,
    state: State<'_, AppState>,
    column_id: String,
) -> Result<OpenedColumn> {
    let column = load_column(&state, &column_id)?;
    let group = state
        .settings
        .load_groups()?
        .into_iter()
        .find(|g| g.id == column.group_id)
        .ok_or_else(|| Error::Invalid(format!("unknown group: {}", column.group_id)))?;
    let (host, token) = state.host_token(&column.account_id)?;
    let compiled = CompiledFilter::compile(&column.filter).map_err(Error::Invalid)?;

    // 通知以外はキャッシュ優先で即時表示（空なら REST）
    let notes = if matches!(column.kind, ColumnKind::Notifications) {
        vec![]
    } else {
        let cached = state.settings.load_cached(&column.id, INITIAL_LIMIT)?;
        if cached.is_empty() { vec![] } else { cached }
    };

    let (fresh_notes, notifications) = if notes.is_empty() {
        open_stream_and_fetch(&app, &state, &column, compiled, host, token).await?
    } else {
        // キャッシュがある: ストリームだけ張り直す
        if let Some((channel, params)) = column.kind.stream_request() {
            state.connections.open_channel(
                app,
                column.id.clone(),
                column.account_id.clone(),
                host,
                token,
                channel,
                params,
                compiled,
                state.eval_context(),
            );
        }
        (notes, vec![])
    };

    Ok(OpenedColumn {
        column,
        group,
        notes: fresh_notes,
        notifications,
    })
}

/// 永続化済みグループ一覧。
#[tauri::command]
#[specta::specta]
pub async fn list_groups(state: State<'_, AppState>) -> Result<Vec<ColumnGroup>> {
    state.settings.load_groups()
}

/// 永続化済みタブ一覧。
#[tauri::command]
#[specta::specta]
pub async fn list_columns(state: State<'_, AppState>) -> Result<Vec<Column>> {
    state.settings.load_columns()
}

/// 過去ページ（上スクロール）。
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

/// 通知カラムの過去ページ。
#[tauri::command]
#[specta::specta]
pub async fn fetch_notifications_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
) -> Result<Vec<Notification>> {
    let column = load_column(&state, &column_id)?;
    let client = state.client_for(&column.account_id)?;
    let raw = fetch_notifications(&client, INITIAL_LIMIT, Some(&until_id)).await?;
    Ok(filter_notifications(&state, &column.account_id, raw))
}

/// グループ幅を更新（永続化）。
#[tauri::command]
#[specta::specta]
pub async fn set_group_width(state: State<'_, AppState>, group_id: String, width: i32) -> Result<()> {
    state.settings.set_group_width(&group_id, width.clamp(220, 720))
}

/// グループ(視覚カラム)の並び順を更新。
#[tauri::command]
#[specta::specta]
pub async fn reorder_groups(state: State<'_, AppState>, ordered_ids: Vec<String>) -> Result<()> {
    state.settings.reorder_groups(&ordered_ids)
}

/// タブを別グループへ移動し、そのグループ内順序を更新（並べ替え兼移動）。
/// `ordered_tab_ids` は移動先グループのタブを希望順に並べた id 列。
#[tauri::command]
#[specta::specta]
pub async fn move_tab(
    state: State<'_, AppState>,
    tab_id: String,
    group_id: String,
    ordered_tab_ids: Vec<String>,
) -> Result<()> {
    state.settings.move_tab(&tab_id, &group_id, &ordered_tab_ids)?;
    state.settings.delete_empty_groups()?;
    Ok(())
}

/// タブを閉じる（購読解除＋永続層から削除＋空グループ掃除）。
#[tauri::command]
#[specta::specta]
pub async fn close_column(state: State<'_, AppState>, column_id: String) -> Result<()> {
    state.connections.close(&column_id);
    state.settings.delete_column(&column_id)?;
    state.settings.clear_column_notes(&column_id)?;
    state.settings.delete_empty_groups()?;
    Ok(())
}

/// 表示中ノートをキャプチャ購読する。
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

/// キャプチャ解除。
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

/// フィルタ（TQL/キーワード）の妥当性検証。
#[tauri::command]
#[specta::specta]
pub async fn validate_filter(filter: FilterQuery) -> Result<()> {
    CompiledFilter::compile(&filter).map(|_| ()).map_err(Error::Invalid)
}

/// ユーザリスト一覧（List タブ作成用）。
#[tauri::command]
#[specta::specta]
pub async fn list_user_lists(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<UserList>> {
    let client = state.client_for(&account_id)?;
    fetch_user_lists(&client).await
}

/// タブ名を変更する（空文字/None で自動生成名に戻す）。
#[tauri::command]
#[specta::specta]
pub async fn rename_column(
    state: State<'_, AppState>,
    column_id: String,
    title: Option<String>,
) -> Result<()> {
    let trimmed = title.as_deref().map(str::trim).filter(|s| !s.is_empty());
    state.settings.set_column_title(&column_id, trimmed)
}

/// アンテナ一覧（Antenna タブ作成用）。
#[tauri::command]
#[specta::specta]
pub async fn list_antennas(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<SourceItem>> {
    let client = state.client_for(&account_id)?;
    fetch_antennas(&client).await
}

/// フォロー中チャンネル一覧（Channel タブ作成用）。
#[tauri::command]
#[specta::specta]
pub async fn list_channels(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<SourceItem>> {
    let client = state.client_for(&account_id)?;
    fetch_followed_channels(&client).await
}

/// acct から User を解決（User タブ作成用）。
#[tauri::command]
#[specta::specta]
pub async fn resolve_user_acct(
    state: State<'_, AppState>,
    account_id: String,
    acct: String,
) -> Result<User> {
    let client = state.client_for(&account_id)?;
    resolve_user(&client, &acct).await
}

// ---- helpers ----

fn load_column(state: &AppState, column_id: &str) -> Result<Column> {
    state
        .settings
        .load_columns()?
        .into_iter()
        .find(|c| c.id == column_id)
        .ok_or_else(|| Error::Invalid(format!("unknown column: {column_id}")))
}

/// タブのストリームを開き、初期ページ(ノート or 通知)を取得する。
async fn open_stream_and_fetch(
    app: &AppHandle,
    state: &AppState,
    column: &Column,
    compiled: CompiledFilter,
    host: String,
    token: String,
) -> Result<(Vec<Note>, Vec<Notification>)> {
    if matches!(column.kind, ColumnKind::Notifications) {
        let client = state.client_for(&column.account_id)?;
        let raw = fetch_notifications(&client, INITIAL_LIMIT, None).await?;
        let notifications = filter_notifications(state, &column.account_id, raw);
        state.connections.open_notifications(
            app.clone(),
            column.id.clone(),
            column.account_id.clone(),
            host,
            token,
        );
        return Ok((vec![], notifications));
    }

    let notes = fetch_and_filter(state, &column.account_id, column, &compiled, None).await?;
    state.settings.cache_notes(&column.id, &notes)?;
    if let Some((channel, params)) = column.kind.stream_request() {
        state.connections.open_channel(
            app.clone(),
            column.id.clone(),
            column.account_id.clone(),
            host,
            token,
            channel,
            params,
            compiled,
            state.eval_context(),
        );
    }
    Ok((notes, vec![]))
}

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
    let mute = state.mute.lock().unwrap().clone();
    Ok(raw
        .into_iter()
        .filter(|n| {
            compiled.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
        })
        .collect())
}

/// ノート本体 or renote 先のユーザがサーバ側ミュート/ブロック対象か。
fn server_muted_note(state: &AppState, account_id: &str, n: &Note) -> bool {
    if state.is_server_muted(account_id, &n.user.id) {
        return true;
    }
    matches!(&n.renote, Some(r) if state.is_server_muted(account_id, &r.user.id))
}

/// 通知一覧から、発生元ユーザが NG（ローカル）/サーバミュート・ブロックのものを除く。
fn filter_notifications(
    state: &AppState,
    account_id: &str,
    raw: Vec<Notification>,
) -> Vec<Notification> {
    let mute = state.mute.lock().unwrap().clone();
    raw.into_iter()
        .filter(|n| match &n.user {
            Some(u) => {
                !state.is_server_muted(account_id, &u.id)
                    && !crate::filter::mute::is_user_muted(u, &mute)
            }
            None => true,
        })
        .collect()
}
