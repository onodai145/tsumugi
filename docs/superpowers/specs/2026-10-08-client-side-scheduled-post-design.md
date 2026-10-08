# 予約投稿(クライアント側フォールバック) 設計

Issue: #60(サブプロジェクト B)。サブプロジェクト A(サーバー側予約)は PR #478 でマージ済み。設計は `docs/superpowers/specs/2026-10-07-scheduled-post-design.md`。

## 背景 / 目的

A は、本家 Misskey 2025.10.0 以降の `notes/drafts` を使うため、非対応のサーバー(2025.10.0 未満、MisskeyIO フォーク、バージョン不明)のアカウントでは予約ボタンを出していない。B では、非対応のアカウントでも予約できるように、**tsumugi が予約をローカルに保持し、時刻に自分で投稿する**。

サーバー側予約と違い、**tsumugi が起動している間しか実行できない**(ウィンドウを閉じるとアプリが終了する。トレイ常駐や自動起動は無い)。この制約を前提に、安全側(意図しない遅延投稿を出さない)に倒す。

## 合意済みの前提

- 方式は「サーバー側優先 + クライアント側フォールバック」。対応サーバーのアカウントは、従来どおりサーバー側予約を使う。
- アプリが起動していない間に予約時刻を過ぎた予約は、**自動では投稿せず、「期限切れ」として一覧に残す**。
- Android を含む**全プラットフォームで使える**ようにし、「アプリを起動している間だけ投稿されます」と注意書きを出す。Android でバックグラウンドに回って停止した場合も、期限切れとして扱う。
- アプリを閉じるときの確認ダイアログは作らない。注意書きは、予約時と予約一覧で出す。
- 実装は Rust 側に常駐スケジューラを置く(Model 層は Rust、フロントは表示と操作だけ)。
- MisskeyIO 方式の予約 API は使わない(B はあくまで tsumugi 自身の予約)。

## 仕様

### 用語

- **サーバー予約**: A の予約(`notes/drafts`)。
- **ローカル予約**: B の予約(tsumugi が保持し、自分で投稿する)。

### データ(ストア)

`store/scheduled_post.rs`(新規)に `ScheduledPostStore` を置く。`store/draft.rs` の `DraftStore` と同じ流儀にする: JSON ファイル(`app_config_dir()/scheduled_posts.json`)、メモリ上の `Mutex`、一時ファイルへ書いてから `rename`、テスト用のメモリ版。

1 件の中身:

| 項目 | 内容 |
|---|---|
| `id` | `uuid` 文字列 |
| `account_id` | 投稿するアカウント |
| `scheduled_at` | 予約日時(epoch **秒**) |
| `input` | `store::draft::DraftInput`(下書きと同じ形。本文・CW・公開範囲・添付の file ID・投票・返信/引用の表示用スナップショットなど) |
| `status` | `Pending` / `Posting` / `Expired` / `Failed` |
| `error` | `Failed` のときの理由(文字列) |
| `attempts` | 送信を試みた回数 |
| `next_attempt_at` | 再試行の予定時刻(epoch 秒)。無ければ `scheduled_at` を使う |
| `created_at` | 作成日時 |

`DraftInput` を保存する理由: 「作成欄に戻す」で返信・引用のバナー(ID だけでなくユーザー名と本文の抜粋)を復元するため。投稿時に `NoteDraft`(`api/notes.rs`)へ変換する(`reply_note.id` → `reply_id`、`quote_note.id` → `renote_id`、本文・CW は trim して空なら `None`、投票は `PollDraftSnapshot` → `PollInput`)。

アカウントを削除したら、そのアカウントのローカル予約も削除する(`commands/account.rs` の `remove_account`)。

### 状態遷移

```
Pending ──(期限が来た・猶予内)──> Posting ──成功──> (削除)
   │                                  │
   │                                  ├─再試行可の失敗(回数内)──> Pending(next_attempt_at を更新)
   │                                  └─それ以外の失敗──────────> Failed(error)
   └─(期限が来た・猶予超過)──> Expired
Expired / Failed ──(「今すぐ投稿」)──> Posting
```

- `Posting` は、送信を始める**前に**ストアへ書く。取り消しや「今すぐ投稿」との二重送信を防ぐため、遷移は `Mutex` を保持したまま行う(同じ予約が `Posting` なら、他の操作は拒否する)。
- 起動時に `Posting` のまま残っている予約は、結果が分からない(送信後に終了した可能性がある)ため、**再送せず** `Failed("投稿結果が不明です。投稿されたか確認してください")` にする。

### スケジューラ

`scheduler.rs`(新規)。`setup` で `AppState` を管理に載せた後に、常駐タスクを 1 つ起動する。

- ループ: 「次の期限まで」または「ストアが変わった通知(`tokio::sync::Notify`)」のどちらか早い方まで待つ。ただし、スリープ復帰や時計の変更に備え、**最大 30 秒**で必ず起き直す。
- 1 回の処理(`process_due(state, now_sec)`)を、ループから切り離した関数にする(テストで `now_sec` を渡せるようにする)。期限が来た `Pending`(`max(scheduled_at, next_attempt_at) <= now`)を、予約日時の古い順に 1 件ずつ処理する。
- **猶予**: `now - scheduled_at <= 300` 秒(5 分)なら投稿する。超えていたら `Expired` にして投稿しない(定数 `MISSED_GRACE_SEC = 300`)。この規則は起動直後の最初の処理にも同じように適用する(起動の 2 分前が期限なら投稿し、2 時間前なら期限切れにする)。時計のずれやスリープ復帰を吸収するための猶予で、「期限切れは自動投稿しない」と整合する範囲に留める。
- 投稿は既存の `api::notes::create_note` を、そのアカウントのクライアント(`AppState::client_for`)で呼ぶ。
- **失敗の分類**(投稿が既に成功している可能性を考える):

  | エラー | 扱い |
  |---|---|
  | `Network` で、接続自体に失敗したもの(`connect:` で始まるメッセージ) | **再試行**(確実に届いていない)。間隔 30 秒・60 秒・120 秒、最大 3 回。使い切ったら `Failed` |
  | `RateLimited`(429) | **再試行**(処理されていない)。同上 |
  | `Network` のタイムアウト(`timeout:`) | **再試行しない**。`Failed("通信がタイムアウトしました。投稿されたか確認してください")`。サーバー側で投稿が成功している可能性があり、再送すると重複投稿になる |
  | `Network` のその他(`connect:` でも `timeout:` でもないメッセージ) | **再試行しない**。`Failed`(届いたかどうか分からないため、重複投稿を避ける) |
  | `Unauthorized` / `Forbidden` / `NotFound` / `Api` ほか | **再試行しない**。`Failed`(エラーコードを含めた理由) |

  接続失敗とタイムアウトの区別は、`error.rs` の `From<reqwest::Error>` が付ける接頭辞(`connect:` / `timeout:`)に依存する。この依存はテストで固定する(接頭辞が変わると再試行の判定が変わるため)。

- 結果の通知: 成功したら `ScheduledPostPosted`、`Failed` になったら `ScheduledPostFailed` のイベントを出す(下記)。

### command

`commands/scheduled.rs` に追加する(A の 4 つは変更しない)。時刻はすべて epoch 秒。

- `schedule_note_local(account_id, input: DraftInput, scheduled_at: u32) -> LocalScheduledNote`: 予約を作る。`scheduled_at` が現在以前なら `Error::Invalid`。作成後にスケジューラを起こす。
- `list_local_scheduled_notes(account_id) -> Vec<LocalScheduledNote>`: そのアカウントのローカル予約を、予約日時の昇順で返す(件数が少ないのでページングしない)。
- `cancel_local_scheduled_note(account_id, id)`: 予約を削除する。`Posting` のときは `Error::Invalid("投稿処理中のため取り消せません")`。
- `post_local_scheduled_now(account_id, id)`: `Expired` / `Failed` の予約を、今すぐ 1 回だけ送る(再試行しない)。成功したら削除し、失敗したら `Failed` にして理由を更新する。`Pending` / `Posting` には使えない。

型(`domain/`、`specta::Type`):

```text
LocalScheduledNote { note: ScheduledNote, status: LocalScheduleStatus, error: Option<String> }
LocalScheduleStatus = "pending" | "posting" | "expired" | "failed"
```

`note` は A の `ScheduledNote`(`id` はローカル予約の ID、`scheduled_at` は秒)を再利用する。これにより、A の「作成欄に戻す」の処理(`restoreScheduled`)をそのまま使える。

### イベント(`events.rs`)

`tauri_specta::Event`。`specta_builder()` の `collect_events!` に登録する。

- `ScheduledPostPosted { account_id, id, note_id }`
- `ScheduledPostFailed { account_id, id, message }`

### フロントエンド

#### 経路の振り分け(`ComposeBar.svelte`)

A では、非対応のアカウントで予約ボタンを隠していた。B では**全アカウントで予約ボタンを出し**、`get_schedule_capabilities` の結果は「サーバー予約かローカル予約か」の経路選択に使う。

- `available: true` → `schedule_note`(サーバー予約、従来どおり)。
- `available: false` → `schedule_note_local`。入力は `buildDraftInput()`(下書きと同じ形)を使う。
- 機能判定の取得中は、A と同じく「確認中」として予約日時つきの送信を止める(経路が決まらないため)。
- A で入れた「非対応のアカウントに切り替えたら予約日時を消す」処理は、非対応でも予約できるようになるため**外す**(それに合わせて A のテストを直す)。
- 期間指定の投票は、A と同じく予約日時を基準に締切を計算する。ローカル経路でも `buildDraftInput()` が同じ基準を使うようにする。
- ローカル経路のとき、ピッカー行に注意書き「アプリを起動している間だけ投稿されます」を出す。

#### 予約一覧(`ScheduledModal.svelte`)

- サーバー予約(対応サーバーのときだけ取得)とローカル予約(常に取得)を、予約日時の昇順で 1 つの一覧にする。サーバーのバージョンが後から上がった場合でも、残ったローカル予約が見える。
- 各行に、サーバー予約かローカル予約かの区別(ローカルは注記「アプリを起動している間だけ投稿されます」)を出す。
- ローカル予約の状態表示: `pending` は予約日時、`expired` は「期限切れ」、`failed` は「投稿に失敗」と理由、`posting` は「投稿中」。
- 操作: 「取り消し」「作成欄に戻す」は両方に共通。ローカルの `expired` / `failed` には、さらに「今すぐ投稿」を出す。`posting` の行は操作できない。
- 「作成欄に戻す」はローカル予約でも A と同じ(内容と、未来の予約なら予約日時を読み込んでから削除)。削除は `cancel_local_scheduled_note`。

#### 結果の通知

`ScheduledPostPosted` / `ScheduledPostFailed` を受けて、アプリ内のログ(トースト)で知らせる。予約一覧が開いていれば読み直す。OS 通知は出さない。

### 起動と終了

- 起動時: ストアを読み、`Posting` を `Failed` に直し、スケジューラを起動する。最初の処理で、期限が来たものを猶予の規則で処理する。
- 終了時: 何もしない(アプリが閉じれば、残った `Pending` は次の起動で期限切れまたは猶予内の投稿になる)。

### Android

共通コードのまま使う。バックグラウンドで停止してタイマーが動かなかった期間は、スケジューラが起き直した時点で猶予の規則により「投稿する」または「期限切れにする」のどちらかになる。バックグラウンドでの確実な実行は保証しない(注意書きで明示する)。

## テスト

- ストア(`store/scheduled_post.rs`): ファイルの往復、一時ファイル経由の書き込み、状態遷移、アカウント削除、壊れた JSON を読んだときの扱い(`DraftStore` の既存の流儀に合わせる)。
- スケジューラ(`scheduler.rs`、`wiremock` と `AppState::set_test_api_base`): 期限判定(期限前は投稿しない)、猶予(5 分以内は投稿・超過は `Expired`)、成功で削除、接続失敗の再試行(30/60/120 秒、使い切りで `Failed`)、429 の再試行、**タイムアウトは再試行せず `Failed`**、認証エラーは `Failed`、起動時の `Posting` の復旧、二重送信の防止(`Posting` への遷移が 1 回だけ)。`now_sec` を引数で渡して時間を進める。
- command: 過去日時の拒否、`Posting` の取り消し拒否、`post_local_scheduled_now` の状態制限。
- バインディング: 新しい command・型・イベントの生成(`generates_frontend_bindings` に追加)。
- フロント(Vitest): 経路の振り分け(`available` の true/false)、機能判定が取得中の送信ブロック、非対応アカウントでも予約ボタンが出ること、一覧の統合と並び順、状態ごとの表示と操作(期限切れの「今すぐ投稿」)、イベントでの再読み込み、注意書きの表示。
- 実機確認(必須): 非対応のサーバー(MisskeyIO 系のアカウントなど)で数分後の予約を作り、起動したままで実際に投稿されることを確認する。アプリを閉じて期限後に再起動し、期限切れとして一覧に残ることも確認する。実アカウントを使うため、実施前に了承を取る。
- ドキュメント: `docs/guide/user-guide.md` の予約投稿の節を、ローカル予約の挙動(アプリを起動している間だけ・期限切れ・今すぐ投稿)に合わせて更新する。

## 対象外

- トレイ常駐、自動起動、ウィンドウを閉じるときの確認ダイアログ(別の仕組みになる規模)。
- OS 通知。
- 対応サーバーのアカウントでのローカル予約の利用。
- 予約のインライン編集(「作成欄に戻す」で直して再予約する運用)。
- 端末間の同期。
- 非対応判定を「バージョン」から「API の 404 応答」に広げること(MisskeyIO のバージョン番号が本家以上になった場合に、サーバー予約の作成が失敗してもローカルへ切り替えない既知の限界は、A から引き継ぐ)。

## 未確定事項(実装計画で確定させる)

- `AppState` への `ScheduledPostStore` の追加の仕方(既存の `AppState::new*` のコンストラクタのシグネチャを変えて、既存テストに波及させない形にする)。
- スケジューラの起動場所と、`AppHandle` からの `AppState` の取り方、`Notify` の持ち方。
- `buildDraftInput()` に予約日時基準の投票締切を渡す形(現状は `computePollExpiresAt()` を基準なしで呼んでいる)。
- A の ComposeBar の「非対応で予約日時を消す」処理とそのテストの具体的な修正範囲。
- `ScheduledModal` の統合一覧の取得順と、サーバー予約・ローカル予約のどちらかの取得だけが失敗したときの表示。
