# 設定インポート後の案内・再認証画面の文言・エラー表示の整理(Issue #553)

## 目的

設定のインポート(Issue #544)の実画面確認で見つかった、画面表示まわりの細かい点を整理する。動作の不具合ではなく、案内と表現の問題である。

- 端末を引っ越した利用者が、インポート直後に何をすればよいかを迷わず分かる。
- トークンなしのアカウントへの通信で、赤いエラーや警告が出ない。
- エラー文に `Error: invalid: …` のような生の接頭辞が付かない。

## スコープ

- Rustで変えるのは、確認ダイアログの件数(項目4)だけ。コマンドやコマンドのシグネチャは変えない。
- `ImportPreview.accountsNeedingLogin` の意味とdocコメントが変わるため、`frontend/src/bindings/tauri.gen.ts` を再生成する(型の形は変わらない)。
- それ以外はフロントエンドの変更。

対象外:

- トークンなしアカウントのカラム枠の見た目。今まで通りタブなしの空枠のままにし、案内はバナーが担う。
- `ForbiddenError`(403)の再認証アクションの仕組み。

## 項目1: 案内バナーと通信のスキップ

### 現状

- トークンなしのアカウントへの通信は、`AppState::host_token`(`state.rs`)が `Unauthorized("no token for account: …")` を返して失敗する。
- フロントはこれを `String(e)` のままエラーログ(`#logFailure`)にする。インスタンス情報の取得とサーバーミュート同期は警告ログになる。
- `accounts_without_token` コマンドは既にあるが、使っているのは `AccountsSection` だけで、メイン画面からは見えない。

### 変更

- store が起動時に `commands.accountsWithoutToken()` を呼び、`needsLogin: Set<string>` を持つ。次のときに更新する。
  - 再認証の完了(`completeAccount`)
  - アカウントの削除
  - 設定のインポート(インポート後はリロードされるため、起動時の取得で足りる)
- 次の経路で、`needsLogin` のアカウントを最初からスキップする。ログは出さない。
  - `#syncServerMutes`(起動時と定期同期の両方)
  - `#refreshInstanceMeta`
  - 起動時のタブ復元の `resumeColumn`
- メイン画面に `ReauthBanner.svelte` を追加する。
  - `needsLogin.size > 0` のときだけ表示する。
  - 文言は「N件のアカウントは再認証が必要です」。
  - ボタン「設定のアカウントを開く」は `openSettings("accounts")` を呼ぶ。
  - 閉じるボタンは付けない。再認証が済めば自然に消える。

## 項目2: 再認証画面の文言

- 純粋関数 `reauthMessage(needsLogin: boolean): string` を `lib/` に置く。
  - トークンなし: 「このアカウントは設定のインポートで追加され、まだログインしていません。ブラウザで認可を完了したら、下のボタンを押してください。」
  - スコープ更新: 現行の「スコープが更新されたトークンを取得します。ブラウザで認可を完了したら、下のボタンを押してください。」
- `AddAccount.svelte` は `app.needsLogin.has(reauthAccount.id)` で判定する。親からのpropsは増やさない。
- `reauthAccount` が無い(通常の追加)ときの文言は変えない。

## 項目3: エラー文の整理

### 現状

`String(e)` が `Error: invalid: …` になる理由は2つある。

1. `formatError` が `kind: message` の形を作る。
2. `Error.prototype.toString` が `Error: ` を付ける。

### 変更

- `ipc.ts` に `AppError extends Error` を追加する。`toString()` は `message` だけを返す。`unwrap` と `unwrapAcc` はこれを投げる。`ForbiddenError` は `AppError` を継承する。
- `formatError` は `kind: ` の接頭辞を外し、`message` だけを返す。
- `message` を持たない種別(現状は `rateLimited` のみ)は日本語にする。「リクエストが多すぎます。しばらく待ってからやり直してください」。
- 既存の `String(e)`(約40か所)は変えない。
- `NoteCard` の `includes("UNAVAILABLE")` と `scheduleErrorMessage` は本文の部分一致なので、接頭辞を外しても影響しない。
- IPCを経由しないJS例外は、従来通り `Error: …` のまま出る。バグと見分けるため、意図して残す。

## 項目4: 確認ダイアログの件数

- `plan_import`(`store/settings_transfer.rs`)は純粋なまま残す。
- コマンド層の `inspect_settings_bytes` と `import_settings_bytes`(`commands/settings_transfer.rs`)で、取り込み後に実際にトークンを持たないアカウントを数える。対象は次の2つ。
  - ファイルにだけあり、新規に追加されるアカウント
  - 端末側にすでにあるアカウント(ファイルと一致したものも、端末にだけ残るものも)のうち、トークンがないもの
- `ImportPreview.accounts_needing_login` のdocコメントを、この意味に合わせて直す。
- ダイアログの文言(「うちN件のアカウントは、取り込み後に再認証が必要です」)は変えない。

## テスト

- Vitest
  - `AppError.toString()` と `formatError`(接頭辞なし、`rateLimited` の日本語)
  - `reauthMessage`
  - `ReauthBanner` の表示条件(0件で非表示、1件以上で表示、ボタンで設定が開く)
  - store が `needsLogin` のアカウントに対して、ミュート同期・インスタンス情報取得・タブ復元を呼ばないこと
- Rust
  - 端末側の一致したアカウントがトークンなしのとき、件数に入る。
  - 端末側の一致したアカウントがトークンありのとき、件数に入らない。
  - ファイルに無く、端末にだけ残るアカウントがトークンなしのとき、件数に入る。
  - ファイルにだけあるアカウントは、従来通り数える。
- 実画面
  - Xvfb と `e2e/` のCompose Misskeyで、インポート直後のバナーと再認証画面の文言をスクリーンショットで確認する。画像は自分で見て確認する。

## 再認証後の復帰(確認済みの前提)

`completeAccount` は、アカウントの更新後に `#syncServerMutes(account.id)` と `#resumeAccountColumns(account.id)` を呼ぶ。このため、スキップしたタブも再認証の完了後に復元され、リロードは要らない。

- `completeAccount` は、この2つを呼ぶ前に `needsLogin` からそのアカウントを外す。外す前に呼ぶと、`#syncServerMutes` のスキップに引っかかって同期されない。
- スキップの判定は、起動時の経路(`boot` の `syncServerMutes` ループ、`refreshInstanceMeta`、タブ復元のループ)と、定期同期の `#syncAllServerMutes` に置く。`#syncServerMutes` と `#resumeAccountColumns` 自体には置かない(再認証後の呼び出しを妨げないため)。
- `needsLogin` が空のとき、起動経路は現状と変わらない。
