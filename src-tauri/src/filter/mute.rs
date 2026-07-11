//! ローカル NG（ミュート）判定。ノートが NG に該当すれば非表示にする。
//! Renote 先も対象（NG ユーザ/語を含む投稿の Renote も隠す）。

use crate::domain::{MuteConfig, Note, User};

/// note が NG に該当するか（本体 or renote 先のいずれかが該当で true）。
pub fn is_muted(note: &Note, cfg: &MuteConfig) -> bool {
    if is_muted_one(note, cfg) {
        return true;
    }
    matches!(&note.renote, Some(r) if is_muted_one(r, cfg))
}

fn is_muted_one(n: &Note, cfg: &MuteConfig) -> bool {
    // ユーザ/インスタンス（通知にも使うため共通化）
    if is_user_muted(&n.user, cfg) {
        return true;
    }
    // NG ワード（本文 + CW を対象に部分一致）
    let hay = format!(
        "{} {}",
        n.text.as_deref().unwrap_or(""),
        n.cw.as_deref().unwrap_or("")
    )
    .to_lowercase();
    cfg.ng_words
        .iter()
        .any(|w| !w.trim().is_empty() && hay.contains(&w.trim().to_lowercase()))
}

/// ユーザが NG（インスタンス/ユーザ）に該当するか。NG ワードは見ない。
/// 通知の発生元ユーザ判定にも使う。
pub fn is_user_muted(user: &User, cfg: &MuteConfig) -> bool {
    // インスタンス
    if let Some(host) = &user.host {
        let h = host.to_lowercase();
        if cfg
            .ng_instances
            .iter()
            .any(|i| !i.trim().is_empty() && i.trim().to_lowercase() == h)
        {
            return true;
        }
    }
    // ユーザ（acct 比較。設定値は @ 省略可）
    let acct = user.acct().to_lowercase();
    cfg.ng_users
        .iter()
        .any(|u| !u.trim().is_empty() && normalize_acct(u) == acct)
}

/// "@user@host" / "user@host" を小文字の "@user@host" に正規化。
fn normalize_acct(s: &str) -> String {
    let t = s.trim().to_lowercase();
    if t.starts_with('@') {
        t
    } else {
        format!("@{t}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{User, Visibility};
    use std::collections::HashMap;

    fn note(text: &str, username: &str, host: Option<&str>) -> Note {
        Note {
            id: "n".into(),
            created_at: 0,
            text: Some(text.into()),
            cw: None,
            visibility: Visibility::Public,
            local_only: false,
            user: User {
                id: "u".into(),
                username: username.into(),
                host: host.map(|h| h.into()),
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: std::collections::HashMap::new(),
            },
            reply_id: None,
            renote_id: None,
            renote: None,
            files: vec![],
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: std::collections::HashMap::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: HashMap::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    #[test]
    fn ng_word_matches_text_case_insensitive() {
        let cfg = MuteConfig {
            ng_words: vec!["Spoiler".into()],
            ..Default::default()
        };
        assert!(is_muted(&note("big spoiler here", "a", None), &cfg));
        assert!(!is_muted(&note("nothing", "a", None), &cfg));
    }

    #[test]
    fn ng_user_matches_acct_with_or_without_at() {
        let cfg = MuteConfig {
            ng_users: vec!["bob@ex.com".into()],
            ..Default::default()
        };
        assert!(is_muted(&note("hi", "bob", Some("ex.com")), &cfg));
        assert!(!is_muted(&note("hi", "alice", Some("ex.com")), &cfg));
    }

    #[test]
    fn ng_instance_matches_host() {
        let cfg = MuteConfig {
            ng_instances: vec!["spam.example".into()],
            ..Default::default()
        };
        assert!(is_muted(&note("hi", "x", Some("Spam.Example")), &cfg));
        assert!(!is_muted(&note("hi", "x", None), &cfg)); // local
    }

    #[test]
    fn muted_renote_target_hides_the_renote() {
        let cfg = MuteConfig {
            ng_words: vec!["bad".into()],
            ..Default::default()
        };
        let mut rn = note("clean", "a", None);
        rn.renote = Some(Box::new(note("bad content", "b", None)));
        assert!(is_muted(&rn, &cfg));
    }

    #[test]
    fn empty_config_mutes_nothing() {
        let cfg = MuteConfig::default();
        assert!(!is_muted(&note("anything", "a", Some("h")), &cfg));
    }

    #[test]
    fn is_user_muted_ignores_ng_word() {
        // NG ワードのみの設定では、ユーザ判定は該当しない（通知向けの挙動）
        let cfg = MuteConfig {
            ng_words: vec!["bad".into()],
            ng_users: vec!["bob@ex.com".into()],
            ..Default::default()
        };
        let n = note("bad word", "bob", Some("ex.com"));
        assert!(is_user_muted(&n.user, &cfg)); // ユーザ一致
        let n2 = note("bad word", "alice", Some("ex.com"));
        assert!(!is_user_muted(&n2.user, &cfg)); // ワード一致でもユーザ判定はしない
    }
}
