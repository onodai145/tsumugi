# 外部サービスのURL設定の検証の設計

Issue #549。設定の「外部連携」にある2つのURL(検索エンジンのURL、URLプレビュー(summaly)の取得先)に、スキームと宛先の検証を入れる。

## 目的

設定ファイルのインポート(Issue #544)で、他人が作ったファイルから設定される経路が増えた。そのため次の2点を防ぐ。

- 検索エンジンのURLが `javascript:` や `data:` などになり、MFMの検索リンクとして描画されること。
- URLプレビューの取得先が、ループバックや社内ネットワークなどの内部ホストに向き、そこへ通信が出ること(SSRF)。

成功条件:

- `javascript:` / `data:` などのスキームが、設定の保存でもインポートでも拒否される。
- インポートで入った取得先は、プライベート宛先に接続できない。DNS rebinding でも突破されない。
- 利用者が設定画面で自分で入力したLAN・ループバックの自前プロキシは、これまで通り使える。
- 既に保存されている不正な値が原因で、他の設定の保存が失敗し続けることがない。

## 範囲

含める:

- 検索エンジンURLと取得先URLの形式検証(Rust側が正、フロントは保存前のエラー表示用)。
- 取得先URLの由来(手入力か、インポートか)の記録と、`fetch_url_preview` の `net_guard` 経由化。
- インポートした取得先の状態表示と、設定画面で保存し直すことによる信頼(新コマンド2つ)。
- 読み込み時の不正値のリセット。
- WebView内での `javascript:` / `data:` リンクの扱いの調査と、Issueへの記録。
- テスト、ユーザーガイド、`CLAUDE.md` の `net_guard` の記述の更新。

含めない:

- URLプレビューのレスポンス(サムネイルやプレイヤーURL)の検証。別の経路で、このIssueの範囲外。
- `AppState.http`(API通信)全体のガード。
- システムプロキシ使用時のホスト名検査(`net_guard` の既知の制限のまま)。
- インポート確認ダイアログの文面と `ImportPreview` の型の変更。

## 決定事項

| 項目 | 決定 | 理由 |
|---|---|---|
| 検証の置き場所 | Rustの `domain/ui.rs` に検証関数。フロントは同じ規則を `lib/searchEngine.ts` に持つ | Rust側が正。フロントは保存前に即座にエラーを出すため |
| 取得先の宛先制限 | 手入力した値は制限しない。インポートした値は取得時に `net_guard` で制限する | 自前プロキシ(HomeLab等)を残すため。ユーザーが選んだ |
| 手入力かインポートかの記録 | `SettingsData.summaly_proxy_imported: bool` を足す(`UiPrefs` には入れない) | エクスポート形式と `UiPrefs` の型を変えないため。既定は偽なので、既存ユーザーの移行処理が要らない |
| インポートした取得先の信頼 | 「外部連携」セクションで保存し直すと手入力扱いになる。画面に状態と注意文を出す | ユーザーが選んだ。値を消して入力し直す手間を避けつつ、状態が分かるようにする |
| 取得の経路 | `fetch_url_preview` は常に `fetch_http` を使う。手入力したホストだけ許可リストに入れる | 接続時に検証済みIPへだけ繋ぐので、DNS rebinding も防げる |
| 既存の不正値 | 読み込み時に既定値へ戻して警告ログを出す。ファイルへの書き戻しは次の保存まで遅延する | 無効値が残ると、`...app.ui` を丸ごと送る他の設定セクションの保存まで失敗するため |

## 検証ルール

### 検索エンジンURL

次をすべて満たすこと。満たさなければ拒否する。

- 2048文字以下。
- `{query}` を含む。
- `{query}` を固定の文字列に置換してパースでき、スキームが `http` か `https`。
- ホストがあり、ユーザー名・パスワードを含まない。
- `{query}` が認証部(スキーム、ユーザー情報、ホスト、ポート)に入っていない。判定は、`{query}` を2通りの文字列(`x` と `y`)に置換してパースし、スキーム・ホスト・ポート・ユーザー情報が一致することで行う。ノート本文のテキストが宛先ホストを決められないようにするため。

### URLプレビュー取得先

空欄は有効(=接続先インスタンスの `/url`)。空欄以外は次をすべて満たすこと。

- 2048文字以下。
- パースでき、スキームが `http` か `https`。
- ホストがあり、ユーザー名・パスワードを含まない。

## 手入力とインポートの区別

`SettingsData` に `summaly_proxy_imported: bool`(`#[serde(default)]`、既定は偽)を足す。偽は「利用者が設定画面で保存した値」か、取得先が空であることを表す。

| 契機 | 動作 |
|---|---|
| `set_ui_prefs` | 取得先URLが保存済みの値と違えば、`imported = false` にする。同じなら変えない |
| `plan_import` | インポートした `summaly_proxy_url` が空なら `false`。空でなく、現在の値と違えば `true`。現在の値と同じなら現在の `imported` を引き継ぐ |
| エクスポート | `summaly_proxy_imported` は書き出さない(`ExportData` に含めない) |

`set_ui_prefs` はフロントから来た `UiPrefs` を受け取るが、`imported` は `UiPrefs` に入れないので、他の設定セクションの保存では変わらない(取得先URLが変わったときを除く)。

### 設定画面での確認と信頼

インポートした取得先は、「外部連携」セクションで保存し直すだけで手入力扱いにできる。利用者に分かるように、状態を画面に出す。

- 新しいコマンド `get_summaly_proxy_imported() -> bool`: 取得先が空でなく、`summaly_proxy_imported` が真のときだけ真を返す。
- 新しいコマンド `trust_summaly_proxy() -> ()`: `summaly_proxy_imported` を偽にして保存し、許可リストを更新する。
- `ExternalIntegrationSection.svelte`:
  - 開いたときに `get_summaly_proxy_imported` を呼び、真なら取得先の入力欄の下に注意文を出す。「この取得先はインポートした値です。プライベートアドレス(LANやループバック)には接続できません。保存すると自分で設定した値として扱い、そうしたアドレスにも接続します。心当たりのない値なら保存せずに変更してください。」
  - 保存が成功したあと、注意文が出ていた場合は `trust_summaly_proxy` を呼び、注意文を消す。
  - 検証に失敗して保存しなかったときは信頼しない。
- 他のセクションの保存や、取得先を変えない操作では信頼されない。信頼するのは、「外部連携」セクションの保存ボタンだけ。
- 取得先がパブリックなホストなら、拒否されることがないので、注意文の後半は「保存すると自分で設定した値として扱います」だけで足りる。ただし宛先の種類はUIで判定せず、注意文は常に同じ文面にする(判定を実装しない)。

フロントエンドが `trust_summaly_proxy` を呼べることは、フロントエンドがアプリ自身のコードである以上、問題ない。守る対象は細工された設定ファイルで、ファイルの内容だけでは `summaly_proxy_imported` を偽にできない(`ExportData` に含めず、`plan_import` が決めるため)。

## 取得の経路

`commands/note.rs` の `fetch_url_preview` と `api/url_preview.rs` が、`state.http` の代わりに `state.fetch_http` を使う。

- 取得先が空のとき: 接続先インスタンスの `https://{host}/url`。このホストは、トークンを持つアカウントのホストとして許可リストに入っているので、今まで通り動く。
- 取得先が設定されているとき: `fetch_http` は、許可リストにないホストのプライベート・ループバック・リンクローカル宛を接続時に拒否する(DNS解決結果の検査、リダイレクトの再検査を含む)。
- 許可リストへの追加: `AppState::refresh_fetch_allowlist` が、アカウントのホストに加えて、`summaly_proxy_url` が空でなく `summaly_proxy_imported` が偽のときだけ、その `host[:port]` を入れる。ポートはURLに明示されているときだけ付け、ないときはホストだけ(80と443を許可)にする。IPv6リテラルの表記は `net_guard::parse_entry` の仕様に合わせ、専用のテストで確認する。
- 呼ぶタイミング: アカウントの追加・削除・再認証に加えて、`set_ui_prefs` の後と、設定インポートの後。起動時の `AppState::new` でも初期値に含める。
- 拒否されたとき: `net_guard::is_blocked_error` で判定し、`Error::Invalid` にして、「このURLはインポートした値のためプライベートアドレスへ接続できません。設定の外部連携で保存し直すと接続できます」という趣旨の文言を返す。

`fetch_http` を使うことによる変化は、取得先ホストが許可リストにないプライベート宛の場合の拒否のみ。ユーザーエージェントと、リダイレクトの追従は `http` と同じ。

## 読み込み時のリセット

`store/settings.rs` の `load_json_or_default` で `SettingsData` を作った直後に、`ui` の2つのURLを検証する。

- 検索エンジンURLが不正なら、既定値(Google)に戻す。
- 取得先URLが不正なら、空欄に戻す。
- 戻したときは `log::warn!` を出す。値そのものはログに出さない(URLに秘密が含まれうるため)。
- ファイルは書き換えない。次の保存で反映される。

描画側の `buildSearchUrl` も、スキームが不正なときは既定の検索エンジンにフォールバックする(二重防御)。

## 各経路での検証

| 経路 | 場所 | 失敗時 |
|---|---|---|
| 設定画面の保存 | `ExternalIntegrationSection.svelte` が保存前に検証する。成功後、注意文が出ていれば `trust_summaly_proxy` を呼ぶ | その場にエラーを出して保存しない(信頼もしない) |
| `set_ui_prefs` | Rust側で検証する | `Error::Invalid` を返す |
| インポート | `plan_import` の中で検証する | ファイル全体を拒否する(既存の方針) |
| 読み込み | `load_json_or_default` | 上の「読み込み時のリセット」 |

## WebView内でのリンクの扱いの調査

検索リンクは `<a href target="_blank" rel="noreferrer noopener">` に入る。`javascript:` / `data:` を入れたときに、WebKitGTK(Linux)で実際にどうなるかは未確認で、受け入れ条件でIssueへの記録が求められている。

- 実装の最初に調べる。隔離したインスタンス(`dbus-run-session` とXvfb、短いXDGパス。メモリの `reference-isolated-instance-needs-short-xdg-path` に従う)で、検索リンクのテンプレートに `javascript:` と `data:` を入れ、実クリックまたはデバッグブリッジの操作で挙動を見る。
- あわせて、外部リンクがWebViewの既定動作で開くのか、`plugin-opener` 経由なのかも確認する。
- 結果はIssue #549にコメントで残す。調査結果が検証ルールの結論(`http`/`https`のみ)を変えることは想定していない。

## テスト

Rust:

- 検証関数: 許可する形と拒否する形(`javascript:` `data:` `file:` `ftp:`、ホストなし、認証情報あり、`{query}` なし、`{query}` がホストに入る、長すぎる値)。
- `set_ui_prefs`: 不正値の拒否、取得先の変更で `imported` が偽になる、変更なしで `imported` が変わらない。
- `trust_summaly_proxy` が `imported` を偽にし、許可リストに取得先のホストが入る。`get_summaly_proxy_imported` は、取得先が空なら偽、インポート直後なら真、信頼後は偽。
- `plan_import`: 不正値でファイル全体が拒否される、取得先が現在の値と違えば `imported` が真、同じなら引き継ぐ、空なら偽。
- 読み込み: 不正値が既定値に戻る。
- 許可リスト: `imported` が偽の取得先ホストだけが入る(ホスト名、IPv4リテラル、ポートあり、IPv6リテラル)。
- `fetch_url_preview`: wiremockで、許可リストにある取得先には繋がる、許可リストにないループバック宛は接続前に拒否される、リダイレクト先がプライベートなら拒否される。

フロント(Vitest):

- `lib/searchEngine.ts` の検証と `buildSearchUrl` のフォールバック。Rust側と同じ入力例の表を使う。
- `ExternalIntegrationSection` が不正値でエラーを出して保存しない。

実機確認:

- Linuxで、設定画面から不正な検索URLと取得先を保存できないこと。
- 自前のループバックのモックプロキシを手入力して、プレビューが出ること。
- インポートした取得先がループバックだと拒否されること(隔離インスタンスで確認)。

## ドキュメント

- `docs/guide/user-guide.md`: 外部連携の項に、許可するスキームと、インポートした取得先が自動では使えないことと再入力の手順を足す。インポートの注意書きも更新する。
- `CLAUDE.md`: `net_guard.rs` の記述に、取得先の許可リストに手入力のsummalyホストも入ること、`fetch_url_preview` が `fetch_http` を通ることを足す。
- `docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md` の「取り込み自体は拒否しない」の記述は、形式が不正な値の場合に拒否する、と更新する。

## 影響範囲

変更するファイル(予定):

- `src-tauri/src/domain/ui.rs`(検証関数)
- `src-tauri/src/store/settings.rs`(`summaly_proxy_imported`、読み込み時のリセット)
- `src-tauri/src/store/settings_transfer.rs`(インポートの検証と `imported`)
- `src-tauri/src/commands/mute.rs`(`set_ui_prefs`)、`src-tauri/src/lib.rs`(新コマンドの登録)
- `src-tauri/src/commands/settings_transfer.rs`(インポート後の許可リスト更新)
- `src-tauri/src/commands/note.rs`、`src-tauri/src/api/url_preview.rs`(`fetch_http` 経由)
- `src-tauri/src/state.rs`、`src-tauri/src/net_guard.rs`(許可リストの更新と、URLから許可リストの項目を作る補助)
- `frontend/src/lib/searchEngine.ts`、`frontend/src/ui/settings/ExternalIntegrationSection.svelte`
- 上記のテストとドキュメント

コマンドを2つ足す(`get_summaly_proxy_imported` `trust_summaly_proxy`)ので、`specta_builder()` への登録と TS バインディング(`tauri.gen.ts`)の再生成が要る。既存コマンドの引数・戻り値の型は変えない。`UiPrefs` の型も変えない。

## 未解決の確認事項

- IPv6リテラルの許可リスト項目の形式は、実装時に `net_guard::parse_entry` を読んで確定する。
- `fetch_http` がシステムプロキシを使う環境では、ホスト名の検査が効かない。既知の制限として、ユーザーガイドには書かない(`CLAUDE.md` に既にある)。
