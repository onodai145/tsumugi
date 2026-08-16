# E2E操作テスト自動化 設計（Issue #132）

## 背景・目的

Issue #73「テストをちゃんとやる」のサブタスク。#130（Vitest基盤）・#131（Svelteコンポーネントテスト）に続き、
ログイン→投稿→リアクションのような実際の操作を伴うE2Eテストを自動化する。

制約（#73本文より）:
- リリース版環境（設定）に影響を与えない
- 他インスタンスに迷惑をかけない
- → テスト用のMisskeyインスタンス/アカウントを分離する必要がある

## 全体アーキテクチャ

新規トップレベルディレクトリ `e2e/`（`frontend/`とは独立したpnpmパッケージ）に、テストコードとテスト用Misskey環境一式を集約する。

```
e2e/
├─ docker-compose.yml      # Traefik + Misskey + PostgreSQL + Redis
├─ seed/                    # 起動後にテスト管理者アカウントを作成するスクリプト
├─ wdio.conf.ts             # WebdriverIO + tauri-driver 設定
├─ specs/
│   └─ account-post-reaction.e2e.ts
└─ helpers/
    └─ miauth-bridge.ts     # Playwright(CDP) でMiAuth同意をブリッジ
```

`test/` ではなく `e2e/` を採用する。このリポジトリでは既に `cargo test`（Rust単体）・`pnpm test`（Vitest単体）を
指して「test」を使っており、`.github/workflows/test.yml` にも単体テスト系ジョブが同居している。同名のディレクトリを
足すと単体テストと紛らわしくなるため、業界一般的な呼称である `e2e/` で明確に区別する。

## コンポーネント

### 1. テスト用Misskeyインスタンス（Docker Compose）

`e2e/docker-compose.yml` で以下を起動する。CIとローカルで同一のcomposeを使う。

- **Traefik**: リバースプロキシ。`localhost` 向けの自己署名CAでTLS終端する。
  - 理由: `session/miauth.rs` / `api/client.rs` はいずれも `https://{host}` をハードコードしており、
    平文HTTPのMisskeyには接続できない。本番コード変更を避け、テスト環境側でhttps化する。
  - CA証明書は毎回使い捨てで生成せず、初回のみ `e2e/certs/`（`.gitignore`対象）に生成して永続化し、
    以降の実行では再利用する。
  - CIステップおよびローカル実行スクリプトで、このCA証明書をOSの信頼ストアに追加する
    （`update-ca-certificates`等）。これにより `reqwest`（Rust側）・WebKitGTK（webview側）双方が
    追加コード変更なしに証明書を信頼する。登録スクリプトはフィンガープリント/ファイル名で
    既に登録済みかを確認し、未登録の場合のみ追加する（冪等）。CIは毎回使い捨て環境のため実質
    無関係だが、ローカルで繰り返し実行しても信頼ストアに重複登録されないようにする。
  - リバースプロキシにはTraefikを採用する（ユーザーの選好。Caddyより優先）。
- **Misskey + PostgreSQL + Redis**: 通常のMisskeyスタック。
- **seedスクリプト**: コンテナ起動後、テスト用管理者アカウントを作成し、CAPTCHA/2FAを無効化した設定を投入する。

### 2. アプリ実行時の分離（本番環境・実キーリングを汚さない）

`session/secrets.rs` の `KeyringStore`（OS Secret Service経由）は `lib.rs` で常時使用されており、
実行環境（本番ビルド）を切り替える機構は無い。これを踏まえ、テスト実行時のプロセス起動方法で分離する
（本番コード変更なし）。

- `dbus-run-session -- gnome-keyring-daemon --unlock --daemonize -- <tauri-driver起動コマンド>` で、
  テスト実行専用の一時DBusセッション上にSecret Serviceを都度立ち上げる。CIにはSecret Serviceデーモンが
  存在しないため、これはCIでアプリを動かすためにも必須。
- `XDG_CONFIG_HOME` / `XDG_CACHE_HOME` をテスト用一時ディレクトリに向けてアプリを起動する。
  Tauriの `app_config_dir` / `app_cache_dir` はこれらの環境変数を参照して解決されるため、設定ファイル・
  ノートキャッシュのSQLiteが実環境と分離される。

### 3. アプリ操作: tauri-driver + WebdriverIO

- Tauri公式が案内する構成。Linux（開発環境のHyprland/WebKitGTK）ではWebKitWebDriverを使い、実際の
  ネイティブウィンドウを直接操作する。
- テスト対象バイナリは `cargo tauri build --debug`（frontendDist埋め込み）で作る。
  `cargo tauri dev` はvite devサーバー前提のため使わない（CLAUDE.md記載の既知の罠）。
- CI/ローカルともディスプレイが無い環境のため `xvfb-run` 配下で実行する。

### 4. MiAuthブリッジ（Playwright + CDP）

MiAuthは `openapi/misskey-api-doc.json` の対象外（Misskey本体のWebページで完結するフロー）。
`session/miauth.rs` のコメント通り、ブラウザで認可 → `check` APIでtoken取得、という流れになっている。

調査の結果、以下の実装方針とする:

1. E2Eテスト開始前に `POST /api/signin`（`username`/`password`）でテスト管理者ユーザーのセッションを確立する。
   レスポンスの `i`（アクセストークン相当）とセットされるセッションCookieを取得する。
   このエンドポイントはOpenAPI未記載だがMisskey公式フロントエンドが実際に使用しているものであり、CAPTCHA/2FAは
   テスト環境側で無効化しているため、この呼び出しに追加のUI操作は不要。
2. Playwrightで別プロセスのChromiumを `--remote-debugging-port` 付きで起動し、`connectOverCDP` で接続する。
   起動したブラウザコンテキストに上記セッションCookieを注入する（ログインフォーム入力は自動化しない）。
3. アプリ側の `opener` プラグインが呼ぶ外部ブラウザ起動コマンド（`BROWSER` 環境変数等）を、
   「CDP接続済みChromiumの既存セッションに新規タブとしてMiAuth URLを開かせる」小さなスクリプトに差し替える。
4. アプリの「アカウント追加」操作（ホスト入力 → ボタン）でMiAuthページが開いたら、Playwright側でそのページの
   「許可」ボタン（`miauth.vue` の `onAccept` → `POST /api/miauth/gen-token`）をクリックする。
5. アプリの「認証完了」ボタン（`AddAccount.svelte` の `complete()`）をtauri-driver側からクリックし、
   `complete_miauth` を経てアカウントが確立されることを確認する。

MiAuthの同意ページ自体（Misskey本体のWebUI）はtsumugi側のコードではないため、ここが自動化・検証の対象外
であっても回帰検知上の問題はない。ブラウザ側で自動化するのは「同意ページを開いて1回クリックする」だけであり、
ログインフォームの自動化は行わない。

Cookie名・`gen-token`呼び出しの詳細（Misskeyバージョンによる差異）は実装時に実インスタンスで検証する。

## テストシナリオ（初回スコープ）

1本の連続フローとして実装する:

1. アプリを起動する（クリーンなプロファイル）
2. アカウント追加: ホスト入力 → MiAuth開始 → ブリッジがブラウザ側で許可 → 「認証完了」でアカウント確立
3. 投稿: コンポーズバーからノートを投稿する
4. リアクション: タイムラインに流れてきた自分の投稿にリアクションを付け、UI上に反映されることを確認する

ログインとその後の操作を別シナリオに分割せず1本にまとめることで、毎回ログインをやり直すことによる実行時間の
増加を避ける。

## CI構成

`.github/workflows/test.yml` に `e2e` ジョブを追加する。

1. `docker compose -f e2e/docker-compose.yml up -d` でMisskeyスタックを起動し、ヘルスチェックを待つ。
2. TraefikのCA証明書をOSの信頼ストアに追加する（`update-ca-certificates`）。
3. `xvfb-run` 配下で `cargo tauri build --debug` → `tauri-driver` 起動 → `dbus-run-session` 経由で
   WebdriverIOスペックを実行する。
4. 失敗時はスクリーンショット・アプリログをアーティファクトとして保存する。

## リスクと対処方針

- **tauri-driverの動作検証が未実施**: 現行Tauri v2 / WebKitGTKバージョンの組み合わせで `tauri-driver` が
  問題なく動くかは未検証。実装計画の最初のタスクとして「ウィンドウタイトルが取得できるか」程度の最小スパイクを
  置く。ここで実用に耐えないことが判明した場合、CIへの組み込みは別issueへ切り出し、本issueはローカルで動く
  基盤の確立までにスコープを縮小する判断ポイントとする。
- **MiAuthブリッジの実装詳細は未検証**: Cookie名・`gen-token`呼び出しのペイロード等は実装時に実インスタンスで
  確認する。想定と異なる場合はブリッジの実装を調整する。

## 対象外（本issueでは扱わない）

- Playwright単独でのTauriデスクトップアプリ直接操作の検証（tauri-driverを軸とするため）
- MiAuth同意ページ（Misskey本体のWebUI）自体の見た目・挙動テスト
- ログイン以外のアカウント管理（再認証、複数アカウント切り替え等）のE2E化
