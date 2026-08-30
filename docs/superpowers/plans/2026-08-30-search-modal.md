# 検索機能（キャッシュDB検索）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #248 対応。既存のTQL `cache` ソース（バックエンドは既に配線済み）にガイドUIで到達できる専用検索モーダルを追加する。

**Architecture:** バックエンドは既存の `filter/sql.rs`（TQL→SQL射影）・`store/note_cache.rs::search_cache()`（SQLite検索）をそのまま使い、特定カラムに紐づかない一回性検索用の新規コマンド `search_cache_notes` を追加する。フロントエンドはキーワード/ユーザー/インスタンス/日時の固定フィールドから TQL where句を組み立てる「簡単」モードと、生TQLを書ける「エキスパート」モードを持つ `SearchModal.svelte` を新設し、`FollowListModal.svelte` と同じ無限スクロールパターンで結果を表示する。

**Tech Stack:** Rust (Tauri v2, rusqlite) / Svelte 5 (runes) / vitest + @testing-library/svelte

## Global Constraints

- ブランチ `feature/issue-248-search` 上で作業する（設計書は既に `docs/superpowers/specs/2026-08-30-search-modal-design.md` としてこのブランチにコミット済み）。
- `mine` / `following` / `@account` 系のTQL述語は、検索が特定アカウントに紐づかないため常に不一致になる既知の制約（YAGNI、対応しない）。
- 「検索条件を保存してカラム化」等の追加機能はスコープ外。
- 各タスクの最後に `git commit`（コミットメッセージは subject 行のみ、本文なし）。

---

### Task 1: バックエンド — `search_cache_notes` コマンド

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/lib.rs:68`（specta_builder への登録）

**Interfaces:**
- Produces:
  - `fn search_cache_core(cache: &NoteCacheStore, filter: &FilterQuery, eval_ctx: &EvalContext, mute: &MuteConfig, until_id: Option<&str>, limit: u32, is_server_muted: impl Fn(&Note) -> bool) -> Result<Vec<Note>>`（純粋関数、AppStateを取らないので単体テスト可能）
  - `#[tauri::command] async fn search_cache_notes(state: State<'_, AppState>, account_id: String, filter: FilterQuery, until_id: Option<String>, limit: u32) -> Result<Vec<Note>>`（コマンド、TS側からは `commands.searchCacheNotes(accountId, filter, untilId, limit)` として呼べるようになる）

- [ ] **Step 1: import を追加する**

`src-tauri/src/commands/column.rs` の先頭 import ブロックを以下に置き換える:

```rust
use crate::api::meta::{fetch_antennas, fetch_followed_channels, fetch_user_lists, resolve_user};
use crate::api::notes::fetch_notes;
use crate::api::notifications::fetch_notifications;
use crate::domain::{
    Column, ColumnGroup, ColumnKind, FilterQuery, MuteConfig, Note, Notification, PaneNode,
    SourceItem, SplitDirection, User, UserList,
};
use crate::error::{Error, Result};
use crate::filter::{ast, eval::EvalContext, parser, sql, CompiledFilter};
use crate::state::AppState;
use crate::store::NoteCacheStore;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event as _;
```

（`MuteConfig` / `eval::EvalContext` / `crate::store::NoteCacheStore` の3つを追加しただけで、他の行は変更しない。）

- [ ] **Step 2: 失敗するテストを書く**

`src-tauri/src/commands/column.rs` の `#[cfg(test)] mod tests` ブロック内、既存の `note()` ヘルパーの直後（`finalize_gap_fill_marks_truncated_when_target_not_reached` の前）に追加する:

```rust
    fn cache_with(notes: &[Note]) -> NoteCacheStore {
        let store = NoteCacheStore::new(crate::store::db::open_cache_in_memory().unwrap());
        store.cache_notes("col1", notes).unwrap();
        store
    }

    #[test]
    fn search_cache_core_filters_by_tql_predicate_and_orders_desc() {
        let mut n1 = note("n1", 100);
        n1.text = Some("hello needle".into());
        let mut n2 = note("n2", 200);
        n2.text = Some("hello world".into());
        let mut n3 = note("n3", 300);
        n3.text = Some("needle again".into());
        let cache = cache_with(&[n1, n2, n3]);

        let filter = FilterQuery::Tql("text -> \"needle\"".into());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |_| false,
        )
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n3", "n1"]);
    }

    #[test]
    fn search_cache_core_with_empty_predicate_returns_all_desc_order() {
        let cache = cache_with(&[note("n1", 100), note("n2", 300), note("n3", 200)]);

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |_| false,
        )
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n3", "n1"]);
    }

    #[test]
    fn search_cache_core_excludes_locally_muted_notes() {
        let mut n1 = note("n1", 100);
        n1.text = Some("spoiler content".into());
        let cache = cache_with(&[n1, note("n2", 200)]);

        let filter = FilterQuery::Tql(String::new());
        let mute = MuteConfig { ng_words: vec!["spoiler".into()], ..Default::default() };
        let got = search_cache_core(&cache, &filter, &EvalContext::default(), &mute, None, 10, |_| false)
            .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);
    }

    #[test]
    fn search_cache_core_excludes_notes_the_closure_marks_server_muted() {
        let cache = cache_with(&[note("n1", 100), note("n2", 200)]);

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            None,
            10,
            |n| n.id == "n2",
        )
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"]);
    }

    #[test]
    fn search_cache_core_respects_until_id_boundary() {
        let cache = cache_with(&[note("n1", 100), note("n2", 200), note("n3", 300)]);

        let filter = FilterQuery::Tql(String::new());
        let got = search_cache_core(
            &cache,
            &filter,
            &EvalContext::default(),
            &MuteConfig::default(),
            Some("n3"),
            10,
            |_| false,
        )
        .unwrap();

        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n1"]);
    }
```

- [ ] **Step 3: テストを実行し失敗を確認する**

Run: `cd src-tauri && cargo test search_cache_core`
Expected: コンパイルエラー（`search_cache_core` が未定義、`cache_notes`/`EvalContext` 等は既存なので `search_cache_core` 未定義由来のエラーのみ）。

- [ ] **Step 4: `search_cache_core` とコマンドを実装する**

`src-tauri/src/commands/column.rs` の `fetch_and_filter` 関数の直前（`INITIAL_LIMIT` 等の定数より後、`async fn fetch_and_filter` の直前）に追加する:

```rust
/// キャッシュDB検索(Issue #248)の中核ロジック。SQL射影で粗く絞り込んだ後、
/// `fetch_and_filter` の cache 経路と同じ二段構成(in-memory フィルタ + ミュート除外)で
/// 再検証する。AppState を直接取らず必要な値だけを受け取ることで単体テスト可能にしている。
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

/// 検索モーダル(Issue #248)専用: 特定カラムに紐づかない一回性のキャッシュDB検索。
/// `filter` は cache ソースの where 句のみを渡す(source節は無し、常にキャッシュ全体が対象)。
#[tauri::command]
#[specta::specta]
pub async fn search_cache_notes(
    state: State<'_, AppState>,
    account_id: String,
    filter: FilterQuery,
    until_id: Option<String>,
    limit: u32,
) -> Result<Vec<Note>> {
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
}
```

- [ ] **Step 5: テストを実行し成功を確認する**

Run: `cd src-tauri && cargo test search_cache_core`
Expected: 5件とも PASS。

- [ ] **Step 6: コマンドを specta_builder に登録する**

`src-tauri/src/lib.rs:68` 付近、`commands::column::update_column,` の直後に1行追加する:

```rust
            commands::column::update_column,
            commands::column::search_cache_notes,
            commands::note::post_note,
```

- [ ] **Step 7: TSバインディングを再生成しビルドを確認する**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS（`generates_frontend_bindings` テストが `frontend/src/bindings/tauri.gen.ts` を再生成し、`commands.searchCacheNotes` が追加されていることを確認する）。

- [ ] **Step 8: コミット**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: search_cache_notesコマンドを追加(#248)"
```

---

### Task 2: フロントエンド — `app.searchCacheNotes` ストアメソッド

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.searchCacheNotes(accountId: string, filter: FilterQuery, untilId: string | null, limit: number)`（Task 1で生成される）
- Produces: `async searchCacheNotes(accountId: string, filter: FilterQuery, untilId?: string, limit = 20): Promise<Note[]>`

- [ ] **Step 1: メソッドを追加する**

`frontend/src/lib/store.svelte.ts` の `getUserFollowing` メソッドの直後（`}` の後、`/// 通知設定を保存。` コメントの前）に追加する:

```ts
  /// 検索モーダル(Issue #248)用: 特定カラムに紐づかないキャッシュDB検索。
  /// 呼び出し元(SearchModal)が自前のエラー表示を持つため this.#fail()（バナー表示）は呼ばない。
  async searchCacheNotes(accountId: string, filter: FilterQuery, untilId?: string, limit = 20) {
    try {
      return await unwrapAcc(
        accountId,
        commands.searchCacheNotes(accountId, filter, untilId ?? null, limit),
      );
    } catch (e) {
      this.#logFailure(e);
      throw e;
    }
  }
```

- [ ] **Step 2: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー0件（`FilterQuery` は既に import 済み、`commands.searchCacheNotes` はTask 1で生成済みバインディングに存在する）。

- [ ] **Step 3: コミット**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: app.searchCacheNotesストアメソッドを追加(#248)"
```

---

### Task 3: フロントエンド — `SearchModal.svelte`

**Files:**
- Create: `frontend/src/ui/SearchModal.svelte`
- Test: `frontend/src/ui/SearchModal.test.ts`

**Interfaces:**
- Consumes: `app.accounts: Account[]`, `app.defaultAccountId(): string`, `app.validateFilter(filter: FilterQuery): Promise<string | null>`, `app.searchCacheNotes(accountId, filter, untilId?, limit?): Promise<Note[]>`（Task 2）, `AccountSelect.svelte`, `TqlCompletionField.svelte`（`mode="predicate"`）, `NoteCard.svelte`, `Modal.svelte`
- Produces: `SearchModal` コンポーネント、props `{ onclose: () => void }`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/SearchModal.test.ts` を新規作成する:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { Account, Note, User } from "../bindings/tauri.gen";
import { app } from "../lib/store.svelte";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
const invokeMock = vi.fn().mockResolvedValue(null);
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: SearchModal } = await import("./SearchModal.svelte");

function makeAccount(): Account {
  return {
    id: "acc1",
    host: "misskey.io",
    username: "alice",
    userId: "u1",
    displayName: "Alice",
    avatarUrl: null,
  };
}

function makeUser(): User {
  return {
    id: "u1",
    username: "alice",
    host: null,
    name: "Alice",
    avatarUrl: null,
    isBot: false,
    isCat: false,
    followersCount: 0,
    followingCount: 0,
    notesCount: 0,
  };
}

function makeNote(id: string, createdAt: number): Note {
  return {
    id,
    createdAt,
    text: "hello",
    cw: null,
    visibility: "public",
    localOnly: false,
    user: makeUser(),
    replyId: null,
    renoteId: null,
    renote: null,
    files: [],
    poll: null,
    tags: [],
    mentions: [],
    emojis: {},
    channelId: null,
    via: null,
    lang: null,
    reactions: {},
    reactionCount: 0,
    renoteCount: 0,
    replyCount: 0,
    myReaction: null,
    isRenotedByMe: false,
    isFavoritedByMe: false,
    isPinned: false,
  };
}

afterEach(() => {
  cleanup();
  invokeMock.mockClear();
  app.accounts = [];
});

describe("SearchModal", () => {
  it("キーワード/ユーザーの入力から組み立てたTQLでsearch_cache_notesを呼ぶ", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "search_cache_notes") return Promise.resolve([makeNote("n1", 100)]);
      return Promise.resolve(null);
    });
    const { getByPlaceholderText, getByTestId, getByText } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.input(getByPlaceholderText("本文に含まれる語"), { target: { value: "rust" } });
    await fireEvent.input(getByPlaceholderText("@user@host"), { target: { value: "@bob@example.com" } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() => expect(getByText("hello")).toBeTruthy());
    expect(invokeMock).toHaveBeenCalledWith(
      "search_cache_notes",
      expect.objectContaining({
        accountId: "acc1",
        filter: { kind: "tql", value: 'text -> "rust" && user.acct == "@bob@example.com"' },
        untilId: null,
        limit: 20,
      }),
    );
  });

  it("条件を何も入れずに検索すると空のTQL(全件)で呼び、0件なら該当なしを表示する", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "search_cache_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByTestId, getByText } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_cache_notes",
        expect.objectContaining({ filter: { kind: "tql", value: "" } }),
      ),
    );
    await waitFor(() => expect(getByText("該当するノートが見つかりませんでした")).toBeTruthy());
  });

  it("末尾までスクロールすると最後のノートIDをuntilIdにして追加取得する", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "search_cache_notes" && args?.untilId == null) {
        return Promise.resolve([makeNote("n1", 200)]);
      }
      if (cmd === "search_cache_notes" && args?.untilId === "n1") {
        return Promise.resolve([makeNote("n2", 100)]);
      }
      return Promise.resolve([]);
    });
    const { getByTestId } = render(SearchModal, { props: { onclose: () => {} } });
    await fireEvent.click(getByTestId("search-submit"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());

    const list = document.querySelector('[data-testid="search-results-scroll"]') as HTMLElement;
    Object.defineProperty(list, "scrollTop", { value: 500, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(list, "scrollHeight", { value: 1200, configurable: true });
    await fireEvent.scroll(list);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_cache_notes",
        expect.objectContaining({ untilId: "n1" }),
      ),
    );
  });

  it("エキスパートモードでは組み立てたTQLではなく入力したTQLをそのまま送る", async () => {
    app.accounts = [makeAccount()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "search_cache_notes") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    const { getByText, getByPlaceholderText, getByTestId } = render(SearchModal, {
      props: { onclose: () => {} },
    });
    await fireEvent.click(getByText("エキスパート(TQL)"));
    const tqlField = getByPlaceholderText(/has_files/);
    await fireEvent.input(tqlField, { target: { value: "has_files" } });
    await waitFor(() => expect(tqlField).toHaveValue("has_files"));
    await fireEvent.click(getByTestId("search-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "search_cache_notes",
        expect.objectContaining({ filter: { kind: "tql", value: "has_files" } }),
      ),
    );
  });
});
```

- [ ] **Step 2: テストを実行し失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/SearchModal.test.ts`
Expected: `Failed to resolve import "./SearchModal.svelte"` で失敗。

- [ ] **Step 3: `SearchModal.svelte` を実装する**

`frontend/src/ui/SearchModal.svelte` を新規作成する:

```svelte
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import AccountSelect from "./AccountSelect.svelte";
  import TqlCompletionField from "../input/TqlCompletionField.svelte";
  import NoteCard from "./NoteCard.svelte";
  import Modal from "./Modal.svelte";
  import { Button } from "$lib/components/ui/button";
  import type { FilterQuery, Note } from "../bindings/tauri.gen";

  let { onclose }: { onclose: () => void } = $props();

  let uiMode = $state<"guided" | "expert">("guided");
  let accountId = $state(app.defaultAccountId());
  let keyword = $state("");
  let userAcct = $state("");
  let host = $state("");
  let dateFrom = $state("");
  let dateTo = $state("");
  let tqlText = $state("");
  let tqlErr = $state<string | null>(null);

  let notes = $state<Note[]>([]);
  let busy = $state(false);
  let done = $state(false);
  let err = $state<string | null>(null);
  let searched = $state(false);
  let requestGen = 0;

  // AddColumnModal.svelte の tqlStr() と同じエスケープ規則（本家パーサの読み方に合わせる）
  function tqlStr(s: string): string {
    return `"${s.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  }

  // ガイドモードの固定フィールドから TQL の where 句を組み立てる。空欄の項目は述語を出さない。
  function guidedPredicate(): string {
    const parts: string[] = [];
    if (keyword.trim()) parts.push(`text -> ${tqlStr(keyword.trim())}`);
    if (userAcct.trim()) parts.push(`user.acct == ${tqlStr(userAcct.trim())}`);
    if (host.trim()) parts.push(`host == ${tqlStr(host.trim())}`);
    if (dateFrom) parts.push(`created_at >= ${Math.floor(new Date(dateFrom).getTime() / 1000)}`);
    if (dateTo) parts.push(`created_at <= ${Math.floor(new Date(dateTo).getTime() / 1000)}`);
    return parts.join(" && ");
  }

  function currentPredicate(): string {
    return uiMode === "expert" ? tqlText.trim() : guidedPredicate();
  }

  // 簡単→エキスパートへ切替た時、まだ何も書いていなければ今の選択内容を反映する
  // (AddColumnModal.svelte の switchToExpert() と同じパターン)。
  function switchToExpert() {
    if (!tqlText.trim()) tqlText = guidedPredicate();
    uiMode = "expert";
  }

  async function onTqlInput() {
    if (!tqlText.trim()) {
      tqlErr = null;
      return;
    }
    tqlErr = await app.validateFilter({ kind: "tql", value: tqlText });
  }

  async function loadMore() {
    if (busy || done) return;
    busy = true;
    err = null;
    const myGen = requestGen;
    try {
      const untilId = notes.length > 0 ? notes[notes.length - 1].id : undefined;
      const filter: FilterQuery = { kind: "tql", value: currentPredicate() };
      const page = await app.searchCacheNotes(accountId, filter, untilId, 20);
      if (myGen !== requestGen) return;
      if (page.length === 0) done = true;
      notes = [...notes, ...page];
    } catch (e) {
      if (myGen !== requestGen) return;
      err = String(e);
    } finally {
      if (myGen === requestGen) busy = false;
    }
  }

  function runSearch(e: Event) {
    e.preventDefault();
    if (uiMode === "expert" && tqlErr) return;
    requestGen++;
    notes = [];
    busy = false;
    done = false;
    err = null;
    searched = true;
    void loadMore();
  }

  // FollowListModal.svelte の onScroll() と同じ「残り300px」判定。
  function onScroll(e: Event) {
    if (err) return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 300) {
      void loadMore();
    }
  }
</script>

<Modal title="検索" {onclose} width="560px">
  <form onsubmit={runSearch} class="flex flex-col gap-2.5">
    <div class="flex flex-col gap-1 text-sm">
      <span class="text-muted-foreground">アカウント（検索結果の操作に使用。検索条件には影響しません）</span>
      <AccountSelect bind:value={accountId} accounts={app.accounts} showLabel />
    </div>

    <div class="flex items-center gap-0 self-start overflow-hidden rounded-lg border border-border text-sm">
      <button
        type="button"
        class={uiMode === "guided"
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-primary-foreground"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-foreground"}
        onclick={() => (uiMode = "guided")}
      >簡単</button>
      <button
        type="button"
        class={uiMode === "expert"
          ? "bg-primary px-3.5 py-1.5 text-primary-foreground"
          : "bg-muted px-3.5 py-1.5 text-foreground"}
        onclick={switchToExpert}
      >エキスパート(TQL)</button>
    </div>

    {#if uiMode === "guided"}
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">キーワード</span>
        <input
          class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
          placeholder="本文に含まれる語"
          bind:value={keyword}
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">ユーザー</span>
        <input
          class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
          placeholder="@user@host"
          bind:value={userAcct}
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">インスタンス</span>
        <input
          class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
          placeholder="misskey.io（ローカルは空欄）"
          bind:value={host}
        />
      </label>
      <div class="flex gap-2.5">
        <label class="flex flex-1 flex-col gap-1 text-sm">
          <span class="text-muted-foreground">日時（開始）</span>
          <input
            type="datetime-local"
            class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            bind:value={dateFrom}
          />
        </label>
        <label class="flex flex-1 flex-col gap-1 text-sm">
          <span class="text-muted-foreground">日時（終了）</span>
          <input
            type="datetime-local"
            class="rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground"
            bind:value={dateTo}
          />
        </label>
      </div>
    {:else}
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-muted-foreground">TQL（cacheソースのwhere句。空欄で全件）</span>
        <TqlCompletionField
          mode="predicate"
          bind:value={tqlText}
          placeholder={'例: has_files && user.acct == "@alice@misskey.io"'}
          invalid={!!tqlErr}
          oninput={onTqlInput}
        />
      </label>
      {#if tqlErr}<p class="mb-0 mt-0 text-sm text-destructive break-words">TQLエラー: {tqlErr}</p>{/if}
    {/if}

    <Button type="submit" disabled={busy || (uiMode === "expert" && !!tqlErr)} data-testid="search-submit"
      >検索</Button
    >
  </form>

  <div class="-mx-4 mt-3 mb-0 max-h-[50vh] overflow-y-auto" data-testid="search-results-scroll" onscroll={onScroll}>
    {#each notes as note (note.id)}
      <NoteCard {note} {accountId} />
    {/each}
    {#if busy}<p class="px-4 py-2.5 text-center text-sm text-muted-foreground">読み込み中…</p>{/if}
    {#if searched && !busy && notes.length === 0 && !err}
      <p class="px-4 py-2.5 text-center text-sm text-muted-foreground">該当するノートが見つかりませんでした</p>
    {/if}
  </div>
  {#if err}
    <p class="mt-2 mb-0 text-sm text-destructive">{err}</p>
    <Button variant="outline" size="sm" class="mt-2" onclick={loadMore} disabled={busy}>再試行</Button>
  {/if}
</Modal>
```

- [ ] **Step 4: テストを実行し成功を確認する**

Run: `cd frontend && pnpm vitest run src/ui/SearchModal.test.ts`
Expected: 4件とも PASS。失敗する場合、多くは非同期タイミング起因（`waitFor` の追加）なのでテストコードの `await` 箇所を調整する。実装（`SearchModal.svelte`）側のロジックは変更しない。

- [ ] **Step 5: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー0件。

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/SearchModal.svelte frontend/src/ui/SearchModal.test.ts
git commit -m "feat: SearchModalコンポーネントを追加(#248)"
```

---

### Task 4: 導線をAppMenu/App.svelteに配線する

**Files:**
- Modify: `frontend/src/ui/AppMenu.svelte`
- Modify: `frontend/src/App.svelte`

**Interfaces:**
- Consumes: `SearchModal`（Task 3）
- Produces: `AppMenu` に `onOpenSearch: () => void` prop を追加。既存の `onAddColumn` / `onOpenSettings` と並ぶ第3のメニュー項目。

このタスクは既存コードへの配線のみで新規ロジックを持たないため、`AppMenu.svelte`/`App.svelte` 自体に既存の専用テストが無いという今のコードベースの慣習に倣い、自動テストは追加しない（Task 5 の手動確認でカバーする）。

- [ ] **Step 1: `AppMenu.svelte` にメニュー項目を追加する**

`frontend/src/ui/AppMenu.svelte` の import 行を置き換える:

```svelte
  import { Menu, Plus, Search, Settings } from "@lucide/svelte";
```

`let { onAddColumn, onOpenSettings }: { onAddColumn: () => void; onOpenSettings: () => void } = $props();` を置き換える:

```svelte
  let {
    onAddColumn,
    onOpenSearch,
    onOpenSettings,
  }: { onAddColumn: () => void; onOpenSearch: () => void; onOpenSettings: () => void } = $props();
```

「カラム追加」ボタンと「設定」ボタンの間に、以下のボタンを追加する:

```svelte
      <button
        type="button"
        role="menuitem"
        class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
        data-testid="app-menu-search"
        onclick={() => pick(onOpenSearch)}
      >
        <Search size={16} /> 検索
      </button>
```

- [ ] **Step 2: `App.svelte` に `SearchModal` を配線する**

`frontend/src/App.svelte` の `import AddColumnModal from "./ui/AddColumnModal.svelte";` の直後に追加する:

```svelte
  import SearchModal from "./ui/SearchModal.svelte";
```

`let showAddColumn = $state(false);` の直後に追加する:

```svelte
  let showSearch = $state(false);
```

`if (showAdd || showAddColumn || showSettings || app.showComposeModal || app.errorModal) return;` を置き換える:

```svelte
    if (showAdd || showAddColumn || showSearch || showSettings || app.showComposeModal || app.errorModal) return;
```

`<AppMenu onAddColumn={openAddColumn} onOpenSettings={() => openSettings("accounts")} />` を置き換える:

```svelte
      <AppMenu
        onAddColumn={openAddColumn}
        onOpenSearch={() => (showSearch = true)}
        onOpenSettings={() => openSettings("accounts")}
      />
```

`{#if showAddColumn}` ブロックの直後（`{#if columnSettingsGroupId}` の前）に追加する:

```svelte
  {#if showSearch}
    <SearchModal onclose={() => (showSearch = false)} />
  {/if}
```

- [ ] **Step 3: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー0件。

- [ ] **Step 4: 既存テストを実行する**

Run: `cd frontend && pnpm test`
Expected: 全テスト PASS（既存テストを壊していないことの確認）。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/AppMenu.svelte frontend/src/App.svelte
git commit -m "feat: AppMenuに検索メニュー項目を追加(#248)"
```

---

### Task 5: 最終確認

**Files:** なし（検証のみ）

- [ ] **Step 1: バックエンド全テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS。

- [ ] **Step 2: フロントエンド全テスト・型チェックを実行する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 全テスト PASS、型エラー0件。

- [ ] **Step 3: 実アプリで動作確認する**

リポジトリルートで `cargo tauri dev` を起動し、`run` スキルの手順に従って画面を確認する:
1. 下部メニュー（ハンバーガーアイコン）→「検索」を開く。
2. アカウントを選び、キーワード欄に既にキャッシュ済みの本文の一部を入力して「検索」→結果が表示されることを確認する。
3. 「エキスパート(TQL)」に切り替え、`has_files` 等を入力して検索できることを確認する。
4. 結果ノートの返信/リアクションボタンが操作可能なことを確認する。
5. 確認後、自分で起動した `cargo tauri dev` を停止する。

- [ ] **Step 4: PRを作成する**

`commit-commands:commit-push-pr` スキル、または以下の手順で作成する:

```bash
git push -u origin feature/issue-248-search
gh pr create --title "feat: 検索機能を追加" --body "Fixes #248

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```
