//! サーバ側ミュート/ブロック・ワードミュートの取得。
//! - `mute/list`/`blocking/list`: 対象ユーザの userId 集合(Krile MuteBlockManager 相当)。
//! - `/i` の `mutedWords`: ソフトワードミュートのルール一覧(Issue #11)。

use crate::api::MisskeyClient;
use crate::error::Result;
use crate::filter::mute::WordMuteRule;
use serde_json::json;
use std::collections::HashSet;

const PAGE: u32 = 100;
const MAX_PAGES: usize = 20; // 安全弁（最大 2000 件）

/// サーバ側でミュート＋ブロックしているユーザの userId 集合を取得する。
/// どちらも「表示を抑制する」用途なので和集合で返す。
pub async fn fetch_muted_and_blocked(client: &MisskeyClient) -> Result<HashSet<String>> {
    let mut ids = HashSet::new();
    collect(client, "mute/list", "muteeId", &mut ids).await?;
    collect(client, "blocking/list", "blockeeId", &mut ids).await?;
    Ok(ids)
}

/// ページングしながら各レコードの `id_field`（対象 userId）を集める。
/// レコード自身の `id` を untilId に使って過去方向へ辿る。
async fn collect(
    client: &MisskeyClient,
    endpoint: &str,
    id_field: &str,
    out: &mut HashSet<String>,
) -> Result<()> {
    let mut until: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut body = json!({ "limit": PAGE });
        if let Some(u) = &until {
            body["untilId"] = json!(u);
        }
        let page: Vec<serde_json::Value> = client.post(endpoint, &body).await?;
        if page.is_empty() {
            break;
        }
        for rec in &page {
            if let Some(uid) = rec.get(id_field).and_then(|v| v.as_str()) {
                out.insert(uid.to_string());
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
}
