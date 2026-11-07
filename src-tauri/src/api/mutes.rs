//! サーバ側ミュート/ブロック・ワードミュートの取得。
//! - `mute/list`/`blocking/list`: 対象ユーザの userId 集合(Krile MuteBlockManager 相当)。
//! - `/i` の `mutedWords`: ソフトワードミュートのルール一覧(Issue #11)。

use crate::api::MisskeyClient;
use crate::error::Result;
use crate::filter::mute::WordMuteRule;
use serde_json::{json, Value};
use std::collections::HashSet;

const PAGE: u32 = 100;
const MAX_PAGES: usize = 20; // 安全弁（最大 2000 件）

/// `mute/list` + `blocking/list` から読んだ、表示を抑制する対象。
#[derive(Debug, Default)]
pub struct ServerMutes {
    /// ミュート+ブロックしているユーザの userId の和集合(期限切れのミュートは除く)。
    pub ids: HashSet<String>,
    /// 有効なミュートのうち最も早い期限(エポックミリ秒)。期限付きのミュートが無ければ `None`。
    pub next_expiry_at_ms: Option<i64>,
}

/// サーバ側でミュート＋ブロックしているユーザの userId 集合を取得する。
/// どちらも「表示を抑制する」用途なので和集合で返す。`expiresAt <= now_ms` のミュートは、サーバーが
/// まだ `mute/list` から消していなくても除外する(時限ミュート。Issue #545)。
pub async fn fetch_muted_and_blocked(client: &MisskeyClient, now_ms: i64) -> Result<ServerMutes> {
    let mut out = ServerMutes::default();
    collect(client, "mute/list", "muteeId", now_ms, &mut out).await?;
    collect(client, "blocking/list", "blockeeId", now_ms, &mut out).await?;
    Ok(out)
}

/// 1レコードを `(対象 userId, 期限ミリ秒)` に読む。対象が無い、または期限切れ(`expiresAt <= now_ms`)なら
/// `None`。`expiresAt` が無い(ブロックなど)・`null`・パース不能なら、期限なしの有効なレコードとして扱う
/// (取りこぼして、ミュートが外れるよりは安全側)。
fn read_row(rec: &Value, id_field: &str, now_ms: i64) -> Option<(String, Option<i64>)> {
    let user_id = rec.get(id_field)?.as_str()?.to_string();
    let expires_at = rec.get("expiresAt").and_then(Value::as_str).and_then(rfc3339_millis);
    match expires_at {
        Some(t) if t <= now_ms => None,
        other => Some((user_id, other)),
    }
}

fn rfc3339_millis(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

/// ページングしながら各レコードの `id_field`（対象 userId）を集める。
/// レコード自身の `id` を untilId に使って過去方向へ辿る(期限切れで除外したレコードも、ページ送りには使う)。
async fn collect(
    client: &MisskeyClient,
    endpoint: &str,
    id_field: &str,
    now_ms: i64,
    out: &mut ServerMutes,
) -> Result<()> {
    let mut until: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut body = json!({ "limit": PAGE });
        if let Some(u) = &until {
            body["untilId"] = json!(u);
        }
        let page: Vec<Value> = client.post(endpoint, &body).await?;
        if page.is_empty() {
            break;
        }
        for rec in &page {
            if let Some((user_id, expires_at)) = read_row(rec, id_field, now_ms) {
                out.ids.insert(user_id);
                if let Some(t) = expires_at {
                    out.next_expiry_at_ms = Some(out.next_expiry_at_ms.map_or(t, |cur| cur.min(t)));
                }
            }
        }
        // 次ページの until はレコードの id
        until = page
            .last()
            .and_then(|r| r.get("id").and_then(|v| v.as_str()))
            .map(str::to_string);
        if until.is_none() || page.len() < PAGE as usize {
            break;
        }
    }
    Ok(())
}

/// ユーザーをミュートする。`mute/create`。`expires_at_ms` は**ミリ秒**のエポック時刻(`None` は無期限)。
/// すでにミュート中なら `ALREADY_MUTING` で失敗する(期限の変更は、解除してから作り直す)。
pub async fn mute_user(client: &MisskeyClient, user_id: &str, expires_at_ms: Option<i64>) -> Result<()> {
    let _: Value = client
        .post("mute/create", &json!({ "userId": user_id, "expiresAt": expires_at_ms }))
        .await?;
    Ok(())
}

/// ユーザーのミュートを解除する。`mute/delete`。
pub async fn unmute_user(client: &MisskeyClient, user_id: &str) -> Result<()> {
    let _: Value = client.post("mute/delete", &json!({ "userId": user_id })).await?;
    Ok(())
}

/// `mutedWords` の1要素。`rule` は適用するルール(不正な正規表現では `None`)、`key` はサーバー側の
/// 要素を表す安定した文字列で、`ServerMuteSnapshot::words` の要素になる(Issue #456)。
/// キーをパース後のルールではなく生の要素から作るので、`i` フラグだけの変更を区別でき、
/// 不正で落とされた正規表現も、サーバーに残っている間はキーが残る。
#[derive(Debug)]
pub(crate) struct MutedWordEntry {
    pub rule: Option<WordMuteRule>,
    pub key: String,
}

/// サーバー側ワードミュートの取得結果。`rules` は適用するルール、`keys` はスナップショット用のキー。
pub struct MutedWords {
    pub rules: Vec<WordMuteRule>,
    pub keys: Vec<String>,
}

/// `/i` から `mutedWords`(ソフトワードミュート)を取得し、ルール一覧とキー一覧にパースする(Issue #11)。
/// `hardMutedWords`/`mutedInstances` は対象外(サーバー側で既に配信が絞られている前提。
/// 設計doc `docs/superpowers/specs/2026-09-03-server-word-mute-design.md` 参照)。
pub async fn fetch_muted_words(client: &MisskeyClient) -> Result<MutedWords> {
    let raw: serde_json::Value = client.post("i", &json!({})).await?;
    let entries = parse_muted_word_entries(&raw);
    let keys = entries.iter().map(|e| e.key.clone()).collect();
    let rules = entries.into_iter().filter_map(|e| e.rule).collect();
    Ok(MutedWords { rules, keys })
}

/// `/i` の生JSONから `mutedWords` フィールドだけを取り出し、ルール一覧にパースする純粋関数。
/// `parse_muted_word_entries` のルールだけを返す薄いラッパ。本番の経路は `fetch_muted_words` が
/// エントリを直接使うので、テスト専用(ルールの変換だけを見るテストが使う)。
#[cfg(test)]
pub(crate) fn parse_muted_words(raw: &serde_json::Value) -> Vec<WordMuteRule> {
    parse_muted_word_entries(raw).into_iter().filter_map(|e| e.rule).collect()
}

/// `/i` の生JSONから `mutedWords` を取り出し、要素ごとにルールとキーを作る純粋関数。
/// Misskey の `mutedWords: (string | string[])[]` を変換する:
/// - 配列要素([string]) → 複数語のANDグループ(空語は除去、全滅したグループは要素にしない)
/// - `/pattern/flags` 形式の文字列 → 正規表現ルール(`i` フラグのみ反映。コンパイル失敗は
///   `rule` を `None` にして警告ログを出す。キーは作る)
/// - それ以外の文字列 → 単語1個のANDグループ(trim して空なら要素にしない)
///
/// 文字列でも配列でもない要素と、配列でない `mutedWords` は無視する。
pub(crate) fn parse_muted_word_entries(raw: &serde_json::Value) -> Vec<MutedWordEntry> {
    let Some(arr) = raw.get("mutedWords").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|el| {
            if let Some(words) = el.as_array() {
                let words: Vec<String> = words
                    .iter()
                    .filter_map(|w| w.as_str())
                    .map(str::trim)
                    .filter(|w| !w.is_empty())
                    .map(str::to_string)
                    .collect();
                words_entry(words)
            } else {
                el.as_str().and_then(parse_word_element)
            }
        })
        .collect()
}

/// ANDグループのキー。語を小文字にしてソートし、区切り文字(`\u{1f}`)で連結して `w:` を付ける。
/// AND は順序に依らず、照合は大小無視のため、順序と大文字小文字に依らない。区切り文字で連結するので、
/// `"ab"` と `"a","b"` は区別される。形式は #454 の `WordMuteRule::key()` と同じ(保存済みの値と比較できる)。
fn words_key(words: &[String]) -> String {
    let mut lowered: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    lowered.sort();
    format!("w:{}", lowered.join("\u{1f}"))
}

fn words_entry(words: Vec<String>) -> Option<MutedWordEntry> {
    if words.is_empty() {
        None
    } else {
        Some(MutedWordEntry { key: words_key(&words), rule: Some(WordMuteRule::Words(words)) })
    }
}

/// 1つの文字列要素をパースする。`/pattern/flags` 構文なら正規表現、それ以外は単語1個のANDグループ。
/// 正規表現のキーは、要素の文字列そのまま(`r:/pattern/flags`)。ルールが作れなくても、キーは作る。
fn parse_word_element(s: &str) -> Option<MutedWordEntry> {
    if let Some((pattern, flags)) = try_parse_regex_syntax(s) {
        let key = format!("r:{s}");
        if !is_valid_regex_flags(flags) {
            log::warn!("invalid muted word regex flags /{pattern}/{flags}: unrecognized flag character");
            return Some(MutedWordEntry { rule: None, key });
        }
        let rule = match regex::RegexBuilder::new(pattern)
            .case_insensitive(flags.contains('i'))
            .build()
        {
            Ok(re) => Some(WordMuteRule::Regex(re)),
            Err(e) => {
                log::warn!("invalid muted word regex /{pattern}/{flags}: {e}");
                None
            }
        };
        return Some(MutedWordEntry { rule, key });
    }
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        words_entry(vec![s.to_string()])
    }
}

/// `flags` がJSの正規表現flag文字(d, g, i, m, s, u, v, y)のみで構成されているかを検証する。
fn is_valid_regex_flags(flags: &str) -> bool {
    flags
        .chars()
        .all(|c| matches!(c, 'd' | 'g' | 'i' | 'm' | 's' | 'u' | 'v' | 'y'))
}

/// `/pattern/flags` 構文なら `(pattern, flags)` を返す。先頭が `/` で、残りに区切りの `/` が
/// 存在し(空パターンは除く)一致した場合のみ Some。
fn try_parse_regex_syntax(s: &str) -> Option<(&str, &str)> {
    let rest = s.strip_prefix('/')?;
    let last_slash = rest.rfind('/')?;
    if last_slash == 0 {
        return None;
    }
    Some((&rest[..last_slash], &rest[last_slash + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::mute::WordMuteRule;
    use serde_json::json;

    #[test]
    fn parses_plain_string_as_single_word_group() {
        let raw = json!({ "mutedWords": ["spoiler"] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == ["spoiler".to_string()].as_slice())
        );
    }

    #[test]
    fn parses_array_element_as_and_group() {
        let raw = json!({ "mutedWords": [["foo", "bar"]] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == ["foo".to_string(), "bar".to_string()].as_slice())
        );
    }

    #[test]
    fn drops_empty_words_within_a_group_and_drops_groups_left_empty() {
        let raw = json!({ "mutedWords": [["", "  ", "bar"], ["", ""]] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == ["bar".to_string()].as_slice())
        );
    }

    #[test]
    fn parses_regex_syntax_with_case_insensitive_flag() {
        let raw = json!({ "mutedWords": ["/sp.iler/i"] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        let WordMuteRule::Regex(re) = &rules[0] else {
            panic!("expected Regex rule")
        };
        assert!(re.is_match("a SPXiler word"));
    }

    #[test]
    fn invalid_regex_is_skipped_but_other_rules_survive() {
        let raw = json!({ "mutedWords": ["/(unclosed/i", "spoiler"] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == ["spoiler".to_string()].as_slice())
        );
    }

    #[test]
    fn missing_muted_words_field_returns_empty() {
        let raw = json!({});
        assert!(parse_muted_words(&raw).is_empty());
    }

    #[test]
    fn regex_syntax_with_unrecognized_flag_characters_is_skipped() {
        let raw = json!({ "mutedWords": ["/r/anime", "spoiler"] });
        let rules = parse_muted_words(&raw);
        // "/r/anime" must NOT become a live regex (flags "anime" contains invalid chars n,e,a);
        // it must be skipped entirely, not fall back to a literal word either.
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == ["spoiler".to_string()].as_slice()));
    }

    #[test]
    fn regex_syntax_with_only_valid_flags_still_compiles() {
        let raw = json!({ "mutedWords": ["/spoiler/gi"] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        let WordMuteRule::Regex(re) = &rules[0] else { panic!("expected Regex rule") };
        assert!(re.is_match("BIG SPOILER"));
    }

    fn keys_of(raw: serde_json::Value) -> Vec<String> {
        parse_muted_word_entries(&raw).into_iter().map(|e| e.key).collect()
    }

    #[test]
    fn word_key_format_is_unchanged_so_saved_snapshots_stay_comparable() {
        assert_eq!(keys_of(json!({ "mutedWords": [["Foo", "bar"]] })), vec!["w:bar\u{1f}foo".to_string()]);
    }

    #[test]
    fn word_key_ignores_word_order_and_case() {
        let a = keys_of(json!({ "mutedWords": [["Foo", "bar"]] }));
        let b = keys_of(json!({ "mutedWords": [["BAR", "foo"]] }));

        assert_eq!(a, b);
    }

    #[test]
    fn word_key_distinguishes_different_groups() {
        let one = keys_of(json!({ "mutedWords": [["foo"]] }));
        let two = keys_of(json!({ "mutedWords": [["foo", "bar"]] }));

        assert_ne!(one, two);
    }

    #[test]
    fn word_key_does_not_confuse_one_joined_word_with_two_words() {
        let joined = keys_of(json!({ "mutedWords": [["ab"]] }));
        let split = keys_of(json!({ "mutedWords": [["a", "b"]] }));

        assert_ne!(joined, split);
    }

    #[test]
    fn plain_string_has_the_same_key_as_a_one_word_group() {
        let plain = keys_of(json!({ "mutedWords": ["Foo"] }));
        let group = keys_of(json!({ "mutedWords": [["foo"]] }));

        assert_eq!(plain, group);
    }

    #[test]
    fn regex_key_is_the_raw_element_and_stays_apart_from_words() {
        let regex = keys_of(json!({ "mutedWords": ["/foo/i"] }));
        let word = keys_of(json!({ "mutedWords": ["foo"] }));

        assert_eq!(regex, vec!["r:/foo/i".to_string()]);
        assert!(word[0].starts_with("w:"));
    }

    #[test]
    fn regex_key_differs_when_only_the_i_flag_is_removed() {
        let with_i = keys_of(json!({ "mutedWords": ["/x/i"] }));
        let without = keys_of(json!({ "mutedWords": ["/x/"] }));

        assert_ne!(with_i, without);
    }

    #[test]
    fn invalid_regex_keeps_its_key_but_has_no_rule() {
        let entries = parse_muted_word_entries(&json!({ "mutedWords": ["/(unclosed/i", "/r/anime"] }));

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.rule.is_none()));
        assert_eq!(entries[0].key, "r:/(unclosed/i"); // コンパイル失敗
        assert_eq!(entries[1].key, "r:/r/anime"); // 不正なフラグ文字
    }

    #[test]
    fn elements_that_end_up_empty_produce_no_entry() {
        let raw = json!({ "mutedWords": [["", "  "], "", "   ", []] });

        assert!(parse_muted_word_entries(&raw).is_empty());
    }

    #[test]
    fn a_lone_slash_and_an_empty_pattern_are_plain_words_not_regexes() {
        let entries = parse_muted_word_entries(&json!({ "mutedWords": ["/", "//"] }));

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.key.starts_with("w:")));
        assert!(entries.iter().all(|e| matches!(&e.rule, Some(WordMuteRule::Words(_)))));
    }

    #[test]
    fn non_string_non_array_elements_and_a_non_array_field_are_ignored() {
        assert!(parse_muted_word_entries(&json!({ "mutedWords": [1, null, { "a": 1 }, true] })).is_empty());
        assert!(parse_muted_word_entries(&json!({ "mutedWords": "spoiler" })).is_empty());
        assert!(parse_muted_word_entries(&json!({ "mutedWords": { "a": 1 } })).is_empty());
    }

    // --- 時限ミュート(Issue #545) ---

    /// 2023-11-14T22:13:20Z。
    const NOW_MS: i64 = 1_700_000_000_000;

    #[test]
    fn read_row_without_expires_at_is_active_and_has_no_expiry() {
        let row = json!({ "id": "r1", "muteeId": "u1" });
        assert_eq!(read_row(&row, "muteeId", NOW_MS), Some(("u1".to_string(), None)));
    }

    #[test]
    fn read_row_with_null_expires_at_is_active_and_has_no_expiry() {
        let row = json!({ "id": "r1", "muteeId": "u1", "expiresAt": null });
        assert_eq!(read_row(&row, "muteeId", NOW_MS), Some(("u1".to_string(), None)));
    }

    #[test]
    fn read_row_with_future_expires_at_keeps_the_expiry_in_milliseconds() {
        let row = json!({ "muteeId": "u1", "expiresAt": "2023-11-14T22:13:21.000Z" });
        assert_eq!(
            read_row(&row, "muteeId", NOW_MS),
            Some(("u1".to_string(), Some(1_700_000_001_000)))
        );
    }

    #[test]
    fn read_row_with_past_expires_at_is_dropped() {
        let row = json!({ "muteeId": "u1", "expiresAt": "2023-11-14T22:13:19.000Z" });
        assert_eq!(read_row(&row, "muteeId", NOW_MS), None);
    }

    #[test]
    fn read_row_expiring_exactly_now_is_dropped() {
        let row = json!({ "muteeId": "u1", "expiresAt": "2023-11-14T22:13:20.000Z" });
        assert_eq!(read_row(&row, "muteeId", NOW_MS), None);
    }

    #[test]
    fn read_row_with_unparsable_expires_at_stays_muted_without_an_expiry() {
        let row = json!({ "muteeId": "u1", "expiresAt": "not a date" });
        assert_eq!(read_row(&row, "muteeId", NOW_MS), Some(("u1".to_string(), None)));
    }

    #[test]
    fn read_row_without_the_id_field_is_dropped() {
        let row = json!({ "id": "r1", "expiresAt": null });
        assert_eq!(read_row(&row, "muteeId", NOW_MS), None);
    }

    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(mock: &MockServer) -> crate::api::MisskeyClient {
        crate::api::MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None)
    }

    #[tokio::test]
    async fn mute_user_sends_the_expiry_as_a_number() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mute/create"))
            .and(body_partial_json(json!({ "userId": "u1", "expiresAt": 1_700_000_000_000_i64 })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;

        mute_user(&test_client(&mock), "u1", Some(1_700_000_000_000)).await.unwrap();
    }

    #[tokio::test]
    async fn mute_user_sends_null_for_an_indefinite_mute() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mute/create"))
            .and(body_partial_json(json!({ "userId": "u1", "expiresAt": null })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;

        mute_user(&test_client(&mock), "u1", None).await.unwrap();
    }

    #[tokio::test]
    async fn unmute_user_posts_the_user_id_to_mute_delete() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mute/delete"))
            .and(body_partial_json(json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;

        unmute_user(&test_client(&mock), "u1").await.unwrap();
    }

    #[tokio::test]
    async fn fetch_muted_and_blocked_drops_expired_mutes_and_reports_the_earliest_expiry() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": "r1", "muteeId": "expired", "expiresAt": "2023-11-14T22:13:19.000Z" },
                { "id": "r2", "muteeId": "later", "expiresAt": "2023-11-14T23:00:00.000Z" },
                { "id": "r3", "muteeId": "sooner", "expiresAt": "2023-11-14T22:30:00.000Z" },
                { "id": "r4", "muteeId": "forever", "expiresAt": null },
            ])))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": "b1", "blockeeId": "blocked" },
            ])))
            .mount(&mock)
            .await;

        let got = fetch_muted_and_blocked(&test_client(&mock), NOW_MS).await.unwrap();

        let mut ids: Vec<&str> = got.ids.iter().map(String::as_str).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["blocked", "forever", "later", "sooner"]);
        // 22:30:00Z = 1_700_001_000_000
        assert_eq!(got.next_expiry_at_ms, Some(1_700_001_000_000));
    }

    #[tokio::test]
    async fn fetch_muted_and_blocked_has_no_next_expiry_when_nothing_expires() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": "r1", "muteeId": "u1" }])))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&mock)
            .await;

        let got = fetch_muted_and_blocked(&test_client(&mock), NOW_MS).await.unwrap();

        assert_eq!(got.next_expiry_at_ms, None);
    }
}
