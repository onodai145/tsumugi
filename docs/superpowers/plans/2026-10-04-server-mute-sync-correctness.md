# サーバー側ミュート同期の正しさ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** サーバー側ミュート同期で、正規表現の `i` フラグだけの解除の見逃し、不正な正規表現が消えたように見える誤検出、同じアカウントの同期の競合をなくす。

**Architecture:** スナップショットのキーを、パース後の `WordMuteRule` ではなく、`/i` の生の `mutedWords` 要素から1パスで作る(`MutedWordEntry { rule, key }`)。`WordMuteRule` の型は変えない。`AppState` にアカウント単位の `tokio::sync::Mutex` を持たせ、`sync_server_mutes_core` の冒頭(取得より前)から保存値の書き込みの終わりまで持つ。

**Tech Stack:** Rust(Tauri v2)、`regex`、`tokio::sync::Mutex`、テストは `wiremock` と `tokio::test`。

**Spec:** `docs/superpowers/specs/2026-10-04-server-mute-sync-correctness-design.md`(実行者は spec も読むこと)

## Global Constraints

- ブランチは `fix/p2-server-mute-sync-456`(作成済み)。`main` に直接コミットしない。
- コミットメッセージは**件名のみ**(本文なし)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別段落として付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら、リトライせず報告する。
- `w:` のキーは、現在の `WordMuteRule::key()` と**同じ文字列**(`w:` + 語を小文字にしてソートし `\u{1f}` で連結)。`r:` のキーだけが `r:{要素の生の文字列}` に変わる。
- DB スキーマ、Tauri コマンドの署名、TS バインディング、フロントエンド、設定 JSON の形は変えない。`specta_builder()` は触らない。
- ロックの順序は「同期ロック → 境界の書きロック(`invalidate_boundaries`)」のみ。逆順の経路を作らない。
- Rust のテストは `cd src-tauri && cargo test --lib <フィルタ>`。コンパイルが数分かかることがある。`pkill` / `killall` は使わない。ユーザーの実アプリ(`target/debug/tsumugi` など)のプロセスには触らない。
- UI 変更は無い。`cargo tauri dev` は起動しない。

## Review Focus

- `mutedWords` が配列でない(文字列・オブジェクト)、要素が文字列でも配列でもない(数値・`null`・オブジェクト): 落ちず、その要素は無視する(Task 1 のテスト)。
- 語が全部空白の配列、空文字・空白だけの文字列: 要素にしない。キーも作らない(Task 1 のテスト)。
- `/` 単体、`//`(空パターン): 正規表現ではなく、通常の単語として `w:` のキーになる(Task 1 のテスト)。
- 同じ要素がサーバーに重複している: キーは重複しうるが、`ServerMuteSnapshot::new` が重複を除く。保存値と比較しても、余計な解除を検出しない(Task 1 の結合テスト)。
- 別アカウントの同期が、他アカウントのロックで待たされない(Task 2 のテスト)。

---

## File Structure

| ファイル | 責任 |
|---|---|
| `src-tauri/src/api/mutes.rs` | `mutedWords` の生 JSON → `MutedWordEntry { rule, key }` のパースと、キー(`w:` / `r:`)の生成。`fetch_muted_words` が `MutedWords { rules, keys }` を返す |
| `src-tauri/src/filter/mute.rs` | `WordMuteRule::key()` と、そのテストを削除(キー生成は `api/mutes.rs` に移る) |
| `src-tauri/src/commands/mute.rs` | `sync_server_mutes_core` が、キーでスナップショットを作り、同期ロックを持つ |
| `src-tauri/src/state.rs` | アカウント単位の同期ロック(`server_mute_sync_lock` / `forget_server_mute_sync_lock`) |
| `src-tauri/src/commands/account.rs` | `remove_account` が同期ロックのエントリを捨てる |
| `src-tauri/src/store/settings.rs` | `ServerMuteSnapshot.words` のコメントのみ更新 |
| `docs/superpowers/specs/2026-10-03-server-mute-boundary-design.md` | 「既知の制限」に、本件で解消した旨の追記 |

## Task 1: 生の要素からキーを作る

**Files:**
- Modify: `src-tauri/src/api/mutes.rs`(パース関数、`fetch_muted_words`、テスト)
- Modify: `src-tauri/src/filter/mute.rs`(`impl WordMuteRule { fn key }` と、そのテスト4件を削除)
- Modify: `src-tauri/src/commands/mute.rs`(`sync_server_mutes_core` の呼び出し側、テスト2件)
- Modify: `src-tauri/src/store/settings.rs`(コメント1行)

**Interfaces:**
- Consumes: 既存の `WordMuteRule`(型は変えない)、`ServerMuteSnapshot::new(users, words)`、`try_parse_regex_syntax` / `is_valid_regex_flags`。
- Produces(Task 2 は、これを前提にしない。`sync_server_mutes_core` が使うだけ):
  - `pub(crate) struct MutedWordEntry { pub rule: Option<WordMuteRule>, pub key: String }`
  - `pub struct MutedWords { pub rules: Vec<WordMuteRule>, pub keys: Vec<String> }`
  - `pub(crate) fn parse_muted_word_entries(raw: &serde_json::Value) -> Vec<MutedWordEntry>`
  - `pub(crate) fn parse_muted_words(raw: &serde_json::Value) -> Vec<WordMuteRule>`(署名そのまま。中身はエントリのラッパ)
  - `pub async fn fetch_muted_words(client: &MisskeyClient) -> Result<MutedWords>`(戻り値の型が `Vec<WordMuteRule>` から変わる)

- [ ] **Step 1: 結合テスト2件を書く(`commands/mute.rs` の `mod tests` の末尾、`sync_saves_the_snapshot_even_when_the_account_has_no_columns` の後ろ)**

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn sync_clears_the_accounts_boundaries_when_only_the_i_flag_of_a_regex_was_removed() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_server_mutes(&mock, &[], serde_json::json!(["/spoiler/i"])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_server_mutes(&mock, &[], serde_json::json!(["/spoiler/"])).await; // i を外した(一致が減る=解除)
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert!(boundaries_of(&state, "c1").await.is_empty());
        assert!(boundaries_of(&state, "c2").await.is_empty());
        assert_eq!(boundaries_of(&state, "c3").await, kept(), "他のアカウントのカラムは、そのまま");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_keeps_boundaries_when_an_invalid_regex_stays_on_the_server() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        // 以前のビルドでは妥当だったルールが、いまは不正で落ちる状況。保存値には、そのキーが残っている
        state.settings.save_server_mute_snapshot("acc1", &snap(&[], &["r:/(unclosed/i"])).unwrap();
        mount_server_mutes(&mock, &[], serde_json::json!(["/(unclosed/i", "/(unclosed/i"])).await; // 重複もある

        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        for column_id in ["c1", "c2", "c3"] {
            assert_eq!(boundaries_of(&state, column_id).await, kept(), "不正でも、サーバーに残っていれば解除ではない");
        }
        assert_eq!(
            state.settings.load_server_mute_snapshot("acc1").unwrap(),
            Some(snap(&[], &["r:/(unclosed/i"])),
            "保存値は変わらない(重複も除かれる)"
        );
    }
```

- [ ] **Step 2: 結合テストが、想定どおりの理由で失敗することを確認する**

Run: `cd src-tauri && cargo test --lib commands::mute::tests::sync_ 2>&1 | tail -30`

Expected: 2件とも FAIL。
- `i` フラグ: `assertion failed: boundaries_of(...).is_empty()`(`r:spoiler` が両方で同じキーなので、解除を検出できない)
- 不正な正規表現: `left: [] right: [(0, "n500")]` 相当(保存値の `r:/(unclosed/i` が、今回の集合に無いので、解除と見なして捨てる)

- [ ] **Step 3: キーを作る関数のテストを書く(`api/mutes.rs` の `mod tests` の末尾)**

```rust
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
```

- [ ] **Step 4: テストが、関数が無いために失敗することを確認する**

Run: `cd src-tauri && cargo test --lib api::mutes 2>&1 | tail -15`
Expected: コンパイルエラー `cannot find function \`parse_muted_word_entries\` in this scope`(11件のテストすべてが対象)。

- [ ] **Step 5: `api/mutes.rs` を実装する**

`fetch_muted_words` から `parse_word_element` までを、次の内容に置き換える(`try_parse_regex_syntax` と `is_valid_regex_flags` は、そのまま残す)。

```rust
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
/// `parse_muted_word_entries` のルールだけを返す薄いラッパ。
pub(crate) fn parse_muted_words(raw: &serde_json::Value) -> Vec<WordMuteRule> {
    parse_muted_word_entries(raw).into_iter().filter_map(|e| e.rule).collect()
}

/// `/i` の生JSONから `mutedWords` を取り出し、要素ごとにルールとキーを作る純粋関数。
/// Misskey の `mutedWords: (string | string[])[]` を変換する:
/// - 配列要素([string]) → 複数語のANDグループ(空語は除去、全滅したグループは要素にしない)
/// - `/pattern/flags` 形式の文字列 → 正規表現ルール(`i` フラグのみ反映。コンパイル失敗は
///   `rule` を `None` にして警告ログを出す。キーは作る)
/// - それ以外の文字列 → 単語1個のANDグループ(trim して空なら要素にしない)
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
```

- [ ] **Step 6: `commands/mute.rs` の `sync_server_mutes_core` を、新しい戻り値に合わせる**

置き換え前:

```rust
    let word_rules = fetch_muted_words(client).await?;
    let result = SyncMuteResult {
        blocked_users: ids.len() as u32,
        word_rules: word_rules.len() as u32,
    };
    let snapshot = ServerMuteSnapshot::new(ids.iter().cloned(), word_rules.iter().map(|r| r.key()));
```

置き換え後:

```rust
    let words = fetch_muted_words(client).await?;
    let result = SyncMuteResult {
        blocked_users: ids.len() as u32,
        word_rules: words.rules.len() as u32,
    };
    let snapshot = ServerMuteSnapshot::new(ids.iter().cloned(), words.keys);
```

さらに、同じ関数の下のほうの `state.set_server_word_mutes(account_id, word_rules);` を `state.set_server_word_mutes(account_id, words.rules);` に直す。

- [ ] **Step 7: `WordMuteRule::key()` とそのテストを削除する(`filter/mute.rs`)**

`impl WordMuteRule { pub fn key(&self) -> String { ... } }` のブロック全体(doc コメントを含む)を削除する。`mod tests` の `word_rule_key_ignores_word_order_and_case`、`word_rule_key_distinguishes_different_rules`、`word_rule_key_does_not_confuse_one_joined_word_with_two_words`、`word_rule_key_keeps_regex_and_words_apart` の4件を削除する(同じ内容が `api/mutes.rs` のテストに移っている)。

- [ ] **Step 8: `ServerMuteSnapshot.words` のコメントを直す(`store/settings.rs`)**

`/// ワードミュートのルールの文字列表現(`WordMuteRule::key()`。ソート済み・重複なし)。` を次に置き換える。

```rust
    /// ワードミュートの要素を表すキー(`api::mutes::MutedWordEntry::key`。ソート済み・重複なし)。
```

- [ ] **Step 9: テストを流して、全件が通ることを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -8`
Expected: `test result: ok.` で、失敗 0 件(追加13件を含む。既存の `parse_muted_words` 系のテストも、変更なしで通る)。

- [ ] **Step 10: clippy の警告数が増えていないことを確認する**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: `23`(変更前と同じ。増えていたら、自分の変更が原因の警告を直す)。

- [ ] **Step 11: コミットする**

```bash
git add src-tauri/src/api/mutes.rs src-tauri/src/filter/mute.rs src-tauri/src/commands/mute.rs src-tauri/src/store/settings.rs
git commit -m "fix: サーバー側ワードミュートのキーを生の要素から作り、iフラグだけの解除と不正な正規表現の誤検出を直す" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 12: 変異確認(コミット後。確認したら必ず元に戻す)**

`api/mutes.rs` の `parse_word_element` の `let key = format!("r:{s}");` を `let key = format!("r:{pattern}");` に一時的に書き換え、次を実行する。

Run: `cd src-tauri && cargo test --lib -- regex_key_differs sync_clears_the_accounts_boundaries_when_only_the_i_flag 2>&1 | tail -15`
Expected: `regex_key_differs_when_only_the_i_flag_is_removed` と `sync_clears_the_accounts_boundaries_when_only_the_i_flag_of_a_regex_was_removed` が FAIL する。

確認したら元に戻す: `git checkout -- src-tauri/src/api/mutes.rs` そして `git status --short` が空であることを確認する。

## Task 2: 同期の直列化

**Files:**
- Modify: `src-tauri/src/state.rs`(フィールド、メソッド2つ、`use`、テスト)
- Modify: `src-tauri/src/commands/mute.rs`(`sync_server_mutes_core` の冒頭とコメント、テスト)
- Modify: `src-tauri/src/commands/account.rs`(`remove_account` に1行)

**Interfaces:**
- Consumes: Task 1 の `sync_server_mutes_core`(署名は変わらない)。
- Produces:
  - `AppState::server_mute_sync_lock(&self, account_id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>>`(同じアカウントには同じ `Arc` を返す)
  - `AppState::forget_server_mute_sync_lock(&self, account_id: &str)`

- [ ] **Step 1: `state.rs` のテストを書く(`mod tests` の末尾)**

```rust
    #[test]
    fn server_mute_sync_lock_is_shared_per_account_and_separate_across_accounts() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let a1 = state.server_mute_sync_lock("a1");
        let a1_again = state.server_mute_sync_lock("a1");
        let a2 = state.server_mute_sync_lock("a2");

        assert!(std::sync::Arc::ptr_eq(&a1, &a1_again));
        assert!(!std::sync::Arc::ptr_eq(&a1, &a2));
    }

    #[test]
    fn forget_server_mute_sync_lock_drops_the_entry() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let before = state.server_mute_sync_lock("a1");

        state.forget_server_mute_sync_lock("a1");

        assert!(!std::sync::Arc::ptr_eq(&before, &state.server_mute_sync_lock("a1")));
    }
```

(`SettingsStore` が `state.rs` の `mod tests` で見えない場合は、`crate::store::SettingsStore::new_in_memory()` と書く。)

- [ ] **Step 2: `commands/mute.rs` の結合テストを書く(`mod tests` の末尾、Task 1 で足した2件の後ろ)**

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn sync_waits_for_the_same_accounts_sync_lock_before_fetching() {
        let mock = MockServer::start().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let lock = state.server_mute_sync_lock("acc1");
        let guard = lock.lock().await; // 別の同期が、同じアカウントで実行中

        let blocked = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            sync_server_mutes_core(&state, "acc1", &client),
        )
        .await;

        assert!(blocked.is_err(), "ロックを持たれている間は完了しない");
        assert!(mock.received_requests().await.unwrap().is_empty(), "取得より前にロックを待つ");
        drop(guard);
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();
        assert!(!mock.received_requests().await.unwrap().is_empty(), "ロックを放すと取得が走る");
        assert!(state.is_server_muted("acc1", "u1"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_of_another_account_is_not_blocked_by_a_held_sync_lock() {
        let mock = MockServer::start().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        let lock = state.server_mute_sync_lock("acc1");
        let _guard = lock.lock().await;

        let done = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sync_server_mutes_core(&state, "acc2", &client),
        )
        .await;

        assert!(done.expect("別アカウントの同期は待たされない").is_ok());
        assert!(state.is_server_muted("acc2", "u1"));
    }
```

- [ ] **Step 3: テストが、メソッドが無いために失敗することを確認する**

Run: `cd src-tauri && cargo test --lib server_mute_sync_lock 2>&1 | tail -15`
Expected: コンパイルエラー `no method named \`server_mute_sync_lock\` found for struct \`AppState\``。

- [ ] **Step 4: `state.rs` に、フィールドとメソッドを足す**

`use std::sync::Mutex;` を `use std::sync::{Arc, Mutex};` に変える(`Arc` が既に別の形で取り込まれていれば、重複しないようにする)。

`AppState` の `server_word_mutes` フィールドの直後に足す:

```rust
    /// account_id -> サーバー側ミュート同期の排他ロック。同じアカウントの `sync_server_mutes_core` を、
    /// 取得から保存値の書き込みまで直列にする(Issue #456)。必要になったときに作る。
    /// ロックの順序は「このロック → 境界の書きロック(`ColumnFence::invalidate_boundaries`)」のみ。
    server_mute_sync_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
```

`new_with_sound` の `Self { ... }` の `server_word_mutes: Mutex::new(HashMap::new()),` の直後に足す:

```rust
            server_mute_sync_locks: Mutex::new(HashMap::new()),
```

`set_server_word_mutes` の直後にメソッドを足す:

```rust
    /// account のサーバー側ミュート同期の排他ロック。同じアカウントには同じ `Arc` を返す。
    pub fn server_mute_sync_lock(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(
            self.server_mute_sync_locks
                .lock()
                .unwrap()
                .entry(account_id.to_string())
                .or_default(),
        )
    }

    /// account の同期ロックのエントリを破棄する(アカウント削除時)。実行中の同期は `Arc` を持つので、
    /// そのまま完了する。
    pub fn forget_server_mute_sync_lock(&self, account_id: &str) {
        self.server_mute_sync_locks.lock().unwrap().remove(account_id);
    }
```

- [ ] **Step 5: `commands/mute.rs` の `sync_server_mutes_core` の冒頭で、ロックを取る**

doc コメントの末尾に次の段落を足す:

```rust
///
/// 同じアカウントの同期は、取得より前に取る排他ロックで直列にする(Issue #456)。保存値の
/// 「読み出し → 比較 → 書き込み」と、メモリ上の集合の差し替えが、並行した同期と交錯しない。
/// ロックの順序は「同期ロック → 境界の書きロック(`clear_account_boundaries` の
/// `invalidate_boundaries`)」で、逆順の経路は無い。
```

関数本体の先頭(`let ids = fetch_muted_and_blocked(client).await?;` の前)に足す:

```rust
    let sync_lock = state.server_mute_sync_lock(account_id);
    let _serialized = sync_lock.lock().await;
```

- [ ] **Step 6: `commands/account.rs` の `remove_account` で、ロックのエントリを捨てる**

`state.forget_translator_available(&account_id);` の直後に足す:

```rust
    state.forget_server_mute_sync_lock(&account_id);
```

- [ ] **Step 7: テストを流して、全件が通ることを確認する**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -8`
Expected: `test result: ok.` で失敗 0 件(追加4件を含む。`sync_waits_for_the_same_accounts_sync_lock_before_fetching` は約200ms かかる)。

- [ ] **Step 8: clippy の警告数が増えていないことを確認する**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: `23`。

- [ ] **Step 9: コミットする**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/mute.rs src-tauri/src/commands/account.rs
git commit -m "fix: 同じアカウントのサーバー側ミュート同期をアカウント単位のロックで直列にする" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 10: 変異確認(コミット後。確認したら必ず元に戻す)**

`commands/mute.rs` の `sync_server_mutes_core` の `let _serialized = sync_lock.lock().await;` の行を、一時的に削除する(`sync_lock` の未使用警告は無視してよい)。

Run: `cd src-tauri && cargo test --lib sync_waits_for_the_same_accounts_sync_lock 2>&1 | tail -15`
Expected: `ロックを持たれている間は完了しない` で FAIL する。

確認したら元に戻す: `git checkout -- src-tauri/src/commands/mute.rs` そして `git status --short` が空であることを確認する。

## Task 3: 既存の設計 docs の追記と、最終確認

**Files:**
- Modify: `docs/superpowers/specs/2026-10-03-server-mute-boundary-design.md`(「既知の制限」の段落に1文)

**Interfaces:**
- Consumes: Task 1・2 の完了。
- Produces: なし。

- [ ] **Step 1: 「既知の制限」に、解消した旨を追記する**

`docs/superpowers/specs/2026-10-03-server-mute-boundary-design.md` の `既知の制限: ...` の段落(45行目付近)の末尾に、次の1文を足す。

```
→ Issue #456 P2 で解消した(キーを生の `mutedWords` 要素から作る。`docs/superpowers/specs/2026-10-04-server-mute-sync-correctness-design.md`)。
```

- [ ] **Step 2: 全体のテストを流す**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: すべて `ok` で、`FAILED` なし。(`generates_frontend_bindings` が走るので、`git status --short` で `frontend/src/bindings/tauri.gen.ts` に差分が出ていないことも確認する。出ていたら、今回の変更が原因ではないか調べ、原因が今回なら直す。)

- [ ] **Step 3: コミットする**

```bash
git add docs/superpowers/specs/2026-10-03-server-mute-boundary-design.md
git commit -m "docs: サーバー側ミュートの既知の制限が解消したことを追記" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

## 完了後(実行者が行う。ユーザーの承認を取ってから)

- push して PR を作る。本文は `.github/pull_request_template.md` の構成に沿い、関連 Issue は `Refs #456`(自動クローズさせない)。検証欄には、実機・実 UI の確認をしていない旨を書く。
- マージ後、#456 の対応済みの3項目(`i` フラグ、同時実行の競合、不正な正規表現)にチェックを入れる。
- `remove_account` の1行は、`State<'_, AppState>` が要るためテストが無い。P3(#456)で扱う。
