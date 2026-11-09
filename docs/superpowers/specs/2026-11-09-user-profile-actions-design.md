# ユーザープロフィールからのフォロー以外の操作 設計 (Issue #546)

## 背景と目的

ProfileModal からできる操作は、フォロー/解除と(#545 で追加した)期限付きミュートだけである。Misskey Web のユーザーメニューにある操作のうち、よく使うものを tsumugi のプロフィールからも行えるようにする。

Issue の本文は「ミュート / ブロック / リノートをミュート / 他に何があったか」。ミュート(期限付き)は #545 で実装済みなので、本件では次を足す。

- ブロック / ブロック解除
- リノートをミュート / 解除
- リストに追加 / 解除
- 通報
- リモートユーザーの元ページをブラウザで開く

### 決定事項(ユーザー確認済み)

- スコープは上の5つ。通報とリストへの追加を含むため、architectural として spec を書く。
- 入口は #545 で作った ProfileModal の「…」メニュー(`UserMenu.svelte`)を拡張する。フォローボタンの横にはボタンを並べない。
- リストへの追加は、追加と解除のトグル(自分のリスト一覧を出し、入っているものにチェックを付ける)。
- 「ブラウザで開く」は、リモートユーザーだけに出し、そのユーザーが所属するサーバー上の元ページを開く。

## スコープ

### 含む
- 上記5操作と、そのための API ラッパ・コマンド・UI。
- `users/show` の `isBlocking` / `isRenoteMuted` / `url` / `uri` の取得と、「ブロック中」バッジ。
- MiAuth 権限 `write:blocks` と `write:report-abuse` の追加。
- ブロックの作成・解除後のミュート集合の再同期(ブロックは `blocking/list` として表示抑制の集合に入っているため)。
- ユーザーガイドの更新。

### 含まない(YAGNI)
- アンテナへの追加、DM・メンションの作成、ノートメニューやカラムのユーザー名からの操作。
- リノートミュートのクライアント側フィルタ。Misskey はサーバー側でタイムラインから除外する。表示中のノートの即時削除もしない(「次に受信・読み込みするノートから反映」。既存のサーバーミュートと同じ)。
- ブロックされている側(`isBlocked`)の表示、フォローボタンの無効化。
- ローカルユーザーの「ブラウザで開く」。
- 設定のエクスポート/インポート(すべてサーバー側の状態で、設定 JSON に変更は無い)。

## 設計

### Rust

1. **`api/users.rs`**
   - `RawUserShow` に `is_blocking` / `is_renote_muted`(`Option<bool>`)と `url` / `uri`(`Option<String>`)を `#[serde(default)]` で追加する。
   - `report_abuse(client, user_id, comment)`(`users/report-abuse`)を追加する。
   - `add_to_list` / `remove_from_list`(`users/lists/push` / `users/lists/pull`、`listId` と `userId`)を追加する。
2. **`api/mutes.rs`**(ブロックは同ファイルの `blocking/list` 読み取りと対になる)
   - `block_user` / `unblock_user`(`blocking/create` / `blocking/delete`)。
   - `renote_mute_user` / `unrenote_mute_user`(`renote-mute/create` / `renote-mute/delete`)。
3. **`api/meta.rs`**: `fetch_user_lists_with_membership(client, user_id)` を追加する。`users/lists/list` を呼び、応答の `userIds` に `user_id` が含まれるかを `has_user` として計算する。既存の `fetch_user_lists` と `UserList` は変更しない(カラム追加の画面に波及させず、`userIds` を IPC に載せないため)。
4. **ドメイン型**: `UserListMembership { id, name, has_user }`(`specta::Type`、`camelCase`)。
5. **`commands/user.rs`**
   - `UserProfile` に `is_blocking` / `is_renote_muted`(`Option<bool>`、自分自身は `None`。既存の `relation_flag` を使う)と `profile_url: Option<String>` を追加する。
   - `sanitize_profile_url(host: &Option<String>, url: Option<&str>, uri: Option<&str>) -> Option<String>`(純粋関数)。ローカルユーザー(`host` が `None`)は `None`。リモートは `url` → `uri` の順で、最初に `http` / `https` として解釈できるものを返す。それ以外のスキーム(`javascript:` など)や不正な URL は読み飛ばし、どちらも無効なら `None`。値は外部サーバーが返すため信用しない。
   - コマンド: `report_user`、`list_user_lists_with_membership`、`add_user_to_list`、`remove_user_from_list`。
   - `report_user` の `comment` は Rust 側でも空(空白のみ)を拒否する。
6. **`commands/mute.rs`**
   - `block_user` / `unblock_user`: `Option<SyncMuteResult>` を返す。`mute_user_core` と同じく、`*_core(state, account_id, client, user_id)` に分けて、成功後に `resync_after_mute_change` を呼ぶ。再同期の失敗は、ブロック自体の成功を妨げない。
   - `renote_mute_user` / `unrenote_mute_user`: `()` を返す。再同期はしない。
7. **`lib.rs`**: 上記コマンドをすべて `specta_builder()` に登録する。`tauri.gen.ts` は `cargo test` / `cargo tauri dev` で再生成する。
8. **MiAuth 権限** `session/miauth.rs`: `PERMISSIONS` に `write:blocks` と `write:report-abuse` を追加する(`write:mutes` と `write:account` は既存)。既存アカウントは 403(`Error::Forbidden`)になり、既存の `unwrapAcc` と Backstage の再認証ボタンで案内される。

### フロントエンド

1. **ストア** `lib/store.svelte.ts`: `muteUser` と同じ形(`unwrapAcc` + `#logFailure` + 再 throw)で次を追加する。`blockUser` / `unblockUser`(`#beginMuteSync` / `#applyMuteSync` でタイマーを張り直す)、`renoteMuteUser` / `unrenoteMuteUser`、`reportUser`、`listUserListsWithMembership`、`addUserToList` / `removeUserFromList`。
2. **`ui/UserMenu.svelte`**
   - ビューは `root` / `period` / `lists`。props は状態(`muted` / `renoteMuted` / `blocking` / `profileUrl`)と、操作ごとのコールバック。
   - `root` の項目の順: ミュート(または解除。期間サブメニュー)、リノートをミュート(または解除)、ブロック(または解除)、リストに追加(サブメニュー)、ブラウザで開く(`profileUrl` がある時だけ。`ExternalLink` アイコン)、区切り線、通報(destructive 色)。
   - `lists` ビューは、独立コンポーネント `ui/UserListsPanel.svelte` に分ける(取得・エラー・トグルの状態を持つ。`UserMenu` は「表示と選択だけ」という既存の方針を保つ)。パネルは、開いた時に初めて `load` を呼ぶ。取得中は「読み込み中…」、失敗時はエラーと再試行、0 件は「リストがありません」を出す。各行は入っていればチェックを表示し、押すとトグルする。トグルは楽観更新せず、完了後にチェックを反映する。失敗はパネル内にエラーを出し、メニューは開いたままにする。`max-height` と内部スクロールで、リストが多くてもはみ出さないようにする。`load` / `toggle` は `ProfileModal` が渡す。
3. **確認と入力のダイアログ**
   - ブロックは既存の `ConfirmDialog`(`danger`、`z=1020`)。文面は「@acct をブロックしますか? 相手はあなたをフォローできなくなり、フォロー関係も解除されます」。ブロック解除・リノートミュート・リストのトグルは確認なしで即実行する。
   - 通報は新規 `ui/ReportUserDialog.svelte`。`ConfirmDialog` と同じ portal と z の作法で、コメントの `textarea`(必須。空の間は送信ボタンを無効)、送信、キャンセルを持つ。送信中は二重送信を防ぐ。失敗したらダイアログ内にエラーを出して開いたままにし、入力を失わない。成功したらダイアログを閉じ、プロフィール上に「通報しました」と表示する。
4. **`ui/ProfileModal.svelte`**
   - `isBlocking` / `isRenoteMuted` / `profileUrl` を受ける。「ブロック中」バッジを、「ミュート中」バッジと同じ書式で並べる。
   - 「…」ボタンの表示条件は今のまま(`isMuted !== null`)にする。4つのフラグは同じ `users/show` から来るので、自分自身では全部 `None` になる。ブロックとリノートミュートの項目は、それぞれのフラグが `null` でないときだけ `UserMenu` に出す(サーバーが返さないとき、効かない項目を出さないため)。
   - ミュート/解除・ブロック/解除・リノートミュート/解除のハンドラは、busy 制御、世代番号による古い応答の無視、`ALREADY_*` / `NOT_*` の表示補正が共通なので、ローカルのヘルパ `runUserAction` にまとめ、既存の `mute` / `unmute` も同じヘルパに移す(挙動は変えない)。
   - ブロック成功時は `isFollowing = false` にする(Misskey はブロック時にフォローを外す。実インスタンスで確認する)。
   - 「ブラウザで開く」は、ストアを介さず `openUrl(profileUrl)` を呼ぶ(`note.open` と同様)。capability は `http(s)://*` に限定済みで、Rust 側の `sanitize_profile_url` と二重に守る。
5. 見た目の値は `docs/design/style-guide.md` の既存スケールだけを使い、1回限りの値は作らない。

## 上流ソースで確認した事項(misskey-dev/misskey の `packages/backend`)

- リノートミュートのエラーコードは、作成が `ALREADY_MUTING`、解除が `NOT_MUTING`(通常のミュートと同名)。ブロックは `ALREADY_BLOCKING` / `NOT_BLOCKING`。いずれも `NO_SUCH_USER` と、自分自身を対象にしたときの `MUTEE_IS_YOURSELF` / `BLOCKEE_IS_YOURSELF` を返す。
- `users/lists/list` を `userId` なしで呼ぶと、自分のリストだけが返る(`userId` を渡すのはローカルユーザー専用で、使わない)。応答の `userIds` は `UserListEntityService.pack` が全メンバーを詰めるため、切り詰めは無い。
- `blocking/create` は `UserBlockingService.block` で、双方向のフォローを解除する。
- `users/lists/push` のエラーは `NO_SUCH_LIST` / `NO_SUCH_USER` / `ALREADY_ADDED` / `YOU_HAVE_BEEN_BLOCKED` / `TOO_MANY_USERS`。`pull` は `NO_SUCH_LIST` / `NO_SUCH_USER`。
- `users/report-abuse` の `comment` は 1〜2048 文字。エラーは `NO_SUCH_USER` / `CANNOT_REPORT_YOURSELF` / `CANNOT_REPORT_THE_ADMIN`。
- 権限は `blocking/*` が `write:blocks`、`renote-mute/*` が `write:mutes`、`users/lists/push|pull` が `write:account`、`users/report-abuse` が `write:report-abuse`。

## 実インスタンスで確認する事項(手動確認の段階)

- `users/show` の `url` / `uri` が、リモートユーザー(Misskey 以外の実装を含む)で実際に埋まっているか。埋まっていなければ「ブラウザで開く」が出ないだけで、他に影響しない。

## テスト

- Rust: API ラッパ・`*_core` を wiremock で、`sanitize_profile_url` を表(ローカル、`url` が有効、`url` が `javascript:` で `uri` が有効、両方無効)で、`UserProfile` の自分自身の `None` を検証する。`generates_frontend_bindings` で `tauri.gen.ts` の再生成を確認する。
- Vitest: `UserMenu.test.ts` と `ProfileModal.test.ts` を拡張する(項目の出し分け、ブロック確認、`ALREADY_*` の補正、リストのトグル、`profileUrl` の有無)。`ReportUserDialog.test.ts` を新設する(必須コメント、二重送信の防止、失敗時に入力が残る)。
- 手動確認: Xvfb 越しの実 UI と、`e2e/` の Compose Misskey で、ブロック・リノートミュート・リスト・通報を実際に行う。レビューが通っただけでは完了としない。

## ドキュメント

- `docs/guide/user-guide.md` に、プロフィールの「…」メニューの各項目と、再認証が必要になる点を追記する。
