# 時限ミュート Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ProfileModal の「…」メニューから、ユーザーを期間指定でサーバーミュート(`mute/create` + `expiresAt`)・解除でき、期限切れが tsumugi 内のミュート集合にも反映される。

**Architecture:** Rust 側に `MutePeriod` と `mute_user` / `unmute_user` コマンドを追加し、操作後は既存の `sync_server_mutes_core` でミュート集合を丸ごと再取得する。同期は `mute/list` の `expiresAt` を読んで期限切れを除外し、最短の期限を返す。フロントはその期限に1回だけ再同期するタイマーを張り、ProfileModal に新設の `UserMenu` から操作する。

**Tech Stack:** Rust(Tauri v2 / tauri-specta / chrono / wiremock)、Svelte 5 + Vitest + @testing-library/svelte。

**Spec:** `docs/superpowers/specs/2026-11-07-timed-mute-design.md`

## Global Constraints

- `expiresAt` は **ミリ秒**の Unix 時刻、`null` は無期限(Misskey `mute/create`)。
- 現在時刻 + 期間は **Rust 側**で計算する(フロントの時計に依存しない)。
- 期間は `indefinitely` / `tenMinutes` / `oneHour` / `oneDay` / `oneWeek` の5つ(`serde(rename_all = "camelCase")`)。
- ミュート作成・解除の後は、集合への差分追加ではなく `sync_server_mutes_core` で**丸ごと再取得**する(#452 / #454 / #456 の境界・ロックの前提)。
- 同期が失敗しても、作成・解除自体は成功として返す(同期失敗はログのみ)。
- `mute/list` の `expiresAt <= 現在時刻` のエントリはクライアント側で除外する。`expiresAt` が無い・`null`・パース不能なら期限なし(有効)。
- フロントの再同期タイマーは、次の定期同期(6時間 = `SERVER_MUTE_SYNC_INTERVAL_MS`)より前に切れるものだけ張る。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集せず `cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- コミットは件名1行のみ(本文なし)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告し、再試行しない。
- 作業ブランチは `feat/issue-545-timed-mute`(作成済み)。`main` では編集しない。
- 見た目の値は `docs/design/style-guide.md` に従い、独自の値(`rounded-[Npx]` 等)を作らない。
- ユーザー向けの説明・コメントは日本語。
- `cargo tauri dev` など自分で起動したプロセスは、タスク完了前に正確な PID で止める(`pkill` / `killall` 禁止)。実 UI の確認は Xvfb 越しで、`WAYLAND_DISPLAY` も unset する。

## Review Focus

- ミュート作成/解除の直後に同期が失敗: 操作は成功扱いで、画面に出さずログのみ(Task 3 の `mute_user_core_succeeds_even_when_the_follow_up_sync_fails`)。
- 期限切れのミュートがサーバーの `mute/list` に残っている: 期限どおりに除外され、他の有効なミュートは残る(Task 2 / Task 3)。
- `expiresAt` が欠落・`null`・壊れた文字列の行: 取りこぼさず有効(無期限)として扱う(Task 2)。
- ミュート中に別ユーザーのプロフィールへ切り替わる(`target` 変更): 古いリクエストの結果で新しいプロフィールの状態を書き換えない(Task 7)。
- タイマー発火時にそのアカウントが既に削除されている: 同期を呼ばず、警告ログも出さない(Task 6)。
- 自分自身、または `isMuted` を取得できないプロフィールでは「…」メニューを出さない(Task 7)。

---

### Task 1: `MutePeriod`(期間 → 期限ミリ秒)

**Files:**
- Modify: `src-tauri/src/domain/mute.rs`
- Modify: `src-tauri/src/domain/mod.rs:37`

**Interfaces:**
- Produces: `crate::domain::MutePeriod`(`Copy`、`specta::Type`)と `MutePeriod::expires_at_ms(self, now_ms: i64) -> Option<i64>`。Task 3 が使う。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/mute.rs` の末尾に追記:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn indefinitely_has_no_expiry() {
        assert_eq!(MutePeriod::Indefinitely.expires_at_ms(NOW), None);
    }

    #[test]
    fn each_period_adds_its_length_in_milliseconds() {
        assert_eq!(MutePeriod::TenMinutes.expires_at_ms(NOW), Some(NOW + 10 * 60 * 1000));
        assert_eq!(MutePeriod::OneHour.expires_at_ms(NOW), Some(NOW + 60 * 60 * 1000));
        assert_eq!(MutePeriod::OneDay.expires_at_ms(NOW), Some(NOW + 24 * 60 * 60 * 1000));
        assert_eq!(MutePeriod::OneWeek.expires_at_ms(NOW), Some(NOW + 7 * 24 * 60 * 60 * 1000));
    }

    #[test]
    fn serializes_as_camel_case_matching_the_frontend_values() {
        assert_eq!(serde_json::to_string(&MutePeriod::TenMinutes).unwrap(), "\"tenMinutes\"");
        assert_eq!(serde_json::to_string(&MutePeriod::Indefinitely).unwrap(), "\"indefinitely\"");
        assert_eq!(serde_json::from_str::<MutePeriod>("\"oneWeek\"").unwrap(), MutePeriod::OneWeek);
    }
}
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib domain::mute`
Expected: FAIL(`MutePeriod` が未定義でコンパイルエラー)

- [ ] **Step 3: 実装する**

`src-tauri/src/domain/mute.rs` の `MuteConfig` の定義の後(テストモジュールの前)に追記:

```rust
/// ユーザーミュートの期間(Misskey Web の選択肢に合わせる)。Misskey に送る期限(`expiresAt`)は
/// この値から Rust 側で計算する。フロントの時計に依存せず、過去の `expiresAt`(サーバーは黙って無視する)
/// を送らないため。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MutePeriod {
    Indefinitely,
    TenMinutes,
    OneHour,
    OneDay,
    OneWeek,
}

impl MutePeriod {
    /// `now_ms`(エポックミリ秒)からの期限。無期限は `None`(Misskey では `expiresAt: null`)。
    pub fn expires_at_ms(self, now_ms: i64) -> Option<i64> {
        const MINUTE: i64 = 60 * 1000;
        let length = match self {
            Self::Indefinitely => return None,
            Self::TenMinutes => 10 * MINUTE,
            Self::OneHour => 60 * MINUTE,
            Self::OneDay => 24 * 60 * MINUTE,
            Self::OneWeek => 7 * 24 * 60 * MINUTE,
        };
        Some(now_ms + length)
    }
}
```

`src-tauri/src/domain/mod.rs` の 37 行目を変更:

```rust
pub use mute::{MuteConfig, MutePeriod};
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd src-tauri && cargo test --lib domain::mute`
Expected: PASS(3 tests)

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/domain/mute.rs src-tauri/src/domain/mod.rs
git commit -m "feat(mute): ユーザーミュートの期間 MutePeriod を追加 (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: API ラッパと `mute/list` の期限処理

**Files:**
- Modify: `src-tauri/src/api/mutes.rs`(`fetch_muted_and_blocked` / `collect` の差し替え、`mute_user` / `unmute_user` の追加、テスト追記)

**Interfaces:**
- Consumes: なし(Task 1 とは独立)。
- Produces:
  - `pub struct ServerMutes { pub ids: HashSet<String>, pub next_expiry_at_ms: Option<i64> }`
  - `pub async fn fetch_muted_and_blocked(client: &MisskeyClient, now_ms: i64) -> Result<ServerMutes>`(戻り値と引数が変わる)
  - `pub async fn mute_user(client: &MisskeyClient, user_id: &str, expires_at_ms: Option<i64>) -> Result<()>`
  - `pub async fn unmute_user(client: &MisskeyClient, user_id: &str) -> Result<()>`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/mutes.rs` の `mod tests` の末尾(最後の `}` の直前)に追記:

```rust
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
        // 22:30:00Z = 1_700_001_800_000
        assert_eq!(got.next_expiry_at_ms, Some(1_700_001_800_000));
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
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib api::mutes`
Expected: FAIL(`read_row` / `mute_user` / `unmute_user` が未定義、`fetch_muted_and_blocked` の引数不一致でコンパイルエラー)

- [ ] **Step 3: 実装する**

`src-tauri/src/api/mutes.rs` の冒頭 `use serde_json::json;` を `use serde_json::{json, Value};` に変え、`fetch_muted_and_blocked` と `collect`(現状の 14〜56 行目付近)を次で置き換える:

```rust
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
```

(元の `collect` の中で `Vec<serde_json::Value>` と書かれていた箇所は上の版で `Vec<Value>` になっている。)

- [ ] **Step 4: api::mutes のテストが通ることを確認する**

`commands/mute.rs` は次の Task 3 で直すため、この時点ではクレート全体がコンパイルエラーになる。先に最小の呼び出し側修正だけ入れる。`src-tauri/src/commands/mute.rs` の `sync_server_mutes_core` 内:

```rust
    let ids = fetch_muted_and_blocked(client).await?;
```
を次に置き換える(Task 3 で本実装に直す暫定):
```rust
    let ids = fetch_muted_and_blocked(client, 0).await?.ids;
```

Run: `cd src-tauri && cargo test --lib api::mutes`
Expected: PASS(新規 12 件 + 既存)

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/api/mutes.rs src-tauri/src/commands/mute.rs
git commit -m "feat(mute): mute/createのAPIラッパと期限切れミュートの除外を追加 (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: コマンド `mute_user` / `unmute_user` と同期結果の期限

**Files:**
- Modify: `src-tauri/src/commands/mute.rs`(import、`SyncMuteResult`、`sync_server_mutes_core`、新コマンド、テスト)
- Modify: `src-tauri/src/lib.rs:134` 付近(コマンド登録)
- Modify: `docs/superpowers/specs/2026-11-07-timed-mute-design.md`(コマンドの置き場所と戻り値の記述)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: Task 1 の `MutePeriod::expires_at_ms`、Task 2 の `ServerMutes` / `fetch_muted_and_blocked(client, now_ms)` / `mute_user` / `unmute_user`。
- Produces:
  - `SyncMuteResult { blocked_users: u32, word_rules: u32, next_expiry_at: Option<i64> }`(TS: `nextExpiryAt: number | null`)。
  - コマンド `mute_user(account_id: String, user_id: String, period: MutePeriod) -> Result<Option<SyncMuteResult>>`(TS: `commands.muteUser(accountId, userId, period)`)。操作後の同期が失敗したときは `None`。
  - コマンド `unmute_user(account_id: String, user_id: String) -> Result<Option<SyncMuteResult>>`(TS: `commands.unmuteUser(accountId, userId)`)。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/mute.rs` の `mod tests` の末尾に追記(`mount_server_mutes` などは同モジュールの既存ヘルパ):

```rust
    // --- 時限ミュート(Issue #545) ---

    /// 2099-01-01T00:00:00Z のエポックミリ秒(実行時刻に依存せず「未来」になる)。
    const FAR_FUTURE_MS: i64 = 4_070_908_800_000;

    /// `mute/list` を、期限付きの1件だけ返すモックにする。他は `mount_server_mutes` と同じ。
    async fn mount_mute_list_with_expiry(mock: &MockServer, user: &str, expires_at: &str) {
        mock.reset().await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": "r1", "muteeId": user, "expiresAt": expires_at },
            ])))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/blocking/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/i"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "mutedWords": [] })))
            .mount(mock)
            .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_reports_the_earliest_expiry_and_hides_expired_mutes() {
        let mock = MockServer::start().await;
        mount_mute_list_with_expiry(&mock, "u1", "2099-01-01T00:00:00.000Z").await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let result = sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert_eq!(result.next_expiry_at, Some(FAR_FUTURE_MS));
        assert!(state.is_server_muted("acc1", "u1"));

        // 期限が過ぎたミュートは、サーバーの mute/list に残っていても、集合から外れる
        mount_mute_list_with_expiry(&mock, "u1", "2000-01-01T00:00:00.000Z").await;
        let result = sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert_eq!(result.next_expiry_at, None);
        assert!(!state.is_server_muted("acc1", "u1"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_clears_boundaries_when_a_timed_mute_expires() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = state_with_three_columns_and_boundaries().await;
        mount_mute_list_with_expiry(&mock, "u1", "2099-01-01T00:00:00.000Z").await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        mount_mute_list_with_expiry(&mock, "u1", "2000-01-01T00:00:00.000Z").await; // 期限が過ぎた=解除
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();

        assert!(boundaries_of(&state, "c1").await.is_empty());
        assert_eq!(boundaries_of(&state, "c3").await, kept(), "他のアカウントのカラムは、そのまま");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mute_user_core_creates_the_mute_with_an_expiry_and_resyncs() {
        let mock = MockServer::start().await;
        mount_server_mutes(&mock, &[], serde_json::json!([])).await;
        Mock::given(method("POST"))
            .and(path("/mute/create"))
            .and(wiremock::matchers::body_partial_json(
                serde_json::json!({ "userId": "u1", "expiresAt": 1_700_003_600_000_i64 }),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let result = mute_user_core(&state, "acc1", &client, "u1", MutePeriod::OneHour, 1_700_000_000_000)
            .await
            .unwrap();

        assert!(result.is_some(), "同期が成功したので、結果を返す");
        // 再同期で mute/list を読んだ(モックは mount_server_mutes のもの。期限なしの空)
        assert!(!state.is_server_muted("acc1", "u1"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mute_user_core_applies_the_new_mute_to_the_server_mute_set() {
        let mock = MockServer::start().await;
        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await; // 作成後の mute/list は u1 を含む
        Mock::given(method("POST"))
            .and(path("/mute/create"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock)
            .await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        mute_user_core(&state, "acc1", &client, "u1", MutePeriod::Indefinitely, 0).await.unwrap();

        assert!(state.is_server_muted("acc1", "u1"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mute_user_core_fails_without_syncing_when_the_create_fails() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mute/create"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": { "code": "ALREADY_MUTING", "message": "You are already muting that user.", "id": "x" }
            })))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .expect(0)
            .mount(&mock)
            .await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let result = mute_user_core(&state, "acc1", &client, "u1", MutePeriod::OneDay, 0).await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mute_user_core_succeeds_even_when_the_follow_up_sync_fails() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mute/create"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/mute/list"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock)
            .await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());

        let result = mute_user_core(&state, "acc1", &client, "u1", MutePeriod::OneDay, 0).await;

        assert!(matches!(result, Ok(None)), "作成は成功。同期の失敗は None で伝える: {result:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unmute_user_core_deletes_the_mute_and_resyncs() {
        let mock = MockServer::start().await;
        let client = MisskeyClient::new_with_api_base(reqwest::Client::new(), mock.uri(), None);
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        mount_server_mutes(&mock, &["u1"], serde_json::json!([])).await;
        sync_server_mutes_core(&state, "acc1", &client).await.unwrap();
        assert!(state.is_server_muted("acc1", "u1"));

        mount_server_mutes(&mock, &[], serde_json::json!([])).await; // 解除後は空
        Mock::given(method("POST"))
            .and(path("/mute/delete"))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({ "userId": "u1" })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock)
            .await;

        let result = unmute_user_core(&state, "acc1", &client, "u1").await.unwrap();

        assert!(result.is_some());
        assert!(!state.is_server_muted("acc1", "u1"));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib commands::mute`
Expected: FAIL(`next_expiry_at` / `mute_user_core` / `unmute_user_core` / `MutePeriod` が未解決でコンパイルエラー)

- [ ] **Step 3: 実装する**

`src-tauri/src/commands/mute.rs`:

1. import(3〜4 行目)を置き換える:

```rust
use crate::api::mutes::{self as mutes_api, fetch_muted_and_blocked, fetch_muted_words};
use crate::domain::{BackgroundKind, MuteConfig, MutePeriod, NotifyConfig, UiPrefs};
```

2. `SyncMuteResult` に1項目を足す:

```rust
pub struct SyncMuteResult {
    pub blocked_users: u32,
    pub word_rules: u32,
    /// 有効なミュートのうち最も早い期限(エポックミリ秒)。期限付きのミュートが無ければ `None`。
    /// フロントはこの時刻に1回だけ再同期して、期限切れを反映する(Issue #545)。
    pub next_expiry_at: Option<i64>,
}
```

3. `sync_server_mutes_core` の該当部分(`let ids = ...` から `state.set_server_mutes(...)` まで)を置き換える。Task 2 の暫定行はここで消える:

```rust
    let server = fetch_muted_and_blocked(client, now_ms()).await?;
    let words = fetch_muted_words(client).await?;
    let result = SyncMuteResult {
        blocked_users: server.ids.len() as u32,
        word_rules: words.rules.len() as u32,
        next_expiry_at: server.next_expiry_at_ms,
    };
    let snapshot = ServerMuteSnapshot::new(server.ids.iter().cloned(), words.keys);
    // メモリ上の集合の差し替えは、境界を捨てる前に行う。書き込み側は、ミュート設定を読む前に世代を
    // 控えるので、境界の世代が進んだ時点で、新しい集合がすでに反映されている(Issue #454, #452)。
    state.set_server_mutes(account_id, server.ids);
```
(続く `state.set_server_word_mutes(...)` 以降はそのまま。)

4. `sync_server_mutes_core` の直前(コマンド `sync_server_mutes` の後)に、新コマンドとコアを追加:

```rust
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// ユーザーをミュートする(期間指定。Issue #545)。成功したら、ミュート集合を再取得して反映する。
/// 戻り値は再取得の結果。再取得に失敗したときは `None`(ミュート自体は成功しており、次回の同期で追いつく)。
#[tauri::command]
#[specta::specta]
pub async fn mute_user(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    period: MutePeriod,
) -> Result<Option<SyncMuteResult>> {
    let client = state.client_for(&account_id)?;
    mute_user_core(&state, &account_id, &client, &user_id, period, now_ms()).await
}

/// ユーザーのミュートを解除する。戻り値は `mute_user` と同じ。
#[tauri::command]
#[specta::specta]
pub async fn unmute_user(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<Option<SyncMuteResult>> {
    let client = state.client_for(&account_id)?;
    unmute_user_core(&state, &account_id, &client, &user_id).await
}

/// `mute_user` の中核。`AppState` / `client` / 現在時刻を引数で受け、`State` と keyring なしでテストできる
/// (`sync_server_mutes_core` と同じ狙い)。期限は `now_ms` + 期間をここで計算する。
async fn mute_user_core(
    state: &AppState,
    account_id: &str,
    client: &crate::api::MisskeyClient,
    user_id: &str,
    period: MutePeriod,
    now_ms: i64,
) -> Result<Option<SyncMuteResult>> {
    mutes_api::mute_user(client, user_id, period.expires_at_ms(now_ms)).await?;
    Ok(resync_after_mute_change(state, account_id, client).await)
}

async fn unmute_user_core(
    state: &AppState,
    account_id: &str,
    client: &crate::api::MisskeyClient,
    user_id: &str,
) -> Result<Option<SyncMuteResult>> {
    mutes_api::unmute_user(client, user_id).await?;
    Ok(resync_after_mute_change(state, account_id, client).await)
}

/// ミュートの作成・解除の後に、集合を丸ごと再取得する。差分で足さないのは、同期が持つ境界・ロックの
/// 前提(#452 / #454 / #456)にそのまま乗るため。失敗しても作成・解除は成功済みなので、ログだけ残す。
async fn resync_after_mute_change(
    state: &AppState,
    account_id: &str,
    client: &crate::api::MisskeyClient,
) -> Option<SyncMuteResult> {
    match sync_server_mutes_core(state, account_id, client).await {
        Ok(result) => Some(result),
        Err(e) => {
            log::warn!("failed to sync server mutes after a mute change ({account_id}): {e}");
            None
        }
    }
}
```

`src-tauri/src/lib.rs` の 134 行目(`commands::mute::sync_server_mutes,`)の直後に追加:

```rust
            commands::mute::mute_user,
            commands::mute::unmute_user,
```

- [ ] **Step 4: 通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib commands::mute`
Expected: PASS(新規 7 件 + 既存)

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`git diff --stat frontend/src/bindings/tauri.gen.ts` で、`MutePeriod`、`SyncMuteResult.nextExpiryAt`、`muteUser` / `unmuteUser` が増えていること。

Run: `cd src-tauri && cargo test`
Expected: PASS(`#[ignore]` は除く)

- [ ] **Step 5: spec を実装に合わせる**

`docs/superpowers/specs/2026-11-07-timed-mute-design.md` の「コマンド」の項を次に書き換える(置き場所と戻り値が、実装のために変わったため):

```markdown
3. **コマンド** `commands/mute.rs`(`sync_server_mutes_core` が同ファイルの非公開関数のため、ここに置く): `mute_user(account_id, user_id, period: MutePeriod)` と `unmute_user(account_id, user_id)`。`specta_builder()` に登録する。戻り値は `Option<SyncMuteResult>` で、操作後の再同期の結果(再同期に失敗したときは `None`)。フロントはこの `nextExpiryAt` で期限切れの再同期タイマーを張る。
```

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/mute.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts docs/superpowers/specs/2026-11-07-timed-mute-design.md
git commit -m "feat(mute): mute_user/unmute_userコマンドと同期結果の次の期限を追加 (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `UserProfile.isMuted` と MiAuth 権限 `write:mutes`

**Files:**
- Modify: `src-tauri/src/api/users.rs:38-43`(`RawUserShow`)とテスト
- Modify: `src-tauri/src/commands/user.rs:10-17, 40-55`(`UserProfile`、`get_user_profile`)とテスト
- Modify: `src-tauri/src/session/miauth.rs:10-25`(`PERMISSIONS`)とテスト
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Produces: `UserProfile.is_muted: Option<bool>`(TS: `isMuted: boolean | null`、自分自身は `None`)。Task 7 が使う。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/users.rs` の `mod tests` の末尾に追記:

```rust
    #[test]
    fn raw_user_show_reads_is_muted() {
        let raw: RawUserShow = serde_json::from_value(serde_json::json!({
            "id": "u1", "username": "alice", "host": null, "isMuted": true, "isFollowing": false
        }))
        .unwrap();
        assert_eq!(raw.is_muted, Some(true));
    }

    #[test]
    fn raw_user_show_without_is_muted_is_none() {
        let raw: RawUserShow = serde_json::from_value(serde_json::json!({
            "id": "u1", "username": "alice", "host": null
        }))
        .unwrap();
        assert_eq!(raw.is_muted, None);
    }
```

`src-tauri/src/commands/user.rs` の `mod tests` の末尾に追記:

```rust
    #[test]
    fn relation_flag_is_hidden_for_the_own_profile() {
        assert_eq!(relation_flag(true, Some(true)), None);
        assert_eq!(relation_flag(false, Some(true)), Some(true));
        assert_eq!(relation_flag(false, Some(false)), Some(false));
        assert_eq!(relation_flag(false, None), None);
    }
```

`src-tauri/src/session/miauth.rs` の `mod tests` の末尾に追記:

```rust
    #[test]
    fn requests_write_mutes_for_timed_mutes() {
        assert!(PERMISSIONS.contains(&"write:mutes"));
        let u = build_miauth_url("misskey.io", "11111111-2222-3333-4444-555555555555");
        assert!(u.contains("write%3Amutes") || u.contains("write:mutes"));
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib -- raw_user_show relation_flag requests_write_mutes`
Expected: FAIL(`is_muted` / `relation_flag` が未定義、`write:mutes` が無い)

- [ ] **Step 3: 実装する**

`src-tauri/src/api/users.rs` の `RawUserShow` に追加(`is_following` の下):

```rust
    #[serde(default)]
    pub is_muted: Option<bool>,
```

`src-tauri/src/commands/user.rs`:

```rust
/// プロフィールモーダル用のレスポンス。`is_following` / `is_muted` は自分自身の場合 `None`。
#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user: User,
    pub is_following: Option<bool>,
    pub is_muted: Option<bool>,
    pub is_self: bool,
}

/// 自分自身のプロフィールでは、関係フラグ(フォロー・ミュート)を出さない(純粋関数、テスト用に分離)。
pub(crate) fn relation_flag(is_self: bool, flag: Option<bool>) -> Option<bool> {
    if is_self {
        None
    } else {
        flag
    }
}
```

`get_user_profile` の返却部を置き換える:

```rust
    Ok(UserProfile {
        user: raw.user.into(),
        is_following: relation_flag(is_self, raw.is_following),
        is_muted: relation_flag(is_self, raw.is_muted),
        is_self,
    })
```

`src-tauri/src/session/miauth.rs` の `PERMISSIONS` の `"read:mutes",` の次の行に `"write:mutes",` を追加する(他の `write:` と並べるなら `"write:following",` の下でもよいが、ここでは `read:mutes` の直後)。

- [ ] **Step 4: 通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib -- raw_user_show relation_flag requests_write_mutes`
Expected: PASS

Run: `cd src-tauri && cargo test`
Expected: PASS(`generates_frontend_bindings` が `tauri.gen.ts` を更新し、`UserProfile.isMuted` が出る)

他に権限の一覧を持つ場所が無いことを確認する:

Run: `grep -rn "read:mutes" --exclude-dir=node_modules --exclude-dir=target --exclude-dir=.git --exclude-dir=openapi . | grep -v "docs/superpowers"`
Expected: `src-tauri/src/session/miauth.rs` のみ。他にも権限リストがあれば(e2e のトークン発行スクリプト等)、`write:mutes` を足す。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/api/users.rs src-tauri/src/commands/user.rs src-tauri/src/session/miauth.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat(mute): UserProfileにisMutedを追加しMiAuth権限write:mutesを要求する (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: フロントエンドのストア(`muteUser` / `unmuteUser` と期限切れの再同期タイマー)

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(定数、フィールド、`teardown`、`#syncServerMutes`、新メソッド)
- Modify: `frontend/src/lib/store.svelte.test.ts`(`syncResult` の型、テスト追加。1360 行目付近の describe)

**Interfaces:**
- Consumes: Task 3/4 で再生成された `commands.muteUser(accountId, userId, period)` → `SyncMuteResult | null`、`commands.unmuteUser(accountId, userId)`、`SyncMuteResult.nextExpiryAt: number | null`、型 `MutePeriod`。
- Produces: `app.muteUser(accountId: string, userId: string, period: MutePeriod): Promise<void>` と `app.unmuteUser(accountId: string, userId: string): Promise<void>`(失敗は `#logFailure` に流して再 throw)。Task 6 が使う。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts` の、`SIX_HOURS` を使う describe(1360 行目付近)を直す。

(a) `syncResult` の型と初期値(1371・1374 行目)を置き換える:

```ts
  let syncResult: { blockedUsers: number; wordRules: number; nextExpiryAt: number | null } | Error;
  /// mute_user / unmute_user の戻り値(再同期の結果。同期に失敗したときは null)。
  let muteCommandResult: { blockedUsers: number; wordRules: number; nextExpiryAt: number | null } | null;
```
`beforeEach` の `syncResult = { blockedUsers: 3, wordRules: 1 };` を次に変え、直後に `muteCommandResult` も初期化する:
```ts
    syncResult = { blockedUsers: 3, wordRules: 1, nextExpiryAt: null };
    muteCommandResult = null;
```
`beforeEach` のモックに分岐を足す(`sync_server_mutes` の分岐の次):
```ts
      if (cmd === "mute_user" || cmd === "unmute_user") return muteCommandResult;
```

(b) 同じ describe の末尾(最後の `});` の直前)にテストを追加:

```ts
  describe("時限ミュートの期限切れ再同期(Issue #545)", () => {
    const callsOf = (cmd: string) => invokeMock.mock.calls.filter(([c]) => c === cmd);

    it("muteUserは、mute_userを呼ぶ", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];

      await app.muteUser("acc1", "u1", "oneHour");

      expect(callsOf("mute_user")).toHaveLength(1);
      expect(callsOf("mute_user")[0][1]).toEqual({ accountId: "acc1", userId: "u1", period: "oneHour" });
    });

    it("unmuteUserは、unmute_userを呼ぶ", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];

      await app.unmuteUser("acc1", "u1");

      expect(callsOf("unmute_user")[0][1]).toEqual({ accountId: "acc1", userId: "u1" });
    });

    it("次の期限が近ければ、その少し後に1回だけ再同期する", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + 60_000 };
      await app.muteUser("acc1", "u1", "tenMinutes");
      invokeMock.mockClear();

      await vi.advanceTimersByTimeAsync(59_000);
      expect(syncedAccounts()).toEqual([]); // まだ

      await vi.advanceTimersByTimeAsync(3_000);
      expect(syncedAccounts()).toEqual(["acc1"]); // 期限の後に1回
    });

    it("次の期限が6時間より先なら、タイマーは張らない(定期同期に任せる)", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + SIX_HOURS + 60_000 };
      await app.muteUser("acc1", "u1", "oneWeek");
      invokeMock.mockClear();

      await vi.advanceTimersByTimeAsync(SIX_HOURS - 1_000);

      expect(syncedAccounts()).toEqual([]);
    });

    it("期限がなければ(null)、前に張ったタイマーも消す", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + 60_000 };
      await app.muteUser("acc1", "u1", "tenMinutes");
      muteCommandResult = { blockedUsers: 0, wordRules: 0, nextExpiryAt: null }; // 解除した
      await app.unmuteUser("acc1", "u1");
      invokeMock.mockClear();

      await vi.advanceTimersByTimeAsync(120_000);

      expect(syncedAccounts()).toEqual([]);
    });

    it("同期に失敗した(null)ときは、タイマーを張らず、操作は成功として返す", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = null;

      await expect(app.muteUser("acc1", "u1", "oneDay")).resolves.toBeUndefined();
      invokeMock.mockClear();
      await vi.advanceTimersByTimeAsync(120_000);

      expect(syncedAccounts()).toEqual([]);
    });

    it("同じアカウントで続けて期限を更新しても、タイマーは1本(再同期は1回)", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + 30_000 };
      await app.muteUser("acc1", "u1", "tenMinutes");
      muteCommandResult = { blockedUsers: 2, wordRules: 0, nextExpiryAt: Date.now() + 30_000 };
      await app.muteUser("acc1", "u2", "tenMinutes");
      invokeMock.mockClear();

      await vi.advanceTimersByTimeAsync(40_000);

      expect(syncedAccounts()).toEqual(["acc1"]);
    });

    it("発火時にそのアカウントが削除されていたら、同期を呼ばない(警告も出さない)", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + 30_000 };
      await app.muteUser("acc1", "u1", "tenMinutes");
      app.accounts = []; // 期限の前に削除された
      invokeMock.mockClear();
      app.logs = [];

      await vi.advanceTimersByTimeAsync(40_000);

      expect(syncedAccounts()).toEqual([]);
      expect(app.logs.filter((l) => l.level === "warn")).toEqual([]);
    });

    it("teardownでタイマーを消す", async () => {
      vi.useFakeTimers();
      app.accounts = [account("acc1")];
      muteCommandResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + 30_000 };
      await app.muteUser("acc1", "u1", "tenMinutes");
      app.teardown();
      invokeMock.mockClear();

      await vi.advanceTimersByTimeAsync(40_000);

      expect(syncedAccounts()).toEqual([]);
    });

    it("定期同期の結果の次の期限でも、再同期のタイマーを張る(他クライアントで作った時限ミュート)", async () => {
      await bootAndForgetCalls();
      app.accounts = [account("acc1")];
      syncResult = { blockedUsers: 1, wordRules: 0, nextExpiryAt: Date.now() + SIX_HOURS + 30_000 };

      await vi.advanceTimersByTimeAsync(SIX_HOURS + 1_000); // 定期同期が1回走る
      expect(syncedAccounts()).toEqual(["acc1"]);

      await vi.advanceTimersByTimeAsync(40_000); // 期限(定期同期の30秒後)の後の再同期
      expect(syncedAccounts()).toEqual(["acc1", "acc1"]);
    });
  });
```

(c) 既存のこの describe 内で `syncResult = { blockedUsers: ..., wordRules: ... }` を直接代入している箇所があれば、`nextExpiryAt: null` を足す(`pnpm check` の型エラーで見つかる)。

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts`
Expected: FAIL(`app.muteUser` / `app.unmuteUser` が未定義)

- [ ] **Step 3: 実装する**

`frontend/src/lib/store.svelte.ts`:

1. `import` に `MutePeriod` を足す(`../bindings/tauri.gen` から型を import している既存の行に追加)。

2. 57 行目の `SERVER_MUTE_SYNC_INTERVAL_MS` の下に定数を足す:

```ts
/// 時限ミュートの期限の後、再同期するまでの余裕(ms)。サーバーとの時計のずれで、期限の直前に同期して
/// まだ有効に見えるのを避ける(Rust 側は `expiresAt <= 現在時刻` を期限切れとして除外する)。
const MUTE_EXPIRY_SYNC_SLACK_MS = 1_000;
/// 再同期までの待ちの下限(ms)。期限が過去・直近のとき、同期の連打にならないようにする。
const MUTE_EXPIRY_SYNC_MIN_DELAY_MS = 5_000;
```

3. 216 行目(`#serverMuteSyncTimer`)の下にフィールドを足す:

```ts
  /// アカウントごとの、時限ミュートの期限切れ再同期のタイマー(Issue #545)。
  #muteExpiryTimers = new Map<string, ReturnType<typeof setTimeout>>();
```

4. `teardown` の `#serverMuteSyncTimer` を片付けるブロックの直後に足す:

```ts
    for (const timer of this.#muteExpiryTimers.values()) clearTimeout(timer);
    this.#muteExpiryTimers.clear();
```

5. `#syncServerMutes` の `try` ブロックの末尾(ログ出力の後)に、結果を使うスケジュールを足す。`try { ... }` の中身を次にする:

```ts
      const result = await unwrapAcc(accountId, commands.syncServerMutes(accountId));
      if (!options.quiet && (result.blockedUsers > 0 || result.wordRules > 0)) {
        this.#log(
          "info",
          `サーバのミュート/ブロックを同期: ユーザ${result.blockedUsers}件・ワード${result.wordRules}件`,
        );
      }
      this.#scheduleMuteExpirySync(accountId, result.nextExpiryAt);
```

6. `#syncServerMutes` の直後に、スケジュールのメソッドを足す:

```ts
  /// 時限ミュートの最短の期限の後に、そのアカウントを1回だけ再同期するタイマーを張り直す(Issue #545)。
  /// 6時間以内に切れるものだけ張る(それより先は定期同期に任せる。長い setTimeout の上限も避けられる)。
  /// 発火時にそのアカウントが居なければ(削除された)、何もしない。
  #scheduleMuteExpirySync(accountId: string, nextExpiryAt: number | null) {
    const previous = this.#muteExpiryTimers.get(accountId);
    if (previous !== undefined) {
      clearTimeout(previous);
      this.#muteExpiryTimers.delete(accountId);
    }
    if (nextExpiryAt === null) return;
    const delay = nextExpiryAt - Date.now() + MUTE_EXPIRY_SYNC_SLACK_MS;
    if (delay >= SERVER_MUTE_SYNC_INTERVAL_MS) return;
    const timer = setTimeout(() => {
      this.#muteExpiryTimers.delete(accountId);
      if (!this.#loggedInAccounts().some((a) => a.id === accountId)) return;
      void this.#syncServerMutes(accountId, { quiet: true });
    }, Math.max(delay, MUTE_EXPIRY_SYNC_MIN_DELAY_MS));
    this.#muteExpiryTimers.set(accountId, timer);
  }
```

7. `unfollowUser` の直後に、公開メソッドを足す:

```ts
  /// ユーザーをミュートする(期間指定。Issue #545)。ミュート集合の再取得は Rust 側で済んでいる。
  /// 再取得の結果に含まれる次の期限で、期限切れの再同期タイマーを張る。
  async muteUser(accountId: string, userId: string, period: MutePeriod) {
    try {
      const result = await unwrapAcc(accountId, commands.muteUser(accountId, userId, period));
      if (result) this.#scheduleMuteExpirySync(accountId, result.nextExpiryAt);
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }

  /// ユーザーのミュートを解除する。`muteUser` と同様に、再取得の結果でタイマーを張り直す。
  async unmuteUser(accountId: string, userId: string) {
    try {
      const result = await unwrapAcc(accountId, commands.unmuteUser(accountId, userId));
      if (result) this.#scheduleMuteExpirySync(accountId, result.nextExpiryAt);
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts`
Expected: PASS(新規 10 件 + 既存)

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat(mute): ストアにmuteUser/unmuteUserと期限切れの再同期タイマーを追加 (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: `UserMenu.svelte`(期間サブメニューつきメニュー)

**Files:**
- Create: `frontend/src/ui/UserMenu.svelte`
- Create: `frontend/src/ui/UserMenu.test.ts`

**Interfaces:**
- Consumes: Task 3/4 の型 `MutePeriod`。
- Produces: `UserMenu` コンポーネント。props は `{ muted: boolean; onmute: (period: MutePeriod) => void; onunmute: () => void }`。表示だけを持ち、非同期処理とエラーは呼び出し側(Task 7)が持つ。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/UserMenu.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import UserMenu from "./UserMenu.svelte";

afterEach(() => cleanup());

describe("UserMenu", () => {
  it("未ミュートなら「ミュート」だけを出し、押すと期間の一覧に切り替わる", async () => {
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { muted: false, onmute: vi.fn(), onunmute: vi.fn() },
    });
    expect(queryByRole("button", { name: "ミュート解除" })).toBeNull();
    expect(queryByRole("button", { name: "1時間" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    for (const label of ["無期限", "10分", "1時間", "1日", "1週間"]) {
      expect(getByRole("button", { name: label })).toBeTruthy();
    }
  });

  it("期間を押すと、その期間でonmuteを呼ぶ", async () => {
    const onmute = vi.fn();
    const { getByRole } = render(UserMenu, { props: { muted: false, onmute, onunmute: vi.fn() } });
    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    await fireEvent.click(getByRole("button", { name: "1時間" }));

    expect(onmute).toHaveBeenCalledWith("oneHour");
  });

  it.each([
    ["無期限", "indefinitely"],
    ["10分", "tenMinutes"],
    ["1日", "oneDay"],
    ["1週間", "oneWeek"],
  ])("「%s」は %s を渡す", async (label, value) => {
    const onmute = vi.fn();
    const { getByRole } = render(UserMenu, { props: { muted: false, onmute, onunmute: vi.fn() } });
    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    await fireEvent.click(getByRole("button", { name: label }));

    expect(onmute).toHaveBeenCalledWith(value);
  });

  it("「戻る」で最初の表示に戻る", async () => {
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { muted: false, onmute: vi.fn(), onunmute: vi.fn() },
    });
    await fireEvent.click(getByRole("button", { name: "ミュート" }));

    await fireEvent.click(getByRole("button", { name: "戻る" }));

    expect(getByRole("button", { name: "ミュート" })).toBeTruthy();
    expect(queryByRole("button", { name: "1時間" })).toBeNull();
  });

  it("ミュート中は「ミュート解除」だけを出し、押すとonunmuteを呼ぶ", async () => {
    const onunmute = vi.fn();
    const { getByRole, queryByRole } = render(UserMenu, {
      props: { muted: true, onmute: vi.fn(), onunmute },
    });
    expect(queryByRole("button", { name: "ミュート" })).toBeNull();

    await fireEvent.click(getByRole("button", { name: "ミュート解除" }));

    expect(onunmute).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/ui/UserMenu.test.ts`
Expected: FAIL(`UserMenu.svelte` が存在しない)

- [ ] **Step 3: 実装する**

`frontend/src/ui/UserMenu.svelte`:

```svelte
<script lang="ts">
  // ProfileModal の「…」メニュー(Issue #545)。見た目と項目は NoteMenu に揃える。
  // 非同期処理とエラー表示は呼び出し側(ProfileModal)が持つ。ここは表示と選択だけ。
  import { ChevronLeft, ChevronRight, Eye, EyeOff } from "@lucide/svelte";
  import type { MutePeriod } from "../bindings/tauri.gen";

  let {
    muted,
    onmute,
    onunmute,
  }: {
    muted: boolean;
    onmute: (period: MutePeriod) => void;
    onunmute: () => void;
  } = $props();

  // Misskey Web の「ミュートする期限」と同じ選択肢(既定の無期限が先頭)。
  const PERIODS: { value: MutePeriod; label: string }[] = [
    { value: "indefinitely", label: "無期限" },
    { value: "tenMinutes", label: "10分" },
    { value: "oneHour", label: "1時間" },
    { value: "oneDay", label: "1日" },
    { value: "oneWeek", label: "1週間" },
  ];

  // "root": 項目の一覧 / "period": 期間の選択。同じパネルの中で切り替える。
  let view = $state<"root" | "period">("root");

  const ITEM =
    "box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted";
</script>

<div
  class="w-[200px] rounded-lg border border-border bg-background p-1 shadow-[0_8px_24px_rgba(0,0,0,0.25)]"
  data-testid="user-menu"
>
  {#if view === "root"}
    {#if muted}
      <button type="button" class={ITEM} onclick={onunmute}>
        <Eye size={16} />
        ミュート解除
      </button>
    {:else}
      <button type="button" class={ITEM} onclick={() => (view = "period")}>
        <EyeOff size={16} />
        ミュート
        <ChevronRight size={16} class="ml-auto" />
      </button>
    {/if}
  {:else}
    <button type="button" class={ITEM} onclick={() => (view = "root")}>
      <ChevronLeft size={16} />
      戻る
    </button>
    <span class="block px-2 py-1 text-xs text-muted-foreground">ミュートする期限</span>
    {#each PERIODS as period (period.value)}
      <button type="button" class={ITEM} onclick={() => onmute(period.value)}>{period.label}</button>
    {/each}
  {/if}
</div>
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/UserMenu.test.ts`
Expected: PASS(8 件。`it.each` の4件を含む)

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/UserMenu.svelte frontend/src/ui/UserMenu.test.ts
git commit -m "feat(mute): ユーザーメニューUserMenuを追加 (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: ProfileModal に「…」メニューと「ミュート中」バッジを組み込む

**Files:**
- Modify: `frontend/src/ui/ProfileModal.svelte`
- Modify: `frontend/src/ui/ProfileModal.test.ts`

**Interfaces:**
- Consumes: Task 5 の `app.muteUser` / `app.unmuteUser`、Task 6 の `UserMenu`、Task 4 の `profile.isMuted: boolean | null`。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ProfileModal.test.ts`:

(a) `profileResponse` の既定値に `isMuted: false` を足す(`isFollowing: false,` の次の行):

```ts
    isMuted: false,
```

(b) `describe("ProfileModal", ...)` の末尾(最後の `});` の直前)にテストを追加:

```ts
  describe("ミュート(Issue #545)", () => {
    function mockProfile(overrides: Record<string, unknown> = {}, onCommand?: (cmd: string) => unknown) {
      invokeMock.mockImplementation((cmd: string) => {
        const custom = onCommand?.(cmd);
        if (custom !== undefined) return custom;
        if (cmd === "get_user_profile") return Promise.resolve(profileResponse(overrides));
        if (cmd === "get_user_notes") return Promise.resolve([]);
        return Promise.resolve(null);
      });
    }
    const renderModal = () =>
      render(ProfileModal, { props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} } });
    const menuButton = (r: ReturnType<typeof renderModal>) =>
      r.findByRole("button", { name: "ユーザーメニュー" });

    it("自分自身のプロフィールでは「…」メニューを出さない", async () => {
      mockProfile({ isSelf: true, isFollowing: null, isMuted: null });
      const r = renderModal();
      await waitFor(() => expect(r.getByText("Alice")).toBeTruthy());

      expect(r.queryByRole("button", { name: "ユーザーメニュー" })).toBeNull();
    });

    it("isMutedを取得できないとき(null)は「…」メニューを出さない", async () => {
      mockProfile({ isMuted: null });
      const r = renderModal();
      await waitFor(() => expect(r.getByText("Alice")).toBeTruthy());

      expect(r.queryByRole("button", { name: "ユーザーメニュー" })).toBeNull();
    });

    it("期間を選ぶとmute_userを呼び、「ミュート中」バッジが出る", async () => {
      mockProfile();
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));

      await fireEvent.click(r.getByRole("button", { name: "1時間" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "mute_user",
          expect.objectContaining({ accountId: "acc1", userId: "u1", period: "oneHour" }),
        ),
      );
      await waitFor(() => expect(r.getByText("ミュート中")).toBeTruthy());
      expect(r.queryByTestId("user-menu")).toBeNull(); // メニューは閉じる
    });

    it("無期限を選ぶとperiodにindefinitelyを渡す", async () => {
      mockProfile();
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));

      await fireEvent.click(r.getByRole("button", { name: "無期限" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "mute_user",
          expect.objectContaining({ period: "indefinitely" }),
        ),
      );
    });

    it("ミュート中のプロフィールでは、バッジが出て、「ミュート解除」でunmute_userを呼びバッジが消える", async () => {
      mockProfile({ isMuted: true });
      const r = renderModal();
      await waitFor(() => expect(r.getByText("ミュート中")).toBeTruthy());
      await fireEvent.click(await menuButton(r));

      await fireEvent.click(r.getByRole("button", { name: "ミュート解除" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "unmute_user",
          expect.objectContaining({ accountId: "acc1", userId: "u1" }),
        ),
      );
      await waitFor(() => expect(r.queryByText("ミュート中")).toBeNull());
    });

    it("mute_userが失敗したら、エラーを表示し、バッジは出さない", async () => {
      mockProfile({}, (cmd) => (cmd === "mute_user" ? Promise.reject(new Error("ALREADY_MUTING")) : undefined));
      const r = renderModal();
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));

      await fireEvent.click(r.getByRole("button", { name: "1日" }));

      await waitFor(() => expect(r.getByText(/ALREADY_MUTING/)).toBeTruthy());
      expect(r.queryByText("ミュート中")).toBeNull();
    });

    it("mute_userの完了前に別ユーザーへ切り替わったら、新しいプロフィールのバッジを書き換えない", async () => {
      let resolveMute: (v: unknown) => void = () => {};
      const mutePending = new Promise((resolve) => (resolveMute = resolve));
      invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "mute_user") return mutePending;
        if (cmd === "get_user_profile") {
          const userId = args?.userId as string;
          return Promise.resolve(
            profileResponse({ user: { ...profileResponse().user, id: userId, username: userId } }),
          );
        }
        if (cmd === "get_user_notes") return Promise.resolve([]);
        return Promise.resolve(null);
      });
      const r = render(ProfileModal, {
        props: { target: { userId: "u1" }, accountId: "acc1", onclose: () => {} },
      });
      await fireEvent.click(await menuButton(r));
      await fireEvent.click(r.getByRole("button", { name: "ミュート" }));
      await fireEvent.click(r.getByRole("button", { name: "1時間" }));
      await r.rerender({ target: { userId: "u2" }, accountId: "acc1", onclose: () => {} });
      await waitFor(() => expect(r.getByText("@u2")).toBeTruthy());

      resolveMute(null);
      await Promise.resolve();
      await Promise.resolve();

      expect(r.queryByText("ミュート中")).toBeNull();
    });
  });
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/ui/ProfileModal.test.ts`
Expected: FAIL(「ユーザーメニュー」ボタンが無い)

- [ ] **Step 3: 実装する**

`frontend/src/ui/ProfileModal.svelte`:

1. import を足す(既存の import 群の末尾に):

```ts
  import UserMenu from "./UserMenu.svelte";
  import { portal } from "../lib/portal";
  import { MoreHorizontal } from "@lucide/svelte";
  import type { MutePeriod } from "../bindings/tauri.gen";
```
(`MutePeriod` を含む型 import が既にあれば統合する。)

2. 状態を足す(`followListKind` の宣言の下):

```ts
  // 「…」メニュー(ミュート。Issue #545)。NoteCard のノートメニューと同じ portal + 固定配置。
  let userMenuOpen = $state(false);
  let userMenuAnchor = $state<HTMLElement | null>(null);
  let userMenuPos = $state<{ left: number; top: number } | null>(null);
  let muteBusy = $state(false);
  let muteErr = $state<string | null>(null);
  const USER_MENU_W = 200;
  $effect(() => {
    if (!userMenuOpen || !userMenuAnchor) return;
    const r = userMenuAnchor.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.right - USER_MENU_W), window.innerWidth - USER_MENU_W - 8);
    userMenuPos = { left, top: r.bottom + 6 };
  });
```

3. `load()` の `followErr = null;` の次の行に、状態の初期化を足す:

```ts
    muteErr = null;
    muteBusy = false;
    userMenuOpen = false;
```

4. `toggleFollow` の次に、ハンドラを足す。`requestGen` で、await の間に `target` が切り替わった場合に結果を捨てる:

```ts
  async function mute(period: MutePeriod) {
    if (profileState.status !== "ready" || muteBusy) return;
    const gen = requestGen;
    const userId = profileState.profile.user.id;
    userMenuOpen = false;
    muteBusy = true;
    muteErr = null;
    try {
      await app.muteUser(accountId, userId, period);
      if (gen === requestGen && profileState.status === "ready") profileState.profile.isMuted = true;
    } catch (e) {
      if (gen === requestGen) muteErr = String(e);
    } finally {
      if (gen === requestGen) muteBusy = false; // 切り替え後の新しいプロフィールの busy を、古い呼び出しが触らない
    }
  }

  async function unmute() {
    if (profileState.status !== "ready" || muteBusy) return;
    const gen = requestGen;
    const userId = profileState.profile.user.id;
    userMenuOpen = false;
    muteBusy = true;
    muteErr = null;
    try {
      await app.unmuteUser(accountId, userId);
      if (gen === requestGen && profileState.status === "ready") profileState.profile.isMuted = false;
    } catch (e) {
      if (gen === requestGen) muteErr = String(e);
    } finally {
      if (gen === requestGen) muteBusy = false;
    }
  }
```

5. テンプレート。アクの行の acct の `<span>` の直後(`<span class="text-sm text-muted-foreground">{acct(profile.user)}</span>` の次)にバッジを足す:

```svelte
          {#if profile.isMuted}
            <span class="inline-flex w-fit items-center rounded-sm border border-border px-1 text-xs text-muted-foreground">ミュート中</span>
          {/if}
```

6. フォローボタンの `{#if !profile.isSelf}` ブロック(`</Button>` の後、`{/if}` の前)に、「…」ボタンを足す。フォローボタンを含む既存の `{#if !profile.isSelf} ... {/if}` を次に置き換える:

```svelte
        {#if !profile.isSelf}
          <Button
            size="sm"
            variant={profile.isFollowing ? "outline" : "default"}
            class="flex-none rounded-full {profile.isFollowing ? 'hover:border-destructive hover:text-destructive' : ''}"
            disabled={followBusy}
            onclick={toggleFollow}
          >
            {profile.isFollowing ? "フォロー解除" : "フォロー"}
          </Button>
          {#if profile.isMuted !== null}
            <span class="flex-none" bind:this={userMenuAnchor}>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="ユーザーメニュー"
                disabled={muteBusy}
                onclick={() => (userMenuOpen = !userMenuOpen)}
              >
                <MoreHorizontal size={16} />
              </Button>
            </span>
            {#if userMenuOpen && userMenuPos}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="fixed inset-0 z-[1010]" use:portal onclick={() => (userMenuOpen = false)} role="presentation">
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="fixed"
                  style={`left:${userMenuPos.left}px;top:${userMenuPos.top}px`}
                  onclick={(e) => e.stopPropagation()}
                  role="presentation"
                >
                  <UserMenu muted={profile.isMuted} onmute={mute} onunmute={unmute} />
                </div>
              </div>
            {/if}
          {/if}
        {/if}
```

7. `{#if followErr}...{/if}` の次の行にエラー表示を足す:

```svelte
      {#if muteErr}<p class="my-2 text-sm text-destructive">{muteErr}</p>{/if}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/ProfileModal.test.ts`
Expected: PASS(新規 7 件 + 既存)

Run: `cd frontend && pnpm check && pnpm test`
Expected: エラーなし、全 PASS

メニューが Modal(`z-[1000]`)より前面に出ること(`z-[1010]`)は、Task 8 の実 UI 確認で見る。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/ProfileModal.svelte frontend/src/ui/ProfileModal.test.ts
git commit -m "feat(mute): ProfileModalに「…」メニューとミュート中バッジを追加 (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: ユーザーガイドと実 UI の確認

**Files:**
- Modify: `docs/guide/user-guide.md`(「NG／ミュート」の項、251 行目付近の直後)

- [ ] **Step 1: ユーザーガイドを書く**

`docs/guide/user-guide.md` の `- **NG／ミュート**: ...` の行(251 行目付近)の直後に、次の項目を追加する:

```markdown
- **ユーザーのミュート(期間指定)**: プロフィールの「…」メニューから「ミュート」を選び、期限(無期限・10分・1時間・1日・1週間)を選ぶと、そのユーザーをサーバー側でミュートします。ミュート中のユーザーのプロフィールには「ミュート中」と表示され、「…」メニューの「ミュート解除」で解除できます。期限が過ぎたミュートはサーバーが自動で解除し、tsumugi も期限の直後に反映します。ミュートの反映は、次に受信・読み込みするノートからです。この機能を使うには、ミュートの書き込み権限(`write:mutes`)が必要なので、これまでログインしていたアカウントは、操作時に権限不足の案内が出たら、設定のアカウント一覧にある「再認証」からもう一度認可してください。すでにミュート中のユーザーの期限を変えるには、いったん解除してからミュートし直します。ミュートの作成は、サーバーの制限で1時間に20回までです。
```

- [ ] **Step 2: 全テストを通す**

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd frontend && pnpm check && pnpm test`
Expected: エラーなし、全 PASS

- [ ] **Step 3: 実 UI で確認する(自分で起動したプロセスは、確認後に正確な PID で止める)**

`e2e/README.md` の手順で Compose 版 Misskey を起動し、Xvfb + `dbus-run-session` 越しに(`WAYLAND_DISPLAY` は unset、`DISPLAY` は Xvfb のもの)、隔離した XDG パス(`/tmp` 直下の短いパス)でアプリを動かして、次を確認する。デバッグブリッジ経由で JS を実行し、スクリーンショットは自分で見る:

1. 新規ログイン(`write:mutes` を含む)で、他ユーザーのプロフィールを開き、「…」→「ミュート」→「10分」でミュートでき、「ミュート中」バッジが出る。
2. 「…」メニューが、Modal の前面に出る(隠れない)。
3. 期限が短いミュートは、Compose 版 Misskey 側で期限を過ぎた後(`mute/list` から消える前でも)、tsumugi の側で解除として扱われ、バッジが再表示時に消える。
4. 「ミュート解除」で解除でき、「…」メニューが「ミュート」に戻る。
5. 既存アカウント(`write:mutes` なしのトークン)で操作すると、Backstage に再認証の案内が出る。
6. すでにミュート中のユーザーを別クライアントで先にミュートしてから、tsumugi で操作すると、エラーが表示される。

実機で確認できなかった項目は、「未確認」と明記して報告する(確認したことにしない)。

- [ ] **Step 4: コミット**

```bash
git add docs/guide/user-guide.md
git commit -m "docs(guide): 時限ミュートの使い方を追加 (#545)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: PR を作る(ユーザーの指示があってから)**

`git push -u origin feat/issue-545-timed-mute` の後、`gh pr create` で PR を作る。本文に `Closes #545` を入れ、`.github/pull_request_template.md` の構成に従う。末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。push 後に CI を待たない。**push と PR 作成は、ユーザーの承認を得てから行う。**
