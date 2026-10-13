# 取得先許可リストのポート単位化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 登録アカウントのホストに対する取得先の例外を「ホスト + 許可ポート」に絞り、同じホストの別ポートへの取得を拒否する(Issue #482)。

**Architecture:** `FetchAllowlist` の内部を `HashMap<String, HashSet<u16>>`(`host_key` → 許可ポート集合)にする。ポート付き登録はそのポートのみ、ポート無し登録は 80/443 を許可する。ポートは DNS リゾルバから見えないため、`validate_url`(送信前とリダイレクトの各ホップで呼ばれる)で、許可ホストの許可外ポートを拒否する。`replace` のシグネチャは変えないので `state.rs` の呼び出し側は無変更。

**Tech Stack:** Rust(`url`、`reqwest`、テストは `wiremock` / `tokio`)。

**Spec:** `docs/superpowers/specs/2026-10-13-fetch-allowlist-port-design.md`(改訂対象の既存 spec: `docs/superpowers/specs/2026-10-10-ssrf-fetch-guard-design.md`)

## Global Constraints

- 作業は feature branch `feat/issue-482-fetch-allowlist-port` 上で行う(`main` への直接コミット禁止)。
- ポート付き登録(`mi.home.lan:3000`)は `{3000}` のみ、ポート無し登録(`mi.home.lan`、IP 直指定の `192.168.1.10` も同様)は `{80, 443}` のみ許可する。同じホストの複数登録は和集合。数値として解釈できないポートのエントリは格納しない。
- URL 側のポートは `url.port_or_known_default()`(`http` は 80、`https` は 443)で取る。スキームとポートの組み合わせは検査しない。
- 許可リストに無いホスト(外部のメディアサーバー)にはポート規則をかけない。
- ユーザー向けエラー文言は変えない: `取得先が許可されていません(ローカル/社内ネットワークのアドレス)`。
- `FetchAllowlist::replace` のシグネチャは変えない。フロントエンド、`state.http`(API 通信)、`fetch_url_preview` は変更しない。
- コミットメッセージは subject 行のみ(本文なし)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` / `--no-gpg-sign` は使わない。コミットが失敗・タイムアウトしたら止めて報告する(再試行しない)。
- Rust のテストは `cd src-tauri && cargo test ...` で実行する。

## Review Focus

1. 登録ポートが既定ポートでないアカウント(`mi.home.lan:3000`)で、既定ポートの URL(`https://mi.home.lan/`)は拒否される(登録ポートだけが例外)。→ Task 2 のテスト
2. ポート無し登録で、明示的な既定ポート付き URL(`http://host:80/`、`https://host:443/`)も通る。→ Task 2 のテスト
3. 大文字・末尾ドット・Unicode のホストでもポート規則が効く(リゾルバに渡る表記との食い違いで素通りしない)。→ Task 1 / Task 2 のテスト
4. 不正なポートの登録(`bad.lan:99999`)が、他のアカウントの許可を巻き込まない。→ Task 1 のテスト
5. アカウントの削除・差し替え(`replace`)の後に、古いポートの許可が残らない。→ Task 1 のテスト

---

### Task 1: `FetchAllowlist` をホスト + 許可ポート集合にする

**Files:**
- Modify: `src-tauri/src/net_guard.rs`(`host_key` 周辺 47–84 行、`validate_url` 内の `contains_key`、`filter_addrs` 内の `contains_key`、テスト)
- Modify: `src-tauri/src/state.rs:479-486`(テストの `contains_key` → `contains_host`)

**Interfaces:**
- Consumes: なし。
- Produces:
  - `FetchAllowlist::replace<I, S>(&self, hosts: I) where I: IntoIterator<Item = S>, S: AsRef<str>`(シグネチャ不変)
  - `FetchAllowlist::contains_host(&self, key: &str) -> bool`(旧 `contains_key`。ホストが許可リストにあるか)
  - `FetchAllowlist::allows(&self, key: &str, port: u16) -> bool`(ホストが許可リストにあり、かつ `port` が許可集合に含まれるか)
  - `host_key(host: &str) -> String`(挙動不変)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/net_guard.rs` の `mod tests` 内、`allowlist_replace_swaps_the_whole_set` の直後に追加する。

```rust
    #[test]
    fn allowlist_port_registration_allows_only_that_port() {
        let allow = FetchAllowlist::default();
        allow.replace(["mi.home.lan:3000"]);
        assert!(allow.contains_host("mi.home.lan"));
        assert!(allow.allows("mi.home.lan", 3000));
        assert!(!allow.allows("mi.home.lan", 22));
        assert!(!allow.allows("mi.home.lan", 443));
    }

    #[test]
    fn allowlist_portless_registration_allows_80_and_443() {
        let allow = FetchAllowlist::default();
        allow.replace(["mi.home.lan"]);
        assert!(allow.allows("mi.home.lan", 80));
        assert!(allow.allows("mi.home.lan", 443));
        assert!(!allow.allows("mi.home.lan", 3000));
    }

    #[test]
    fn allowlist_ip_literal_without_port_allows_80_and_443() {
        let allow = FetchAllowlist::default();
        allow.replace(["192.168.1.10"]);
        assert!(allow.allows("192.168.1.10", 80));
        assert!(allow.allows("192.168.1.10", 443));
        assert!(!allow.allows("192.168.1.10", 8080));
    }

    #[test]
    fn allowlist_same_host_registered_twice_unions_ports() {
        let allow = FetchAllowlist::default();
        allow.replace(["mi.home.lan:3000", "mi.home.lan"]);
        for port in [3000, 80, 443] {
            assert!(allow.allows("mi.home.lan", port), "{port} should be allowed");
        }
        assert!(!allow.allows("mi.home.lan", 8080));
    }

    #[test]
    fn allowlist_drops_entry_with_unparsable_port_without_affecting_others() {
        let allow = FetchAllowlist::default();
        allow.replace(["bad.home.lan:99999", "ok.home.lan:3000"]);
        assert!(!allow.contains_host("bad.home.lan"));
        assert!(allow.allows("ok.home.lan", 3000));
    }

    #[test]
    fn allowlist_normalizes_case_trailing_dot_and_unicode_with_port() {
        let allow = FetchAllowlist::default();
        allow.replace(["MI.Home.Lan.:3000", "みすきー.example:4000"]);
        assert!(allow.allows("mi.home.lan", 3000));
        let ascii = url::Url::parse("http://みすきー.example/").unwrap().host_str().unwrap().to_string();
        assert!(allow.allows(&ascii, 4000));
        assert!(!allow.allows(&ascii, 3000));
    }

    #[test]
    fn allowlist_replace_drops_old_ports() {
        let allow = FetchAllowlist::default();
        allow.replace(["a.home.lan:3000"]);
        allow.replace(["a.home.lan:4000"]);
        assert!(!allow.allows("a.home.lan", 3000));
        assert!(allow.allows("a.home.lan", 4000));
    }
```

既存テストの `contains_key` を `contains_host` に書き換える(同じ場所)。

```rust
    #[test]
    fn allowlist_replace_swaps_the_whole_set() {
        let allow = FetchAllowlist::default();
        allow.replace(["Mi.Example.com:3000", "localhost"]);
        assert!(allow.contains_host("mi.example.com"));
        assert!(allow.contains_host("localhost"));
        allow.replace(["other.example.com"]);
        assert!(!allow.contains_host("mi.example.com"));
        assert!(allow.contains_host("other.example.com"));
    }
```

`src-tauri/src/state.rs` の `refresh_fetch_allowlist_follows_registered_accounts` 内の 3 か所の `contains_key` も `contains_host` に書き換える。

- [ ] **Step 2: テストが失敗(コンパイルエラー)することを確認する**

Run: `cd src-tauri && cargo test --lib net_guard 2>&1 | tail -20`
Expected: FAIL。`no method named contains_host` / `allows` found for struct `FetchAllowlist`。

- [ ] **Step 3: 実装する**

`src-tauri/src/net_guard.rs` の先頭の `use std::collections::HashSet;` を `use std::collections::{HashMap, HashSet};` に変える。

47–84 行(`host_key` の doc コメントから `FetchAllowlist` の `impl` の終わりまで)を次で置き換える。

```rust
/// `host:port` を (ホスト部, ポート文字列) に分ける。最後の `:` 以降が数字だけならポートとみなす。
/// 角括弧付きIPv6はアカウントのホストとして来ない(`normalize_host` が `.` を要求する)ため考慮しない。
fn split_port(h: &str) -> (&str, Option<&str>) {
    match h.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => (name, Some(port)),
        _ => (h, None),
    }
}

/// 許可リストの照合キー: 小文字化・punycode化し、ポートと末尾のドットを除いたホスト部。
/// punycode化するのは、リゾルバに渡る名前(`url` クレートがUnicodeのホストを変換したもの)と
/// Unicode表記のアカウントホストを一致させるため。
pub(crate) fn host_key(host: &str) -> String {
    let (h, _) = split_port(host.trim());
    // 変換できない文字列は、ASCIIの小文字化だけで従来どおりのキーにする(照合は一致しなくなるだけ)。
    let normalized = match url::Host::parse(h) {
        Ok(parsed) => parsed.to_string(),
        Err(_) => h.to_ascii_lowercase(),
    };
    normalized.trim_end_matches('.').to_string()
}

/// アカウントのホスト文字列を (照合キー, 許可ポート) に分ける。ポート付きはそのポートだけ、
/// ポート無しは 80 と 443(APIクライアントは `https://{host}` 固定で、`http://` のメディアURLも許す)。
/// ポートが数値として解釈できない(u16に収まらない)エントリは `None`(許可しない側に倒す)。
fn parse_entry(raw: &str) -> Option<(String, Vec<u16>)> {
    let h = raw.trim();
    let ports = match split_port(h).1 {
        Some(p) => vec![p.parse::<u16>().ok()?],
        None => vec![80, 443],
    };
    Some((host_key(h), ports))
}

/// 例外として取得を許可するホスト(登録アカウントのホスト)と、そのホストで許可するポートの集合。
/// クローンは同じ集合を共有する。
#[derive(Clone, Default)]
pub(crate) struct FetchAllowlist(Arc<RwLock<HashMap<String, HashSet<u16>>>>);

impl FetchAllowlist {
    /// 集合を丸ごと差し替える。各ホストは `host_key` でキー化し、ポートは `parse_entry` の規則で格納する。
    /// 同じキーの複数登録は、ポート集合の和集合にする。
    pub(crate) fn replace<I, S>(&self, hosts: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut map: HashMap<String, HashSet<u16>> = HashMap::new();
        for host in hosts {
            if let Some((key, ports)) = parse_entry(host.as_ref()) {
                map.entry(key).or_default().extend(ports);
            }
        }
        *self.0.write().unwrap() = map;
    }

    /// ホストが許可リストにあるか(ポートは問わない)。ポートが見えないDNSリゾルバ側の判定に使う。
    pub(crate) fn contains_host(&self, key: &str) -> bool {
        self.0.read().unwrap().contains_key(key)
    }

    /// ホストが許可リストにあり、かつ `port` が許可ポートに含まれるか。
    pub(crate) fn allows(&self, key: &str, port: u16) -> bool {
        self.0.read().unwrap().get(key).is_some_and(|ports| ports.contains(&port))
    }
}
```

同じファイル内の残りの `contains_key` 呼び出しを `contains_host` に変える(この Task では挙動を変えない)。

- `validate_url` 内: `allow.contains_key(&ip.to_string())` → `allow.contains_host(&ip.to_string())`
- `filter_addrs` 内: `allow.contains_key(&host_key(host))` → `allow.contains_host(&host_key(host))`

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib net_guard 2>&1 | tail -15 && cargo test --lib refresh_fetch_allowlist 2>&1 | tail -5`
Expected: PASS(`net_guard` の全テストと `refresh_fetch_allowlist_follows_registered_accounts`)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/net_guard.rs src-tauri/src/state.rs
git commit -m "$(cat <<'EOF'
refactor: 取得先許可リストをホストと許可ポートの集合で持つ(Issue #482)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `validate_url` で許可ホストの許可外ポートを拒否する

**Files:**
- Modify: `src-tauri/src/net_guard.rs`(`validate_url`、テスト)
- Modify: `src-tauri/src/commands/note.rs:644-648`(テストの `allow_localhost`)と、それを使うテスト

**Interfaces:**
- Consumes: Task 1 の `FetchAllowlist::contains_host(&self, &str) -> bool`、`FetchAllowlist::allows(&self, &str, u16) -> bool`、`host_key(&str) -> String`。
- Produces: `validate_url(url: &url::Url, allow: &FetchAllowlist) -> Result<(), BlockedAddress>`(シグネチャ不変。許可ホストの許可外ポートを `BlockedAddress` で拒否する)。テスト用ヘルパー `allow_localhost_port(server: &MockServer) -> FetchAllowlist`(`net_guard.rs` のテスト内)、`allow_localhost(server: &MockServer) -> FetchAllowlist`(`note.rs` のテスト内)。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/net_guard.rs` の `mod tests` 内、`validate_url_does_not_block_hostnames` の直後に追加する。

```rust
    #[test]
    fn validate_url_rejects_other_port_on_allowlisted_hostname() {
        let allow = FetchAllowlist::default();
        allow.replace(["mi.home.lan:3000"]);
        assert!(validate_url(&url("http://mi.home.lan:3000/files/a.png"), &allow).is_ok());
        assert!(validate_url(&url("http://mi.home.lan:22/"), &allow).is_err());
        assert!(validate_url(&url("http://mi.home.lan:8080/"), &allow).is_err());
        // 登録ポートが既定ポートでないとき、既定ポートも例外ではない
        assert!(validate_url(&url("https://mi.home.lan/"), &allow).is_err());
        assert!(validate_url(&url("http://mi.home.lan/"), &allow).is_err());
    }

    #[test]
    fn validate_url_allows_default_ports_for_portless_hostname() {
        let allow = FetchAllowlist::default();
        allow.replace(["mi.home.lan"]);
        for s in ["http://mi.home.lan/", "https://mi.home.lan/", "http://mi.home.lan:80/", "https://mi.home.lan:443/"] {
            assert!(validate_url(&url(s), &allow).is_ok(), "{s} should be allowed");
        }
        for s in ["http://mi.home.lan:3000/", "https://mi.home.lan:8080/"] {
            assert!(validate_url(&url(s), &allow).is_err(), "{s} should be rejected");
        }
    }

    #[test]
    fn validate_url_applies_port_rule_to_case_trailing_dot_and_unicode_hosts() {
        let allow = FetchAllowlist::default();
        allow.replace(["MI.Home.Lan:3000", "みすきー.example:4000"]);
        assert!(validate_url(&url("https://MI.HOME.LAN.:3000/"), &allow).is_ok());
        assert!(validate_url(&url("https://MI.HOME.LAN.:8080/"), &allow).is_err());
        assert!(validate_url(&url("http://みすきー.example:4000/"), &allow).is_ok());
        assert!(validate_url(&url("http://みすきー.example:22/"), &allow).is_err());
    }

    #[test]
    fn validate_url_rejects_other_port_on_allowlisted_ip_literal() {
        let allow = FetchAllowlist::default();
        allow.replace(["192.168.1.10:3000", "192.168.1.20"]);
        assert!(validate_url(&url("http://192.168.1.10:3000/"), &allow).is_ok());
        assert!(validate_url(&url("http://192.168.1.10:22/"), &allow).is_err());
        assert!(validate_url(&url("http://192.168.1.20/"), &allow).is_ok());
        assert!(validate_url(&url("http://192.168.1.20:8080/"), &allow).is_err());
    }

    #[test]
    fn validate_url_does_not_apply_port_rule_to_unlisted_hosts() {
        let allow = FetchAllowlist::default();
        allow.replace(["mi.home.lan:3000"]);
        assert!(validate_url(&url("http://cdn.other.example:8443/x"), &allow).is_ok());
        assert!(validate_url(&url("https://cdn.other.example/x"), &allow).is_ok());
        assert!(validate_url(&url("http://8.8.8.8:8080/"), &allow).is_ok());
    }
```

同じ `mod tests` 内、`fn client(...)` の直後にテスト用ヘルパーとリダイレクトのテストを追加する。

```rust
    /// `localhost:{サーバーのポート}` だけを許可するリスト(`MockServer` はランダムポートで待ち受ける)。
    fn allow_localhost_port(server: &MockServer) -> FetchAllowlist {
        let allow = FetchAllowlist::default();
        allow.replace([format!("localhost:{}", server.address().port())]);
        allow
    }

    #[tokio::test]
    async fn redirect_to_other_port_of_allowlisted_host_is_rejected() {
        let first = MockServer::start().await;
        let other = MockServer::start().await;
        let target = localhost_url(&other, "/secret");
        Mock::given(method("GET")).and(path("/r")).respond_with(ResponseTemplate::new(302).insert_header("Location", target.as_str())).mount(&first).await;
        Mock::given(method("GET")).and(path("/secret")).respond_with(ResponseTemplate::new(200).set_body_string("other service")).mount(&other).await;
        let allow = allow_localhost_port(&first);
        let err = client(&allow).get(localhost_url(&first, "/r")).send().await.unwrap_err();
        assert!(is_blocked_error(&err), "unexpected error: {err:?}");
        assert!(other.received_requests().await.unwrap().is_empty(), "the other port must not be requested");
    }
```

`src-tauri/src/commands/note.rs` の `mod tests` に、`fetch_guarded_bytes_rejects_hostname_resolving_to_loopback` の直後でテストを追加する(このテストは `allow_localhost(&server)` を使うので、Step 4 で下記のとおりヘルパーを直す)。

```rust
    #[tokio::test]
    async fn fetch_guarded_bytes_rejects_other_port_of_allowlisted_host() {
        let allowed = MockServer::start().await;
        let other = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200)).mount(&other).await;
        let allow = allow_localhost(&allowed);
        let url = format!("http://localhost:{}/f", other.address().port());
        let err = fetch_guarded_bytes(&fetch_client(&allow), &allow, &url).await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("許可されていません")), "unexpected: {err:?}");
        assert!(other.received_requests().await.unwrap().is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

`note.rs` はまだ `allow_localhost()` が引数なしなのでコンパイルが通らない。先に `note.rs` のヘルパーだけ直す(`src-tauri/src/commands/note.rs:644-648`)。

```rust
    /// `localhost:{サーバーのポート}` だけを許可するリスト(`MockServer` はランダムポートで待ち受ける)。
    fn allow_localhost(server: &MockServer) -> crate::net_guard::FetchAllowlist {
        let allow = crate::net_guard::FetchAllowlist::default();
        allow.replace([format!("localhost:{}", server.address().port())]);
        allow
    }
```

`allow_localhost()` を呼んでいる 3 テスト(`fetch_guarded_bytes_returns_body_for_allowed_host`、`fetch_guarded_bytes_maps_http_error_status`、`fetch_guarded_bytes_rejects_oversize_content_length`)の呼び出しを `allow_localhost(&server)` に変える。

Run: `cd src-tauri && cargo test --lib net_guard::tests::validate_url 2>&1 | tail -25 && cargo test --lib fetch_guarded_bytes_rejects_other_port 2>&1 | tail -10`
Expected: FAIL。`validate_url_rejects_other_port_on_allowlisted_hostname`、`validate_url_allows_default_ports_for_portless_hostname`、`validate_url_applies_port_rule_...`、`validate_url_rejects_other_port_on_allowlisted_ip_literal`、`fetch_guarded_bytes_rejects_other_port_of_allowlisted_host` が失敗する(今は許可ホストならポートを問わず通る)。`validate_url_does_not_apply_port_rule_to_unlisted_hosts` は今も通る。

- [ ] **Step 3: 実装する**

`src-tauri/src/net_guard.rs` の `validate_url`(doc コメントごと)を次で置き換える。

```rust
/// スキーム、IPリテラル、許可ホストのポートを検証する。許可リストに無いホスト名は、
/// 接続時にリゾルバが検証するので、ここでは通す。
///
/// 許可リストにあるホストは、DNSリゾルバにポートが渡らず全アドレスが通ってしまうため、
/// URLのポートが許可ポートに含まれるかをここで確認する(Issue #482)。
pub(crate) fn validate_url(url: &url::Url, allow: &FetchAllowlist) -> std::result::Result<(), BlockedAddress> {
    let blocked = |host: String| BlockedAddress { host };
    if !matches!(url.scheme(), "http" | "https") {
        return Err(blocked(format!("scheme {}", url.scheme())));
    }
    let Some(port) = url.port_or_known_default() else {
        return Err(blocked("no port".to_string()));
    };
    match url.host() {
        Some(url::Host::Domain(domain)) => {
            let key = host_key(domain);
            if allow.contains_host(&key) && !allow.allows(&key, port) {
                return Err(blocked(format!("{key}:{port}")));
            }
            Ok(())
        }
        Some(url::Host::Ipv4(v4)) => validate_ip_literal(IpAddr::V4(v4), port, allow),
        Some(url::Host::Ipv6(v6)) => validate_ip_literal(IpAddr::V6(v6), port, allow),
        None => Err(blocked("no host".to_string())),
    }
}

/// IPリテラルのURLを検証する。禁止IPは、許可リストにそのIPがあり、かつポートが許可ポートに含まれる場合だけ通す。
/// 許可リストのキーはポートを除いたホスト部。`ip.to_string()` は既にポートを含まない正規形なので、
/// `host_key` を通さずそのまま照合する(IPv6の `:` をポート区切りと誤認しないため)。
fn validate_ip_literal(ip: IpAddr, port: u16, allow: &FetchAllowlist) -> std::result::Result<(), BlockedAddress> {
    if is_blocked_ip(ip) && !allow.allows(&ip.to_string(), port) {
        return Err(BlockedAddress { host: format!("{ip}:{port}") });
    }
    Ok(())
}
```

- [ ] **Step 4: 新規テストが通り、影響を受ける既存テストが失敗することを確認して直す**

Run: `cd src-tauri && cargo test --lib net_guard 2>&1 | tail -30`
Expected: 新規テストは PASS。既存の `redirect_between_allowed_hosts_is_followed` と `too_many_redirects_is_an_error` が FAIL する(`allow.replace(["localhost"])` はポート無し登録で 80/443 のみとなり、ランダムポートのリダイレクト先が `BlockedAddress` で拒否されるため)。

この 2 テストの `allow` を `allow_localhost_port` に直す(`net_guard.rs` の `mod tests`)。

```rust
    #[tokio::test]
    async fn redirect_between_allowed_hosts_is_followed() {
        let server = MockServer::start().await;
        let target = localhost_url(&server, "/final");
        Mock::given(method("GET")).and(path("/r")).respond_with(ResponseTemplate::new(302).insert_header("Location", target.as_str())).mount(&server).await;
        Mock::given(method("GET")).and(path("/final")).respond_with(ResponseTemplate::new(200).set_body_string("done")).mount(&server).await;
        let allow = allow_localhost_port(&server);
        let body = client(&allow).get(localhost_url(&server, "/r")).send().await.unwrap().text().await.unwrap();
        assert_eq!(body, "done");
    }

    #[tokio::test]
    async fn too_many_redirects_is_an_error() {
        let server = MockServer::start().await;
        let me = localhost_url(&server, "/loop");
        Mock::given(method("GET")).and(path("/loop")).respond_with(ResponseTemplate::new(302).insert_header("Location", me.as_str())).mount(&server).await;
        let allow = allow_localhost_port(&server);
        let err = client(&allow).get(me).send().await.unwrap_err();
        assert!(err.is_redirect(), "unexpected error: {err:?}");
        assert!(!is_blocked_error(&err));
    }
```

`resolver_allows_localhost_when_allowlisted` と `redirect_to_blocked_ip_literal_is_rejected` は、最初のリクエストが `validate_url` を通らず(リダイレクトポリシーは呼ばれない)、リダイレクト先も禁止 IP リテラルなので、変更不要。

Run: `cd src-tauri && cargo test --lib net_guard 2>&1 | tail -8 && cargo test --lib commands::note 2>&1 | tail -8`
Expected: PASS(`net_guard` と `commands::note` の全テスト)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/net_guard.rs src-tauri/src/commands/note.rs
git commit -m "$(cat <<'EOF'
fix: 取得先許可リストのホストでも許可外ポートへの取得を拒否する(Issue #482)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: 既存 spec の改訂と全体確認

**Files:**
- Modify: `docs/superpowers/specs/2026-10-10-ssrf-fetch-guard-design.md`(§1 の `validate_url` の説明、§2、「既知の限界」)

**Interfaces:**
- Consumes: Task 1 / Task 2 の実装(許可リストの型とポート規則)。
- Produces: 実装と一致した仕様書。

- [ ] **Step 1: §1 の IP リテラルの項を置き換える**

`docs/superpowers/specs/2026-10-10-ssrf-fetch-guard-design.md` の次の箇条書き(§1 の `validate_url` の説明)を、

```
- ホストが IP リテラルの場合は、許可リストに同じ文字列(IPリテラルのアカウントホスト)があれば許可し、無ければ `is_blocked_ip` で判定する(IP リテラルは DNS リゾルバを通らないため、ここで止める)。
```

次で置き換える。

```
- ホストが IP リテラルの場合は、`is_blocked_ip` に該当しなければ許可する。該当する場合は、許可リストにそのIP(IPリテラルのアカウントホスト)があり、かつURLのポートがその許可ポートに含まれるときだけ許可する(IP リテラルは DNS リゾルバを通らないため、ここで止める)。
- ホストがドメイン名で許可リストにある場合は、URLのポート(`port_or_known_default()`)が許可ポートに含まれなければ拒否する。DNS リゾルバにはポートが渡らないため、ここで止める(Issue #482)。許可リストに無いドメイン名は通し、接続時にリゾルバが検証する。
```

- [ ] **Step 2: §2 の型・キー・照合の項を置き換える**

次の 2 つの箇条書きを置き換える。

```
- 型は `Arc<RwLock<HashSet<String>>>`。登録済みアカウントのホスト名(小文字化、ポート無し)の集合。
```
→
```
- 型は `Arc<RwLock<HashMap<String, HashSet<u16>>>>`。登録済みアカウントのホスト(照合キー)から、そのホストで許可するポートの集合への対応。ポート付きで登録したホスト(`mi.example.com:3000`)は `{3000}`、ポート無しのホスト(`mi.example.com`、IP直指定の `192.168.1.10` も同様)は `{80, 443}`。同じホストの複数登録は和集合。数値として解釈できないポートのエントリは格納しない。
```

```
- キーが一致するホストだけは private へ解決されても許可する。IP リテラルのURLは、キーと完全一致する場合に限り許可し、それ以外は拒否する。
```
→
```
- キーが一致し、かつURLのポートが許可ポートに含まれるホストだけは private へ解決されても許可する。ポートが許可外なら、公開IPへ解決される場合も `validate_url` が拒否する(Issue #482)。IP リテラルのURLは、キーとポートの両方が一致する場合に限り許可し、それ以外は拒否する。
```

- [ ] **Step 3: 「既知の限界」に追記する**

「既知の限界」の末尾に次の項を足す。

```
- アカウントのホストと同じホスト名で、登録ポート(ポート無しなら 80/443)以外にあるメディアサーバー(同じホスト名の `:9000` の MinIO など)は、公開IPに解決される場合でも拒否される。リゾルバがポートを見られず、許可ホストの条件をポートだけで分けられないため(Issue #482)。
```

- [ ] **Step 4: 全体テストを実行する**

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: PASS(`#[ignore]` のテストはスキップされる)。警告に `unused` / `dead_code` が増えていないことも確認する(`contains_key` を使う箇所が残っていればコンパイルエラーになる)。

- [ ] **Step 5: コミットする**

```bash
git add docs/superpowers/specs/2026-10-10-ssrf-fetch-guard-design.md
git commit -m "$(cat <<'EOF'
docs: 取得先検証の設計書を許可リストのポート単位化に合わせて改訂(Issue #482)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review メモ

- **Spec coverage:** 型の変更・`replace` 不変・`contains_host` / `allows`(§1)→ Task 1。`validate_url` のドメイン名 / IP リテラル検査と `port_or_known_default`(§2)、リダイレクトは `validate_url` 経由で追加変更なし → Task 2。副作用の割り切り(§3)→ Task 3 の「既知の限界」。テスト(§4)→ Task 1 / 2 の各テストと既存テストの修正。仕様書の更新(§5)→ Task 3。
- **型の一貫性:** `contains_host(&str) -> bool`、`allows(&str, u16) -> bool`、`validate_ip_literal(IpAddr, u16, &FetchAllowlist)`、`allow_localhost_port(&MockServer)`(`net_guard.rs`)と `allow_localhost(&MockServer)`(`note.rs`)は Task 間で同じ名前・型を使っている。
- **実機確認:** 単体・結合テストは wiremock のローカルサーバーで通る。実アカウント(ポート付き / 無し)での取得確認は、実装後に別途手動で行う。
