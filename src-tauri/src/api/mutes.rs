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

/// `/i` から `mutedWords`(ソフトワードミュート)を取得し、ルール一覧にパースする(Issue #11)。
/// `hardMutedWords`/`mutedInstances` は対象外(サーバー側で既に配信が絞られている前提。
/// 設計doc `docs/superpowers/specs/2026-09-03-server-word-mute-design.md` 参照)。
pub async fn fetch_muted_words(client: &MisskeyClient) -> Result<Vec<WordMuteRule>> {
    let raw: serde_json::Value = client.post("i", &json!({})).await?;
    Ok(parse_muted_words(&raw))
}

/// `/i` の生JSONから `mutedWords` フィールドだけを取り出し、ルール一覧にパースする純粋関数。
/// Misskey の `mutedWords: (string | string[])[]` を変換する:
/// - 配列要素([string]) → 複数語のANDグループ(空語は除去、全滅したグループは無視)
/// - `/pattern/flags` 形式の文字列 → 正規表現ルール(`i` フラグのみ反映。コンパイル失敗は
///   そのルールだけスキップして警告ログを出す)
/// - それ以外の文字列 → 単語1個のANDグループ
pub(crate) fn parse_muted_words(raw: &serde_json::Value) -> Vec<WordMuteRule> {
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
                if words.is_empty() {
                    None
                } else {
                    Some(WordMuteRule::Words(words))
                }
            } else {
                el.as_str().and_then(parse_word_element)
            }
        })
        .collect()
}

/// 1つの文字列要素をパースする。`/pattern/flags` 構文なら正規表現、それ以外は単語1個のANDグループ。
fn parse_word_element(s: &str) -> Option<WordMuteRule> {
    if let Some((pattern, flags)) = try_parse_regex_syntax(s) {
        return match regex::RegexBuilder::new(pattern)
            .case_insensitive(flags.contains('i'))
            .build()
        {
            Ok(re) => Some(WordMuteRule::Regex(re)),
            Err(e) => {
                log::warn!("invalid muted word regex /{pattern}/{flags}: {e}");
                None
            }
        };
    }
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(WordMuteRule::Words(vec![s.to_string()]))
    }
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
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == &["spoiler".to_string()])
        );
    }

    #[test]
    fn parses_array_element_as_and_group() {
        let raw = json!({ "mutedWords": [["foo", "bar"]] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == &["foo".to_string(), "bar".to_string()])
        );
    }

    #[test]
    fn drops_empty_words_within_a_group_and_drops_groups_left_empty() {
        let raw = json!({ "mutedWords": [["", "  ", "bar"], ["", ""]] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == &["bar".to_string()])
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
            matches!(&rules[0], WordMuteRule::Words(w) if w.as_slice() == &["spoiler".to_string()])
        );
    }

    #[test]
    fn missing_muted_words_field_returns_empty() {
        let raw = json!({});
        assert!(parse_muted_words(&raw).is_empty());
    }
}
