# 時限ミュート 設計 (Issue #545)

## 背景と目的

特定ユーザーを一定期間だけミュートし、期限が来たら自動で解除されるようにする。

Misskey の `POST /mute/create` は `expiresAt`(ミリ秒の Unix 時刻。`null` は無期限)を受け付け、`mute/list` の `Muting` も `expiresAt` を返す(`src-tauri/openapi/misskey-api-doc.json`、上流 `packages/backend/src/server/api/endpoints/mute/create.ts` で確認)。期限切れの解除はサーバーが行うので、他のクライアントにも反映される。

現状の tsumugi は、サーバーミュートを `mute/list` から**読むだけ**で、ユーザーをミュートする操作自体が無い。そのため本件は「期間指定」だけでなく「ミュート作成・解除の導線」の新設を含む。

### 決定事項(ユーザー確認済み)

- 方式は**サーバーミュート**(`mute/create` + `expiresAt`)。ローカルNG(`MuteConfig`)への期限追加はしない。
- 入口は ProfileModal の「…」メニュー(新設)。フォローボタンの横にはボタンを並べない。
- 挙動は Misskey Web に揃える。期限の選択肢は 無期限 / 10分 / 1時間 / 1日 / 1週間。

## スコープ

### 含む
- ユーザーのミュート作成(期間指定)とミュート解除。
- ProfileModal の「…」メニューと、期間のサブメニュー。
- ミュート中かどうかの取得(`users/show` の `isMuted`)と、ミュート中バッジの表示。
- MiAuth 権限 `write:mutes` の追加。
- 期限切れのクライアント側での反映(同期の改善)。
- ユーザーガイドの更新。

### 含まない(YAGNI)
- 残り時間の表示(`users/show` は期限を返さない。Misskey Web も出さない)。
- ノートメニューからのミュート、設定画面のミュート一覧、リノートミュート、期限の自由入力。
- 表示中ノートの即時削除。既存のサーバーミュートと同じく「次に受信・読み込みするノートから反映」とする。
- 設定のエクスポート/インポート(ミュートはサーバー側の状態で、`MuteConfig` に変更は無い)。

## 設計

### Rust

1. **ドメイン型** `domain/mute.rs`: `MutePeriod { Indefinitely, TenMinutes, OneHour, OneDay, OneWeek }`(`specta::Type`、`camelCase`)。`fn expires_at_ms(self, now_ms: i64) -> Option<i64>` を純粋関数として持つ。
2. **API ラッパ** `api/mutes.rs`: `mute_user(client, user_id, expires_at_ms: Option<i64>)` と `unmute_user(client, user_id)`。`expiresAt` は `null` または数値で送る。
3. **コマンド** `commands/mute.rs`(`sync_server_mutes_core` が同ファイルの非公開関数のため、ここに置く): `mute_user(account_id, user_id, period: MutePeriod)` と `unmute_user(account_id, user_id)`。`specta_builder()` に登録する。戻り値は `Option<SyncMuteResult>` で、操作後の再同期の結果(再同期に失敗したときは `None`)。フロントはこの `nextExpiryAt` で期限切れの再同期タイマーを張る。
   - 現在時刻 + 期間は Rust 側で計算する。サーバーは過去の `expiresAt` を黙って無視するため、フロントの時計ずれに依存しない。
   - 成功後に `sync_server_mutes_core` を呼び、ミュート集合を丸ごと再取得する。集合への差分追加はしない(#452 / #454 / #456 の境界とロックの前提にそのまま乗るため)。解除の検出と境界の破棄も既存経路で動く。
   - 同期が失敗してもミュート操作自体は成功として返し、同期失敗はログに残す(次回の定期同期で追いつく)。
4. **`UserProfile`**: `is_muted: Option<bool>` を追加(自分自身は `None`)。`RawUserShow` に `is_muted` を足す。`tauri.gen.ts` は再生成する。
5. **MiAuth 権限** `session/miauth.rs`: `PERMISSIONS` に `write:mutes` を追加する。既存アカウントは 403(`Error::Forbidden`)になり、既存の `unwrapAcc` と Backstage の「再認証」ボタン(`2026-07-22-miauth-reauth-ui-design.md`)で案内される。
6. **期限切れの反映**: 現状の同期は起動時・アカウント追加時・6時間ごとで、期限が切れても最大6時間は古い集合が残る。
   - `fetch_muted_and_blocked` が `mute/list` の `expiresAt` を読み、`expiresAt <= 現在時刻` のエントリを**クライアントで除外**する。サーバーが期限切れを `mute/list` から消す頻度は未確認のため、サーバーの掃除の遅れに依存しない。
   - あわせて、有効なミュートのうち最も早い期限を返す。`SyncMuteResult` に `nextExpiryAt: Option<i64>` を追加する。
   - ブロック(`blocking/list`)には期限が無いので対象外。

### フロントエンド

1. **ストア** `lib/store.svelte.ts`:
   - `muteUser(accountId, userId, period)` / `unmuteUser(accountId, userId)`。`followUser` と同じ形(`unwrapAcc` + `#logFailure` + 再 throw)。
   - `#syncServerMutes` は `nextExpiryAt` を受け取り、1回限りのタイマーを張り直す。次の定期同期(6時間)より前に切れる場合だけ張り、それより先は定期同期に任せる(長い `setTimeout` の上限も避けられる)。破棄は `#serverMuteSyncTimer` と同じ箇所で行う。
2. **`ui/UserMenu.svelte`**(新設): `NoteCard` / `NoteMenu` と同じ作り(`portal` + 全画面の透明オーバーレイ)。項目の見た目は `NoteMenu` に揃える。
   - 未ミュート: 「ミュート」(`ChevronRight`)を押すと同じメニュー内でサブメニューに切り替わり、5つの期間を並べる。期間を押した時点で `muteUser` を実行して閉じる。
   - ミュート中: 「ミュート解除」。確認なしで即実行する(Misskey Web と同じ)。
3. **`ui/ProfileModal.svelte`**:
   - フォローボタンの隣に、ghost + `icon-sm` の「…」(`MoreHorizontal`)ボタンを置く。自分自身(`profile.isSelf`)では出さない。`profile.isMuted` が `null` のとき(取得できない場合)も出さない。
   - ミュート中は表示名の近くに小さな「ミュート中」バッジを出す(メニューを開かなくても状態が分かるため。不要なら外してよい)。
   - 成功したら `profile.isMuted` を更新する。楽観更新はしない(期間選択が挟まるため)。
   - 失敗は `followErr` と同じ要領で表示する。想定するエラーは `ALREADY_MUTING`(表示が古い)、レート制限(1時間に20回)、403(再認証案内)。
   - メニューは Modal より前面に出るよう、重なり順(z-index)を実装時に確認する。
   - 見た目の値は `docs/design/style-guide.md` に従い、独自の値を作らない。

### エラー処理

| 事象 | 扱い |
|---|---|
| 403(`write:mutes` 不足) | `unwrapAcc` が Backstage に「再認証」ボタンを出す |
| `ALREADY_MUTING` / `NOT_MUTING` | 表示が古かっただけなので、「ミュート中」の表示を実際の状態に直し、その旨をモーダルに表示する(メニューで解除・ミュートし直せる) |
| レート制限(1時間に20回) | エラー文言を表示 |
| 同期の失敗(作成・解除は成功済み) | ログのみ。次回の同期で追いつく |

## テスト

- **Rust 単体**: `MutePeriod::expires_at_ms`(各期間と無期限)、`mute/list` のパース(期限切れの除外、最短期限の算出、期限なしのみの場合は `None`)、`UserProfile.is_muted` が自分自身では `None`、`SyncMuteResult` の新フィールド、`mute/create` の送信ボディ(`expiresAt` が `null` / 数値)。
- **Vitest**: ProfileModal(「…」の表示条件、サブメニュー遷移、期間選択から `muteUser` の呼び出し、解除、エラー表示、バッジ)、ストアのタイマー(fake timers で張り直し・破棄・6時間より先は張らない)。
- **手動確認**: e2e の Compose 版 Misskey に対し、Xvfb 越しの実 UI で、無期限・短い期限の作成、期限切れでの自動解除、解除、権限不足時の再認証案内を確認する(Compose の手順は `e2e/README.md` を参照)。実機で未確認の項目は、そのまま未確認と記録する。
- **ドキュメント**: `docs/guide/user-guide.md` に時限ミュートの節を追加する。

## 未確認事項・リスク

- Misskey サーバーは、期限が切れたミュートをすぐには `mute/list` から消さない。Compose 版で確認したところ、期限の72秒後も `mute/list` に残り、`users/show` の `isMuted` も `true` のままで、期限の約5分後に両方から消えた(5分周期の定期ジョブとみられる)。クライアント側の除外で吸収する設計にしてある。このため、開いたままのプロフィールの「ミュート中」バッジは、サーバーが掃除する(最大約5分)まで、期限が切れても取り直しでは直らない(開き直しても同じ)。
- ストリーミング上、サーバーがミュート済みユーザーのノートを配信から除外する挙動が、期限切れ直後にいつ切り替わるかは未確認。
- Windows / Android での「…」メニューのタップ操作は、本件では実機確認しない。
