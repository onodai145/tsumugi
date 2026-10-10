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

/// 許可リストの照合キー: 小文字化・punycode化し、ポートと末尾のドットを除いたホスト部。
/// punycode化するのは、リゾルバに渡る名前(`url` クレートがUnicodeのホストを変換したもの)と
/// Unicode表記のアカウントホストを一致させるため。
pub(crate) fn host_key(host: &str) -> String {
    let h = host.trim();
    // 角括弧付きIPv6はアカウントのホストとして来ない(`normalize_host` が `.` を要求する)ため、
    // `host:port` の最後の `:` 以降が数字だけならポートとして落とす。
    let h = match h.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => h,
    };
    // 変換できない文字列は、ASCIIの小文字化だけで従来どおりのキーにする(照合は一致しなくなるだけ)。
    let normalized = match url::Host::parse(h) {
        Ok(parsed) => parsed.to_string(),
        Err(_) => h.to_ascii_lowercase(),
    };
    normalized.trim_end_matches('.').to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
    fn host_key_converts_unicode_host_to_punycode() {
        let key = host_key("みすきー.example:3000");
        assert!(key.starts_with("xn--"), "unexpected key: {key}");
        // reqwest/url がリゾルバへ渡す名前と同じ表記になる
        let ascii = url::Url::parse("http://みすきー.example:3000/").unwrap().host_str().unwrap().to_string();
        assert_eq!(key, ascii);
    }

    #[test]
    fn allowlisted_unicode_host_matches_the_name_the_resolver_receives() {
        let allow = FetchAllowlist::default();
        allow.replace(["みすきー.example:3000"]);
        let ascii = url::Url::parse("http://みすきー.example:3000/").unwrap().host_str().unwrap().to_string();
        let out = filter_addrs(&ascii, vec![sa("192.168.1.10")], &allow).unwrap();
        assert_eq!(out, vec![sa("192.168.1.10")]);
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
}
