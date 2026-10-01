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
use crate::state::{AppState, BackfillOutcome};
use crate::store::NoteCacheStore;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_specta::Event as _;

const INITIAL_LIMIT: u32 = 20;
const DEFAULT_WIDTH: i32 = 300;
const GAP_FILL_PAGE_SIZE: u32 = 100;
const GAP_FILL_MAX_PAGES: u32 = 10;
/// `collect_backfill_pages` の最大パス数(Issue #428)。画面へ返すノートが0件のときの内部再取得の上限。
const BACKFILL_MAX_PASSES: u32 = 5;

/// タブを開いた結果。所属グループも返す（新規グループの幅などをフロントへ）。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedColumn {
    pub column: Column,
    pub group: ColumnGroup,
    pub notes: Vec<Note>,
    pub notifications: Vec<Notification>,
}

/// キャッシュhit/fallback回数のスナップショット(Issue #241)。BackstageのメトリクスUI用。
/// フィールドを増やせば他の指標(WS再接続回数など)も同じ場所に追加できる想定の汎用DTO。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DebugMetrics {
    pub backfill_cache_hit: i32,
    pub backfill_cache_fallback_boundary: i32,
    pub backfill_cache_fallback_other: i32,
    pub resume_cache_hit: i32,
    pub resume_cache_fallback: i32,
}

/// デバッグ用メトリクスのスナップショットを返す。Backstageの「メトリクス」タブがポーリングする。
#[tauri::command]
#[specta::specta]
pub async fn get_debug_metrics(state: State<'_, AppState>) -> Result<DebugMetrics> {
    Ok(DebugMetrics {
        backfill_cache_hit: state.cache_metrics.backfill_hit(),
        backfill_cache_fallback_boundary: state.cache_metrics.backfill_fallback_boundary(),
        backfill_cache_fallback_other: state.cache_metrics.backfill_fallback_other(),
        resume_cache_hit: state.cache_metrics.resume_hit(),
        resume_cache_fallback: state.cache_metrics.resume_fallback(),
    })
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
        // ギャップ埋めが打ち切られて境界が引き上げられていると、境界より古い側には穴がありうる。
        // 復元一覧が穴をまたがないよう、有効境界以上に絞る。結果が空なら下の「キャッシュが空」
        // 経路(REST初回取得)に入る(Issue #432)。
        let restore_e = match resolved.as_ref() {
            Some(r) => restore_boundary(&state, &column.id, r).await,
            None => None,
        };
        let notes = restrict_to_boundary(cached, restore_e.as_deref());
        state.cache_metrics.record_resume(!notes.is_empty());
        notes
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
                    .unwrap_or(GapFillResult {
                        notes: vec![],
                        truncated: false,
                        boundary_id: None,
                        sources: vec![],
                        all_reached: false,
                        dropped_floor: None,
                    });
                // 打ち切られた(=穴が残りうる)なら境界を引き上げる。収集が0件でも未取得の範囲は
                // 残るので、空判定の前に行う(Issue #432)。
                apply_gap_fill_boundaries(&state.cache, &column_id, &gap_result).await;
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

/// 過去ページ（上スクロール）。`from cache` を含まず、かつ全ソースがストリーミング対応の
/// カラムは、要求範囲が全ソースのbackfill境界(`max(b_i)`)より新しければキャッシュのみで応答する
/// (Issue #228 / #238)。User/Tag/Search などストリーミングを持たないソースを含むカラムは、
/// ライブノートが column_note に入らず境界より新しい範囲の完全性を言えないため常にAPIへ。
/// いずれかのソースの境界が未確定・範囲外・件数不足なら通常どおりAPIへ。
/// `bypass_cache=true`(fillRemainingGap)のときはキャッシュ読み出しを行わず常にAPIへ行く。
/// ギャップ区間 `(targetId, boundaryId)` は未取得のため、キャッシュHitで埋めた気になってはならない(Issue #427)。
/// 境界の延長はバイパス時も従来どおり行う(`plan_boundary_extend` が連続性を検証する)。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
    bypass_cache: bool,
) -> Result<Vec<Note>> {
    let column = load_column(&state, &column_id)?;
    let resolved = resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?;

    let cache_eligible = backfill_cache_eligible(&resolved);
    let boundaries: std::collections::HashMap<u32, String> = if cache_eligible {
        state
            .cache
            .get_fetch_boundaries(&column.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect()
    } else {
        std::collections::HashMap::new()
    };
    if should_try_backfill_cache(cache_eligible, bypass_cache) {
        let effective = effective_boundary(&boundaries, resolved.kinds.len());
        let mut cached = match &effective {
            Some(e) if until_id.as_str() > e.as_str() => state
                .cache
                .load_cached_before(&column.id, &until_id, INITIAL_LIMIT)
                .await
                .unwrap_or_default(),
            _ => vec![],
        };
        // [E, until_id) の範囲外(=このセッションでは未検証)の行を除外する。
        // load_cached_before 自体は下限を持たないため、範囲内の件数が不足していても
        // セッションをまたいだ古いキャッシュ行で limit を満たしてしまう可能性がある。
        if let Some(e) = &effective {
            cached.retain(|n| n.id.as_str() >= e.as_str());
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
        if let Some(notes) = cache_backfill_page(effective.as_deref(), &until_id, cached, INITIAL_LIMIT) {
            state.cache_metrics.record_backfill(BackfillOutcome::Hit);
            return Ok(notes);
        }
        state.cache_metrics.record_backfill(if effective.is_none() {
            BackfillOutcome::FallbackBoundaryUnset
        } else {
            BackfillOutcome::FallbackOther
        });
    }

    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    cache_fetched(&state.cache, &column.id, &fetch).await?;
    if cache_eligible {
        // ソースごとに、既存の境界と連続している場合のみ延長する(plan_boundary_extend)。
        // 境界未確定のソースは連続性を検証できないので延長せず、カラム開き直し時の
        // open_stream_and_fetch が改めて確定させる。
        let entries = plan_boundary_extend(&boundaries, &until_id, &fetch.source_outcomes);
        if !entries.is_empty() {
            let _ = state.cache.extend_fetch_boundaries(&column.id, &entries).await;
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
    cache_fetched(&state.cache, &column.id, &fetch).await?;
    if backfill_cache_eligible(&resolved) {
        let entries = plan_boundary_initial(&fetch.source_outcomes);
        let _ = state.cache.replace_fetch_boundaries(&column.id, &entries).await;
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

/// `fill_gap` の1ソース分の到達状況。打ち切られたギャップ埋めの後に、境界をどこまで
/// 引き上げるかの計算に使う(Issue #432)。
#[derive(Debug, Clone, PartialEq, Eq)]
struct GapSourceState {
    /// このソースが遡って取得できた最古の生ページのID(ページ内の最小id)。1ページも取れなければ None。
    oldest_fetched: Option<String>,
    /// newest_known_id に追いついた(またはソースが枯渇した)か。
    reached_target: bool,
}

/// `fill_gap` の結果。`newest_known_id` に追いつけたか(=打ち切りが起きたか)を
/// 呼び出し元(resume_column/gap_fill_on_reconnect)がフロントへ伝えるために持つ。
struct GapFillResult {
    notes: Vec<Note>,
    /// newest_known_id に追いつく前に limit/ページ数上限で打ち切られた場合、または追いついたが
    /// limit 超過で古い側を切り捨てた場合 true(どちらも間にギャップが残る)。
    truncated: bool,
    /// truncated=true のとき、取得できた中で一番古いノートのid。
    /// フロントが「続きを取得」で fetch_backfill の until_id に使う。
    boundary_id: Option<String>,
    /// `resolved.kinds` と同じ並びの、ソースごとの到達状況。
    sources: Vec<GapSourceState>,
    /// 全ソースが newest_known_id に追いついたか。偽なら穴が残りうる(収集が0件でも偽になりうる)。
    all_reached: bool,
    /// `limit` で切り捨てたノートの id の最大値。切り捨てが無ければ None。
    /// 全ソースが追いついていても、内側ループは1周で各ソース1ページずつ足すため `limit` を超えうる。
    /// 切り捨てた範囲はキャッシュに入らないので、これより新しい側だけが「完全」と言える(Issue #432)。
    dropped_floor: Option<String>,
}

/// 収集済みノートを重複除去・整形し、`newest_known_id` に追いつけたかを判定する。
/// ネットワークを伴わない純粋関数にして単体テストしやすくしている。
fn finalize_gap_fill(mut collected: Vec<Note>, all_sources_reached_target: bool, limit: i32) -> GapFillResult {
    let mut seen = std::collections::HashSet::new();
    collected.retain(|n| seen.insert(n.id.clone()));
    collected.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    let keep = limit.max(0) as usize;
    // 切り詰めは created_at 順、境界は id 順なので、捨てたノートの id が残した最古より大きい
    // こともありうる。境界の下限には「捨てた id の最大値」を使う(Issue #432)。
    let dropped_floor = collected.iter().skip(keep).map(|n| n.id.as_str()).max().map(str::to_string);
    collected.truncate(keep);
    // 全ソースが追いついても、limit 超過で切り捨てがあれば newest_known 直上に穴ができる。
    // フロントにギャップマーカーを出させて、ユーザーが埋められるようにする。
    let truncated = (!all_sources_reached_target || dropped_floor.is_some()) && !collected.is_empty();
    let boundary_id = if truncated { collected.last().map(|n| n.id.clone()) } else { None };
    GapFillResult {
        notes: collected,
        truncated,
        boundary_id,
        sources: vec![],
        all_reached: all_sources_reached_target,
        dropped_floor,
    }
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
        return Ok(GapFillResult {
            notes: vec![],
            truncated: false,
            boundary_id: None,
            sources: vec![],
            all_reached: true,
            dropped_floor: None,
        });
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
    // ソースごとに遡れた最古の生ページIDを「ページ内の最小id」で持つ(Issue #432)。境界の比較が
    // id の辞書順のため、API用の cursors(created_at降順の最後)とは別に持つ。
    let mut oldest_fetched: Vec<Option<String>> = vec![None; resolved.kinds.len()];

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
            if let Some(m) = page.iter().map(|n| n.id.as_str()).min() {
                if oldest_fetched[i].as_deref().is_none_or(|o| m < o) {
                    oldest_fetched[i] = Some(m.to_string());
                }
            }
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
    let mut result = finalize_gap_fill(collected, all_reached, limit);
    result.sources = oldest_fetched
        .into_iter()
        .zip(reached_target)
        .map(|(oldest_fetched, reached_target)| GapSourceState { oldest_fetched, reached_target })
        .collect();
    Ok(result)
}

/// フラッピング再接続時に同一カラムへ複数波のギャップ埋めタスクが多重起動されないよう、
/// 実行中の column_id を記録するガード。Drop で自動的に集合から取り除く（RAII）。
struct GapFillGuard<R: Runtime> {
    app: AppHandle<R>,
    column_id: String,
}

impl<R: Runtime> Drop for GapFillGuard<R> {
    fn drop(&mut self) {
        if let Some(state) = self.app.try_state::<AppState>() {
            state.gap_fill_in_flight.lock().unwrap().remove(&self.column_id);
        }
    }
}

impl<R: Runtime> GapFillGuard<R> {
    /// column_id の in-flight 登録を試みる。既に実行中なら None を返す。
    fn try_acquire(app: &AppHandle<R>, state: &AppState, column_id: &str) -> Option<Self> {
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
pub(crate) async fn gap_fill_on_reconnect<R: Runtime>(app: &AppHandle<R>, column_id: &str) {
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
    // 打ち切られた(=穴が残りうる)なら境界を引き上げる。収集が0件でも未取得の範囲は
    // 残るので、空判定の前に行う(Issue #432)。
    apply_gap_fill_boundaries(&state.cache, &column.id, &gap_result).await;
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
pub(crate) async fn notification_gap_fill_on_reconnect<R: Runtime>(
    app: &AppHandle<R>,
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

/// 1ソースのREST取得結果。backfill境界の更新内容を決めるのに使う(Issue #238)。
#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceOutcome {
    /// 取得成功。生APIレスポンスの最古ID(id辞書順の最小)。フィルタ適用前の値で、
    /// フィルタで末尾が弾かれても「実際にはもっと深くAPIを見ている」事実を取り逃さない。
    Fetched(String),
    /// 取得成功だが0件(それより古いノートは無い)。境界は `""`(枯渇済み)にする。
    Exhausted,
    /// 取得失敗、またはREST取得できない種別。境界は更新しない。
    Failed,
}

/// 複数パスにまたがる1ソースの取得結果を統合する(Issue #428)。ソースごとの結果は、
/// 最初のパスから連続して取得できた範囲を表す必要がある(backfill境界の前提)。
/// - 最初のパスで失敗(`Failed`)したソースは、その後取得できても先頭から連続していないので `Failed`。
/// - `Exhausted` は以降も `Exhausted`。
/// - 取得成功(`Fetched`)同士はより古い(小さい)idへ。後続で `Exhausted` なら `Exhausted`。
/// - 後続パスの失敗は、それまでに取れた範囲を保つ。ただし1ソースずつ2つの結果を比べるだけでは、
///   その後に取得が復帰したとき間の欠落を連続とみなしてしまう。`collect_backfill_pages` は
///   一度 `Fetched` になった後に `Failed` になったソースを凍結し、以降のパス結果を渡さない。
fn merge_source_outcome(prev: SourceOutcome, next: SourceOutcome) -> SourceOutcome {
    use SourceOutcome::{Exhausted, Failed, Fetched};
    match (prev, next) {
        (Failed, _) => Failed,
        (Exhausted, _) => Exhausted,
        (Fetched(_), Exhausted) => Exhausted,
        (Fetched(a), Fetched(b)) => Fetched(a.min(b)),
        (Fetched(a), Failed) => Fetched(a),
    }
}

/// 1パス分の取得結果から、画面へ返してよい最古のid(透かし)を決める(Issue #428)。
/// 各ソースの生ページの最古idのうち最大(=最も浅いソースの深さ)。`Exhausted`/`Failed` の
/// ソースは制約しない。`cache_min`(`from cache` の検索結果が `INITIAL_LIMIT` 件に達したときの
/// その最小id)も1ソースとして含める。制約するソースが無ければ None(全ノートを返してよい)。
///
/// 画面へ返すノートを透かし以上に限ると、フロントが次の `until_id` に使う「表示中の最古」は
/// 透かし以上になる。次回は全ソースが `until_id` から取り直すので、浅いソースの区間が飛ばされない。
fn backfill_watermark(outcomes: &[SourceOutcome], cache_min: Option<&str>) -> Option<String> {
    outcomes
        .iter()
        .filter_map(|o| match o {
            SourceOutcome::Fetched(id) => Some(id.as_str()),
            _ => None,
        })
        .chain(cache_min)
        .max()
        .map(str::to_string)
}

/// `fetch_and_filter_multi` の戻り値。
struct FilteredFetch {
    /// 画面へ返す分(重複除去・created_at降順・INITIAL_LIMIT件へtruncate済み)。
    notes: Vec<Note>,
    /// キャッシュする分。境界は「ソースごとのRESTページ全体が column_note に入っている」ことを
    /// 前提に進めるため、複数ソースではtruncate前の重複除去済みフィルタ通過分を全件入れる。
    /// `from cache` を含むカラムは境界の対象外なので従来どおりtruncate後(`notes`と同内容)。
    cacheable: Vec<Note>,
    /// `resolved.kinds` と同じ並びの、ソースごとの取得結果。
    source_outcomes: Vec<SourceOutcome>,
}

/// `collect_backfill_pages` が1パス分の取得として受け取る結果(Issue #428)。
struct BackfillPass {
    /// フィルタ/ミュート適用済みのノート(重複除去前)。
    notes: Vec<Note>,
    /// `resolved.kinds` と同じ並びの、ソースごとの取得結果。
    outcomes: Vec<SourceOutcome>,
    /// `from cache` の検索結果が `INITIAL_LIMIT` 件に達したときのその最小id。
    cache_min: Option<String>,
}

/// 1パス分の取得を `fetch_pass` で繰り返し、透かし以上のノートだけを画面用として集める(Issue #428)。
///
/// 各パスで `backfill_watermark` を計算し、`split_display_and_cacheable` に渡す。画面用が
/// `INITIAL_LIMIT` 件に満たず透かしがあれば(=まだ深く取れるソースがある)、`until_id = 透かし` で
/// 取り直し、各パスの画面用を積み上げる。これが無いと、
/// - 画面用が0件のとき、フロントが同じ `until_id` を繰り返してそのカラムが遡れなくなる。
/// - 画面用が数件のとき、カラムがスクロールできず(`loadMore` は scroll イベントでだけ呼ばれる)
///   それ以上遡れなくなる。
/// 後続パスの画面用は前のパスの透かしより古い範囲に収まるので、積み上げた全体は最後の透かし以上に
/// 収まり、次の `until_id` も最後の透かし以上になる。
/// `BACKFILL_MAX_PASSES` に達しても0件なら、スクロールが止まらないよう最後の1パスだけ透かしなしで返す。
/// `cacheable` は全パスの和集合、`source_outcomes` はパス間で統合する。一度 `Fetched` になった後に
/// `Failed` になったソースは、以降のパスの結果を無視する(間の区間が未取得なので、後のパスの結果と
/// 連続しているとは言えない)。
async fn collect_backfill_pages<F, Fut>(
    until_id: Option<&str>,
    use_cache: bool,
    mut fetch_pass: F,
) -> Result<FilteredFetch>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<BackfillPass>>,
{
    let mut until = until_id.map(str::to_string);
    let mut all_cacheable: Vec<Note> = Vec::new();
    let mut display_acc: Vec<Note> = Vec::new();
    let mut merged_outcomes: Vec<SourceOutcome> = Vec::new();
    let mut frozen: Vec<bool> = Vec::new();
    let mut pass_no: u32 = 0;
    loop {
        pass_no += 1;
        let pass = fetch_pass(until.clone()).await?;
        if pass_no == 1 {
            merged_outcomes = pass.outcomes.clone();
            frozen = vec![false; pass.outcomes.len()];
        } else {
            for (i, next) in pass.outcomes.iter().enumerate() {
                let Some(prev) = merged_outcomes.get(i).cloned() else { break };
                if frozen[i] {
                    continue;
                }
                if matches!((&prev, next), (SourceOutcome::Fetched(_), SourceOutcome::Failed)) {
                    frozen[i] = true;
                    continue;
                }
                merged_outcomes[i] = merge_source_outcome(prev, next.clone());
            }
        }
        let watermark = backfill_watermark(&pass.outcomes, pass.cache_min.as_deref());
        let is_last = pass_no >= BACKFILL_MAX_PASSES;
        let fallback_notes = if is_last { Some(pass.notes.clone()) } else { None };
        let (mut display, mut cacheable) = split_display_and_cacheable(pass.notes, use_cache, watermark.as_deref());
        if display_acc.is_empty() && display.is_empty() && watermark.is_some() && is_last {
            if let Some(notes) = fallback_notes {
                (display, cacheable) = split_display_and_cacheable(notes, use_cache, None);
            }
        }
        all_cacheable.extend(cacheable);
        display_acc.extend(display);
        if display_acc.len() as u32 >= INITIAL_LIMIT || watermark.is_none() || is_last {
            break;
        }
        until = watermark;
    }

    // 全パスの和集合(id 重複除去、created_at 降順)。1パスで終わった場合は split の結果と同じ。
    let order = |a: &Note, b: &Note| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id));
    let mut seen = std::collections::HashSet::new();
    display_acc.retain(|n| seen.insert(n.id.clone()));
    display_acc.sort_by(order);
    display_acc.truncate(INITIAL_LIMIT as usize);
    let mut seen = std::collections::HashSet::new();
    all_cacheable.retain(|n| seen.insert(n.id.clone()));
    all_cacheable.sort_by(order);
    // `from cache` を含むカラムは境界の対象外なので、cacheable は画面用と同じ。
    let cacheable = if use_cache { display_acc.clone() } else { all_cacheable };
    Ok(FilteredFetch { notes: display_acc, cacheable, source_outcomes: merged_outcomes })
}

/// 取得結果をキャッシュへ書く。書くのは画面用の `notes`(truncate後)ではなく `cacheable`。
/// backfill境界は「ソースごとのRESTページ全体が column_note に入っている」ことを前提に
/// 進めるため、`notes` を書くと境界が完全と主張する範囲に欠落が出る。`fetch_backfill` と
/// `open_stream_and_fetch` の両方がこれを経由することで、書き込み対象を1か所に固定する。
async fn cache_fetched(cache: &NoteCacheStore, column_id: &str, fetch: &FilteredFetch) -> Result<()> {
    cache.cache_notes(column_id, &fetch.cacheable).await
}

/// 1ページ分の生レスポンスから `SourceOutcome` を決める。
fn source_outcome_from_page(raw: &[Note]) -> SourceOutcome {
    // 境界の比較は全て id の辞書順で行うため、ここも id 基準で最古を選ぶ(Issue #228)。
    match raw.iter().map(|n| n.id.as_str()).min() {
        Some(id) => SourceOutcome::Fetched(id.to_string()),
        None => SourceOutcome::Exhausted,
    }
}

/// 初回取得(`until_id` 無し)で境界へ書く内容。失敗したソースは行を作らない(未確定のまま)。
fn plan_boundary_initial(outcomes: &[SourceOutcome]) -> Vec<(u32, String)> {
    outcomes
        .iter()
        .enumerate()
        .filter_map(|(i, o)| match o {
            SourceOutcome::Fetched(id) => Some((i as u32, id.clone())),
            SourceOutcome::Exhausted => Some((i as u32, String::new())),
            SourceOutcome::Failed => None,
        })
        .collect()
}

/// `fetch_backfill` のAPI取得後に境界へ延長する内容。ソースごとに、既存の境界があり
/// `until_id >= b_i`(=今回の取得範囲が既存の完全範囲と連続)なときだけ延長する。
/// 不連続なソース(例: fillRemainingGap が gap marker の targetId まで遡った場合)や
/// 境界未確定のソースは、間の未検証の隙間を「完全」と誤認しないよう更新しない。
fn plan_boundary_extend(
    prev: &std::collections::HashMap<u32, String>,
    until_id: &str,
    outcomes: &[SourceOutcome],
) -> Vec<(u32, String)> {
    plan_boundary_initial(outcomes)
        .into_iter()
        .filter(|(i, _)| prev.get(i).is_some_and(|b| until_id >= b.as_str()))
        .collect()
}

/// 全ソースの境界が揃っているときだけ、カラム全体で完全な範囲 `id > E` の `E = max(b_i)` を返す。
/// 1ソースでも境界が無ければ None(未確定)。`""`(枯渇済み)は他ソースの境界より小さいので塞がない。
fn effective_boundary(boundaries: &std::collections::HashMap<u32, String>, source_count: usize) -> Option<String> {
    if source_count == 0 {
        return None;
    }
    let mut max: Option<&String> = None;
    for i in 0..source_count as u32 {
        let b = boundaries.get(&i)?;
        max = Some(match max {
            Some(m) if m >= b => m,
            _ => b,
        });
    }
    max.cloned()
}

/// backfill のキャッシュ優先経路の対象か。次のカラムは対象外(常にAPI経由)。
/// - `from cache` を含むカラム: `search_cache` がグローバルな note テーブルを読み、
///   `column_note` だけでは API 経路と同じ結果を再現できない。
/// - ストリーミングを持たないソース(User / Tag / Search 等。`stream_request()` が None)を含むカラム:
///   これらはライブノートが `column_note` に入らないため、境界 E より新しい範囲が完全だとは言えず、
///   キャッシュだけで返すとそのソースの新着ノートが欠落する。
fn backfill_cache_eligible(resolved: &ResolvedSources) -> bool {
    !resolved.use_cache
        && !resolved.kinds.is_empty()
        && resolved.kinds.iter().all(|k| k.stream_request().is_some())
}

/// 打ち切られたギャップ埋めの後に書き戻す境界の一覧(`replace_fetch_boundaries` に渡す全行)。
/// 変更が無ければ None。`prev` は既存の境界(ソース位置 -> id)。
///
/// 全ソースが追いつき(`all_reached`)、かつ切り捨ても無い(`dropped_floor` が None)なら穴は
/// 無いので None。そうでなければ `prev` の各行を、
/// - 追いついたソース: `dropped_floor` があれば `max(b, dropped_floor)`、無ければそのまま。
/// - 追いついていないソースで `oldest_fetched` が `Some(o)`: `max(b, o, dropped_floor)`。
/// - それ以外(1ページも取れていない、ソース情報が無い): 行を落として未確定にする。
///
/// `dropped_floor`(`finalize_gap_fill` が `limit` で切り捨てたノートの id の最大値)を全ソースに
/// 適用するのは、切り詰めが全ソース合算で行われ、追いついたソースのノートも捨てられうるため。
/// 行はソースごとの「id > b は揃っている」を表し、後続の延長もソースごとに動くので、
/// 追いついたソースの行を古いまま残すと意味が壊れる。
/// 未到達ソースが `o` まで遡っていても、切り捨てた範囲は `column_note` に無いので `dropped_floor`
/// も下限に含める。
///
/// `prev` に無いソースは行を作らない。
fn plan_boundary_raise_after_gap(
    prev: &std::collections::HashMap<u32, String>,
    sources: &[GapSourceState],
    all_reached: bool,
    dropped_floor: Option<&str>,
) -> Option<Vec<(u32, String)>> {
    if (all_reached && dropped_floor.is_none()) || prev.is_empty() {
        return None;
    }
    let raise = |b: &str, extra: Option<&str>| -> String {
        let mut v = b;
        for c in [extra, dropped_floor].into_iter().flatten() {
            v = v.max(c);
        }
        v.to_string()
    };
    let mut next: Vec<(u32, String)> = prev
        .iter()
        .filter_map(|(i, b)| match sources.get(*i as usize) {
            Some(s) if s.reached_target => Some((*i, raise(b, None))),
            Some(GapSourceState { oldest_fetched: Some(o), .. }) => Some((*i, raise(b, Some(o)))),
            _ => None,
        })
        .collect();
    next.sort();
    let mut before: Vec<(u32, String)> = prev.iter().map(|(i, b)| (*i, b.clone())).collect();
    before.sort();
    if next == before {
        None
    } else {
        Some(next)
    }
}

/// 再起動時の復元一覧を、有効境界 `effective` 以上のノートに絞る(Issue #432)。
/// 境界より古い側にはギャップの穴がありうるため、復元一覧が穴をまたがないようにする。
/// 境界未確定(None)のカラムは絞らない。
fn restrict_to_boundary(mut cached: Vec<Note>, effective: Option<&str>) -> Vec<Note> {
    if let Some(e) = effective {
        cached.retain(|n| n.id.as_str() >= e);
    }
    cached
}

/// 打ち切られたギャップ埋めの結果に応じて、保存済みの境界を引き上げる(Issue #432)。
/// 境界が無い(cache_eligible でない)カラムでは何もしない。DBエラーは握りつぶす
/// (更新できなくても従来の挙動に戻るだけで、他の境界書き込みと同じ扱い)。
async fn apply_gap_fill_boundaries(cache: &NoteCacheStore, column_id: &str, gap: &GapFillResult) {
    let Ok(prev) = cache.get_fetch_boundaries(column_id).await else {
        return;
    };
    let prev: std::collections::HashMap<u32, String> = prev.into_iter().collect();
    if let Some(entries) =
        plan_boundary_raise_after_gap(&prev, &gap.sources, gap.all_reached, gap.dropped_floor.as_deref())
    {
        let _ = cache.replace_fetch_boundaries(column_id, &entries).await;
    }
}

/// `resume_column` の復元一覧を絞る有効境界。キャッシュ優先の対象でないカラムや、
/// 境界が未確定のカラムは None(絞らない)。
async fn restore_boundary(state: &AppState, column_id: &str, resolved: &ResolvedSources) -> Option<String> {
    if !backfill_cache_eligible(resolved) {
        return None;
    }
    let boundaries: std::collections::HashMap<u32, String> = state
        .cache
        .get_fetch_boundaries(column_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();
    effective_boundary(&boundaries, resolved.kinds.len())
}

/// backfill でキャッシュ読み出し(`load_cached_before` 〜 Hit判定)に入るか。
/// `bypass_cache`(fillRemainingGap=ギャップ埋め)のときは、`until_id` より新しい未取得区間を
/// キャッシュが覆っていると言えないため、eligible でも必ずAPIへ行く(Issue #427)。
fn should_try_backfill_cache(cache_eligible: bool, bypass_cache: bool) -> bool {
    cache_eligible && !bypass_cache
}

/// 重複除去・created_at降順ソート済みのフィルタ通過ノートから、画面へ返す分と
/// キャッシュする分を決める。詳細は `FilteredFetch::cacheable` を参照。
/// `watermark`(Some)のときは、画面へ返す分を `id >= watermark` のノートだけに限る
/// (`backfill_watermark`)。`cacheable` には透かしより古いノートも入る(Issue #428)。
fn split_display_and_cacheable(
    mut filtered: Vec<Note>,
    use_cache: bool,
    watermark: Option<&str>,
) -> (Vec<Note>, Vec<Note>) {
    // 複数ソースに同じノートが跨る場合の重複除去 + created_at 降順ソート
    let mut seen = std::collections::HashSet::new();
    filtered.retain(|n| seen.insert(n.id.clone()));
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    let display: Vec<Note> = filtered
        .iter()
        .filter(|n| watermark.is_none_or(|w| n.id.as_str() >= w))
        .take(INITIAL_LIMIT as usize)
        .cloned()
        .collect();
    let cacheable = if use_cache { display.clone() } else { filtered };
    (display, cacheable)
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
/// ただし失敗は `source_outcomes` に `Failed` として残し、backfill境界を進めないようにする。
/// 画面へ返すのは、各ソースの生ページの最古idのうち最大(透かし)以上のノートだけで、必要なら
/// 内部で取り直す(`collect_backfill_pages`, Issue #428)。
async fn fetch_and_filter_multi(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<FilteredFetch> {
    collect_backfill_pages(until_id, resolved.use_cache, |until| async move {
        fetch_backfill_pass(state, account_id, resolved, until.as_deref()).await
    })
    .await
}

/// `fetch_and_filter_multi` の1パス分の取得(Issue #428)。各ソースから `until_id` より古い
/// 生ページを取得し、`cache` ソースがあればローカル検索も合成して、フィルタ/ミュートを適用する。
async fn fetch_backfill_pass(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<BackfillPass> {
    let mut all: Vec<Note> = Vec::new();
    let mut outcomes: Vec<SourceOutcome> = Vec::with_capacity(resolved.kinds.len());

    if !resolved.kinds.is_empty() {
        let client = state.client_for(account_id)?;
        for k in &resolved.kinds {
            let outcome = match k.rest_request(INITIAL_LIMIT, until_id) {
                None => SourceOutcome::Failed,
                Some((endpoint, body)) => match fetch_notes(&client, endpoint, &body).await {
                    Ok(raw) => {
                        let outcome = source_outcome_from_page(&raw);
                        all.extend(raw);
                        outcome
                    }
                    Err(_) => SourceOutcome::Failed,
                },
            };
            outcomes.push(outcome);
        }
    }

    let mut cache_min: Option<String> = None;
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
            // 検索結果が上限に達したときだけ、その最小idを透かしの候補にする(未満なら枯渇扱い)。
            if cached.len() as u32 >= INITIAL_LIMIT {
                cache_min = cached.iter().map(|n| n.id.clone()).min();
            }
            all.extend(cached);
        }
    }

    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let notes: Vec<Note> = all
        .into_iter()
        .filter(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
                && !state.is_word_muted(account_id, n)
        })
        .collect();

    Ok(BackfillPass { notes, outcomes, cache_min })
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
                avatar_blurhash: None,
                instance: None,
            },
            reply_id: None,
            reply_user_id: None,
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
    fn finalize_gap_fill_marks_truncated_when_notes_were_dropped_even_if_all_sources_reached_target() {
        // 全ソースが追いついても、limit 超過分の切り捨てで newest_known 直上に穴ができる。
        // マーカーを出して、ユーザーが埋められるようにする。基準は残した最古のノート。
        let collected = vec![note("n3", 30), note("n2", 20), note("n1", 10)];
        let result = finalize_gap_fill(collected, true, 2);

        assert!(result.truncated);
        assert_eq!(result.boundary_id.as_deref(), Some("n2"));
        assert_eq!(result.notes.len(), 2);
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

    fn fetched(id: &str) -> SourceOutcome {
        SourceOutcome::Fetched(id.to_string())
    }

    fn bmap(entries: &[(u32, &str)]) -> std::collections::HashMap<u32, String> {
        entries.iter().map(|(i, b)| (*i, b.to_string())).collect()
    }

    #[test]
    fn source_outcome_from_page_uses_min_id_and_flags_empty_as_exhausted() {
        // 最古は created_at ではなく id の辞書順の最小(境界比較が id 基準のため)
        let raw = vec![note("n3", 10), note("n1", 30), note("n2", 20)];
        assert_eq!(source_outcome_from_page(&raw), fetched("n1"));
        assert_eq!(source_outcome_from_page(&[]), SourceOutcome::Exhausted);
    }

    #[test]
    fn plan_boundary_initial_skips_failed_sources_and_maps_exhausted_to_empty_string() {
        let outcomes = [fetched("n5"), SourceOutcome::Failed, SourceOutcome::Exhausted];
        assert_eq!(plan_boundary_initial(&outcomes), vec![(0, "n5".to_string()), (2, String::new())]);
        assert!(plan_boundary_initial(&[SourceOutcome::Failed]).is_empty());
        assert!(plan_boundary_initial(&[]).is_empty());
    }

    #[test]
    fn plan_boundary_extend_extends_only_contiguous_sources() {
        // source0: 境界n500, source1: 境界n300。until_id=n400 なので source0 は不連続(400<500)、
        // source1 は連続(400>=300)。連続なソースだけ延長する。
        let prev = bmap(&[(0, "n500"), (1, "n300")]);
        let outcomes = [fetched("n380"), fetched("n350")];
        assert_eq!(plan_boundary_extend(&prev, "n400", &outcomes), vec![(1, "n350".to_string())]);
    }

    #[test]
    fn plan_boundary_extend_skips_failed_and_unknown_sources_and_extends_exhausted_to_empty() {
        let prev = bmap(&[(0, "n100"), (2, "n100")]); // source1 は境界未確定
        let outcomes = [SourceOutcome::Failed, fetched("n50"), SourceOutcome::Exhausted];
        // 0: 失敗 / 1: 境界が無く連続性を検証できない / 2: 枯渇 → ""
        assert_eq!(plan_boundary_extend(&prev, "n200", &outcomes), vec![(2, String::new())]);
    }

    #[test]
    fn plan_boundary_extend_treats_until_id_equal_to_boundary_as_contiguous() {
        let prev = bmap(&[(0, "n300")]);
        assert_eq!(plan_boundary_extend(&prev, "n300", &[fetched("n250")]), vec![(0, "n250".to_string())]);
    }

    #[test]
    fn effective_boundary_is_max_and_requires_every_source() {
        assert_eq!(effective_boundary(&bmap(&[(0, "n100"), (1, "n300")]), 2), Some("n300".to_string()));
        // 枯渇済み("")は有効境界を塞がない
        assert_eq!(effective_boundary(&bmap(&[(0, ""), (1, "n300")]), 2), Some("n300".to_string()));
        assert_eq!(effective_boundary(&bmap(&[(0, ""), (1, "")]), 2), Some(String::new()));
        // 1ソースでも境界が無ければ未確定
        assert_eq!(effective_boundary(&bmap(&[(0, "n100")]), 2), None);
        assert_eq!(effective_boundary(&bmap(&[(1, "n100")]), 2), None);
        // 余分な行(ソース数より大きい idx)は無視する
        assert_eq!(effective_boundary(&bmap(&[(0, "n100"), (5, "n900")]), 1), Some("n100".to_string()));
        assert_eq!(effective_boundary(&bmap(&[]), 0), None);
    }

    #[test]
    fn backfill_cache_eligible_requires_streaming_api_sources_and_no_cache_source() {
        let mk = |kinds: Vec<ColumnKind>, use_cache: bool| ResolvedSources {
            kinds,
            use_cache,
            filter: CompiledFilter::PassAll,
        };
        assert!(backfill_cache_eligible(&mk(vec![ColumnKind::Home], false)));
        assert!(backfill_cache_eligible(&mk(vec![ColumnKind::Home, ColumnKind::Local], false)));
        assert!(!backfill_cache_eligible(&mk(vec![ColumnKind::Home, ColumnKind::Local], true)));
        assert!(!backfill_cache_eligible(&mk(vec![], true)));
        assert!(backfill_cache_eligible(&mk(
            vec![ColumnKind::Home, ColumnKind::Local, ColumnKind::List { list_id: "l1".into() }],
            false
        )));
        // ストリーミングを持たないソース(User/Tag/Search)はライブノートが column_note に入らないため対象外
        let user = || ColumnKind::User { user_id: "u1".into() };
        let tag = || ColumnKind::Tag { tag: "rust".into() };
        let search = || ColumnKind::Search { query: "q".into() };
        assert!(!backfill_cache_eligible(&mk(vec![ColumnKind::Home, user()], false)));
        assert!(!backfill_cache_eligible(&mk(vec![user()], false)));
        assert!(!backfill_cache_eligible(&mk(vec![tag()], false)));
        assert!(!backfill_cache_eligible(&mk(vec![search()], false)));
        assert!(!backfill_cache_eligible(&mk(vec![ColumnKind::Home, search()], false)));
    }

    #[test]
    fn should_try_backfill_cache_is_off_when_bypassed_or_ineligible() {
        // 通常の上スクロール: eligible のときだけキャッシュを試す
        assert!(should_try_backfill_cache(true, false));
        assert!(!should_try_backfill_cache(false, false));
        // ギャップ埋め(Issue #427): eligible でもキャッシュを使わない
        assert!(!should_try_backfill_cache(true, true));
        assert!(!should_try_backfill_cache(false, true));
    }

    fn gs(oldest: Option<&str>, reached: bool) -> GapSourceState {
        GapSourceState { oldest_fetched: oldest.map(str::to_string), reached_target: reached }
    }

    fn pairs(entries: &[(u32, &str)]) -> Vec<(u32, String)> {
        entries.iter().map(|(i, b)| (*i, b.to_string())).collect()
    }

    #[test]
    fn plan_boundary_raise_after_gap_none_when_gap_fully_filled() {
        let prev = bmap(&[(0, "n100")]);
        assert_eq!(plan_boundary_raise_after_gap(&prev, &[gs(Some("n050"), true)], true, None), None);
    }

    #[test]
    fn plan_boundary_raise_after_gap_raises_to_max_of_boundary_oldest_fetched_and_floor() {
        let prev = bmap(&[(0, "n100")]);
        // floor(切り詰め後の最古ノート)が最大
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, Some("n350")),
            Some(pairs(&[(0, "n350")]))
        );
        // oldest_fetched が最大(収集0件で floor が無い場合を含む)
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, None),
            Some(pairs(&[(0, "n300")]))
        );
        // 既存境界の方が新しい(大きい)なら変わらない → None
        let prev = bmap(&[(0, "n900")]);
        assert_eq!(plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, Some("n350")), None);
    }

    #[test]
    fn plan_boundary_raise_after_gap_treats_exhausted_boundary_as_smallest() {
        let prev = bmap(&[(0, "")]);
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &[gs(Some("n300"), false)], false, None),
            Some(pairs(&[(0, "n300")]))
        );
    }

    #[test]
    fn plan_boundary_raise_after_gap_leaves_reached_sources_alone_unless_notes_were_dropped() {
        let prev = bmap(&[(0, "n100"), (1, "n200")]);
        let sources = [gs(Some("n500"), false), gs(Some("n150"), true)];
        // 切り捨てが無ければ、追いついたソース(1)の行はそのまま。未到達(0)だけ oldest_fetched へ。
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &sources, false, None),
            Some(pairs(&[(0, "n500"), (1, "n200")]))
        );
        // 切り捨てがあれば、追いついたソースも dropped_floor まで引き上げる(Issue #432 レビュー指摘2)。
        // 切り詰めは全ソース合算で行うため、追いついたソースのノートも捨てられうる。
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &sources, false, Some("n450")),
            Some(pairs(&[(0, "n500"), (1, "n450")]))
        );
    }

    #[test]
    fn plan_boundary_raise_after_gap_raises_every_source_when_all_reached_but_notes_were_dropped() {
        // 全ソースが追いついても、limit 超過で収集結果を切り詰めると newest_known 直上の範囲が
        // キャッシュに入らない(Issue #432 レビュー指摘1)。
        let prev = bmap(&[(0, "n100"), (1, "n200")]);
        let sources = [gs(Some("n050"), true), gs(Some("n150"), true)];
        assert_eq!(
            plan_boundary_raise_after_gap(&prev, &sources, true, Some("n300")),
            Some(pairs(&[(0, "n300"), (1, "n300")]))
        );
        // 切り捨ても無ければ何もしない
        assert_eq!(plan_boundary_raise_after_gap(&prev, &sources, true, None), None);
    }

    #[test]
    fn plan_boundary_raise_after_gap_drops_rows_it_cannot_raise() {
        // 1ページも取れなかったソース(oldest_fetched=None)は未確定にする
        let prev = bmap(&[(0, "n100"), (1, "n200")]);
        let sources = [gs(None, false), gs(Some("n150"), true)];
        assert_eq!(plan_boundary_raise_after_gap(&prev, &sources, false, None), Some(pairs(&[(1, "n200")])));
        // ソース情報が無い(fill_gap が Err)なら全行を落とす
        assert_eq!(plan_boundary_raise_after_gap(&prev, &[], false, None), Some(vec![]));
    }

    #[test]
    fn plan_boundary_raise_after_gap_does_not_create_rows_for_unknown_sources() {
        let prev = bmap(&[(0, "n100")]);
        let sources = [gs(Some("n300"), false), gs(Some("n300"), false)];
        assert_eq!(plan_boundary_raise_after_gap(&prev, &sources, false, None), Some(pairs(&[(0, "n300")])));
        // 境界が空(cache_eligible でないカラム)なら何もしない
        assert_eq!(plan_boundary_raise_after_gap(&bmap(&[]), &sources, false, None), None);
    }

    #[test]
    fn finalize_gap_fill_carries_all_reached_through() {
        assert!(finalize_gap_fill(vec![note("n1", 10)], true, 100).all_reached);
        assert!(!finalize_gap_fill(vec![], false, 100).all_reached);
    }

    #[test]
    fn finalize_gap_fill_reports_the_largest_dropped_id_even_when_all_sources_reached() {
        // 全ソースが追いついていても、limit 超過分は切り捨てられる(Issue #432 レビュー指摘1)。
        let result = finalize_gap_fill(vec![note("n3", 30), note("n2", 20), note("n1", 10)], true, 2);
        assert!(result.all_reached);
        assert_eq!(result.dropped_floor.as_deref(), Some("n1"));

        // 切り詰めは created_at 順、境界は id 順。捨てたノートの id が残した最古より大きくなりうる
        // ので、下限は「捨てた id の最大値」でなければならない(レビュー指摘3)。
        let result = finalize_gap_fill(vec![note("n5", 10), note("n9", 5)], true, 1);
        assert_eq!(result.notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n5"]);
        assert_eq!(result.dropped_floor.as_deref(), Some("n9"));

        // 切り捨てが無ければ None
        assert_eq!(finalize_gap_fill(vec![note("n1", 10)], true, 100).dropped_floor, None);
        assert_eq!(finalize_gap_fill(vec![], false, 100).dropped_floor, None);
    }

    #[test]
    fn restrict_to_boundary_keeps_notes_at_or_newer_than_the_effective_boundary() {
        let cached = vec![note("n5", 50), note("n3", 30), note("n1", 10)];
        // 境界未確定なら絞らない
        assert_eq!(restrict_to_boundary(cached.clone(), None).len(), 3);
        // 境界 "n3" 以上だけ残す(fetch_backfill の retain と同じ >=)
        let kept = restrict_to_boundary(cached.clone(), Some("n3"));
        assert_eq!(kept.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n5", "n3"]);
        // 枯渇済み("")は何も落とさない
        assert_eq!(restrict_to_boundary(cached.clone(), Some("")).len(), 3);
        // すべて境界より古ければ空になる
        assert!(restrict_to_boundary(cached, Some("n9")).is_empty());
    }

    fn gap_result(sources: Vec<GapSourceState>, all_reached: bool, dropped_floor: Option<&str>) -> GapFillResult {
        GapFillResult {
            notes: vec![],
            truncated: !all_reached,
            boundary_id: None,
            sources,
            all_reached,
            dropped_floor: dropped_floor.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_raises_stored_boundaries_when_truncated() {
        let cache = cache_with(&[]).await;
        cache.replace_fetch_boundaries("col1", &pairs(&[(0, "n100"), (1, "n200")])).await.unwrap();

        let gap = gap_result(vec![gs(Some("n300"), false), gs(Some("n150"), true)], false, Some("n350"));
        apply_gap_fill_boundaries(&cache, "col1", &gap).await;

        let mut got = cache.get_fetch_boundaries("col1").await.unwrap();
        got.sort();
        assert_eq!(got, pairs(&[(0, "n350"), (1, "n350")]));
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_leaves_boundaries_when_gap_fully_filled() {
        let cache = cache_with(&[]).await;
        cache.replace_fetch_boundaries("col1", &pairs(&[(0, "n100")])).await.unwrap();

        apply_gap_fill_boundaries(&cache, "col1", &gap_result(vec![gs(Some("n050"), true)], true, None)).await;

        assert_eq!(cache.get_fetch_boundaries("col1").await.unwrap(), pairs(&[(0, "n100")]));
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_is_a_no_op_for_columns_without_boundaries() {
        let cache = cache_with(&[]).await;
        apply_gap_fill_boundaries(&cache, "col1", &gap_result(vec![gs(None, false)], false, None)).await;
        assert!(cache.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn apply_gap_fill_boundaries_clears_rows_when_fill_gap_errored() {
        // fill_gap が Err のとき resume_column は sources=[] / all_reached=false を渡す
        let cache = cache_with(&[]).await;
        cache.replace_fetch_boundaries("col1", &pairs(&[(0, "n100")])).await.unwrap();

        apply_gap_fill_boundaries(&cache, "col1", &gap_result(vec![], false, None)).await;

        assert!(cache.get_fetch_boundaries("col1").await.unwrap().is_empty());
    }

    #[test]
    fn split_display_and_cacheable_dedupes_sorts_and_truncates_only_the_display() {
        let filtered: Vec<Note> = (1..=25).map(|i| note(&format!("n{i:02}"), i as i64)).collect();
        let mut with_dup = filtered.clone();
        with_dup.push(note("n25", 25)); // 複数ソースに同じノートが跨る場合

        let (display, cacheable) = split_display_and_cacheable(with_dup.clone(), false, None);
        assert_eq!(display.len(), INITIAL_LIMIT as usize);
        assert_eq!(display[0].id, "n25"); // created_at 降順
        assert_eq!(cacheable.len(), 25); // 重複除去済み・truncateしない

        // `from cache` を含むカラムは従来どおり truncate 後だけをキャッシュする
        let (display, cacheable) = split_display_and_cacheable(with_dup, true, None);
        assert_eq!(cacheable.len(), display.len());
    }

    #[test]
    fn split_display_and_cacheable_hides_notes_older_than_the_watermark_from_the_display_only() {
        let all: Vec<Note> = (1..=10).map(|i| note(&format!("n{i:02}"), i as i64)).collect();

        // 透かし n06: 画面用は n06 以上だけ(n10..n06)。cacheable は全件(10件)。
        let (display, cacheable) = split_display_and_cacheable(all.clone(), false, Some("n06"));
        assert_eq!(display.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n10", "n09", "n08", "n07", "n06"]);
        assert_eq!(cacheable.len(), 10);

        // `from cache` を含むカラムは cacheable が画面用と同じ
        let (display, cacheable) = split_display_and_cacheable(all.clone(), true, Some("n06"));
        assert_eq!(cacheable.len(), display.len());
        assert_eq!(display.len(), 5);

        // 透かしより新しいノートが無ければ画面用は空
        assert!(split_display_and_cacheable(all, false, Some("n99")).0.is_empty());
    }

    #[test]
    fn backfill_watermark_is_the_newest_of_the_per_source_oldest_ids() {
        // 最も浅い(=最古idが最も新しい)ソースの深さ
        assert_eq!(
            backfill_watermark(&[fetched("n500"), fetched("n200")], None),
            Some("n500".to_string())
        );
        // 枯渇済み・失敗したソースは制約しない
        assert_eq!(
            backfill_watermark(&[SourceOutcome::Exhausted, fetched("n200"), SourceOutcome::Failed], None),
            Some("n200".to_string())
        );
        // 制約するソースが無ければ None
        assert_eq!(backfill_watermark(&[SourceOutcome::Exhausted, SourceOutcome::Failed], None), None);
        assert_eq!(backfill_watermark(&[], None), None);
        // `from cache` の検索結果(INITIAL_LIMIT件に達したときの最小id)も1ソースとして含める
        assert_eq!(backfill_watermark(&[fetched("n200")], Some("n600")), Some("n600".to_string()));
        assert_eq!(backfill_watermark(&[], Some("n600")), Some("n600".to_string()));
    }

    #[test]
    fn merge_source_outcome_keeps_the_contiguous_per_source_coverage() {
        use SourceOutcome::{Exhausted, Failed};
        // 最初のパスで失敗したソースは、先頭から連続して取得できていないので Failed のまま
        assert_eq!(merge_source_outcome(Failed, fetched("n100")), Failed);
        assert_eq!(merge_source_outcome(Failed, Exhausted), Failed);
        // 枯渇済みはそれ以降も枯渇済み
        assert_eq!(merge_source_outcome(Exhausted, fetched("n100")), Exhausted);
        // 取得成功同士は、より古い(小さい)idへ
        assert_eq!(merge_source_outcome(fetched("n500"), fetched("n400")), fetched("n400"));
        assert_eq!(merge_source_outcome(fetched("n300"), fetched("n400")), fetched("n300"));
        // 後続パスで枯渇したら枯渇済み
        assert_eq!(merge_source_outcome(fetched("n500"), Exhausted), Exhausted);
        // 後続パスの失敗は、それまでに取れた範囲を保つ
        assert_eq!(merge_source_outcome(fetched("n500"), Failed), fetched("n500"));
    }

    fn bp(notes: &[(&str, i64)], outcomes: Vec<SourceOutcome>, cache_min: Option<&str>) -> BackfillPass {
        BackfillPass {
            notes: notes.iter().map(|(id, c)| note(id, *c)).collect(),
            outcomes,
            cache_min: cache_min.map(str::to_string),
        }
    }

    /// id `n{lo}`..`n{hi}`(3桁、created_at は数値と同順)のノートを `hi` から `lo` の順に並べて作る。
    fn range_notes(lo: i64, hi: i64) -> Vec<Note> {
        (lo..=hi).rev().map(|i| note(&format!("n{i:03}"), i)).collect()
    }

    fn ids(notes: &[Note]) -> Vec<&str> {
        notes.iter().map(|n| n.id.as_str()).collect()
    }

    /// issue #428 のシナリオ。密なソース A はフィルタで全て落ち、疎なソース B のノートだけが残る。
    /// A の生ページは n500 まで、B の生ページは n200 まで届く。透かし n500 より古い B のノートは
    /// 画面へ返さない(返すと次の until_id が n500 より古くなり、A の (until_id, n500) が飛ばされる)。
    #[tokio::test]
    async fn collect_backfill_pages_hides_notes_older_than_the_shallowest_source_depth() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        // 透かし n500 以上が INITIAL_LIMIT 件あるので1パスで終わる
        let mut notes = range_notes(601, 620);
        notes.push(note("n300", 300));
        notes.push(note("n250", 250));
        let mut script = std::collections::VecDeque::from(vec![BackfillPass {
            notes,
            outcomes: vec![fetched("n500"), fetched("n200")],
            cache_min: None,
        }]);

        let fetch = collect_backfill_pages(Some("n900"), false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(fetch.notes.len(), INITIAL_LIMIT as usize);
        assert!(fetch.notes.iter().all(|n| n.id.as_str() >= "n500"));
        // 透かしより古いノートもキャッシュには入る(境界は生ページ全体が column_note にある前提)
        assert_eq!(fetch.cacheable.len(), 22);
        assert_eq!(*calls.borrow(), vec![Some("n900".to_string())]);
        assert_eq!(fetch.source_outcomes, vec![fetched("n500"), fetched("n200")]);
    }

    #[tokio::test]
    async fn collect_backfill_pages_refetches_from_the_watermark_when_nothing_is_left_to_show() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let mut script = std::collections::VecDeque::from(vec![
            // 1パス目: 返せるノートは全て透かし n500 より古い → 画面用は空
            bp(&[("n300", 300), ("n250", 250)], vec![fetched("n500"), fetched("n250")], None),
            // 2パス目(until_id = n500): 全ソースが枯渇 → 透かし無し、これで終わる
            bp(&[("n480", 480), ("n470", 470)], vec![SourceOutcome::Exhausted, SourceOutcome::Exhausted], None),
        ]);

        let fetch = collect_backfill_pages(Some("n900"), false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(*calls.borrow(), vec![Some("n900".to_string()), Some("n500".to_string())]);
        assert_eq!(ids(&fetch.notes), vec!["n480", "n470"]);
        // cacheable は全パスの和集合
        assert_eq!(fetch.cacheable.len(), 4);
        assert_eq!(fetch.source_outcomes, vec![SourceOutcome::Exhausted, SourceOutcome::Exhausted]);
    }

    /// 画面用が INITIAL_LIMIT 件に満たない間はパスを続け、各パスの画面用を積み上げる。
    /// 最初のページが短いとカラムがスクロールできず、loadMore が呼ばれない(Column.svelte は
    /// scroll イベントでだけ loadMore を呼ぶ)。
    #[tokio::test]
    async fn collect_backfill_pages_accumulates_display_notes_across_passes_up_to_the_limit() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let mut script = std::collections::VecDeque::from(vec![
            // 1パス目: 透かし n500 以上は2件だけ
            bp(&[("n600", 600), ("n550", 550), ("n300", 300)], vec![fetched("n500"), fetched("n300")], None),
            // 2パス目(until_id = n500): 透かし n400 以上が18件 → 合計 INITIAL_LIMIT 件で止まる
            BackfillPass {
                notes: range_notes(481, 498),
                outcomes: vec![fetched("n400"), fetched("n300")],
                cache_min: None,
            },
        ]);

        let fetch = collect_backfill_pages(Some("n900"), false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(*calls.borrow(), vec![Some("n900".to_string()), Some("n500".to_string())]);
        assert_eq!(fetch.notes.len(), INITIAL_LIMIT as usize);
        assert_eq!(&ids(&fetch.notes)[..2], ["n600", "n550"]);
        assert_eq!(fetch.notes.last().unwrap().id, "n481");
        // 返したノートは全て最後の透かし n400 以上(次の until_id もそれ以上になる)
        assert!(fetch.notes.iter().all(|n| n.id.as_str() >= "n400"));
    }

    #[tokio::test]
    async fn collect_backfill_pages_does_not_refetch_when_every_source_is_exhausted() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let mut script = std::collections::VecDeque::from(vec![bp(
            &[],
            vec![SourceOutcome::Exhausted, SourceOutcome::Exhausted],
            None,
        )]);

        let fetch = collect_backfill_pages(None, false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert!(fetch.notes.is_empty());
        assert_eq!(calls.borrow().len(), 1);
    }

    #[tokio::test]
    async fn collect_backfill_pages_gives_up_at_the_pass_limit_and_returns_the_last_pass_without_a_watermark() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        // どのパスも透かし n500 より古いノートしか返らない
        let mut script: std::collections::VecDeque<BackfillPass> = (0..BACKFILL_MAX_PASSES)
            .map(|_| bp(&[("n100", 100)], vec![fetched("n500"), fetched("n100")], None))
            .collect();

        let fetch = collect_backfill_pages(Some("n900"), false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(calls.borrow().len(), BACKFILL_MAX_PASSES as usize);
        // スクロールが止まらないよう、最後の1回だけ透かしなしで返す(従来の挙動)
        assert_eq!(ids(&fetch.notes), vec!["n100"]);
    }

    #[tokio::test]
    async fn collect_backfill_pages_keeps_a_source_that_failed_on_the_first_pass_failed() {
        let mut script = std::collections::VecDeque::from(vec![
            bp(&[("n300", 300)], vec![SourceOutcome::Failed, fetched("n500")], None),
            bp(&[("n460", 460)], vec![SourceOutcome::Exhausted, SourceOutcome::Exhausted], None),
        ]);

        let fetch = collect_backfill_pages(Some("n900"), false, |_| {
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(ids(&fetch.notes), vec!["n460"]);
        // 先頭から連続して取得できていないソースは境界に使わせない
        assert_eq!(fetch.source_outcomes, vec![SourceOutcome::Failed, SourceOutcome::Exhausted]);
    }

    /// 途中のパスで失敗したソースが後のパスで復帰しても、間の欠落を連続とみなして境界を書いてはいけない
    /// (Issue #428 レビュー指摘1)。X: n500 まで取得 → 2パス目で失敗 → 3パス目で枯渇、のとき、
    /// (n300, n500) は未取得なので X の境界は枯渇(`""`)ではなく n500 のまま。
    #[tokio::test]
    async fn collect_backfill_pages_freezes_a_source_that_failed_after_it_had_succeeded() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let mut script = std::collections::VecDeque::from(vec![
            bp(&[], vec![fetched("n500"), fetched("n480")], None),
            // X は失敗(透かしを制約しない)、Y は n300 まで
            bp(&[], vec![SourceOutcome::Failed, fetched("n300")], None),
            // X が復帰して枯渇、Y も枯渇
            bp(&[], vec![SourceOutcome::Exhausted, SourceOutcome::Exhausted], None),
        ]);

        let fetch = collect_backfill_pages(None, false, |until| {
            calls.borrow_mut().push(until);
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(*calls.borrow(), vec![None, Some("n500".to_string()), Some("n300".to_string())]);
        assert_eq!(fetch.source_outcomes, vec![fetched("n500"), SourceOutcome::Exhausted]);
    }

    #[tokio::test]
    async fn collect_backfill_pages_treats_a_full_cache_search_page_as_a_source_for_the_watermark() {
        let mut script = std::collections::VecDeque::from(vec![
            bp(&[("n700", 700), ("n300", 300)], vec![fetched("n200")], Some("n600")),
            // 2パス目(until_id = n600): 枯渇して終わる
            bp(&[], vec![SourceOutcome::Exhausted], None),
        ]);

        let fetch = collect_backfill_pages(None, true, |_| {
            std::future::ready(Ok(script.pop_front().expect("unexpected extra pass")))
        })
        .await
        .unwrap();

        assert_eq!(ids(&fetch.notes), vec!["n700"]);
        // `from cache` を含むカラムは cacheable が画面用と同じ
        assert_eq!(ids(&fetch.cacheable), vec!["n700"]);
    }

    /// truncate 起因の欠落の回帰テスト: 2ソースが交互に並び全件フィルタを通るとき、
    /// 初回取得後の境界 E 以上のノートが column_note に全部入っていなければならない。
    /// 画面へ返す20件だけをキャッシュすると E 以上のノートが欠落する。
    #[tokio::test]
    async fn caching_the_full_filtered_set_keeps_every_note_at_or_above_the_boundary() {
        // source A: 偶数 n02..n40、source B: 奇数 n01..n39(各20件、id と created_at は同順)
        let mut all: Vec<Note> = (1..=40).map(|i| note(&format!("n{i:02}"), i as i64)).collect();
        all.reverse();
        let outcomes = [fetched("n02"), fetched("n01")];
        let boundaries = bmap(&plan_boundary_initial(&outcomes).iter().map(|(i, b)| (*i, b.as_str())).collect::<Vec<_>>());
        let e = effective_boundary(&boundaries, 2).unwrap();
        assert_eq!(e, "n02");

        let (display, cacheable) = split_display_and_cacheable(all, false, None);
        assert_eq!(display.len(), 20);

        // 全件キャッシュ: E 以上(n02..n40 の39件)がすべて取り出せる
        let store = cache_with(&cacheable).await;
        let got = store.load_cached_before("col1", "n99", 100).await.unwrap();
        let at_or_above: Vec<&str> = got.iter().map(|n| n.id.as_str()).filter(|id| *id >= e.as_str()).collect();
        assert_eq!(at_or_above.len(), 39);

        // 対比: 画面へ返す20件だけをキャッシュすると、境界 E が完全と主張する範囲に欠落が出る
        let store = cache_with(&display).await;
        let got = store.load_cached_before("col1", "n99", 100).await.unwrap();
        assert!(got.iter().filter(|n| n.id.as_str() >= e.as_str()).count() < 39);
    }

    /// 呼び出し側(fetch_backfill / open_stream_and_fetch)がキャッシュへ書くのは
    /// `FilteredFetch::cacheable` でなければならない。画面用の `notes`(truncate後)を書くと、
    /// 境界が完全と主張する範囲に欠落が出る。両呼び出し元が共有する `cache_fetched` で固定する。
    #[tokio::test]
    async fn cache_fetched_stores_the_cacheable_set_not_the_truncated_display_notes() {
        let all: Vec<Note> = (1..=40).map(|i| note(&format!("n{i:02}"), i as i64)).collect();
        let (notes, cacheable) = split_display_and_cacheable(all, false, None);
        assert_eq!((notes.len(), cacheable.len()), (20, 40));
        let fetch = FilteredFetch { notes, cacheable, source_outcomes: vec![fetched("n02"), fetched("n01")] };

        let store = cache_with(&[]).await;
        cache_fetched(&store, "col1", &fetch).await.unwrap();

        let got = store.load_cached_before("col1", "n99", 100).await.unwrap();
        assert_eq!(got.len(), 40);
    }
}
