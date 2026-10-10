# 任意URL取得コマンドの取得先検証(SSRF対策) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `save_url_to_file` と `fetch_url_bytes` が、loopback / private / link-local などへ解決されるURL(IPリテラル・リダイレクト・DNSリバインディング経由を含む)を取得しないようにする。ただし登録アカウントのホストは例外として許可する。

**Architecture:** 新規モジュール `net_guard` に、IP判定・URL検証・許可リスト・検証付きDNSリゾルバ・リダイレクトポリシーを置く。リゾルバは名前解決の結果から禁止IPを除くので、接続は検証済みIPにしか向かない。`AppState` に取得専用の `fetch_http` を追加し、2コマンドの重複したダウンロード処理を1つのヘルパーに統合してこのクライアントを使わせる。既存の `state.http` は変更しない。

**Tech Stack:** Rust, `reqwest` 0.12.28 (`reqwest::dns::Resolve`, `reqwest::redirect::Policy::custom`), `tokio::net::lookup_host`, `url` 2.5, `wiremock` 0.6 (dev-dependency、既存)。

**Spec:** `docs/superpowers/specs/2026-10-10-ssrf-fetch-guard-design.md`

## Global Constraints

- 許可スキームは `http` / `https` のみ。
- 禁止IPv4: loopback、private (10/8, 172.16/12, 192.168/16)、link-local (169.254/16)、unspecified、broadcast、CGNAT (100.64/10)、multicast。
- 禁止IPv6: `::1`、unique-local (fc00::/7)、link-local (fe80::/10)、unspecified、multicast。IPv4-mapped IPv6 (`::ffff:a.b.c.d`) は中身のIPv4で判定する。
- 許可リストの照合キーは、小文字化し、ポートと末尾のドットを除いたホスト部。キーが一致するホスト名は private に解決されても許可。IPリテラルのURLは、キーと完全一致する場合に限り許可。
- リダイレクトは最大10ホップ。各ホップで `validate_url` を通す。
- 拒否時のエラーは `Error::Invalid("取得先が許可されていません(ローカル/社内ネットワークのアドレス)")`。解決されたIPやホスト名はメッセージに含めず、`log::warn!` にだけ残す。
- システムプロキシは従来どおり使う(`no_proxy()` にしない)。プロキシ使用時はホスト名経由のIP検証が効かないことを既知の限界とする。
- `state.http`(API・miauth・更新確認用)と `fetch_url_preview` は変更しない。
- コミットメッセージは件名のみ(本文・箇条書きなし)。`Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` のトレーラーだけを付ける。
- 作業ブランチは `fix/issue-377-ssrf-fetch-guard`(作成済み)。`main` では編集しない。
- テストは `cd src-tauri && cargo test <name>` で実行する。CI は `cargo test` のみ(clippy/fmt は走らない)。
- Task 1・2 は、Task 3 で使われるまで `dead_code` 警告が出る。意図どおりなので `#[allow(dead_code)]` は付けない。

## Review Focus

- IPv4-mapped IPv6 (`::ffff:127.0.0.1`) で禁止IPv4を迂回される: 中身のIPv4で判定し拒否する (Task 1)。
- `http://0x7f.1/` や `http://2130706433/` のような数値表記の迂回: `url` クレートが `127.0.0.1` に正規化するので、IPリテラルとして拒否される (Task 1)。
- 公開IPと内部IPの両方を返すDNS応答(リバインディングの典型): 内部IPだけを除き、公開IPには接続できる。内部IPしか無ければ拒否 (Task 1)。
- 許可ホスト名が `LOCALHOST.` や `localhost:3000` のように大文字・末尾ドット・ポート付きで登録されている: キー化して一致する (Task 1)。
- 許可ホストからのリダイレクトで、許可外のIPリテラル(`127.0.0.1`)へ飛ばされる: 拒否する (Task 2)。
- アカウント削除後、そのホストが許可リストに残る: 削除直後に外れる (Task 3)。

---

### Task 1: `net_guard` の純粋な判定ロジック

**Files:**
- Create: `src-tauri/src/net_guard.rs`
- Modify: `src-tauri/src/lib.rs`(`mod net_guard;` を追加)

**Interfaces:**
- Consumes: なし
- Produces(すべて `pub(crate)`):
  - `fn is_blocked_ip(ip: IpAddr) -> bool`
  - `fn host_key(host: &str) -> String`
  - `struct FetchAllowlist`(`Clone`、`Default`)。`fn replace<I, S>(&self, hosts: I) where I: IntoIterator<Item = S>, S: AsRef<str>`、`fn contains_key(&self, key: &str) -> bool`
  - `struct BlockedAddress`(`Debug`、`std::error::Error`、`Display`)
  - `fn validate_url(url: &url::Url, allow: &FetchAllowlist) -> std::result::Result<(), BlockedAddress>`
  - `fn filter_addrs(host: &str, addrs: Vec<SocketAddr>, allow: &FetchAllowlist) -> std::result::Result<Vec<SocketAddr>, BlockedAddress>`

- [ ] **Step 1: モジュールを登録し、失敗するテストを書く**

`src-tauri/src/lib.rs` の `mod filter;` の次の行(アルファベット順で `mobile_intent` の前)に追加する。

```rust
mod net_guard;
```

`src-tauri/src/net_guard.rs` を次の内容で作る(実装はまだ無く、テストだけ)。

```rust
//! 任意URLをRust側で取得するコマンド向けの取得先検証(SSRF対策、Issue #377)。
//!
//! WebViewから渡されたURLが loopback / private / link-local などを指していると、アプリが
//! LAN内・ループバックへ接続してしまう。ここでは (1) スキームとIPリテラルのURL検証、
//! (2) 名前解決の結果から禁止IPを除くDNSリゾルバ、(3) リダイレクトの各ホップ検証 を提供する。
//! 接続は検証済みのIPにしか向かないので、DNSリバインディングも成立しない。
//!
//! 登録アカウントのホストだけは、自鯖がLAN内にある運用のために許可リスト経由で例外にする。
//! システムプロキシ使用時は名前解決がプロキシ側で行われ、リゾルバが呼ばれないため、
//! ホスト名経由のIP検証は効かない(既知の限界)。

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, RwLock};

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn blocks_ipv4_special_ranges() {
        for s in [
            "127.0.0.1", "127.255.255.254", "10.0.0.1", "172.16.0.1", "172.31.255.255", "192.168.0.1",
            "169.254.169.254", "0.0.0.0", "255.255.255.255", "100.64.0.1", "100.127.255.255", "224.0.0.1",
        ] {
            assert!(is_blocked_ip(ip(s)), "{s} should be blocked");
        }
    }

    #[test]
    fn allows_ipv4_just_outside_special_ranges() {
        for s in ["8.8.8.8", "1.1.1.1", "172.15.255.255", "172.32.0.1", "100.63.255.255", "100.128.0.1", "192.167.1.1", "11.0.0.1"] {
            assert!(!is_blocked_ip(ip(s)), "{s} should be allowed");
        }
    }

    #[test]
    fn blocks_ipv6_special_ranges() {
        for s in ["::1", "::", "fc00::1", "fd12:3456::1", "fe80::1", "ff02::1"] {
            assert!(is_blocked_ip(ip(s)), "{s} should be blocked");
        }
    }

    #[test]
    fn allows_public_ipv6() {
        assert!(!is_blocked_ip(ip("2606:4700:4700::1111")));
    }

    #[test]
    fn ipv4_mapped_ipv6_is_judged_by_inner_ipv4() {
        assert!(is_blocked_ip(ip("::ffff:127.0.0.1")));
        assert!(is_blocked_ip(ip("::ffff:192.168.1.1")));
        assert!(!is_blocked_ip(ip("::ffff:8.8.8.8")));
    }

    #[test]
    fn host_key_lowercases_and_strips_port_and_trailing_dot() {
        assert_eq!(host_key("Misskey.IO"), "misskey.io");
        assert_eq!(host_key("mi.example.com:3000"), "mi.example.com");
        assert_eq!(host_key("LOCALHOST."), "localhost");
        assert_eq!(host_key("192.168.1.10"), "192.168.1.10");
        assert_eq!(host_key("192.168.1.10:3000"), "192.168.1.10");
    }

    #[test]
    fn allowlist_replace_swaps_the_whole_set() {
        let allow = FetchAllowlist::default();
        allow.replace(["Mi.Example.com:3000", "localhost"]);
        assert!(allow.contains_key("mi.example.com"));
        assert!(allow.contains_key("localhost"));
        allow.replace(["other.example.com"]);
        assert!(!allow.contains_key("mi.example.com"));
        assert!(allow.contains_key("other.example.com"));
    }

    fn url(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    #[test]
    fn validate_url_rejects_non_http_schemes() {
        let allow = FetchAllowlist::default();
        assert!(validate_url(&url("file:///etc/passwd"), &allow).is_err());
        assert!(validate_url(&url("ftp://example.com/x"), &allow).is_err());
        assert!(validate_url(&url("https://example.com/x"), &allow).is_ok());
        assert!(validate_url(&url("http://example.com/x"), &allow).is_ok());
    }

    #[test]
    fn validate_url_rejects_blocked_ip_literals() {
        let allow = FetchAllowlist::default();
        for s in [
            "http://127.0.0.1/",
            "http://127.0.0.1:8080/x",
            "http://[::1]/",
            "http://169.254.169.254/latest/meta-data/",
            "http://192.168.1.1/",
            "http://[::ffff:127.0.0.1]/",
            "http://0x7f.1/",
            "http://2130706433/",
        ] {
            assert!(validate_url(&url(s), &allow).is_err(), "{s} should be rejected");
        }
    }

    #[test]
    fn validate_url_allows_public_ip_literal() {
        assert!(validate_url(&url("http://8.8.8.8/"), &FetchAllowlist::default()).is_ok());
    }

    #[test]
    fn validate_url_allows_ip_literal_only_on_exact_allowlist_match() {
        let allow = FetchAllowlist::default();
        allow.replace(["192.168.1.10:3000"]);
        assert!(validate_url(&url("http://192.168.1.10:3000/files/a.png"), &allow).is_ok());
        assert!(validate_url(&url("http://192.168.1.11/"), &allow).is_err());
    }

    #[test]
    fn validate_url_does_not_block_hostnames() {
        // ホスト名は接続時にリゾルバが検証するため、ここでは通す
        assert!(validate_url(&url("http://localhost:3000/"), &FetchAllowlist::default()).is_ok());
    }

    fn sa(s: &str) -> SocketAddr {
        format!("{s}:0").parse().unwrap()
    }

    #[test]
    fn filter_addrs_drops_blocked_and_keeps_public() {
        let allow = FetchAllowlist::default();
        let out = filter_addrs("rebind.example", vec![sa("127.0.0.1"), sa("93.184.216.34"), sa("10.0.0.5")], &allow).unwrap();
        assert_eq!(out, vec![sa("93.184.216.34")]);
    }

    #[test]
    fn filter_addrs_errors_when_every_address_is_blocked() {
        let err = filter_addrs("evil.example", vec![sa("127.0.0.1"), sa("192.168.0.1")], &FetchAllowlist::default()).unwrap_err();
        assert!(err.to_string().contains("evil.example"));
    }

    #[test]
    fn filter_addrs_keeps_everything_for_allowlisted_host() {
        let allow = FetchAllowlist::default();
        allow.replace(["Mi.Home.Lan:3000"]);
        let out = filter_addrs("MI.HOME.LAN.", vec![sa("192.168.1.10")], &allow).unwrap();
        assert_eq!(out, vec![sa("192.168.1.10")]);
    }
}
```

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認**

Run: `cd src-tauri && cargo test --lib net_guard 2>&1 | tail -20`
Expected: FAIL。`cannot find function is_blocked_ip` など、未定義の関数・型のコンパイルエラー。

- [ ] **Step 3: 最小限の実装を書く**

`src-tauri/src/net_guard.rs` の `use` 行と `#[cfg(test)]` の間に、次を挿入する。

```rust
/// 取得先として拒否するIPか。
pub(crate) fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_blocked_v4(v4),
            None => is_blocked_v6(v6),
        },
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || (a == 100 && (64..=127).contains(&b)) // CGNAT 100.64.0.0/10
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00 // unique-local fc00::/7
        || (first & 0xffc0) == 0xfe80 // link-local fe80::/10
}

/// 許可リストの照合キー: 小文字化し、ポートと末尾のドットを除いたホスト部。
pub(crate) fn host_key(host: &str) -> String {
    let h = host.trim().to_ascii_lowercase();
    // `[::1]:80` のような角括弧付きIPv6はアカウントのホストとして来ない(`normalize_host` が `.` を要求する)ため、
    // `host:port` の最後の `:` 以降が数字だけならポートとして落とす。
    let h = match h.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name.to_string(),
        _ => h,
    };
    h.trim_end_matches('.').to_string()
}

/// 例外として取得を許可するホスト(登録アカウントのホスト)の集合。クローンは同じ集合を共有する。
#[derive(Clone, Default)]
pub(crate) struct FetchAllowlist(Arc<RwLock<HashSet<String>>>);

impl FetchAllowlist {
    /// 集合を丸ごと差し替える。各ホストは `host_key` でキー化して格納する。
    pub(crate) fn replace<I, S>(&self, hosts: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let keys: HashSet<String> = hosts.into_iter().map(|h| host_key(h.as_ref())).collect();
        *self.0.write().unwrap() = keys;
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.0.read().unwrap().contains(key)
    }
}

/// 取得先が許可されていないことを表すエラー。`reqwest::Error` の `source` チェーンから
/// `downcast_ref` で見つけ、ユーザー向けの `Error::Invalid` に変換する。
#[derive(Debug)]
pub(crate) struct BlockedAddress {
    host: String,
}

impl std::fmt::Display for BlockedAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "blocked address: {}", self.host)
    }
}

impl std::error::Error for BlockedAddress {}

/// スキームとIPリテラルを検証する。ホスト名は接続時にリゾルバが検証するので、ここでは通す。
pub(crate) fn validate_url(url: &url::Url, allow: &FetchAllowlist) -> std::result::Result<(), BlockedAddress> {
    let blocked = |host: String| BlockedAddress { host };
    if !matches!(url.scheme(), "http" | "https") {
        return Err(blocked(format!("scheme {}", url.scheme())));
    }
    let literal = match url.host() {
        Some(url::Host::Ipv4(v4)) => Some(IpAddr::V4(v4)),
        Some(url::Host::Ipv6(v6)) => Some(IpAddr::V6(v6)),
        Some(url::Host::Domain(_)) => None,
        None => return Err(blocked("no host".to_string())),
    };
    let Some(ip) = literal else { return Ok(()) };
    // 許可リストのキーはポートを除いたホスト部。`ip.to_string()` は既にポートを含まない正規形なので、
    // `host_key` を通さずそのまま照合する(IPv6の `:` をポート区切りと誤認しないため)。
    if is_blocked_ip(ip) && !allow.contains_key(&ip.to_string()) {
        return Err(blocked(ip.to_string()));
    }
    Ok(())
}

/// 名前解決の結果から禁止IPを除く。許可リストのホスト名なら全て通す。全て除かれたらエラー。
pub(crate) fn filter_addrs(
    host: &str,
    addrs: Vec<SocketAddr>,
    allow: &FetchAllowlist,
) -> std::result::Result<Vec<SocketAddr>, BlockedAddress> {
    if allow.contains_key(&host_key(host)) {
        return Ok(addrs);
    }
    let kept: Vec<SocketAddr> = addrs.into_iter().filter(|a| !is_blocked_ip(a.ip())).collect();
    if kept.is_empty() {
        return Err(BlockedAddress { host: host.to_string() });
    }
    Ok(kept)
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib net_guard`
Expected: PASS(13 tests)。`dead_code` 警告は出てよい。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/net_guard.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: 取得先検証の判定ロジック(net_guard)を追加(#377)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: 検証付きDNSリゾルバ・リダイレクトポリシー・取得専用クライアント

**Files:**
- Modify: `src-tauri/src/net_guard.rs`

**Interfaces:**
- Consumes: Task 1 の `FetchAllowlist`、`BlockedAddress`、`validate_url`、`filter_addrs`、`host_key`
- Produces(`pub(crate)`):
  - `fn build_fetch_client(user_agent: &str, allow: FetchAllowlist) -> reqwest::Result<reqwest::Client>`
  - `fn is_blocked_error(err: &(dyn std::error::Error + 'static)) -> bool`(`source` チェーンに `BlockedAddress` があるか)

- [ ] **Step 1: 失敗するテストを書く**

`net_guard.rs` の `mod tests` の末尾(最後の `}` の前)に追加する。`use` は `mod tests` 内の既存の `use super::*;` で足りる。ただし、`wiremock` と `reqwest` の型を使うので、`mod tests` の先頭の `use super::*;` の下に次も足す。

```rust
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
```

テスト本体を追加する。

```rust
    fn client(allow: &FetchAllowlist) -> reqwest::Client {
        build_fetch_client("tsumugi-test", allow.clone()).unwrap()
    }

    /// `MockServer` は 127.0.0.1 で待ち受ける。`localhost` 名でアクセスして、リゾルバ経由の経路を通す。
    fn localhost_url(server: &MockServer, p: &str) -> String {
        let port = server.address().port();
        format!("http://localhost:{port}{p}")
    }

    #[tokio::test]
    async fn resolver_rejects_localhost_when_not_allowlisted() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/a")).respond_with(ResponseTemplate::new(200).set_body_string("ok")).mount(&server).await;
        let err = client(&FetchAllowlist::default()).get(localhost_url(&server, "/a")).send().await.unwrap_err();
        assert!(is_blocked_error(&err), "unexpected error: {err:?}");
    }

    #[tokio::test]
    async fn resolver_allows_localhost_when_allowlisted() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/a")).respond_with(ResponseTemplate::new(200).set_body_string("ok")).mount(&server).await;
        let allow = FetchAllowlist::default();
        allow.replace(["localhost"]);
        let body = client(&allow).get(localhost_url(&server, "/a")).send().await.unwrap().text().await.unwrap();
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn redirect_to_blocked_ip_literal_is_rejected() {
        let server = MockServer::start().await;
        let secret = format!("http://127.0.0.1:{}/secret", server.address().port());
        Mock::given(method("GET")).and(path("/r")).respond_with(ResponseTemplate::new(302).insert_header("Location", secret.as_str())).mount(&server).await;
        Mock::given(method("GET")).and(path("/secret")).respond_with(ResponseTemplate::new(200).set_body_string("internal")).mount(&server).await;
        let allow = FetchAllowlist::default();
        allow.replace(["localhost"]); // 最初のホップは許可し、リダイレクト先のIPリテラルだけを拒否させる
        let err = client(&allow).get(localhost_url(&server, "/r")).send().await.unwrap_err();
        assert!(is_blocked_error(&err), "unexpected error: {err:?}");
        let received = server.received_requests().await.unwrap();
        assert!(received.iter().all(|r| r.url.path() != "/secret"), "the redirect target must not be requested");
    }

    #[tokio::test]
    async fn redirect_between_allowed_hosts_is_followed() {
        let server = MockServer::start().await;
        let target = localhost_url(&server, "/final");
        Mock::given(method("GET")).and(path("/r")).respond_with(ResponseTemplate::new(302).insert_header("Location", target.as_str())).mount(&server).await;
        Mock::given(method("GET")).and(path("/final")).respond_with(ResponseTemplate::new(200).set_body_string("done")).mount(&server).await;
        let allow = FetchAllowlist::default();
        allow.replace(["localhost"]);
        let body = client(&allow).get(localhost_url(&server, "/r")).send().await.unwrap().text().await.unwrap();
        assert_eq!(body, "done");
    }

    #[tokio::test]
    async fn too_many_redirects_is_an_error() {
        let server = MockServer::start().await;
        let me = localhost_url(&server, "/loop");
        Mock::given(method("GET")).and(path("/loop")).respond_with(ResponseTemplate::new(302).insert_header("Location", me.as_str())).mount(&server).await;
        let allow = FetchAllowlist::default();
        allow.replace(["localhost"]);
        let err = client(&allow).get(me).send().await.unwrap_err();
        assert!(err.is_redirect(), "unexpected error: {err:?}");
        assert!(!is_blocked_error(&err));
    }

    #[test]
    fn is_blocked_error_is_false_for_unrelated_errors() {
        let e = std::io::Error::other("boom");
        assert!(!is_blocked_error(&e));
    }
```

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認**

Run: `cd src-tauri && cargo test --lib net_guard 2>&1 | tail -20`
Expected: FAIL。`cannot find function build_fetch_client` / `is_blocked_error`。

- [ ] **Step 3: 実装を書く**

`net_guard.rs` の `filter_addrs` の後(`#[cfg(test)]` の前)に追加する。

```rust
/// 検証付きDNSリゾルバ。システムの名前解決(`tokio::net::lookup_host`)の結果から
/// 禁止IPを除く。reqwest はここで返したアドレスにだけ接続する。
struct GuardedResolver {
    allow: FetchAllowlist,
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let allow = self.allow.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            match filter_addrs(&host, addrs, &allow) {
                Ok(kept) => Ok(Box::new(kept.into_iter()) as reqwest::dns::Addrs),
                Err(blocked) => {
                    log::warn!("fetch blocked at DNS resolution: {host}");
                    Err(Box::new(blocked) as Box<dyn std::error::Error + Send + Sync>)
                }
            }
        })
    }
}

const MAX_REDIRECTS: usize = 10;

/// 各ホップでスキームとIPリテラルを検証するリダイレクトポリシー。
/// ホスト名のホップは、接続時に `GuardedResolver` が検証する。
fn redirect_policy(allow: FetchAllowlist) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        match validate_url(attempt.url(), &allow) {
            Ok(()) => attempt.follow(),
            Err(blocked) => {
                log::warn!("fetch blocked at redirect: {blocked}");
                attempt.error(blocked)
            }
        }
    })
}

/// 取得専用のHTTPクライアント。検証付きリゾルバとリダイレクトポリシーを備える。
/// システムプロキシ設定は従来どおり尊重する(プロキシ使用時はホスト名経由のIP検証が効かない)。
pub(crate) fn build_fetch_client(user_agent: &str, allow: FetchAllowlist) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(user_agent)
        .dns_resolver(Arc::new(GuardedResolver { allow: allow.clone() }))
        .redirect(redirect_policy(allow))
        .build()
}

/// `source` チェーンのどこかに `BlockedAddress` があるか(`reqwest::Error` を渡す)。
pub(crate) fn is_blocked_error(err: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        if e.downcast_ref::<BlockedAddress>().is_some() {
            return true;
        }
        cur = e.source();
    }
    false
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib net_guard`
Expected: PASS(Task 1 の13件 + 本タスクの6件 = 19 tests)。

もし `resolver_rejects_localhost_when_not_allowlisted` が `is_blocked_error` で失敗する場合は、`{err:?}` の出力で `source` チェーンを確認し、`BlockedAddress` がチェーン内にあるのに `downcast_ref` が効いていないのか、そもそも別の理由(接続拒否など)で失敗しているのかを切り分ける。原因が分からないまま `is_blocked_error` をゆるめない。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/net_guard.rs
git commit -m "$(cat <<'EOF'
feat: 検証付きDNSリゾルバとリダイレクトポリシーの取得専用クライアントを追加(#377)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `AppState` への組み込みと2コマンドの切り替え

**Files:**
- Modify: `src-tauri/src/state.rs`(`AppState` のフィールド、`new_with_sound`、`register_test_account`、テスト)
- Modify: `src-tauri/src/commands/account.rs`(`add_account` 相当の `upsert` 直後と `remove_account`)
- Modify: `src-tauri/src/commands/note.rs`(`save_url_to_file`、`fetch_url_bytes`、共通ヘルパー、テスト)

**Interfaces:**
- Consumes: Task 1・2 の `FetchAllowlist`、`validate_url`、`build_fetch_client`、`is_blocked_error`
- Produces:
  - `AppState.fetch_allowlist: FetchAllowlist`、`AppState.fetch_http: reqwest::Client`(`pub`)
  - `AppState::refresh_fetch_allowlist(&self)`(`pub`、`accounts` の全ホストで許可リストを差し替える)
  - `commands/note.rs` の `async fn fetch_guarded_bytes(http: &reqwest::Client, allow: &FetchAllowlist, url: &str) -> Result<Vec<u8>>`(モジュール内 private)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/state.rs` の `mod tests` に追加する(既存の `is_word_muted_false_before_sync_and_true_after` の近く)。

```rust
    #[test]
    fn refresh_fetch_allowlist_follows_registered_accounts() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        assert!(!state.fetch_allowlist.contains_key("misskey.test"));

        state.register_test_account("acc1");
        assert!(state.fetch_allowlist.contains_key("misskey.test"));

        state.accounts.lock().unwrap().remove("acc1").unwrap();
        state.refresh_fetch_allowlist();
        assert!(!state.fetch_allowlist.contains_key("misskey.test"));
    }
```

`src-tauri/src/commands/note.rs` の `mod tests` に追加する。`use` が足りなければ `mod tests` の先頭に `use wiremock::matchers::{method, path}; use wiremock::{Mock, MockServer, ResponseTemplate};` を足す。

```rust
    fn allow_localhost() -> crate::net_guard::FetchAllowlist {
        let allow = crate::net_guard::FetchAllowlist::default();
        allow.replace(["localhost"]);
        allow
    }

    fn fetch_client(allow: &crate::net_guard::FetchAllowlist) -> reqwest::Client {
        crate::net_guard::build_fetch_client("tsumugi-test", allow.clone()).unwrap()
    }

    #[tokio::test]
    async fn fetch_guarded_bytes_returns_body_for_allowed_host() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/f")).respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8, 2, 3])).mount(&server).await;
        let allow = allow_localhost();
        let url = format!("http://localhost:{}/f", server.address().port());
        let bytes = fetch_guarded_bytes(&fetch_client(&allow), &allow, &url).await.unwrap();
        assert_eq!(bytes, vec![1u8, 2, 3]);
    }

    #[tokio::test]
    async fn fetch_guarded_bytes_rejects_ip_literal_without_connecting() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200)).mount(&server).await;
        let allow = crate::net_guard::FetchAllowlist::default();
        let url = format!("http://127.0.0.1:{}/f", server.address().port());
        let err = fetch_guarded_bytes(&fetch_client(&allow), &allow, &url).await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("許可されていません")), "unexpected: {err:?}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_guarded_bytes_rejects_hostname_resolving_to_loopback() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200)).mount(&server).await;
        let allow = crate::net_guard::FetchAllowlist::default();
        let url = format!("http://localhost:{}/f", server.address().port());
        let err = fetch_guarded_bytes(&fetch_client(&allow), &allow, &url).await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("許可されていません")), "unexpected: {err:?}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_guarded_bytes_rejects_unparsable_url() {
        let allow = crate::net_guard::FetchAllowlist::default();
        let err = fetch_guarded_bytes(&fetch_client(&allow), &allow, "not a url").await.unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "unexpected: {err:?}");
    }

    #[tokio::test]
    async fn fetch_guarded_bytes_maps_http_error_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let allow = allow_localhost();
        let url = format!("http://localhost:{}/missing", server.address().port());
        let err = fetch_guarded_bytes(&fetch_client(&allow), &allow, &url).await.unwrap_err();
        assert!(matches!(err, Error::Api(_)), "unexpected: {err:?}");
    }

    #[tokio::test]
    async fn fetch_guarded_bytes_rejects_oversize_content_length() {
        let server = MockServer::start().await;
        let big = vec![0u8; (MAX_SAVE_FILE_BYTES as usize) + 1];
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_bytes(big)).mount(&server).await;
        let allow = allow_localhost();
        let url = format!("http://localhost:{}/big", server.address().port());
        let err = fetch_guarded_bytes(&fetch_client(&allow), &allow, &url).await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("大きすぎます")), "unexpected: {err:?}");
    }
```

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認**

Run: `cd src-tauri && cargo test --lib fetch_guarded_bytes refresh_fetch_allowlist 2>&1 | tail -20`
Expected: FAIL。`no field fetch_allowlist` / `cannot find function fetch_guarded_bytes`。

(`cargo test` は複数の名前フィルタを受け取れないので、確認は `cargo test --lib fetch_guarded_bytes` と `cargo test --lib refresh_fetch_allowlist` を別々に実行する。)

- [ ] **Step 3: `AppState` に組み込む**

`src-tauri/src/state.rs` の `pub struct AppState` の `pub http: reqwest::Client,` の直後に追加する。

```rust
    /// WebViewから渡された任意URLを取得するコマンド専用のHTTPクライアント(Issue #377)。
    /// 禁止IP(loopback/private/link-local等)への接続を `net_guard` が拒否する。API通信は `http` を使う。
    pub fetch_http: reqwest::Client,
    /// `fetch_http` が例外として許可するホスト(登録アカウントのホスト)。`accounts` が変わったら
    /// `refresh_fetch_allowlist` で差し替える。
    pub fetch_allowlist: crate::net_guard::FetchAllowlist,
```

`new_with_sound` の `let mute = settings.load_mute().unwrap_or_default();` の直後に追加する。

```rust
        let fetch_allowlist = crate::net_guard::FetchAllowlist::default();
        fetch_allowlist.replace(accounts.iter().map(|a| a.host.as_str()));
```

(`accounts` は `AccountManager::with_accounts(accounts)` に move される前の `Vec<Account>` を使う。`with_accounts(accounts)` の行より前にこの2行を置くこと。`let accounts = settings.load_accounts()...;` の直後、`let mute` の前後どちらでもよいが、`with_accounts(accounts)` より前に置く。)

`Self { http: ..., ` の直後に追加する。

```rust
            fetch_http: crate::net_guard::build_fetch_client(USER_AGENT, fetch_allowlist.clone())
                .expect("failed to build fetch client"),
            fetch_allowlist,
```

`impl AppState` に、`host_token` の直前あたりでメソッドを追加する。

```rust
    /// `accounts` の全ホストで取得先の許可リストを差し替える。アカウントの追加・削除の直後に呼ぶ。
    pub fn refresh_fetch_allowlist(&self) {
        let hosts: Vec<String> = self.accounts.lock().unwrap().list().into_iter().map(|a| a.host).collect();
        self.fetch_allowlist.replace(hosts);
    }
```

`register_test_account` の末尾(`self.secrets.set(account_id, "token").unwrap();` の後)に追加する。

```rust
        self.refresh_fetch_allowlist();
```

`src-tauri/src/commands/account.rs`:

`state.accounts.lock().unwrap().upsert(account.clone());` の直後に追加する。

```rust
    state.refresh_fetch_allowlist();
```

`remove_account` の `state.accounts.lock().unwrap().remove(&account_id)?;` の直後に追加する。

```rust
    state.refresh_fetch_allowlist();
```

- [ ] **Step 4: 共通ヘルパーを作り、2コマンドを切り替える**

`src-tauri/src/commands/note.rs` の `save_url_to_file` と `fetch_url_bytes` を置き換える。`MAX_SAVE_FILE_BYTES` の定義とその直前のdocコメントは残す。

````rust
/// 添付ファイルのURLを `fetch_http` でダウンロードする(`save_url_to_file` / `fetch_url_bytes` 共通)。
/// 取得先は `net_guard` が検証する: スキームとIPリテラルは送信前に、ホスト名は接続時(DNS解決後)と
/// リダイレクトの各ホップで拒否する(Issue #377)。上限サイズを超えるファイルは拒否する。
async fn fetch_guarded_bytes(
    http: &reqwest::Client,
    allow: &crate::net_guard::FetchAllowlist,
    url: &str,
) -> Result<Vec<u8>> {
    let blocked = || {
        Error::Invalid("取得先が許可されていません(ローカル/社内ネットワークのアドレス)".to_string())
    };
    let parsed = url::Url::parse(url).map_err(|e| Error::Invalid(format!("invalid url: {e}")))?;
    if let Err(b) = crate::net_guard::validate_url(&parsed, allow) {
        log::warn!("fetch blocked before request: {b}");
        return Err(blocked());
    }
    let resp = http.get(parsed).send().await.map_err(|e| {
        if crate::net_guard::is_blocked_error(&e) {
            blocked()
        } else {
            Error::from(e)
        }
    })?;
    if !resp.status().is_success() {
        return Err(Error::Api(format!("failed to fetch file: {}", resp.status())));
    }
    let too_large = || {
        Error::Invalid(format!(
            "ファイルが大きすぎます（{}MB超）",
            MAX_SAVE_FILE_BYTES / 1024 / 1024
        ))
    };
    if resp.content_length().is_some_and(|len| len > MAX_SAVE_FILE_BYTES) {
        return Err(too_large());
    }
    let bytes = resp.bytes().await?;
    if bytes.len() as u64 > MAX_SAVE_FILE_BYTES {
        return Err(too_large());
    }
    Ok(bytes.to_vec())
}

#[tauri::command]
#[specta::specta]
pub async fn save_url_to_file(state: State<'_, AppState>, url: String, path: String) -> Result<()> {
    let bytes = fetch_guarded_bytes(&state.fetch_http, &state.fetch_allowlist, &url).await?;
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|e| Error::Invalid(format!("cannot write file {path}: {e}")))?;
    Ok(())
}
````

`fetch_url_bytes` 側は、本体を次に置き換える(関数のdocコメントと `#[tauri::command]` は残す)。

```rust
#[tauri::command]
pub async fn fetch_url_bytes(state: State<'_, AppState>, url: String) -> Result<tauri::ipc::Response> {
    let bytes = fetch_guarded_bytes(&state.fetch_http, &state.fetch_allowlist, &url).await?;
    Ok(tauri::ipc::Response::new(bytes))
}
```

`bytes` クレートは直接依存に無いため、ヘルパーは `Vec<u8>` を返す(`Cargo.toml` は変更しない)。`save_url_to_file` の `tokio::fs::write(&path, &bytes)` は `Vec<u8>` のままで動く。

- [ ] **Step 5: テストが通ることを確認**

Run:
```bash
cd src-tauri && cargo test --lib fetch_guarded_bytes && cargo test --lib refresh_fetch_allowlist && cargo test --lib net_guard
```
Expected: すべて PASS。`oversize` テストは200MB超のボディを作るためメモリを使う(約200MB)。実行が極端に遅い・メモリ不足で落ちる場合は、テストを削らずにユーザーへ報告して判断を仰ぐ。

続けて全体のテストと生成物を確認する。

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: PASS。`generates_frontend_bindings` が通り、`frontend/src/bindings/tauri.gen.ts` に差分が出ない(`git status` で確認)。コマンドのシグネチャは変えていないので、差分が出たら理由を調べる。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/account.rs src-tauri/src/commands/note.rs
git commit -m "$(cat <<'EOF'
fix: 任意URL取得コマンドの取得先を検証する(#377)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: ドキュメントと最終確認

**Files:**
- Modify: `docs/guide/user-guide.md`(「トラブルシューティング」節の末尾)
- Modify: `CLAUDE.md`(`src-tauri/src layout` の一覧に `net_guard.rs` を追加)

**Interfaces:**
- Consumes: Task 1〜3
- Produces: なし

- [ ] **Step 1: ユーザーガイドに追記**

`docs/guide/user-guide.md` の末尾(`トラブルシューティング` 節の最後の項目の後)に追加する。

```markdown

**メディアの保存や音声の波形表示が「取得先が許可されていません」で失敗する**

セキュリティのため、メディアの保存と音声波形のための取得は、ローカルや社内ネットワーク(`localhost`、`192.168.x.x` など)のアドレスへは行いません。ログイン中のアカウントのインスタンスについては例外として許可されますが、インスタンスとは別のドメイン(メディア用CDNなど)がLAN内のアドレスに解決される構成では取得できません。なお、システムのプロキシを使っている環境では、この検証は一部しか効きません。
```

- [ ] **Step 2: CLAUDE.md の構成一覧に追記**

`CLAUDE.md` の `### src-tauri/src layout` の `scheduler.rs` の項目の次に、同じ書式で追加する。

```markdown
- `net_guard.rs` — WebView から渡された任意URLを取得するコマンド(`save_url_to_file` / `fetch_url_bytes`、Issue #377)の取得先検証。`AppState.fetch_http` に検証付き DNS リゾルバとリダイレクトポリシーを組み込み、loopback/private/link-local への接続を拒否する(接続は検証済みIPにしか向かない)。登録アカウントのホストだけは `AppState.fetch_allowlist` で例外にする。アカウントを増減したら `refresh_fetch_allowlist()` を呼ぶこと。システムプロキシ使用時は名前解決がプロキシ側のためホスト名経由の検証は効かない(既知の限界)。API通信用の `AppState.http` はこの対象外。
```

- [ ] **Step 3: 最終確認**

Run:
```bash
cd src-tauri && cargo test 2>&1 | tail -15 && cargo build 2>&1 | grep -E 'warning|error' | head
```
Expected: テスト全件 PASS。`net_guard` に関する `dead_code` 警告が出ていない(Task 3 ですべて使われているため)。出ていれば未使用の関数を調べる。

Run: `git status --short`
Expected: 意図した変更だけ。`frontend/src/bindings/tauri.gen.ts` に差分が無い。

- [ ] **Step 4: コミット**

```bash
git add docs/guide/user-guide.md CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: 取得先検証の説明をユーザーガイドとCLAUDE.mdに追記(#377)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

## 手動確認(実装後、PR前にユーザーと行う)

自動テストでは、実際のWebViewから渡るURLや実インスタンスのメディア経路は確認できない。次をXvfb越しの `cargo tauri dev`(CLAUDE.mdとメモリの手順どおり、`WAYLAND_DISPLAY` も unset)で確認する。

1. 公開インスタンスのメディアを「保存」できる。
2. 音声ノートの波形が表示される(`fetch_url_bytes` 経由)。
3. 自鯖(LAN内)のアカウントでログインしている場合、その自鯖のメディアが保存でき、波形も出る。
4. 開発者ツール相当(debug bridge)から `commands.saveUrlToFile("http://127.0.0.1:1/", path)` を実行し、「取得先が許可されていません」で失敗する。
