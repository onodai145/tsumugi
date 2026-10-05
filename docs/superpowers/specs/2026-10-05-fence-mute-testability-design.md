# 世代フェンス・ミュート境界の、テスト可能性とテスト追加 設計 (Issue #456 P3)

#446 / PR #451(カラムごとの世代フェンス)、#452 / PR #453(`set_mute` と実行中の取得の競合)、#454 / PR #455(サーバー側ミュートの解除の検出)の最終レビューで残った、「テストで固定できていない」項目を、テストで固定する。本番の挙動は変えない。変えるのは、テストから呼べるようにするための、private 関数の切り出しと、テスト専用の継ぎ目だけ。

## 背景

`ColumnFence` 自体と、`commit_*` ヘルパーは、単体テストがある。残っているのは、それらを呼び出す側(コマンド)の「配線」と、外部(DB、HTTP)が絡む部分で、次の5項目(#456 のチェックリストから)。

| # | 項目 | なぜ、いま固定できていないか |
|---|---|---|
| 1 | `update_column` / `close_column` / `open_stream_and_fetch` の、古い場合の分岐 | `AppHandle`(`update_column`、`open_stream_and_fetch`)と `State`(`close_column`)が要る。さらに `AppState::client_for` が `https://{host}/api` 固定で、モック HTTP に向けられない |
| 2 | `apply_mute_config` の「`state.mute` を差し替えてから境界を捨てる」順序 | 境界を捨てる処理がクロージャとして内部に埋まっていて、差し込めない。順序を入れ替える回帰は、静かに不変条件(#452)を壊す |
| 3 | 境界の破棄(DB)が失敗したとき、保存値を更新しない | DB の失敗を、単体テストで作れていない |
| 4 | `sync_server_mutes` のラッパ(`State` の取り出し) | `tauri::State` を作れていない |
| 5 | Postgres / MySQL の実 DB で、`clear_all_fetch_boundaries` を境界の書きロックの中で実行する挙動 | SQLite でしか、ロックの中の実行を確認していない |

## スコープ

- 対象: 上の5項目のテスト、およびテストを可能にする最小の切り出し。
- 対象外:
  - `fetch_backfill` の境界計算とロックの競合、`update_column` が `invalidate` の後で失敗してストリームが閉じたままになる件(#456 の P5。挙動の変更を伴う)。
  - 定期同期(#456 の P4)。
  - DB スキーマ、Tauri コマンドの署名、TS バインディング、`lib.rs` の `specta_builder()`、フロントエンド。

## 設計

### 本番コードの変更(挙動は変えない)

1. **`commands/column.rs`**
   - `open_stream_and_fetch` と `open_streams_only` を `R: Runtime` でジェネリックにする(`app: &AppHandle<R>`)。`ConnectionManager::open_channel` / `open_notifications` は、すでに `R: Runtime` でジェネリック。呼び出し元(`add_column`、`update_column`、`resume_column`)は `AppHandle<Wry>` のままで、型推論で `R = Wry` になる。
   - `update_column` の本体を `update_column_core<R: Runtime>(app: &AppHandle<R>, state: &AppState, column_id: String, kind: ColumnKind, filter: FilterQuery, title: Option<String>) -> Result<OpenedColumn>` に、`close_column` の本体を `close_column_core(state: &AppState, column_id: &str) -> Result<()>` に切り出す。コマンドは、`State` / `AppHandle` を展開して core を呼ぶだけの薄いラッパにする(`sync_server_mutes_core`、`search_cache_core` と同じ作り)。**コマンドの署名は変わらない**。
2. **`commands/mute.rs`**: `apply_mute_config_with<F, Fut>(state: &AppState, config: MuteConfig, clear_boundaries: F) -> Result<()>`(`F: FnOnce() -> Fut`、`Fut: Future<Output = Result<()>>`)を切り出す。`apply_mute_config` は、本番のクロージャ(`|| state.cache.clear_all_fetch_boundaries()`)を渡す薄いラッパ。保存 → `state.mute` の差し替え → `invalidate_boundaries` の中で `clear_boundaries` を実行、という順序と、`clear_boundaries` の失敗を無視する現状の挙動(`let _ =`)は、そのまま保つ。
3. **`state.rs`**: `#[cfg(test)]` の API ベースの上書きを足す。`AppState` に `#[cfg(test)] pub(crate) test_api_base: Mutex<Option<String>>` を持たせ、`client_for` が、設定されていれば `MisskeyClient::new_with_api_base`(既存のテスト専用コンストラクタ)で `wiremock` 向けのクライアントを返す。本番ビルドには、フィールドも分岐も存在しない。`new_for_test` で `None` に初期化する。

### 追加するテスト

すべて、実装を外すと落ちることを変異確認する。

1. **`apply_mute_config` の順序**(`commands/mute.rs`): `apply_mute_config_with` に、クロージャの中で `state.mute` が新しい設定になっていることを記録するクロージャを渡す。差し替えを `invalidate_boundaries` の後ろへ動かすと落ちる。あわせて、クロージャが呼ばれること、`invalidate_boundaries` の後に控えた世代が古くなることを確認する。
2. **境界の破棄の失敗で、保存値を更新しない**(`commands/mute.rs`): `column_source_boundary` テーブルを DROP した SQLite 接続(`SqliteBackend::new(conn)`)を、`state.cache` に差し替えて、DB の失敗を作る。前回の保存値で `acc1` のユーザーを減らして同期すると、`clear_account_boundaries` が失敗する。確認すること: 同期は `Ok` を返す、保存値は前回のまま、メモリ上のミュート集合は新しい集合に差し替わっている。
3. **`sync_server_mutes` のラッパ**(`commands/mute.rs`): `tauri::test::mock_app()` に `AppState` を `manage` し、`app.state::<AppState>()` で `State` を取り、ラッパを呼ぶ。未知のアカウントは `Err`(`unknown account`)で、何も変えない。登録済みのアカウント(`state.accounts` と `state.secrets` にテスト用に登録)と `test_api_base` + `wiremock` では、集合が反映される。
4. **`open_stream_and_fetch` の古い場合の分岐**(`commands/column.rs`): `fence.begin(column)` で `Epoch` を控え、`fence.invalidate(column, ..)` で古くした後に、その `Epoch` で `open_stream_and_fetch`(`mock_app()` の `AppHandle`、`wiremock` の `notes/local-timeline`)を呼ぶ。確認すること: `Err(Error::Invalid(..))`、キャッシュにノートが入っていない、境界が書かれていない、`state.connections.open_count()` が 0。古くなければ(新しい `Epoch`)、ノートと境界が書かれる(ストリームは開く。非ストリーミングのソースで確認する)。
5. **`update_column_core` / `close_column_core` の配線**(`commands/column.rs`):
   - `close_column_core`: 実行前に控えた `Epoch` は、実行後に古くなる(`write_if_current` が `None`)。フェンスのエントリが消える(P1 の `tracks`)。カラムのキャッシュが消え、定義が消える。
   - `update_column_core`: 新しい定義が保存される。実行前に控えた `Epoch` は、実行後に古くなる。旧フィルタで貯めたキャッシュが消える。成功経路は、ストリームを開かない `Tag` ソース(`notes/search-by-tag`、`wiremock`)で確認する。
6. **実 DB: 境界の書きロックの中での `clear_all_fetch_boundaries`**(`store/postgres_backend.rs`、`store/mysql_backend.rs`。`#[ignore]`、既存の `TestBackend` を使う): 次の流れを、実 DB で確認する。
   - 複数カラムに境界を置く。
   - 読みロックを持つ書き込み(`write_if_current` の中で `extend_fetch_boundaries`)を、複数、並行して走らせる。
   - その最中に `invalidate_boundaries(|| backend.clear_all_fetch_boundaries())` を実行する。
   - タイムアウト内に完了する(接続プールの枯渇やデッドロックが無い)。完了後、全カラムの境界が空。`invalidate_boundaries` より前に控えた世代の書き込みは、`boundaries_ok` が偽(境界を書かない)。
   - 実行: `cd src-tauri && cargo test --lib postgres_ -- --ignored` と `mysql_ -- --ignored --test-threads=2`(CLAUDE.md のとおり、並列度を下げる)。

### 継ぎ目の方針

- 本番の関数の署名を変えるのは、private な `open_stream_and_fetch` / `open_streams_only` のジェネリック化と、新しい private な `*_core` / `*_with` だけ。他モジュールが依存する公開の署名は変えない。
- `#[cfg(test)]` の分岐は、`client_for` の1か所だけ。`MisskeyClient::new_with_api_base` と同じ流儀(テスト専用の継ぎ目として、既にある)。

## 検討した代替案

- **コマンド自体を `R: Runtime` でジェネリックにする**: コマンドの署名が変わり、`specta_builder()` での登録(`::<tauri::Wry>`)と、バインディングの再生成の確認が要る。薄いラッパ + core の方が、変更が閉じる。
- **`apply_mute_config` の順序を、ログや `ColumnFence` の観測で確認する**: `state.mute` を `Arc` に変えるなど、本番の型の変更が要る。クロージャを差し込む方が小さい。
- **DB の失敗を、バックエンドのデコレータで作る**: `NoteCacheBackend` の全メソッドの委譲(約12個)が要る。テーブルを DROP した本物の SQLite の方が、ボイラープレートが無く、実際の SQL エラーになる。
- **実 DB のロックのテストを、SQLite だけで済ませる**: SQLite は接続1本で、プール枯渇やサーバー側のロック待ちを起こしえない。#453 の最終レビューが指摘した、実 DB 固有のリスクを確認できない。

## リスク

- `update_column_core` の成功経路のテストで、`Tag` ソース以外(ストリーミングあり)を使うと、`wss://` への接続を試みるバックグラウンドタスクが残る。`Tag` に限る。`Tag` では組めない場合は、失敗・古い場合の経路に絞る(スコープを狭める判断として、実装時に台帳へ記録する)。
- 実 DB のテストは Docker が要り、時間がかかる。`#[ignore]` なので、通常の `cargo test` には含まれない。ローカルのイメージ(`postgres:11-alpine`、`mysql:8.1`)で実行できることを、実装時に確認する。
- 並行のテストは、時間に依存しないよう、順序を `Notify` や既存のロック(`write_if_current` の読みロック)で制御する。タイムアウトは「完了しない」ことの確認ではなく、デッドロック検出の上限としてだけ使う(長めの値にする)。

## 影響範囲

- 変更するファイル: `src-tauri/src/commands/column.rs`、`src-tauri/src/commands/mute.rs`、`src-tauri/src/state.rs`、`src-tauri/src/store/postgres_backend.rs`(テストのみ)、`src-tauri/src/store/mysql_backend.rs`(テストのみ)。
- DB スキーマ、Tauri コマンドの署名、TS バインディング、`lib.rs`、フロントエンドは変わらない。本番の挙動は変わらない。

## 後続(本設計では扱わない)

- #456 の P5: `fetch_backfill` の境界計算の競合、`update_column` 失敗時にストリームが閉じたままになる件。本設計の `update_column_core` が、その修正のテストの足場になる。
- #456 の P4: 定期同期。
