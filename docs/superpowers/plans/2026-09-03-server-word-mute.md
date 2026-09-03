# サーバ側ワードミュート反映 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Misskeyサーバー側の `mutedWords`(ソフトワードミュート)設定を取得・同期し、tsumugiのノート表示フィルタ(初期ロード/ギャップ埋め/キャッシュ検索/ストリーミング受信)に適用する(Issue #11)。

**Architecture:** `/i` から `mutedWords` を取得してパースし(AND語グループ + `/regex/flags`)、`AppState` にアカウント単位で保持する。既存のローカルNGワード(`filter::mute::is_muted`)・サーバ側ユーザー/ブロックミュート(`server_muted_note`/`is_server_muted_note`)と全く同じ5箇所の呼び出しサイトに、新しいチェックを並べて追加する。

**Tech Stack:** Rust(`src-tauri/`, Tauri v2)、`regex` crate(既存依存、`filter/eval.rs` で利用実績あり)、`specta`/`tauri-specta`(コマンド戻り値の型をTSへ自動生成)。

## Global Constraints

- 対象は `mutedWords` のみ。`hardMutedWords`/`mutedInstances` は対象外(設計docの「非対象・既知の限界」参照)。
- ワードミュートは通知には適用しない(ローカルNGワードと同じ方針)。
- 正規表現ルールのコンパイル失敗は該当ルールのみスキップし `log::warn!` で警告する。他のルールは有効なまま。
- 既存の `server_mutes`/`is_server_muted` パターン(account_id をキーにした `Mutex<HashMap<...>>`、値はロック内で都度取得)を踏襲する。
- 設計doc: `docs/superpowers/specs/2026-09-03-server-word-mute-design.md`

---

## Task 1: `WordMuteRule` 型とマッチングロジック(`filter/mute.rs`)

**Files:**
- Modify: `src-tauri/src/filter/mute.rs`

**Interfaces:**
- Consumes: `crate::domain::Note`(既存)
- Produces:
  - `pub enum WordMuteRule { Words(Vec<String>), Regex(regex::Regex) }`(`Debug, Clone` 導出)
  - `pub fn is_word_muted(text: Option<&str>, cw: Option<&str>, rules: &[WordMuteRule]) -> bool`
  - `pub fn is_word_note_muted(note: &Note, rules: &[WordMuteRule]) -> bool`(本体 or renote先のいずれかで true)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/filter/mute.rs` の `mod tests` 内、既存の `is_user_muted_ignores_ng_word` テストの直後に追加:

```rust
    #[test]
    fn word_mute_and_group_requires_all_words() {
        let rules = vec![WordMuteRule::Words(vec!["foo".into(), "bar".into()])];
        assert!(is_word_muted(Some("foo and bar here"), None, &rules));
        assert!(!is_word_muted(Some("only foo here"), None, &rules));
    }

    #[test]
    fn word_mute_groups_are_ored() {
        let rules = vec![
            WordMuteRule::Words(vec!["neverused".into()]),
            WordMuteRule::Words(vec!["spoiler".into()]),
        ];
        assert!(is_word_muted(Some("big spoiler"), None, &rules));
    }

    #[test]
    fn word_mute_matches_case_insensitively() {
        let rules = vec![WordMuteRule::Words(vec!["Spoiler".into()])];
        assert!(is_word_muted(Some("BIG SPOILER"), None, &rules));
    }

    #[test]
    fn word_mute_checks_cw_too() {
        let rules = vec![WordMuteRule::Words(vec!["ct".into()])];
        assert!(is_word_muted(None, Some("ct warning"), &rules));
    }

    #[test]
    fn word_mute_regex_matches_with_case_insensitive_flag() {
        let re = regex::RegexBuilder::new("sp.iler").case_insensitive(true).build().unwrap();
        let rules = vec![WordMuteRule::Regex(re)];
        assert!(is_word_muted(Some("a SPXiler word"), None, &rules));
    }

    #[test]
    fn word_mute_empty_rules_never_matches() {
        assert!(!is_word_muted(Some("anything"), None, &[]));
    }

    #[test]
    fn word_note_mute_checks_renote_target() {
        let rules = vec![WordMuteRule::Words(vec!["bad".into()])];
        let mut rn = note("clean", "a", None);
        rn.renote = Some(Box::new(note("bad content", "b", None)));
        assert!(is_word_note_muted(&rn, &rules));
    }

    #[test]
    fn word_note_mute_false_when_neither_matches() {
        let rules = vec![WordMuteRule::Words(vec!["bad".into()])];
        assert!(!is_word_note_muted(&note("clean text", "a", None), &rules));
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib filter::mute::tests::word_mute -- --nocapture`
Expected: コンパイルエラー(`WordMuteRule`/`is_word_muted`/`is_word_note_muted` が未定義)

- [ ] **Step 3: 実装する**

`src-tauri/src/filter/mute.rs` の先頭 `use` 文を以下に変更:

```rust
use crate::domain::{MuteConfig, Note, User};
```
↓
```rust
use crate::domain::{MuteConfig, Note, User};
use regex::RegexBuilder;
```

`normalize_acct` 関数の直後(ファイル末尾のテストモジュールの直前)に追加:

```rust
/// サーバ側ワードミュート(`mutedWords`)の1ルール。`api::mutes::parse_muted_words` が
/// `/i` の生JSONから構築する(Issue #11)。
#[derive(Debug, Clone)]
pub enum WordMuteRule {
    /// 複数語のAND(1語のみのケースも含む)。大小無視の部分一致。
    Words(Vec<String>),
    /// `/pattern/flags` 形式の正規表現ルール。
    Regex(regex::Regex),
}

/// text/cw が word-mute ルール群のいずれかに該当するか(OR)。
pub fn is_word_muted(text: Option<&str>, cw: Option<&str>, rules: &[WordMuteRule]) -> bool {
    if rules.is_empty() {
        return false;
    }
    let text = text.unwrap_or("");
    let cw = cw.unwrap_or("");
    let hay_lower = format!("{text} {cw}").to_lowercase();
    let hay_raw = format!("{text} {cw}");
    rules.iter().any(|rule| match rule {
        WordMuteRule::Words(words) => {
            !words.is_empty() && words.iter().all(|w| hay_lower.contains(&w.to_lowercase()))
        }
        WordMuteRule::Regex(re) => re.is_match(&hay_raw),
    })
}

/// note が word-mute ルール群に該当するか(本体 or renote 先のいずれかで true)。
pub fn is_word_note_muted(note: &Note, rules: &[WordMuteRule]) -> bool {
    if is_word_muted(note.text.as_deref(), note.cw.as_deref(), rules) {
        return true;
    }
    matches!(
        &note.renote,
        Some(r) if is_word_muted(r.text.as_deref(), r.cw.as_deref(), rules)
    )
}
```

`RegexBuilder` の import は Step 3 のテストでは直接使わないが、Task 2 のパース処理から `filter::mute` を経由せず `regex::RegexBuilder` を直接使うため、ここでは削除して構わない。**`use regex::RegexBuilder;` の行は追加しない**(未使用importの警告になる)。上の実装ブロックでは `regex::Regex`/`regex::RegexBuilder` はフルパスで書いているため、追加の `use` は不要。

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib filter::mute::tests -- --nocapture`
Expected: PASS(既存テスト含め全件)

- [ ] **Step 5: コミット**

```bash
cd src-tauri && cargo fmt
git add src/filter/mute.rs
git commit -m "feat: サーバ側ワードミュートのルール型とマッチング処理を追加(Issue #11)"
```

---

## Task 2: `/i` から `mutedWords` を取得・パース(`api/mutes.rs`)

**Files:**
- Modify: `src-tauri/src/api/mutes.rs`

**Interfaces:**
- Consumes: `crate::filter::mute::WordMuteRule`(Task 1)、`crate::api::MisskeyClient::post`
- Produces:
  - `pub(crate) fn parse_muted_words(raw: &serde_json::Value) -> Vec<WordMuteRule>`(純粋関数、単体テスト対象)
  - `pub async fn fetch_muted_words(client: &MisskeyClient) -> Result<Vec<WordMuteRule>>`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/mutes.rs` の末尾に追加(ファイルにまだ `mod tests` は無いので新規作成):

```rust
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
        assert!(matches!(&rules[0], WordMuteRule::Words(w) if w == &vec!["spoiler".to_string()]));
    }

    #[test]
    fn parses_array_element_as_and_group() {
        let raw = json!({ "mutedWords": [["foo", "bar"]] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0], WordMuteRule::Words(w) if w == &vec!["foo".to_string(), "bar".to_string()]));
    }

    #[test]
    fn drops_empty_words_within_a_group_and_drops_groups_left_empty() {
        let raw = json!({ "mutedWords": [["", "  ", "bar"], ["", ""]] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0], WordMuteRule::Words(w) if w == &vec!["bar".to_string()]));
    }

    #[test]
    fn parses_regex_syntax_with_case_insensitive_flag() {
        let raw = json!({ "mutedWords": ["/sp.iler/i"] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        let WordMuteRule::Regex(re) = &rules[0] else { panic!("expected Regex rule") };
        assert!(re.is_match("a SPXiler word"));
    }

    #[test]
    fn invalid_regex_is_skipped_but_other_rules_survive() {
        let raw = json!({ "mutedWords": ["/(unclosed/i", "spoiler"] });
        let rules = parse_muted_words(&raw);
        assert_eq!(rules.len(), 1);
        assert!(matches!(&rules[0], WordMuteRule::Words(w) if w == &vec!["spoiler".to_string()]));
    }

    #[test]
    fn missing_muted_words_field_returns_empty() {
        let raw = json!({});
        assert!(parse_muted_words(&raw).is_empty());
    }
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib api::mutes::tests -- --nocapture`
Expected: コンパイルエラー(`parse_muted_words` が未定義)

- [ ] **Step 3: 実装する**

`src-tauri/src/api/mutes.rs` の冒頭コメント・use文を以下に変更:

```rust
//! サーバ側ミュート/ブロックの取得（mute/list・blocking/list）。
//! Krile の MuteBlockManager 相当。返るのは対象ユーザの userId 集合。

use crate::api::MisskeyClient;
use crate::error::Result;
use serde_json::json;
use std::collections::HashSet;
```

↓

```rust
//! サーバ側ミュート/ブロック・ワードミュートの取得。
//! - `mute/list`/`blocking/list`: 対象ユーザの userId 集合(Krile MuteBlockManager 相当)。
//! - `/i` の `mutedWords`: ソフトワードミュートのルール一覧(Issue #11)。

use crate::api::MisskeyClient;
use crate::error::Result;
use crate::filter::mute::WordMuteRule;
use serde_json::json;
use std::collections::HashSet;
```

ファイル末尾(`collect` 関数の直後、`mod tests` の直前)に追加:

```rust
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
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib api::mutes::tests -- --nocapture`
Expected: PASS(全6件)

- [ ] **Step 5: コミット**

```bash
cd src-tauri && cargo fmt
git add src/api/mutes.rs
git commit -m "feat: /iからmutedWordsを取得・パースする処理を追加(Issue #11)"
```

---

## Task 3: `AppState` にサーバ側ワードミュートを保持する

**Files:**
- Modify: `src-tauri/src/state.rs`

**Interfaces:**
- Consumes: `crate::filter::mute::WordMuteRule`(Task 1)、`crate::domain::Note`
- Produces:
  - フィールド `pub server_word_mutes: Mutex<HashMap<String, Vec<crate::filter::mute::WordMuteRule>>>`
  - `pub fn AppState::is_word_muted(&self, account_id: &str, note: &Note) -> bool`
  - `pub fn AppState::set_server_word_mutes(&self, account_id: &str, rules: Vec<crate::filter::mute::WordMuteRule>)`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/state.rs` の末尾 `mod tests` 内、`restores_persisted_accounts_on_construction` の直後に追加:

```rust
    #[test]
    fn is_word_muted_false_before_sync_and_true_after() {
        use crate::domain::{User, Visibility};
        use crate::filter::mute::WordMuteRule;

        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let note = crate::domain::Note {
            id: "n1".into(),
            created_at: 0,
            text: Some("spoiler here".into()),
            cw: None,
            visibility: Visibility::Public,
            local_only: false,
            user: User {
                id: "u1".into(),
                username: "alice".into(),
                host: None,
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: std::collections::HashMap::new(),
                bio: None,
                banner_url: None,
                instance: None,
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
            reactions: std::collections::HashMap::new(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        };

        assert!(!state.is_word_muted("acc1", &note)); // 未同期なら常に false
        state.set_server_word_mutes("acc1", vec![WordMuteRule::Words(vec!["spoiler".into()])]);
        assert!(state.is_word_muted("acc1", &note));
        assert!(!state.is_word_muted("other-acc", &note)); // 別アカウントには影響しない
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib state::tests::is_word_muted -- --nocapture`
Expected: コンパイルエラー(`server_word_mutes`/`is_word_muted`/`set_server_word_mutes` が未定義)

- [ ] **Step 3: 実装する**

`src-tauri/src/state.rs` の冒頭 import を変更:

```rust
use crate::domain::{EmojiDef, MuteConfig};
```
↓
```rust
use crate::domain::{EmojiDef, MuteConfig, Note};
use crate::filter::mute::WordMuteRule;
```

`AppState` 構造体定義内、`server_mutes` フィールドの直後に追加:

```rust
    /// account_id -> サーバ側ワードミュート(mutedWords)のルール一覧。
    /// server_mutes と同じタイミングで同期し、ノート本文/CWの追加フィルタに使う(Issue #11)。
    pub server_word_mutes: Mutex<HashMap<String, Vec<WordMuteRule>>>,
```

`new_with_sound` 内、`server_mutes: Mutex::new(HashMap::new()),` の直後に追加:

```rust
            server_word_mutes: Mutex::new(HashMap::new()),
```

`set_server_mutes` メソッドの直後に追加:

```rust
    /// account の note が サーバ側ワードミュート(mutedWords)に該当するか。
    pub fn is_word_muted(&self, account_id: &str, note: &Note) -> bool {
        self.server_word_mutes
            .lock()
            .unwrap()
            .get(account_id)
            .is_some_and(|rules| crate::filter::mute::is_word_note_muted(note, rules))
    }

    /// account のサーバ側ワードミュートルールを差し替える。
    pub fn set_server_word_mutes(&self, account_id: &str, rules: Vec<WordMuteRule>) {
        self.server_word_mutes
            .lock()
            .unwrap()
            .insert(account_id.to_string(), rules);
    }
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib state::tests -- --nocapture`
Expected: PASS(既存含め全件)

- [ ] **Step 5: コミット**

```bash
cd src-tauri && cargo fmt
git add src/state.rs
git commit -m "feat: AppStateにサーバ側ワードミュートの保持・判定を追加(Issue #11)"
```

---

## Task 4: `sync_server_mutes` を拡張する

**Files:**
- Modify: `src-tauri/src/commands/mute.rs`

**Interfaces:**
- Consumes: `crate::api::mutes::fetch_muted_words`(Task 2)、`AppState::set_server_word_mutes`(Task 3)
- Produces: `pub struct SyncMuteResult { pub blocked_users: u32, pub word_rules: u32 }`(`specta::Type` 付与、TSへ自動生成)。`sync_server_mutes` の戻り値が `Result<u32>` → `Result<SyncMuteResult>` に変わる(破壊的変更、Task 6 でフロントを追随させる)。

- [ ] **Step 1: 実装する**(戻り値の型変更を伴うため、先に本体を書き換えてから既存の呼び出し元コンパイルエラーを解消する形で進める。テストは Step 3 相当を兼ねる)

`src-tauri/src/commands/mute.rs` の冒頭 use 文を変更:

```rust
use crate::api::mutes::fetch_muted_and_blocked;
use crate::domain::{MuteConfig, NotifyConfig, UiPrefs};
use crate::error::{Error, Result};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::{AppHandle, State};
```
↓
```rust
use crate::api::mutes::{fetch_muted_and_blocked, fetch_muted_words};
use crate::domain::{MuteConfig, NotifyConfig, UiPrefs};
use crate::error::{Error, Result};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
```

ファイル末尾の `sync_server_mutes` を丸ごと置き換え:

```rust
/// サーバ側のミュート/ブロックを取得して AppState に反映する。返り値は対象ユーザ数。
/// 起動時とアカウント追加時にフロントから呼ぶ（Krile MuteBlockManager 相当）。
#[tauri::command]
#[specta::specta]
pub async fn sync_server_mutes(state: State<'_, AppState>, account_id: String) -> Result<u32> {
    let client = state.client_for(&account_id)?;
    let ids = fetch_muted_and_blocked(&client).await?;
    let n = ids.len() as u32;
    state.set_server_mutes(&account_id, ids);
    Ok(n)
}
```
↓
```rust
/// `sync_server_mutes` の戻り値。ユーザ/ブロックミュート数とワードミュートのルール数を
/// 別々に返す(フロントのログ表示用。Issue #11)。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncMuteResult {
    pub blocked_users: u32,
    pub word_rules: u32,
}

/// サーバ側のミュート/ブロック・ワードミュート(mutedWords)を取得して AppState に反映する。
/// 起動時とアカウント追加時にフロントから呼ぶ（Krile MuteBlockManager 相当。Issue #11）。
#[tauri::command]
#[specta::specta]
pub async fn sync_server_mutes(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<SyncMuteResult> {
    let client = state.client_for(&account_id)?;
    let ids = fetch_muted_and_blocked(&client).await?;
    let word_rules = fetch_muted_words(&client).await?;
    let result = SyncMuteResult {
        blocked_users: ids.len() as u32,
        word_rules: word_rules.len() as u32,
    };
    state.set_server_mutes(&account_id, ids);
    state.set_server_word_mutes(&account_id, word_rules);
    Ok(result)
}
```

- [ ] **Step 2: ビルドが通ることを確認**(この時点でフロント側 `store.svelte.ts` の呼び出しが型不一致になるが、Rustのビルド自体は独立して確認できる)

Run: `cd src-tauri && cargo build`
Expected: 成功(warningなし)

- [ ] **Step 3: コミット**

```bash
cd src-tauri && cargo fmt
git add src/commands/mute.rs
git commit -m "feat: sync_server_mutesでmutedWordsも同期するよう拡張(Issue #11)"
```

---

## Task 5: `commands/column.rs` のノートフィルタに組み込む

**Files:**
- Modify: `src-tauri/src/commands/column.rs`

**Interfaces:**
- Consumes: `AppState::is_word_muted(&self, account_id: &str, note: &Note) -> bool`(Task 3)
- Produces: `search_cache_core` のシグネチャに `is_word_muted: impl Fn(&Note) -> bool` パラメータが追加される(既存呼び出し元・テストは全て更新が必要)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/column.rs` の `mod tests` 内、`search_cache_core_excludes_notes_the_closure_marks_server_muted` の直後に追加:

```rust
    #[test]
    fn search_cache_core_excludes_notes_matched_by_word_mute_closure() {
        let cache = cache_with(&[note("n1", 100), note("n2", 200)]);

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |_| false,
            |n| n.id == "n2",
        )
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"]);
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd src-tauri && cargo test --lib commands::column::tests::search_cache_core_excludes_notes_matched_by_word_mute_closure -- --nocapture`
Expected: コンパイルエラー(`search_cache_core` の引数が7個で呼び出しが8個になっている/既存呼び出し元も引数不足でコンパイルエラーになる)

- [ ] **Step 3: 実装する**

`search_cache_core` のシグネチャとフィルタ処理を変更:

```rust
fn search_cache_core(
    cache: &NoteCacheStore,
    filter: &FilterQuery,
    eval_ctx: &EvalContext,
    mute: &MuteConfig,
    until_id: Option<&str>,
    limit: u32,
    is_server_muted: impl Fn(&Note) -> bool,
) -> Result<Vec<Note>> {
    let compiled = CompiledFilter::compile(filter).map_err(Error::Invalid)?;
    let sql_ctx = sql::SqlCtx {
        my_ids: eval_ctx.my_user_ids.iter().cloned().collect(),
        following_ids: None,
    };
    let where_sql = match &compiled {
        CompiledFilter::Tql(expr) => sql::build_where(expr, &sql_ctx).map_err(Error::Invalid)?,
        _ => sql::SqlWhere { sql: "1=1".into(), params: vec![] },
    };
    let raw = cache.search_cache(&where_sql, until_id, limit)?;
    let mut filtered: Vec<Note> = raw
        .into_iter()
        .filter(|n| {
            compiled.matches(n, eval_ctx)
                && !crate::filter::mute::is_muted(n, mute)
                && !is_server_muted(n)
        })
        .collect();
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    filtered.truncate(limit as usize);
    Ok(filtered)
}
```
↓
```rust
fn search_cache_core(
    cache: &NoteCacheStore,
    filter: &FilterQuery,
    eval_ctx: &EvalContext,
    mute: &MuteConfig,
    until_id: Option<&str>,
    limit: u32,
    is_server_muted: impl Fn(&Note) -> bool,
    is_word_muted: impl Fn(&Note) -> bool,
) -> Result<Vec<Note>> {
    let compiled = CompiledFilter::compile(filter).map_err(Error::Invalid)?;
    let sql_ctx = sql::SqlCtx {
        my_ids: eval_ctx.my_user_ids.iter().cloned().collect(),
        following_ids: None,
    };
    let where_sql = match &compiled {
        CompiledFilter::Tql(expr) => sql::build_where(expr, &sql_ctx).map_err(Error::Invalid)?,
        _ => sql::SqlWhere { sql: "1=1".into(), params: vec![] },
    };
    let raw = cache.search_cache(&where_sql, until_id, limit)?;
    let mut filtered: Vec<Note> = raw
        .into_iter()
        .filter(|n| {
            compiled.matches(n, eval_ctx)
                && !crate::filter::mute::is_muted(n, mute)
                && !is_server_muted(n)
                && !is_word_muted(n)
        })
        .collect();
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    filtered.truncate(limit as usize);
    Ok(filtered)
}
```

`search_cache_notes` コマンドの呼び出しを変更:

```rust
    let mute = state.mute.lock().unwrap().clone();
    let eval_ctx = state.eval_context();
    search_cache_core(
        &state.cache,
        &filter,
        &eval_ctx,
        &mute,
        until_id.as_deref(),
        limit,
        |n| server_muted_note(&state, &account_id, n),
    )
```
↓
```rust
    let mute = state.mute.lock().unwrap().clone();
    let eval_ctx = state.eval_context();
    search_cache_core(
        &state.cache,
        &filter,
        &eval_ctx,
        &mute,
        until_id.as_deref(),
        limit,
        |n| server_muted_note(&state, &account_id, n),
        |n| state.is_word_muted(&account_id, n),
    )
```

`resume_column`(キャッシュ再検証、初期ロード)の該当箇所を変更:

```rust
        let ctx = state.eval_context();
        let mute = state.mute.lock().unwrap().clone();
        cached.retain(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(&state, &column.account_id, n)
        });
```
↓
```rust
        let ctx = state.eval_context();
        let mute = state.mute.lock().unwrap().clone();
        cached.retain(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(&state, &column.account_id, n)
                && !state.is_word_muted(&column.account_id, n)
        });
```

`fill_gap` の該当箇所を変更:

```rust
                if resolved.filter.matches(&n, &ctx)
                    && !crate::filter::mute::is_muted(&n, &mute)
                    && !server_muted_note(state, account_id, &n)
                {
                    collected.push(n);
                }
```
↓
```rust
                if resolved.filter.matches(&n, &ctx)
                    && !crate::filter::mute::is_muted(&n, &mute)
                    && !server_muted_note(state, account_id, &n)
                    && !state.is_word_muted(account_id, &n)
                {
                    collected.push(n);
                }
```

`fetch_and_filter_multi` の該当箇所を変更:

```rust
    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let mut filtered: Vec<Note> = all
        .into_iter()
        .filter(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
        })
        .collect();
```
↓
```rust
    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let mut filtered: Vec<Note> = all
        .into_iter()
        .filter(|n| {
            resolved.filter.matches(n, &ctx)
                && !crate::filter::mute::is_muted(n, &mute)
                && !server_muted_note(state, account_id, n)
                && !state.is_word_muted(account_id, n)
        })
        .collect();
```

既存の5件の `search_cache_core` テスト呼び出し(`search_cache_core_filters_by_tql_predicate_and_orders_desc` / `search_cache_core_with_empty_predicate_returns_all_desc_order` / `search_cache_core_excludes_locally_muted_notes` / `search_cache_core_excludes_notes_the_closure_marks_server_muted` / `search_cache_core_respects_until_id_boundary`)は、それぞれ末尾の `|_| false`(または `|n| n.id == "n2"`)の直後に `, |_| false` を追加して8引数にする。例(`search_cache_core_excludes_locally_muted_notes`):

```rust
        let got = search_cache_core(&cache, &filter, &EvalContext::default(), &mute, None, 10, |_| false)
            .unwrap();
```
↓
```rust
        let got =
            search_cache_core(&cache, &filter, &EvalContext::default(), &mute, None, 10, |_| false, |_| false)
                .unwrap();
```

他4件も同様に、既存の最終引数(closure)の後ろへ `, |_| false` を追加する。

- [ ] **Step 4: テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib commands::column::tests -- --nocapture`
Expected: PASS(既存含め全件)

- [ ] **Step 5: コミット**

```bash
cd src-tauri && cargo fmt
git add src/commands/column.rs
git commit -m "feat: ノート取得・検索経路にサーバ側ワードミュートを適用(Issue #11)"
```

---

## Task 6: `stream/connection.rs` のストリーミング受信に組み込む

**Files:**
- Modify: `src-tauri/src/stream/connection.rs`

**Interfaces:**
- Consumes: `AppState::is_word_muted`(Task 3)

- [ ] **Step 1: 実装する**(このパスは既存の `is_server_muted_note` 呼び出し自体も単体テストが無い統合コード。同じパターンで直接組み込む)

```rust
            if let Some(state) = app.try_state::<AppState>() {
                if crate::filter::mute::is_muted(&normalized, &state.mute.lock().unwrap()) {
                    return HandleResult::None;
                }
                if is_server_muted_note(&state, account_id, &normalized) {
                    return HandleResult::None;
                }
                let _ = state.cache.cache_note(&column_id, &normalized);
            }
```
↓
```rust
            if let Some(state) = app.try_state::<AppState>() {
                if crate::filter::mute::is_muted(&normalized, &state.mute.lock().unwrap()) {
                    return HandleResult::None;
                }
                if is_server_muted_note(&state, account_id, &normalized) {
                    return HandleResult::None;
                }
                if state.is_word_muted(account_id, &normalized) {
                    return HandleResult::None;
                }
                let _ = state.cache.cache_note(&column_id, &normalized);
            }
```

- [ ] **Step 2: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: 成功

- [ ] **Step 3: 既存テストが通ることを確認**

Run: `cd src-tauri && cargo test --lib stream::connection::tests -- --nocapture`
Expected: PASS(既存全件、変更なし)

- [ ] **Step 4: コミット**

```bash
cd src-tauri && cargo fmt
git add src/stream/connection.rs
git commit -m "feat: ストリーミング受信にサーバ側ワードミュートを適用(Issue #11)"
```

---

## Task 7: TSバインディング再生成 + フロントのログ表示を更新

**Files:**
- Modify: `frontend/src/bindings/tauri.gen.ts`(生成物、手編集しない)
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.syncServerMutes(accountId: string) -> Promise<SyncMuteResult>`(生成後の型。`SyncMuteResult = { blockedUsers: number; wordRules: number }`)

- [ ] **Step 1: TSバインディングを再生成する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` の `syncServerMutes` の戻り値型が `number` から `SyncMuteResult`(新規生成される型)に変わっていることを確認する:

```bash
grep -n "syncServerMutes\|SyncMuteResult" frontend/src/bindings/tauri.gen.ts
```

- [ ] **Step 2: `store.svelte.ts` のログ表示を更新する**

`frontend/src/lib/store.svelte.ts` の `#syncServerMutes` を変更:

```typescript
  /// サーバ側ミュート/ブロックを同期（失敗しても致命的でないのでログのみ）。
  async #syncServerMutes(accountId: string) {
    try {
      const n = await unwrapAcc(accountId, commands.syncServerMutes(accountId));
      if (n > 0) this.#log("info", `サーバのミュート/ブロックを同期: ${n}件`);
    } catch (e) {
      if (e instanceof ForbiddenError) {
        this.#log("warn", "サーバミュート同期: 権限不足。再認証してください", e.accountId);
      } else {
        this.#log("warn", `サーバミュート同期に失敗: ${String(e)}`);
      }
    }
  }
```
↓
```typescript
  /// サーバ側ミュート/ブロック・ワードミュート(mutedWords)を同期（失敗しても致命的でないのでログのみ）。
  async #syncServerMutes(accountId: string) {
    try {
      const result = await unwrapAcc(accountId, commands.syncServerMutes(accountId));
      if (result.blockedUsers > 0 || result.wordRules > 0) {
        this.#log(
          "info",
          `サーバのミュート/ブロックを同期: ユーザ${result.blockedUsers}件・ワード${result.wordRules}件`,
        );
      }
    } catch (e) {
      if (e instanceof ForbiddenError) {
        this.#log("warn", "サーバミュート同期: 権限不足。再認証してください", e.accountId);
      } else {
        this.#log("warn", `サーバミュート同期に失敗: ${String(e)}`);
      }
    }
  }
```

- [ ] **Step 3: フロントの型チェックを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 4: コミット**

```bash
git add frontend/src/bindings/tauri.gen.ts frontend/src/lib/store.svelte.ts
git commit -m "feat: サーバ側ワードミュート同期件数をログに反映(Issue #11)"
```

---

## Task 8: 全体テストと最終確認

**Files:** なし(検証のみ)

- [ ] **Step 1: Rust全体テスト**

Run: `cd src-tauri && cargo test`
Expected: PASS(`#[ignore]` の実サーバ疎通テストを除く全件)

- [ ] **Step 2: フロント単体テスト**

Run: `cd frontend && pnpm test`
Expected: PASS

- [ ] **Step 3: フロント型チェック**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 4: `cargo clippy` で警告が無いことを確認**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: エラーなし

この時点でIssue #11の対応は完了。PRを作成する際は `Fixes #11` をPR本文に含める(CLAUDE.md「Development workflow」参照)。
