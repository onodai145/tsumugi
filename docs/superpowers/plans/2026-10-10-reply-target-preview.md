# 返信先プレビュー Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 返信ノートの上に返信先の投稿者名と本文冒頭を1行で表示し、クリックで返信先ノートをその場に展開できるようにする(Issue #287)。

**Architecture:** `domain::Note` に `reply: Option<Box<Note>>` を浅く(1階層)保持し、`renote` と同じ経路で正規化・キャッシュする。キャッシュの user 参照処理(`collect_users` / `stub_user_refs` / `collect_user_id_refs` / `has_legacy_full_user` / `hydrate_user_refs` / `self_heal_node`)を `reply` にも再帰させ、保存と復元が非対称にならないようにする。フロントは `NoteCard.svelte` の「↩ 返信」バナーを置き換える。

**Tech Stack:** Rust (serde, rusqlite, sqlx, tauri-specta), Svelte 5 (runes), Vitest, svelte-check

**Spec:** `docs/superpowers/specs/2026-10-10-reply-target-preview-design.md`

## Global Constraints

- 作業ブランチは `feat/issue-287-reply-target-preview`(`main` へ直接コミットしない)。
- コミットメッセージは件名のみ・本文なし。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` トレーラーを付ける(トレーラーは本文ではない)。`--no-verify` / `--no-gpg-sign` は使わない。コミットが失敗・タイムアウトしたら停止して報告し、再試行しない。
- `reply` は1階層のみ保持する。`reply.reply` は常に `None`。
- DB スキーマは変更しない(`note.payload` 内の追加フィールドのみ)。マイグレーションは書かない。
- `reply_id` / `reply_user_id` は TQL が使うため変更しない。
- `frontend/src/bindings/tauri.gen.ts` は手編集しない。`cd src-tauri && cargo test` で再生成する。
- フロントのスタイルは `docs/design/style-guide.md` のスケールに従い、`rounded-[Npx]` や `text-[Nrem]` のような任意値を新設しない。アイコンは `size={12}`。
- `cargo tauri dev` / `./target/debug/tsumugi` / `cargo run` を直接起動しない。実画面確認が必要な場合は、リポジトリルートから Xvfb 越し・`WAYLAND_DISPLAY` を unset・`dbus-run-session` 内で、隔離した短い XDG パスの別インスタンスとして起動し、自分で起動したプロセスは正確な PID で終了させる(`pkill` / `killall` 禁止)。
- Postgres / MySQL の実 DB テストは `--ignored` で、MySQL は `--test-threads=2`。コンテナ掃除は `docker rm -fv <id>`。
- 日本語のコメント・ドキュメント文体を既存コードに合わせる。

## Review Focus

- 返信先ユーザーが `user` テーブルに無い(欠落)キャッシュ行: ノート行を捨てず `reply` だけ `null` になる(Task 2, Task 3 のテストで固定)。
- `reply` キー自体を持たない旧 payload: `reply: None` として読める(Task 1 のテストで固定)。
- 返信先が CW 付き・本文なしでファイルのみ・本文もファイルも無い純 Renote: 1行プレビューが空にならず、CW 文言 / 「(画像)」「(ファイル)」/ 「(Renote)」になる(Task 4 のテストで固定)。
- 返信先が取れない返信(削除済み・閲覧不可): 従来どおり「↩ 返信」だけが出て、クリック可能な空ボタンが出ない(Task 5 のテストで固定)。
- 展開した返信先カードが、さらにバナーを出して返信の連鎖を展開し続けない(Task 5 のテストで固定)。

---

## File Structure

| ファイル | 責務 |
|---|---|
| `src-tauri/src/domain/note.rs` | `Note.reply` フィールド定義 |
| `src-tauri/src/api/normalize.rs` | `RawNote.reply` → `Note.reply`(1階層に切る) |
| `src-tauri/src/store/user_ref.rs` | user 参照のスタブ化・埋め戻しを `reply` へ拡張(純粋関数) |
| `src-tauri/src/store/note_cache.rs` / `postgres_backend.rs` / `mysql_backend.rs` | `self_heal_node` を `reply` へ再帰。往復テスト |
| `src-tauri/src/store/sqlite_backend.rs` | SQLite の往復・縮退テスト |
| `frontend/src/lib/replyPreview.ts`(新規) | 1行プレビューに出す本文の決定(純粋関数) |
| `frontend/src/ui/NoteCard.svelte` | バナーを1行プレビュー+展開に置き換え |
| `docs/guide/user-guide.md` / `docs/design/misskey-multicolumn-client-design.md` | ドキュメント |

---

### Task 1: `Note.reply` フィールドと正規化

**Files:**
- Modify: `src-tauri/src/domain/note.rs:21-26`(フィールド追加)、同ファイル `mod tests`(テスト追加)
- Modify: `src-tauri/src/api/normalize.rs:311-313`(正規化)、同ファイル `mod tests`
- Modify: `reply: None,` を追加する Rust のテスト用 `Note` リテラル(下記一覧)
- Modify: `frontend/src/bindings/tauri.gen.ts`(再生成のみ)
- Modify: `reply: null,` を追加する TS テスト用 `Note` リテラル(下記一覧)

**Interfaces:**
- Consumes: なし
- Produces: `Note.reply: Option<Box<Note>>`(Rust)/ `Note.reply: Note | null`(TS)。以降の Task は `Note.reply` を前提とする。

- [ ] **Step 1: 失敗するテストを書く(正規化)**

`src-tauri/src/api/normalize.rs` の `mod tests` の `reply_user_id_is_none_when_not_a_reply` の直後に追加:

```rust
    #[test]
    fn keeps_nested_reply_as_shallow_note() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n2","createdAt":"2026-07-05T12:00:00.000Z","text":"@bob hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public","replyId":"r1",
              "reply":{
                "id":"r1","createdAt":"2026-07-05T11:00:00.000Z","text":"original",
                "user":{"id":"bob-id","username":"bob","host":null,"name":"Bob"},
                "visibility":"public","replyId":"r0",
                "reply":{
                  "id":"r0","createdAt":"2026-07-05T10:00:00.000Z","text":"grandparent",
                  "user":{"id":"carol-id","username":"carol","host":null},
                  "visibility":"public"
                }
              }
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        let reply = n.reply.expect("reply should be kept");
        assert_eq!(reply.id, "r1");
        assert_eq!(reply.text.as_deref(), Some("original"));
        assert_eq!(reply.user.name.as_deref(), Some("Bob"));
        // 浅く1階層のみ: 返信先の返信先は落とす(reply_id は残る)
        assert!(reply.reply.is_none());
        assert_eq!(reply.reply_id.as_deref(), Some("r0"));
    }

    #[test]
    fn reply_is_none_when_not_a_reply() {
        let raw: RawNote = serde_json::from_str(
            r#"{
              "id":"n3","createdAt":"2026-07-05T12:00:00.000Z","text":"hi",
              "user":{"id":"u1","username":"alice","host":null},
              "visibility":"public"
            }"#,
        )
        .unwrap();
        let n: Note = raw.into();
        assert!(n.reply.is_none());
    }
```

`src-tauri/src/domain/note.rs` の `mod tests` の `reply_user_id_round_trips_as_camel_case` の直後に追加:

```rust
    #[test]
    fn reply_round_trips_and_missing_key_deserializes_as_none() {
        let mut n = minimal_note();
        let mut target = minimal_note();
        target.id = "r1".into();
        n.reply_id = Some("r1".into());
        n.reply = Some(Box::new(target));

        let v = serde_json::to_value(&n).unwrap();
        assert_eq!(v["reply"]["id"], "r1");
        let back: Note = serde_json::from_value(v).unwrap();
        assert_eq!(back.reply.as_ref().map(|r| r.id.as_str()), Some("r1"));

        // 旧キャッシュ行(reply キーを持たない payload)は None として読める
        let mut legacy = serde_json::to_value(&minimal_note()).unwrap();
        legacy.as_object_mut().unwrap().remove("reply");
        let back: Note = serde_json::from_value(legacy).unwrap();
        assert!(back.reply.is_none());
    }
```

- [ ] **Step 2: フィールドを追加する前に、コンパイルエラーで失敗することを確認する**

Run: `cd src-tauri && cargo test --lib normalize::tests::keeps_nested_reply_as_shallow_note 2>&1 | tail -15`
Expected: FAIL(`no field named reply` / `no field reply on type Note` のコンパイルエラー)

- [ ] **Step 3: フィールドを追加する**

`src-tauri/src/domain/note.rs` の `reply_user_id` の直後(`renote_id` の前)に追加:

```rust
    /// 返信先ノート。renote と同様に浅く1階層のみ保持する(`reply.reply` は常に None)。
    /// 表示専用でフィルタ評価には使わない。返信でない/返信先が取得できない場合は None
    pub reply: Option<Box<Note>>,
```

`src-tauri/src/api/normalize.rs` の `impl From<RawNote> for Note` で、`reply_user_id: ...` の直後に追加(`reply_user_id` は `r.reply` を借用するので、必ず `reply` より前に書く):

```rust
            reply: r.reply.map(|n| {
                let mut reply: Note = (*n).into();
                // 浅く1階層のみ保持する
                reply.reply = None;
                Box::new(reply)
            }),
```

- [ ] **Step 4: 既存のテスト用 `Note` リテラルに `reply: None,` を追加する**

次の各ファイルで `renote: None,` の行(インデント同じ)の直前に `reply: None,` を1行足す。

対象(`renote: None,` が単独行のもの): `src-tauri/src/state.rs`、`src-tauri/src/commands/mute.rs`、`src-tauri/src/commands/column.rs`、`src-tauri/src/commands/note.rs`、`src-tauri/src/domain/note.rs`(`minimal_note` 内)、`src-tauri/src/store/sqlite_backend.rs`、`src-tauri/src/store/user_ref.rs`、`src-tauri/src/stream/connection.rs`、`src-tauri/src/filter/mute.rs`、`src-tauri/src/filter/eval.rs`、`src-tauri/src/filter/mod.rs`、`src-tauri/src/store/note_cache.rs`。

行の位置を確認しながら進める:

```sh
cd src-tauri && grep -rn "^\s*renote: None,$" src --include='*.rs'
```

各行の直前に、同じインデントで `reply: None,` を挿入する(`sed -i 's/^\(\s*\)renote: None,$/\1reply: None,\n\1renote: None,/'` を上記ファイルに適用してよい。`renote: None,` が1行に複数フィールドと並ぶインライン形式は sed の対象外)。

インライン形式の2箇所は手で直す:
- `src-tauri/src/store/postgres_backend.rs`(1258行付近): `reply_id: None, reply_user_id: None, renote_id: None, renote: None,` → `reply_id: None, reply_user_id: None, reply: None, renote_id: None, renote: None,`
- `src-tauri/src/store/mysql_backend.rs`(1285行付近): 同様に `reply: None,` を `reply_user_id: None,` の後ろへ足す。

`sed` の結果と上記2箇所を確認する:

```sh
cd src-tauri && git diff --stat && cargo check --tests 2>&1 | tail -20
```
Expected: エラーなし(未対応のリテラルがあれば `missing field reply` が出るので、そのファイルを同様に直す)

- [ ] **Step 5: テストを実行し、パスすることと TS バインディングの再生成を確認する**

`cargo test` は1回に1フィルタしか渡せないので、個別に実行する:

```sh
cd src-tauri && cargo test normalize:: 2>&1 | tail -5
cd src-tauri && cargo test domain::note:: 2>&1 | tail -5
cd src-tauri && cargo test generates_frontend_bindings 2>&1 | tail -5
git diff frontend/src/bindings/tauri.gen.ts
```
Expected: 全て PASS。`tauri.gen.ts` の `Note` に `reply: Note | null,` が `replyUserId` の直後に1行追加されただけの差分になる。

- [ ] **Step 6: TS のテスト用 `Note` リテラルに `reply: null,` を追加する**

次の各ファイルで `replyUserId: null,` の行の直後に同じインデントで `reply: null,` を足す:
`frontend/src/ui/NoteCard.test.ts`、`frontend/src/lib/store.svelte.haptics.test.ts`、`frontend/src/ui/ProfileModal.test.ts`、`frontend/src/ui/SearchModal.test.ts`、`frontend/src/ui/NotificationCard.test.ts`、`frontend/src/lib/store.svelte.test.ts`。

Run: `cd frontend && pnpm check 2>&1 | tail -15`
Expected: エラーなし(漏れがあれば `Property 'reply' is missing` が出るので同様に直す)

- [ ] **Step 7: Rust 全体とフロント既存テストの回帰確認**

Run: `cd src-tauri && cargo test 2>&1 | tail -8` と `cd frontend && pnpm test 2>&1 | tail -8`
Expected: 全て PASS

- [ ] **Step 8: コミット**

```bash
git add src-tauri frontend/src
git commit -m "feat: Noteに返信先ノートを浅く保持する(#287)"
```

---

### Task 2: user 参照処理を `reply` に拡張する(純粋関数)

**Files:**
- Modify: `src-tauri/src/store/user_ref.rs:147-214`(関数本体)、同ファイル `mod tests`

**Interfaces:**
- Consumes: `Note.reply: Option<Box<Note>>`(Task 1)
- Produces: シグネチャは変更しない。次の挙動が変わる。
  - `collect_users(&Note) -> Vec<&User>`: 本体 → renote → reply の順で返す。
  - `stub_user_refs(&mut Value)`: `reply.user` もスタブ化する。
  - `collect_user_id_refs(&Value, &mut Vec<String>)`: `reply.user.id` も集める。
  - `has_legacy_full_user(&Value) -> bool`: `reply` 側の旧形式も検出する。
  - `hydrate_user_refs(&mut Value, &HashMap<String, User>) -> bool`: `reply.user` が欠けても `false` を返さず、`note_value["reply"]` を `null` にして `true`。本体・renote の欠落は従来どおり `false`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/user_ref.rs` の `mod tests` の末尾(最後の `}` の前)に追加:

```rust
    #[test]
    fn collect_users_includes_reply_author() {
        let mut n = bare_note("n1", user_lite("u1", "Alice"));
        n.reply = Some(Box::new(bare_note("n0", user_lite("u3", "Carol"))));
        let users = collect_users(&n);
        assert_eq!(users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1", "u3"]);
    }

    #[test]
    fn stub_user_refs_recurses_into_reply() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1", "username": "alice" },
            "reply": {
                "id": "n0",
                "user": { "id": "u3", "username": "carol" },
                "reply": null
            }
        });
        stub_user_refs(&mut v);
        assert_eq!(v["user"], json!({ "id": "u1" }));
        assert_eq!(v["reply"]["user"], json!({ "id": "u3" }));
    }

    #[test]
    fn collect_user_id_refs_collects_reply_author() {
        let v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" }, "renote": null },
            "reply": { "id": "r0", "user": { "id": "u3" }, "reply": null }
        });
        let mut ids = Vec::new();
        collect_user_id_refs(&v, &mut ids);
        assert_eq!(ids, vec!["u1".to_string(), "u2".to_string(), "u3".to_string()]);
    }

    #[test]
    fn has_legacy_full_user_detects_legacy_shape_in_reply() {
        let v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "reply": { "id": "r0", "user": { "id": "u3", "username": "carol" } }
        });
        assert!(has_legacy_full_user(&v));
        let v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "reply": { "id": "r0", "user": { "id": "u3" } }
        });
        assert!(!has_legacy_full_user(&v));
    }

    #[test]
    fn hydrate_user_refs_fills_in_reply_author() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "reply": { "id": "r0", "user": { "id": "u3" }, "reply": null }
        });
        let mut users = HashMap::new();
        users.insert("u1".to_string(), user_lite("u1", "Alice"));
        users.insert("u3".to_string(), user_lite("u3", "Carol"));

        assert!(hydrate_user_refs(&mut v, &users));
        assert_eq!(v["reply"]["user"]["username"], json!("carol"));
    }

    #[test]
    fn hydrate_user_refs_drops_only_reply_when_reply_author_missing() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "reply": { "id": "r0", "user": { "id": "u3" }, "reply": null }
        });
        let mut users = HashMap::new();
        users.insert("u1".to_string(), user_lite("u1", "Alice"));

        // 返信先は補助表示なので、本体は復元可能(true)のまま reply だけ null に落とす
        assert!(hydrate_user_refs(&mut v, &users));
        assert_eq!(v["user"]["username"], json!("alice"));
        assert_eq!(v["reply"], serde_json::Value::Null);
    }

    #[test]
    fn hydrate_user_refs_still_fails_when_renote_author_missing_even_with_reply() {
        let mut v = json!({
            "id": "n1",
            "user": { "id": "u1" },
            "renote": { "id": "n0", "user": { "id": "u2" }, "renote": null },
            "reply": { "id": "r0", "user": { "id": "u3" }, "reply": null }
        });
        let mut users = HashMap::new();
        users.insert("u1".to_string(), user_lite("u1", "Alice"));
        users.insert("u3".to_string(), user_lite("u3", "Carol"));

        assert!(!hydrate_user_refs(&mut v, &users));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test user_ref:: 2>&1 | tail -25`
Expected: 追加した7件が FAIL(`reply` を無視しているため)。既存テストは PASS。

- [ ] **Step 3: 実装する**

`src-tauri/src/store/user_ref.rs` の各関数を次のように変更する。

`collect_users`:

```rust
/// ノート本体+renote+reply(入れ子)分の User をすべて集める(重複排除はしない)。
/// upsert_note が「note.payload に埋め込まれる全ユーザー」をキャッシュへ反映するために使う。
pub(crate) fn collect_users(note: &Note) -> Vec<&User> {
    let mut out = vec![&note.user];
    if let Some(renote) = &note.renote {
        out.extend(collect_users(renote));
    }
    if let Some(reply) = &note.reply {
        out.extend(collect_users(reply));
    }
    out
}
```

`stub_user_refs`(`renote` ブロックの後に追加):

```rust
    if note_value.get("reply").map(|r| r.is_object()).unwrap_or(false) {
        stub_user_refs(&mut note_value["reply"]);
    }
```

`has_legacy_full_user`(末尾の式を置き換える):

```rust
    note_value.get("renote").map(has_legacy_full_user).unwrap_or(false)
        || note_value.get("reply").map(has_legacy_full_user).unwrap_or(false)
```

`collect_user_id_refs`(`renote` ブロックの後に追加):

```rust
    if let Some(reply) = note_value.get("reply") {
        if reply.is_object() {
            collect_user_id_refs(reply, out);
        }
    }
```

`hydrate_user_refs`(関数末尾の `renote` 再帰部分を次に置き換える。`user` の埋め戻しまでの部分は変更しない):

```rust
    note_value["user"] = serde_json::to_value(user).unwrap_or(serde_json::Value::Null);

    if note_value.get("renote").map(|r| r.is_object()).unwrap_or(false)
        && !hydrate_user_refs(&mut note_value["renote"], users)
    {
        return false;
    }
    // reply は補助表示。参照先ユーザーが欠けていても本体は捨てず、reply だけ落とす
    if note_value.get("reply").map(|r| r.is_object()).unwrap_or(false)
        && !hydrate_user_refs(&mut note_value["reply"], users)
    {
        note_value["reply"] = serde_json::Value::Null;
    }
    true
}
```

関連する doc コメント(「本体+renote分」)も「本体+renote+reply分」に直す。`hydrate_user_refs` の doc には「reply 側の欠落は reply を null にして true を返す」旨を追記する。

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test user_ref:: 2>&1 | tail -10`
Expected: 全て PASS

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/store/user_ref.rs
git commit -m "feat: キャッシュのuser参照処理を返信先にも適用する(#287)"
```

---

### Task 3: バックエンドの自己修復と往復テスト

**Files:**
- Modify: `src-tauri/src/store/note_cache.rs:260-277`(`self_heal_node`)
- Modify: `src-tauri/src/store/postgres_backend.rs:739-756`(`self_heal_node`)、同ファイル `mod tests`
- Modify: `src-tauri/src/store/mysql_backend.rs:662-679`(`self_heal_node`)、同ファイル `mod tests`
- Modify: `src-tauri/src/store/sqlite_backend.rs` の `mod tests`

**Interfaces:**
- Consumes: Task 2 の `reply` 対応済み純粋関数
- Produces: `NoteCacheBackend::cache_note` → `load_cached` の往復で `reply` と `reply.user` が復元される(SQLite / Postgres / MySQL)。

- [ ] **Step 1: 失敗するテストを書く(SQLite)**

`src-tauri/src/store/sqlite_backend.rs` の `mod tests` の `load_cached_skips_note_when_referenced_user_row_missing` の直前に追加:

```rust
    fn note_replying_to_bob(id: &str, created_at: i64) -> Note {
        let mut n = note(id, created_at);
        let mut target = note("n_target", created_at - 100);
        target.user.id = "u_bob".into();
        target.user.username = "bob".into();
        target.user.name = Some("Bob".into());
        target.text = Some("original".into());
        n.reply_id = Some("n_target".into());
        n.reply = Some(Box::new(target));
        n
    }

    #[tokio::test]
    async fn cache_roundtrip_restores_reply_and_its_author() {
        let s = store();
        s.cache_note("col1", &note_replying_to_bob("n_reply", 200)).await.unwrap();

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1);
        let reply = got[0].reply.as_ref().expect("reply should be restored");
        assert_eq!(reply.id, "n_target");
        assert_eq!(reply.text.as_deref(), Some("original"));
        assert_eq!(reply.user.name.as_deref(), Some("Bob"));

        // payload 内では reply.user もスタブ化されていること
        let conn = s.conn().lock().unwrap();
        let raw: String = conn.query_row("SELECT payload FROM note WHERE id = 'n_reply'", [], |r| r.get(0)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["reply"]["user"], serde_json::json!({ "id": "u_bob" }));
    }

    #[tokio::test]
    async fn load_cached_keeps_note_but_drops_reply_when_reply_author_row_missing() {
        let s = store();
        s.cache_note("col1", &note_replying_to_bob("n_reply", 200)).await.unwrap();
        {
            let conn = s.conn().lock().unwrap();
            conn.execute("DELETE FROM user WHERE id = 'u_bob'", []).unwrap();
        }

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1, "返信先ユーザーの欠落でノート行ごと捨ててはいけない");
        assert_eq!(got[0].id, "n_reply");
        assert!(got[0].reply.is_none());
    }

    #[tokio::test]
    async fn load_cached_reads_legacy_payload_without_reply_key() {
        let s = store();
        {
            let conn = s.conn().lock().unwrap();
            let n = note("n_old", 100);
            let mut v = serde_json::to_value(&n).unwrap();
            v["user"] = serde_json::json!({ "id": "u1" });
            v.as_object_mut().unwrap().remove("reply");
            let payload = serde_json::to_string(&v).unwrap();
            conn.execute(
                "INSERT INTO user (id, username, host, name, is_bot, is_cat, followers_count, following_count, notes_count)
                 VALUES ('u1', 'alice', NULL, 'Alice', 0, 0, 0, 0, 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO note (
                    id, created_at, text, text_length, cw, visibility, local_only, user_id,
                    reply_id, reply_user_id, renote_id, channel_id, via, lang,
                    files_count, has_poll, has_link, is_pinned,
                    reaction_count, renote_count, reply_count, my_reaction,
                    is_renoted_by_me, is_favorited_by_me, payload
                ) VALUES ('n_old', 100, '', 0, NULL, 'home', 0, 'u1', NULL, NULL, NULL, NULL, NULL, NULL,
                    0, 0, 0, 0, 0, 0, 0, NULL, 0, 0, ?1)",
                params![payload],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO column_note (column_id, note_id, received_at, created_at) VALUES ('col1', 'n_old', 0, 100)",
                [],
            )
            .unwrap();
        }

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[0].reply.is_none());
    }
```

- [ ] **Step 2: テストの状態を確認する**

Run: `cd src-tauri && cargo test sqlite_backend::tests::cache_roundtrip_restores_reply 2>&1 | tail -15`
Expected: Task 2 実装済みなので、`cache_roundtrip_restores_reply_and_its_author` は **すでに PASS しうる**(SQLite の保存・復元は純粋関数だけで完結するため)。これは想定どおり。往復テストは回帰防止として残す。FAIL した場合は原因を調べる(スタブ化と埋め戻しの非対称が疑わしい)。

- [ ] **Step 3: `self_heal_node` を `reply` に再帰させる(3バックエンド)**

`src-tauri/src/store/note_cache.rs` の `self_heal_node`(`renote` 再帰の直後):

```rust
    if node.get("reply").map(|r| r.is_object()).unwrap_or(false) {
        changed |= self_heal_node(conn, &mut node["reply"])?;
    }
```

doc コメント「1ノード分(本体 or renote)」を「(本体 / renote / reply)」に直し、「renote へ再帰する」を「renote と reply へ再帰する」に直す。

`src-tauri/src/store/postgres_backend.rs` と `src-tauri/src/store/mysql_backend.rs` の `self_heal_node`(`renote` 再帰の直後):

```rust
    if node.get("reply").map(|r| r.is_object()).unwrap_or(false) {
        changed |= Box::pin(self_heal_node(pool, &mut node["reply"])).await?;
    }
```

- [ ] **Step 4: 旧形式の reply が自己修復されるテストを追加する(SQLite)**

`src-tauri/src/store/sqlite_backend.rs` の `mod tests`、Step 1 のテストの後に追加:

```rust
    #[tokio::test]
    async fn load_cached_self_heals_legacy_reply_author() {
        let s = store();
        {
            let conn = s.conn().lock().unwrap();
            let n = note("n_with_reply", 200);
            let mut v = serde_json::to_value(&n).unwrap();
            v["user"] = serde_json::json!({
                "id": "u_main", "username": "mainuser", "host": null, "name": "Main User",
                "avatarUrl": null, "isBot": false, "isCat": false,
                "followersCount": 0, "followingCount": 0, "notesCount": 0,
                "emojis": {}, "bio": null, "bannerUrl": null, "instance": null
            });
            let mut reply = serde_json::to_value(&note("n_target", 100)).unwrap();
            reply["user"] = serde_json::json!({
                "id": "u_target", "username": "target", "host": null, "name": "Target",
                "avatarUrl": null, "isBot": false, "isCat": false,
                "followersCount": 0, "followingCount": 0, "notesCount": 0,
                "emojis": {}, "bio": null, "bannerUrl": null, "instance": null
            });
            v["reply"] = reply;
            let payload = serde_json::to_string(&v).unwrap();
            conn.execute(
                "INSERT INTO note (
                    id, created_at, text, text_length, cw, visibility, local_only, user_id,
                    reply_id, reply_user_id, renote_id, channel_id, via, lang,
                    files_count, has_poll, has_link, is_pinned,
                    reaction_count, renote_count, reply_count, my_reaction,
                    is_renoted_by_me, is_favorited_by_me, payload
                ) VALUES ('n_with_reply', 200, '', 0, NULL, 'home', 0, 'u_main', 'n_target', NULL, NULL, NULL, NULL, NULL,
                    0, 0, 0, 0, 0, 0, 0, NULL, 0, 0, ?1)",
                params![payload],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO column_note (column_id, note_id, received_at, created_at) VALUES ('col1', 'n_with_reply', 0, 200)",
                [],
            )
            .unwrap();
        }

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].reply.as_ref().expect("reply should be present").user.name.as_deref(), Some("Target"));

        let conn = s.conn().lock().unwrap();
        let raw: String =
            conn.query_row("SELECT payload FROM note WHERE id = 'n_with_reply'", [], |r| r.get(0)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["reply"]["user"], serde_json::json!({ "id": "u_target" }));
    }
```

- [ ] **Step 5: SQLite テストを実行する**

Run: `cd src-tauri && cargo test sqlite_backend:: 2>&1 | tail -10` と `cargo test note_cache:: 2>&1 | tail -5`
Expected: 全て PASS

- [ ] **Step 6: Postgres / MySQL の実 DB テストを追加する**

`src-tauri/src/store/postgres_backend.rs` の `mod tests` の `cache_roundtrip_preserves_note_and_order` の直後に追加:

```rust
    #[tokio::test]
    #[ignore]
    async fn cache_roundtrip_restores_reply_and_drops_it_when_author_missing() {
        let s = backend().await;
        let mut n = note("n_reply", 200);
        let mut target = note("n_target", 100);
        target.user.id = "u_bob".into();
        target.user.username = "bob".into();
        target.user.name = Some("Bob".into());
        n.reply_id = Some("n_target".into());
        n.reply = Some(Box::new(target));
        s.cache_note("col1", &n).await.unwrap();

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1);
        let reply = got[0].reply.as_ref().expect("reply should be restored");
        assert_eq!(reply.user.name.as_deref(), Some("Bob"));

        sqlx::query("DELETE FROM \"user\" WHERE id = 'u_bob'").execute(s.pool()).await.unwrap();
        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1, "返信先ユーザーの欠落でノート行ごと捨ててはいけない");
        assert!(got[0].reply.is_none());
    }
```

`src-tauri/src/store/mysql_backend.rs` の `mod tests` の `cache_roundtrip_preserves_note_and_order` 相当の往復テストの直後に、同じテストを追加する。違いは `user` テーブルの引用だけで、MySQL ではバッククォート(`` DELETE FROM `user` WHERE id = 'u_bob' ``)、Postgres ではダブルクォート(`DELETE FROM "user" ...`)を使う。`backend().await` と `s.pool()` は、同ファイルの既存テストが使っているものをそのまま使う(`grep -n "backend().await\|\.pool()" src-tauri/src/store/mysql_backend.rs | head -5` で確認し、名前が違えば合わせる)。

- [ ] **Step 7: 実 DB テストを実行する(Docker が必要)**

```sh
cd src-tauri && cargo test --lib postgres_backend::tests::cache_roundtrip_restores_reply -- --ignored 2>&1 | tail -10
cd src-tauri && cargo test --lib mysql_backend::tests::cache_roundtrip_restores_reply -- --ignored --test-threads=2 2>&1 | tail -10
docker ps -a --filter ancestor=postgres --filter ancestor=mysql:8.1 -q
```
Expected: 両方 PASS。最後のコマンドで孤児コンテナが残っていれば `docker rm -fv <id>` で消す。

- [ ] **Step 8: Rust 全体の回帰確認とコミット**

Run: `cd src-tauri && cargo test 2>&1 | tail -8`
Expected: 全て PASS

```bash
git add src-tauri/src/store
git commit -m "feat: キャッシュの自己修復と往復テストを返信先に対応する(#287)"
```

---

### Task 4: 1行プレビュー本文の決定ロジック

**Files:**
- Create: `frontend/src/lib/replyPreview.ts`
- Test: `frontend/src/lib/replyPreview.test.ts`

**Interfaces:**
- Consumes: `Note`(`frontend/src/bindings/tauri.gen.ts`、Task 1 で `reply` 追加済み)
- Produces:
  ```ts
  export type ReplyPreviewBody =
    | { kind: "cw" | "text"; text: string }
    | { kind: "label"; label: string };
  export function replyPreviewBody(reply: Note): ReplyPreviewBody | null;
  ```

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/replyPreview.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { DriveFile, Note } from "../bindings/tauri.gen";
import { replyPreviewBody } from "./replyPreview";

function file(mimeType: string): DriveFile {
  return {
    id: "f1",
    mimeType,
    isSensitive: false,
    url: "https://example.com/f",
    thumbnailUrl: null,
    name: "f",
    size: 1,
  };
}

function partial(overrides: Partial<Note>): Note {
  return {
    text: null,
    cw: null,
    files: [],
    renote: null,
    ...overrides,
  } as Note;
}

describe("replyPreviewBody", () => {
  it("returns the text when there is no cw", () => {
    expect(replyPreviewBody(partial({ text: "こんにちは" }))).toEqual({ kind: "text", text: "こんにちは" });
  });

  it("prefers the cw over the text", () => {
    expect(replyPreviewBody(partial({ cw: "ネタバレ注意", text: "本文" }))).toEqual({
      kind: "cw",
      text: "ネタバレ注意",
    });
  });

  it("labels image-only notes as (画像)", () => {
    expect(replyPreviewBody(partial({ files: [file("image/png"), file("image/jpeg")] }))).toEqual({
      kind: "label",
      label: "(画像)",
    });
  });

  it("labels notes with a non-image file as (ファイル)", () => {
    expect(replyPreviewBody(partial({ files: [file("image/png"), file("video/mp4")] }))).toEqual({
      kind: "label",
      label: "(ファイル)",
    });
  });

  it("labels a pure renote as (Renote)", () => {
    expect(replyPreviewBody(partial({ renote: {} as Note }))).toEqual({ kind: "label", label: "(Renote)" });
  });

  it("treats whitespace-only text as empty", () => {
    expect(replyPreviewBody(partial({ text: "  \n " }))).toBeNull();
  });

  it("returns null when there is nothing to show", () => {
    expect(replyPreviewBody(partial({}))).toBeNull();
  });
});
```

`DriveFile` の実フィールドは `grep -n "export type DriveFile" -A12 frontend/src/bindings/tauri.gen.ts` で確認し、`file()` の戻り値をその型に合わせる(足りない/余るフィールドがあれば直す)。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/replyPreview.test.ts 2>&1 | tail -10`
Expected: FAIL(`Cannot find module './replyPreview'`)

- [ ] **Step 3: 実装する**

`frontend/src/lib/replyPreview.ts`:

```ts
import type { Note } from "../bindings/tauri.gen";

export type ReplyPreviewBody =
  | { kind: "cw" | "text"; text: string }
  | { kind: "label"; label: string };

/**
 * 返信先の1行プレビューに出す本文を決める。
 * CW があれば本文より優先し(CW 配下の本文を隠す意図を尊重する)、本文が無ければ
 * ファイル/Renote の種別ラベルにする。何も出せなければ null(呼び出し側は表示名だけにする)。
 */
export function replyPreviewBody(reply: Note): ReplyPreviewBody | null {
  if (reply.cw?.trim()) return { kind: "cw", text: reply.cw };
  if (reply.text?.trim()) return { kind: "text", text: reply.text };
  if (reply.files.length > 0) {
    const allImages = reply.files.every((f) => f.mimeType.startsWith("image/"));
    return { kind: "label", label: allImages ? "(画像)" : "(ファイル)" };
  }
  if (reply.renote) return { kind: "label", label: "(Renote)" };
  return null;
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/replyPreview.test.ts 2>&1 | tail -10`
Expected: PASS(7件)

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/replyPreview.ts frontend/src/lib/replyPreview.test.ts
git commit -m "feat: 返信先プレビュー本文の決定ロジックを追加する(#287)"
```

---

### Task 5: `NoteCard.svelte` の1行プレビューと展開

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`(import、script、358-362行のバナー)
- Test: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: `replyPreviewBody`(Task 4)、`Note.reply`(Task 1)
- Produces: `data-testid="reply-preview"` の `button`(`aria-expanded` を持つ)。展開時はその直後に返信先の `NoteCard` が描画される。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` の `describe("NoteCard action banner", ...)` ブロックの末尾(最後の `it` の後ろ、閉じ `});` の前)に追加:

```ts
  describe("reply target preview", () => {
    const target = () =>
      makeNote({
        id: "parent1",
        text: "元のノートの本文",
        user: makeUser({ id: "u2", name: "Bob", username: "bob" }),
      });

    it("shows the target author and a body excerpt on one line", () => {
      const note = makeNote({ replyId: "parent1", reply: target() });
      const { getByTestId } = render(NoteCard, { props: { note } });
      const preview = getByTestId("reply-preview");
      expect(preview.textContent).toContain("Bob");
      expect(preview.textContent).toContain("元のノートの本文");
      expect(preview.getAttribute("aria-expanded")).toBe("false");
    });

    it("shows the cw instead of the body when the target has a cw", () => {
      const note = makeNote({
        replyId: "parent1",
        reply: makeNote({ id: "parent1", cw: "ネタバレ", text: "秘密の本文" }),
      });
      const { getByTestId } = render(NoteCard, { props: { note } });
      const text = getByTestId("reply-preview").textContent ?? "";
      expect(text).toContain("ネタバレ");
      expect(text).not.toContain("秘密の本文");
    });

    it("shows (画像) when the target only has image files", () => {
      const note = makeNote({
        replyId: "parent1",
        reply: makeNote({
          id: "parent1",
          text: null,
          files: [
            {
              id: "f1",
              mimeType: "image/png",
              isSensitive: false,
              url: "https://example.com/a.png",
              thumbnailUrl: null,
              name: "a.png",
              size: 1,
            },
          ],
        }),
      });
      const { getByTestId } = render(NoteCard, { props: { note } });
      expect(getByTestId("reply-preview").textContent).toContain("(画像)");
    });

    it("falls back to the plain reply label when the target is unavailable", () => {
      const note = makeNote({ replyId: "parent1", reply: null });
      const { getByText, queryByTestId } = render(NoteCard, { props: { note } });
      expect(getByText("返信")).toBeTruthy();
      expect(queryByTestId("reply-preview")).toBeNull();
    });

    it("hides the preview when hideActionBanner is set", () => {
      const note = makeNote({ replyId: "parent1", reply: target() });
      const { queryByTestId } = render(NoteCard, { props: { note, hideActionBanner: true } });
      expect(queryByTestId("reply-preview")).toBeNull();
    });

    it("expands the target note on click and collapses on the second click", async () => {
      const note = makeNote({ replyId: "parent1", reply: target() });
      const { getByTestId, container } = render(NoteCard, { props: { note } });
      const preview = getByTestId("reply-preview");
      expect(container.querySelectorAll("article").length).toBe(1);

      await fireEvent.click(preview);
      expect(preview.getAttribute("aria-expanded")).toBe("true");
      expect(container.querySelectorAll("article").length).toBe(2);

      await fireEvent.click(preview);
      expect(preview.getAttribute("aria-expanded")).toBe("false");
      expect(container.querySelectorAll("article").length).toBe(1);
    });

    it("does not offer a nested preview inside the expanded target", async () => {
      const nested = makeNote({ id: "parent1", replyId: "grand", reply: makeNote({ id: "grand" }) });
      const note = makeNote({ replyId: "parent1", reply: nested });
      const { getByTestId, getAllByTestId } = render(NoteCard, { props: { note } });
      await fireEvent.click(getByTestId("reply-preview"));
      expect(getAllByTestId("reply-preview").length).toBe(1);
    });
  });
```

ファイル先頭の import を `import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";` に直す(`fireEvent` を追加)。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/NoteCard.test.ts -t "reply target preview" 2>&1 | tail -20`
Expected: FAIL(`reply-preview` が見つからない)

- [ ] **Step 3: 実装する**

`frontend/src/ui/NoteCard.svelte`:

import に追加(`../lib/userDisplay` の import の近く):

```ts
  import { replyPreviewBody } from "../lib/replyPreview";
```

script に追加(`inner` の定義の直後):

```ts
  // 返信先の1行プレビュー(Issue #287)。reply が無ければ従来の「↩ 返信」だけを出す。
  const replyTarget = $derived(inner.reply ?? null);
  const replyPreview = $derived(replyTarget ? replyPreviewBody(replyTarget) : null);
  let replyExpanded = $state(false);
```

358-362行の既存バナーを次に置き換える:

```svelte
  {#if inner.replyId && !hideActionBanner}
    {#if replyTarget}
      <button
        type="button"
        class="mb-0.5 flex w-full min-w-0 cursor-pointer items-center gap-1 border-0 bg-transparent p-0 text-left text-xs text-[var(--info)]"
        data-testid="reply-preview"
        aria-expanded={replyExpanded}
        onclick={() => (replyExpanded = !replyExpanded)}
      >
        <Reply size={12} class="flex-none" />
        <span class="min-w-0 truncate">
          <Mfm
            text={displayName(replyTarget.user)}
            emojis={proxiedEmojiMap(replyTarget.user.emojis, instanceHost)}
            simple
          />{#if replyPreview}:
            {#if replyPreview.kind === "label"}
              {replyPreview.label}
            {:else}
              <Mfm text={replyPreview.text} emojis={proxiedEmojiMap(replyTarget.emojis, instanceHost)} simple />
            {/if}
          {/if}
        </span>
      </button>
      {#if replyExpanded}
        <Self note={replyTarget} quoted hideReactions hideActionBanner emojiAccountId={emojiAcct} />
      {/if}
    {:else}
      <div class="mb-0.5 inline-flex items-center gap-1 text-xs text-[var(--info)]">
        <Reply size={12} /> 返信
      </div>
    {/if}
  {/if}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/NoteCard.test.ts 2>&1 | tail -20`
Expected: 全て PASS(既存の「返信」バナーのテストも PASS)

- [ ] **Step 5: 型チェックと全テスト**

Run: `cd frontend && pnpm check 2>&1 | tail -10` と `cd frontend && pnpm test 2>&1 | tail -8`
Expected: エラーなし、全て PASS

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: 返信ノートに返信先の1行プレビューと展開を追加する(#287)"
```

---

### Task 6: ドキュメントと最終検証

**Files:**
- Modify: `docs/guide/user-guide.md`
- Modify: `docs/design/misskey-multicolumn-client-design.md:162` 付近(`Note` 定義)

**Interfaces:**
- Consumes: Task 1〜5 の完了
- Produces: なし

- [ ] **Step 1: 設計書の `Note` 定義を更新する**

`docs/design/misskey-multicolumn-client-design.md` の `pub reply_id: Option<String>,`(162行付近)の直後に追加:

```rust
    /// 返信先ノート(浅く1階層のみ。表示専用でフィルタ評価には使わない)
    pub reply: Option<Box<Note>>,
```

周辺に `reply_user_id` があれば、その後ろに置く。

- [ ] **Step 2: ユーザーガイドに追記する**

`docs/guide/user-guide.md` で、ノート表示/操作を説明している節(`grep -n "Renote\|引用" docs/guide/user-guide.md | head` で見つける)に、次の趣旨の段落を追記する:

```markdown
返信ノートの上には、返信先の投稿者名と本文の冒頭が1行で表示されます(例: 「↩ Bob: 元のノートの本文…」)。返信先にCWがある場合は本文の代わりにCWの文言、本文がなく画像だけの場合は「(画像)」と表示されます。この行をクリックすると返信先のノートがその場に展開され、もう一度クリックすると折りたたまれます。返信先が削除されている・閲覧できないなどで取得できない場合は、従来どおり「↩ 返信」とだけ表示されます。
```

- [ ] **Step 3: 全体の自動検証**

```sh
cd src-tauri && cargo test 2>&1 | tail -8
cd frontend && pnpm check 2>&1 | tail -5
cd frontend && pnpm test 2>&1 | tail -5
git status --short
```
Expected: 全て PASS。`git status` に `tauri.gen.ts` 以外の予期しない変更がない(`tauri.gen.ts` は Task 1 でコミット済み)。

- [ ] **Step 4: 実画面確認(Xvfb 越しの隔離インスタンス)**

Global Constraints の手順に従い、実際の返信ノートで次を目視確認する(スクリーンショットを撮って確認):

1. 返信ノートの上に「↩ 表示名: 本文冒頭…」が1行(はみ出さず省略記号付き)で出る。
2. 返信先がCW付きのとき、CW文言が出る。
3. クリックで返信先が `quoted` 表示で展開し、もう一度クリックで折りたたまれる。
4. 展開した返信先にバナーが出ない。
5. 既存のキャッシュ(`reply` を持たない旧行)の返信ノートは「↩ 返信」のまま表示され、エラーにならない。
6. 起動したプロセス・一時ディレクトリを正確な PID で終了・削除した。

実機の `~/.cache/com.onodai.tsumugi/cache.db` は触らない。確認できなかった項目があれば、完了報告で「未確認」と明記する。

- [ ] **Step 5: コミット**

```bash
git add docs
git commit -m "docs: 返信先プレビューの説明を追記する(#287)"
```

- [ ] **Step 6: PR 作成(ユーザーの指示があるときのみ)**

push と PR 作成はユーザーが指示したときに行う。PR 本文は `.github/pull_request_template.md` の構成に従い、`Fixes #287` を含め、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。マージは `gh pr merge --merge`(squash しない)。push 後に CI を待つ Monitor は使わない。
