# E2Eシナリオ網羅確認・拡充 設計 (Issue #223)

## 背景

#132でPlaywright/tauri-driverによるE2E基盤は導入済みだが、現状のシナリオは
`e2e/specs/` の2本にとどまっている:

- `account-post-reaction.e2e.ts` — 単一アカウントのMiAuth追加 → 投稿 → 自分の
  投稿へのリアクション
- `server-word-mute.e2e.ts` — サーバ側ワードミュート(mutedWords)がタイムライン
  表示に反映されるか

tsumugi特有の複雑な機能(TQL、複数アカウント同時使用、ストリーミング系の
リアルタイム反映、クライアント側ミュート、設定の永続化)はいずれもカバーされて
いない。本ドキュメントはこのギャップを埋める5本の新規シナリオと、それに必要な
共通インフラの拡張を設計する。

## スコープ

以下5本の新規E2Eシナリオを追加し、1本のPRでIssue #223をcloseする。

| # | シナリオファイル | 検証すること |
|---|---|---|
| 1 | `tql-column-filter.e2e.ts` | エキスパートモードのTQLクエリでカラムを作成し、実データ(REST初期ロード＋ストリーミング両方)が正しく絞り込まれる |
| 2 | `multi-account.e2e.ts` | 2アカウントを同時追加し、各アカウントのカラムが互いを混同せずに独立して動作する |
| 3 | `streaming-events.e2e.ts` | 通知(main channel)・リノート・ノート削除がタイムライン/通知カラムへリアルタイム反映される |
| 4 | `client-user-mute.e2e.ts` | 設定→NG(ミュート)のNGユーザ登録がタイムライン表示に反映される(クライアント側) |
| 5 | `settings-persistence-restart.*.e2e.ts` | アプリ再起動後もアカウント・カラム構成が復元される |

### スコープ外

- アプリ再起動を伴わない「永続化」以上のクラッシュリカバリ系は対象外。
- Android実機でのE2Eは対象外。
- モバイルUIのE2Eは対象外(既存の未着手事項として別途)。
- 上記5本以外に気づいたギャップは、必要になった時点で別Issueとして切り出す。

## 共通インフラの変更点

### a. `miauthBridge.ts` のパラメータ化

`startMiauthBridge()` は現状 `certs/seeded-account.json` の単一シードアカウント
に固定でサインインする作りになっている(内部で `/api/signin-flow` を叩き、
その結果を `localStorage['account']` へ注入する)。2アカウント目を追加する
E2E(シナリオ2・3)のために、`{username, password}` を任意で渡せる引数を
追加する(省略時は現行どおりシードアカウントを読む)。既存2本の呼び出しは
無変更で動く。

### b. `misskeyApi.ts` へのヘルパー追加

既存の `createNote()` と同じ薄いPOSTラッパーとして以下を追加する:

- `signUp(username, password): Promise<{ token: string }>` — `/api/signup` で
  新規の一般ユーザーを作成する(シナリオ2・3・4で使う2人目のユーザー用。
  `seed-misskey.ts` が使う `/api/admin/accounts/create` とは別の、管理者権限
  不要な経路)。この環境で自己registrationが許可されていること・メール確認等
  の追加ステップが無いことは実装時に実機で確認する。
- `renoteNote(token, noteId): Promise<string>` — シナリオ3用。
- `deleteNote(token, noteId): Promise<void>` — シナリオ3用。

### c. `run-app.sh` への再起動サポート

`E2E_REUSE_HOME_FILE` という環境変数(ファイルパスを指す)を新設する:

- 未設定なら現行どおり `mktemp` で毎回新規HOMEを作る(既存2本・シナリオ1〜4は
  この経路のまま無変更)。
- 設定されていて、指すファイルが存在しない/空 → 新規に `mktemp` し、生成した
  ディレクトリパスをそのファイルに書き込む。
- 設定されていて、指すファイルに既にパスが書かれている → そのディレクトリを
  そのまま再利用する(終了時に削除しない)。

これにより、「アプリを一度正常終了させ、同じ一時HOMEで再度起動する」という
ユーザーの実際の操作(アプリを閉じて開き直す)を、tauri-driverのWebDriver
セッションライフサイクル(1セッション=1プロセス起動を前提とする)に手を
入れずに再現できる。具体的には `settings-persistence-restart` シナリオを
`.part1.e2e.ts` / `.part2.e2e.ts` という2つの独立したspecファイル(=2つの
独立したwdioセッション)に分割し、両ファイルで同じ `E2E_REUSE_HOME_FILE` の
値(固定パス、例: `wdio-logs/persistence-home-path.txt`)を使う。`wdio.conf.ts`
の `specs` はglobで列挙されるため、`part1` → `part2` の実行順序はファイル名の
辞書順で保証する。

### d. フロントエンドへの `data-testid` 追加

既存2本と同じ方針(表示に影響しない追加のみ)を踏襲する:

- `AddColumnModal.svelte` のTQLテキストエリア(expert mode、`tqlText` に
  bindしている要素)
- `Settings.svelte` の設定タブ切り替え要素(「NG(ミュート)」タブ)
- `MuteSection.svelte` のNGユーザー欄テキストエリア・保存ボタン
- 通知カラムをタイムラインカラムと区別して選択できるようにするための、
  カラム外枠またはタイトル要素(必要になった時点で実装時に判断する)

## 各シナリオの詳細

### 1. TQLフィルタ(`tql-column-filter.e2e.ts`)

1. 既存シードユーザーでREST経由(`createNote`)にユニークマーカー文字列入りの
   ノートと入っていないノートを1件ずつ投稿する。
2. MiAuthでアカウントを追加する(既存2本と同じ`clickThroughAccountSelect`
   パターンを再利用)。
3. カラム追加をexpertモードに切り替え、TQLとして
   `from home where text -> "<marker>"` を入力する(`text -> "..."` は
   `docs/design/filter-dsl-design.md` の「本文に含む」演算子)。
4. 初期表示(REST fetch)でマーカー入りノートのみ表示されることを確認する。
5. その後REST経由でマーカー入り/無しノートをそれぞれ1件追加投稿し、
   ストリーミング側(eval pipeline)のフィルタも効くこと(マーカー入りだけ
   ライブで増える。マーカー無しは最後まで一切現れない)を確認する。

### 2. 複数アカウント(`multi-account.e2e.ts`)

1. `signUp()` で2人目の一般ユーザーBを作成する。
2. シードアカウントA用のブリッジと、B用のブリッジ(パラメータ化した
   `startMiauthBridge({ username, password })`)を両方起動する。
3. 2アカウントをMiAuthで順に追加する(1アカウント目追加完了 →
   2アカウント目追加開始、既存の「アカウント追加」フローをもう一度通す)。
4. それぞれのアカウントでHomeカラムを1つずつ追加する。
5. アカウントAでノートを投稿し、Aのカラムにのみ現れる(AB間にフォロー関係が
   無い前提なのでBのカラムには現れない)ことを確認する。
6. ComposeBarのアカウント切り替えUIでBに切り替えて投稿し、Bのカラムに
   現れることを確認する。

### 3. ストリーミングイベント(`streaming-events.e2e.ts`)

1. アカウントA(既存シードユーザー)をMiAuthで追加し、Homeカラムと通知
   カラムを開く。
2. 2人目ユーザーBを`signUp()`で作成し、Aをメンションするノートを
   REST投稿する。
3. Aの通知カラムにリアルタイムで通知が現れることを確認する
   (`main` channel経由、`stream/connection.rs`の`open_notifications`)。
4. Bが別ノートをリノート(`renoteNote()`)する。
5. Aのタイムラインにリノートがライブで現れることを確認する。
6. Bが自分のノートを`deleteNote()`で削除する。
7. 表示済みだったそのノートがAの画面から消える(削除イベントの反映)ことを
   確認する。

### 4. クライアント側ユーザーミュート(`client-user-mute.e2e.ts`)

1. 2人目ユーザーBを`signUp()`で作成し、Bで制御ノート/対象ノートを投稿する。
2. AをMiAuthで追加しHomeカラムを開く(この時点で両方のノートが表示される)。
3. 設定→NG(ミュート)でBのacctをNGユーザーに登録・保存する。
4. 対象ノートが非表示になり、無関係な制御ノートは表示されたままであることを
   確認する(`server-word-mute.e2e.ts`の構成と対になる、クライアント側版)。

### 5. 設定・カラム構成の永続化(`settings-persistence-restart.*.e2e.ts`)

- **part1**: `E2E_REUSE_HOME_FILE`を指定した状態で通常どおりアカウントを
  追加し、Homeカラム・TQLカラムなど複数のカラムを追加する(種類・順序・
  タイトルを後で照合できるよう記録しておく) → アプリを正常終了させる
  (セッション終了)。
- **part2**: 同じ`E2E_REUSE_HOME_FILE`を指定して別プロセスとして再起動する
  → アカウント追加画面を経由せずいきなり以前のカラム構成(種類・順序・
  タイトル)が復元されていることを確認する。

## リスク・不確定要素(実装時に実機検証で解消する)

- `signUp()`(`/api/signup`)がこのMisskeyバージョン・設定でメール確認や
  CAPTCHA無しに1ステップで完了するかは未確認。既存の`signin-flow`と同様、
  実機で挙動を確認してから実装する。
- `E2E_REUSE_HOME_FILE`経由の再利用ディレクトリに対して、gnome-keyring/
  D-Busセッション周りの状態(前回セッションのkeyringロック状態等)が
  2回目の起動でも問題なく機能するかは未検証。既存の`run-app.sh`の
  keyring起動ロジック(空パスワードで新規作成・アンロック)がpart2でも
  そのまま通るか、実機で確認が必要。
- 通知カラムの`data-testid`設計(カラム種別をどう見分けるか)は既存の
  `Column.svelte`/`AddColumnModal.svelte`の実装を見てから決める。
