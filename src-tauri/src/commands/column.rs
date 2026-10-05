//! カラム(視覚グループ)とタブ(1タイムライン)の command。
//! タブはソース種別＋フィルタを持ち、購読＋REST取得しフィルタ適用して表示する。
//! 定義は SQLite に永続化し、起動時に list_groups/list_columns → resume_column で復元する。

use crate::api::meta::{
    fetch_antennas, fetch_followed_channels, fetch_server_version, fetch_user_lists, resolve_user,
};
use crate::api::notes::{fetch_notes, search_notes, SearchParams};
use crate::api::notifications::fetch_notifications;
use crate::domain::{
    search_capabilities, Column, ColumnGroup, ColumnKind, Edge, FilterQuery, MuteConfig, Note,
    Notification, PaneNode, SearchCapabilities, SourceItem, SplitDirection, User, UserList,
};
use crate::error::{Error, Result};
use crate::fence::{ColumnFence, Epoch};
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
/// サーバー検索(Issue #430)で、ミュートにより1ページが全滅したときの内部再取得の最大パス数。
const SEARCH_MAX_PASSES: u32 = 5;

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
    let epoch = state.column_fence.begin(&column.id);
    state.settings.upsert_column(&column)?;

    let (notes, notifications) =
        open_stream_and_fetch(&app, &state, &column, resolved, host, token, &epoch).await?;
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
    update_column_core(&app, &state, column_id, kind, filter, title).await
}

/// `update_column` の本体。`AppHandle` / `State` を引数に取らない形(`R: Runtime` ジェネリック)にして、
/// `mock_app()` からテストできるようにしている(`sync_server_mutes_core` と同じ狙い。Issue #456)。
///
/// 既知の制限(Issue #456。設計は docs/superpowers/specs/2026-10-05-fetch-backfill-boundary-race-design.md):
/// `invalidate` の後の失敗(キャッシュ DB の書き込み、通知カラムの初回取得)は、新定義が保存され、
/// ストリームが閉じたまま `Err` を返す。画面は旧定義のままで、再編集で直る。
async fn update_column_core<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    column_id: String,
    kind: ColumnKind,
    filter: FilterQuery,
    title: Option<String>,
) -> Result<OpenedColumn> {
    let mut column = load_column(state, &column_id)?;
    let is_notif = matches!(kind, ColumnKind::Notifications);
    let resolved = if is_notif {
        None
    } else {
        Some(resolve_sources(state, &column.account_id, &kind, &filter).await?)
    };

    column.kind = kind;
    column.filter = filter;
    column.title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    // どちらも読むだけで、`invalidate` の結果に依存しない(グループとアカウントは、更新で変わらない)。
    // `invalidate` の後に置くと、失敗したときに、新定義が保存され、ストリームが閉じ、キャッシュが消えた後に
    // `Err` を返してしまう(Issue #456)。
    let group = state
        .settings
        .load_groups()?
        .into_iter()
        .find(|g| g.id == column.group_id)
        .ok_or_else(|| Error::Invalid(format!("unknown group: {}", column.group_id)))?;
    let (host, token) = state.host_token(&column.account_id)?;
    // 新しい定義の保存・既存ストリームのクローズ・旧フィルタで貯めたキャッシュの破棄を、世代を進める
    // ロックの中で行う。実行中の取得は、世代が古くなって書き込みを捨てる(Issue #446)。
    let (epoch, cleared) = state
        .column_fence
        .invalidate(&column_id, || async {
            state.settings.upsert_column(&column)?;
            state.connections.close(&column_id);
            state.cache.clear_column_notes(&column_id).await
        })
        .await;
    cleared?;

    let (notes, notifications) =
        open_stream_and_fetch(app, state, &column, resolved, host, token, &epoch).await?;

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
    let (epoch, column) = begin_and_load_column(&state, &column_id)?;
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
        open_stream_and_fetch(&app, &state, &column, resolved, host, token, &epoch).await?
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
                // 残るので、空判定の前に行う(Issue #432)。取得中に update_column / close_column が
                // 走っていた(世代が古い)なら、何も書かず、イベントも出さない(Issue #446)。
                if !commit_gap_fill_writes(&state.column_fence, &state.cache, &column_id, &epoch, &gap_result).await {
                    return;
                }
                if gap_result.notes.is_empty() {
                    return;
                }
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
    // 世代はカラム定義を読む前に控える。取得中に update_column / close_column が走ったら、
    // 下の書き込みは捨てられる(Issue #446)。
    let (epoch, column) = begin_and_load_column(&state, &column_id)?;
    let resolved = resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?;

    let cache_eligible = backfill_cache_eligible(&resolved);
    let (boundaries, boundary_read_failed) = if cache_eligible {
        read_boundaries(state.cache.get_fetch_boundaries(&column.id).await)
    } else {
        (std::collections::HashMap::new(), false)
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
        let raw_loaded = cached.len();
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
        if let Some(notes) = cache_backfill_page(effective.as_deref(), &until_id, cached, raw_loaded, INITIAL_LIMIT) {
            state.cache_metrics.record_backfill(BackfillOutcome::Hit);
            return Ok(notes);
        }
        state
            .cache_metrics
            .record_backfill(backfill_fallback_outcome(boundary_read_failed, effective.as_deref()));
    }

    let fetch = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
    // 境界の延長は、ソースごとに、既存の境界と連続している場合のみ行う(plan_boundary_extend)。
    // 境界未確定のソースは連続性を検証できないので延長せず、カラム開き直し時の
    // open_stream_and_fetch が改めて確定させる。計画は、ここ(ロックの外)の古い境界ではなく、
    // commit_backfill_writes がロックの中で読む最新の境界から作る(Issue #456)。
    let extend_until = cache_eligible.then_some(until_id.as_str());
    // 取得中に update_column / close_column が走っていた(世代が古い)なら、何も書かず空で返す(Issue #446)。
    match commit_backfill_writes(&state.column_fence, &state.cache, &column.id, &epoch, &fetch, extend_until).await {
        None => return Ok(vec![]),
        Some(written) => written?,
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
    close_column_core(&state, &column_id).await
}

/// `close_column` の本体。`State` を引数に取らない形にして、テストできるようにしている(Issue #456)。
async fn close_column_core(state: &AppState, column_id: &str) -> Result<()> {
    state.settings.delete_column(column_id)?;
    // ストリームを閉じる処理と clear を、世代を進めるロックの中で行う。進行中の open_stream_and_fetch は、
    // ストリームを開く処理も同じロックの中なので、閉じた後に開き直すことも、書き込みを残すことも無い
    // (孤児データとリークしたストリームを作らない, Issue #446)。
    let (_, cleared) = state
        .column_fence
        .invalidate(column_id, || async {
            state.connections.close(column_id);
            state.cache.clear_column_notes(column_id).await
        })
        .await;
    state.column_fence.remove(column_id);
    cleared?;
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

/// カラム定義を読む**前**に世代を控え(Issue #446)、定義を読む。世代を控えるのが先という不変条件を
/// 1か所に集めたもの。未知のカラムID(閉じた後の取得など)では、`begin` が作ったエントリを消す。
/// 消さないと、閉じたカラムへの取得のたびにエントリが残る。実行中の書き込みは古い扱いになるが、
/// 存在しないカラムなので問題ない。
fn begin_and_load_column(state: &AppState, column_id: &str) -> Result<(Epoch, Column)> {
    let epoch = state.column_fence.begin(column_id);
    match load_column(state, column_id) {
        Ok(column) => Ok((epoch, column)),
        Err(e) => {
            state.column_fence.remove(column_id);
            Err(e)
        }
    }
}

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
async fn open_stream_and_fetch<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    column: &Column,
    resolved: Option<ResolvedSources>,
    host: String,
    token: String,
    epoch: &Epoch,
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
    let boundaries = backfill_cache_eligible(&resolved).then(|| plan_boundary_initial(&fetch.source_outcomes));
    // 書き込みと、ストリームを開く処理を、同じロックの中で行う。取得中に update_column / close_column が
    // 走っていた(世代が古い)なら、何も書かず、ストリームも開かず、Err を返す(Issue #446)。
    commit_initial_writes(
        &state.column_fence,
        &state.cache,
        &column.id,
        epoch,
        &fetch,
        boundaries.as_deref(),
        || open_streams_only(app, state, column, &resolved, host, token),
    )
    .await?;
    Ok((fetch.notes, vec![]))
}

/// 解決済みソースのうちストリーミング対応のものだけ購読を開く（REST初期取得は済んでいる前提）。
/// 複数ソースは column_id を共有しつつ sub_key を分けて同一カラムへ多重購読させる。
fn open_streams_only<R: Runtime>(
    app: &AppHandle<R>,
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

/// 境界の読み出し結果をマップにする。失敗時は空マップ(=全ソース未確定扱いでAPIへ落ちる。
/// 延長もされないので安全)にし、失敗したことを第2要素で返す。
fn read_boundaries(
    result: Result<Vec<(u32, String)>>,
) -> (std::collections::HashMap<u32, String>, bool) {
    match result {
        Ok(rows) => (rows.into_iter().collect(), false),
        Err(e) => {
            log::warn!("failed to read backfill fetch boundaries: {e}");
            (std::collections::HashMap::new(), true)
        }
    }
}

/// キャッシュで賄えず API へ落ちたときの理由。境界の読み出し失敗は「未確定」と区別して
/// `FallbackOther` に数える(DBエラーで `FallbackBoundaryUnset` が膨らむのを防ぐ)。
fn backfill_fallback_outcome(boundary_read_failed: bool, effective: Option<&str>) -> BackfillOutcome {
    if effective.is_none() && !boundary_read_failed {
        BackfillOutcome::FallbackBoundaryUnset
    } else {
        BackfillOutcome::FallbackOther
    }
}

/// backfill 要求(until_id より古いページ)をキャッシュのみで賄えるか判定する純粋関数。
/// `boundary`(Some) は「これより新しいノートはAPI取得済みで完全」という境界。
/// `cached` は呼び出し元が事前に `load_cached_before` で取得した結果。
/// 境界が未確定、要求範囲が境界に届かない(未検証領域を含みうる)、
/// またはキャッシュ件数が limit に満たない場合は None(=APIへフォールバックすべき)を返す。
/// ただし境界が `""`(全ソース枯渇済み)でカラム全体が完全と分かっており、`load_cached_before` の
/// 生の読み出し件数 `raw_loaded` が limit 未満(=キャッシュを読み尽くした)なら、件数不足でも
/// キャッシュだけで返す。判定にフィルタ後の `cached.len()` は使わない: ミュート等で間引かれて
/// 短くなっただけのページを末尾到達と取り違えないため。
fn cache_backfill_page(
    boundary: Option<&str>,
    until_id: &str,
    cached: Vec<Note>,
    raw_loaded: usize,
    limit: u32,
) -> Option<Vec<Note>> {
    let boundary = boundary?;
    if until_id <= boundary {
        return None;
    }
    let column_exhausted = boundary.is_empty() && (raw_loaded as u32) < limit;
    if column_exhausted || cached.len() as u32 >= limit {
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
    let Ok((epoch, column)) = begin_and_load_column(&state, column_id) else {
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
    // 残るので、空判定の前に行う(Issue #432)。取得中に update_column / close_column が
    // 走っていた(世代が古い)なら、何も書かず、イベントも出さない(Issue #446)。
    if !commit_gap_fill_writes(&state.column_fence, &state.cache, &column.id, &epoch, &gap_result).await {
        return;
    }
    if gap_result.notes.is_empty() {
        return;
    }
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

/// `fetch_backfill` の書き込み: 取得ノートのキャッシュと、ソースごとの境界の延長。
/// `epoch` が古ければ(取得中に `update_column` / `close_column` が走った)何も書かず `None` を返す
/// (Issue #446)。境界の延長の失敗は握りつぶす(更新できなくても従来の挙動に戻るだけ)。
///
/// 境界の延長は、**ロックの中で読んだ最新の境界**から計画する(`extend_until` は、キャッシュ対象の
/// カラムで `Some(until_id)`)。取得の前にロックの外で読んだ境界から計画すると、取得中に、同じ世代の
/// ギャップ埋めが境界を新しい方へ引き上げた場合に、その引き上げを古い方へ広げ戻して、未取得の区間を
/// 「完全」と主張してしまう(Issue #456)。
async fn commit_backfill_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    fetch: &FilteredFetch,
    extend_until: Option<&str>,
) -> Option<Result<()>> {
    fence
        .write_if_current(column_id, epoch, |boundaries_ok| async move {
            cache_fetched(cache, column_id, fetch).await?;
            // 境界の世代が古い(取得中に set_mute が境界を捨てた)なら、旧ミュートの結果に基づく
            // 延長を書かない(Issue #452)。
            if boundaries_ok {
                if let Some(until_id) = extend_until {
                    extend_boundaries_in_lock(cache, column_id, until_id, &fetch.source_outcomes).await;
                }
            }
            Ok::<(), Error>(())
        })
        .await
}

/// カラムのロックの中で、最新の境界を読み、`plan_boundary_extend` の計画で延長する。境界を読めなければ、
/// 延長を飛ばす(保守的)。計画に入るのは、既存の行があるソースだけなので、境界の行は挿入されない。
async fn extend_boundaries_in_lock(cache: &NoteCacheStore, column_id: &str, until_id: &str, outcomes: &[SourceOutcome]) {
    let prev = match cache.get_fetch_boundaries(column_id).await {
        Ok(prev) => prev,
        Err(e) => {
            log::warn!("skipping the backfill boundary extension: failed to read the boundaries of column {column_id}: {e}");
            return;
        }
    };
    let prev: std::collections::HashMap<u32, String> = prev.into_iter().collect();
    let plan = plan_boundary_extend(&prev, until_id, outcomes);
    if !plan.is_empty() {
        let _ = cache.extend_fetch_boundaries(column_id, &plan).await;
    }
}

/// `open_stream_and_fetch` の書き込み: 初回取得ノートのキャッシュと、`boundaries`(Some の時)での
/// 境界の置き換え。書き込みが成功したら、同じロックの中で `on_current`(ストリームを開く処理)を
/// 実行する。ロックの外で開くと、`close_column` / `update_column` と競合して、削除済み・旧定義の
/// カラムのストリームが開いたまま残る(Issue #446)。
/// `epoch` が古ければ、何も書かず `on_current` も呼ばず、明示的な `Err` を返す。「空の成功」を返すと、
/// 並行した `update_column` で古い定義と空のノートが画面に適用されてしまうため。
async fn commit_initial_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    fetch: &FilteredFetch,
    boundaries: Option<&[(u32, String)]>,
    on_current: impl FnOnce(),
) -> Result<()> {
    fence
        .write_if_current(column_id, epoch, |boundaries_ok| async move {
            cache_fetched(cache, column_id, fetch).await?;
            // 境界の世代が古いなら、旧ミュートの結果に基づく境界を書かない。未確定のままなら、
            // 次回の backfill は API 経由になるので安全(Issue #452)。
            if boundaries_ok {
                if let Some(entries) = boundaries {
                    let _ = cache.replace_fetch_boundaries(column_id, entries).await;
                }
            }
            on_current();
            Ok::<(), Error>(())
        })
        .await
        .unwrap_or_else(|| Err(Error::Invalid(format!("column {column_id} was modified while it was being opened"))))
}

/// ギャップ埋めの書き込み: 境界の引き上げと、収集したノートのキャッシュ。`epoch` が古ければ
/// 何も書かず `false` を返す。呼び出し元は `false` なら `ColumnGapFill` イベントも出さない(Issue #446)。
/// 境界の世代が古くても、引き上げは飛ばさない: 引き上げは、ロックの中で読んだ最新の境界の既存の行を
/// 保守的な方向へ動かすだけで、`set_mute` が捨てた後の空の状態からは何も書かない。飛ばすと、
/// 打ち切られたギャップが境界で覆われないまま残る(Issue #452)。
async fn commit_gap_fill_writes(
    fence: &ColumnFence,
    cache: &NoteCacheStore,
    column_id: &str,
    epoch: &Epoch,
    gap: &GapFillResult,
) -> bool {
    fence
        .write_if_current(column_id, epoch, |_boundaries_ok| async {
            apply_gap_fill_boundaries(cache, column_id, gap).await;
            if !gap.notes.is_empty() {
                let _ = cache.cache_notes(column_id, &gap.notes).await;
            }
        })
        .await
        .is_some()
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

/// サーバー検索(Issue #430)の `host` 入力を API 用に正規化する。空は未指定、`.` と
/// アカウント自身のホスト（大文字小文字・前後空白は無視）は Misskey の「ローカル」表記 `"."` にする。
fn normalize_search_host(input: Option<&str>, account_host: &str) -> Option<String> {
    let h = input?.trim();
    if h.is_empty() {
        return None;
    }
    if h == "." || h.eq_ignore_ascii_case(account_host) {
        return Some(".".into());
    }
    Some(h.to_string())
}

/// サーバー検索の入力検証。`notes/search` は `query` 必須（空の挙動は未確認なので送らない）で、
/// 日時範囲はサーバーが対応しているときだけ許可する。
fn check_search_request(query: &str, has_date: bool, caps: &SearchCapabilities) -> Result<()> {
    if query.trim().is_empty() {
        return Err(Error::Invalid("search query is empty".into()));
    }
    if has_date && !caps.date_range {
        return Err(Error::Invalid("date range search is not supported by this server".into()));
    }
    Ok(())
}

/// フロントから秒で受けた日時を API が使うミリ秒へ変換する。
fn secs_to_ms(secs: u32) -> u64 {
    u64::from(secs) * 1000
}

/// 開始日時だけが指定されたときに補う終了日時(ミリ秒)。Misskey の `makePaginationQuery` は
/// `sinceId` だけだと昇順（開始に近い古い側から）で返すため、現在時刻の上限を置いて新しい順に揃える。
/// 終了が指定済み、またはページング中（`untilId` が上限になる）なら補わない。
fn effective_until_date_ms(
    since_ms: Option<u64>,
    until_ms: Option<u64>,
    until_id: Option<&str>,
    now_ms: u64,
) -> Option<u64> {
    if until_ms.is_some() {
        until_ms
    } else if since_ms.is_some() && until_id.is_none() {
        Some(now_ms)
    } else {
        None
    }
}

/// 1ページ分を取得してフィルタ(ミュート)をかける。生応答が `limit` 件ちょうどで全件除外された
/// ときは、画面側が「これ以上なし」と誤判定しないよう、生の最後の id から最大 `SEARCH_MAX_PASSES`
/// パス取り直す。生応答が `limit` 未満（最終ページ）や空なら、そのまま結果（空含む）を返す。
async fn collect_unmuted_page<F, Fut>(
    until_id: Option<String>,
    limit: u32,
    mut fetch: F,
    filter: impl Fn(Vec<Note>) -> Vec<Note>,
) -> Result<Vec<Note>>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<Note>>>,
{
    let mut until = until_id;
    for _ in 0..SEARCH_MAX_PASSES {
        let raw = fetch(until.clone()).await?;
        let full = !raw.is_empty() && raw.len() as u32 >= limit;
        let last_id = raw.last().map(|n| n.id.clone());
        let kept = filter(raw);
        if !kept.is_empty() || !full {
            return Ok(kept);
        }
        until = last_id;
    }
    Ok(Vec::new())
}

/// サーバー検索の結果にキャッシュ検索(`search_cache_core`)と同じミュートを適用する。
fn apply_search_mutes(
    notes: Vec<Note>,
    mute: &MuteConfig,
    is_server_muted: impl Fn(&Note) -> bool,
    is_word_muted: impl Fn(&Note) -> bool,
) -> Vec<Note> {
    notes
        .into_iter()
        .filter(|n| !crate::filter::mute::is_muted(n, mute) && !is_server_muted(n) && !is_word_muted(n))
        .collect()
}

/// アカウントの接続先サーバーが対応する検索機能。バージョンは `AppState` にキャッシュし、
/// 未取得なら `/api/meta` から取得する。取得失敗は非対応扱い（日時欄を隠す側に倒す）で、
/// 失敗はキャッシュしないため次回また取りに行く。未登録アカウントだけはエラーを返す。
async fn search_capabilities_for(state: &AppState, account_id: &str) -> Result<SearchCapabilities> {
    if let Some(v) = state.server_version(account_id) {
        return Ok(search_capabilities(Some(&v)));
    }
    let client = state.client_for(account_id)?;
    match fetch_server_version(&client).await {
        Ok(Some(v)) => {
            state.set_server_version(account_id, v.clone());
            Ok(search_capabilities(Some(&v)))
        }
        Ok(None) | Err(_) => Ok(search_capabilities(None)),
    }
}

/// 検索モーダル(Issue #430)用: アカウントの接続先サーバーが対応する検索機能を返す。
#[tauri::command]
#[specta::specta]
pub async fn get_search_capabilities(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<SearchCapabilities> {
    search_capabilities_for(&state, &account_id).await
}

/// 検索モーダル(Issue #430)用: Misskey サーバーの `notes/search` による一回性の検索。
/// `acct` は `@user@host` 形式（userId へ解決する）、日時は秒（日時範囲はサーバーが対応する場合のみ）。
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn search_server_notes(
    state: State<'_, AppState>,
    account_id: String,
    query: String,
    acct: Option<String>,
    host: Option<String>,
    since_date: Option<u32>,
    until_date: Option<u32>,
    until_id: Option<String>,
    limit: u32,
) -> Result<Vec<Note>> {
    let has_date = since_date.is_some() || until_date.is_some();
    // 日時指定が無ければ対応判定のためのネットワークアクセスを省く
    let caps = if has_date {
        search_capabilities_for(&state, &account_id).await?
    } else {
        search_capabilities(None)
    };
    check_search_request(&query, has_date, &caps)?;

    let client = state.client_for(&account_id)?;
    let user_id = match acct.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => Some(resolve_user(&client, a).await?.id),
        None => None,
    };
    let since_ms = since_date.map(secs_to_ms);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let params = SearchParams {
        query: query.trim().to_string(),
        user_id,
        host: normalize_search_host(host.as_deref(), client.host()),
        since_date_ms: since_ms,
        until_date_ms: effective_until_date_ms(
            since_ms,
            until_date.map(secs_to_ms),
            until_id.as_deref(),
            now_ms,
        ),
        until_id: None,
        limit,
    };

    let mute = state.mute.lock().unwrap().clone();
    let client = &client;
    collect_unmuted_page(
        until_id,
        limit,
        |until| {
            let p = SearchParams { until_id: until, ..params.clone() };
            async move { search_notes(client, &p).await }
        },
        |raw| {
            apply_search_mutes(
                raw,
                &mute,
                |n| server_muted_note(&state, &account_id, n),
                |n| state.is_word_muted(&account_id, n),
            )
        },
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
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
        let result = cache_backfill_page(None, "n999", cached, 0, 20);
        assert!(result.is_none());
    }

    #[test]
    fn cache_backfill_page_none_when_until_id_at_or_before_boundary() {
        let cached = vec![note("n1", 10)];
        // until_id が境界と同じ、または境界より古い場合は「未検証の領域」を含みうるのでAPIへ
        assert!(cache_backfill_page(Some("n500"), "n500", cached.clone(), 1, 1).is_none());
        assert!(cache_backfill_page(Some("n500"), "n400", cached, 1, 1).is_none());
    }

    #[test]
    fn cache_backfill_page_none_when_cached_count_below_limit() {
        let cached = vec![note("n1", 10), note("n2", 20)];
        let result = cache_backfill_page(Some("n001"), "n999", cached, 2, 20);
        assert!(result.is_none());
    }

    #[test]
    fn cache_backfill_page_some_when_within_boundary_and_enough_notes() {
        let cached = vec![note("n2", 20), note("n1", 10)];
        let result = cache_backfill_page(Some("n001"), "n999", cached.clone(), 2, 2);
        assert_eq!(
            result.unwrap().iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            cached.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_backfill_page_some_when_every_source_exhausted_even_below_limit() {
        // 有効境界が ""(全ソース枯渇済み)で、読み出しが limit 未満で尽きた = カラム全体が完全
        let cached = vec![note("n2", 20), note("n1", 10)];
        let result = cache_backfill_page(Some(""), "n999", cached.clone(), 2, 20);
        assert_eq!(result.unwrap().len(), cached.len());
        // 0件でも、末尾に達したという完全な答えとして返す
        assert_eq!(cache_backfill_page(Some(""), "n999", vec![], 0, 20), Some(vec![]));
    }

    #[test]
    fn cache_backfill_page_none_when_exhausted_but_page_was_full_before_filtering() {
        // 読み出しが limit 件に達していれば、ミュート等で間引かれて短くなっただけで続きがありうる
        let cached = vec![note("n2", 20), note("n1", 10)];
        assert!(cache_backfill_page(Some(""), "n999", cached, 20, 20).is_none());
    }

    #[test]
    fn cache_backfill_page_none_when_not_exhausted_even_if_read_ran_dry() {
        // 通常の境界では、読み出しが尽きても境界より古い側の完全性は言えないのでAPIへ
        let cached = vec![note("n2", 20), note("n1", 10)];
        assert!(cache_backfill_page(Some("n001"), "n999", cached, 2, 20).is_none());
    }

    #[test]
    fn backfill_fallback_outcome_classifies_unset_boundary_and_other() {
        assert_eq!(backfill_fallback_outcome(false, None), BackfillOutcome::FallbackBoundaryUnset);
        assert_eq!(backfill_fallback_outcome(false, Some("n100")), BackfillOutcome::FallbackOther);
        assert_eq!(backfill_fallback_outcome(false, Some("")), BackfillOutcome::FallbackOther);
    }

    #[test]
    fn backfill_fallback_outcome_counts_boundary_read_failure_as_other_not_unset() {
        // 読み出し失敗で境界が空に見えても「未確定」ではない(DBエラーをメトリクスで区別する)
        assert_eq!(backfill_fallback_outcome(true, None), BackfillOutcome::FallbackOther);
    }

    #[test]
    fn read_boundaries_flags_error_and_falls_back_to_empty_map() {
        let (map, failed) = read_boundaries(Ok(vec![(0, "n1".to_string()), (1, String::new())]));
        assert!(!failed);
        assert_eq!(map, bmap(&[(0, "n1"), (1, "")]));

        let (map, failed) = read_boundaries(Err(Error::Invalid("db down".into())));
        assert!(failed);
        assert!(map.is_empty());
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

    #[test]
    fn normalize_search_host_maps_own_host_and_dot_to_local() {
        assert_eq!(normalize_search_host(None, "misskey.io"), None);
        assert_eq!(normalize_search_host(Some(""), "misskey.io"), None);
        assert_eq!(normalize_search_host(Some("   "), "misskey.io"), None);
        assert_eq!(normalize_search_host(Some("."), "misskey.io"), Some(".".into()));
        assert_eq!(normalize_search_host(Some("misskey.io"), "misskey.io"), Some(".".into()));
        // 大文字小文字・前後空白は無視して自ホストと比較する
        assert_eq!(normalize_search_host(Some("  Misskey.IO "), "misskey.io"), Some(".".into()));
        assert_eq!(
            normalize_search_host(Some(" example.com "), "misskey.io"),
            Some("example.com".into())
        );
    }

    #[test]
    fn check_search_request_requires_keyword_and_capability_for_dates() {
        let no_date = SearchCapabilities { date_range: false };
        let with_date = SearchCapabilities { date_range: true };

        assert!(check_search_request("rust", false, &no_date).is_ok());
        assert!(check_search_request("rust", true, &with_date).is_ok());
        assert!(matches!(check_search_request("", false, &with_date), Err(Error::Invalid(_))));
        assert!(matches!(check_search_request("  \t", false, &with_date), Err(Error::Invalid(_))));
        // 日時が指定されたのにサーバーが非対応なら拒否（UIが隠す前提の二重防御）
        assert!(matches!(check_search_request("rust", true, &no_date), Err(Error::Invalid(_))));
    }

    #[test]
    fn secs_to_ms_converts_without_overflow() {
        assert_eq!(secs_to_ms(0), 0);
        assert_eq!(secs_to_ms(1_700_000_000), 1_700_000_000_000);
        assert_eq!(secs_to_ms(u32::MAX), 4_294_967_295_000);
    }

    #[test]
    fn apply_search_mutes_drops_local_server_and_word_muted_notes_keeping_order() {
        let mut spoiler = note("n1", 100);
        spoiler.text = Some("spoiler content".into());
        let notes = vec![spoiler, note("n2", 200), note("n3", 300), note("n4", 400)];
        let mute = MuteConfig { ng_words: vec!["spoiler".into()], ..Default::default() };

        let got = apply_search_mutes(notes, &mute, |n| n.id == "n2", |n| n.id == "n3");

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n4"]);
    }

    #[tokio::test]
    async fn search_capabilities_for_uses_the_cached_version_without_network() {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory());
        state.set_server_version("a1", "2026.9.1".into());
        state.set_server_version("a2", "2025.4.1-io.12b-fb6fbea074".into());

        assert!(search_capabilities_for(&state, "a1").await.unwrap().date_range);
        assert!(!search_capabilities_for(&state, "a2").await.unwrap().date_range);
    }

    #[test]
    fn effective_until_date_adds_an_upper_bound_only_when_since_is_alone() {
        let now = 1_900_000_000_000;
        // 開始だけ・ページング前: Misskey は sinceId だけだと昇順で返すので、現在時刻で上限を補い新しい順に揃える
        assert_eq!(effective_until_date_ms(Some(1), None, None, now), Some(now));
        // 終了が指定されていればそのまま
        assert_eq!(effective_until_date_ms(Some(1), Some(5), None, now), Some(5));
        assert_eq!(effective_until_date_ms(None, Some(5), None, now), Some(5));
        // ページング中は untilId が上限なので補わない
        assert_eq!(effective_until_date_ms(Some(1), None, Some("n9"), now), None);
        // 開始も終了も無ければ何も足さない
        assert_eq!(effective_until_date_ms(None, None, None, now), None);
    }

    fn muted_ids(ids: &'static [&'static str]) -> impl Fn(Vec<Note>) -> Vec<Note> {
        move |notes| notes.into_iter().filter(|n| !ids.contains(&n.id.as_str())).collect()
    }

    #[tokio::test]
    async fn collect_unmuted_page_refetches_past_a_fully_muted_full_page() {
        let calls = std::cell::RefCell::new(Vec::<Option<String>>::new());
        let got = collect_unmuted_page(
            None,
            2,
            |until| {
                calls.borrow_mut().push(until.clone());
                async move {
                    Ok(match until.as_deref() {
                        None => vec![note("n3", 3), note("n2", 2)],
                        _ => vec![note("n1", 1)],
                    })
                }
            },
            muted_ids(&["n3", "n2"]),
        )
        .await
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"]);
        // 2回目は生の最後のid(n2)を untilId にして取り直す
        assert_eq!(*calls.borrow(), vec![None, Some("n2".to_string())]);
    }

    #[tokio::test]
    async fn collect_unmuted_page_returns_immediately_when_something_survives_or_page_is_short() {
        let calls = std::cell::Cell::new(0);
        let fetch = |_u: Option<String>| {
            calls.set(calls.get() + 1);
            async { Ok(vec![note("n2", 2), note("n1", 1)]) }
        };
        let got = collect_unmuted_page(None, 2, fetch, muted_ids(&["n2"])).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"]);
        assert_eq!(calls.get(), 1);

        // 生応答が limit 未満＝最終ページ。全部ミュートでも取り直さず空（終端）を返す
        let calls = std::cell::Cell::new(0);
        let fetch = |_u: Option<String>| {
            calls.set(calls.get() + 1);
            async { Ok(vec![note("n1", 1)]) }
        };
        let got = collect_unmuted_page(None, 2, fetch, muted_ids(&["n1"])).await.unwrap();
        assert!(got.is_empty());
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn collect_unmuted_page_gives_up_after_the_pass_limit() {
        let calls = std::cell::Cell::new(0u32);
        let fetch = |_u: Option<String>| {
            let n = calls.get();
            calls.set(n + 1);
            // 毎回 limit 件ちょうどで全件ミュート（untilId が前進することも確認できる id にする）
            async move { Ok(vec![note(&format!("a{n}"), 2), note(&format!("b{n}"), 1)]) }
        };
        let keep_none = |_notes: Vec<Note>| Vec::new();
        let got = collect_unmuted_page(None, 2, fetch, keep_none).await.unwrap();
        assert!(got.is_empty());
        assert_eq!(calls.get(), SEARCH_MAX_PASSES);
    }

    #[tokio::test]
    async fn collect_unmuted_page_propagates_fetch_errors() {
        let got = collect_unmuted_page(
            None,
            2,
            |_u: Option<String>| async { Err::<Vec<Note>, _>(Error::Network("down".into())) },
            muted_ids(&[]),
        )
        .await;
        assert!(matches!(got, Err(Error::Network(_))));
    }

    fn mem_cache() -> NoteCacheStore {
        NoteCacheStore::new(crate::store::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        ))
    }

    fn fetch_of(ids: &[&str]) -> FilteredFetch {
        FilteredFetch {
            notes: vec![],
            cacheable: ids.iter().enumerate().map(|(i, id)| note(id, i as i64 + 1)).collect(),
            source_outcomes: vec![],
        }
    }

    fn fetch_with_outcomes(ids: &[&str], outcomes: Vec<SourceOutcome>) -> FilteredFetch {
        FilteredFetch { source_outcomes: outcomes, ..fetch_of(ids) }
    }

    fn pair(idx: u32, id: &str) -> (u32, String) {
        (idx, id.to_string())
    }

    /// `update_column` が行うこと(新しい定義の保存の代わりに新フィルタ側の境界を置く + clear)を模す。
    async fn simulate_update_column(fence: &ColumnFence, cache: &NoteCacheStore, column_id: &str) -> Epoch {
        fence
            .invalidate(column_id, || async {
                cache.clear_column_notes(column_id).await.unwrap();
                cache.replace_fetch_boundaries(column_id, &[pair(0, "n900")]).await.unwrap();
            })
            .await
            .0
    }

    #[tokio::test]
    async fn commit_backfill_writes_caches_notes_and_extends_boundaries_when_current() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();
        let epoch = fence.begin("c1");

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_with_outcomes(&["n400"], vec![fetched("n300")]), Some("n600")).await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n300")]);
    }

    #[tokio::test]
    async fn commit_backfill_writes_writes_nothing_after_update_column() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1"); // 旧定義で取得を始めた
        simulate_update_column(&fence, &cache, "c1").await;

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_with_outcomes(&["n400"], vec![fetched("n300")]), Some("n950")).await;

        assert!(written.is_none());
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        // until_id(n950)は新フィルタの境界(n900)と連続しているので、書き込まれていれば、旧フィルタの延長(n300)で
        // 新フィルタの境界(n900)が古い方へ動く。世代が古いので、動かない
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n900")]);
    }

    #[tokio::test]
    async fn commit_backfill_writes_leaves_no_orphans_after_close() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        fence.invalidate("c1", || async { cache.clear_column_notes("c1").await.unwrap() }).await;
        fence.remove("c1"); // close_column

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_with_outcomes(&["n400"], vec![fetched("n300")]), Some("n600")).await;

        assert!(written.is_none());
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_initial_writes_replaces_boundaries_when_current() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");

        let written = commit_initial_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_of(&["n400"]),
            Some(&[pair(0, "n100")]),
            || {},
        )
        .await;

        assert!(written.is_ok());
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n100")]);
    }

    #[tokio::test]
    async fn commit_initial_writes_keeps_boundaries_untouched_when_none_given() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n700")]).await.unwrap();
        let epoch = fence.begin("c1");

        commit_initial_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), None, || {}).await.unwrap();

        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n700")]);
    }

    #[tokio::test]
    async fn commit_initial_writes_does_not_overwrite_a_newer_update_column() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let slow = fence.begin("c1"); // 1回目の update_column の取得(遅い)
        simulate_update_column(&fence, &cache, "c1").await; // 2回目が先に終わった

        let written = commit_initial_writes(
            &fence,
            &cache,
            "c1",
            &slow,
            &fetch_of(&["n400"]),
            Some(&[pair(0, "n100")]),
            || {},
        )
        .await;

        // 古い取得は「空の成功」ではなく明示的な失敗にする(フロントが古い定義を適用しないように)
        assert!(matches!(written, Err(Error::Invalid(_))));
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n900")]);
    }

    #[tokio::test]
    async fn commit_initial_writes_runs_on_current_after_the_writes_when_current() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        let opened = std::cell::Cell::new(false);

        commit_initial_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), None, || opened.set(true))
            .await
            .unwrap();

        assert!(opened.get(), "世代が現在ならストリームを開く処理(on_current)が走る");
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn commit_initial_writes_does_not_run_on_current_when_stale() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        simulate_update_column(&fence, &cache, "c1").await;
        let opened = std::cell::Cell::new(false);

        let written =
            commit_initial_writes(&fence, &cache, "c1", &epoch, &fetch_of(&["n400"]), None, || opened.set(true)).await;

        assert!(written.is_err());
        assert!(!opened.get(), "古い世代では、旧定義のストリームを開かない");
    }

    fn gap_with(notes: Vec<Note>) -> GapFillResult {
        GapFillResult {
            notes,
            truncated: true,
            boundary_id: None,
            // 全ソースが追いついたが、limit で n450 以下を切り捨てた = 境界が n450 へ引き上がる
            sources: vec![GapSourceState { oldest_fetched: None, reached_target: true }],
            all_reached: true,
            dropped_floor: Some("n450".to_string()),
        }
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_caches_notes_and_raises_boundaries_when_current() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n400")]).await.unwrap();
        let epoch = fence.begin("c1");

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(written);
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n450")]);
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_writes_nothing_when_stale() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        simulate_update_column(&fence, &cache, "c1").await;

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(!written);
        assert!(cache.load_cached("c1", 10).await.unwrap().is_empty());
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n900")]);
    }

    /// `set_mute` が行うこと(境界の書きロックの中で、全カラムの境界を捨てる)を模す。
    async fn simulate_set_mute(fence: &ColumnFence, cache: &NoteCacheStore) {
        fence
            .invalidate_boundaries(|| async { cache.clear_all_fetch_boundaries().await.unwrap() })
            .await;
    }

    #[tokio::test]
    async fn commit_backfill_writes_caches_notes_but_does_not_resurrect_boundaries_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();
        let epoch = fence.begin("c1"); // 旧ミュートで取得を始めた
        simulate_set_mute(&fence, &cache).await;

        let written =
            commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_with_outcomes(&["n400"], vec![fetched("n300")]), Some("n600")).await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1); // ノートは書かれる
        // ロックの中で読む境界は、捨てた後なので空で、延長は計画されない(復活しない)
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_backfill_writes_extends_boundaries_when_the_epoch_was_begun_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        simulate_set_mute(&fence, &cache).await;
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();
        let epoch = fence.begin("c1"); // ミュート変更の後に取得を始めた(対照)

        commit_backfill_writes(&fence, &cache, "c1", &epoch, &fetch_with_outcomes(&["n400"], vec![fetched("n300")]), Some("n600"))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n300")]);
    }

    #[tokio::test]
    async fn commit_initial_writes_keeps_notes_and_streams_but_skips_boundaries_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1"); // 旧ミュートでカラムを開き始めた
        simulate_set_mute(&fence, &cache).await;
        let opened = std::cell::Cell::new(false);

        let written = commit_initial_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_of(&["n400"]),
            Some(&[pair(0, "n100")]),
            || opened.set(true),
        )
        .await;

        assert!(written.is_ok(), "ミュートを変えても、カラムは開ける(Err にしない)");
        assert!(opened.get(), "ストリームは開く");
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        // 旧ミュートの結果に基づく境界は書かない。未確定のままなら、次回の backfill は API 経由になる
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_does_not_resurrect_cleared_boundaries_after_set_mute() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        simulate_set_mute(&fence, &cache).await; // 境界は空

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(written);
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
        // 引き上げは既存の行にしか効かない(`prev` が空なら何も書かない)ので、復活しない
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_gap_fill_writes_still_raises_boundaries_when_only_the_boundary_epoch_is_stale() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1");
        simulate_set_mute(&fence, &cache).await;
        // ミュート変更の後に、別の取得が作った行
        cache.replace_fetch_boundaries("c1", &[pair(0, "n400")]).await.unwrap();

        let written = commit_gap_fill_writes(&fence, &cache, "c1", &epoch, &gap_with(vec![note("n600", 6)])).await;

        assert!(written);
        // 引き上げを飛ばすと、打ち切られたギャップが境界で覆われないまま残る
        assert_eq!(cache.get_fetch_boundaries("c1").await.unwrap(), vec![pair(0, "n450")]);
    }

    fn test_column(id: &str) -> Column {
        Column {
            id: id.into(),
            account_id: "a1".into(),
            kind: ColumnKind::Home,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
        }
    }

    #[test]
    fn begin_and_load_column_keeps_the_fence_entry_for_a_known_column() {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory());
        state.settings.upsert_column(&test_column("c1")).unwrap();

        let (_epoch, column) = begin_and_load_column(&state, "c1").unwrap();

        assert_eq!(column.id, "c1");
        assert!(state.column_fence.tracks("c1"));
    }

    #[test]
    fn begin_and_load_column_leaves_no_fence_entry_for_an_unknown_column() {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory());

        assert!(begin_and_load_column(&state, "closed").is_err());

        assert!(!state.column_fence.tracks("closed"));
    }

    /// `acc1` を登録し、API を `mock` へ向け、グループ `g1` を持つ `AppState`。
    fn command_state(mock: &MockServer) -> AppState {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory());
        state.register_test_account("acc1");
        state.set_test_api_base(mock.uri());
        state
            .settings
            .upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 400, auto: false })
            .unwrap();
        state
    }

    fn command_column(id: &str, kind: ColumnKind) -> Column {
        Column {
            id: id.into(),
            account_id: "acc1".into(),
            kind,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
        }
    }

    /// すべての POST に、空のページを返す。
    async fn mount_empty_pages(mock: &MockServer) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(mock)
            .await;
    }

    /// すべての POST に、ノート1件のページを返す(書き込みが漏れたときに、キャッシュに入るものがあるように)。
    async fn mount_one_note_page(mock: &MockServer) {
        let page = serde_json::json!([{
            "id": "n9",
            "createdAt": "2026-07-05T12:00:00.000Z",
            "text": "hello",
            "user": { "id": "u1", "username": "alice", "host": null },
            "visibility": "home"
        }]);
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(page)).mount(mock).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_stream_and_fetch_returns_an_error_and_writes_nothing_when_the_epoch_is_stale() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await; // 現行の世代なら、n9 がキャッシュされ、境界が書かれる
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Local);
        state.settings.upsert_column(&column).unwrap();
        let app = tauri::test::mock_app();
        let resolved = resolve_sources(&state, "acc1", &column.kind, &column.filter).await.unwrap();
        let (host, token) = state.host_token("acc1").unwrap();
        let stale = state.column_fence.begin("c1");
        state.column_fence.invalidate("c1", || async {}).await; // 取得中に update_column / close_column が走った

        let result = open_stream_and_fetch(app.handle(), &state, &column, Some(resolved), host, token, &stale).await;

        assert!(matches!(result, Err(Error::Invalid(_))), "古ければ、空の成功ではなく Err を返す");
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "境界を書かない");
        assert!(state.cache.load_cached("c1", 10).await.unwrap().is_empty(), "ノートを書かない");
        assert_eq!(state.connections.open_count(), 0, "ストリームを開かない");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_stream_and_fetch_succeeds_for_a_current_epoch() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await;
        let state = command_state(&mock);
        let column = command_column("c1", ColumnKind::Tag { tag: "foo".into() }); // ストリームを開かないソース
        state.settings.upsert_column(&column).unwrap();
        let app = tauri::test::mock_app();
        let resolved = resolve_sources(&state, "acc1", &column.kind, &column.filter).await.unwrap();
        let (host, token) = state.host_token("acc1").unwrap();
        let current = state.column_fence.begin("c1");

        let result = open_stream_and_fetch(app.handle(), &state, &column, Some(resolved), host, token, &current).await;

        let (notes, notifications) = result.unwrap();
        assert_eq!(notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n9"], "ページが実際にパースされて流れる");
        assert!(notifications.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_saves_the_new_definition_clears_the_cache_and_stales_earlier_epochs() {
        let mock = MockServer::start().await;
        mount_empty_pages(&mock).await;
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Home)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap(); // 旧定義で貯めた境界
        let in_flight = state.column_fence.begin("c1"); // 旧定義で取得を始めた
        let app = tauri::test::mock_app();

        let opened = update_column_core(
            app.handle(),
            &state,
            "c1".into(),
            ColumnKind::Tag { tag: "foo".into() },
            FilterQuery::Keywords(vec![]),
            Some("新しい名前".into()),
        )
        .await
        .unwrap();

        assert_eq!(opened.column.kind, ColumnKind::Tag { tag: "foo".into() });
        let saved = state.settings.load_columns().unwrap();
        assert_eq!(saved[0].kind, ColumnKind::Tag { tag: "foo".into() });
        assert_eq!(saved[0].title.as_deref(), Some("新しい名前"));
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "旧定義の境界は消える");
        let late = state.column_fence.write_if_current("c1", &in_flight, |_| async {}).await;
        assert!(late.is_none(), "旧定義の取得の書き込みは捨てられる");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_returns_an_error_for_an_unknown_column() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);
        let app = tauri::test::mock_app();

        let result = update_column_core(
            app.handle(),
            &state,
            "ghost".into(),
            ColumnKind::Home,
            FilterQuery::Keywords(vec![]),
            None,
        )
        .await;

        assert!(matches!(result, Err(Error::Invalid(_))));
        assert!(!state.column_fence.tracks("ghost"), "未知のカラムのエントリを作らない");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn close_column_core_removes_the_column_clears_the_cache_and_stales_earlier_epochs() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Home)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap();
        let in_flight = state.column_fence.begin("c1");

        close_column_core(&state, "c1").await.unwrap();

        assert!(state.settings.load_columns().unwrap().is_empty());
        assert!(state.settings.load_groups().unwrap().is_empty(), "空になったグループも消える");
        assert!(state.cache.get_fetch_boundaries("c1").await.unwrap().is_empty());
        assert!(!state.column_fence.tracks("c1"), "フェンスのエントリが残らない");
        let late = state.column_fence.write_if_current("c1", &in_flight, |_| async {}).await;
        assert!(late.is_none(), "閉じる前の取得の書き込みは捨てられる");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn close_column_core_leaves_no_fence_entry_for_an_unknown_column() {
        let mock = MockServer::start().await;
        let state = command_state(&mock);

        close_column_core(&state, "ghost").await.unwrap();

        assert!(!state.column_fence.tracks("ghost"));
    }

    #[tokio::test]
    async fn commit_backfill_writes_does_not_insert_a_boundary_for_a_source_without_a_row() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        let epoch = fence.begin("c1"); // 境界の行が、まだ無い(set_mute が捨てた後など)

        let written = commit_backfill_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_with_outcomes(&["n400"], vec![fetched("n300")]),
            Some("n600"),
        )
        .await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1, "ノートは書かれる");
        assert!(cache.get_fetch_boundaries("c1").await.unwrap().is_empty(), "行が無いソースは、延長で挿入しない");
    }

    #[tokio::test]
    async fn commit_backfill_writes_does_not_extend_a_boundary_rebuilt_after_set_mute_by_an_older_epoch() {
        let (fence, cache) = (ColumnFence::default(), mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();
        let epoch = fence.begin("c1"); // 旧ミュートで取得を始めた
        simulate_set_mute(&fence, &cache).await;
        // set_mute の後に、新ミュートで開き直されたカラムが、境界を作り直した(open_stream_and_fetch)。
        // ロックの中で読む境界は空ではないので、`boundaries_ok` のガードが無いと、旧ミュートの結果で延長してしまう
        cache.replace_fetch_boundaries("c1", &[pair(0, "n500")]).await.unwrap();

        let written = commit_backfill_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_with_outcomes(&["n400"], vec![fetched("n300")]),
            Some("n600"), // 作り直された境界 n500 に対して、連続している
        )
        .await;

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(
            cache.get_fetch_boundaries("c1").await.unwrap(),
            vec![pair(0, "n500")],
            "旧ミュートの結果に基づく延長は、作り直された境界を動かさない(Issue #452)"
        );
    }

    #[tokio::test]
    async fn commit_backfill_writes_skips_the_extension_when_the_boundaries_cannot_be_read() {
        let fence = ColumnFence::default();
        // 境界のテーブルを失った DB。`get_fetch_boundaries` が実際の SQL エラーになる(ノートの表は生きている)
        let conn = crate::store::db::open_cache_in_memory().unwrap();
        conn.execute("DROP TABLE column_source_boundary", []).unwrap();
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(conn));
        let epoch = fence.begin("c1");

        let written = commit_backfill_writes(
            &fence,
            &cache,
            "c1",
            &epoch,
            &fetch_with_outcomes(&["n400"], vec![fetched("n300")]),
            Some("n600"),
        )
        .await;

        assert!(matches!(written, Some(Ok(()))), "境界を読めなくても、ノートのキャッシュは成功する");
        assert_eq!(cache.load_cached("c1", 10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn commit_backfill_writes_plans_the_extension_from_the_boundary_read_inside_the_lock() {
        use std::sync::Arc;
        use tokio::sync::{mpsc, Semaphore};

        let fence = Arc::new(ColumnFence::default());
        let cache = Arc::new(mem_cache());
        cache.replace_fetch_boundaries("c1", &[pair(0, "n100")]).await.unwrap();
        let fetch_epoch = fence.begin("c1"); // fetch_backfill が、境界 n100 のもとで取得を始めた
        let gap_epoch = fence.begin("c1"); // 並行するギャップ埋め
        let (ready_tx, mut ready_rx) = mpsc::unbounded_channel();
        let gate = Arc::new(Semaphore::new(0));
        let gap_fill = {
            let (fence, cache, gate) = (Arc::clone(&fence), Arc::clone(&cache), Arc::clone(&gate));
            tokio::spawn(async move {
                fence
                    .write_if_current("c1", &gap_epoch, |_| async move {
                        ready_tx.send(()).unwrap();
                        gate.acquire().await.unwrap().forget();
                        // 打ち切られたギャップ埋めが、境界を n350 へ引き上げる(完全と言える範囲を縮める)
                        cache.replace_fetch_boundaries("c1", &[pair(0, "n350")]).await.unwrap();
                    })
                    .await
            })
        };
        ready_rx.recv().await.unwrap(); // ギャップ埋めが、カラムのロックを持った
        let commit = {
            let (fence, cache) = (Arc::clone(&fence), Arc::clone(&cache));
            tokio::spawn(async move {
                commit_backfill_writes(
                    &fence,
                    &cache,
                    "c1",
                    &fetch_epoch,
                    &fetch_with_outcomes(&["n080"], vec![fetched("n060")]), // id は文字列比較なので、桁数を揃える
                    Some("n120"), // 古い境界 n100 に対しては、連続している
                )
                .await
            })
        };
        // commit が、ロックの待ちに入るまで待つ。ロックの外で境界を読む実装(変異)が、先に境界を読み終える
        // 時間も兼ねる(`get_fetch_boundaries` は別スレッドで動くので、`yield_now` では足りない)。
        // 正しい実装は、待ち時間の長さによらず通る。
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        gate.add_permits(1); // ギャップ埋めが、境界を引き上げて、ロックを放す

        gap_fill.await.unwrap();
        let written = commit.await.unwrap();

        assert!(matches!(written, Some(Ok(()))));
        assert_eq!(
            cache.get_fetch_boundaries("c1").await.unwrap(),
            vec![pair(0, "n350")],
            "ロックの中で読んだ最新の境界(n350)に対しては、n120 は連続でない。引き上げた境界を、古い写しで広げない"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_backfill_extends_a_contiguous_boundary() {
        let mock = MockServer::start().await;
        mount_one_note_page(&mock).await; // ノート n9 の1ページ
        let app = tauri::test::mock_app();
        let state = command_state(&mock);
        state.settings.upsert_column(&command_column("c1", ColumnKind::Local)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n95".to_string())]).await.unwrap();
        app.manage(state);

        let notes = fetch_backfill(app.state::<AppState>(), "c1".into(), "n99".into(), true).await.unwrap();

        assert_eq!(notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n9"]);
        let boundaries = app.state::<AppState>().cache.get_fetch_boundaries("c1").await.unwrap();
        assert_eq!(boundaries.len(), 1);
        assert!(
            boundaries[0].1.as_str() < "n95",
            "n99 は境界 n95 と連続しているので、境界が古い方へ延長される: {boundaries:?}"
        );
    }

    /// `update_column_core` が `Err` を返したあと、定義・境界・世代・ストリームが変わっていないこと。
    async fn assert_update_left_nothing_changed(state: &AppState, original_kind: &ColumnKind, before: &Epoch) {
        let saved = state.settings.load_columns().unwrap();
        assert_eq!(saved[0].kind, *original_kind, "定義は旧定義のまま");
        assert_eq!(
            state.cache.get_fetch_boundaries("c1").await.unwrap(),
            vec![(0, "n100".to_string())],
            "キャッシュ(境界)は破棄されない"
        );
        let still_current = state.column_fence.write_if_current("c1", before, |_| async {}).await;
        assert!(still_current.is_some(), "世代は進まない(実行中の取得を捨てない)");
        // ストリームが閉じないことは、ここでは検証しない: `open_count()` はアカウント単位の接続数で、事前に
        // 接続を開いていないテストでは、`invalidate` の後で失敗する旧い順序でも 0 のままになるため。
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_changes_nothing_when_the_group_is_unknown() {
        let mock = MockServer::start().await;
        mount_empty_pages(&mock).await;
        let state = command_state(&mock);
        let column = Column { group_id: "ghost".into(), ..command_column("c1", ColumnKind::Home) };
        state.settings.upsert_column(&column).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap();
        let before = state.column_fence.begin("c1");
        let app = tauri::test::mock_app();

        let result = update_column_core(
            app.handle(),
            &state,
            "c1".into(),
            ColumnKind::Tag { tag: "foo".into() },
            FilterQuery::Keywords(vec![]),
            None,
        )
        .await;

        assert!(matches!(result, Err(Error::Invalid(_))));
        assert_update_left_nothing_changed(&state, &ColumnKind::Home, &before).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn update_column_core_changes_nothing_when_the_account_is_not_registered() {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory()); // アカウントを登録しない
        state
            .settings
            .upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 400, auto: false })
            .unwrap();
        state.settings.upsert_column(&command_column("c1", ColumnKind::Home)).unwrap();
        state.cache.replace_fetch_boundaries("c1", &[(0, "n100".to_string())]).await.unwrap();
        let before = state.column_fence.begin("c1");
        let app = tauri::test::mock_app();

        let result = update_column_core(
            app.handle(),
            &state,
            "c1".into(),
            ColumnKind::Tag { tag: "foo".into() },
            FilterQuery::Keywords(vec![]),
            None,
        )
        .await;

        assert!(matches!(result, Err(Error::Invalid(_))));
        assert_update_left_nothing_changed(&state, &ColumnKind::Home, &before).await;
    }
}
