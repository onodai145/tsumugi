//! カラム(視覚グループ)とタブ(1タイムライン)の command。
//! タブはソース種別＋フィルタを持ち、購読＋REST取得しフィルタ適用して表示する。
//! 定義は SQLite に永続化し、起動時に list_groups/list_columns → resume_column で復元する。

use crate::api::meta::{fetch_antennas, fetch_followed_channels, fetch_user_lists, resolve_user};
use crate::api::notes::fetch_notes;
use crate::api::notifications::fetch_notifications;
use crate::domain::{
    Column, ColumnGroup, ColumnKind, Edge, FilterQuery, MuteConfig, Note, Notification, PaneNode,
    SourceItem, SplitDirection, User, UserList,
};
use crate::error::{Error, Result};
use crate::filter::{ast, eval::EvalContext, parser, sql, CompiledFilter};
use crate::state::AppState;
use crate::store::NoteCacheStore;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event as _;

const INITIAL_LIMIT: u32 = 20;
const DEFAULT_WIDTH: i32 = 300;
const GAP_FILL_PAGE_SIZE: u32 = 100;
const GAP_FILL_MAX_PAGES: u32 = 10;

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
    let resolved = if is_notif {
        None
    } else {
        Some(resolve_sources(&state, &account_id, &kind, &filter).await?)
    };

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
                auto: false,
            };
            // load_pane_layout は「groupsに存在するのに木に無いグループ」を自動補完する
            // (Issue #31の自己修復ロジック)。この group はまだ upsert_group していない
            // = groupsにまだ存在しないため、ここで読んでも補完対象にならない。
            // 先にupsert_groupしてしまうと、次のload_pane_layoutが「木に無い新規グループ」
            // として自動補完し、直後の明示的なappend_row_leafと合わせて二重挿入になる
            // (実際に発生した不具合: カラム追加のたびに2つ追加される)。
            let mut root = state.settings.load_pane_layout()?;
            root.append_row_leaf(&group.id, width as f32);
            state.settings.upsert_group(&group)?;
            state.settings.save_pane_layout(&root)?;
            (group, 0)
        }
    };

    let column = Column {
        id: uuid::Uuid::new_v4().to_string(),
        account_id: account_id.clone(),
        kind,
        order: tab_order,
        filter,
        // 通知タブは従来どおり既定ON（オプトアウト方式）。それ以外のタブは新機能なので
        // 既定OFF（オプトイン）にし、Global/Local 等の高頻度タブでの通知過多を避ける。
        // 設定→通知のグローバルスイッチと両方ONのときのみ実際に発火する。
        notify_sound: is_notif,
        notify_desktop: is_notif,
        notify_sound_choice: String::new(),
        group_id: group.id.clone(),
        title: None,
    };
    state.settings.upsert_column(&column)?;

    let (notes, notifications) = open_stream_and_fetch(&app, &state, &column, resolved, host, token).await?;
    Ok(OpenedColumn {
        column,
        group,
        notes,
        notifications,
    })
}

/// reference_group_id の隣に空の新規グループ(タブなし)を挿入し、その ColumnGroup を返す。
/// フロントは戻り値の group.id で AddColumnModal を「このグループにタブ追加」モードで開く。
#[tauri::command]
#[specta::specta]
pub async fn split_pane(
    state: State<'_, AppState>,
    reference_group_id: String,
    direction: SplitDirection,
) -> Result<ColumnGroup> {
    let order = state.settings.load_groups()?.len() as i32;
    let width = state
        .settings
        .load_ui()
        .map(|p| p.default_column_width)
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(220, 720);
    let group = ColumnGroup { id: uuid::Uuid::new_v4().to_string(), order, width, auto: false };
    // upsert_groupより先にload_pane_layoutする(add_columnと同じ理由: 自己修復ロジックとの
    // 二重挿入を避けるため。Issue #31)。
    let mut root = state.settings.load_pane_layout()?;
    if !root.insert_sibling(&reference_group_id, &group.id, direction) {
        // reference_group_idはフロントが既存グループのidしか渡さない前提のため通常到達しない。
        return Err(Error::Invalid(format!("unknown reference group: {reference_group_id}")));
    }
    state.settings.upsert_group(&group)?;
    state.settings.save_pane_layout(&root)?;
    Ok(group)
}

/// ペインノード(Leaf/Splitどちらのidでも可)のsizeを更新する(Column分割の高さ調整用)。
#[tauri::command]
#[specta::specta]
pub async fn resize_pane(state: State<'_, AppState>, node_id: String, size: f32) -> Result<()> {
    let mut root = state.settings.load_pane_layout()?;
    if !root.set_size(&node_id, size) {
        return Err(Error::Invalid(format!("unknown pane node: {node_id}")));
    }
    state.settings.save_pane_layout(&root)
}

/// ペインノード(Leaf/Splitどちらのidでも可)のauto(自動幅調整)フラグを更新する。
#[tauri::command]
#[specta::specta]
pub async fn set_pane_auto(state: State<'_, AppState>, node_id: String, auto: bool) -> Result<()> {
    let mut root = state.settings.load_pane_layout()?;
    if !root.set_auto(&node_id, auto) {
        return Err(Error::Invalid(format!("unknown pane node: {node_id}")));
    }
    state.settings.save_pane_layout(&root)
}

/// dragged_group_idを木から取り外し(親が1子になれば畳む)、target_group_idの指定エッジに
/// 挿入する(内部的には「remove_group→insert_sibling_at」の組み合わせ)。
/// dragged_group_id == target_group_idの場合は何もしない(同じ場所への無意味なドロップ)。
#[tauri::command]
#[specta::specta]
pub async fn move_pane(state: State<'_, AppState>, dragged_group_id: String, target_group_id: String, edge: Edge) -> Result<()> {
    if dragged_group_id == target_group_id {
        return Ok(());
    }
    let mut root = state.settings.load_pane_layout()?;
    if !root.remove_group(&dragged_group_id) {
        return Err(Error::Invalid(format!("unknown dragged group: {dragged_group_id}")));
    }
    if !root.insert_sibling_at(&target_group_id, &dragged_group_id, edge.direction(), edge.before()) {
        return Err(Error::Invalid(format!("unknown target group: {target_group_id}")));
    }
    state.settings.save_pane_layout(&root)
}

/// 永続化済みペイン分割ツリー(起動時のレイアウト復元用)。
#[tauri::command]
#[specta::specta]
pub async fn load_pane_layout(state: State<'_, AppState>) -> Result<PaneNode> {
    state.settings.load_pane_layout()
}

/// タブが1つも無い空グループを削除する(split_paneでタブ追加をキャンセルされた後始末用)。
/// タブが残っている場合は何もしない(誤操作防止)。
#[tauri::command]
#[specta::specta]
pub async fn discard_empty_group(state: State<'_, AppState>, group_id: String) -> Result<()> {
    let has_tabs = state.settings.load_columns()?.iter().any(|c| c.group_id == group_id);
    if has_tabs {
        return Ok(());
    }
    state.settings.delete_empty_groups()?; // group_id自体がタブ0件ならここで消え、木からも畳まれる
    Ok(())
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
    let resolved = if is_notif {
        None
    } else {
        Some(resolve_sources(&state, &column.account_id, &kind, &filter).await?)
    };

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
    state.cache.clear_column_notes(&column_id).await?;

    let group = state
        .settings
        .load_groups()?
        .into_iter()
        .find(|g| g.id == column.group_id)
        .ok_or_else(|| Error::Invalid(format!("unknown group: {}", column.group_id)))?;
    let (host, token) = state.host_token(&column.account_id)?;
    let (notes, notifications) =
        open_stream_and_fetch(&app, &state, &column, resolved, host, token).await?;

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
    let is_notif = matches!(column.kind, ColumnKind::Notifications);
    let resolved = if is_notif {
        None
    } else {
        Some(resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?)
    };

    // 通知以外はキャッシュ優先で即時表示（空なら REST）
    let notes = if is_notif {
        vec![]
    } else {
        let cached = state.cache.load_cached(&column.id, INITIAL_LIMIT).await?;
        if cached.is_empty() { vec![] } else { cached }
    };

    let (fresh_notes, notifications) = if notes.is_empty() {
        open_stream_and_fetch(&app, &state, &column, resolved, host, token).await?
    } else {
        // キャッシュがある: まずキャッシュを即返して体感速度を維持し、閉じていた間のギャップ埋めは
        // バックグラウンドで行って ColumnGapFill イベントでまとめて反映する（1件ずつ ColumnNote を
        // 出すと新着通知/通知音が誤爆するため、専用イベントで通知ロジックを経由させない）。
        let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
        let gap_limit = state
            .settings
            .load_ui()
            .map(|p| p.gap_fill_limit)
            .unwrap_or(0)
            .max(0);
        let newest_known_id = notes[0].id.clone(); // load_cached は created_at 降順（先頭が最新）
        open_streams_only(&app, &state, &column, &resolved, host, token);
        if gap_limit > 0 {
            let app2 = app.clone();
            let column_id = column.id.clone();
            let account_id = column.account_id.clone();
            tauri::async_runtime::spawn(async move {
                let Some(state) = app2.try_state::<AppState>() else { return };
                let gap_result = fill_gap(&state, &account_id, &resolved, &newest_known_id, gap_limit)
                    .await
                    .unwrap_or(GapFillResult { notes: vec![], truncated: false, boundary_id: None });
                if gap_result.notes.is_empty() {
                    return;
                }
                let _ = state.cache.cache_notes(&column_id, &gap_result.notes).await;
                let _ = crate::events::ColumnGapFill {
                    column_id,
                    notes: gap_result.notes,
                    truncated: gap_result.truncated,
                    boundary_id: gap_result.boundary_id,
                    target_id: if gap_result.truncated {
                        Some(newest_known_id)
                    } else {
                        None
                    },
                }
                .emit(&app2);
            });
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

/// ローカルDBにキャッシュ済みのノート総数。Backstageのステータス表示用。
#[tauri::command]
#[specta::specta]
pub async fn note_count(state: State<'_, AppState>) -> Result<i32> {
    state.cache.note_count().await
}

/// 投稿日時(epoch秒)が since_epoch_secs 以降のノート件数。Backstageの流速表示用。
#[tauri::command]
#[specta::specta]
pub async fn notes_since(state: State<'_, AppState>, since_epoch_secs: i32) -> Result<i32> {
    state.cache.notes_since(since_epoch_secs).await
}

/// 設定（表示→ノートキャッシュの上限）に従ってキャッシュから古いノートを削除する（Issue #6）。
/// 上限0なら無制限で何もしない。実際に削除した件数を返す。
#[tauri::command]
#[specta::specta]
pub async fn prune_note_cache(state: State<'_, AppState>) -> Result<i32> {
    let ui = state.settings.load_ui()?;
    Ok(state
        .cache
        .prune(ui.note_cache_limit, ui.note_cache_max_age_days, ui.note_cache_max_size_mb)
        .await?
        as i32)
}

/// 過去ページ（上スクロール）。単一ソースのカラムは、要求範囲がbackfill境界より新しければ
/// キャッシュのみで応答する(Issue #228)。境界未確定・範囲外・件数不足なら通常どおりAPIへ。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
) -> Result<Vec<Note>> {
    let column = load_column(&state, &column_id)?;
    let resolved = resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?;

    let cache_eligible = resolved.kinds.len() == 1 && !resolved.use_cache;
    let boundary = if cache_eligible {
        state.cache.get_fetch_boundary(&column.id).await.ok().flatten()
    } else {
        None
    };
    if cache_eligible {
        let mut cached = match &boundary {
            Some(b) if until_id.as_str() > b.as_str() => state
                .cache
                .load_cached_before(&column.id, &until_id, INITIAL_LIMIT)
                .await
                .unwrap_or_default(),
            _ => vec![],
        };
        // [boundary, until_id) の範囲外(=このセッションでは未検証)の行を除外する。
        // load_cached_before 自体は下限を持たないため、範囲内の件数が不足していても
        // セッションをまたいだ古いキャッシュ行で limit を満たしてしまう可能性がある。
        if let Some(b) = &boundary {
            cached.retain(|n| n.id.as_str() >= b.as_str());
        }
        // ミュート/フィルタ設定はキャッシュ後に変更されうるため、都度再適用する。
        let ctx = state.eval_context();
        let mute = state.mute.lock().unwrap().clone();
        cached.retain(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(&state, &column.account_id, n)
                && !state.is_word_muted(&column.account_id, n)
        });
        if let Some(notes) = cache_backfill_page(boundary.as_deref(), &until_id, cached, INITIAL_LIMIT) {
            return Ok(notes);
        }
    }

    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    state.cache.cache_notes(&column.id, &fetch.notes).await?;
    if cache_eligible {
        if let Some(oldest) = &fetch.raw_oldest_id {
            // 既存の境界と連続している(=until_idが境界以上)場合のみ延長する。
            // 不連続な場合(例: fillRemainingGapがgap markerのtargetIdまで遡って取得した場合)に
            // 境界と今回の取得範囲の間の未検証の隙間を「完全」と誤認するのを防ぐ(Issue #228)。
            // 境界未確定(None)なら連続性を検証できないので、延長せずAPI経由のままにする
            // (境界はカラム開き直し時の open_stream_and_fetch が改めて確定させる)。
            let contiguous = match &boundary {
                Some(b) => until_id.as_str() >= b.as_str(),
                None => false,
            };
            if contiguous {
                let _ = state.cache.extend_fetch_boundary(&column.id, oldest).await;
            }
        }
    }
    Ok(fetch.notes)
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

/// グループの幅モード（固定/自動調整）を更新。
#[tauri::command]
#[specta::specta]
pub async fn set_group_auto(state: State<'_, AppState>, group_id: String, auto: bool) -> Result<()> {
    state.settings.set_group_auto(&group_id, auto)
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
    state.cache.clear_column_notes(&column_id).await?;
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

/// エキスパートモード用: `from <sources> where <expr>` 全文の構文検証のみ行う。
/// list/antenna/channel の id 存在確認や user acct 解決は行わない（実際の解決はカラム作成時）。
#[tauri::command]
#[specta::specta]
pub async fn validate_tql_query(text: String) -> Result<()> {
    let q = parser::parse(&text).map_err(Error::Invalid)?;
    if q.sources.is_empty() {
        return Err(Error::Invalid("from 節に1つ以上ソースが必要です".into()));
    }
    Ok(())
}

/// TQL入力補完。カーソル位置までの部分入力を文脈分類し、候補一覧を返す。
/// list/antenna/channel の実ID候補はフロント側で別途解決する(このコマンドは構文語彙のみ)。
#[tauri::command]
#[specta::specta]
pub fn tql_complete(
    text: String,
    cursor: u32,
    mode: crate::filter::complete::TqlEditMode,
) -> Vec<crate::filter::complete::TqlCompletionItem> {
    crate::filter::complete::complete(&text, cursor as usize, mode)
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

/// タブごとの通知可否・通知音の選択を変更する。ストリームは張り直さない軽量操作。
/// notify_sound_choice は空文字ならグローバル設定を継承する。
#[tauri::command]
#[specta::specta]
pub async fn set_column_notify(
    state: State<'_, AppState>,
    column_id: String,
    notify_desktop: bool,
    notify_sound: bool,
    notify_sound_choice: String,
) -> Result<()> {
    state
        .settings
        .set_column_notify(&column_id, notify_desktop, notify_sound, &notify_sound_choice)
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

/// カラムの実ソース群。単一ソースのカラムは kinds に1件、TQLエキスパートモードの
/// カラムは `from` 節に列挙されたソース数だけ入る（cache は kinds に含めず use_cache で表す）。
struct ResolvedSources {
    kinds: Vec<ColumnKind>,
    use_cache: bool,
    filter: CompiledFilter,
}

/// kind/filter からこのカラムの実ソース群を解決する。単一ソースのカラムは従来どおり
/// `CompiledFilter::compile`(where述語のみ)。`ColumnKind::Tql` は filter 全文
/// (`from <sources> where <expr>`)をパースし、各ソースを解決する（User は acct→userId 解決の
/// ため非同期）。
async fn resolve_sources(
    state: &AppState,
    account_id: &str,
    kind: &ColumnKind,
    filter: &FilterQuery,
) -> Result<ResolvedSources> {
    if !matches!(kind, ColumnKind::Tql) {
        if kind.rest_request(1, None).is_none() {
            return Err(Error::Invalid("このソースはまだ未対応です".into()));
        }
        let compiled = CompiledFilter::compile(filter).map_err(Error::Invalid)?;
        return Ok(ResolvedSources {
            kinds: vec![kind.clone()],
            use_cache: false,
            filter: compiled,
        });
    }

    let FilterQuery::Tql(text) = filter else {
        return Err(Error::Invalid("TQLカラムには from 節を含むクエリが必要です".into()));
    };
    let q = parser::parse(text).map_err(Error::Invalid)?;
    if q.sources.is_empty() {
        return Err(Error::Invalid("from 節に1つ以上ソースが必要です".into()));
    }

    let mut kinds = Vec::new();
    let mut use_cache = false;
    for s in &q.sources {
        match s {
            ast::Source::Cache => use_cache = true,
            ast::Source::Mentions => {
                return Err(Error::Invalid("mentions ソースは現在未対応です".into()))
            }
            ast::Source::User(acct) => {
                let client = state.client_for(account_id)?;
                let u = resolve_user(&client, acct).await?;
                kinds.push(ColumnKind::User { user_id: u.id });
            }
            ast::Source::Home => kinds.push(ColumnKind::Home),
            ast::Source::Local => kinds.push(ColumnKind::Local),
            ast::Source::Hybrid => kinds.push(ColumnKind::Hybrid),
            ast::Source::Global => kinds.push(ColumnKind::Global),
            ast::Source::List(id) => kinds.push(ColumnKind::List { list_id: id.clone() }),
            ast::Source::Antenna(id) => kinds.push(ColumnKind::Antenna { antenna_id: id.clone() }),
            ast::Source::Channel(id) => kinds.push(ColumnKind::Channel { channel_id: id.clone() }),
            ast::Source::Tag(t) => kinds.push(ColumnKind::Tag { tag: t.clone() }),
            ast::Source::Search(query) => kinds.push(ColumnKind::Search { query: query.clone() }),
        }
    }
    if kinds.is_empty() && !use_cache {
        return Err(Error::Invalid("有効なソースがありません".into()));
    }
    let filter = match q.predicate {
        Some(expr) => CompiledFilter::Tql(expr),
        None => CompiledFilter::PassAll,
    };
    Ok(ResolvedSources { kinds, use_cache, filter })
}

/// タブのストリームを開き、初期ページ(ノート or 通知)を取得する。
async fn open_stream_and_fetch(
    app: &AppHandle,
    state: &AppState,
    column: &Column,
    resolved: Option<ResolvedSources>,
    host: String,
    token: String,
) -> Result<(Vec<Note>, Vec<Notification>)> {
    if matches!(column.kind, ColumnKind::Notifications) {
        let client = state.client_for(&column.account_id)?;
        let raw = fetch_notifications(&client, INITIAL_LIMIT, None).await?;
        // 初期REST取得で得た最新id(新しい順の先頭)を再接続ギャップ埋めの初期ウォーターマークにする。
        // ライブ配信で1件も受信しないまま再接続した場合でもギャップ埋めが機能するようにするため。
        let initial_last_seen_id = raw.first().map(|n| n.id.clone());
        let notifications = filter_notifications(state, &column.account_id, raw);
        state.connections.open_notifications(
            app.clone(),
            column.id.clone(),
            column.account_id.clone(),
            host,
            token,
            initial_last_seen_id,
        );
        return Ok((vec![], notifications));
    }

    let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
    let fetch = fetch_and_filter_multi(state, &column.account_id, &resolved, None).await?;
    state.cache.cache_notes(&column.id, &fetch.notes).await?;
    if resolved.kinds.len() == 1 && !resolved.use_cache {
        if let Some(oldest) = &fetch.raw_oldest_id {
            let _ = state.cache.set_fetch_boundary(&column.id, oldest).await;
        }
    }
    open_streams_only(app, state, column, &resolved, host, token);
    Ok((fetch.notes, vec![]))
}

/// 解決済みソースのうちストリーミング対応のものだけ購読を開く（REST初期取得は済んでいる前提）。
/// 複数ソースは column_id を共有しつつ sub_key を分けて同一カラムへ多重購読させる。
fn open_streams_only(
    app: &AppHandle,
    state: &AppState,
    column: &Column,
    resolved: &ResolvedSources,
    host: String,
    token: String,
) {
    for (i, k) in resolved.kinds.iter().enumerate() {
        if let Some((channel, params)) = k.stream_request() {
            state.connections.open_channel(
                app.clone(),
                format!("{}#{}", column.id, i),
                column.id.clone(),
                column.account_id.clone(),
                host.clone(),
                token.clone(),
                channel,
                params,
                resolved.filter.clone(),
                state.eval_context(),
            );
        }
    }
}

/// `fill_gap` の結果。`newest_known_id` に追いつけたか(=打ち切りが起きたか)を
/// 呼び出し元(resume_column/gap_fill_on_reconnect)がフロントへ伝えるために持つ。
struct GapFillResult {
    notes: Vec<Note>,
    /// newest_known_id に追いつく前に limit/ページ数上限で打ち切られた場合 true。
    truncated: bool,
    /// truncated=true のとき、取得できた中で一番古いノートのid。
    /// フロントが「続きを取得」で fetch_backfill の until_id に使う。
    boundary_id: Option<String>,
}

/// 収集済みノートを重複除去・整形し、`newest_known_id` に追いつけたかを判定する。
/// ネットワークを伴わない純粋関数にして単体テストしやすくしている。
fn finalize_gap_fill(mut collected: Vec<Note>, all_sources_reached_target: bool, limit: i32) -> GapFillResult {
    let mut seen = std::collections::HashSet::new();
    collected.retain(|n| seen.insert(n.id.clone()));
    collected.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    collected.truncate(limit.max(0) as usize);
    let truncated = !all_sources_reached_target && !collected.is_empty();
    let boundary_id = if truncated { collected.last().map(|n| n.id.clone()) } else { None };
    GapFillResult { notes: collected, truncated, boundary_id }
}

/// backfill 要求(until_id より古いページ)をキャッシュのみで賄えるか判定する純粋関数。
/// `boundary`(Some) は「これより新しいノートはAPI取得済みで完全」という境界。
/// `cached` は呼び出し元が事前に `load_cached_before` で取得した結果。
/// 境界が未確定、要求範囲が境界に届かない(未検証領域を含みうる)、
/// またはキャッシュ件数が limit に満たない場合は None(=APIへフォールバックすべき)を返す。
fn cache_backfill_page(
    boundary: Option<&str>,
    until_id: &str,
    cached: Vec<Note>,
    limit: u32,
) -> Option<Vec<Note>> {
    let boundary = boundary?;
    if until_id <= boundary {
        return None;
    }
    if cached.len() as u32 >= limit {
        Some(cached)
    } else {
        None
    }
}

/// 起動時のギャップ埋め: アプリを閉じていた間に流れたノートを、キャッシュの最新ノートid
/// (`newest_known_id`)まで REST で遡って取得する。`limit` 件、または既知のノートに追いつく
/// (取得ページの中に newest_known_id 以前のノートが現れる)まで、どちらか早い方で打ち切る。
/// ページ数にも上限(GAP_FILL_MAX_PAGES)を設け、長期間閉じていた場合の暴走取得を防ぐ。
/// 打ち切られた(=追いつけなかった)場合は `GapFillResult::truncated` が true になる。
async fn fill_gap(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    newest_known_id: &str,
    limit: i32,
) -> Result<GapFillResult> {
    if resolved.kinds.is_empty() {
        return Ok(GapFillResult { notes: vec![], truncated: false, boundary_id: None });
    }
    let client = state.client_for(account_id)?;
    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let mut collected: Vec<Note> = Vec::new();

    // ソースごとに独立した until_id カーソルと「既知ノートに追いついた/枯渇した」フラグを持つ。
    // 複数ソースを1本の until_id で回すと、疎なソースの古い1件に引きずられて密なソース側の
    // 途中が埋まらないまま打ち切られてしまうため、ソース単位で打ち切りを判定する。
    let mut cursors: Vec<Option<String>> = vec![None; resolved.kinds.len()];
    let mut done: Vec<bool> = vec![false; resolved.kinds.len()];
    // done とは別に「newest_known_id に本当に追いついたか」を持つ。done はページ枯渇/失敗
    // でも true になるため、truncated 判定には使えない。
    let mut reached_target: Vec<bool> = vec![false; resolved.kinds.len()];

    for _ in 0..GAP_FILL_MAX_PAGES {
        if done.iter().all(|d| *d) || collected.len() as i32 >= limit {
            break;
        }
        let mut any_fetched = false;
        for (i, k) in resolved.kinds.iter().enumerate() {
            if done[i] {
                continue;
            }
            let Some((endpoint, body)) = k.rest_request(GAP_FILL_PAGE_SIZE, cursors[i].as_deref())
            else {
                done[i] = true;
                continue;
            };
            let Ok(mut page) = fetch_notes(&client, endpoint, &body).await else {
                done[i] = true;
                continue;
            };
            if page.is_empty() {
                done[i] = true;
                reached_target[i] = true;
                continue;
            }
            any_fetched = true;
            page.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
            let oldest_this_page = page.last().map(|n| n.id.clone());
            for n in page {
                if n.id.as_str() <= newest_known_id {
                    done[i] = true;
                    reached_target[i] = true;
                    continue;
                }
                if resolved.filter.matches(&n, &ctx)
                    && !crate::filter::mute::is_muted(&n, &mute)
                    && !server_muted_note(state, account_id, &n)
                    && !state.is_word_muted(account_id, &n)
                {
                    collected.push(n);
                }
            }
            cursors[i] = oldest_this_page;
        }
        if !any_fetched {
            break;
        }
    }

    let all_reached = reached_target.iter().all(|r| *r);
    Ok(finalize_gap_fill(collected, all_reached, limit))
}

/// フラッピング再接続時に同一カラムへ複数波のギャップ埋めタスクが多重起動されないよう、
/// 実行中の column_id を記録するガード。Drop で自動的に集合から取り除く（RAII）。
struct GapFillGuard {
    app: AppHandle,
    column_id: String,
}

impl Drop for GapFillGuard {
    fn drop(&mut self) {
        if let Some(state) = self.app.try_state::<AppState>() {
            state.gap_fill_in_flight.lock().unwrap().remove(&self.column_id);
        }
    }
}

impl GapFillGuard {
    /// column_id の in-flight 登録を試みる。既に実行中なら None を返す。
    fn try_acquire(app: &AppHandle, state: &AppState, column_id: &str) -> Option<Self> {
        let mut inflight = state.gap_fill_in_flight.lock().unwrap();
        if !inflight.insert(column_id.to_string()) {
            return None;
        }
        drop(inflight);
        Some(Self {
            app: app.clone(),
            column_id: column_id.to_string(),
        })
    }
}

/// Stream再接続時のノートギャップ埋め(Issue #147)。起動時(resume_column)と同じ fill_gap を
/// 使い、SQLiteキャッシュの最新ノートidを起点にRESTで遡って補完する。初回接続では呼ばれない
/// 前提（呼び出し判定は stream/connection.rs の is_reconnect 側で行う）。
pub(crate) async fn gap_fill_on_reconnect(app: &AppHandle, column_id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(_guard) = GapFillGuard::try_acquire(app, &state, column_id) else {
        // 同一カラムの前回ギャップ埋めが実行中(フラッピング再接続対策)。
        return;
    };
    let Ok(column) = load_column(&state, column_id) else {
        return;
    };
    if matches!(column.kind, ColumnKind::Notifications) {
        return;
    }
    let Ok(resolved) =
        resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await
    else {
        return;
    };
    let Ok(cached) = state.cache.load_cached(&column.id, 1).await else {
        return;
    };
    let Some(newest) = cached.first() else {
        return;
    };
    let newest_known_id = newest.id.clone();
    let gap_limit = state
        .settings
        .load_ui()
        .map(|p| p.gap_fill_limit)
        .unwrap_or(0)
        .max(0);
    if gap_limit == 0 {
        return;
    }
    let Ok(gap_result) =
        fill_gap(&state, &column.account_id, &resolved, &newest_known_id, gap_limit).await
    else {
        return;
    };
    if gap_result.notes.is_empty() {
        return;
    }
    let _ = state.cache.cache_notes(&column.id, &gap_result.notes).await;
    let _ = crate::events::ColumnGapFill {
        column_id: column.id.clone(),
        notes: gap_result.notes,
        truncated: gap_result.truncated,
        boundary_id: gap_result.boundary_id,
        target_id: if gap_result.truncated {
            Some(newest_known_id)
        } else {
            None
        },
    }
    .emit(app);
}

/// Stream再接続時の通知ギャップ埋め(Issue #147)。通知はSQLiteキャッシュを持たないため、
/// stream/connection.rs がメモリ上で保持する最終受信通知id(last_seen_id)を起点に、
/// fill_gap と同構造(until_id で遡り、既知idに追いついたら打ち切り)でREST補完する。
pub(crate) async fn notification_gap_fill_on_reconnect(
    app: &AppHandle,
    column_id: &str,
    last_seen_id: &str,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(_guard) = GapFillGuard::try_acquire(app, &state, column_id) else {
        // 同一カラムの前回ギャップ埋めが実行中(フラッピング再接続対策)。
        return;
    };
    let Ok(column) = load_column(&state, column_id) else {
        return;
    };
    let Ok(client) = state.client_for(&column.account_id) else {
        return;
    };
    let gap_limit = state
        .settings
        .load_ui()
        .map(|p| p.gap_fill_limit)
        .unwrap_or(0)
        .max(0);
    if gap_limit == 0 {
        return;
    }

    let mut collected: Vec<Notification> = Vec::new();
    let mut until_id: Option<String> = None;
    for _ in 0..GAP_FILL_MAX_PAGES {
        if collected.len() as i32 >= gap_limit {
            break;
        }
        let Ok(mut page) =
            fetch_notifications(&client, GAP_FILL_PAGE_SIZE, until_id.as_deref()).await
        else {
            break;
        };
        if page.is_empty() {
            break;
        }
        page.sort_by(|a, b| b.id.cmp(&a.id));
        let oldest_this_page = page.last().map(|n| n.id.clone());
        let mut hit_known = false;
        for n in page {
            if n.id.as_str() <= last_seen_id {
                hit_known = true;
                continue;
            }
            collected.push(n);
        }
        until_id = oldest_this_page;
        if hit_known {
            break;
        }
    }

    if collected.is_empty() {
        return;
    }
    let mut seen = std::collections::HashSet::new();
    collected.retain(|n| seen.insert(n.id.clone()));
    collected.sort_by(|a, b| b.id.cmp(&a.id));
    collected.truncate(gap_limit.max(0) as usize);

    let notifications = filter_notifications(&state, &column.account_id, collected);
    if notifications.is_empty() {
        return;
    }
    let _ = crate::events::ColumnNotificationGapFill {
        column_id: column.id.clone(),
        notifications,
    }
    .emit(app);
}

/// `fetch_and_filter_multi` の戻り値。`raw_oldest_id` は単一ソース時のみ、
/// フィルタ適用前の生APIレスポンスの最古IDを持つ（backfill境界の更新に使う。
/// フィルタ後の最古IDだと、末尾がフィルタで弾かれた場合に「実際にはもっと深く
/// APIを見ている」事実を取り逃すため）。複数ソース時はNone(境界追跡の対象外)。
struct FilteredFetch {
    notes: Vec<Note>,
    raw_oldest_id: Option<String>,
}

/// キャッシュDB検索(Issue #248)の中核ロジック。SQL射影で粗く絞り込んだ後、
/// `fetch_and_filter` の cache 経路と同じ二段構成(in-memory フィルタ + ミュート除外)で
/// 再検証する。AppState を直接取らず必要な値だけを受け取ることで単体テスト可能にしている。
// `is_server_muted`と`is_word_muted`を1つのクロージャに統合しないのは、各々を単独で
// 検証するテスト(search_cache_core_excludes_notes_the_closure_marks_server_muted /
// search_cache_core_excludes_notes_matched_by_word_mute_closure)を独立させたいため。
// AppState を取らない設計もテスト容易性のためで、引数を減らす方向のリファクタは避ける。
#[allow(clippy::too_many_arguments)]
async fn search_cache_core(
    cache: &NoteCacheStore,
    filter: &FilterQuery,
    eval_ctx: &EvalContext,
    mute: &MuteConfig,
    until_id: Option<&str>,
    limit: u32,
    is_server_muted: impl Fn(&Note) -> bool,
    is_word_muted: impl Fn(&Note) -> bool,
) -> Result<Vec<Note>> {
    let compiled = CompiledFilter::compile(filter).map_err(Error::Invalid)?;
    let sql_ctx = sql::SqlCtx {
        my_ids: eval_ctx.my_user_ids.iter().cloned().collect(),
        following_ids: None,
    };
    let where_sql = match &compiled {
        CompiledFilter::Tql(expr) => sql::build_where(expr, &sql_ctx).map_err(Error::Invalid)?,
        _ => sql::SqlWhere { sql: "1=1".into(), params: vec![] },
    };
    let raw = cache.search_cache(&where_sql, until_id, limit).await?;
    let mut filtered: Vec<Note> = raw
        .into_iter()
        .filter(|n| {
            compiled.matches(n, eval_ctx)
                && !crate::filter::mute::is_muted(n, mute)
                && !is_server_muted(n)
                && !is_word_muted(n)
        })
        .collect();
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    filtered.truncate(limit as usize);
    Ok(filtered)
}

/// 検索モーダル(Issue #248)専用: 特定カラムに紐づかない一回性のキャッシュDB検索。
/// `filter` は cache ソースの where 句のみを渡す(source節は無し、常にキャッシュ全体が対象)。
#[tauri::command]
#[specta::specta]
pub async fn search_cache_notes(
    state: State<'_, AppState>,
    account_id: String,
    filter: FilterQuery,
    until_id: Option<String>,
    limit: u32,
) -> Result<Vec<Note>> {
    let mute = state.mute.lock().unwrap().clone();
    let eval_ctx = state.eval_context();
    search_cache_core(
        &state.cache,
        &filter,
        &eval_ctx,
        &mute,
        until_id.as_deref(),
        limit,
        |n| server_muted_note(&state, &account_id, n),
        |n| state.is_word_muted(&account_id, n),
    )
    .await
}

/// 解決済みソース群から REST 初期/過去ページを取得し、id重複除去+created_at降順マージの上、
/// フィルタ/ミュートを適用する。`cache` ソースが含まれる場合はローカルSQLite検索も合成する。
/// 個別ソースの取得失敗は他ソースの結果を活かすため無視する（TQL§複数ソースは OR 合成のため）。
async fn fetch_and_filter_multi(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<FilteredFetch> {
    let mut all: Vec<Note> = Vec::new();

    if !resolved.kinds.is_empty() {
        let client = state.client_for(account_id)?;
        for k in &resolved.kinds {
            if let Some((endpoint, body)) = k.rest_request(INITIAL_LIMIT, until_id) {
                if let Ok(raw) = fetch_notes(&client, endpoint, &body).await {
                    all.extend(raw);
                }
            }
        }
    }

    // 単一ソース時のみ、フィルタ適用前の生レスポンスの最古IDを控える(backfill境界用)。
    let raw_oldest_id = if resolved.kinds.len() == 1 {
        // 境界の比較は全て id の辞書順で行うため、ここも id 基準で最古を選ぶ(Issue #228)。
        all.iter().min_by(|a, b| a.id.cmp(&b.id)).map(|n| n.id.clone())
    } else {
        None
    };

    if resolved.use_cache {
        let sql_ctx = sql::SqlCtx {
            my_ids: state.eval_context().my_user_ids.into_iter().collect(),
            following_ids: None,
        };
        let expr = match &resolved.filter {
            CompiledFilter::Tql(e) => Some(e),
            _ => None,
        };
        let where_sql = match expr {
            Some(e) => sql::build_where(e, &sql_ctx).map_err(Error::Invalid)?,
            None => sql::SqlWhere { sql: "1=1".into(), params: vec![] },
        };
        if let Ok(cached) = state.cache.search_cache(&where_sql, until_id, INITIAL_LIMIT).await {
            all.extend(cached);
        }
    }

    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let mut filtered: Vec<Note> = all
        .into_iter()
        .filter(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
                && !state.is_word_muted(account_id, n)
        })
        .collect();

    // 複数ソースに同じノートが跨る場合の重複除去 + created_at 降順ソート + limit へ切り詰め
    let mut seen = std::collections::HashSet::new();
    filtered.retain(|n| seen.insert(n.id.clone()));
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    filtered.truncate(INITIAL_LIMIT as usize);
    Ok(FilteredFetch { notes: filtered, raw_oldest_id })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{User, Visibility};

    fn note(id: &str, created_at: i64) -> Note {
        Note {
            id: id.into(),
            created_at,
            text: Some("hello".into()),
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
                emojis: std::collections::HashMap::new(),
                bio: None,
                banner_url: None,
                instance: None,
            },
            reply_id: None,
            renote_id: None,
            renote: None,
            files: vec![],
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: std::collections::HashMap::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: std::collections::HashMap::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    async fn cache_with(notes: &[Note]) -> NoteCacheStore {
        let store = NoteCacheStore::new(crate::store::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        ));
        store.cache_notes("col1", notes).await.unwrap();
        store
    }

    #[tokio::test]
    async fn search_cache_core_filters_by_tql_predicate_and_orders_desc() {
        let mut n1 = note("n1", 100);
        n1.text = Some("hello needle".into());
        let mut n2 = note("n2", 200);
        n2.text = Some("hello world".into());
        let mut n3 = note("n3", 300);
        n3.text = Some("needle again".into());
        let cache = cache_with(&[n1, n2, n3]).await;

        let filter = FilterQuery::Tql("text -> \"needle\"".into());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |_| false,
            |_| false,
        )
        .await
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n3", "n1"]);
    }

    #[tokio::test]
    async fn search_cache_core_with_empty_predicate_returns_all_desc_order() {
        let cache = cache_with(&[note("n1", 100), note("n2", 300), note("n3", 200)]).await;

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |_| false,
            |_| false,
        )
        .await
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n3", "n1"]);
    }

    #[tokio::test]
    async fn search_cache_core_excludes_locally_muted_notes() {
        let mut n1 = note("n1", 100);
        n1.text = Some("spoiler content".into());
        let cache = cache_with(&[n1, note("n2", 200)]).await;

        let filter = FilterQuery::Tql(String::new());
        let mute = MuteConfig { ng_words: vec!["spoiler".into()], ..Default::default() };
        let got =
            search_cache_core(&cache, &filter, &EvalContext::default(), &mute, None, 10, |_| false, |_| false)
                .await
                .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);
    }

    #[tokio::test]
    async fn search_cache_core_excludes_notes_the_closure_marks_server_muted() {
        let cache = cache_with(&[note("n1", 100), note("n2", 200)]).await;

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |n| n.id == "n2",
            |_| false,
        )
        .await
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"]);
    }

    #[tokio::test]
    async fn search_cache_core_excludes_notes_matched_by_word_mute_closure() {
        let cache = cache_with(&[note("n1", 100), note("n2", 200)]).await;

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |_| false,
            |n| n.id == "n2",
        )
        .await
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"]);
    }

    #[tokio::test]
    async fn search_cache_core_respects_until_id_boundary() {
        let cache = cache_with(&[note("n1", 100), note("n2", 200), note("n3", 300)]).await;

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            Some("n3"),
            10,
            |_| false,
            |_| false,
        )
        .await
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n1"]);
    }

    #[test]
    fn finalize_gap_fill_marks_truncated_when_target_not_reached() {
        let collected = vec![note("n3", 30), note("n2", 20), note("n1", 10)];
        let result = finalize_gap_fill(collected, false, 100);

        assert!(result.truncated);
        assert_eq!(result.boundary_id.as_deref(), Some("n1"));
        assert_eq!(result.notes.len(), 3);
    }

    #[test]
    fn finalize_gap_fill_not_truncated_when_all_sources_reached_target() {
        let collected = vec![note("n2", 20), note("n1", 10)];
        let result = finalize_gap_fill(collected, true, 100);

        assert!(!result.truncated);
        assert_eq!(result.boundary_id, None);
        assert_eq!(result.notes.len(), 2);
    }

    #[test]
    fn finalize_gap_fill_truncates_to_limit_and_reports_oldest_kept_as_boundary() {
        let collected = vec![note("n3", 30), note("n2", 20), note("n1", 10)];
        let result = finalize_gap_fill(collected, false, 2);

        assert_eq!(result.notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n3", "n2"]);
        assert!(result.truncated);
        assert_eq!(result.boundary_id.as_deref(), Some("n2"));
    }

    #[test]
    fn finalize_gap_fill_not_truncated_when_nothing_collected() {
        let result = finalize_gap_fill(vec![], false, 100);

        assert!(!result.truncated);
        assert_eq!(result.boundary_id, None);
        assert!(result.notes.is_empty());
    }

    #[test]
    fn finalize_gap_fill_dedupes_by_id() {
        let collected = vec![note("n1", 10), note("n1", 10)];
        let result = finalize_gap_fill(collected, true, 100);

        assert_eq!(result.notes.len(), 1);
    }

    #[test]
    fn cache_backfill_page_none_when_boundary_unknown() {
        let cached = vec![note("n1", 10); 0]; // 空でも境界未確定なら常にAPIへ
        let result = cache_backfill_page(None, "n999", cached, 20);
        assert!(result.is_none());
    }

    #[test]
    fn cache_backfill_page_none_when_until_id_at_or_before_boundary() {
        let cached = vec![note("n1", 10)];
        // until_id が境界と同じ、または境界より古い場合は「未検証の領域」を含みうるのでAPIへ
        assert!(cache_backfill_page(Some("n500"), "n500", cached.clone(), 1).is_none());
        assert!(cache_backfill_page(Some("n500"), "n400", cached, 1).is_none());
    }

    #[test]
    fn cache_backfill_page_none_when_cached_count_below_limit() {
        let cached = vec![note("n1", 10), note("n2", 20)];
        let result = cache_backfill_page(Some("n001"), "n999", cached, 20);
        assert!(result.is_none());
    }

    #[test]
    fn cache_backfill_page_some_when_within_boundary_and_enough_notes() {
        let cached = vec![note("n2", 20), note("n1", 10)];
        let result = cache_backfill_page(Some("n001"), "n999", cached.clone(), 2);
        assert_eq!(
            result.unwrap().iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            cached.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
        );
    }
}
