# タイムライン返信可視性修正 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Local/SocialカラムがMisskeyの返信(`withReplies`)を無条件で全部受け取るようにし、TQLの `reply_to_me` 述語を実際に機能させることで、Issue #297（他人宛返信が出ない）と #298（自分宛返信がREST/Streamingで挙動が違う）を解消する。

**Architecture:** `domain::Note` に `reply_user_id` フィールドを追加し、`api/normalize.rs` でMisskeyの生レスポンスから返信先ユーザーIDを抽出して詰める。`filter/eval.rs` の `ReplyToMe` 述語スタブをこの値を使う実装に置き換える。`domain/column.rs` の `ColumnKind::Local`/`Hybrid` のREST/Streamingリクエストに `withReplies: true` を追加し、サーバ側フィルタを無効化して全返信を受信する。

**Tech Stack:** Rust (src-tauri), serde/specta, rusqlite

## Global Constraints

- 設計書: `docs/superpowers/specs/2026-09-12-timeline-reply-visibility-design.md`
- `Note` は `#[serde(rename_all = "camelCase")]` + `specta::Type` — 新規フィールドもTSへ自動でcamelCase出力される。手動でのバインディング編集はしない（`cargo test` が自動生成）。
- Homeタイムラインは対象外（本家APIに制御パラメータが存在しないため）。今回のタスクでは一切触らない。
- トグルUIは作らない。挙動変更はサーバへのリクエストパラメータのみ。

---

### Task 1: `domain::Note` に `reply_user_id` を追加する

**Files:**
- Modify: `src-tauri/src/domain/note.rs`（`Note` 構造体定義、`reply_id` フィールドの直後に追加。既存テストフィクスチャ `minimal_note()` も修正）
- Modify: `src-tauri/src/filter/mod.rs`（テスト用フィクスチャ、`reply_id: None,` の行）
- Modify: `src-tauri/src/filter/eval.rs`（テスト用フィクスチャ `base_note()`、`reply_id: None,` の行）
- Modify: `src-tauri/src/commands/column.rs:1292`（テスト用フィクスチャ、`reply_id: None,` の行）
- Modify: `src-tauri/src/commands/note.rs:489`（テスト用フィクスチャ、`reply_id: None,` の行）
- Modify: `src-tauri/src/store/note_cache.rs`（テスト用フィクスチャ `note()`、`reply_id: None,` の行）
- Test: `src-tauri/src/domain/note.rs`（既存 `#[cfg(test)] mod tests` に追加）

**Interfaces:**
- Produces: `domain::Note.reply_user_id: Option<String>` — 以降のタスクがこのフィールドを読み書きする。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/note.rs` の既存 `#[cfg(test)] mod tests` 内、`minimal_note()` 定義の後に追加：

```rust
    #[test]
    fn reply_user_id_round_trips_as_camel_case() {
        let mut n = minimal_note();
        n.reply_id = Some("r1".into());
        n.reply_user_id = Some("target-user".into());

        let v = serde_json::to_value(&n).unwrap();
        assert_eq!(v["replyUserId"], "target-user");

        let back: Note = serde_json::from_value(v).unwrap();
        assert_eq!(back.reply_user_id.as_deref(), Some("target-user"));
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test reply_user_id_round_trips_as_camel_case`
Expected: FAIL（`Note` に `reply_user_id` フィールドが無くコンパイルエラー）

- [ ] **Step 3: `Note` 構造体にフィールドを追加する**

`src-tauri/src/domain/note.rs` の `pub reply_id: Option<String>,` の直後に追加：

```rust
    pub reply_id: Option<String>,
    /// 返信先ノートの投稿者 userId（`reply_to_me` 述語用）。返信でない場合は None
    pub reply_user_id: Option<String>,
```

- [ ] **Step 4: 既存フィクスチャを修正してコンパイルを通す**

以下6箇所の `reply_id: None,` の直後に `reply_user_id: None,` を追加する：
- `src-tauri/src/domain/note.rs`（`minimal_note()` 内）
- `src-tauri/src/filter/mod.rs`
- `src-tauri/src/filter/eval.rs`（`base_note()` 内）
- `src-tauri/src/commands/column.rs:1292`
- `src-tauri/src/commands/note.rs:489`
- `src-tauri/src/store/note_cache.rs`（`note()` 内）

- [ ] **Step 5: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test reply_user_id_round_trips_as_camel_case`
Expected: PASS

- [ ] **Step 6: 全体のコンパイル・既存テストを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テストPASS（フィクスチャ漏れがあればここでコンパイルエラーが出るので都度Step 4に戻って直す）

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/domain/note.rs src-tauri/src/filter/mod.rs src-tauri/src/filter/eval.rs src-tauri/src/commands/column.rs src-tauri/src/commands/note.rs src-tauri/src/store/note_cache.rs
git commit -m "feat: domain::Note に reply_user_id を追加"
```

---

### Task 2: `api/normalize.rs` で `reply_user_id` を実データから抽出する

**Files:**
- Modify: `src-tauri/src/api/normalize.rs`（`RawNote` 構造体、`From<RawNote> for Note` 実装）
- Test: `src-tauri/src/api/normalize.rs`（既存 `#[cfg(test)] mod tests` に追加）

**Interfaces:**
- Consumes: `domain::Note.reply_user_id`（Task 1で追加済み）
- Produces: `RawNote.reply: Option<Box<RawNote>>`（Misskeyの生JSONレスポンスが持つネストされた返信元ノート。以降のタスクは使わない）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/api/normalize.rs` の `#[cfg(test)] mod tests` 内、既存の `parses_note_with_reactions_and_epoch` の後に追加：

```rust
    #[test]
    fn extracts_reply_user_id_from_nested_reply() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n2","createdAt":"2026-07-05T12:00:00.000Z","text":"@bob hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public","replyId":"r1",
              "reply":{
                "id":"r1","createdAt":"2026-07-05T11:00:00.000Z","text":"original",
                "user":{"id":"bob-id","username":"bob","host":null},
                "visibility":"public"
              }
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert_eq!(n.reply_id.as_deref(), Some("r1"));
        assert_eq!(n.reply_user_id.as_deref(), Some("bob-id"));
    }

    #[test]
    fn reply_user_id_is_none_when_not_a_reply() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n3","createdAt":"2026-07-05T12:00:00.000Z","text":"hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public"
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert_eq!(n.reply_user_id, None);
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test extracts_reply_user_id_from_nested_reply`
Expected: FAIL（`RawNote` に `reply` フィールドが無く、JSON中の `reply` は無視され、`n.reply_user_id` は常に `None` のためassert失敗。または`Note`に`reply_user_id`が無ければコンパイルエラー＝Task1未完了）

- [ ] **Step 3: `RawNote` に `reply` フィールドを追加する**

`src-tauri/src/api/normalize.rs` の `RawNote` 構造体、`pub reply_id: Option<String>,` の直後に追加：

```rust
    #[serde(default)]
    pub reply_id: Option<String>,
    /// ネストされた返信元ノート（reply_user_id 抽出用。浅く1階層のみ使う）
    #[serde(default)]
    pub reply: Option<Box<RawNote>>,
```

- [ ] **Step 4: `From<RawNote> for Note` で `reply_user_id` を詰める**

`impl From<RawNote> for Note` 内、`reply_id: r.reply_id,` の直後に追加：

```rust
            reply_id: r.reply_id,
            reply_user_id: r.reply.as_ref().map(|reply| reply.user.id.clone()),
```

- [ ] **Step 5: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test extracts_reply_user_id_from_nested_reply reply_user_id_is_none_when_not_a_reply`
Expected: 両方PASS

- [ ] **Step 6: 全体テストを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テストPASS

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/api/normalize.rs
git commit -m "feat: RawNote から reply_user_id を抽出する"
```

---

### Task 3: `filter/eval.rs` の `ReplyToMe` 述語を実装する

**Files:**
- Modify: `src-tauri/src/filter/eval.rs`
- Test: `src-tauri/src/filter/eval.rs`（既存 `#[cfg(test)] mod tests` に追加）

**Interfaces:**
- Consumes: `domain::Note.reply_user_id`（Task 1）、`EvalContext.my_user_ids: HashSet<String>`（既存）
- Produces: `evaluate(&Expr, &Note, &EvalContext) -> bool` が `reply_to_me` クエリで正しく判定するようになる（既存シグネチャ、変更なし）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/filter/eval.rs` の `#[cfg(test)] mod tests` 内、`matches` ヘルパーの近くに追加：

```rust
    #[test]
    fn reply_to_me_matches_when_reply_user_id_is_mine() {
        let mut n = base_note();
        n.reply_id = Some("r1".into());
        // ctx() の my_user_ids は "me1" を含む（本ファイル既存の ctx() ヘルパー参照）
        n.reply_user_id = Some("me1".into());
        assert!(matches("reply_to_me", &n));
    }

    #[test]
    fn reply_to_me_does_not_match_when_reply_user_id_is_someone_else() {
        let mut n = base_note();
        n.reply_id = Some("r1".into());
        n.reply_user_id = Some("stranger".into());
        assert!(!matches("reply_to_me", &n));
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test reply_to_me_matches_when_reply_user_id_is_mine`
Expected: FAIL（`ReplyToMe => false` で常に不一致）

- [ ] **Step 3: `eval_bool` の `ReplyToMe` を実装する**

`src-tauri/src/filter/eval.rs` の `eval_bool` 内：

```rust
        // before
        // reply_user_id は domain::Note に無いため未対応（常に false）
        ReplyToMe => false,

        // after
        ReplyToMe => n.reply_user_id.as_ref().map_or(false, |u| ctx.my_user_ids.contains(u)),
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test reply_to_me_matches_when_reply_user_id_is_mine reply_to_me_does_not_match_when_reply_user_id_is_someone_else`
Expected: 両方PASS

- [ ] **Step 5: 全体テストを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テストPASS

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/filter/eval.rs
git commit -m "feat: TQL の reply_to_me 述語を実装する"
```

---

### Task 4: `store/note_cache.rs` に実際の `reply_user_id` を保存する

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs`
- Test: `src-tauri/src/store/note_cache.rs`（既存 `#[cfg(test)] mod tests` に追加）

**Interfaces:**
- Consumes: `domain::Note.reply_user_id`（Task 1）
- Produces: なし（既存の `reply_user_id TEXT` カラムへの書き込み値が変わるのみ。読み出しは既存どおり `payload` JSON経由でありこのタスクの影響を受けない）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/note_cache.rs` の `#[cfg(test)] mod tests` 内、`upsert_note_stores_stubbed_user_in_payload` の後に追加：

```rust
    #[test]
    fn upsert_note_stores_reply_user_id_column() {
        let conn = crate::store::db::open_cache_in_memory().unwrap();
        let mut n = note("n1", 100);
        n.reply_id = Some("r1".into());
        n.reply_user_id = Some("bob-id".into());
        upsert_note(&conn, &n).unwrap();

        let stored: Option<String> = conn
            .query_row("SELECT reply_user_id FROM note WHERE id = 'n1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored.as_deref(), Some("bob-id"));
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test upsert_note_stores_reply_user_id_column`
Expected: FAIL（`reply_user_id` 列には常に `NULL` が書き込まれるため `stored` が `None`）

- [ ] **Step 3: ハードコードされた `None` を実値に置き換える**

`src-tauri/src/store/note_cache.rs` の `upsert_note` 内、`INSERT OR REPLACE INTO note (...)` の `params![...]` にある：

```rust
            n.reply_id,
            Option::<String>::None, // reply_user_id: Note には無いため NULL（reply_to_me は限定的）
```

を以下に置き換える：

```rust
            n.reply_id,
            n.reply_user_id,
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test upsert_note_stores_reply_user_id_column`
Expected: PASS

- [ ] **Step 5: 全体テストを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テストPASS

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/store/note_cache.rs
git commit -m "feat: note_cache に reply_user_id の実値を保存する"
```

---

### Task 5: Local/Hybrid カラムに `withReplies: true` を送る

**Files:**
- Modify: `src-tauri/src/domain/column.rs`（`ColumnKind::stream_request`、`ColumnKind::rest_request`）
- Test: `src-tauri/src/domain/column.rs`（既存 `#[cfg(test)] mod tests` に追加）

**Interfaces:**
- Consumes: なし（Task 1〜4と独立）
- Produces: なし（`ColumnKind::Local`/`Hybrid` の `stream_request()`/`rest_request()` が返すJSONの中身が変わるのみ。呼び出し側 `stream/connection.rs`・`commands/column.rs` はJSONをそのまま転送しているだけで、シグネチャ変更なし）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/column.rs` の `#[cfg(test)] mod tests` 内、既存テストの後に追加：

```rust
    #[test]
    fn local_and_hybrid_stream_request_include_with_replies() {
        let local = ColumnKind::Local;
        let (ch, params) = local.stream_request().unwrap();
        assert_eq!(ch, "localTimeline");
        assert_eq!(params["withReplies"], true);

        let hybrid = ColumnKind::Hybrid;
        let (ch, params) = hybrid.stream_request().unwrap();
        assert_eq!(ch, "hybridTimeline");
        assert_eq!(params["withReplies"], true);
    }

    #[test]
    fn local_and_hybrid_rest_request_include_with_replies() {
        let local = ColumnKind::Local;
        let (ep, body) = local.rest_request(20, None).unwrap();
        assert_eq!(ep, "notes/local-timeline");
        assert_eq!(body["withReplies"], true);

        let hybrid = ColumnKind::Hybrid;
        let (ep, body) = hybrid.rest_request(20, None).unwrap();
        assert_eq!(ep, "notes/hybrid-timeline");
        assert_eq!(body["withReplies"], true);
    }

    #[test]
    fn home_and_global_do_not_include_with_replies() {
        let home = ColumnKind::Home;
        let (_, params) = home.stream_request().unwrap();
        assert!(params.get("withReplies").is_none());
        let (_, body) = home.rest_request(20, None).unwrap();
        assert!(body.get("withReplies").is_none());

        let global = ColumnKind::Global;
        let (_, params) = global.stream_request().unwrap();
        assert!(params.get("withReplies").is_none());
        let (_, body) = global.rest_request(20, None).unwrap();
        assert!(body.get("withReplies").is_none());
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test local_and_hybrid_stream_request_include_with_replies local_and_hybrid_rest_request_include_with_replies`
Expected: FAIL（現状 `params`/`body` に `withReplies` キー自体が無いため `params["withReplies"]` は `Value::Null` になり `true` と不一致）

- [ ] **Step 3: `stream_request()` を修正する**

`src-tauri/src/domain/column.rs` の `stream_request()` 内：

```rust
            // before
            ColumnKind::Local => ("localTimeline", json!({})),
            ColumnKind::Hybrid => ("hybridTimeline", json!({})),

            // after
            ColumnKind::Local => ("localTimeline", json!({ "withReplies": true })),
            ColumnKind::Hybrid => ("hybridTimeline", json!({ "withReplies": true })),
```

- [ ] **Step 4: `rest_request()` を修正する**

`rest_request()` 内、`ColumnKind::Local`/`ColumnKind::Hybrid` の分岐に `withReplies` を追加：

```rust
            // before
            ColumnKind::Home => ("notes/timeline", body),
            ColumnKind::Local => ("notes/local-timeline", body),
            ColumnKind::Global => ("notes/global-timeline", body),
            ColumnKind::Hybrid => ("notes/hybrid-timeline", body),

            // after
            ColumnKind::Home => ("notes/timeline", body),
            ColumnKind::Local => {
                body["withReplies"] = json!(true);
                ("notes/local-timeline", body)
            }
            ColumnKind::Global => ("notes/global-timeline", body),
            ColumnKind::Hybrid => {
                body["withReplies"] = json!(true);
                ("notes/hybrid-timeline", body)
            }
```

- [ ] **Step 5: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test local_and_hybrid_stream_request_include_with_replies local_and_hybrid_rest_request_include_with_replies home_and_global_do_not_include_with_replies`
Expected: 全てPASS

- [ ] **Step 6: 全体テストを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テストPASS

- [ ] **Step 7: コミット**

```bash
git add src-tauri/src/domain/column.rs
git commit -m "fix: Local/Hybridカラムで withReplies:true を送るようにする"
```

---

### Task 6: フロントエンドバインディング再生成の確認 + 動作確認

**Files:**
- Modify: なし（`frontend/src/bindings/tauri.gen.ts` は自動生成、手編集禁止）

**Interfaces:**
- Consumes: Task 1〜5の全変更
- Produces: なし（最終確認タスク）

- [ ] **Step 1: バインディング再生成を確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` の `Note` 型に `replyUserId: string | null` が含まれることを確認する（`grep replyUserId frontend/src/bindings/tauri.gen.ts`）。

- [ ] **Step 2: フロントエンドの型チェックを確認する**

Run: `cd frontend && pnpm check`
Expected: PASS（`Note` 型へのフィールド追加は既存コードの破壊的変更にならないはず。エラーが出た場合は該当箇所を確認して対応する）

- [ ] **Step 3: Rust側の全テストを最終確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テストPASS

- [ ] **Step 4: コミット（バインディング差分がある場合のみ）**

```bash
git add frontend/src/bindings/tauri.gen.ts
git commit -m "chore: フロントエンドバインディングを再生成する" --allow-empty
```
