# 設定・下書きの保存に失敗しても、メモリだけ変更が残る(Issue #696)

## 背景

`store/settings.rs` の `SettingsStore` と `store/draft.rs` の `DraftStore` の更新メソッドは、
ロックの中でメモリを書き換えてから保存する。保存に失敗すると `Err` を返すが、メモリは変更後のまま残る。
そのため次のことが起きる。

- 画面は失敗と表示するが、実行中のアプリは変更後の状態で動き、再起動すると変更前に戻る。
- 次の別の保存が成功すると、失敗した変更も一緒にディスクへ書かれる。
- メモリの値で保護を判定する箇所(MCP の公開、summaly の取得先の信頼)では、失敗した操作が緩い側に効く。

コードを読んで確認したもので、実際のアプリでは再現していない。ディスクへの書き込みが失敗するときだけ起きる。

## 目的

失敗と返した操作は、何も変えない。利用者に失敗が見えない自動下書きだけは例外とする。

## 決定事項

### 1. `SettingsStore` は一律に、保存に成功してからメモリを差し替える

緩める操作も締める操作も区別しない。ディスクに書けない状態で MCP のアカウント公開を外すと、
公開されたままになる(画面にはエラーが出て、公開中と表示される)。画面と実際の状態が一致することと、
`replace` / `delete_account` と同じ形であることを優先する。締める操作だけメモリに残す案は採らない。

順序を決める場所は 1 つにする。

- `commit(&mut guard, next)`: `next` の保存に成功してから `*guard` を差し替える。
- `update(|data| ...)`: ロック → 複製 → 変更 → `commit`。クロージャの戻り値を返す。

対象:

- `upsert_account` / `upsert_group` / `set_group_width` / `set_group_auto` / `reorder_groups` /
  `delete_empty_groups` / `set_column_title` / `set_column_notify` / `upsert_column` / `delete_column` /
  `save_mute` / `save_server_mute_snapshot` / `save_cache_backend` / `update_mcp` / `update_speech` /
  `set_column_speech` / `set_local_model_license_version` / `set_speech_core_terms_version` /
  `save_notify` / `save_ui` / `trust_summaly_proxy` / `save_pane_layout` / `move_tab`
- すでにこの順序の `delete_account` / `replace` も、同じ補助メソッドに寄せる。

保存せずに戻る分岐は残す。`set_column_speech`(カラムが無ければ `Ok(false)`)と
`trust_summaly_proxy`(取得先が違えば `Err`、インポート由来でなければ `Ok`)は、
分岐のあとに `commit` を直接呼ぶ。`save_ui` の URL 検証は、変更の前に行う(現状どおり)。

公開メソッドのシグネチャと、成功時の動作は変えない。呼び出し元は変更しない。

### 2. `DraftStore` は、手動下書きだけ巻き戻す

| メソッド | 失敗時の画面 | 方針 |
|---|---|---|
| `save_manual` | エラー表示。本文は入力欄に残る | 保存に成功してから差し替える |
| `delete_manual` | エラー表示。一覧に残る | 保存に成功してから差し替える |
| `save_auto` | 何も出ない | 変えない(メモリを正とする) |
| `clear_auto` | 何も出ない | 変えない(メモリを正とする) |

自動下書きを巻き戻さない理由:

- モバイルは投稿画面を閉じるたびに `ComposeBar` を破棄し、開き直すと `get_auto_draft` で復元する。
  `save_auto` を巻き戻すと、古い本文が復元されて直近の入力が消える。
- `clear_auto` は投稿の成功後に呼ぶ。巻き戻すと投稿済みの本文が復元され、二重投稿になりうる。
- 失敗を画面に出さないので、「失敗と表示したのに反映されている」という不整合は起きない。

手動下書きの保存は、自動下書きを含むメモリ全体の複製を書く。したがって手動下書きの保存が成功すれば、
未保存だった自動下書きもディスクへ書かれる。手動下書きの保存が失敗しても、自動下書きのメモリは変わらない。

## テスト

`store/settings.rs`:

- 対象の各操作を表にして、ファイルを使うストアの保存先ディレクトリを消した状態で、
  「`Err` を返し、`snapshot` が変わらない」ことを確認する(`SettingsData` は `PartialEq` を持たないので、
  `serde_json::to_value` で比べる)。
- 同じ表を保存できるストアでも実行し、各操作が `snapshot` を実際に変えることを確認する
  (何も変えない操作で、上のテストが素通りするのを防ぐ)。

`store/draft.rs`:

- `save_manual` / `delete_manual` は、保存の失敗で一覧が変わらない。
- `save_auto` / `clear_auto` は、保存に失敗しても変更がメモリに残る。

## 直さないこと(既知の制限)

- `add_column` / `split_pane` / `move_tab` のように複数回保存するコマンドは、途中で失敗すると
  前の手順が残る(例: タブの無い空のグループ)。コマンド全体を 1 つの更新にはしない。
- `save_server_mute_snapshot` が失敗すると、前回の値が残る。前回の保存より後に追加したミュートを
  同じ起動中に解除しても、解除と検出できず、backfill の境界を捨てない。再起動後は現状でも同じ。
- `set_mcp_enabled` で、MCP の起動に失敗し、「無効に戻す」保存も失敗すると、有効のまま残る
  (キーリングなどの失敗とディスクの失敗が重なる場合だけ)。
- `refresh_instance_meta` は `AccountManager` のメモリを先に更新する。対象の 2 ストアの外なので触らない。
- 下書きの投稿後に読み込み元の手動下書きを消す `delete_draft` は、失敗を画面に出さない。
  失敗すると、投稿済みの手動下書きが一覧に残る。
