//! MCPサーバーへのリクエストの検証(Issue #557)。純粋関数で、HTTPの層(axum の middleware)から呼ぶ。
//! 順序は Host → Origin → bearer。Host / Origin の違反は 403、bearer の不一致は 401。

use axum::http::{header, HeaderMap, Uri};
use subtle::ConstantTimeEq;

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// 403: Host / Origin が不正(DNSリバインディングやブラウザからの呼び出し)。
    Forbidden,
    /// 401: トークンが無い・違う。
    Unauthorized,
}

/// 宛先が `127.0.0.1:<port>` か `localhost:<port>` に完全一致するか(ASCIIの大文字小文字は無視)。
/// Host ヘッダーがあればそれを、URI に authority があれば(HTTP/2 の `:authority` や絶対形のリクエスト)
/// それも検査し、**あるものはすべて**許可されていなければならない。どちらも無ければ拒否する。
pub fn host_allowed(headers: &HeaderMap, uri: &Uri, port: u16) -> bool {
    let ok = |h: &str| {
        let h = h.to_ascii_lowercase();
        h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}")
    };
    let mut seen = false;
    if let Some(v) = headers.get(header::HOST) {
        match v.to_str() {
            Ok(h) if ok(h) => seen = true,
            _ => return false,
        }
    }
    if let Some(a) = uri.authority() {
        if !ok(a.as_str()) {
            return false;
        }
        seen = true;
    }
    seen
}

/// Origin ヘッダーが無いこと。ブラウザは必ず付けるが、正規のMCPクライアントは付けない。
pub fn origin_absent(headers: &HeaderMap) -> bool {
    !headers.contains_key(header::ORIGIN)
}

/// `Authorization: Bearer <token>` が、期待するトークンに定数時間で一致するか。
pub fn bearer_ok(headers: &HeaderMap, expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some((scheme, token)) = value.split_once(' ') else {
        return false;
    };
    let token = token.trim();
    scheme.eq_ignore_ascii_case("bearer")
        && !token.is_empty()
        && bool::from(token.as_bytes().ct_eq(expected.as_bytes()))
}

pub fn check(headers: &HeaderMap, uri: &Uri, port: u16, expected_token: Option<&str>) -> Verdict {
    if !host_allowed(headers, uri, port) || !origin_absent(headers) {
        return Verdict::Forbidden;
    }
    if !bearer_ok(headers, expected_token) {
        return Verdict::Unauthorized;
    }
    Verdict::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue, Uri};

    /// HTTP/1.1 の通常の形(origin-form)。authority を持たない。
    fn path() -> Uri {
        "/mcp".parse().unwrap()
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn host_accepts_loopback_with_the_exact_port_only() {
        assert!(host_allowed(&headers(&[("host", "127.0.0.1:49731")]), &path(), 49731));
        assert!(host_allowed(&headers(&[("host", "localhost:49731")]), &path(), 49731));
        assert!(host_allowed(&headers(&[("host", "LocalHost:49731")]), &path(), 49731), "大文字小文字を区別しない");
        assert!(!host_allowed(&headers(&[("host", "127.0.0.1:49732")]), &path(), 49731), "別ポート");
        assert!(!host_allowed(&headers(&[("host", "127.0.0.1")]), &path(), 49731), "ポート無し");
        assert!(!host_allowed(&headers(&[("host", "evil.example:49731")]), &path(), 49731), "DNSリバインディング");
        assert!(!host_allowed(&headers(&[("host", "[::1]:49731")]), &path(), 49731), "IPv6は許可しない");
        assert!(!host_allowed(&headers(&[("host", "localhost.:49731")]), &path(), 49731), "末尾のドット");
        assert!(!host_allowed(&headers(&[]), &path(), 49731), "Host無し");
    }

    #[test]
    fn http2_style_requests_without_a_host_header_are_checked_by_their_uri_authority() {
        // HTTP/2 には Host ヘッダーが無く、:authority が URI 側に入る。
        let h2 = |a: &str| -> Uri { format!("http://{a}/mcp").parse().unwrap() };
        assert!(host_allowed(&headers(&[]), &h2("127.0.0.1:49731"), 49731));
        assert!(host_allowed(&headers(&[]), &h2("localhost:49731"), 49731));
        assert!(!host_allowed(&headers(&[]), &h2("evil.example:49731"), 49731));
        assert!(!host_allowed(&headers(&[]), &path(), 49731), "Host も authority も無い");
    }

    #[test]
    fn when_both_the_host_header_and_the_uri_authority_exist_both_must_be_allowed() {
        let good: Uri = "http://127.0.0.1:49731/mcp".parse().unwrap();
        let evil: Uri = "http://evil.example:49731/mcp".parse().unwrap();
        assert!(host_allowed(&headers(&[("host", "127.0.0.1:49731")]), &good, 49731));
        assert!(!host_allowed(&headers(&[("host", "127.0.0.1:49731")]), &evil, 49731), "絶対形のURIがevil");
        assert!(!host_allowed(&headers(&[("host", "evil.example:49731")]), &good, 49731), "Hostがevil");
    }

    #[test]
    fn origin_header_is_always_rejected() {
        assert!(origin_absent(&headers(&[])));
        assert!(!origin_absent(&headers(&[("origin", "http://localhost:49731")])), "同じオリジンでも拒否");
        assert!(!origin_absent(&headers(&[("origin", "null")])));
    }

    #[test]
    fn bearer_requires_the_exact_token() {
        let ok = headers(&[("authorization", "Bearer secret-token")]);
        assert!(bearer_ok(&ok, Some("secret-token")));
        assert!(bearer_ok(&headers(&[("authorization", "bearer secret-token")]), Some("secret-token")), "スキーム名は大文字小文字を区別しない");
        assert!(!bearer_ok(&headers(&[("authorization", "Bearer secret-tokeN")]), Some("secret-token")), "1文字違い");
        assert!(!bearer_ok(&headers(&[("authorization", "Bearer secret")]), Some("secret-token")), "長さ違い");
        assert!(!bearer_ok(&headers(&[("authorization", "Bearer ")]), Some("secret-token")), "空");
        assert!(!bearer_ok(&headers(&[("authorization", "Basic secret-token")]), Some("secret-token")), "別のスキーム");
        assert!(!bearer_ok(&headers(&[("authorization", "secret-token")]), Some("secret-token")), "スキーム無し");
        assert!(!bearer_ok(&headers(&[]), Some("secret-token")), "ヘッダー無し");
        assert!(!bearer_ok(&ok, None), "サーバー側にトークンが無いときは常に拒否");
    }

    #[test]
    fn check_orders_host_then_origin_then_bearer() {
        let good = [("host", "127.0.0.1:49731"), ("authorization", "Bearer t")];
        assert_eq!(check(&headers(&good), &path(), 49731, Some("t")), Verdict::Allow);

        let bad_host = [("host", "evil:49731"), ("authorization", "Bearer t")];
        assert_eq!(check(&headers(&bad_host), &path(), 49731, Some("t")), Verdict::Forbidden);

        let with_origin = [("host", "127.0.0.1:49731"), ("origin", "http://x"), ("authorization", "Bearer t")];
        assert_eq!(check(&headers(&with_origin), &path(), 49731, Some("t")), Verdict::Forbidden);

        let no_token = [("host", "127.0.0.1:49731")];
        assert_eq!(check(&headers(&no_token), &path(), 49731, Some("t")), Verdict::Unauthorized);

        // Host が不正なら、トークンが正しくても 403(トークンの有無を探らせない)。
        assert_eq!(check(&headers(&[("host", "evil:1"), ("authorization", "Bearer wrong")]), &path(), 49731, Some("t")), Verdict::Forbidden);
    }
}
