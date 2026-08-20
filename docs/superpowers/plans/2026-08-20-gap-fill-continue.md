# 起動時ギャップ埋め打ち切り分の続き取得 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** アプリ再起動時／WS再接続時の起動時ギャップ埋めが `gap_fill_limit` を超えて打ち切られた場合に、タイムライン中間にできる恒久的な空白を、ユーザーが手動で「省略された投稿を表示」ボタンから埋められるようにする（Issue #148）。

**Architecture:** バックエンドの `fill_gap`（`src-tauri/src/commands/column.rs`）が「`newest_known_id` まで追いつけたか」を判定して返すよう拡張し、打ち切り時は `ColumnGapFill` イベントに `truncated`/`boundary_id`/`target_id` を載せてフロントへ伝える。フロントは `TabView.gapMarker` にこれを保持し、タイムライン上の該当ノート直後に区切り線＋ボタンを描画する。ボタン押下で、既存の `fetch_backfill` コマンドを `target_id` に到達するまでループ呼び出しし、ギャップを埋める。新規コマンドは追加しない。

**Tech Stack:** Rust (Tauri v2 backend, `src-tauri/`), Svelte 5 runes (frontend, `frontend/src/`), Vitest (frontend tests), `cargo test` (Rust tests)。

## Global Constraints

- 対象はノートカラムの起動時ギャップ埋めのみ。通知カラムは対象外。
- `gapMarker` はセッション中のみ保持（アプリ再起動をまたいだ永続化はしない）。
- 「続きを取得」の1クリックあたりの上限は10ページ（既存 `fetch_backfill` は1ページ `INITIAL_LIMIT=20` 件なので最大200件/クリック）。到達しなければマーカーを更新して残し、再クリックで継続できるようにする。
- 複数マーカーの同時表示はサポートしない。新しい `truncated: true` イベントが来たら上書きする。
- `frontend/src/bindings/tauri.gen.ts` は生成ファイル。手で編集せず、Rust側の変更後に `cd src-tauri && cargo test generates_frontend_bindings` で再生成する。

---

## Task 1: `fill_gap` に打ち切り判定を追加する（Rust・純粋関数を切り出してテスト可能にする）

**Files:**
- Modify: `src-tauri/src/commands/column.rs:713-783`（`fill_gap` 関数）
- Test: `src-tauri/src/commands/column.rs`（同ファイル末尾に `#[cfg(test)] mod tests` を新規追加）

**Interfaces:**
- Consumes: なし（このタスクは既存コードのみに依存）
- Produces:
  - `struct GapFillResult { notes: Vec<Note>, truncated: bool, boundary_id: Option<String> }`（`column.rs` 内、`pub(crate)` 不要、モジュール内 private で可）
  - `fn finalize_gap_fill(collected: Vec<Note>, all_sources_reached_target: bool, limit: i32) -> GapFillResult`（private関数、Task 2で呼び出し元が使う`fill_gap`のシグネチャ変更の中身）
  - `async fn fill_gap(...) -> Result<GapFillResult>`（戻り型が `Vec<Note>` から `GapFillResult` に変更。呼び出し元は Task 2 で更新）

- [ ] **Step 1: `finalize_gap_fill` の失敗するテストを書く**

`src-tauri/src/commands/column.rs` の末尾（ファイル最終行の後）に追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{User, Visibility};

    fn note(id: &str, created_at: i64) -> Note {
        Note {
            id: id.into(),
            created_at,
            text: Some("hello".into()),
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
        }
    }

    #[test]
    fn finalize_gap_fill_marks_truncated_when_target_not_reached() {
        let collected = vec![note("n3", 30), note("n2", 20), note("n1", 10)];
        let result = finalize_gap_fill(collected, false, 100);

        assert!(result.truncated);
        assert_eq!(result.boundary_id.as_deref(), Some("n1"));
        assert_eq!(result.notes.len(), 3);
    }

    #[test]
    fn finalize_gap_fill_not_truncated_when_all_sources_reached_target() {
        let collected = vec![note("n2", 20), note("n1", 10)];
        let result = finalize_gap_fill(collected, true, 100);

        assert!(!result.truncated);
        assert_eq!(result.boundary_id, None);
        assert_eq!(result.notes.len(), 2);
    }

    #[test]
    fn finalize_gap_fill_truncates_to_limit_and_reports_oldest_kept_as_boundary() {
        let collected = vec![note("n3", 30), note("n2", 20), note("n1", 10)];
        let result = finalize_gap_fill(collected, false, 2);

        assert_eq!(result.notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n3", "n2"]);
        assert!(result.truncated);
        assert_eq!(result.boundary_id.as_deref(), Some("n2"));
    }

    #[test]
    fn finalize_gap_fill_not_truncated_when_nothing_collected() {
        let result = finalize_gap_fill(vec![], false, 100);

        assert!(!result.truncated);
        assert_eq!(result.boundary_id, None);
        assert!(result.notes.is_empty());
    }

    #[test]
    fn finalize_gap_fill_dedupes_by_id() {
        let collected = vec![note("n1", 10), note("n1", 10)];
        let result = finalize_gap_fill(collected, true, 100);

        assert_eq!(result.notes.len(), 1);
    }
}
```

- [ ] **Step 2: テストを実行し、コンパイルエラーで失敗することを確認する**

Run: `cd src-tauri && cargo test finalize_gap_fill`
Expected: FAIL — `finalize_gap_fill` / `GapFillResult` が未定義というコンパイルエラー

- [ ] **Step 3: `GapFillResult` と `finalize_gap_fill` を実装し、`fill_gap` をリファクタリングする**

`src-tauri/src/commands/column.rs:709-783` の `fill_gap` 関数を以下に置き換える（コメント行709-712も含む既存のdocコメントは維持しつつ拡張):

```rust
/// `fill_gap` の結果。`newest_known_id` に追いつけたか(=打ち切りが起きたか)を
/// 呼び出し元(resume_column/gap_fill_on_reconnect)がフロントへ伝えるために持つ。
struct GapFillResult {
    notes: Vec<Note>,
    /// newest_known_id に追いつく前に limit/ページ数上限で打ち切られた場合 true。
    truncated: bool,
    /// truncated=true のとき、取得できた中で一番古いノートのid。
    /// フロントが「続きを取得」で fetch_backfill の until_id に使う。
    boundary_id: Option<String>,
}

/// 収集済みノートを重複除去・整形し、`newest_known_id` に追いつけたかを判定する。
/// ネットワークを伴わない純粋関数にして単体テストしやすくしている。
fn finalize_gap_fill(mut collected: Vec<Note>, all_sources_reached_target: bool, limit: i32) -> GapFillResult {
    let mut seen = std::collections::HashSet::new();
    collected.retain(|n| seen.insert(n.id.clone()));
    collected.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    collected.truncate(limit.max(0) as usize);
    let truncated = !all_sources_reached_target && !collected.is_empty();
    let boundary_id = if truncated { collected.last().map(|n| n.id.clone()) } else { None };
    GapFillResult { notes: collected, truncated, boundary_id }
}

/// 起動時のギャップ埋め: アプリを閉じていた間に流れたノートを、キャッシュの最新ノートid
/// (`newest_known_id`)まで REST で遡って取得する。`limit` 件、または既知のノートに追いつく
/// (取得ページの中に newest_known_id 以前のノートが現れる)まで、どちらか早い方で打ち切る。
/// ページ数にも上限(GAP_FILL_MAX_PAGES)を設け、長期間閉じていた場合の暴走取得を防ぐ。
/// 打ち切られた(=追いつけなかった)場合は `GapFillResult::truncated` が true になる。
async fn fill_gap(
    state: &AppState,
    account_id: &str,
    resolved: &ResolvedSources,
    newest_known_id: &str,
    limit: i32,
) -> Result<GapFillResult> {
    if resolved.kinds.is_empty() {
        return Ok(GapFillResult { notes: vec![], truncated: false, boundary_id: None });
    }
    let client = state.client_for(account_id)?;
    let ctx = state.eval_context();
    let mute = state.mute.lock().unwrap().clone();
    let mut collected: Vec<Note> = Vec::new();

    // ソースごとに独立した until_id カーソルと「既知ノートに追いついた/枯渇した」フラグを持つ。
    // 複数ソースを1本の until_id で回すと、疎なソースの古い1件に引きずられて密なソース側の
    // 途中が埋まらないまま打ち切られてしまうため、ソース単位で打ち切りを判定する。
    let mut cursors: Vec<Option<String>> = vec![None; resolved.kinds.len()];
    let mut done: Vec<bool> = vec![false; resolved.kinds.len()];
    // done とは別に「newest_known_id に本当に追いついたか」を持つ。done はページ枯渇/失敗
    // でも true になるため、truncated 判定には使えない。
    let mut reached_target: Vec<bool> = vec![false; resolved.kinds.len()];

    for _ in 0..GAP_FILL_MAX_PAGES {
        if done.iter().all(|d| *d) || collected.len() as i32 >= limit {
            break;
        }
        let mut any_fetched = false;
        for (i, k) in resolved.kinds.iter().enumerate() {
            if done[i] {
                continue;
            }
            let Some((endpoint, body)) = k.rest_request(GAP_FILL_PAGE_SIZE, cursors[i].as_deref())
            else {
                done[i] = true;
                continue;
            };
            let Ok(mut page) = fetch_notes(&client, endpoint, &body).await else {
                done[i] = true;
                continue;
            };
            if page.is_empty() {
                done[i] = true;
                continue;
            }
            any_fetched = true;
            page.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
            let oldest_this_page = page.last().map(|n| n.id.clone());
            for n in page {
                if n.id.as_str() <= newest_known_id {
                    done[i] = true;
                    reached_target[i] = true;
                    continue;
                }
                if resolved.filter.matches(&n, &ctx)
                    && !crate::filter::mute::is_muted(&n, &mute)
                    && !server_muted_note(state, account_id, &n)
                {
                    collected.push(n);
                }
            }
            cursors[i] = oldest_this_page;
        }
        if !any_fetched {
            break;
        }
    }

    let all_reached = reached_target.iter().all(|r| *r);
    Ok(finalize_gap_fill(collected, all_reached, limit))
}
```

- [ ] **Step 4: テストを実行し、パスすることを確認する**

Run: `cd src-tauri && cargo test finalize_gap_fill`
Expected: PASS（5テストすべて）

- [ ] **Step 5: `fill_gap` を呼び出している箇所がまだ `Vec<Note>` 前提のままでコンパイルエラーになることを確認する**

Run: `cd src-tauri && cargo build 2>&1 | head -50`
Expected: `resume_column` と `gap_fill_on_reconnect` の呼び出し箇所で型エラー（Task 2で解消する。ここではエラーが出ることの確認のみ）

- [ ] **Step 6: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/column.rs
git commit -m "refactor: fill_gapがnewest_known_idへの到達可否を返すようにする"
```

---

## Task 2: `ColumnGapFill` イベントに打ち切り情報を追加し、呼び出し元を更新する（Rust）

**Files:**
- Modify: `src-tauri/src/events.rs:16-23`（`ColumnGapFill` 構造体）
- Modify: `src-tauri/src/commands/column.rs:266-310`（`resume_column` の背景ギャップ埋め）
- Modify: `src-tauri/src/commands/column.rs:818-867`（`gap_fill_on_reconnect`）

**Interfaces:**
- Consumes: Task 1 の `GapFillResult { notes, truncated, boundary_id }` と `fill_gap`
- Produces: `ColumnGapFill { column_id: String, notes: Vec<Note>, truncated: bool, boundary_id: Option<String>, target_id: Option<String> }`（イベント。tauri-specta 経由でTS型 `ColumnGapFillEvent` 相当が生成される）

- [ ] **Step 1: `ColumnGapFill` イベントにフィールドを追加**

`src-tauri/src/events.rs:16-23` を以下に置き換える:

```rust
/// 起動時のギャップ埋め結果をまとめて反映する（通知は鳴らさない・出入りの都度イベントにしない）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ColumnGapFill {
    pub column_id: String,
    pub notes: Vec<Note>,
    /// newest_known_id に追いつく前に gap_fill_limit 等で打ち切られた場合 true。
    pub truncated: bool,
    /// truncated=true のとき、続きを取得する際に fetch_backfill の until_id に使う境界ノートid。
    pub boundary_id: Option<String>,
    /// truncated=true のときの到達目標(元のキャッシュ最新ノートid)。
    pub target_id: Option<String>,
}
```

- [ ] **Step 2: `resume_column` の背景ギャップ埋めを更新**

`src-tauri/src/commands/column.rs:280-308` を以下に置き換える:

```rust
        let resolved = resolved.expect("非通知カラムは resolve_sources 済み");
        let gap_limit = state
            .settings
            .load_ui()
            .map(|p| p.gap_fill_limit)
            .unwrap_or(0)
            .max(0);
        let newest_known_id = notes[0].id.clone(); // load_cached は created_at 降順（先頭が最新）
        open_streams_only(&app, &state, &column, &resolved, host, token);
        if gap_limit > 0 {
            let app2 = app.clone();
            let column_id = column.id.clone();
            let account_id = column.account_id.clone();
            tauri::async_runtime::spawn(async move {
                let Some(state) = app2.try_state::<AppState>() else { return };
                let result = fill_gap(&state, &account_id, &resolved, &newest_known_id, gap_limit)
                    .await
                    .unwrap_or(GapFillResult { notes: vec![], truncated: false, boundary_id: None });
                if result.notes.is_empty() {
                    return;
                }
                let _ = state.cache.cache_notes(&column_id, &result.notes);
                let _ = crate::events::ColumnGapFill {
                    column_id,
                    notes: result.notes,
                    truncated: result.truncated,
                    boundary_id: result.boundary_id,
                    target_id: if result.truncated { Some(newest_known_id) } else { None },
                }
                .emit(&app2);
            });
        }
```

- [ ] **Step 3: `gap_fill_on_reconnect` を更新**

`src-tauri/src/commands/column.rs:853-866` を以下に置き換える:

```rust
    let Ok(result) =
        fill_gap(&state, &column.account_id, &resolved, &newest_known_id, gap_limit).await
    else {
        return;
    };
    if result.notes.is_empty() {
        return;
    }
    let _ = state.cache.cache_notes(&column.id, &result.notes);
    let _ = crate::events::ColumnGapFill {
        column_id: column.id.clone(),
        notes: result.notes,
        truncated: result.truncated,
        boundary_id: result.boundary_id,
        target_id: if result.truncated { Some(newest_known_id) } else { None },
    }
    .emit(app);
```

- [ ] **Step 4: ビルドが通ることを確認**

Run: `cd src-tauri && cargo build`
Expected: 成功（警告なし、エラーなし）

- [ ] **Step 5: 全Rustテストを実行**

Run: `cd src-tauri && cargo test`
Expected: PASS（`#[ignore]` の実接続系を除く全テスト。Task 1のテストも含む）

- [ ] **Step 6: TSバインディングを再生成**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` の `ColumnGapFill` 型に `truncated`/`boundaryId`/`targetId` が追加されていることを確認する:

Run: `grep -n "ColumnGapFill" frontend/src/bindings/tauri.gen.ts`
Expected: `truncated: boolean`, `boundaryId: string | null`, `targetId: string | null` を含む型定義が出力されている

- [ ] **Step 7: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/events.rs src-tauri/src/commands/column.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: ColumnGapFillイベントにギャップ打ち切り情報を追加"
```

---

## Task 3: `TabView` に `gapMarker`/`fillingGap` を追加し、イベント受信でマーカーをセットする（フロントエンド）

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:49-69`（`TabView` interface）
- Modify: `frontend/src/lib/store.svelte.ts:316-335`（`#makeTab`）
- Modify: `frontend/src/lib/store.svelte.ts:810-824`（`columnGapFill` イベントリスナ）
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: Task 2 で生成された `events.columnGapFill` のペイロード `{ columnId, notes, truncated, boundaryId, targetId }`
- Produces: `TabView.gapMarker: { boundaryId: string; targetId: string } | null`、`TabView.fillingGap: boolean`（Task 4の `fillRemainingGap` が使う）

- [ ] **Step 1: `TabView` に型を追加**

`frontend/src/lib/store.svelte.ts:64-68` を以下に置き換える:

```ts
  notes: Note[];
  notifications: Notification[];
  state: ConnectionState;
  loadingMore: boolean;
  /// 起動時ギャップ埋めが gap_fill_limit 等で打ち切られたまま残っている空白（Issue #148）。
  /// boundaryId のノート直後(より過去側)にタイムライン上で区切り線+ボタンを描画する。
  gapMarker: { boundaryId: string; targetId: string } | null;
  /// fillRemainingGap の多重実行防止フラグ。loadingMore とは独立して持つ
  /// (スクロールでの追加読み込みとギャップ埋め継続を同時に走らせても問題ないが、
  /// ボタンの二重クリックだけは防ぎたいため)。
  fillingGap: boolean;
  selectedNoteId: string | null;
```

- [ ] **Step 2: `#makeTab` に初期値を追加**

`frontend/src/lib/store.svelte.ts:329-333` を以下に置き換える:

```ts
      notes: opened.notes,
      notifications: opened.notifications,
      state: this.#connState.get(opened.column.id) ?? "connecting",
      loadingMore: false,
      gapMarker: null,
      fillingGap: false,
      selectedNoteId: null,
```

- [ ] **Step 3: `columnGapFill` リスナで `truncated` ならマーカーをセットするテストを書く**

`frontend/src/lib/store.svelte.test.ts` に以下の `describe` ブロックを追加する（既存の `makeGroup`/`makeNote` ヘルパーを再利用。ファイル末尾に追加）:

```ts
function makeNoteTab(notes: Note[], overrides: Partial<TabView> = {}): TabView {
  return {
    id: "tab1",
    accountId: ACCOUNT_ID,
    kind: { type: "home" },
    title: "ホーム",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes,
    notifications: [],
    state: "connected",
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    ...overrides,
  };
}

describe("app.fillRemainingGap (Issue #148)", () => {
  it("gapMarkerが無ければ何もしない", async () => {
    const tab = makeNoteTab([makeNote({ id: "n1" })]);
    app.groups = [makeGroup([tab])];

    await app.fillRemainingGap(tab.id);

    expect(invokeMock).not.toHaveBeenCalled();
  });
});
```

（`TabView` は既にテストファイル冒頭で `import type { GroupView, TabView } from "./store.svelte";` 済み。`kind: { type: "home" }` は `ColumnKind`（`frontend/src/bindings/tauri.gen.ts:335`）の判別可能ユニオンの有効な1バリアント。）

- [ ] **Step 4: テストを実行し、`fillRemainingGap` 未定義でコンパイルエラーになることを確認**

Run: `cd frontend && pnpm test -- store.svelte.test.ts`
Expected: FAIL — `app.fillRemainingGap is not a function`

- [ ] **Step 5: `columnGapFill` リスナを更新（マーカーのセットのみ。`fillRemainingGap` はTask 4で実装）**

`frontend/src/lib/store.svelte.ts:810-824` を以下に置き換える:

```ts
    this.#unlisten.push(
      await events.columnGapFill.listen((e) => {
        const tab = this.#findTab(e.payload.columnId);
        if (!tab) return;
        // 起動時のギャップ埋め結果をまとめて反映する。1件ずつの新着とは違い、
        // 新着通知/通知音は鳴らさない（不在中に溜まったノートで誤爆しないため）。
        const known = new Set(tab.notes.map((n) => n.id));
        const merged = [...tab.notes];
        for (const n of e.payload.notes) {
          if (!known.has(n.id)) merged.push(n);
        }
        merged.sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
        tab.notes = merged.slice(0, MAX_NOTES);
        // 打ち切られた(=newest_known_idに追いつけなかった)場合のみマーカーをセットする。
        // truncated=false のイベントで既存マーカーを消すことはしない — このイベントの
        // newest_known_idは「現在のキャッシュ最新」基準であり、過去に打ち切られた
        // もっと古い空白とは無関係な場合があるため（Issue #148）。
        if (e.payload.truncated && e.payload.boundaryId && e.payload.targetId) {
          tab.gapMarker = { boundaryId: e.payload.boundaryId, targetId: e.payload.targetId };
        }
      }),
    );
```

- [ ] **Step 6: ここまでのテストを実行してパスすることを確認（`fillRemainingGap`はまだ存在しないため、Step 3のテストのみ対象に一旦絞って確認）**

Run: `cd frontend && pnpm test -- store.svelte.test.ts -t "fillRemainingGap"`
Expected: `gapMarkerが無ければ何もしない` は依然 `app.fillRemainingGap is not a function` で FAIL（想定通り。Task 4で解消）

- [ ] **Step 7: `pnpm check` を実行し、型エラーが無いことを確認（`fillRemainingGap`呼び出し以外の変更箇所について）**

Run: `cd frontend && pnpm check`
Expected: `fillRemainingGap` 呼び出しに関するエラーのみが残る想定（Task 4で解消）。それ以外の新規エラーが無いことを確認する

- [ ] **Step 8: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: TabViewにgapMarkerを追加しギャップ埋め打ち切りを検知する"
```

---

## Task 4: `fillRemainingGap` を実装する（フロントエンド）

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`（`MAX_NOTES` 付近に定数追加、`loadMore` の直後に新規メソッド追加）
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: `TabView.gapMarker`/`fillingGap`（Task 3）、`commands.fetchBackfill(columnId: string, untilId: string): Promise<Note[]>`（既存、`frontend/src/bindings/tauri.gen.ts:86`）、`this.#captureInitial(tabId, notes)`（既存private method）、`this.#logFailure(e)`（既存）
- Produces: `async fillRemainingGap(tabId: string): Promise<void>`（`AppStore` のpublicメソッド）

- [ ] **Step 1: 「targetIdに到達したらgapMarkerを消す」テストを書く**

`frontend/src/lib/store.svelte.test.ts` の `describe("app.fillRemainingGap (Issue #148)", ...)` ブロックに以下のテストを追加:

```ts
  it("targetIdに到達したらgapMarkerを消し、取得したノートをマージする", async () => {
    const existing = [makeNote({ id: "n5", createdAt: 50 }), makeNote({ id: "target", createdAt: 10 })];
    const tab = makeNoteTab(existing, { gapMarker: { boundaryId: "n4", targetId: "target" } });
    app.groups = [makeGroup([tab])];

    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        expect(args).toMatchObject({ columnId: "tab1", untilId: "n4" });
        return {
          status: "ok",
          data: [makeNote({ id: "n3", createdAt: 30 }), makeNote({ id: "target", createdAt: 10 })],
        };
      }
      if (cmd === "capture_notes") return { status: "ok", data: null };
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    expect(tab.gapMarker).toBeNull();
    expect(tab.fillingGap).toBe(false);
    expect(tab.notes.map((n) => n.id)).toEqual(["n5", "n3", "target"]);
  });

  it("targetIdに到達しないままページ上限に達したらgapMarkerを新しい境界で更新する", async () => {
    const tab = makeNoteTab(
      [makeNote({ id: "n5", createdAt: 50 })],
      { gapMarker: { boundaryId: "n4", targetId: "target" } },
    );
    app.groups = [makeGroup([tab])];

    let call = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "fetch_backfill") {
        call += 1;
        return { status: "ok", data: [makeNote({ id: `page${call}`, createdAt: 100 - call })] };
      }
      if (cmd === "capture_notes") return { status: "ok", data: null };
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    expect(call).toBe(10); // GAP_CONTINUE_MAX_PAGES
    expect(tab.gapMarker).toEqual({ boundaryId: "page10", targetId: "target" });
    expect(tab.fillingGap).toBe(false);
  });

  it("APIが失敗したらgapMarkerを維持しfillingGapをfalseに戻す", async () => {
    const tab = makeNoteTab(
      [makeNote({ id: "n5", createdAt: 50 })],
      { gapMarker: { boundaryId: "n4", targetId: "target" } },
    );
    app.groups = [makeGroup([tab])];
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "fetch_backfill") return { status: "error", error: { kind: "network", message: "boom" } };
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    expect(tab.gapMarker).toEqual({ boundaryId: "n4", targetId: "target" });
    expect(tab.fillingGap).toBe(false);
  });
```

（`kind: "network"` は `Error` 型（`frontend/src/bindings/tauri.gen.ts:431`）の有効なバリアント。）

- [ ] **Step 2: テストを実行し、`fillRemainingGap` 未定義で失敗することを確認**

Run: `cd frontend && pnpm test -- store.svelte.test.ts -t "fillRemainingGap"`
Expected: FAIL

- [ ] **Step 3: 定数と `fillRemainingGap` を実装**

`frontend/src/lib/store.svelte.ts:44-46` の定数群に1行追加:

```ts
const MAX_NOTES = 300; // タブあたり DOM に保持する上限（仮想化-lite）
const GAP_CONTINUE_MAX_PAGES = 10; // 「省略された投稿を表示」1クリックあたりの取得ページ上限（Issue #148）
const UPDATE_CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000; // 新バージョン確認の間隔（4時間）
```

`loadMore` メソッド（`frontend/src/lib/store.svelte.ts` の既存 `async loadMore(tabId: string) { ... }` の直後、`}` の次の行）に新規メソッドを追加:

```ts
  /// 起動時ギャップ埋めが打ち切られて残った空白を、ユーザー操作で埋める(Issue #148)。
  /// tab.gapMarker.targetId に到達するまで fetch_backfill を最大 GAP_CONTINUE_MAX_PAGES 回
  /// ループ呼び出しする。到達しなければ gapMarker を最新の境界で更新して残す(再クリック可能)。
  async fillRemainingGap(tabId: string) {
    const tab = this.#findTab(tabId);
    if (!tab || !tab.gapMarker || tab.fillingGap) return;
    tab.fillingGap = true;
    const { targetId } = tab.gapMarker;
    let boundaryId = tab.gapMarker.boundaryId;
    try {
      for (let page = 0; page < GAP_CONTINUE_MAX_PAGES; page++) {
        const fetched = await unwrap(commands.fetchBackfill(tabId, boundaryId));
        if (fetched.length === 0) break;

        const known = new Set(tab.notes.map((n) => n.id));
        const fresh = fetched.filter((n) => !known.has(n.id));
        if (fresh.length > 0) {
          const merged = [...tab.notes, ...fresh];
          merged.sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
          tab.notes = merged.slice(0, MAX_NOTES);
          // captureInitial 同様に subNote 購読しないと、この先そのノートへの
          // リアクション追加/削除が noteUpdated イベントとして届かず反映されない(Issue #3)。
          this.#captureInitial(tab.id, fresh);
        }

        if (fetched.some((n) => n.id === targetId)) {
          tab.gapMarker = null;
          return;
        }
        boundaryId = fetched[fetched.length - 1].id;
        tab.gapMarker = { boundaryId, targetId };
      }
    } catch (e) {
      this.#logFailure(e);
    } finally {
      tab.fillingGap = false;
    }
  }
```

- [ ] **Step 4: テストを実行しパスすることを確認**

Run: `cd frontend && pnpm test -- store.svelte.test.ts -t "fillRemainingGap"`
Expected: PASS（4テストすべて）

- [ ] **Step 5: 全フロントエンドテストと型チェックを実行**

Run: `cd frontend && pnpm test && pnpm check`
Expected: 両方 PASS

- [ ] **Step 6: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: fillRemainingGapでギャップ埋め打ち切り分を手動取得できるようにする"
```

---

## Task 5: タイムラインに区切り線＋ボタンを描画する（UI）

**Files:**
- Modify: `frontend/src/ui/Column.svelte:153-165`

**Interfaces:**
- Consumes: `activeTab.gapMarker`、`activeTab.fillingGap`（Task 3/4）、`app.fillRemainingGap(tabId)`（Task 4）
- Produces: なし（末端のUI）

- [ ] **Step 1: `{#each activeTab.notes}` ブロックにマーカー描画を追加**

`frontend/src/ui/Column.svelte:153-161` を以下に置き換える:

```svelte
      {:else}
        {#each activeTab.notes as note (note.id)}
          <NoteCard
            {note}
            accountId={activeTab.accountId}
            tabId={activeTab.id}
            selected={note.id === activeTab.selectedNoteId}
          />
          {#if activeTab.gapMarker && note.id === activeTab.gapMarker.boundaryId}
            <div class="flex items-center gap-2 border-y border-border bg-muted/40 px-3.5 py-2 text-sm text-muted-foreground">
              <span class="flex-1">この間の投稿は省略されています</span>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={activeTab.fillingGap}
                onclick={() => app.fillRemainingGap(activeTab.id)}
              >
                {activeTab.fillingGap ? "取得中…" : "省略された投稿を表示"}
              </Button>
            </div>
          {/if}
        {/each}
        {#if activeTab.notes.length === 0 && !activeTab.loadingMore}
          <div class="p-3.5 text-center text-sm text-muted-foreground">まだノートがありません</div>
        {/if}
      {/if}
```

- [ ] **Step 2: フロントの型チェックとビルドを確認**

Run: `cd frontend && pnpm check`
Expected: PASS

- [ ] **Step 3: 手動確認（`gap_fill_limit` を一時的に下げて打ち切りを再現する）**

打ち切りを自然発生させるにはアプリを長時間閉じてから多数のノートが投稿されるのを待つ必要があり非現実的なため、設定値を一時的に下げて確認する:

1. `cargo tauri dev` を起動（プロジェクトルートから。`CLAUDE.md` 参照）
2. 設定→表示（または該当する設定UI）で `gap_fill_limit` を仮に小さい値（例: 5）に変更できる項目があるか確認する。UIから変更できない場合は、DB (`~/.local/share/tsumugi/*.sqlite3` 相当。実際のパスは `state.settings` の実装 `src-tauri/src/store/settings.rs` を参照)に直接該当行を仮更新するか、テスト用に一時的に `default_gap_fill_limit()`（`src-tauri/src/domain/ui.rs:209`）の戻り値を `2` 等に変更してビルドし直す（**確認後に必ず元の値に戻すこと**）
3. 何件かノートが飛び交うホームカラムを開いた状態でアプリを終了し、他クライアント等で数件投稿してから再度アプリを起動する
4. タイムライン中に区切り線＋「省略された投稿を表示」ボタンが表示されることを確認する
5. ボタンを押し、ローディング表示→ノートが挿入されボタンが消える（または「まだ空白がある」場合は再度表示され続ける）ことを確認する
6. Step 2 で `default_gap_fill_limit()` を変更した場合は元の値に戻し、`git diff src-tauri/src/domain/ui.rs` が空になっていることを確認する

- [ ] **Step 4: コミット**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/ui/Column.svelte
git commit -m "feat: タイムラインにギャップ埋め打ち切りの区切り線とボタンを表示する"
```

---

## Task 6: 仕上げ確認とPR作成

**Files:** なし（確認のみ）

**Interfaces:** なし

- [ ] **Step 1: バックエンド全テスト**

Run: `cd src-tauri && cargo test`
Expected: PASS（`#[ignore]` 系を除く）

- [ ] **Step 2: フロントエンド全テスト＋型チェック**

Run: `cd frontend && pnpm test && pnpm check`
Expected: PASS

- [ ] **Step 3: `git status` で意図しない変更が残っていないか確認**

Run: `git status --short`
Expected: `docs/superpowers/specs/2026-08-20-gap-fill-continue-design.md`、`docs/superpowers/plans/2026-08-20-gap-fill-continue.md` とTask 1〜5でコミット済みの変更以外に差分が無いこと

- [ ] **Step 4: PR作成**

```bash
git push -u origin feat/issue-148-gap-fill-continue
gh pr create --title "feat: 起動時ギャップ埋め打ち切り分の続き取得(Issue #148)" --body "$(cat <<'EOF'
## 概要
起動時/再接続時のギャップ埋めが gap_fill_limit を超えて打ち切られた場合、タイムライン中間に恒久的な空白ができていた問題を修正。タイムライン上に区切り線とボタンを表示し、手動で続きを取得できるようにした。

Fixes #148

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Expected: PR URLが出力される。マージは行わず、ここで作業完了とする（CLAUDE.mdの方針通り、CI結果はユーザーが確認する）。
