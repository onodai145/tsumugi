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
use crate::filter::{ast, parser, sql, CompiledFilter};
use crate::state::AppState;
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
    state.settings.clear_column_notes(&column_id)?;

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
        let cached = state.settings.load_cached(&column.id, INITIAL_LIMIT)?;
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
                let gap_notes = fill_gap(&state, &account_id, &resolved, &newest_known_id, gap_limit)
                    .await
                    .unwrap_or_default();
                if gap_notes.is_empty() {
                    return;
                }
                let _ = state.settings.cache_notes(&column_id, &gap_notes);
                let _ = crate::events::ColumnGapFill {
                    column_id,
                    notes: gap_notes,
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
    state.settings.note_count()
}

/// 投稿日時(epoch秒)が since_epoch_secs 以降のノート件数。Backstageの流速表示用。
#[tauri::command]
#[specta::specta]
pub async fn notes_since(state: State<'_, AppState>, since_epoch_secs: i32) -> Result<i32> {
    state.settings.notes_since(since_epoch_secs)
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
    let resolved = resolve_sources(&state, &column.account_id, &column.kind, &column.filter).await?;
    let notes = fetch_and_filter_multi(&state, &column.account_id, &resolved, Some(&until_id)).await?;
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

    let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
    let notes = fetch_and_filter_multi(state, &column.account_id, &resolved, None).await?;
    state.settings.cache_notes(&column.id, &notes)?;
    open_streams_only(app, state, column, &resolved, host, token);
    Ok((notes, vec![]))
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

/// 起動時のギャップ埋め: アプリを閉じていた間に流れたノートを、キャッシュの最新ノートid
/// (`newest_known_id`)まで REST で遡って取得する。`limit` 件、または既知のノートに追いつく
/// (取得ページの中に newest_known_id 以前のノートが現れる)まで、どちらか早い方で打ち切る。
/// ページ数にも上限(GAP_FILL_MAX_PAGES)を設け、長期間閉じていた場合の暴走取得を防ぐ。
async fn fill_gap(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    newest_known_id: &str,
    limit: i32,
) -> Result<Vec<Note>> {
    if resolved.kinds.is_empty() {
        return Ok(vec![]);
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
                continue;
            }
            any_fetched = true;
            page.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
            let oldest_this_page = page.last().map(|n| n.id.clone());
            for n in page {
                if n.id.as_str() <= newest_known_id {
                    done[i] = true;
                    continue;
                }
                if resolved.filter.matches(&n, &ctx)
                    && !crate::filter::mute::is_muted(&n, &mute)
                    && !server_muted_note(state, account_id, &n)
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

    let mut seen = std::collections::HashSet::new();
    collected.retain(|n| seen.insert(n.id.clone()));
    collected.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    collected.truncate(limit.max(0) as usize);
    Ok(collected)
}

/// 解決済みソース群から REST 初期/過去ページを取得し、id重複除去+created_at降順マージの上、
/// フィルタ/ミュートを適用する。`cache` ソースが含まれる場合はローカルSQLite検索も合成する。
/// 個別ソースの取得失敗は他ソースの結果を活かすため無視する（TQL§複数ソースは OR 合成のため）。
async fn fetch_and_filter_multi(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    until_id: Option<&str>,
) -> Result<Vec<Note>> {
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
        if let Ok(cached) = state.settings.search_cache(&where_sql, until_id, INITIAL_LIMIT) {
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
        })
        .collect();

    // 複数ソースに同じノートが跨る場合の重複除去 + created_at 降順ソート + limit へ切り詰め
    let mut seen = std::collections::HashSet::new();
    filtered.retain(|n| seen.insert(n.id.clone()));
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    filtered.truncate(INITIAL_LIMIT as usize);
    Ok(filtered)
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
