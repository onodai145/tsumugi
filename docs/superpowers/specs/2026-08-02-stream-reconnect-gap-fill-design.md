# Stream再接続時のギャップ埋め設計 (Issue #147)

## 背景・課題

`resume_column`（アプリ起動/カラム再開時）には既に、SQLiteキャッシュの最新ノートidを起点にRESTで遡って欠落ノートを埋める `fill_gap` の仕組みがある（`ColumnGapFill` イベントで通知音を鳴らさず静かにマージ）。

しかし `stream/connection.rs` の `run_account` は、アプリ**稼働中**にWebSocketが切断→バックオフ再接続した場合、購読チャンネルの張り直しのみを行い、切断中にサーバ側で発生していたはずのノート・通知を一切補完しない。ネットワークの瞬断・スリープ復帰・回線切替等（Issue #12のコメントにもある通り、この種の切断は前提として設計されている）のたびに、その間のノートや通知が完全に失われる。

## スコープ

- ノート系カラム（Home/Local/Hybrid/Global/List/Antenna/Channel/User/Tag/Search/TQL複数ソース）
- 通知カラム（Notifications）

いずれも対象とする。カラム初回接続時（起動直後の最初の接続）はこれまで通り既存のREST初期取得のみで、今回追加するのは「一度確立した接続が切断され、再接続に成功した」タイミングでのギャップ埋めに限定する。

## 設計

### 1. 再接続検知（`stream/connection.rs`）

`run_account` に `ever_connected: bool`（初期値 `false`）を追加する。`connect_and_run` 呼び出し時にこのフラグを渡し（`is_reconnect` 引数）、接続確立に成功した1回目の呼び出しでは `false`、2回目以降（＝過去に一度でも接続が確立していた）では `true` になる。`connect_and_run` 内で接続確立（`emit_state_all(.., Connected)` 直後）した際、`is_reconnect == true` ならギャップ埋めをトリガーする。ループ側では、そのイテレーションで `connected` が立てば `ever_connected = true` に更新する。

### 2. ノート系カラムのギャップ埋め

`commands/column.rs` に既存 `fill_gap` を再利用する新関数を追加する：

```rust
pub(crate) async fn gap_fill_on_reconnect(app: &AppHandle, column_id: &str)
```

処理内容（`resume_column` のキャッシュありパスと同一ロジック）：
1. `load_column` でカラム定義を取得（`Notifications` kindなら何もしない）
2. `resolve_sources` でソース解決
3. `state.cache.load_cached(column_id, 1)` から最新ノートidをウォーターマークとして取得（キャッシュが空なら何もしない＝そもそも初期取得が済んでいないケースなので対象外）
4. 設定の `gapFillLimit`（0なら何もしない）
5. 既存 `fill_gap` を呼び、結果を `cache_notes` に保存し、既存の `ColumnGapFill` イベントをemit（**新規イベント型は不要**、フロントエンドの `columnGapFill` ハンドラをそのまま流用できる）

`connection.rs` 側は、再接続確立時に `subs` を走査し、`StreamMode::Notes` を持つ sub の `column_id`（TQL複数ソースは重複排除）ごとに `tauri::async_runtime::spawn` でバックグラウンド実行する（再接続ループ自体はブロックしない）。

### 3. 通知カラムのギャップ埋め

通知はSQLiteキャッシュを持たないため、ウォーターマークをコネクション層のメモリ上で保持する。

- `ChannelSub` に `last_seen_notification_id: Option<String>` を追加。
- `handle_text` の `Incoming::ChannelNotification` 処理内で、受信した通知のidが現在の値より大きければ更新する（Misskeyのidは辞書順ソート可能なULID系なので文字列比較で可）。
- `commands/column.rs` に新関数を追加：

```rust
pub(crate) async fn notification_gap_fill_on_reconnect(app: &AppHandle, column_id: &str, last_seen_id: &str)
```

`fill_gap` と同構造（`until_id` で遡り、`id <= last_seen_id` で打ち切り、`GAP_FILL_PAGE_SIZE`/`GAP_FILL_MAX_PAGES`/`gapFillLimit` を流用）で `fetch_notifications` をページングし、`filter_notifications` でミュート除外した上で、新規イベント `ColumnNotificationGapFill { column_id, notifications }` をemitする。

`connection.rs` 側は、再接続確立時に `StreamMode::Notifications` を持つ sub のうち `last_seen_notification_id` が `Some` のものについて、同様にバックグラウンドタスクとして起動する（`None` の場合＝再接続までに一度も通知を受信していない場合はスキップ）。

### 4. 新規イベント `ColumnNotificationGapFill`

`events.rs` に追加：

```rust
pub struct ColumnNotificationGapFill {
    pub column_id: String,
    pub notifications: Vec<Notification>,
}
```

`lib.rs` の `specta_builder()` に登録する（CLAUDE.mdの規約通り、TSバインディング生成のため必須）。

### 5. フロントエンド (`frontend/src/lib/store.svelte.ts`)

`events.columnNotificationGapFill.listen(...)` を新規購読し、既存の `columnGapFill` ハンドラ（793-807行目）と同じパターンで実装する：
- `tab.notifications` に id 重複除去でマージ
- 通知音・デスクトップ通知は発火させない（不在中/瞬断中に溜まった通知で誤爆しないため。既存のノートギャップ埋めと同じ設計判断）
- id でソート（新しい順）して `MAX_NOTES` に切り詰め

## エラーハンドリング・境界条件

- ギャップ埋めの各ステップ（`resolve_sources` 失敗、REST失敗、キャッシュ未取得等）は既存 `fill_gap` 同様、フェイルソフトで「何もしない」（他カラムの復旧を妨げない）。
- `gapFillLimit == 0`（無効設定）なら両方ともスキップ。
- 頻繁な瞬断で再接続が連発しても、ウォーターマーク一致なら1ページのREST呼び出しで空扱いとなり早期終了するため、過大な負荷にはならない想定。
- 通知の `last_seen_notification_id` はプロセスのメモリ内のみで保持し、アプリ再起動時にはリセットされる（起動時ギャップ埋めは今回のスコープ外、既存動作を変更しない）。

## テスト

- `stream/connection.rs`: `ChannelSub` の `last_seen_notification_id` 更新ロジックの単体テスト（新規通知到着で更新、古い/同じidでは更新されない）。
- `commands/column.rs`: `notification_gap_fill_on_reconnect` のページング・打ち切り条件のテスト（可能な範囲でモック/フェイクを使用、既存 `fill_gap` のテストパターンに準拠）。
- 手動確認: `cargo tauri dev` でアプリ起動→ネットワーク切断（Wi-Fi OFF等）→再接続し、切断中に流れたノート/通知が再接続後に反映されることを確認。
