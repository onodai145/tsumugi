# 外部サービスのURL設定の検証 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 検索エンジンURLとURLプレビュー取得先に、スキーム・形式の検証と、インポート由来の取得先に対する宛先検査(SSRF対策)を入れる。

**Architecture:** 検証関数をRustの `domain/ui.rs` に置き(正)、`set_ui_prefs`・読み込み・インポートの3経路から使う。取得先が「利用者が設定画面で保存した値」か「インポートで入った値」かを `SettingsData.summaly_proxy_imported` に持ち、`fetch_url_preview` は常に `fetch_http`(`net_guard`)で取得して、手入力した取得先のホストだけを許可リストに入れる。フロントは同じ規則の検証と、インポート由来である旨の表示・保存し直しによる信頼を持つ。

**Tech Stack:** Rust(`url` クレート、`wiremock`、`tauri-specta`)、Svelte 5 + Vitest。

**Spec:** `docs/superpowers/specs/2026-11-05-issue-549-external-url-validation-design.md`

## Global Constraints

- ブランチは `fix/issue-549-external-url-validation`(作成済み)。`main` では編集しない。
- コミットメッセージは件名1行のみ(本文なし)。末尾にトレーラー `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を別段落で付ける。形式は `type(scope): 件名 (#549)`。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告する(再試行しない)。
- URLの最大長は2048文字。許可するスキームは `http` と `https` のみ。ホストがあり、ユーザー名・パスワードを含まないこと。
- 検索エンジンURLは `{query}` を含み、`{query}` が認証部(スキーム、ユーザー情報、ホスト、ポート)に入っていないこと。判定は `{query}` を `x` と `y` に置換してそれぞれパースし、スキーム・ホスト・ポートが一致することで行う。
- URLプレビュー取得先は空欄を許可する(=接続先インスタンスの `/url`)。
- 手入力した値は宛先を制限しない。インポートした値は取得時に `net_guard` で制限する。
- `UiPrefs` の型とエクスポート形式は変えない。TSバインディングはコマンド2つの追加だけが変わる。`tauri.gen.ts` は手編集せず `cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- ログに URL の値を出さない(秘密を含みうる)。
- 実機確認やdev起動で自分が起動したプロセスは、タスク完了前に正確なPIDでkillする(`pkill`/`killall` を名前で使わない)。
- 実UIの確認は必ずXvfb越しで、`WAYLAND_DISPLAY` を unset し、`dbus-run-session` で隔離する。`cargo tauri dev` はリポジトリルートから実行する。

## Review Focus

仕様が暗黙に求めるが、素直に書くとテストから漏れやすい入力。各行のテストは担当タスクに入れてある。

- `{query}` がホスト位置にあるテンプレート(`https://{query}.evil.example/`、`https://{query}/`、`https://example.com{query}`)は拒否される(Task 2、Task 8)。ノート本文のテキストが宛先を決められてはいけない。
- インポート由来の取得先が **IPリテラル**(`http://127.0.0.1:PORT/url`)のとき、DNSリゾルバを通らないので、送信前の `validate_url` で止まる。リクエストが1件も届かない(Task 6)。
- 手入力した取得先が別のプライベートホストへ **リダイレクト** しても、その先は拒否される(Task 6)。
- 既存の `settings.json` に不正値(`javascript:` の検索URL、`ftp:` の取得先)が残っていても、起動でき、他セクションの保存が失敗し続けない(Task 4)。
- インポートした取得先が現在の値と同じでインポート由来のとき、勝手に信頼されない。取得先が空のインポートは、インポート由来を引き継がない(Task 7)。大文字のスキームや先頭の空白(` JavaScript:alert(1)`)でも拒否される(Task 2、Task 8)。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `src-tauri/src/domain/ui.rs` | URL検証関数、`UiPrefs::validate_external_urls` / `reset_invalid_external_urls` |
| `src-tauri/src/net_guard.rs` | 許可リストの `[IPv6]:port` 対応、`allowlist_entry_for_url` |
| `src-tauri/src/store/settings.rs` | `summaly_proxy_imported`、検証つき `save_ui`、読み込み時のリセット、信頼・許可リスト項目の取得 |
| `src-tauri/src/state.rs` | 許可リストにsummalyホストを含める |
| `src-tauri/src/commands/mute.rs` | `set_ui_prefs` の許可リスト更新、新コマンド2つ |
| `src-tauri/src/lib.rs` | 新コマンドの登録 |
| `src-tauri/src/api/url_preview.rs` | 許可リストを受け取り、送信前検査とブロック時のエラー化 |
| `src-tauri/src/commands/note.rs` | `fetch_url_preview` を `fetch_http` 経由に |
| `src-tauri/src/store/settings_transfer.rs` | インポート時の検証と `summaly_proxy_imported` |
| `frontend/src/lib/externalUrl.ts`(新規) | フロント側の検証 |
| `frontend/src/lib/searchEngine.ts` | `buildSearchUrl` のフォールバック |
| `frontend/src/ui/settings/ExternalIntegrationSection.svelte` | 検証・注意文・信頼 |
| `docs/guide/user-guide.md`、`CLAUDE.md`、544の仕様書 | ドキュメント |

---

### Task 0: 仕様書と計画書をコミットする

**Files:**
- Add: `docs/superpowers/specs/2026-11-05-issue-549-external-url-validation-design.md`
- Add: `docs/superpowers/plans/2026-11-05-issue-549-external-url-validation.md`

- [ ] **Step 1: ブランチを確認してコミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git branch --show-current   # fix/issue-549-external-url-validation
git add docs/superpowers/specs/2026-11-05-issue-549-external-url-validation-design.md docs/superpowers/plans/2026-11-05-issue-549-external-url-validation.md
git commit -m "docs(spec): 外部サービスURLの検証の設計と計画 (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 1: WebViewでの `javascript:` / `data:` リンクの扱いを調べる(調査。コードは変えない)

受け入れ条件「WebView内でのリンクの扱いを調べた結果が、Issueに記録されている」を満たす。他のタスクとは独立で、`main` 相当のコード(まだ検証が入っていない状態)で行う。結果は検証ルール(`http`/`https` のみ)を変えない想定。

**Files:** なし(調査結果はIssue #549のコメントに残す)。

- [ ] **Step 1: 外部リンクの開き方をコードで確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
grep -rnE "closest\(.a.\)|closest\(\"a\"\)|target=.?_blank|plugin-opener|openUrl" frontend/src --include=*.ts --include=*.svelte | grep -v "\.test\." | head -30
grep -rn "opener" src-tauri/capabilities src-tauri/tauri.conf.json src-tauri/src/lib.rs 2>/dev/null | head
```

見るもの: `<a target="_blank">` のクリックをアプリが横取りして `openUrl` に渡しているか、WebViewの既定動作に任せているか。`opener:default` のスコープが `http`/`https`/`mailto`/`tel` のみか(`tauri-plugin-opener` の公式ドキュメントで確認する)。

- [ ] **Step 2: 隔離したインスタンスを起動する**

メモリの手順(`reference-isolated-instance-needs-short-xdg-path`、`feedback-dev-server-verification-must-use-virtual-display`、`tauri-single-instance-blocks-xdg-isolated-verification`)に従う。デバッグブリッジのソケット(`$XDG_CACHE_HOME/com.onodai.tsumugi/debug-bridge.sock`)のパス長が `SUN_LEN`(108)を超えると黙って失敗するので、`/tmp` 直下の短いパスを使う。

```bash
mkdir -p /tmp/t549/{c,k,d}
Xvfb :99 -screen 0 1280x800x24 >/tmp/t549/xvfb.log 2>&1 &
echo "xvfb pid=$!"
cd /home/onodai145/repos/github.com/onodai145/tsumugi
env -u WAYLAND_DISPLAY DISPLAY=:99 XDG_CONFIG_HOME=/tmp/t549/c XDG_CACHE_HOME=/tmp/t549/k XDG_DATA_HOME=/tmp/t549/d \
  dbus-run-session -- cargo tauri dev >/tmp/t549/dev.log 2>&1 &
echo "dev pid=$!"
```

起動の完了は `ls /tmp/t549/k/com.onodai.tsumugi/debug-bridge.sock` で待つ(ポーリングのループは使わず、ログを見て判断する)。

- [ ] **Step 3: ブリッジで、`javascript:` / `data:` のリンクをクリックしたときの挙動を調べる**

ブリッジの本体は同期関数として実行され、`return` が必要(`debug-bridge-does-not-support-await`)。返り値は二重にJSONエンコードされている。

```bash
SOCK=/tmp/t549/k/com.onodai.tsumugi/debug-bridge.sock
curl -s --unix-socket $SOCK http://localhost/ --data-binary '
  window.__t549 = [];
  for (const [id, href] of [["js","javascript:window.__t549.push(\"js-ran\")"],["data","data:text/html,<script>1</script>"],["https","https://example.com/"]]) {
    const a = document.createElement("a");
    a.id = "t549-" + id; a.href = href; a.target = "_blank"; a.rel = "noreferrer noopener"; a.textContent = id;
    a.style.cssText = "position:fixed;left:10px;top:" + (20 + 30 * window.__t549.length) + "px;z-index:99999";
    document.body.appendChild(a);
  }
  return "ok";'
```

その後、(a) ブリッジから `document.getElementById("t549-js").click()` を呼ぶ、(b) `xdotool`(あれば)で実クリックする、の両方で `window.__t549` の中身と、新しいウィンドウ・ナビゲーションが起きたか(`/tmp/t549/dev.log` の警告、`window.location.href` が変わっていないか)を確認する。

結果として、次の3点を表にまとめる: `javascript:` が実行されるか、`data:` が開くか、`https:` は期待通りに外部ブラウザで開くか。

- [ ] **Step 4: 後始末**

```bash
ps aux | grep -E "Xvfb :99|t549" | grep -v grep   # 自分が起動した PID だけを確認する
kill <xvfb pid> <dev pid と、その子プロセスの cargo-tauri / tsumugi / vite の PID>
```

名前による一括kill(`pkill` `killall`)は使わない。

- [ ] **Step 5: 結果をIssueに記録する(投稿の前にユーザーへ内容を見せて承認を得る)**

下書きを `/home/onodai145/.claude/jobs/20a18c17/tmp/issue549-comment.md` に書き、ユーザーに見せる。承認されたら次を実行する。

```bash
gh issue comment 549 --body-file /home/onodai145/.claude/jobs/20a18c17/tmp/issue549-comment.md
```

---

### Task 2: URL検証関数(Rust)

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`(`default_search_engine_url` の近くに追加、末尾の `mod tests` にテスト追加)

**Interfaces:**
- Produces(後続のタスクが使う):
  - `impl UiPrefs { pub fn validate_external_urls(&self) -> Result<()>; pub fn reset_invalid_external_urls(&mut self) -> bool }`(後者は戻したら `true`)
  - 同ファイル内: `pub const MAX_EXTERNAL_URL_LEN: usize = 2048;`、`pub fn validate_search_engine_url(template: &str) -> Result<()>`、`pub fn validate_summaly_proxy_url(raw: &str) -> Result<()>`(空欄・空白のみは `Ok`)。モジュールが非公開なので、他のモジュールは使えない
  - エラーはすべて `Error::Invalid(String)`。`validate_external_urls` のメッセージは「検索エンジンのURL: …」「URLプレビューの取得先: …」の接頭辞つき。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/ui.rs` の `mod tests` の中に追加する。

```rust
    #[test]
    fn search_engine_url_accepts_http_and_https_templates() {
        for ok in [
            "https://www.google.com/search?q={query}",
            "http://localhost:8080/s?q={query}",
            "https://example.com/{query}/x",
            "HTTPS://Example.com/?q={query}",
        ] {
            assert!(validate_search_engine_url(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn search_engine_url_rejects_dangerous_or_malformed_templates() {
        for bad in [
            "javascript:alert({query})",
            "JavaScript:alert({query})",
            " javascript:alert({query})",
            "data:text/html,{query}",
            "file:///etc/passwd?{query}",
            "ftp://example.com/{query}",
            "https://example.com/",
            "",
            "https://user:pass@example.com/?q={query}",
            "https://{query}@example.com/",
            "https://{query}.example.com/",
            "https://{query}/",
            "https://example.com:{query}/",
            "https://example.com{query}",
            "not a url {query}",
        ] {
            assert!(validate_search_engine_url(bad).is_err(), "{bad}");
        }
        let too_long = format!("https://example.com/?q={{query}}{}", "a".repeat(MAX_EXTERNAL_URL_LEN));
        assert!(validate_search_engine_url(&too_long).is_err());
    }

    #[test]
    fn summaly_proxy_url_accepts_empty_and_http_urls() {
        for ok in [
            "",
            "   ",
            "https://summaly.example.com/",
            "http://192.168.1.10:3030/url",
            "http://localhost:3030",
            "http://[::1]:3030/url",
        ] {
            assert!(validate_summaly_proxy_url(ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn summaly_proxy_url_rejects_other_schemes_and_malformed_urls() {
        for bad in [
            "javascript:alert(1)",
            "data:,x",
            "ftp://example.com",
            "file:///x",
            "https://u:p@example.com/",
            "example.com/url",
            "http://",
            "//example.com",
        ] {
            assert!(validate_summaly_proxy_url(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ui_prefs_default_passes_external_url_validation() {
        assert!(UiPrefs::default().validate_external_urls().is_ok());
    }

    #[test]
    fn validate_external_urls_names_the_offending_field() {
        let mut p = UiPrefs::default();
        p.search_engine_url = "javascript:alert({query})".into();
        assert!(p.validate_external_urls().unwrap_err().to_string().contains("検索エンジンのURL"));

        let mut p = UiPrefs::default();
        p.summaly_proxy_url = "ftp://example.com".into();
        assert!(p.validate_external_urls().unwrap_err().to_string().contains("URLプレビューの取得先"));
    }

    #[test]
    fn reset_invalid_external_urls_restores_defaults_only_for_invalid_fields() {
        let mut p = UiPrefs::default();
        p.search_engine_url = "javascript:alert({query})".into();
        p.summaly_proxy_url = "data:text/plain,x".into();
        assert!(p.reset_invalid_external_urls());
        assert_eq!(p.search_engine_url, UiPrefs::default().search_engine_url);
        assert_eq!(p.summaly_proxy_url, "");

        let mut ok = UiPrefs::default();
        ok.search_engine_url = "https://duckduckgo.com/?q={query}".into();
        ok.summaly_proxy_url = "http://192.168.1.10:3030/url".into();
        assert!(!ok.reset_invalid_external_urls());
        assert_eq!(ok.search_engine_url, "https://duckduckgo.com/?q={query}");
        assert_eq!(ok.summaly_proxy_url, "http://192.168.1.10:3030/url");
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib domain::ui 2>&1 | tail -20`
Expected: コンパイルエラー(`validate_search_engine_url` などが未定義)。

- [ ] **Step 3: 実装する**

`src-tauri/src/domain/ui.rs` の先頭の `use` に `use crate::error::{Error, Result};` を足す。`default_search_engine_url` の直後に次を追加する。

```rust
/// 外部サービスのURL(検索エンジン、URLプレビューの取得先)の最大長(文字数)。
pub const MAX_EXTERNAL_URL_LEN: usize = 2048;

/// 検索エンジンURLの、クエリを差し込む位置を表すプレースホルダ。
const QUERY_PLACEHOLDER: &str = "{query}";

fn invalid_url(msg: &str) -> Error {
    Error::Invalid(msg.to_string())
}

/// `http` / `https` で、ホストがあり、認証情報を含まないURLだけ通す(Issue #549)。
fn parse_http_url(raw: &str) -> Result<url::Url> {
    if raw.chars().count() > MAX_EXTERNAL_URL_LEN {
        return Err(invalid_url("URLが長すぎます(2048文字まで)"));
    }
    let parsed = url::Url::parse(raw).map_err(|_| invalid_url("URLの形式が正しくありません"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid_url("URLのスキームはhttpまたはhttpsにしてください"));
    }
    if parsed.host().is_none() {
        return Err(invalid_url("URLにホストがありません"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(invalid_url("URLにユーザー名やパスワードは含められません"));
    }
    Ok(parsed)
}

/// 検索エンジンURLのテンプレートを検証する。`{query}` を含み、`{query}` を置換したURLが
/// `http` / `https` として正しく、かつ `{query}` が宛先(ホスト・ポート)の位置に入っていないこと。
/// ノート本文のテキストが宛先を決められないようにするため、`{query}` を `x` と `y` に置換した
/// 2つのURLのスキーム・ホスト・ポートが一致することで確かめる。
pub fn validate_search_engine_url(template: &str) -> Result<()> {
    if !template.contains(QUERY_PLACEHOLDER) {
        return Err(invalid_url("検索URLには{query}を含めてください"));
    }
    let with_x = parse_http_url(&template.replacen(QUERY_PLACEHOLDER, "x", 1))?;
    let with_y = parse_http_url(&template.replacen(QUERY_PLACEHOLDER, "y", 1))?;
    if with_x.scheme() != with_y.scheme()
        || with_x.host_str() != with_y.host_str()
        || with_x.port() != with_y.port()
    {
        return Err(invalid_url("{query}はホストやポートの位置には置けません"));
    }
    Ok(())
}

/// URLプレビューの取得先を検証する。空欄(空白のみを含む)は「接続先インスタンスの `/url`」で有効。
pub fn validate_summaly_proxy_url(raw: &str) -> Result<()> {
    if raw.trim().is_empty() {
        return Ok(());
    }
    parse_http_url(raw).map(|_| ())
}

fn with_label(label: &str, e: Error) -> Error {
    match e {
        Error::Invalid(m) => Error::Invalid(format!("{label}: {m}")),
        other => other,
    }
}

impl UiPrefs {
    /// 外部サービスの2つのURLを検証する(保存・インポートの入口で使う)。
    pub fn validate_external_urls(&self) -> Result<()> {
        validate_search_engine_url(&self.search_engine_url).map_err(|e| with_label("検索エンジンのURL", e))?;
        validate_summaly_proxy_url(&self.summaly_proxy_url).map_err(|e| with_label("URLプレビューの取得先", e))?;
        Ok(())
    }

    /// 不正な値を既定値に戻す(読み込み時用)。戻したら `true`。値そのものはログに出さない。
    pub fn reset_invalid_external_urls(&mut self) -> bool {
        let mut changed = false;
        if validate_search_engine_url(&self.search_engine_url).is_err() {
            log::warn!("検索エンジンのURLが不正だったため既定値に戻しました");
            self.search_engine_url = default_search_engine_url();
            changed = true;
        }
        if validate_summaly_proxy_url(&self.summaly_proxy_url).is_err() {
            log::warn!("URLプレビューの取得先が不正だったため空欄に戻しました");
            self.summaly_proxy_url = String::new();
            changed = true;
        }
        changed
    }
}
```

`domain/mod.rs` の `mod ui;` は非公開で、`BackgroundKind` と `UiPrefs` だけを再エクスポートしている。後続のタスクは `UiPrefs` のメソッド(`validate_external_urls` / `reset_invalid_external_urls`)だけを使うので、再エクスポートの追加は要らない。URL検証の自由関数は `domain/ui.rs` の中のテストからだけ直接使う。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib domain::ui 2>&1 | tail -20`
Expected: すべて PASS。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/domain
git commit -m "fix(domain): 外部サービスURLの検証関数を追加 (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 許可リストの `[IPv6]:port` 対応と、URLから許可リスト項目を作る補助

`parse_entry` の `split_port` は、最後の `:` 以降が数字だと `::1` を「ホスト `:` ・ポート `1`」と誤認する。IPv6リテラルの取得先を許可リストに入れられるよう、角括弧つきの書式 `[::1]` / `[::1]:8080` を扱えるようにする。許可リストのキーは `validate_ip_literal` が使う `ip.to_string()`(括弧なし)に合わせる。

**Files:**
- Modify: `src-tauri/src/net_guard.rs`(`parse_entry`、新関数、`mod tests`)

**Interfaces:**
- Produces: `pub(crate) fn allowlist_entry_for_url(url: &url::Url) -> Option<String>` — ホスト名はそのまま、IPv6は `[…]`、ポートはURLに明示されているときだけ `:port` を付けた文字列。`FetchAllowlist::replace` にそのまま渡せる。

- [ ] **Step 1: 失敗するテストを書く**

`net_guard.rs` の `mod tests` に追加する(既存の `url(s)` ヘルパーを使う)。

```rust
    #[test]
    fn allowlist_accepts_bracketed_ipv6_with_and_without_port() {
        let allow = FetchAllowlist::default();
        allow.replace(["[::1]:8080", "[2001:db8::1]"]);
        assert!(allow.allows("::1", 8080));
        assert!(!allow.allows("::1", 80));
        assert!(allow.allows("2001:db8::1", 443));
        assert!(allow.allows("2001:db8::1", 80));
    }

    #[test]
    fn allowlist_drops_malformed_bracketed_entries() {
        let allow = FetchAllowlist::default();
        allow.replace(["[::1", "[::1]x", "[::1]:99999", "[notip]:80"]);
        assert!(!allow.contains_host("::1"));
    }

    #[test]
    fn validate_url_allows_ipv6_literal_on_exact_allowlist_match() {
        let allow = FetchAllowlist::default();
        allow.replace(["[::1]:8080"]);
        assert!(validate_url(&url("http://[::1]:8080/x"), &allow).is_ok());
        assert!(validate_url(&url("http://[::1]:9090/x"), &allow).is_err());
        assert!(validate_url(&url("http://[::1]/x"), &FetchAllowlist::default()).is_err());
    }

    #[test]
    fn allowlist_entry_for_url_keeps_the_explicit_port_only() {
        let e = |s: &str| allowlist_entry_for_url(&url(s));
        assert_eq!(e("https://proxy.example.com/url").as_deref(), Some("proxy.example.com"));
        assert_eq!(e("http://192.168.1.10:3030/url").as_deref(), Some("192.168.1.10:3030"));
        assert_eq!(e("http://[::1]:3030/url").as_deref(), Some("[::1]:3030"));
        assert_eq!(e("http://[::1]/url").as_deref(), Some("[::1]"));
        // 既定ポートは `url` クレートが落とすので、ホストだけ(80と443を許可)になる。
        assert_eq!(e("https://proxy.example.com:443/url").as_deref(), Some("proxy.example.com"));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib net_guard 2>&1 | tail -20`
Expected: コンパイルエラー(`allowlist_entry_for_url` が未定義)。

- [ ] **Step 3: 実装する**

`parse_entry` を次のように書き換える(角括弧つきの分岐を先頭に足す)。

```rust
fn parse_entry(raw: &str) -> Option<(String, Vec<u16>)> {
    let h = raw.trim();
    // `[IPv6]` / `[IPv6]:port`。キーは `validate_ip_literal` が使う括弧なしの正規形に合わせる。
    if let Some(rest) = h.strip_prefix('[') {
        let (inner, after) = rest.split_once(']')?;
        let ip: Ipv6Addr = inner.parse().ok()?;
        let ports = if after.is_empty() {
            vec![80, 443]
        } else {
            vec![after.strip_prefix(':')?.parse::<u16>().ok()?]
        };
        return Some((ip.to_string(), ports));
    }
    let ports = match split_port(h).1 {
        Some(p) => vec![p.parse::<u16>().ok()?],
        None => vec![80, 443],
    };
    Some((host_key(h), ports))
}
```

`parse_entry` の直後に追加する。

```rust
/// URLの宛先を `FetchAllowlist::replace` に渡せる文字列にする。ポートはURLに明示されているときだけ付ける
/// (付けないと80と443を許可する)。ホストが無いURLは `None`。
pub(crate) fn allowlist_entry_for_url(url: &url::Url) -> Option<String> {
    let host = match url.host()? {
        url::Host::Domain(d) => d.to_string(),
        url::Host::Ipv4(v4) => v4.to_string(),
        url::Host::Ipv6(v6) => format!("[{v6}]"),
    };
    Some(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib net_guard 2>&1 | tail -20`
Expected: 既存テストも含めすべて PASS。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/net_guard.rs
git commit -m "feat(net_guard): 許可リストでIPv6リテラルを扱えるようにする (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: SettingsStore — 由来の記録、検証つき保存、読み込み時のリセット

**Files:**
- Modify: `src-tauri/src/store/settings.rs`(`SettingsData`、`save_ui`、新メソッド、`load_json_or_default`、`import legacy` の `SettingsData` リテラル(約646行目)、`mod tests`)

**Interfaces:**
- Consumes: `UiPrefs::validate_external_urls`、`UiPrefs::reset_invalid_external_urls`(Task 2)、`net_guard::allowlist_entry_for_url`(Task 3)。
- Produces:
  - `SettingsData.summaly_proxy_imported: bool`(`pub(crate)`、`#[serde(default)]`)
  - `SettingsStore::save_ui(&self, &UiPrefs) -> Result<()>` — 検証に落ちたら `Error::Invalid` で何も変えない。取得先URLが保存済みと違えば `summaly_proxy_imported = false`
  - `SettingsStore::summaly_proxy_imported(&self) -> bool` — 取得先が空でなく、かつフラグが真のときだけ真
  - `SettingsStore::trust_summaly_proxy(&self) -> Result<()>` — フラグを偽にして保存
  - `SettingsStore::trusted_summaly_allowlist_entry(&self) -> Option<String>` — 取得先が空でなく、かつフラグが偽のときだけ `Some`

- [ ] **Step 1: 失敗するテストを書く**

`settings.rs` の `mod tests` に追加する(`store()` ヘルパーを使う)。

```rust
    #[test]
    fn save_ui_rejects_dangerous_external_urls_and_keeps_the_old_value() {
        let s = store();
        let mut ui = UiPrefs::default();
        ui.search_engine_url = "javascript:alert({query})".into();
        assert!(s.save_ui(&ui).is_err());
        assert_eq!(s.load_ui().unwrap().search_engine_url, UiPrefs::default().search_engine_url);

        let mut ui = UiPrefs::default();
        ui.summaly_proxy_url = "ftp://example.com/".into();
        assert!(s.save_ui(&ui).is_err());
        assert_eq!(s.load_ui().unwrap().summaly_proxy_url, "");
    }

    #[test]
    fn changing_the_summaly_url_clears_the_imported_flag_but_other_changes_do_not() {
        let s = store();
        let mut data = SettingsData::default();
        data.ui.summaly_proxy_url = "https://a.example/url".into();
        data.summaly_proxy_imported = true;
        s.replace(data).unwrap();
        assert!(s.summaly_proxy_imported());

        let mut ui = s.load_ui().unwrap();
        ui.theme = "dark".into();
        s.save_ui(&ui).unwrap();
        assert!(s.summaly_proxy_imported(), "他の項目の保存では、インポート由来のまま");

        ui.summaly_proxy_url = "https://b.example/url".into();
        s.save_ui(&ui).unwrap();
        assert!(!s.summaly_proxy_imported(), "取得先を変えたら手入力扱い");
    }

    #[test]
    fn trust_summaly_proxy_clears_the_flag_and_exposes_the_allowlist_entry() {
        let s = store();
        let mut data = SettingsData::default();
        data.ui.summaly_proxy_url = "http://192.168.1.10:3030/url".into();
        data.summaly_proxy_imported = true;
        s.replace(data).unwrap();
        assert_eq!(s.trusted_summaly_allowlist_entry(), None, "インポート由来は許可リストに入れない");

        s.trust_summaly_proxy().unwrap();
        assert!(!s.summaly_proxy_imported());
        assert_eq!(s.trusted_summaly_allowlist_entry().as_deref(), Some("192.168.1.10:3030"));
    }

    #[test]
    fn imported_flag_is_false_and_no_entry_when_no_summaly_url_is_set() {
        let s = store();
        let mut data = SettingsData::default();
        data.summaly_proxy_imported = true; // 取得先が空
        s.replace(data).unwrap();
        assert!(!s.summaly_proxy_imported());
        assert_eq!(s.trusted_summaly_allowlist_entry(), None);
    }

    #[test]
    fn invalid_external_urls_in_the_file_are_reset_on_load_without_rewriting_the_file() {
        let path = std::env::temp_dir().join(format!("tsumugi-bad-urls-{}.json", uuid::Uuid::new_v4()));
        let json = r#"{"ui":{"theme":"dark","searchEngineUrl":"javascript:alert({query})","summalyProxyUrl":"data:text/plain,x"}}"#;
        std::fs::write(&path, json).unwrap();

        let s = SettingsStore::new(path.clone(), std::env::temp_dir()).unwrap();
        let ui = s.load_ui().unwrap();
        assert_eq!(ui.search_engine_url, UiPrefs::default().search_engine_url);
        assert_eq!(ui.summaly_proxy_url, "");
        assert_eq!(ui.theme, "dark", "他の項目はそのまま");
        // 読み込みだけではファイルを書き換えない(次の保存で反映される)。
        assert_eq!(std::fs::read_to_string(&path).unwrap(), json);

        // 無効な値が残っていないので、他の項目の保存は失敗しない。
        let mut ui = s.load_ui().unwrap();
        ui.theme = "light".into();
        s.save_ui(&ui).unwrap();

        std::fs::remove_file(&path).ok();
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib store::settings 2>&1 | tail -20`
Expected: コンパイルエラー(`summaly_proxy_imported` などが未定義)。

- [ ] **Step 3: 実装する**

`SettingsData` の `server_mute_snapshots` の後にフィールドを足す。

```rust
    /// `ui.summaly_proxy_url` が設定ファイルのインポートで入った値か(Issue #549)。真の間は、取得先を
    /// `fetch_http` の許可リストに入れない(プライベート宛へ接続できない)。偽は「利用者が設定画面で
    /// 保存した値」か、取得先が空であること。エクスポートには含めない。
    #[serde(default)]
    pub(crate) summaly_proxy_imported: bool,
```

`SettingsData { ... }` を直接書いている箇所(約646行目の `import legacy` 系の関数)に `summaly_proxy_imported: false,` を足す。他にコンパイルエラーになる箇所があれば同様に直す。

`save_ui` を置き換え、メソッドを足す。

```rust
    /// 表示設定を保存する。外部サービスのURLが不正なら `Error::Invalid` で何も変えない(Issue #549)。
    /// URLプレビューの取得先が保存済みの値と違うときは、利用者が入力した値として扱う
    /// (`summaly_proxy_imported` を偽にする)。
    pub fn save_ui(&self, prefs: &UiPrefs) -> Result<()> {
        prefs.validate_external_urls()?;
        let mut guard = self.data.lock().unwrap();
        if prefs.summaly_proxy_url != guard.ui.summaly_proxy_url {
            guard.summaly_proxy_imported = false;
        }
        guard.ui = prefs.clone();
        self.save(&guard)
    }

    /// 取得先が設定されていて、かつインポート由来(まだ利用者が保存し直していない)か。
    pub fn summaly_proxy_imported(&self) -> bool {
        let g = self.data.lock().unwrap();
        g.summaly_proxy_imported && !g.ui.summaly_proxy_url.trim().is_empty()
    }

    /// インポート由来の取得先を、利用者が設定した値として扱う。
    pub fn trust_summaly_proxy(&self) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        if !guard.summaly_proxy_imported {
            return Ok(());
        }
        guard.summaly_proxy_imported = false;
        self.save(&guard)
    }

    /// `fetch_http` の許可リストに入れる、利用者が設定した取得先の `host[:port]`。
    /// 取得先が空、インポート由来、またはURLとして読めないときは `None`。
    pub fn trusted_summaly_allowlist_entry(&self) -> Option<String> {
        let g = self.data.lock().unwrap();
        if g.summaly_proxy_imported {
            return None;
        }
        let raw = g.ui.summaly_proxy_url.trim();
        if raw.is_empty() {
            return None;
        }
        let parsed = url::Url::parse(raw).ok()?;
        crate::net_guard::allowlist_entry_for_url(&parsed)
    }
```

`load_json_or_default` の `let data: SettingsData = serde_json::from_value(value)?;` を次に変える(`migrated` の書き戻しはこの後でも既存どおり)。

```rust
    let mut data: SettingsData = serde_json::from_value(value)?;
    // 不正な外部サービスURLは既定値に戻す(Issue #549)。ファイルは書き換えない(次の保存で反映される)。
    data.ui.reset_invalid_external_urls();
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib store::settings 2>&1 | tail -20`
Expected: すべて PASS。`cargo build` で他の呼び出し元(`SettingsData` のリテラルなど)がコンパイルエラーになる場合は、`summaly_proxy_imported: false` を足して直す。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/settings.rs
git commit -m "fix(settings): 外部サービスURLの検証と取得先の由来の記録を追加 (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: 許可リストへの配線と、新コマンド2つ

**Files:**
- Modify: `src-tauri/src/state.rs`(`new_with_sound`、`refresh_fetch_allowlist`、`mod tests`)
- Modify: `src-tauri/src/commands/mute.rs`(`set_ui_prefs`、新コマンド)
- Modify: `src-tauri/src/lib.rs`(`specta_builder()` に登録)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: `SettingsStore::trusted_summaly_allowlist_entry`、`summaly_proxy_imported`、`trust_summaly_proxy`(Task 4)。
- Produces:
  - コマンド `get_summaly_proxy_imported() -> Result<bool>`(TS: `commands.getSummalyProxyImported()`)
  - コマンド `trust_summaly_proxy() -> Result<()>`(TS: `commands.trustSummalyProxy()`)
  - `AppState::refresh_fetch_allowlist` が、利用者が設定した取得先のホストも許可リストに入れる。

- [ ] **Step 1: 失敗するテストを書く**

`state.rs` の `mod tests` に追加する。

```rust
    #[test]
    fn fetch_allowlist_includes_the_summaly_host_only_when_the_user_saved_it() {
        let settings = SettingsStore::new_in_memory();
        let mut ui = crate::domain::UiPrefs::default();
        ui.summaly_proxy_url = "http://192.168.1.10:3030/url".into();
        settings.save_ui(&ui).unwrap();
        let state = AppState::new_for_test(settings);
        assert!(state.fetch_allowlist.allows("192.168.1.10", 3030), "起動時の初期値に入る");
        assert!(!state.fetch_allowlist.allows("192.168.1.10", 80), "ポートは明示されたものだけ");

        // インポート直後相当: 許可リストから外れる
        let mut data = state.settings.snapshot();
        data.summaly_proxy_imported = true;
        state.settings.replace(data).unwrap();
        state.refresh_fetch_allowlist();
        assert!(!state.fetch_allowlist.contains_host("192.168.1.10"));

        // 設定画面で保存し直す(信頼する)と入る
        state.settings.trust_summaly_proxy().unwrap();
        state.refresh_fetch_allowlist();
        assert!(state.fetch_allowlist.allows("192.168.1.10", 3030));
    }

    #[test]
    fn fetch_allowlist_has_no_summaly_host_when_the_url_is_empty() {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.refresh_fetch_allowlist();
        assert!(!state.fetch_allowlist.contains_host(""));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib state:: 2>&1 | tail -20`
Expected: 1つ目のテストが FAIL(許可リストに入らない)。

- [ ] **Step 3: 実装する**

`state.rs` の `hosts_with_token` の近くに追加する。

```rust
/// 許可リストに入れるホストの全体: トークンを持つアカウントのホストと、利用者が設定した
/// URLプレビューの取得先(Issue #549。インポート由来の取得先は入れない)。
fn allowlist_hosts(accounts: &[Account], secrets: &dyn SecretStore, settings: &SettingsStore) -> Vec<String> {
    let mut hosts = hosts_with_token(accounts, secrets);
    hosts.extend(settings.trusted_summaly_allowlist_entry());
    hosts
}
```

`new_with_sound` の `fetch_allowlist.replace(hosts_with_token(&accounts, secrets.as_ref()));` を次に置き換える。

```rust
        fetch_allowlist.replace(allowlist_hosts(&accounts, secrets.as_ref(), &settings));
```

`refresh_fetch_allowlist` の本体を次に置き換える。

```rust
    pub fn refresh_fetch_allowlist(&self) {
        let accounts = self.accounts.lock().unwrap().list();
        self.fetch_allowlist
            .replace(allowlist_hosts(&accounts, self.secrets.as_ref(), &self.settings));
    }
```

`refresh_fetch_allowlist` のドキュメントコメントに「利用者が設定したURLプレビューの取得先も含む。取得先を変えた・信頼した・インポートした直後にも呼ぶ」を足す。

`commands/mute.rs` の `set_ui_prefs` を置き換え、直後にコマンドを足す。

```rust
/// 表示設定を更新（永続化）。外部サービスのURLが不正なら `Error::Invalid` で何も変えない(Issue #549)。
#[tauri::command]
#[specta::specta]
pub async fn set_ui_prefs(state: State<'_, AppState>, prefs: UiPrefs) -> Result<()> {
    state.settings.save_ui(&prefs)?;
    // URLプレビューの取得先が変わると、許可リストに入れる項目も変わる。
    state.refresh_fetch_allowlist();
    Ok(())
}

/// URLプレビューの取得先が、設定ファイルのインポートで入った値のままか。設定画面の注意文に使う。
#[tauri::command]
#[specta::specta]
pub async fn get_summaly_proxy_imported(state: State<'_, AppState>) -> Result<bool> {
    Ok(state.settings.summaly_proxy_imported())
}

/// インポートした取得先を、利用者が設定した値として扱う(プライベートアドレスへの接続を許可する)。
#[tauri::command]
#[specta::specta]
pub async fn trust_summaly_proxy(state: State<'_, AppState>) -> Result<()> {
    state.settings.trust_summaly_proxy()?;
    state.refresh_fetch_allowlist();
    Ok(())
}
```

`lib.rs` の `commands::mute::set_ui_prefs,` の直後に追加する。

```rust
            commands::mute::get_summaly_proxy_imported,
            commands::mute::trust_summaly_proxy,
```

- [ ] **Step 4: 通ることを確認し、バインディングを再生成する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib state:: 2>&1 | tail -10 && cargo test generates_frontend_bindings 2>&1 | tail -10`
Expected: PASS。続けて `git diff --stat frontend/src/bindings/tauri.gen.ts` に `getSummalyProxyImported` / `trustSummalyProxy` の追加だけが出ること(それ以外の差分があれば原因を調べる)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/state.rs src-tauri/src/commands/mute.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat(settings): 取得先を許可リストに配線し信頼用のコマンドを追加 (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: `fetch_url_preview` を `fetch_http` 経由にする

IPリテラルのホストはDNSリゾルバを通らないので、送信前の `validate_url` が必要(`fetch_guarded_bytes` と同じ構成)。リダイレクトの各ホップは `fetch_http` のリダイレクトポリシーが検査する。

**Files:**
- Modify: `src-tauri/src/api/url_preview.rs`(`fetch_url_preview` のシグネチャ、`real_url_preview_from_misskey_io` テスト)
- Modify: `src-tauri/src/commands/note.rs`(`fetch_url_preview` コマンド、`mod tests`)

**Interfaces:**
- Consumes: `net_guard::validate_url`、`net_guard::is_blocked_error`、`AppState.fetch_http`、`AppState.fetch_allowlist`。
- Produces:
  - `api::url_preview::fetch_url_preview(http: &reqwest::Client, allow: &FetchAllowlist, proxy_base: &str, target_url: &str) -> Result<UrlPreview>`(`allow` が新しい引数)
  - `commands::note::fetch_url_preview_core(state: &AppState, account_id: &str, url: &str) -> Result<UrlPreview>`(`pub(crate)`、テスト用に切り出し)
  - 拒否されたときは `Error::Invalid` で、メッセージに「許可されていません」を含む。

- [ ] **Step 1: 失敗するテストを書く**

`commands/note.rs` の `mod tests` に追加する(`MockServer` などは既にimport済み)。

```rust
    fn state_with_summaly(url: &str, imported: bool) -> AppState {
        let settings = crate::store::settings::SettingsStore::new_in_memory();
        let mut data = crate::store::settings::SettingsData::default();
        data.ui.summaly_proxy_url = url.into();
        data.summaly_proxy_imported = imported;
        settings.replace(data).unwrap();
        AppState::new_for_test(settings)
    }

    async fn preview_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/url"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "title": "T" })))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn fetch_url_preview_reaches_a_user_saved_loopback_proxy() {
        let server = preview_server().await;
        let state = state_with_summaly(&format!("http://127.0.0.1:{}/url", server.address().port()), false);
        let preview = fetch_url_preview_core(&state, "acc", "https://example.com/").await.unwrap();
        assert_eq!(preview.title.as_deref(), Some("T"));
    }

    #[tokio::test]
    async fn fetch_url_preview_rejects_an_imported_ip_literal_without_connecting() {
        let server = preview_server().await;
        let state = state_with_summaly(&format!("http://127.0.0.1:{}/url", server.address().port()), true);
        let err = fetch_url_preview_core(&state, "acc", "https://example.com/").await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("許可されていません")), "unexpected: {err:?}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_url_preview_rejects_an_imported_hostname_resolving_to_loopback() {
        let server = preview_server().await;
        let state = state_with_summaly(&format!("http://localhost:{}/url", server.address().port()), true);
        let err = fetch_url_preview_core(&state, "acc", "https://example.com/").await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("許可されていません")), "unexpected: {err:?}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_url_preview_rejects_a_redirect_from_a_saved_proxy_to_another_private_host() {
        let target = preview_server().await;
        let proxy = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/url"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("http://127.0.0.1:{}/url", target.address().port())),
            )
            .mount(&proxy)
            .await;
        let state = state_with_summaly(&format!("http://127.0.0.1:{}/url", proxy.address().port()), false);
        let err = fetch_url_preview_core(&state, "acc", "https://example.com/").await.unwrap_err();
        assert!(matches!(err, Error::Invalid(ref m) if m.contains("許可されていません")), "unexpected: {err:?}");
        assert!(target.received_requests().await.unwrap().is_empty());
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib commands::note 2>&1 | tail -20`
Expected: コンパイルエラー(`fetch_url_preview_core` が未定義)。

- [ ] **Step 3: 実装する**

`api/url_preview.rs` の `fetch_url_preview` を次に置き換える。

```rust
/// 取得先が `net_guard` に拒否されたときのメッセージ(Issue #549)。
pub(crate) const BLOCKED_MESSAGE: &str = "取得先が許可されていません(ローカル/社内ネットワークのアドレス)。\
インポートした設定の取得先は、設定の外部連携で保存し直すと使えます";

/// `proxy_base`（例: `"https://misskey.io/url"` またはユーザ設定のカスタムプロキシURL）に
/// `?url=<target_url>` を付与してGETし、応答を [`UrlPreview`] へ正規化する。
/// 認証不要（トークンは付与しない）。
///
/// `http` は `net_guard::build_fetch_client` で作った取得専用クライアントを渡す。IPリテラルの
/// 取得先はDNSリゾルバを通らないため、ここで送信前に `validate_url` で検査する。ホスト名は
/// 接続時(DNS解決後)と、リダイレクトの各ホップで検査される。
pub async fn fetch_url_preview(
    http: &reqwest::Client,
    allow: &crate::net_guard::FetchAllowlist,
    proxy_base: &str,
    target_url: &str,
) -> Result<UrlPreview> {
    let base = url::Url::parse(proxy_base).map_err(|e| Error::Invalid(format!("invalid url: {e}")))?;
    if let Err(blocked) = crate::net_guard::validate_url(&base, allow) {
        log::warn!("url preview blocked before request: {blocked}");
        return Err(Error::Invalid(BLOCKED_MESSAGE.to_string()));
    }
    let resp = http
        .get(base)
        .query(&[("url", target_url)])
        .send()
        .await
        .map_err(|e| {
            if crate::net_guard::is_blocked_error(&e) {
                Error::Invalid(BLOCKED_MESSAGE.to_string())
            } else {
                Error::from(e)
            }
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Api(format!("url preview: HTTP {status}")));
    }
    let raw: RawUrlPreview = resp.json().await?;
    Ok(normalize(raw, target_url))
}
```

同ファイルの `real_url_preview_from_misskey_io` テストの呼び出しを次に直す。

```rust
        let http = reqwest::Client::new();
        let allow = crate::net_guard::FetchAllowlist::default();
        let preview = fetch_url_preview(&http, &allow, "https://misskey.io/url", "https://misskey.io/")
```

`commands/note.rs` の `fetch_url_preview` コマンドを次に置き換える。

```rust
/// URLプレビューを取得する。取得は `fetch_http`(`net_guard` 付き)で行い、利用者が設定画面で
/// 保存した取得先のホストだけが許可リストに入る(Issue #549)。
#[tauri::command]
#[specta::specta]
pub async fn fetch_url_preview(
    state: State<'_, AppState>,
    account_id: String,
    url: String,
) -> Result<UrlPreview> {
    fetch_url_preview_core(&state, &account_id, &url).await
}

pub(crate) async fn fetch_url_preview_core(
    state: &AppState,
    account_id: &str,
    url: &str,
) -> Result<UrlPreview> {
    let prefs = state.settings.load_ui()?;
    let configured = prefs.summaly_proxy_url.trim();
    let proxy_base = if configured.is_empty() {
        let client = state.client_for(account_id)?;
        format!("https://{}/url", client.host())
    } else {
        configured.to_string()
    };
    fetch_url_preview_api(&state.fetch_http, &state.fetch_allowlist, &proxy_base, url).await
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib commands::note 2>&1 | tail -20 && cargo test --lib api::url_preview 2>&1 | tail -10`
Expected: すべて PASS(`#[ignore]` のテストは実行されない)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/api/url_preview.rs src-tauri/src/commands/note.rs
git commit -m "fix(note): URLプレビューの取得を net_guard 経由にする (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: インポート時の検証と由来の記録

**Files:**
- Modify: `src-tauri/src/store/settings_transfer.rs`(`plan_import`、`mod tests`)

**Interfaces:**
- Consumes: `UiPrefs::validate_external_urls`(Task 2)、`SettingsData.summaly_proxy_imported`(Task 4)。
- Produces: `plan_import` が、外部サービスのURLが不正なファイルを `Error::Invalid` でファイルごと拒否し、`ImportPlan.data.summaly_proxy_imported` を設定する。
  - 取得先が空 → `false`
  - 空でなく、現在の値と違う → `true`
  - 現在の値と同じ → 現在の `summaly_proxy_imported` を引き継ぐ

`commands/settings_transfer.rs` の `import_settings_bytes` は、`settings.replace(plan.data)` の後で既に `state.refresh_fetch_allowlist()` を呼んでいるので、変更は要らない(許可リストは `summaly_proxy_imported` を見て決まる)。

- [ ] **Step 1: 失敗するテストを書く**

`settings_transfer.rs` の `mod tests` に追加する(`empty_data()`、`file_with()`、`plan_err()`、`current_with_local_account()` を使う)。

```rust
    #[test]
    fn a_file_with_a_dangerous_search_engine_url_is_rejected_as_a_whole() {
        let mut data = empty_data();
        data.ui.search_engine_url = "javascript:alert({query})".into();
        let err = plan_err(&SettingsData::default(), file_with(data));
        assert!(err.contains("検索エンジンのURL"), "{err}");
    }

    #[test]
    fn a_file_with_a_non_http_summaly_proxy_is_rejected_as_a_whole() {
        let mut data = empty_data();
        data.ui.summaly_proxy_url = "ftp://example.com/url".into();
        let err = plan_err(&SettingsData::default(), file_with(data));
        assert!(err.contains("URLプレビューの取得先"), "{err}");
    }

    #[test]
    fn a_new_summaly_proxy_from_the_file_is_marked_as_imported() {
        let mut data = empty_data();
        data.ui.summaly_proxy_url = "https://p.example/url".into();
        let plan = plan_import(&SettingsData::default(), file_with(data)).unwrap();
        assert!(plan.data.summaly_proxy_imported);
    }

    #[test]
    fn an_empty_summaly_proxy_in_the_file_is_not_marked_as_imported() {
        let mut current = SettingsData::default();
        current.ui.summaly_proxy_url = "https://p.example/url".into();
        current.summaly_proxy_imported = true;
        let plan = plan_import(&current, file_with(empty_data())).unwrap();
        assert!(!plan.data.summaly_proxy_imported);
        assert_eq!(plan.data.ui.summaly_proxy_url, "");
    }

    #[test]
    fn the_same_summaly_proxy_keeps_the_current_imported_flag() {
        let mut data = empty_data();
        data.ui.summaly_proxy_url = "https://p.example/url".into();

        let mut trusted = SettingsData::default();
        trusted.ui.summaly_proxy_url = "https://p.example/url".into();
        assert!(!plan_import(&trusted, file_with(data.clone())).unwrap().data.summaly_proxy_imported);

        let mut untrusted = trusted.clone();
        untrusted.summaly_proxy_imported = true;
        assert!(plan_import(&untrusted, file_with(data)).unwrap().data.summaly_proxy_imported);
    }

    #[test]
    fn a_different_summaly_proxy_is_marked_as_imported_even_if_the_current_one_was_trusted() {
        let mut current = SettingsData::default();
        current.ui.summaly_proxy_url = "https://a.example/url".into();
        let mut data = empty_data();
        data.ui.summaly_proxy_url = "https://b.example/url".into();
        assert!(plan_import(&current, file_with(data)).unwrap().data.summaly_proxy_imported);
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib store::settings_transfer 2>&1 | tail -20`
Expected: 新しいテストが FAIL(不正値が通る、フラグが偽のまま)。

- [ ] **Step 3: 実装する**

`plan_import` の `let d = file.data;` の直後に追加する。

```rust
    // 他人が作ったファイルの外部サービスURLは、形式を確かめる(Issue #549)。
    d.ui.validate_external_urls()?;
```

`let mut ui = d.ui;` の後(`preview` を作る前)に追加する。

```rust
    // 取得先の由来: 空なら対象外、現在の値と違えばインポート由来、同じなら現在の状態を引き継ぐ。
    let summaly_proxy_imported = if ui.summaly_proxy_url.trim().is_empty() {
        false
    } else if ui.summaly_proxy_url != current.ui.summaly_proxy_url {
        true
    } else {
        current.summaly_proxy_imported
    };
```

`let data = SettingsData { ... }` に `summaly_proxy_imported,` を足す(`server_mute_snapshots` の後)。

- [ ] **Step 4: 通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib store::settings_transfer 2>&1 | tail -20 && cargo test --lib commands::settings_transfer 2>&1 | tail -10`
Expected: すべて PASS(既存の `has_custom_endpoints` のテストも通る)。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/store/settings_transfer.rs
git commit -m "fix(settings): インポートで外部サービスURLを検証し取得先の由来を記録する (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: フロントの検証と `buildSearchUrl` のフォールバック

**Files:**
- Create: `frontend/src/lib/externalUrl.ts`
- Create: `frontend/src/lib/externalUrl.test.ts`
- Modify: `frontend/src/lib/searchEngine.ts`
- Modify: `frontend/src/lib/searchEngine.test.ts`

**Interfaces:**
- Produces:
  - `validateSearchEngineUrl(template: string): string | null` — 問題があれば日本語のメッセージ、なければ `null`
  - `validateSummalyProxyUrl(raw: string): string | null`(空欄・空白のみは `null`)
  - `MAX_EXTERNAL_URL_LEN = 2048`
  - `buildSearchUrl` は、検証に落ちるテンプレートでは既定のGoogle検索にフォールバックする。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/externalUrl.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { MAX_EXTERNAL_URL_LEN, validateSearchEngineUrl, validateSummalyProxyUrl } from "./externalUrl";

// Rust側(domain/ui.rs のテスト)と同じ入力例。
describe("validateSearchEngineUrl", () => {
  it.each([
    "https://www.google.com/search?q={query}",
    "http://localhost:8080/s?q={query}",
    "https://example.com/{query}/x",
    "HTTPS://Example.com/?q={query}",
  ])("accepts %s", (ok) => {
    expect(validateSearchEngineUrl(ok)).toBeNull();
  });

  it.each([
    "javascript:alert({query})",
    "JavaScript:alert({query})",
    " javascript:alert({query})",
    "data:text/html,{query}",
    "file:///etc/passwd?{query}",
    "ftp://example.com/{query}",
    "https://example.com/",
    "",
    "https://user:pass@example.com/?q={query}",
    "https://{query}@example.com/",
    "https://{query}.example.com/",
    "https://{query}/",
    "https://example.com:{query}/",
    "https://example.com{query}",
    "not a url {query}",
  ])("rejects %s", (bad) => {
    expect(validateSearchEngineUrl(bad)).not.toBeNull();
  });

  it("rejects a template longer than the limit", () => {
    expect(validateSearchEngineUrl(`https://example.com/?q={query}${"a".repeat(MAX_EXTERNAL_URL_LEN)}`)).not.toBeNull();
  });
});

describe("validateSummalyProxyUrl", () => {
  it.each(["", "   ", "https://summaly.example.com/", "http://192.168.1.10:3030/url", "http://localhost:3030", "http://[::1]:3030/url"])(
    "accepts %j",
    (ok) => {
      expect(validateSummalyProxyUrl(ok)).toBeNull();
    },
  );

  it.each(["javascript:alert(1)", "data:,x", "ftp://example.com", "file:///x", "https://u:p@example.com/", "example.com/url", "http://", "//example.com"])(
    "rejects %j",
    (bad) => {
      expect(validateSummalyProxyUrl(bad)).not.toBeNull();
    },
  );
});
```

`frontend/src/lib/searchEngine.test.ts` の `describe("buildSearchUrl", …)` の中に追加する。

```ts
  it("falls back to Google when the template uses a dangerous scheme", () => {
    for (const bad of ["javascript:alert({query})", "data:text/html,{query}", "https://{query}.evil.example/"]) {
      expect(buildSearchUrl(bad, "cat")).toBe(DEFAULT_SEARCH_ENGINE_URL.replace("{query}", "cat"));
    }
  });
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/externalUrl.test.ts src/lib/searchEngine.test.ts 2>&1 | tail -20`
Expected: `externalUrl` が見つからず FAIL。

- [ ] **Step 3: 実装する**

`frontend/src/lib/externalUrl.ts`:

```ts
// 外部サービスのURL(検索エンジン、URLプレビューの取得先)の検証(Issue #549)。
// Rust側(src-tauri/src/domain/ui.rs)が正で、ここは保存前に即座にエラーを出すための同じ規則。

export const MAX_EXTERNAL_URL_LEN = 2048;
const QUERY_PLACEHOLDER = "{query}";

/// `http` / `https` で、ホストがあり、認証情報を含まないURLだけ通す。通らなければメッセージを返す。
function parseHttpUrl(raw: string): URL | string {
  if (raw.length > MAX_EXTERNAL_URL_LEN) return `URLが長すぎます(${MAX_EXTERNAL_URL_LEN}文字まで)`;
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    return "URLの形式が正しくありません";
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return "URLのスキームはhttpまたはhttpsにしてください";
  }
  if (!parsed.hostname) return "URLにホストがありません";
  if (parsed.username || parsed.password) return "URLにユーザー名やパスワードは含められません";
  return parsed;
}

/// 検索エンジンURLのテンプレートを検証する。問題があればメッセージ、なければ `null`。
/// `{query}` を `x` と `y` に置換した2つのURLのスキーム・ホスト(ポート込み)が一致することで、
/// `{query}` が宛先の位置に入っていないことを確かめる。
export function validateSearchEngineUrl(template: string): string | null {
  if (!template.includes(QUERY_PLACEHOLDER)) return "検索URLには{query}を含めてください";
  const withX = parseHttpUrl(template.replace(QUERY_PLACEHOLDER, "x"));
  if (typeof withX === "string") return withX;
  const withY = parseHttpUrl(template.replace(QUERY_PLACEHOLDER, "y"));
  if (typeof withY === "string") return withY;
  if (withX.protocol !== withY.protocol || withX.host !== withY.host) {
    return "{query}はホストやポートの位置には置けません";
  }
  return null;
}

/// URLプレビューの取得先を検証する。空欄(空白のみを含む)は「接続先インスタンスの /url」で有効。
export function validateSummalyProxyUrl(raw: string): string | null {
  if (raw.trim() === "") return null;
  const parsed = parseHttpUrl(raw);
  return typeof parsed === "string" ? parsed : null;
}
```

`frontend/src/lib/searchEngine.ts` の先頭コメントの後に import を足し、`buildSearchUrl` を書き換える。

```ts
import { validateSearchEngineUrl } from "./externalUrl";
```

```ts
export function buildSearchUrl(template: string | undefined, query: string): string {
  // 空・`{query}` なし・`http`/`https` 以外などの不正な値(壊れた保存値、他人の設定ファイル)は既定にフォールバックする。
  const t = template && validateSearchEngineUrl(template) === null ? template : DEFAULT_SEARCH_ENGINE_URL;
  return t.replace("{query}", encodeURIComponent(query));
}
```

ファイル先頭コメントの「テンプレートが空/プレースホルダを含まない…既定のGoogle検索にフォールバックする」に「`http`/`https` 以外などの不正な値も同様」を足す。

- [ ] **Step 4: 通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/externalUrl.test.ts src/lib/searchEngine.test.ts 2>&1 | tail -20`
Expected: すべて PASS。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/externalUrl.ts frontend/src/lib/externalUrl.test.ts frontend/src/lib/searchEngine.ts frontend/src/lib/searchEngine.test.ts
git commit -m "fix(frontend): 外部サービスURLの検証と検索URLのフォールバックを追加 (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: 設定画面 — 保存前の検証、インポート由来の注意文、保存し直しによる信頼

**Files:**
- Modify: `frontend/src/ui/settings/ExternalIntegrationSection.svelte`
- Modify: `frontend/src/ui/settings/ExternalIntegrationSection.test.ts`

**Interfaces:**
- Consumes: `validateSearchEngineUrl` / `validateSummalyProxyUrl`(Task 8)、`commands.getSummalyProxyImported()` / `commands.trustSummalyProxy()`(Task 5で生成済み)。
- Produces: 画面の挙動。注意文の要素に `data-testid="summaly-imported-notice"`。

- [ ] **Step 1: 失敗するテストを書く**

`ExternalIntegrationSection.test.ts` の先頭の import に追加する。

```ts
import { fireEvent } from "@testing-library/svelte";
import { commands } from "../../lib/ipc";
```

ファイル末尾に追加する。

```ts
describe("ExternalIntegrationSection 外部サービスのURL", () => {
  const SEARCH_PLACEHOLDER = "検索URLテンプレート（{query} をクエリ文字列に置換）";
  const SUMMALY_PLACEHOLDER = "空欄なら接続先インスタンスの /url を使用";

  function mockImported(value: boolean) {
    return vi.spyOn(commands, "getSummalyProxyImported").mockResolvedValue({ status: "ok", data: value });
  }

  it("javascript: の検索URLは保存できず、エラーが出る", async () => {
    mockImported(false);
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByPlaceholderText, getByText } = render(ExternalIntegrationSection);

    await fireEvent.input(getByPlaceholderText(SEARCH_PLACEHOLDER), { target: { value: "javascript:alert({query})" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(getByText(/検索エンジンのURL/)).toBeTruthy());
    expect(spy).not.toHaveBeenCalled();
  });

  it("http/https 以外のURLプレビュー取得先は保存できず、エラーが出る", async () => {
    mockImported(false);
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByPlaceholderText, getByText } = render(ExternalIntegrationSection);

    await fireEvent.input(getByPlaceholderText(SUMMALY_PLACEHOLDER), { target: { value: "ftp://example.com/url" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(getByText(/URLプレビューの取得先/)).toBeTruthy());
    expect(spy).not.toHaveBeenCalled();
  });

  it("手入力のLANの取得先は保存できる", async () => {
    mockImported(false);
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByPlaceholderText, getByText } = render(ExternalIntegrationSection);

    await fireEvent.input(getByPlaceholderText(SUMMALY_PLACEHOLDER), { target: { value: "http://192.168.1.10:3030/url" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy.mock.calls[0][0].summalyProxyUrl).toBe("http://192.168.1.10:3030/url");
  });

  it("インポート由来の取得先では注意文が出て、保存すると信頼のコマンドが呼ばれて注意文が消える", async () => {
    mockImported(true);
    vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const trust = vi.spyOn(commands, "trustSummalyProxy").mockResolvedValue({ status: "ok", data: null });
    const { getByTestId, queryByTestId, getByText } = render(ExternalIntegrationSection);

    await vi.waitFor(() => expect(getByTestId("summaly-imported-notice")).toBeTruthy());
    getByText("保存").click();

    await vi.waitFor(() => expect(trust).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(queryByTestId("summaly-imported-notice")).toBeNull());
  });

  it("インポート由来でなければ注意文は出ず、信頼のコマンドも呼ばれない", async () => {
    mockImported(false);
    vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const trust = vi.spyOn(commands, "trustSummalyProxy").mockResolvedValue({ status: "ok", data: null });
    const { queryByTestId, getByText } = render(ExternalIntegrationSection);

    getByText("保存").click();

    await vi.waitFor(() => expect(getByText("保存しました")).toBeTruthy());
    expect(queryByTestId("summaly-imported-notice")).toBeNull();
    expect(trust).not.toHaveBeenCalled();
  });

  it("検証に落ちて保存しなかったときは、信頼のコマンドを呼ばない", async () => {
    mockImported(true);
    vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const trust = vi.spyOn(commands, "trustSummalyProxy").mockResolvedValue({ status: "ok", data: null });
    const { getByPlaceholderText, getByTestId, getByText } = render(ExternalIntegrationSection);

    await vi.waitFor(() => expect(getByTestId("summaly-imported-notice")).toBeTruthy());
    await fireEvent.input(getByPlaceholderText(SUMMALY_PLACEHOLDER), { target: { value: "ftp://example.com/url" } });
    getByText("保存").click();

    await vi.waitFor(() => expect(getByText(/URLプレビューの取得先/)).toBeTruthy());
    expect(trust).not.toHaveBeenCalled();
  });
});
```

`SaveBar.svelte` は保存成功時に「保存しました」を出す(確認済み)。

- [ ] **Step 2: 失敗を確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/ExternalIntegrationSection.test.ts 2>&1 | tail -30`
Expected: 新しいテストが FAIL(検証がない、注意文がない)。

- [ ] **Step 3: 実装する**

`ExternalIntegrationSection.svelte` の `<script>` を次のように変える。import に追加:

```ts
  import { onMount } from "svelte";
  import { commands, unwrap } from "../../lib/ipc";
  import { validateSearchEngineUrl, validateSummalyProxyUrl } from "../../lib/externalUrl";
```

`let saved = $state(false);` の直後に追加:

```ts
  // 取得先が設定ファイルのインポートで入った値のままか。真のとき注意文を出し、保存すると利用者の設定として扱う。
  let summalyImported = $state(false);
  onMount(async () => {
    try {
      summalyImported = (await unwrap(commands.getSummalyProxyImported())) === true;
    } catch {
      summalyImported = false;
    }
  });
```

`save()` を次に置き換える。

```ts
  async function save() {
    err = null;
    saved = false;
    const searchUrl = searchEngineUrl.trim() || DEFAULT_SEARCH_ENGINE_URL;
    const proxyUrl = summalyProxyUrl.trim();
    const searchErr = validateSearchEngineUrl(searchUrl);
    if (searchErr) {
      err = `検索エンジンのURL: ${searchErr}`;
      return;
    }
    const proxyErr = validateSummalyProxyUrl(proxyUrl);
    if (proxyErr) {
      err = `URLプレビューの取得先: ${proxyErr}`;
      return;
    }
    busy = true;
    try {
      // このセクションが編集しないフィールド(外観・データ等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        searchEngineUrl: searchUrl,
        urlPreviewEnabled,
        summalyProxyUrl: proxyUrl,
        translateTargetLang: normalizeTranslateLang(translateTargetLang),
      });
      // インポート由来の取得先は、この保存で利用者が設定した値として扱う。
      if (summalyImported) {
        await unwrap(commands.trustSummalyProxy());
        summalyImported = false;
      }
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
```

マークアップ: 取得先の入力欄の下の説明 `<p>`(「設定すると、リンクプレビュー対象のURLは…」)の直後に追加する。

```svelte
  {#if summalyImported}
    <p class="mb-0 mt-0 text-xs text-destructive" data-testid="summaly-imported-notice">
      この取得先はインポートした値です。プライベートアドレス(LANやループバック)には接続できません。
      保存すると自分で設定した値として扱い、そうしたアドレスにも接続します。
      心当たりのない値なら保存せずに変更してください。
    </p>
  {/if}
```

検索エンジンURLの説明 `<p>`(「空欄や{query}を含まない値を保存した場合は…」)を次に置き換える。

```svelte
  <p class="mb-4 mt-0 text-xs text-muted-foreground">
    プレースホルダ<code class="mfm-code">{"{query}"}</code>を含むURLを指定すると好きな検索エンジンを使えます。
    <code class="mfm-code">http</code>か<code class="mfm-code">https</code>のURLのみ指定できます。
    空欄のまま保存した場合はGoogle検索に戻ります。
  </p>
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/ui/settings/ExternalIntegrationSection.test.ts 2>&1 | tail -30 && pnpm check 2>&1 | tail -10`
Expected: すべて PASS、`pnpm check` はエラーなし。

- [ ] **Step 5: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/settings
git commit -m "fix(settings): 外部連携の保存前検証とインポート由来の取得先の表示を追加 (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: ドキュメント

**Files:**
- Modify: `docs/guide/user-guide.md`(246行目付近の「外部連携」、263行目付近のインポートの注意書き)
- Modify: `CLAUDE.md`(`net_guard.rs` の項)
- Modify: `docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md`(101行目)

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md` の「外部連携」の項目(`- **外部連携**: …`)の直後に、同じ字下げの箇条書きで追加する。

```markdown
  - 検索エンジンのURLとURLプレビューの取得先に指定できるのは、`http` か `https` のURLだけです。`javascript:` や `data:` などは保存できません。
  - 取得先に、LANやループバック(自宅サーバーなど)のプロキシを手入力して保存すると、そのまま使えます。
```

インポートの注意書き(`- 共有されたファイルには、カスタムCSSなど…`)の直後に追加する。

```markdown
- 検索エンジンやURLプレビューのURLが `http`/`https` の形式でないファイルは、インポートできません。インポートで入ったURLプレビューの取得先は、LANやループバックなどのプライベートアドレスには接続できません。そうしたプロキシを使うときは、設定の「外部連携」を開くと注意文が出るので、内容を確かめて保存し直してください(保存し直すと、自分で設定した値として扱われます)。
```

- [ ] **Step 2: `CLAUDE.md` の `net_guard.rs` の項を更新する**

`net_guard.rs` の項の末尾(`AppState.http` (API traffic) is outside this guard.` の前)に追加する。

```
`fetch_url_preview` (URL previews, Issue #549) also goes through `fetch_http`: the user-saved summaly proxy host is added to `fetch_allowlist` (via `SettingsStore::trusted_summaly_allowlist_entry`), but only while `SettingsData.summaly_proxy_imported` is false — a proxy that arrived via settings import stays blocked from private addresses until the user re-saves it in Settings → External integration (`trust_summaly_proxy`).
```

- [ ] **Step 3: 544の仕様書を更新する**

`docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md` の101行目の「取り込み自体は拒否しない。」を次に置き換える。

```
形式が不正な値(`http`/`https` 以外のスキームなど)を含むファイルは取り込みを拒否する(Issue #549)。形式が正しければ取り込み自体は拒否しない。インポートした取得先は、設定画面で保存し直すまでプライベートアドレスに接続できない。
```

- [ ] **Step 4: コミットする**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add docs/guide/user-guide.md CLAUDE.md docs/superpowers/specs/2026-11-02-issue-544-settings-import-export-design.md
git commit -m "docs: 外部サービスURLの検証と取得先の扱いを追記 (#549)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: 全テストと実機確認

**Files:** なし(確認のみ。確認用の一時ファイルは `/home/onodai145/.claude/jobs/20a18c17/tmp` に置く)。

- [ ] **Step 1: 全テストを走らせる**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test 2>&1 | tail -15
cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -10 && pnpm test 2>&1 | tail -15
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo clippy --all-targets 2>&1 | tail -15
```

Expected: すべて PASS、clippy の警告なし。失敗したら、原因を特定して直してから先へ進む。`git status --short` で `tauri.gen.ts` に意図しない差分がないことも確認する。

- [ ] **Step 2: 隔離インスタンスを起動する**

Task 1 の Step 2 と同じ手順(Xvfb、`WAYLAND_DISPLAY` を unset、`dbus-run-session`、`/tmp/t549` の短いXDGパス)。以前の `/tmp/t549` が残っていたら、中身を消してから使う。

- [ ] **Step 3: ブリッジで次を確認する**

ブリッジの本体は同期関数で `return` が必要。`invoke` の結果は Promise なので、kick-and-poll 方式(結果を `window.__r` に入れ、次の呼び出しで読む。`debug-bridge-does-not-support-await`)で扱う。

1. `invoke('set_ui_prefs', { prefs: {…現在の値, searchEngineUrl: 'javascript:alert({query})'} })` がエラーになる。`summalyProxyUrl: 'ftp://x'` も同様。
2. 手元でJSONを返す簡易サーバー(`python3 -m http.server` ではなく、`{"title":"T"}` を返す数行のスクリプトを `$CLAUDE_JOB_DIR/tmp` に書いて `127.0.0.1:PORT` で待ち受ける)を立て、`set_ui_prefs` で `summalyProxyUrl: 'http://127.0.0.1:PORT/url'` を保存 → `invoke('fetch_url_preview', { accountId: 'x', url: 'https://example.com/' })` が `title: 'T'` を返す。
3. エクスポートしたJSON(`export_settings`)の `ui.summalyProxyUrl` を同じ `http://127.0.0.1:PORT/url` にした別ファイルを作り、いったん取得先を空にしてから `import_settings` → `get_summaly_proxy_imported` が `true`、`fetch_url_preview` が「許可されていません」で失敗し、簡易サーバーに届いたリクエストが0件。
4. `trust_summaly_proxy` → `get_summaly_proxy_imported` が `false`、`fetch_url_preview` が成功。
5. `javascript:` を含む設定ファイルをインポート → 拒否される(エラーに「検索エンジンのURL」)。

- [ ] **Step 4: 設定画面の見た目を実描画で確認する**

Xvfb 上で「設定 → 外部連携」を開き、(a) インポート直後(手順3の状態)に注意文が出る、(b) 不正な値を入れて保存するとエラーが出る、を `import`(ImageMagick)かスクリーンショット機能で撮って、自分で目視する(レビュー承認とは別に、実画面の確認は省略しない)。

- [ ] **Step 5: 後始末**

自分が起動した Xvfb / `cargo tauri dev` / 簡易サーバーを、正確なPIDでkillする。`/tmp/t549` を削除する。`ps aux | grep -E "t549|Xvfb :99"` で残りがないことを確認する。

---

### Task 12: 仕上げ

- [ ] **Step 1: 差分を確認する**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git log --oneline main..HEAD
git diff --stat main..HEAD
```

- [ ] **Step 2: `superpowers:finishing-a-development-branch` を使って、統合方法をユーザーに確認する**

pushとPRはユーザーが頼んだときだけ行う。PRを作る場合は `.github/pull_request_template.md` の構成に手で合わせ、本文に `Fixes #549` を入れ、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。マージは通常のマージコミット(`gh pr merge --merge`)で、squashにしない。pushの後にCIをMonitorで待たない。
