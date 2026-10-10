# 任意URL取得コマンドの取得先検証(SSRF対策)設計

Issue #377

## 目的

WebView から渡された任意URLを Rust (`reqwest`) で取得するコマンドが、悪意あるノートのメディアURL経由で LAN 内・ループバック・リンクローカル(クラウドのメタデータ等)へ接続しないようにする。HomeLab 運用では特に影響が大きい。

成功条件:

- 公開ホストへの取得は従来どおり動く。
- loopback / private / link-local などへ解決されるホスト、およびそれらのIPリテラルへの取得は拒否される。
- 公開URLからのリダイレクトで内部アドレスへ誘導されても拒否される。
- DNS リバインディング(検証時と接続時で解決結果が変わる攻撃)が成立しない。
- 登録済みアカウントのインスタンスホストだけは、private IP に解決されても取得できる(自鯖運用のため)。

## 棚卸し結果

| 呼び出し箇所 | 取得先の決まり方 | 対応 |
|---|---|---|
| `save_url_to_file` (`commands/note.rs`) | WebView が渡す任意URL | **対象** |
| `fetch_url_bytes` (`commands/note.rs`) | WebView が渡す任意URL | **対象** |
| `fetch_url_preview` (`commands/note.rs`) | 接続先は設定値のsummalyプロキシかアカウントのホスト。渡されたURLはクエリ値であり、アプリは取得しない | 対象外(穴なし) |
| `MisskeyClient` / `miauth` / `drive` アップロード | ユーザーが設定したインスタンスホスト | 対象外 |
| `check_latest_release` | 固定URL (`api.github.com`) | 対象外 |

加えて、`state.http` は `reqwest` 既定でリダイレクトを最大10回自動で辿るため、最初のURLだけを検証しても迂回される。リダイレクトの各ホップを検証する。

## 採用しない案

- **`@tauri-apps/plugin-http` への置き換え**: スコープがURLのglobで、CIDR指定も解決後IPの検証もできない。メディアのホストは任意なので許可スコープが実質 `https://*` になり、検証の代替にならない。Android での動作も未検証。自前コマンドを残す。
- **事前に名前解決して検証し、IPに固定して接続する方式**: SNI・証明書検証・リダイレクトの扱いが複雑で、リクエストごとのクライアント生成が要る。リゾルバ方式と得られる安全性が同じなので採らない。

## 設計

### 1. `net_guard` モジュール(新規: `src-tauri/src/net_guard.rs`)

責務は次の3つ。

**(a) IP の判定 `is_blocked_ip(IpAddr) -> bool`**

- IPv4: loopback、private (10/8, 172.16/12, 192.168/16)、link-local (169.254/16)、unspecified、broadcast、CGNAT (100.64/10)、multicast。
- IPv6: `::1`、unique-local (fc00::/7)、link-local (fe80::/10)、unspecified、multicast。
- IPv4-mapped IPv6 (`::ffff:a.b.c.d`) は中身のIPv4で判定する。

**(b) 検証付き DNS リゾルバ `GuardedResolver`**(`reqwest::dns::Resolve` 実装)

- `tokio::net::lookup_host` で解決し、`is_blocked_ip` に該当するアドレスを結果から除く。
- 全アドレスが除かれたら `BlockedAddress` エラー(下記)を返す。
- ホスト名が許可リストに含まれる場合は除外せず、そのまま返す。
- 接続は返されたアドレスにだけ向かうため、検証と接続の間に解決結果が変わる余地がない(DNS リバインディング対策)。

**(c) URL 検証 `validate_url(&Url) -> Result<()>` とリダイレクトポリシー**

- スキームは `http` / `https` のみ許可する。
- ホストが IP リテラルの場合は、許可リストに同じ文字列(IPリテラルのアカウントホスト)があれば許可し、無ければ `is_blocked_ip` で判定する(IP リテラルは DNS リゾルバを通らないため、ここで止める)。
- リダイレクトポリシー(`reqwest::redirect::Policy::custom`)は、各ホップで `validate_url` を呼び、ホップ数は10回までとする。ホスト名のホップは (b) のリゾルバが接続時に検証する。

### 2. 許可リスト

- 型は `Arc<RwLock<HashSet<String>>>`。登録済みアカウントのホスト名(小文字化、ポート無し)の集合。
- `GuardedResolver` が共有して参照する。
- `AppState` に `refresh_fetch_allowlist()` を設け、`accounts` が変わる箇所の直後で呼ぶ。対象は `AppState::new`(起動時のロード)、`commands/account.rs` のアカウント追加(`upsert`)・削除(`remove`)、`state.rs` の `upsert` 呼び出し。
- 照合キーは、小文字化・punycode化し、ポートと末尾のドットを除いたホスト部(Unicode表記のホストも、リゾルバに渡る `xn--...` と一致させるため)。アカウントのホストはポート付き (`mi.example.com:3000`) やIP直指定 (`192.168.1.10`) でも登録できる (`normalize_host` が `.` を含めば通すため) ので、キー化して格納する。
- キーが一致するホストだけは private へ解決されても許可する。IP リテラルのURLは、キーと完全一致する場合に限り許可し、それ以外は拒否する。

### 3. 取得専用クライアント

- `AppState` に `fetch_http: reqwest::Client` を追加する。`GuardedResolver` とリダイレクトポリシーを組み込み、`USER_AGENT` は `http` と同じにする。
- 取得系ヘルパー `fetch_guarded_bytes(state, url, max_bytes)` を `commands/note.rs` に切り出す。`save_url_to_file` と `fetch_url_bytes` の重複しているダウンロード処理(ステータス確認、サイズ上限、バイト取得)をここに統合し、先頭で `validate_url` を呼ぶ。
- 既存の `state.http` は変更しない。

### 4. エラー

- `net_guard` に `BlockedAddress` というマーカーエラー型を置く。リゾルバがこれを返し、`reqwest::Error` の `source` チェーンを辿って検出できるようにする。
- 拒否時はフロントへ `Error::Invalid("取得先が許可されていません(ローカル/社内ネットワークのアドレス)")` を返す。解決されたIPやホスト名は `log::warn!` にのみ残し、メッセージには出さない。

### 5. テスト

- `is_blocked_ip` の表形式テスト(上記の全範囲と、境界値: 172.15.x / 172.32.x、100.63.x / 100.128.x、IPv4-mapped)。
- `validate_url`: スキーム (`file:`, `ftp:`)、IPリテラル(`127.0.0.1`, `[::1]`, `169.254.169.254`, `192.168.1.1`)、公開ホスト名が通ること。
- `GuardedResolver`: 禁止IPのみに解決されるホスト(`localhost`)で `BlockedAddress`、許可リストにあると通る。
- `wiremock` のローカルサーバーを使った統合テスト: 許可リストにある `localhost` ホストなら取得できる。公開ホストから `127.0.0.1` へのリダイレクトは拒否される。

## 既知の限界

- **システムプロキシ (`HTTP_PROXY` 等) 使用時**: 名前解決がプロキシ側で行われ、`GuardedResolver` が呼ばれないため、ホスト名経由のIP検証は効かない。`validate_url` によるスキームとIPリテラルの検証は、プロキシの有無によらず効く。プロキシ環境でも取得は従来どおり動かす(`no_proxy()` にはしない)ことを優先する。
- アカウントのホストとは別ドメインのメディアCDNが private IP に解決される構成は、許可リストに無いため拒否される。
- 許可リストはアカウントのホスト名のみで、設定画面からの追加は提供しない(YAGNI)。必要になれば別Issueで扱う。

## スコープ外

- `fetch_url_preview` と API 系クライアントの検証。
- フロントエンド側の変更(コマンドのシグネチャは変えず、エラーメッセージが増えるのみ)。
