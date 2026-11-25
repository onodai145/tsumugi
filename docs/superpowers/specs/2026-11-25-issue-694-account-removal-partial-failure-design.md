# アカウント削除が途中で失敗しても、中途半端な状態を残さない (Issue #694)

`commands/account.rs` の `remove_account` は、複数の保存先を順に更新し、各ステップを `?` で抜ける。途中で失敗すると、それより前の変更は戻らず、アカウントが中途半端に残る。

この spec は、**削除の順序を組み替えて、「エラーなら何も変わっていない。確定した後はエラーを返さない」状態にする**ところまでを扱う。

## 決定事項(合意済み)

- 確定点は `SettingsStore::delete_account`(アカウントとカラムの永続化)にする。
- トークンの削除は確定点の**前**に置き、確定に失敗したら書き戻す。
- 確定した後の失敗は、警告ログだけを残して成功扱いにする。

## 目標 / 非目標

目標:

- `remove_account` がエラーを返したとき、アカウント、カラム、トークン、予約投稿、ストリームが削除前のまま残る(下記「既知の制限」の場合を除く)。
- エラーの原因が直れば、再起動なしで、もう一度削除を実行して完了できる。
- `remove_account` が成功を返したとき、アカウントは画面と永続層の両方から消えている。
- `SettingsStore::delete_account` は、保存に失敗したらメモリを変えない。

非目標:

- `SettingsStore` のほかの更新メソッド(メモリを先に更新してから保存する形)の修正。
- 予約投稿ストアの巻き戻し(Issue #682、PR #695 で扱う)。この変更は #695 に依存しない。
- `complete_miauth` や `refresh_instance_meta` と削除の競合の解消。
- フロントエンドの変更。

## 現状(調査結果)

コードを読んで確認した。実際のアプリでの再現はしていない。

現在の順序と、失敗したときに残る状態:

| 順 | ステップ | 失敗しうるか | 失敗したときに残る状態 |
|---|---|---|---|
| 1 | `settings.load_columns` と `connections.close` | 読み込みのみ | - |
| 2 | `scheduled_posts.remove_account` | はい | ストリームは止まったまま、アカウントは残る |
| 3 | `accounts.remove` | 未登録のみ | - |
| 4 | `secrets.delete` | はい | メモリからは消え、設定にはアカウントとカラムが残る |
| 5 | `settings.delete_account` | はい | トークンは削除済み。メモリとディスクの設定がずれる |
| 6 | `settings.delete_empty_groups` | はい | 空のグループがディスクに残る |

4 以降で失敗すると、`AccountManager::remove` が `unknown account` を返すため、再起動するまでやり直せない。

`SettingsStore::delete_account` の呼び出し元は、テストを除けば `remove_account` だけである。

## 設計

### 順序

0. メモリにアカウントが無ければ、`Error::Invalid("unknown account: …")` を返す(現在の挙動を保つ)。
1. そのアカウントのカラムIDを控える。確定後は設定から引けない。
2. `secrets.get` でトークンを控え、`secrets.delete` する。どちらかが失敗したら、エラーを返す。
3. **確定点:** `settings.delete_account`。失敗したら、控えたトークンを `secrets.set` で書き戻して、元のエラーを返す。トークンが元から無いアカウント(インポート直後など)は、何も書き戻さない。
4. 失敗しない後始末を行う。
   - 控えたカラムIDの `connections.close`
   - `accounts` からの削除と `refresh_fetch_allowlist`
   - `forget_server_version` / `forget_translator_available` / `forget_server_mute_sync_lock`
   - `attribution::forget`
5. 失敗しうる後始末を行う。失敗は `log::warn!` に残して、次へ進む。
   - `scheduled_posts.remove_account`
   - `settings.delete_empty_groups`
6. `attribution::forget` をもう一度呼ぶ(実行中の取得が再記録した分を消す。現在と同じ)。

ログには、トークンとアカウントIDを含めない。

### なぜこの順序か

- **トークンの削除を確定点より前に置く。** 後に置くと、keyring の失敗でアカウントだけが設定から消え、トークンが keyring に残る。画面にアカウントが無いので、消し直せない。
- **確定した後はエラーを返さない。** フロントの `removeAccount` は、成功したときだけアカウントとタブを閉じる。確定後にエラーを返すと、Rust 側では消えたアカウントとタブが画面に残る。
- **予約投稿の削除を確定後に置く。** 予約はユーザーが書いた再取得できないデータなので、削除が確定する前には消さない。

### `SettingsStore::delete_account`

複製を書き換え、保存に成功してからメモリを差し替える(`SettingsStore::replace` と同じ形)。シグネチャと、消す対象(アカウント、カラム、サーバーミュートのスナップショット、MCP の公開アカウント)は変えない。空グループの掃除は `delete_empty_groups` のまま、取り込まない。

### 構成

- `commands/account.rs`: 本体を `remove_account_core(state: &AppState, account_id: &str)` に切り出す(`close_column_core` と同じ形)。`#[tauri::command]` の `remove_account` と `logout` のシグネチャは変えない。
- `store/settings.rs`: `delete_account` の内部だけを変える。
- `CLAUDE.md`: `commands/` の説明に、削除の順序の決まりを1文足す。

## テスト

`AppState::new_for_test` を使う。保存の失敗は、`store/scheduled_post.rs` の既存テストと同じく、保存先のディレクトリを消して起こす。keyring の失敗は、`AppState.secrets` を失敗する `SecretStore` に差し替えて起こす。

`store/settings.rs`:

- `delete_account` の保存に失敗したら、メモリのアカウントとカラムが残る。

`commands/account.rs`:

- 成功: アカウント、カラム、トークン、予約が消え、ストリームが閉じる。
- keyring の削除に失敗: エラーになり、アカウント、カラム、予約、ストリームが残る。
- 設定の保存に失敗: エラーになり、トークンが戻り、アカウント、カラム、予約、ストリームが残る。保存先を直して再実行すると成功する。
- 予約の保存に失敗: 成功扱いになり、アカウントとカラムが消える。
- 未登録のアカウント: `Error::Invalid` になる。

## 既知の制限

- 予約の削除の保存に失敗すると、そのアカウントの予約がファイル(PR #695 のマージ後はメモリにも)に残る場合がある。アカウントが無いので投稿はされないが、本文はディスクに残る。
- 手順 2 と 3 の間でプロセスが終了すると、トークンの無いアカウントが残る(「要ログイン」と表示される)。もう一度削除できる。
- 手順 3 の失敗後、トークンの書き戻しにも失敗した場合も、同じ状態になる。
