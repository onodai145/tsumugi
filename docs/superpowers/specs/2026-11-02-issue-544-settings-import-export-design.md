# 設定のインポートとエクスポートの設計

Issue #544。設定をJSONファイルに書き出し(エクスポート)、読み込んで復元する(インポート)機能。Issue本文は空だったので、要件は会話で決めた(下の「決定事項」)。

## 目的

PC間の移行、バックアップ、設定の共有のために、アカウント一覧・カラム構成・アプリ設定を1つのファイルで持ち運べるようにする。

成功条件:

- 端末AでエクスポートしたファイルをB端末でインポートし、各アカウントを再認証すると、カラム構成とアプリ設定がAと同じになる。
- ファイルにトークン・キーリングの中身・ノートキャッシュ・DB接続設定は含まれない。
- インポートに失敗しても、現在の設定は何も変わらない(ディスク上は全体が置き換わるか、何も変わらないかのどちらか)。
- 細工されたファイルをインポートしても、SSRFガード(Issue #377)の除外対象が増えない。
- デスクトップとAndroidの両方で動く。

## 範囲

含める:

- エクスポート: `accounts` `groups` `columns` `mute` `notify` `ui`(UiPrefs 全体) `paneLayout`。
- インポート: 全置換。アカウントは `host + userId` で端末側のものと対応づける。
- 設定の「データ」タブのUI、確認ダイアログ、要ログインの表示。
- Rust側のコマンドと純粋ロジック、フロントのコンポーネント、テスト、ユーザーガイド。

含めない:

- トークン、キーリングの内容、Postgres/MySQLの認証情報。
- `cache_backend`(DB接続先。端末固有で、認証情報はキーリング)。
- `server_mute_snapshots`(サーバー側ミュートの同期の内部状態。Issue #454)。
- ノートキャッシュ、下書き、予約投稿。
- 背景画像・動画の実体ファイルと `backgroundKind` / `backgroundPath`(端末ローカルのパスのため、エクスポート時に除き、インポート後は背景なしになる)。
- セクション単位の選択取り込みとマージ(全置換のみ)。
- 既存の `save_url_to_file` がAndroidで書けない問題の修正(別Issueにする。下の「関連する既存の問題」)。

## 決定事項

| 項目 | 決定 | 理由 |
|---|---|---|
| 対象範囲 | アカウント情報を含む全部 | 移行を最も楽にするため。ユーザーが選んだ |
| 取り込み方式 | 全置換 | 挙動が単純で、「ファイルどおりになる」と説明できる。ユーザーが選んだ |
| 実装の置き場所 | Rust側に純粋関数とコマンド | Model層はRustが持つ方針。除外とID付け替えをテストしやすい |
| 適用後の反映 | `AppState` を更新して WebView をリロード | 実行中の差分反映は、状態が `AppState`・接続・フロントのストアに散らばっていて危険 |
| Android | 対応する。既存の `read_file_bytes` と、同じプラグインでの書き込みを使う | ユーザーが選んだ |

却下した案:

- フロントが既存の各コマンドで値を集めて組み立てる案: Model層のロジックがフロントに漏れ、取り込みの原子性も弱い。
- `settings.json` をそのままコピーする案: 端末固有の項目が混ざり、バージョン管理もできない。

## ファイル形式

```json
{
  "format": "tsumugi-settings",
  "version": 1,
  "exportedAt": "2026-11-02T00:00:00Z",
  "appVersion": "0.0.0",
  "data": {
    "accounts": [],
    "groups": [],
    "columns": [],
    "mute": {},
    "notify": {},
    "ui": {},
    "paneLayout": null
  }
}
```

- フィールド名は既存の serde の `camelCase` に従う。`data` の中身の型は既存の `Account` `ColumnGroup` `Column` `MuteConfig` `NotifyConfig` `UiPrefs` `PaneNode` をそのまま使う。
- `ui` の `backgroundKind` と `backgroundPath` は、エクスポート時に `None` にする。
- `version` は整数。形式を壊す変更をしたときだけ上げる。フィールドの追加は `#[serde(default)]` で吸収し、上げない。このアプリが知っている `version` より大きいファイルは拒否する。

## 構成

追加するのは次の3つだけ。既存の関数・型のシグネチャは変えない。

- `src-tauri/src/store/settings_transfer.rs`: 純粋ロジック(I/Oなし)。
  - `build_export(&SettingsData, app_version, now) -> ExportFile`
  - `plan_import(&SettingsData /*現在*/, ExportFile) -> Result<ImportPlan>`
  - `ExportFile` `ImportPlan` と、概要用の `ImportPreview` を持つ。
- `src-tauri/src/store/settings.rs`: `SettingsStore::replace(new: SettingsData)` を1つ追加する。
- `src-tauri/src/commands/settings_transfer.rs`: コマンドとI/O。

型は `specta::Type` を付け、`specta_builder()` に登録する。バイト列の読み書きは `commands/mute.rs` の `read_file_bytes` を再利用し、書き込み用に `write_file_bytes` を並べて置く。

`SettingsData` は現状 `store/settings.rs` の非公開型。`build_export` / `plan_import` / `replace` から参照できるよう、同じクレート内に限り見えるようにする(`pub(crate)`)。

### コマンド

| コマンド | 内容 |
|---|---|
| `export_settings(path)` | `build_export` の結果をJSON(pretty)にして `write_file_bytes` で書く |
| `inspect_settings_file(path) -> ImportPreview` | 読み込み・検証だけ。書き込みなし。確認ダイアログ用の概要を返す |
| `import_settings(path) -> ImportResult` | 読み込み、再検証、保存、`AppState` 更新を行う |
| `accounts_without_token() -> Vec<String>` | トークンを持たないアカウントIDの一覧 |

`ImportPreview`: カラム数、アカウント数、うち要ログインの数、カスタムCSSの有無、カスタムテーマの有無、外部サービスのURL設定の有無。

外部サービスのURL設定の有無(`hasCustomEndpoints`)は、`ui.searchEngineUrl` または `ui.summalyProxyUrl` が `UiPrefs::default()` の値と異なるときに真になる。共有されたファイルで、URLプレビューや検索の送信先を他人のサーバーに向けられるため、確認ダイアログで知らせる。取り込み自体は拒否しない。
`ImportResult`: 取り込んだカラム数、要ログインのアカウントID一覧。

## インポートの規則(`plan_import`)

入力は信頼できない外部ファイルとして扱う。

アカウントの対応づけ:

1. ファイル内の各アカウントについて、現在の端末に同じ `host + userId` があれば、その端末側アカウントのIDを使う(トークンもそのまま使われる)。ファイル側のIDからの付け替え表を作る。
2. 端末側に無いアカウントは、ファイル側のIDのままトークンなしで追加する。ただし端末側の別アカウントとIDが衝突する場合は拒否する。
3. ファイルに無い端末側アカウントは残す。
4. 付け替え表を `column.accountId` と `ui.defaultAccountId` に適用する。

検証(いずれか1つでも満たさなければ、ファイル全体を拒否する):

- `format == "tsumugi-settings"` であること。
- `version` がこのアプリの対応範囲内であること。
- ファイル内で、アカウントID・グループID・カラムIDがそれぞれ重複しないこと。
- すべての `column.accountId` が、ファイル内の `accounts` のいずれかに存在すること。
- すべての `column.groupId` が、ファイル内の `groups` に存在すること。
- ペイン配置(`paneLayout`)の各 Leaf が参照する `groupId` が、ファイル内の `groups` に存在すること。ペイン配置はグループIDだけを参照し、カラムIDは参照しない。
- アカウントの `host` が、スキーム(`://`)・パス・ポート・認証情報を含まない素のホスト名であること。
- `notify.soundChoice` と、すべての `column.notifySoundChoice` が、`mml::validate_sound_choice` を通ること(`set_notify` と同じ検証。`mml:` で始まる不正なMMLを拒否する)。
- `ui` のうち、Rust側が直接使う数値を、設定の「データ」タブと同じ範囲に丸めること(拒否はしない): `noteCacheLimit` は 0〜100000、`noteCacheMaxAgeDays` は 0〜3650、`noteCacheMaxSizeMb` は 0〜10000、`gapFillLimit` は 0〜1000。それ以外の数値は、`set_ui_prefs` と同じく不透明に保存し、フロントが適用時に丸める(`ui_scale` など。`UiPrefs` の docコメントの方針どおり)。
- ファイルサイズが8MiB以下であること(読んだバイト列の長さで判定する。Androidの `content://` はサイズを事前に取れない場合があるため)。

`defaultAccountId` がファイル内のどのアカウントも指さない場合は、空文字にする。

### 適用(`import_settings`)

1. `read_file_bytes` で読み、`plan_import` で新しい `SettingsData` を作る。`cache_backend` と `server_mute_snapshots` は現在の値を引き継ぐ。
2. `SettingsStore::replace` が、ロックを1回取り、既存の `save()`(一時ファイルに書いて rename)で保存する。メモリ上の状態は、保存に成功してから差し替える。
3. `AppState` を更新する。
   - `accounts`: 新しい一覧で作り直す。
   - `mute`: 新しい値で `apply_mute_config` を呼ぶ(`pub(crate)` にする)。`state.mute` の差し替えに加え、カラムごとのbackfill境界を捨てる。カラムが置き換わるため、古い境界を残さない。
   - `fetch_allowlist`: 下の「SSRF対策」に従って作り直す。
   - 置き換え前の全カラムのストリーム接続を閉じる(`connections.close`)。
4. `ImportResult` を返す。フロントがWebViewをリロードし、カラムとストアをRustの保存内容から作り直す。

### SSRF対策

`fetch_allowlist` は、アカウントのホストをプライベートIP拒否の対象から除外する(Issue #377)。インポートしたトークンなしアカウントのホストをここに入れると、細工したファイルで `127.0.0.1` などを除外対象にできてしまう。

- `fetch_allowlist` に入れるのは、トークンを持つアカウントのホストだけにする。次の2か所を、トークンの有無を見る形に変える。
  - `AppState::refresh_fetch_allowlist()`(呼び出し元は `commands/account.rs` の `complete_miauth` と `remove_account`、`register_test_account`)。
  - `AppState::new_with_sound` の起動時の初期構築。インポート後にアプリを再起動しても、トークンなしアカウントのホストが許可リストに入らないようにするため、こちらも必須。
- 再認証でトークンが付いたとき(`complete_miauth`)には、すでに `refresh_fetch_allowlist()` が呼ばれるので、そのホストが許可リストに入る。

## UI

設定の「データ」タブの末尾に、「設定のバックアップ」見出しと、`エクスポート…` / `インポート…` のボタンを置く。新規コンポーネント `frontend/src/ui/settings/SettingsTransfer.svelte` に分け、`DataSection.svelte` から読み込む。`DataSection` の保存バー(SaveBar)は入力して保存する流れ用なので、このボタン類はその外に置く。

- エクスポート: `save` ダイアログ(初期名 `tsumugi-settings-YYYYMMDD.json`)から `export_settings(path)` を呼び、「書き出しました」と表示する。ボタンの下に「アカウント一覧とカスタムCSSを含みます。トークンは含みません」と注記する。
- インポート:
  1. `open` ダイアログでJSONを選ぶ。
  2. `inspect_settings_file(path)` で検証する。
  3. `ConfirmDialog`(`danger`)に概要(カラムN件・アカウントN件(うち要ログインN件)・カスタムCSS/テーマの有無・外部サービスのURL設定の有無)と「現在の設定とカラムは置き換えられます」を出す。
  4. 承認で `import_settings(path)` を呼び、`location.reload()` する。
- 確認後にファイルが書き換わる場合に備え、`import_settings` でも再検証する。
- 要ログインの表示: `AccountsSection.svelte` が `accounts_without_token` を呼び、該当アカウントに「要ログイン」バッジを出し、「再認証」ボタンを強調する。`Account` 型は変更しない。
- エラーは、既存のセクションと同じく `err` をボタンの下に赤字で出す。

ビジュアル規約(角丸・文字サイズ・アイコン・focus-visible)は `docs/design/style-guide.md` に従い、一度きりの値を作らない。

### エラーメッセージ

すべて `Error::Invalid` の日本語メッセージ。

- 「tsumugiの設定ファイルではありません」
- 「より新しいバージョン(vN)で書き出されたファイルです」
- 「ファイルが大きすぎます(上限8MiB)」
- JSON構文エラー(位置付き)
- 「カラムが存在しないアカウントを参照しています」など、検証に落ちた項目。

## Android

- 読み込みは、既存の `read_file_bytes` を使う。Androidでは `content://` URIを `tauri-plugin-fs` 経由で読む(Issue #400 で作られた仕組み)。
- 書き込みは、`write_file_bytes(app, path, bytes)` を新設する。Androidでは `app.fs().open(file_path, OpenOptions { write, truncate })` を `spawn_blocking` 内で書く。`tauri-plugin-fs 2.5.1` のソースで、`content://` URIに対して `write` と `truncate` が `"wt"` モードに変換されることを確認済み。デスクトップでは `tokio::fs::write` を使う。
- フロントは、ダイアログが返す値をそのままコマンドに渡す。OSごとの分岐はフロントに持たない。
- UIはモバイル幅でも収まるように、ボタンを縦積みにできる構成にする。
- 保存ダイアログは先に空ファイルを作る。書き込みに失敗すると空ファイルが残る可能性があるが、エラーは表示する。この挙動は許容する。

## テスト

Rust(単体、`settings_transfer.rs`):

- `build_export`: `cache_backend` と同期スナップショットが落ちる。`backgroundKind` と `backgroundPath` が落ちる。
- `plan_import`:
  - 一致するアカウントが端末側のIDになり、`column.accountId` と `ui.defaultAccountId` が付け替わる。
  - 端末側にだけあるアカウントが残る。
  - ファイルにだけあるアカウントがトークンなしで追加される。
- 拒否ケース: 形式違い、`version` が新しすぎる、IDの重複、`accountId` の宙ぶらりん、グループ参照の宙ぶらりん、ペイン配置の参照切れ、スキームやパスを含む `host`、IDの衝突。
- `ui` のうち、キャッシュ上限3項目とギャップ埋めの範囲外の値が、範囲内に丸められる。
- 不正な `mml:` の `notify.soundChoice` と `column.notifySoundChoice` が拒否される。
- エクスポートしてからインポートした結果が、背景を除いて元と一致する。
- `SettingsStore::replace`: 書き込み不能なパスで保存に失敗したとき、メモリ上の状態が変わらない。

Rust(`AppState` 統合):

- 細工したファイルの `host` が `127.0.0.1` のとき、`fetch_allowlist` に入らない(SSRF対策の回帰テスト)。
- インポート後に全ストリーム接続が閉じている。
- `generates_frontend_bindings` が、新しい型を含めて通る。

フロント(Vitest、`SettingsTransfer.test.ts` と `AccountsSection` の既存テストへの追加):

- ダイアログをキャンセルしたら、何もしない。
- 確認で `import_settings` を呼び、リロードする。
- エラーが表示される。
- 「要ログイン」バッジが表示される。

実機確認(レビュー通過だけで済ませない):

- Xvfb + 隔離インスタンスで、エクスポートから別の隔離環境へインポートし、再認証してカラムが復活するまで通す。
- Android: エミュレーターで、`adb shell input tap` の本物のタップで操作し、保存と読み込みの両方を確認する。

## 実装前の検証項目

実装計画の最初に確かめる。結果で設計が変わる場合は、設計書に戻る。

1. WebViewのリロードで、カラムのストリームが正しく再接続されること。再接続されない場合は、リロードではなく「再起動してください」の案内に切り替える。
2. Androidで、保存ダイアログが返す `content://` URIへ実際に書けること。書けない場合は、Androidのエクスポートを別の手段(共有シートなど)に切り替えるか、スコープから外す。
3. 保存ダイアログの `defaultPath` が、Androidで提案ファイル名として効くか(効かなくても機能には影響しない)。
4. 開くダイアログのJSONフィルターが、Androidで効かない場合に内容検証で弾けること。
5. `Column` / `NotifyConfig` / `MuteConfig` に、`account_id` を参照するフィールドがほかにないこと(調査では `Column.account_id` と `UiPrefs.defaultAccountId` の2か所だった)。
6. トークンなしアカウントのカラムが出すエラー表示が、利用者に分かる文言であること。分からなければ、「このアカウントは再認証が必要です」を出す。
7. `refresh_fetch_allowlist()` の呼び出し元(`complete_miauth`、`remove_account`、テスト用の `register_test_account`)と、起動時の初期構築で、トークンの有無で絞る変更が問題を起こさないこと。

## 関連する既存の問題

- `save_url_to_file`(`commands/note.rs`)は `tokio::fs::write` を使っていて、Androidの保存ダイアログが返す `content://` URIには書けない可能性が高い。`write_file_bytes` ができるので、別Issueで直せる。今回は直さない。
