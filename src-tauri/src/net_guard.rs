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

use std::collections::{HashMap, HashSet};
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
    let [a, b, c, _] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || (a == 100 && (64..=127).contains(&b)) // CGNAT 100.64.0.0/10
        || a == 0 // "このネットワーク" 0.0.0.0/8
        || (a == 192 && b == 0 && c == 0) // IETFプロトコル割り当て 192.0.0.0/24
        || (a == 198 && (b & 0xfe) == 18) // ベンチマーク用 198.18.0.0/15
        || a >= 240 // 予約済み 240.0.0.0/4
        || (a == 192 && b == 0 && c == 2) // ドキュメント用 TEST-NET-1 192.0.2.0/24
        || (a == 198 && b == 51 && c == 100) // ドキュメント用 TEST-NET-2 198.51.100.0/24
        || (a == 203 && b == 0 && c == 113) // ドキュメント用 TEST-NET-3 203.0.113.0/24
        || (a == 192 && b == 88 && c == 99) // 廃止済みの6to4リレー 192.88.99.0/24
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    let seg = ip.segments();
    let first = seg[0];
    // 下位32ビットにIPv4を埋め込む NAT64 64:ff9b::/96。埋め込みのIPv4で判定する。
    if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        let [_, _, _, _, _, _, hi, lo] = seg;
        return is_blocked_v4(Ipv4Addr::from(((hi as u32) << 16) | lo as u32));
    }
    // 上位16ビット目から32ビットにIPv4を埋め込む 6to4 2002::/16。埋め込みのIPv4で判定する。
    if first == 0x2002 {
        return is_blocked_v4(Ipv4Addr::from(((seg[1] as u32) << 16) | seg[2] as u32));
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00 // unique-local fc00::/7
        || (first & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (first & 0xffc0) == 0xfec0 // site-local fec0::/10(廃止済み)
        || seg[..6] == [0; 6] // IPv4互換 ::/96(廃止済み)
        || seg[..4] == [0x100, 0, 0, 0] // discard 100::/64
        || seg[..3] == [0x64, 0xff9b, 1] // ローカル用NAT64 64:ff9b:1::/48(埋め込み位置が可変なので範囲ごと拒否)
        || seg[..4] == [0x100, 0, 0, 1] // ダミー 100:0:0:1::/64
        || (first == 0x2001 && seg[1] == 0) // Teredo 2001::/32(IPv4を埋め込むので範囲ごと拒否)
        || (first == 0x2001 && seg[1] == 0xdb8) // ドキュメント用 2001:db8::/32
        || (first == 0x3fff && (seg[1] & 0xf000) == 0) // ドキュメント用 3fff::/20
        || seg[..3] == [0x2001, 2, 0] // ベンチマーク用 2001:2::/48
        || (first == 0x2001 && (seg[1] & 0xfff0) == 0x10) // 廃止済みORCHID 2001:10::/28
        || first == 0x5f00 // SRv6 SID 5f00::/16
        || seg[..6] == [0, 0, 0, 0, 0xffff, 0] // 旧SIIT形式のIPv4-translated ::ffff:0:0:0/96(登録簿に無い廃止済み形式)
}

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
        // http/https でホストの無いURLは `url` クレートがパースしないため実際には到達しない。
        // `Option` を網羅するための分岐で、万一来ても通さない側に倒す。
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

/// 名前解決の結果から禁止IPを除く。許可リストのホスト名なら全て通す。全て除かれたらエラー。
pub(crate) fn filter_addrs(
    host: &str,
    addrs: Vec<SocketAddr>,
    allow: &FetchAllowlist,
) -> std::result::Result<Vec<SocketAddr>, BlockedAddress> {
    if allow.contains_host(&host_key(host)) {
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
            "0.0.0.1", "0.255.255.255", "192.0.0.0", "192.0.0.255", "198.18.0.0", "198.19.255.255",
            "240.0.0.0", "254.255.255.255",
            // ドキュメント用 TEST-NET-1/2/3 と、廃止済みの 6to4 リレー 192.88.99.0/24
            "192.0.2.0", "192.0.2.255", "198.51.100.0", "198.51.100.255", "203.0.113.0", "203.0.113.255",
            "192.88.99.0", "192.88.99.255",
        ] {
            assert!(is_blocked_ip(ip(s)), "{s} should be blocked");
        }
    }

    #[test]
    fn allows_ipv4_just_outside_special_ranges() {
        for s in ["8.8.8.8", "1.1.1.1", "172.15.255.255", "172.32.0.1", "100.63.255.255", "100.128.0.1", "192.167.1.1", "11.0.0.1",
            "1.0.0.0", "192.0.1.0", "198.17.255.255", "198.20.0.0", "223.255.255.255",
            "192.0.3.0", "198.51.99.255", "198.51.101.0", "203.0.112.255", "203.0.114.0", "192.88.98.255", "192.88.100.0",
        ] {
            assert!(!is_blocked_ip(ip(s)), "{s} should be allowed");
        }
    }

    #[test]
    fn blocks_ipv6_special_ranges() {
        for s in [
            "::1", "::", "fc00::1", "fd12:3456::1", "fe80::1", "ff02::1",
            // site-local fec0::/10(廃止済み)
            "fec0::1", "feff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
            // IPv4互換 ::/96(廃止済み)。埋め込みが公開IPでも範囲ごと拒否する
            "::2", "::7f00:1", "::8.8.8.8", "::ffff:ffff",
            // discard 100::/64
            "100::", "100::ffff:ffff:ffff:ffff",
            // ローカル用NAT64 64:ff9b:1::/48。埋め込みが公開IPでも範囲ごと拒否する
            "64:ff9b:1::", "64:ff9b:1:ffff:ffff:ffff:ffff:ffff", "64:ff9b:1::808:808",
            // ダミー 100:0:0:1::/64
            "100:0:0:1::", "100:0:0:1:ffff:ffff:ffff:ffff",
            // Teredo 2001::/32(IPv4を埋め込む。範囲ごと拒否)
            "2001::", "2001:0:4136:e378:8000:63bf:3fff:fdd2", "2001:0:ffff:ffff:ffff:ffff:ffff:ffff",
            // ドキュメント用 2001:db8::/32 と 3fff::/20
            "2001:db8::", "2001:db8:ffff:ffff:ffff:ffff:ffff:ffff", "3fff::", "3fff:fff:ffff:ffff:ffff:ffff:ffff:ffff",
            // ベンチマーク用 2001:2::/48、廃止済みORCHID 2001:10::/28、SRv6 SID 5f00::/16
            "2001:2::", "2001:2:0:ffff:ffff:ffff:ffff:ffff", "2001:10::", "2001:1f:ffff:ffff:ffff:ffff:ffff:ffff",
            "5f00::", "5f00:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
            // 旧SIIT形式のIPv4-translated ::ffff:0:0:0/96(登録簿に無い廃止済み形式。範囲ごと拒否)
            "::ffff:0:7f00:1", "::ffff:0:8.8.8.8", "::ffff:0:ffff:ffff",
        ] {
            assert!(is_blocked_ip(ip(s)), "{s} should be blocked");
        }
    }

    #[test]
    fn allows_ipv6_just_outside_special_ranges() {
        for s in [
            "2606:4700:4700::1111", "fe7f::1", "::1:0:0", "101::1", "64:ff9b:2::1",
            "64:ff9c::1", "64:ff9a::1", "64:ff9b:0:1::1",
            "100:0:0:2::", "100:1::",
            // 登録簿で全世界から到達可能とされる 2001::/23 内の細分は拒否しない
            "2001:1::1", "2001:1::2", "2001:1::3", "2001:3::1", "2001:4:112::1", "2001:20::1", "2001:30::1", "2620:4f:8000::1",
            // 禁止範囲の境界の外側
            "2001:1::", "2001:db7:ffff:ffff:ffff:ffff:ffff:ffff", "2001:db9::", "2001:2:1::", "2001:f:ffff:ffff:ffff:ffff:ffff:ffff",
            "3ffe:ffff::1", "3fff:1000::", "5eff:ffff::1", "5f01::", "::ffff:1:0:0",
        ] {
            assert!(!is_blocked_ip(ip(s)), "{s} should be allowed");
        }
    }

    #[test]
    fn nat64_is_judged_by_embedded_ipv4() {
        for s in [
            "64:ff9b::7f00:1", "64:ff9b::c0a8:101", "64:ff9b::a9fe:a9fe", "64:ff9b::a00:1",
            "64:ff9b::c000:1", "64:ff9b::f000:1", "64:ff9b::127.0.0.1",
        ] {
            assert!(is_blocked_ip(ip(s)), "{s} should be blocked");
        }
        for s in ["64:ff9b::808:808", "64:ff9b::101:101", "64:ff9b::8.8.8.8"] {
            assert!(!is_blocked_ip(ip(s)), "{s} should be allowed");
        }
    }

    #[test]
    fn sixto4_is_judged_by_embedded_ipv4() {
        for s in ["2002:7f00:1::", "2002:7f00:1::1", "2002:c0a8:101::1", "2002:a9fe:a9fe::1", "2002:0a00:0001:1:2:3:4:5"] {
            assert!(is_blocked_ip(ip(s)), "{s} should be blocked");
        }
        for s in ["2002:808:808::", "2002:808:808:1::1", "2001:7f00:1::1", "2003:7f00:1::1"] {
            assert!(!is_blocked_ip(ip(s)), "{s} should be allowed");
        }
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
        assert!(allow.contains_host("mi.example.com"));
        assert!(allow.contains_host("localhost"));
        allow.replace(["other.example.com"]);
        assert!(!allow.contains_host("mi.example.com"));
        assert!(allow.contains_host("other.example.com"));
    }

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
            "http://[64:ff9b::7f00:1]/",
            "http://[64:ff9b::c0a8:101]/",
            "http://[2002:c0a8:101::1]/",
            "http://240.0.0.1/",
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

    #[test]
    fn is_blocked_error_is_false_for_unrelated_errors() {
        let e = std::io::Error::other("boom");
        assert!(!is_blocked_error(&e));
    }
}
