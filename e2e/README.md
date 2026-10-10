# e2e

tsumugi本体(Tauri v2アプリ)を実機(tauri-driver経由のWebKitGTK)で起動し、アカウント追加(MiAuth)
→ 投稿 → 自分の投稿へのリアクション、という一番基本的な操作フローを通しで検証するE2Eテスト一式。

設計の背景・各コンポーネントの選定理由は
[docs/superpowers/specs/2026-08-17-e2e-automation-design.md](../docs/superpowers/specs/2026-08-17-e2e-automation-design.md)
を参照。このREADMEは「どう動かすか」のみを扱う。

## 前提環境

Linux(WebKitGTK)前提。以下が必要:

- Docker / Docker Compose(テスト用Misskeyインスタンス一式の起動)
- Node.js 22 + pnpm
- `gcc` / `build-essential`(DNS解決用LD_PRELOADシムのビルド)
- `webkit2gtk-driver`(tauri-driverが叩くWebKitWebDriver)
- `xvfb` + `x11-utils`(仮想ディスプレイ、`xdpyinfo`によるレディネス確認)
- `dbus-user-session` + `gnome-keyring`(OS Secret Service経由のトークン保存先)
- `libwebkit2gtk-4.1-dev` / `libjavascriptcoregtk-4.1-dev` / `librsvg2-dev` / `patchelf` などtsumugi本体のビルド依存一式

正確なaptパッケージ一覧は `.github/workflows/test.yml` の `e2e` ジョブの
`Install system dependencies` ステップを参照(CIで実際に動いている構成が正)。

## 実行手順

リポジトリルートで `cargo build`(または `cargo tauri build --debug --no-bundle`)を実行し、
`src-tauri/target/debug/tsumugi` を先にビルドしておくこと。フロントエンドも `frontend` で
`pnpm build` 済みであること(`frontendDist` の埋め込みに必要)。

以降は `e2e/` ディレクトリで:

```sh
./scripts/gen-ca.sh          # E2E用の自己署名テストCA(certs/ca.pem)を生成
docker compose up -d --wait  # テスト用Misskeyインスタンス一式を起動(`files-init` がファイルボリュームの所有者を直すため、画像アップロードも動く)
pnpm seed                    # 管理者アカウントを1件だけ作成(2回目以降は冪等スキップ)
xvfb-run -a pnpm e2e         # 実際のE2Eテストを実行
```

終わったら:

```sh
docker compose down -v
```

## 個別コマンド

- `pnpm seed` — `scripts/seed-misskey.ts`。テスト用管理者アカウントを作成し、
  `certs/seeded-account.json` に `{username, password}`(初回作成時のみ`token`も)を書く。
- `pnpm e2e` — `wdio run wdio.conf.ts`。`specs/**/*.e2e.ts` を実行する。
  `settings-persistence-restart.*.e2e.ts` は別ディレクトリ `specs-persistence/` に
  置かれているため、この実行には含まれない(WebdriverIOの`specs`配列は`!`による
  除外パターンをサポートしないため、パターンではなくディレクトリ分けで除外している)。
  失敗時のログは `wdio-logs/` に出力される。
- `pnpm e2e:persistence` — `specs-persistence/settings-persistence-restart.part1.e2e.ts` →
  `part2.e2e.ts` を `E2E_REUSE_HOME_FILE` 経由で同じ一時HOMEを再利用しながら1回の
  `wdio run` で連続実行し、設定・カラム構成がアプリ再起動後も復元されることを
  検証する専用コマンド。

## クライアント側の予約投稿(Issue #60 B)

予約に対応していないサーバーのアカウントでは、tsumugi が予約をローカル(`scheduled_posts.json`)に保持し、
起動している間に自分で投稿する。E2E の Misskey(2026.7.0)は予約に**対応している**ため、
**デバッグビルド限定**の環境変数 `TSUMUGI_DEBUG_SERVER_VERSION=2025.9.0` で、アプリが認識する
サーバーバージョンだけを上書きし、ローカル予約の経路を通す(投稿は本物の `notes/create`)。
リリースビルドにはこの環境変数の分岐は含まれない(`src-tauri/src/commands/column.rs` の
`debug_server_version_override`)。`pnpm e2e` には含まれず、専用のスクリプトで実行する。

- `pnpm e2e:scheduled` — `specs-scheduled/`。3 件(P1・P2・P3)を同じ分に予約し、P2 は取り消し、P3 は作成欄に戻す。
  予約時刻に P1 だけが**ちょうど 1 件**投稿されること、取り消した/戻した予約は投稿されないことを、Misskey の API
  (`users/notes`)で確認する。待ち時間は 2 分前後。
- `pnpm e2e:scheduled-restart` — `specs-scheduled-restart/`(`scripts/run-reuse-home.sh` の再起動シナリオ)。
  セッションの合間に、`helpers/sessionHooks.ts` の `runPreSession` が `scheduled_posts.json` を**直接書き換える**
  (A: 予約時刻を猶予より前の過去にする、B: `status` を `posting` にする)。再起動後、A は自動投稿されず
  「期限切れ」として残り、B は再送されず「投稿に失敗」(結果不明)になり、A は「今すぐ投稿」でちょうど 1 件投稿
  されることを確認する。待ち時間は 1.5〜2 分前後。ストアの JSON のキー名・状態の値を変えたら、このフックも直すこと
  (`src-tauri/src/store/scheduled_post.rs` と相互に参照している)。

## 再起動をまたぐ・キャッシュDBを検査するシナリオ

次の3つは、`wdio.restart.conf.ts` で実行する(`pnpm e2e` には含まれない)。一時HOMEを使い回し(`scripts/run-reuse-home.sh`が終了時に消す)、
アプリのキャッシュDB(`sqlite3 -readonly ... ".backup"` のコピー)と設定JSONを読んで検査する。再起動の合間に、Misskey側の状態を
変えたい場合は、`helpers/sessionHooks.ts` の `runPreSession`(wdioの `beforeSession` から呼ぶ)に、ディレクトリ名とファイル名で書く。
いずれも `xvfb-run -a` 経由で実行する(Issue #458)。

- `pnpm e2e:column-edit` — 上スクロール(`loadMore`)の最中にカラムのフィルタを編集する/名前だけ変える/閉じる。
  旧フィルタのノートが画面にもキャッシュDB(`column_note`)にも残らないこと、孤児行が残らないこと、編集後のストリームがライブで届くことを見る。
  競合の窓(取得1往復分)に入るよう、スクロールから保存までの遅延を掃引する。ページ内で `window.fetch` を包み、IPCの開始/終了を記録する
  (`__TAURI_INTERNALS__.invoke` は書き換えできない)。フェンスの世代比較を外すと、この掃引は落ちる(変異で確認済み)。
- `pnpm e2e:server-mute-restart` — サーバー側ミュートの同期(`server_mute_snapshots`)と境界。アカウント追加直後と、スナップショットを消した
  再起動では境界が消えず、ミュートを追加しただけでも消えない。ミュートを解除して再起動すると、そのアカウントのカラムの境界だけが空になり、
  解除したユーザーのノートが出てくる。
- `pnpm e2e:ng-restart` — ローカルのNG設定。NGの下で作られた境界が、再起動後も残っていること、NGを解除して保存すると境界が空になり、
  以前キャッシュ済みだった範囲のノートが出てくることを見る。

## ローカルで同じspecを繰り返し実行する場合

`signUp()`(セルフサインアップ)で使う2人目以降のユーザー名は固定文字列
(`e2etestuser2`など)のため、`docker compose down -v` を挟まずに `pnpm e2e` を
2回連続実行すると `DUPLICATED_USERNAME` で失敗する。ローカルで同じspecを
繰り返し実行する場合は `docker compose down -v` → `docker compose up -d --wait` →
`pnpm seed` でテスト用Misskeyインスタンスをリセットしてから再実行すること
(CIは毎回使い捨てのコンテナで実行されるため影響しない)。

## アプリの起動方法について

`wdio.conf.ts` は tsumugi本体を直接叩くのではなく `scripts/run-app.sh` 経由で起動する。
本番の設定ディレクトリ・OS keyringから分離した一時HOME、DNS解決用のLD_PRELOADシム、
入れ子のXvfb、gnome-keyring-daemonの起動など、単純ではない仕組みがいくつも入っているが、
それぞれの「なぜ」は `scripts/run-app.sh` 内のコメントに実機検証の経緯込みで詳しく書いてある
ので、そちらを参照。ここでは重複させない。

## モバイルUI E2E（`pnpm e2e:mobile`）

モバイルUI（Issue #259）のレイアウト・セーフエリア・横はみ出しを検証する専用スイート。
`specs-mobile/` を対象とし、デスクトップ用 `specs/` とは別config（`wdio.mobile.conf.ts`）で実行する。

```sh
cd e2e && xvfb-run -a pnpm e2e:mobile
```

`uiMode=mobile` を設定し、ウィンドウを390x844にリサイズしたうえで、`--safe-*` CSS変数を
注入してセーフエリアを再現する（`helpers/mobile.ts`）。検証は `getBoundingClientRect()` の比較で、
スクリーンショット比較は使わない。

- `layout.e2e.ts`: FAB表示、投稿欄が常時表示でないこと、カラムが100%幅で横スナップすること
- `safe-area.e2e.ts`: 注入inset（6vh/8vhより大きい値。モーダルはtop 120、FABは非ゼロのright）分だけ
  FAB・投稿モーダル・下部メニューバー・カラム領域の上端、および下部メニューボタンの左inset（30px注入）が内側に収まること
- `safe-area-media.e2e.ts`: 画像付きノートを事前投稿（`uploadImage`/`createNote`）してメディアビューワーを開き、
  閉じるボタンの上端・右insetとツールバー（ズームイン）の下端が内側に収まること
- `overflow.e2e.ts`: `[data-columns-scroll]` の外側の各要素が `right <= innerWidth + 1` を満たすこと。
  ルートが overflow-hidden のため `documentElement.scrollWidth` では検出できず、要素ごとに比較している
- `overflow-column.e2e.ts`: カラム内容（長いURL・長い単語・コードブロック・画像）を含むノートを事前投稿し、
  ノート一覧（slot）とその中の各ノート（article）が意図しない横はみ出し（`scrollWidth > clientWidth`）を
  起こしていないこと、slotが viewport 内に収まること。コードブロック等の自前overflow要素は除外される。4件のマーカーノートが測定済み・非折りたたみであることも検証する。画像ケースはメディアセルの `overflow-hidden` を外した/壊した場合にのみ検出する（通常はセルがクリップするため）

限界: デスクトップWebKitGTK上の近似のため、Android WebView固有の挙動（Edge-to-Edgeの実inset値、
IME、ジェスチャー）は対象外で、実機確認は手動。
下部バー右端の右insetは未検証（右insetはFABとビューワーの閉じるボタンで確認している）。

CIでは `e2e` ジョブ内で `pnpm e2e` の直後に実行する（所要時間は約1分で、45分のtimeoutに十分収まる）。
