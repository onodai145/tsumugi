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
docker compose up -d --wait  # テスト用Misskeyインスタンス一式を起動
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
  失敗時のログは `wdio-logs/` に出力される。

## アプリの起動方法について

`wdio.conf.ts` は tsumugi本体を直接叩くのではなく `scripts/run-app.sh` 経由で起動する。
本番の設定ディレクトリ・OS keyringから分離した一時HOME、DNS解決用のLD_PRELOADシム、
入れ子のXvfb、gnome-keyring-daemonの起動など、単純ではない仕組みがいくつも入っているが、
それぞれの「なぜ」は `scripts/run-app.sh` 内のコメントに実機検証の経緯込みで詳しく書いてある
ので、そちらを参照。ここでは重複させない。
