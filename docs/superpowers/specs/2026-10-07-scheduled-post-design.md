# 予約投稿(サーバー側) 設計

Issue: #60

## 背景 / 目的

指定した日時に自動で投稿されるようにしたい。Issue #60 は本文が空のため、要件は次の合意から起こした。

本機能は **2 つのサブプロジェクトの 1 つ目(A)** である。

| | 内容 | 状態 |
|---|---|---|
| **A** | サーバー側予約(本家 Misskey の予約投稿 API を使う) | 本 spec |
| B | クライアント側フォールバック(非対応インスタンス向けに tsumugi が予約を保持して送信する) | 別 spec / plan(A の後) |

方式は「サーバー側優先 + クライアント側フォールバック」で合意済み。ただし規模が大きいため A と B を分け、A の UI(日時指定・予約一覧)を土台に B を載せる。

## 合意済みの前提

- 対象は **本家 Misskey(`misskey-dev/misskey`)の API のみ**。MisskeyIO フォークの方式(`notes/create` の `scheduledAt`、`notes/scheduled/*`)は対象外。
- 予約済み投稿の一覧・取り消しは **専用モーダル**(`Modal.svelte` ベース)に置く。
- 予約済み投稿への操作は **「取り消し」と「作成欄に戻す」** まで。一覧上でのインライン編集(`drafts/update`)はしない。
- 非対応インスタンスでは予約 UI を出さない(B で代替手段を足すまでは出さないだけ)。
- 実装前に OpenAPI スナップショットを本家 2026.10.0 に更新済み(PR #477)。

## 調査結果(Misskey 本家)

`dev.misskey.omhnc.net`(2026.10.0)の `/api.json` と、上流ソース(`misskey-dev/misskey`)で確認した。

### 予約投稿の実体

予約投稿は、サーバー側の**下書き(`notes/drafts/*`)に予約情報を付けたもの**。専用の「予約」エンドポイントは無い。

| 操作 | エンドポイント | 備考 |
|---|---|---|
| 予約作成 | `notes/drafts/create` | `scheduledAt`(ミリ秒の整数)と `isActuallyScheduled: true` を渡す。レスポンスは `{ createdDraft: NoteDraft }` |
| 予約一覧 | `notes/drafts/list` | `scheduled: true` で予約のみ。`limit`(最大100、既定30)、`sinceId` / `untilId` でページング。レスポンスは `NoteDraft[]` |
| 取り消し | `notes/drafts/delete` | `draftId`。204 |
| 下書きに戻す | `notes/drafts/update` | `isActuallyScheduled: false` にする。**本機能では使わない** |

`notes/drafts/create` のパラメータは `visibility` / `visibleUserIds` / `cw` / `hashtag` / `localOnly` / `reactionAcceptance` / `replyId` / `renoteId` / `channelId` / `text`(最大3000) / `fileIds`(最大16) / `poll` / `scheduledAt` / `isActuallyScheduled`。

### バージョン

予約投稿は本家 PR #16577(2025-09-26 マージ)で入り、**2025.10.0 から**使える(2025.9.0 には含まれず、2025.10.0-alpha.0 が最初。タグとの包含関係で確認)。

### サーバー側の検証とエラー

| コード | 意味 |
|---|---|
| `TOO_MANY_SCHEDULED_NOTES` | ロールポリシー `scheduledNoteLimit` 超過 |
| `SCHEDULED_AT_REQUIRED` | `isActuallyScheduled: true` なのに `scheduledAt` が無い |
| `SCHEDULED_AT_MUST_BE_IN_FUTURE` | `scheduledAt` が過去 |
| `CANNOT_CREATE_ALREADY_EXPIRED_POLL` ほか | 通常の投稿と同じ系統(返信先なし、ブロック、禁止ワード等) |

### 実行時の挙動(重要)

予約時刻になるとサーバーが投稿する。

- **成功**: 下書きを削除し、`scheduledNotePosted` 通知を出す。
- **失敗**(返信先が消えた等): 下書きは**削除されず**、`isActuallyScheduled: true` のまま `scheduledAt` が過去になった状態で残る。`scheduledNotePostFailed` 通知(`noteDraftId` 付き)を出す。

したがって `scheduled: true` の一覧には、**時刻を過ぎて失敗した予約**も混ざる。一覧は `scheduledAt` が現在より過去のものを「投稿に失敗」として区別して表示する。

## 仕様

### 機能判定

- 既存の `get_search_capabilities`(`commands/column.rs`)と同じ型にする。サーバーバージョンを取得してキャッシュし、**バージョン ≥ 2025.10.0 なら予約可能**、バージョン不明・解析不能は不可とする。
- `domain/` に `ScheduleCapabilities { available: bool }` と判定関数を置く(`domain/search.rs` の `parse_misskey_version` を再利用)。
- バージョン取得・キャッシュ部分(`search_capabilities_for`)は予約判定と共通化する。共通化の形(関数の切り出し先)は実装計画で決める。
- 既知の限界: MisskeyIO フォークが将来バージョン番号だけ 2025.10.0 以上になっても API が本家と異なる場合、予約作成が 404 等で失敗する。その場合は通常のエラーとして表示する(特別扱いはしない)。

### Rust

追加のみで、既存の型・シグネチャは変更しない。

- `api/drafts.rs`(新規)
  - `create_scheduled(client, draft: &NoteDraft, scheduled_at: i64) -> Result<ScheduledNote>`: 既存 `NoteDraft`(`api/notes.rs`)に `scheduledAt` と `isActuallyScheduled: true` を足したリクエストを `notes/drafts/create` へ送る。Rust の command 引数と `ScheduledNote.scheduled_at` は epoch 秒(既存の `Note.created_at`・`search_server_notes` と同じ)で、HTTP 境界でミリ秒に変換する(サーバー仕様の `scheduledAt` はミリ秒の整数のまま)。
  - `list_scheduled(client, until_id: Option<&str>, limit) -> Result<Vec<ScheduledNote>>`: `scheduled: true`。
  - `delete_draft(client, draft_id) -> Result<()>`。
- `domain/scheduled.rs`(新規): `ScheduledNote`(`specta::Type`)。id、`scheduled_at`(epoch 秒)、本文、CW、公開範囲、添付(ID とサムネイル URL)、投票、返信/引用先の要約、チャンネル、`local_only`、`reaction_acceptance`。`normalize.rs` に `NoteDraft` レスポンスからの変換を追加する。
- `commands/scheduled.rs`(新規): `schedule_note` / `list_scheduled_notes` / `cancel_scheduled_note` / `get_schedule_capabilities`。`specta_builder()` と `commands/mod.rs` に登録し、`generates_frontend_bindings` テストにも新コマンド名を足す。
- 添付は既存の投稿と同じく、送信時にアップロードしてから `fileIds` を渡す(`2026-07-20-upload-on-submit-design.md`)。予約専用の経路は作らない。

### フロントエンド

#### 予約の設定(`ComposeBar.svelte`)

- 投稿ボタンの隣に予約ボタンを置く。`get_schedule_capabilities` が不可なら**非表示**。
- 押すと日時ピッカーを開く。入力はローカルタイムゾーンの日時で、送信時にミリ秒へ変換する。
- 日時が設定されている間は、投稿ボタンのラベルを「予約」に変え、押すと `schedule_note` を呼ぶ。日時の解除手段(ピッカー内のクリア)を用意する。
- **クライアント側の検証**: 過去の日時は拒否する。さらに、投票の締切(`expires_at`)が予約日時以前の場合は、投稿後すぐ期限切れになるため拒否してメッセージを出す(本家のサーバー検証には無い、tsumugi 側の追加ガード)。投票の期間指定(N 時間後)は、予約日時を基準に締切を計算する。
- 成功時は通常の投稿と同じく作成欄をクリアし、自動下書きも消す。成功を示すトースト等の通知は既存の流儀に合わせる。

#### 予約一覧(`ScheduledModal.svelte` 新規、`Modal.svelte` ベース)

- ComposeBar の予約ボタン付近から開く。アカウント別に表示する。ComposeBar で選択中のアカウントの予約を表示する(切り替えは ComposeBar 側で行う)。
- 各行に、本文の抜粋・予約日時・公開範囲を出す。`scheduledAt` が過去のものは「投稿に失敗」と明示する。
- 操作は 2 つ。
  - **取り消し**: `cancel_scheduled_note`(=`drafts/delete`)。
  - **作成欄に戻す**: 内容を ComposeBar に読み込み、読み込み成功後に `cancel_scheduled_note` でサーバー側の予約を削除する。削除に失敗した場合は、読み込んだ内容は残したまま「予約が残っており重複投稿の恐れがある」旨のエラーを出す。作成欄に既に入力がある場合の扱いは、既存の下書き読み込み(`loadDraft`)に合わせる。
- 一覧は `untilId` による追加読み込みに対応する(最大100件/回)。

#### 戻す際の項目対応

- `hashtag`(サーバー下書き専用フィールド)と `visibleUserIds` は、tsumugi の `NoteDraft` に無い。`hashtag` は上流の投稿処理(`PostScheduledNoteProcessorService`)でも使われないため無視してよい。`visibleUserIds` は、現行の投稿(公開範囲「指名」)と同じ制約の範囲で扱う(実装計画で現行の挙動を確認して決める)。

### エラー表示

本家のエラーコードを日本語メッセージに対応づける。

- `TOO_MANY_SCHEDULED_NOTES`: 予約の上限数に達している旨。
- `SCHEDULED_AT_MUST_BE_IN_FUTURE` / `SCHEDULED_AT_REQUIRED`: 日時を確認する旨(クライアント検証で通常は到達しない)。
- その他(返信先なし等)は既存の投稿エラー表示に合わせる。
- 件数上限(ロールポリシーの `scheduledNoteLimit`)は事前取得せず、サーバーのエラーに任せる。

## テスト

- Rust(`cargo test`):
  - 予約作成リクエストのシリアライズ(`scheduledAt` が整数、`isActuallyScheduled` が `true`、既定値のフィールドが省略されること)。
  - `NoteDraft` レスポンスから `ScheduledNote` への変換(投票・添付・返信先あり/なし)。
  - 機能判定の境界値: `2025.9.0` 不可 / `2025.10.0` 可 / `2026.10.0-alpha.0` 可 / `2025.4.1-io.12b-...` 不可 / 不明・解析不能は不可。
  - `generates_frontend_bindings` に新コマンド名と、`scheduledAt` の camelCase を確認するアサーションを追加。
- フロント(Vitest):
  - ComposeBar: 非対応で予約ボタンが出ない / 日時設定で投稿ボタンが「予約」になる / 過去日時と投票締切が予約日時以前の場合に拒否される / 成功で作成欄がクリアされる。
  - ScheduledModal: 一覧表示、失敗(過去)の表示、取り消し、作成欄に戻す(削除失敗時の挙動を含む)。
- 実機確認(必須): Xvfb 越しの `cargo tauri dev`(過去の取り決めどおり、`WAYLAND_DISPLAY` も unset し `dbus-run-session` を使う)で、`dev.misskey.omhnc.net` に未来時刻の予約を作り、一覧表示・取り消し・作成欄に戻すまで確認する。実アカウントを使うため、実施前にユーザーの了承を取る。
- ドキュメント: `docs/guide/user-guide.md` に予約投稿の節を追加する。

## 対象外

- B(クライアント側フォールバック)。別 spec / plan で扱う。B の設計時に、非対応判定を「バージョン」から「404 応答」に広げる案を再検討する。
- MisskeyIO 方式。
- 一覧上でのインライン編集(`drafts/update`)。
- サーバー側下書き(`notes/drafts` の `isActuallyScheduled: false`)の一覧・操作。
- `scheduledNotePosted` / `scheduledNotePostFailed` 通知の専用表示。tsumugi の通知カラムがこれらの通知種別をどう扱うかは未確認のため、フォローアップとして別途確認する。

## 未確定事項(実装計画で確定させる)

- `search_capabilities_for` の共通化の形。
- 公開範囲「指名」の `visibleUserIds` の現行の扱いと、作成欄に戻す際の対応。
- 「作成欄に戻す」で、作成欄に既に入力がある場合の挙動(`loadDraft` の既存挙動に合わせる前提だが未確認)。
- 日時ピッカーの具体的な UI 部品(`docs/design/style-guide.md` に沿う)。
