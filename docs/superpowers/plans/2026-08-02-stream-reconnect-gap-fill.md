# Stream再接続時のギャップ埋め Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** WebSocket接続が稼働中に切断→再接続した際、切断中にサーバ側で発生していたノート・通知をRESTで補完し、既存の「アプリ起動時ギャップ埋め」と同じ体験を再接続時にも提供する(Issue #147)。

**Architecture:** `stream/connection.rs` の `run_account` が「一度でも接続確立済みの状態からの再接続」を検知し、再接続確立の瞬間に各カラムのギャップ埋めをバックグラウンドタスクとして起動する。ノート系カラムは既存の `fill_gap`(SQLiteキャッシュの最新ノートidが起点)をそのまま再利用する。通知カラムはSQLiteキャッシュを持たないため、`ChannelSub` にメモリ上のウォーターマーク(`last_seen_notification_id`)を追加し、それを起点に `fill_gap` と同構造のページング関数を新設する。結果はどちらも専用イベント(既存 `ColumnGapFill` / 新規 `ColumnNotificationGapFill`)でフロントへ静かに(通知音・デスクトップ通知なしで)反映する。

**Tech Stack:** Rust (Tauri v2, tokio, tokio-tungstenite, rusqlite), TypeScript/Svelte 5 (フロントの `store.svelte.ts`), tauri-specta（イベント/コマンドのTSバインディング自動生成）。

## Global Constraints

- コミット前に必ずフィーチャーブランチ上で作業する（既に `fix/stream-reconnect-gap-fill-issue-147` 上で作業中）。
- コマンド/イベントを追加したら必ず `src-tauri/src/lib.rs` の `specta_builder()` に登録する（`frontend/src/bindings/tauri.gen.ts` の自動生成対象にするため）。
- `cargo tauri dev` / `cargo run` は直接使わない。ビルド確認は `cargo build` / `cargo check` / `cargo test` を使う。
- コミットメッセージは件名のみ（本文・箇条書きなし）。Co-Authored-By トレイラーは別途付与される。
- 新規コードのコメントは「なぜ」（非自明な制約・注意点）のみ。「何をしているか」の説明は書かない。

---

### Task 1: 通知ウォーターマーク（`ChannelSub.last_seen_notification_id`）

**Files:**
- Modify: `src-tauri/src/stream/connection.rs`

**Interfaces:**
- Produces: `ChannelSub.last_seen_notification_id: Option<String>`（フィールド）、`fn update_last_seen_notification_id(current: &mut Option<String>, new_id: &str)`（このタスクで完結する純粋関数）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/stream/connection.rs` の `mod tests`（ファイル末尾、既存 `decode_reaction` 系テストの近く）に追加：

```rust
    #[test]
    fn update_last_seen_notification_id_keeps_max_and_ignores_regressions() {
        let mut cur: Option<String> = None;
        update_last_seen_notification_id(&mut cur, "9tj000001");
        assert_eq!(cur.as_deref(), Some("9tj000001"));
        // 古いidが後から来ても後退しない（順序が入れ替わって届くケースの保険）
        update_last_seen_notification_id(&mut cur, "9tj000000");
        assert_eq!(cur.as_deref(), Some("9tj000001"));
        update_last_seen_notification_id(&mut cur, "9tj000005");
        assert_eq!(cur.as_deref(), Some("9tj000005"));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test update_last_seen_notification_id -- --nocapture`
Expected: FAIL（`update_last_seen_notification_id` が存在しないためコンパイルエラー）

- [ ] **Step 3: 最小実装を書く**

`ChannelSub` 構造体定義（290行目付近）を以下に変更：

```rust
/// 購読中の 1 チャンネル(=1カラム)。sub_id は接続ごとに振り直す。
struct ChannelSub {
    sub_id: String,
    /// このチャンネル購読が属するカラム（TQL複数ソースでは複数の ChannelSub が同じ column_id を持つ）
    column_id: String,
    channel: String,
    params: Value,
    mode: StreamMode,
    dedup: Dedup,
    /// 通知カラムのみ使用: 直近に受信した通知id。再接続時のギャップ埋め(Issue #147)の
    /// ウォーターマークとして使う（通知はSQLiteキャッシュを持たないためメモリ上で保持する）。
    last_seen_notification_id: Option<String>,
}
```

`decode_reaction` 関数の直前（822行目付近、`reaction_event_key` 関数の前でも可）に純粋関数を追加：

```rust
/// 通知ウォーターマークを、より新しい(=大きい)idの場合だけ更新する。Misskeyのidは
/// 辞書順ソート可能なULID系のため文字列比較でよい。順序が入れ替わって届いても後退しない。
fn update_last_seen_notification_id(current: &mut Option<String>, new_id: &str) {
    if current.as_deref().map(|c| new_id > c).unwrap_or(true) {
        *current = Some(new_id.to_string());
    }
}
```

この時点では `ChannelSub` の構築箇所（`AddChannel` ハンドラ）がコンパイルエラーになるため、既存の2箇所（`open_channel`/`open_notifications` から到達する `AddChannel` コマンド処理、557行目付近）の `ChannelSub { ... }` リテラルに `last_seen_notification_id: None,` を追加する：

```rust
                        subs.insert(sub_key, ChannelSub {
                            sub_id: sub_id.clone(),
                            column_id: column_id.clone(),
                            channel: channel.clone(),
                            params: params.clone(),
                            mode,
                            dedup: Dedup::new(DEDUP_CAPACITY),
                            last_seen_notification_id: None,
                        });
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test update_last_seen_notification_id -- --nocapture`
Expected: PASS

- [ ] **Step 5: ビルド全体を確認する**

Run: `cd src-tauri && cargo build`
Expected: 成功（`ChannelSub` の他の構築箇所がないこと、未使用フィールド警告が出ないことを確認）

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/stream/connection.rs
git commit -m "feat: 通知カラムの再接続ギャップ埋め用ウォーターマークを追加"
```

---

### Task 2: 受信通知でウォーターマークを更新する

**Files:**
- Modify: `src-tauri/src/stream/connection.rs`

**Interfaces:**
- Consumes: Task 1 の `ChannelSub.last_seen_notification_id`, `update_last_seen_notification_id`
- Produces: `handle_text` が `Incoming::ChannelNotification` を処理するたびにウォーターマークを更新する（副作用のみ、新規公開インターフェースなし）

- [ ] **Step 1: `handle_text` の `Incoming::ChannelNotification` アームを変更する**

`handle_text` 関数内（714行目付近）の該当ブロックを変更する。変更前：

```rust
        Incoming::ChannelNotification { channel_id, notification } => {
            let Some(sub_key) = sub_index.get(&channel_id) else {
                return HandleResult::None;
            };
            let Some(sub) = subs.get_mut(sub_key) else {
                return HandleResult::None;
            };
            let column_id = sub.column_id.clone();
            if !sub.dedup.accept(&notification.id) {
                return HandleResult::None;
            }
            let account_id = sub.mode.account_id().to_string();
```

変更後：

```rust
        Incoming::ChannelNotification { channel_id, notification } => {
            let Some(sub_key) = sub_index.get(&channel_id) else {
                return HandleResult::None;
            };
            let Some(sub) = subs.get_mut(sub_key) else {
                return HandleResult::None;
            };
            let column_id = sub.column_id.clone();
            if !sub.dedup.accept(&notification.id) {
                return HandleResult::None;
            }
            update_last_seen_notification_id(&mut sub.last_seen_notification_id, &notification.id);
            let account_id = sub.mode.account_id().to_string();
```

このタスクは既存の `handle_text` を直接呼ぶユニットテストがコードベースに存在しない（`AppHandle` を要求するため）。ロジックはTask 1で検証済みの純粋関数を呼ぶだけなので、ここではビルド確認のみとする。

- [ ] **Step 2: ビルドを確認する**

Run: `cd src-tauri && cargo build`
Expected: 成功

- [ ] **Step 3: 既存テストが壊れていないことを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全てPASS

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/stream/connection.rs
git commit -m "feat: 通知受信のたびに再接続ギャップ埋め用ウォーターマークを更新"
```

---

### Task 3: 新規イベント `ColumnNotificationGapFill`

**Files:**
- Modify: `src-tauri/src/events.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `frontend/src/bindings/tauri.gen.ts`（`cargo test` により自動生成、手動編集しない）

**Interfaces:**
- Produces: `crate::events::ColumnNotificationGapFill { column_id: String, notifications: Vec<Notification> }`（`tauri_specta::Event` 実装済み）、フロント側 `events.columnNotificationGapFill`（自動生成）

- [ ] **Step 1: イベント型を追加する**

`src-tauri/src/events.rs` の `ColumnNotification` 定義（25〜31行目）の直後に追加：

```rust
/// 再接続時の通知ギャップ埋め結果をまとめて反映する(Issue #147)。ノートの ColumnGapFill と
/// 同じ設計判断で、通知音・デスクトップ通知は鳴らさない（瞬断中に溜まった通知で誤爆しないため）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnNotificationGapFill {
    pub column_id: String,
    pub notifications: Vec<Notification>,
}
```

- [ ] **Step 2: `specta_builder()` に登録する**

`src-tauri/src/lib.rs` の98〜102行目付近、既存リストに1行追加：

```rust
            events::ColumnNote,
            events::ColumnNoteUpdated,
            events::ColumnNotification,
            events::ColumnConnectionState,
            events::ColumnGapFill,
            events::ColumnNotificationGapFill,
```

- [ ] **Step 3: ビルドしてTSバインディングを再生成する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `columnNotificationGapFill` と `ColumnNotificationGapFill` 型が追加されていることを `git diff frontend/src/bindings/tauri.gen.ts` で確認する。

- [ ] **Step 4: 全体テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: 全てPASS

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/events.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: 通知の再接続ギャップ埋めイベントColumnNotificationGapFillを追加"
```

---

### Task 4: ノート系カラムの再接続ギャップ埋め

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/stream/connection.rs`

**Interfaces:**
- Consumes: 既存 `fill_gap(state: &AppState, account_id: &str, resolved: &ResolvedSources, newest_known_id: &str, limit: i32) -> Result<Vec<Note>>`、既存 `resolve_sources`、既存 `load_column`、`state.cache.load_cached(column_id: &str, limit: u32) -> Result<Vec<Note>>`、`state.settings.load_ui()`、Task 1の `ChannelSub`
- Produces: `pub(crate) async fn gap_fill_on_reconnect(app: &AppHandle, column_id: &str)`（`commands::column` モジュール）、`stream/connection.rs` 内の `is_reconnect: bool` 引数・`ever_connected` フラグ・`spawn_reconnect_gap_fill` 関数（このタスクではNotesのみ処理）

- [ ] **Step 1: `commands/column.rs` に `gap_fill_on_reconnect` を追加する**

`fill_gap` 関数（767行目、`Ok(collected)` で終わる関数）の直後に追加：

```rust
/// Stream再接続時のノートギャップ埋め(Issue #147)。起動時(resume_column)と同じ fill_gap を
/// 使い、SQLiteキャッシュの最新ノートidを起点にRESTで遡って補完する。初回接続では呼ばれない
/// 前提（呼び出し判定は stream/connection.rs の is_reconnect 側で行う）。
pub(crate) async fn gap_fill_on_reconnect(app: &AppHandle, column_id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
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
    let Ok(cached) = state.cache.load_cached(&column.id, 1) else {
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
    let Ok(gap_notes) =
        fill_gap(&state, &column.account_id, &resolved, &newest_known_id, gap_limit).await
    else {
        return;
    };
    if gap_notes.is_empty() {
        return;
    }
    let _ = state.cache.cache_notes(&column.id, &gap_notes);
    let _ = crate::events::ColumnGapFill {
        column_id: column.id.clone(),
        notes: gap_notes,
    }
    .emit(app);
}
```

- [ ] **Step 2: ビルドを確認する（この時点では未呼び出しの警告が出る想定）**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: `gap_fill_on_reconnect` は `pub(crate)` だが未使用のため `dead_code` 警告が出る可能性がある（エラーではない）。Step 3〜4で呼び出し側を追加すれば消える。

- [ ] **Step 3: `stream/connection.rs` に再接続検知プラミングを追加する**

`run_account` 関数（397〜457行目）を変更する。変更前の該当部分：

```rust
    let mut reaction_event_dedup = Dedup::new(DEDUP_CAPACITY);
    let mut backoff = BACKOFF_START;

    loop {
        if *cancel.borrow() {
            return;
        }
        emit_state_all(&app, &subs, ConnectionState::Connecting);

        let mut connected = false;
        let outcome = connect_and_run(
            &app,
            &account_id,
            &host,
            &token,
            &mut subs,
            &mut sub_index,
            &mut captures,
            &mut reaction_event_dedup,
            &mut cancel,
            &mut cmd_rx,
            &mut connected,
        )
        .await;

        // 一度でも接続確立していれば、次の切断はネットワークの一過性の問題である
        // 可能性が高いのでバックオフを初期値へ戻す（そうしないと長時間安定接続した
        // 後の再接続まで無関係に長い待ち時間を引きずってしまう）。
        if connected {
            backoff = BACKOFF_START;
        }
```

変更後：

```rust
    let mut reaction_event_dedup = Dedup::new(DEDUP_CAPACITY);
    let mut backoff = BACKOFF_START;
    // 一度でも接続確立した後の再接続かどうか(Issue #147)。true のときだけ
    // 再接続ギャップ埋めを行う（初回接続時は open_stream_and_fetch 側のREST初期取得で足りる）。
    let mut ever_connected = false;

    loop {
        if *cancel.borrow() {
            return;
        }
        emit_state_all(&app, &subs, ConnectionState::Connecting);

        let mut connected = false;
        let outcome = connect_and_run(
            &app,
            &account_id,
            &host,
            &token,
            &mut subs,
            &mut sub_index,
            &mut captures,
            &mut reaction_event_dedup,
            &mut cancel,
            &mut cmd_rx,
            &mut connected,
            ever_connected,
        )
        .await;

        // 一度でも接続確立していれば、次の切断はネットワークの一過性の問題である
        // 可能性が高いのでバックオフを初期値へ戻す（そうしないと長時間安定接続した
        // 後の再接続まで無関係に長い待ち時間を引きずってしまう）。
        if connected {
            backoff = BACKOFF_START;
            ever_connected = true;
        }
```

`connect_and_run` の関数シグネチャ（459〜472行目）に引数を追加：

```rust
#[allow(clippy::too_many_arguments)]
async fn connect_and_run(
    app: &AppHandle,
    account_id: &str,
    host: &str,
    token: &str,
    subs: &mut HashMap<String, ChannelSub>,
    sub_index: &mut HashMap<String, String>,
    captures: &mut CaptureSet,
    reaction_event_dedup: &mut Dedup,
    cancel: &mut watch::Receiver<bool>,
    cmd_rx: &mut mpsc::Receiver<AccountCommand>,
    connected: &mut bool,
    is_reconnect: bool,
) -> RunOutcome {
```

接続確立直後（518〜519行目）に呼び出しを追加。変更前：

```rust
    emit_state_all(app, subs, ConnectionState::Connected);
    *connected = true;
```

変更後：

```rust
    emit_state_all(app, subs, ConnectionState::Connected);
    *connected = true;
    if is_reconnect {
        spawn_reconnect_gap_fill(app, subs);
    }
```

- [ ] **Step 4: `spawn_reconnect_gap_fill` を追加する（Notesのみ）**

`apply_capture_add` 関数（637行目付近）の直前に追加：

```rust
/// 再接続確立時、切断中に届いていたはずのノート/通知をRESTで補完する(Issue #147)。
/// 再接続ループ自体をブロックしないようバックグラウンドタスクとして起動する。
fn spawn_reconnect_gap_fill(app: &AppHandle, subs: &HashMap<String, ChannelSub>) {
    let mut seen_columns = HashSet::new();
    for sub in subs.values() {
        if let StreamMode::Notes { .. } = &sub.mode {
            // TQL複数ソースでは複数の ChannelSub が同じ column_id を持つため重複起動しない
            if seen_columns.insert(sub.column_id.clone()) {
                let app = app.clone();
                let column_id = sub.column_id.clone();
                tauri::async_runtime::spawn(async move {
                    crate::commands::column::gap_fill_on_reconnect(&app, &column_id).await;
                });
            }
        }
    }
}
```

- [ ] **Step 5: ビルドと既存テストを確認する**

Run: `cd src-tauri && cargo build && cargo test`
Expected: 成功、全テストPASS（`dead_code` 警告が消えていること）

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/stream/connection.rs
git commit -m "feat: ノート系カラムのStream再接続時ギャップ埋めを追加"
```

---

### Task 5: 通知カラムの再接続ギャップ埋め

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/stream/connection.rs`

**Interfaces:**
- Consumes: Task 1の `ChannelSub.last_seen_notification_id`、Task 3の `crate::events::ColumnNotificationGapFill`、既存 `fetch_notifications(client: &MisskeyClient, limit: u32, until_id: Option<&str>) -> Result<Vec<Notification>>`、既存 `filter_notifications(state: &AppState, account_id: &str, raw: Vec<Notification>) -> Vec<Notification>`、既存 `GAP_FILL_PAGE_SIZE` / `GAP_FILL_MAX_PAGES`、Task 4の `spawn_reconnect_gap_fill`
- Produces: `pub(crate) async fn notification_gap_fill_on_reconnect(app: &AppHandle, column_id: &str, last_seen_id: &str)`

- [ ] **Step 1: `commands/column.rs` に `notification_gap_fill_on_reconnect` を追加する**

Task 4で追加した `gap_fill_on_reconnect` の直後に追加：

```rust
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
```

- [ ] **Step 2: `spawn_reconnect_gap_fill` に通知アームを追加する**

`src-tauri/src/stream/connection.rs` の `spawn_reconnect_gap_fill`（Task 4で追加）を変更する。変更前：

```rust
fn spawn_reconnect_gap_fill(app: &AppHandle, subs: &HashMap<String, ChannelSub>) {
    let mut seen_columns = HashSet::new();
    for sub in subs.values() {
        if let StreamMode::Notes { .. } = &sub.mode {
            // TQL複数ソースでは複数の ChannelSub が同じ column_id を持つため重複起動しない
            if seen_columns.insert(sub.column_id.clone()) {
                let app = app.clone();
                let column_id = sub.column_id.clone();
                tauri::async_runtime::spawn(async move {
                    crate::commands::column::gap_fill_on_reconnect(&app, &column_id).await;
                });
            }
        }
    }
}
```

変更後：

```rust
fn spawn_reconnect_gap_fill(app: &AppHandle, subs: &HashMap<String, ChannelSub>) {
    let mut seen_columns = HashSet::new();
    for sub in subs.values() {
        match &sub.mode {
            StreamMode::Notes { .. } => {
                // TQL複数ソースでは複数の ChannelSub が同じ column_id を持つため重複起動しない
                if seen_columns.insert(sub.column_id.clone()) {
                    let app = app.clone();
                    let column_id = sub.column_id.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::commands::column::gap_fill_on_reconnect(&app, &column_id).await;
                    });
                }
            }
            StreamMode::Notifications { .. } => {
                // 再接続までに一度も通知を受信していなければウォーターマークが無く、
                // 補完すべき範囲が定まらないためスキップする。
                if let Some(last_seen_id) = sub.last_seen_notification_id.clone() {
                    let app = app.clone();
                    let column_id = sub.column_id.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::commands::column::notification_gap_fill_on_reconnect(
                            &app,
                            &column_id,
                            &last_seen_id,
                        )
                        .await;
                    });
                }
            }
        }
    }
}
```

- [ ] **Step 3: ビルドと既存テストを確認する**

Run: `cd src-tauri && cargo build && cargo test`
Expected: 成功、全テストPASS

- [ ] **Step 4: clippy を確認する**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: 警告・エラーなし（既存の `#[allow(clippy::too_many_arguments)]` が必要な関数には既に付与済みであることを確認。`notification_gap_fill_on_reconnect` は引数3つなので不要）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/stream/connection.rs
git commit -m "feat: 通知カラムのStream再接続時ギャップ埋めを追加"
```

---

### Task 6: フロントエンド — `columnNotificationGapFill` の反映

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: Task 3で生成された `events.columnNotificationGapFill`（ペイロード: `{ columnId: string; notifications: Notification[] }`）、既存 `this.#findTab(columnId)`、既存 `tab.notifications: Notification[]`、既存 `MAX_NOTES`

- [ ] **Step 1: 既存の `columnGapFill` 購読パターンを確認する**

`frontend/src/lib/store.svelte.ts` 793〜807行目の既存実装（ノート版）を参考にする。このパターンをそのまま通知版として複製する。

- [ ] **Step 2: `columnNotificationGapFill` の購読を追加する**

`#subscribe` メソッド内、既存の `columnGapFill` 購読ブロック（793〜807行目）の直後、`columnConnectionState` 購読ブロック（808行目）の前に追加：

```typescript
    this.#unlisten.push(
      await events.columnNotificationGapFill.listen((e) => {
        const tab = this.#findTab(e.payload.columnId);
        if (!tab) return;
        // Stream再接続時のギャップ埋め結果(Issue #147)。ColumnGapFill(ノート)と同じ理由で、
        // 通知音/デスクトップ通知は鳴らさない（瞬断中に溜まった通知で誤爆しないため）。
        const known = new Set(tab.notifications.map((n) => n.id));
        const merged = [...tab.notifications];
        for (const n of e.payload.notifications) {
          if (!known.has(n.id)) merged.push(n);
        }
        merged.sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
        tab.notifications = merged.slice(0, MAX_NOTES);
      }),
    );
```

- [ ] **Step 3: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし（`events.columnNotificationGapFill` の型が `tauri.gen.ts` から正しく解決されることを確認）

- [ ] **Step 4: コミット**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: 通知カラムの再接続ギャップ埋め結果をフロントへ反映"
```

---

### Task 7: 最終確認

**Files:** なし（検証のみ）

**Interfaces:**
- Consumes: Task 1〜6 の全成果物

- [ ] **Step 1: Rust側フルテスト**

Run: `cd src-tauri && cargo test`
Expected: 全てPASS

- [ ] **Step 2: Rust側 clippy**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: 警告・エラーなし

- [ ] **Step 3: フロント側型チェック**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 4: 手動確認（実アプリでの動作確認）**

`cargo tauri dev` でアプリを起動し、以下を確認する：
1. Home等、ノートが流れるカラムを開いた状態で、ネットワークを切断する（Wi-Fi OFF、または `WEBKIT_DISABLE_DMABUF_RENDERER=1` 環境ならOS側のネットワークアダプタを一時無効化）。
2. Misskey側（ブラウザ等の別クライアント）で新しいノート・自分宛の通知を発生させる。
3. ネットワークを再接続する。Backstageログに「再接続しました」と出た後、切断中に発生したノート・通知がカラムに反映されることを確認する（新着通知音は鳴らないこと）。
4. `note_count` などで確認できるなら、SQLiteキャッシュにも反映されていることを確認する。

この手順は自動テストで代替できない（実ネットワーク切断が必要なため）。結果をユーザーに報告する。

- [ ] **Step 5: 差分全体を確認する**

Run: `git diff main --stat`
Expected: 意図した8ファイル程度の変更のみ（`src-tauri/src/events.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/commands/column.rs`, `src-tauri/src/stream/connection.rs`, `frontend/src/lib/store.svelte.ts`, `frontend/src/bindings/tauri.gen.ts`, `docs/superpowers/specs/...`, `docs/superpowers/plans/...`）
