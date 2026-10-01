# fillRemainingGap キャッシュ優先バイパス Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `fillRemainingGap` が `fetch_backfill` のキャッシュ Hit によってギャップを埋めずにマーカーを消す不具合(Issue #427)を、`fetch_backfill` に `bypass_cache: bool` 引数を足して直す。

**Architecture:** Rust の `fetch_backfill` がキャッシュ読み出しに入る条件を `cache_eligible && !bypass_cache` にする(純粋関数 `should_try_backfill_cache` に切り出す)。境界の取得・延長は従来どおり `cache_eligible` で行う。フロントは `loadMore` が `false`、`fillRemainingGap` が `true` を渡す。TS バインディングは `cargo test` で再生成する。

**Tech Stack:** Rust (Tauri v2, tauri-specta), Svelte 5 / TypeScript, Vitest

**Spec:** `docs/superpowers/specs/2026-10-01-gap-fill-bypass-cache-design.md`

## Global Constraints

- ブランチは `fix/427-gap-fill-bypass-cache`(作成済み)。`main` へ直接コミットしない。
- コミットメッセージは件名1行のみ。本文・箇条書きを書かない。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` トレーラーを付ける(`git commit -m "$(cat <<'EOF' ... EOF)"` で件名の後に空行を挟んで書く)。`--no-verify` / `--no-gpg-sign` 禁止。`git commit` が失敗/タイムアウトしたら止めてユーザーに報告し、リトライしない。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集しない。`cd src-tauri && cargo test` で再生成する。
- 引数名は `bypass_cache`(TS 側は specta の camelCase 変換で `bypassCache`)。Rust コマンド引数は必須の `bool`(`Option` にしない)。
- `cargo tauri dev` はリポジトリルートから実行する(src-tauri の中ではなく)。実機確認は Xvfb 越し、`WAYLAND_DISPLAY` を unset、`dbus-run-session` 必須。自分で起動した dev サーバはタスク完了前に正確な PID で kill する(`pkill`/`killall` 禁止)。

## Review Focus

- バイパス時でも、API 取得結果が既存境界と連続していれば境界が延びること(`plan_boundary_extend` に渡る `cache_eligible` が false にならないこと)。Task 1 の純粋関数テストと実装方針(`cache_eligible` を境界処理に残す)で担保。
- `cache_eligible == false`(User/Tag/Search を含む等)のカラムで `bypass_cache=false` でも従来どおりキャッシュを使わないこと。Task 1 のテストで固定。
- `loadMore` が誤って `bypassCache: true` を渡し、通常スクロールのキャッシュ優先が死なないこと。Task 2 のテストで固定。
- ギャップ埋めの複数ページループで、2ページ目以降も `bypassCache: true` が渡ること。Task 2 のテストで固定。
- Rust のメトリクス(`record_backfill`)がバイパス時に記録されないこと。コード上、記録はキャッシュ読み出しブロック内にのみあるため、条件を変えるだけで満たされる(Task 1 Step 3 で確認)。

---

### Task 1: Rust — `bypass_cache` 引数と判定関数、バインディング再生成

**Files:**
- Modify: `src-tauri/src/commands/column.rs`(`fetch_backfill` 約 411–491 行、`backfill_cache_eligible` 約 1218 行付近、`mod tests` 内約 1724 行付近)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`(`cargo test` が生成)

**Interfaces:**
- Consumes: 既存の `backfill_cache_eligible(&ResolvedSources) -> bool`
- Produces:
  - `fn should_try_backfill_cache(cache_eligible: bool, bypass_cache: bool) -> bool`(`commands/column.rs` 内の非公開関数)
  - `commands.fetchBackfill(columnId: string, untilId: string, bypassCache: boolean)`(`tauri.gen.ts`。Task 2 が呼ぶ)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/column.rs` の `mod tests` 内、`backfill_cache_eligible_requires_streaming_api_sources_and_no_cache_source` の直後に追加する。

```rust
    #[test]
    fn should_try_backfill_cache_is_off_when_bypassed_or_ineligible() {
        // 通常の上スクロール: eligible のときだけキャッシュを試す
        assert!(should_try_backfill_cache(true, false));
        assert!(!should_try_backfill_cache(false, false));
        // ギャップ埋め(Issue #427): eligible でもキャッシュを使わない
        assert!(!should_try_backfill_cache(true, true));
        assert!(!should_try_backfill_cache(false, true));
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test --lib should_try_backfill_cache`
Expected: コンパイルエラー `cannot find function should_try_backfill_cache in this scope`

- [ ] **Step 3: 実装する**

(a) `backfill_cache_eligible` の直後に純粋関数を追加する。

```rust
/// backfill でキャッシュ読み出し(`load_cached_before` 〜 Hit判定)に入るか。
/// `bypass_cache`(fillRemainingGap=ギャップ埋め)のときは、`until_id` より新しい未取得区間を
/// キャッシュが覆っていると言えないため、eligible でも必ずAPIへ行く(Issue #427)。
fn should_try_backfill_cache(cache_eligible: bool, bypass_cache: bool) -> bool {
    cache_eligible && !bypass_cache
}
```

(b) `fetch_backfill` のドキュメントコメントの末尾に追記し、引数を足す。

```rust
/// `bypass_cache=true`(fillRemainingGap)のときはキャッシュ読み出しを行わず常にAPIへ行く。
/// ギャップ区間 `(targetId, boundaryId)` は未取得のため、キャッシュHitで埋めた気になってはならない(Issue #427)。
/// 境界の延長はバイパス時も従来どおり行う(`plan_boundary_extend` が連続性を検証する)。
#[tauri::command]
#[specta::specta]
pub async fn fetch_backfill(
    state: State<'_, AppState>,
    column_id: String,
    until_id: String,
    bypass_cache: bool,
) -> Result<Vec<Note>> {
```

(c) キャッシュ読み出しブロックの条件だけを変える。境界の取得(`let boundaries = if cache_eligible {`)と、API取得後の境界延長(`if cache_eligible {` + `plan_boundary_extend`)は `cache_eligible` のまま変えない。

`Edit` で、次の一意な文字列を置換する。

old:
```rust
    if cache_eligible {
        let effective = effective_boundary(&boundaries, resolved.kinds.len());
```
new:
```rust
    if should_try_backfill_cache(cache_eligible, bypass_cache) {
        let effective = effective_boundary(&boundaries, resolved.kinds.len());
```

メトリクス(`state.cache_metrics.record_backfill(...)`)はこのブロック内にしかないので、バイパス時は記録されない(追加の変更は不要)。

- [ ] **Step 4: テストが通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS(Postgres/MySQL/実 Misskey の `#[ignore]` テストは対象外)。`generates_frontend_bindings` が `frontend/src/bindings/tauri.gen.ts` を再生成する。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi && git diff -- frontend/src/bindings/tauri.gen.ts`
Expected: `fetchBackfill: (columnId: string, untilId: string, bypassCache: boolean) => typedError<Note[], Error>(__TAURI_INVOKE("fetch_backfill", { columnId, untilId, bypassCache }))` の1行のみが変わる。

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | tail -20`
Expected: この変更に起因する新しい警告なし(既存の `commands/user.rs` の未使用 import 警告は #431 で別管理なので無視してよい)。

- [ ] **Step 5: コミットする**

注意: この時点で `cd frontend && pnpm check` は `fetchBackfill` の引数不足で失敗する。Task 2 で直す(同一ブランチ内の途中状態)。

```bash
git add src-tauri/src/commands/column.rs frontend/src/bindings/tauri.gen.ts
git commit -m "$(cat <<'EOF'
fix: fetch_backfillにbypass_cacheを追加しギャップ埋めのキャッシュHitを避ける (#427)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: フロント — 呼び出し側の更新とテスト、ドキュメント

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(`loadMore` 約 1772 行、`fillRemainingGap` 約 1800 行)
- Test: `frontend/src/lib/store.svelte.test.ts`(`describe("app.loadMore (Issue #239)")` と `describe("app.fillRemainingGap (Issue #148)")`)
- Modify: `docs/design/phase0-scaffold.md:352`

**Interfaces:**
- Consumes: `commands.fetchBackfill(columnId: string, untilId: string, bypassCache: boolean)`(Task 1 で再生成済みの `tauri.gen.ts`)
- Produces: なし

- [ ] **Step 1: 失敗するテストを書く**

(a) `store.svelte.test.ts` の `describe("app.loadMore (Issue #239)")` 内、最初の `it(...)`(MAX_NOTES 到達後も…)の直前に追加する。

```ts
  it("通常の上スクロールはキャッシュ優先を使う(bypassCache=false で fetch_backfill を呼ぶ, Issue #427)", async () => {
    const tab = makeNoteTab([makeNote({ id: "n0002", createdAt: 2 })]);
    app.groups = [makeGroup([tab])];

    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        expect(args).toMatchObject({ columnId: tab.id, untilId: "n0002", bypassCache: false });
        return [];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.loadMore(tab.id);

    expect(invokeMock).toHaveBeenCalledWith("fetch_backfill", expect.objectContaining({ bypassCache: false }));
  });
```

(b) `describe("app.fillRemainingGap (Issue #148)")` 内、最後の `it(...)`(APIが失敗したら…)の直後、`describe` を閉じる `});` の前に追加する。

```ts
  it("全ページで bypassCache=true を指定してキャッシュHitを避ける(Issue #427)", async () => {
    const tab = makeNoteTab(
      [makeNote({ id: "n5", createdAt: 50 })],
      { gapMarker: { boundaryId: "n4", targetId: "a0" } },
    );
    app.groups = [makeGroup([tab])];

    const calls: unknown[] = [];
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "fetch_backfill") {
        calls.push(args);
        // 2ページ目で targetId("a0") に到達させる
        return calls.length === 1
          ? [makeNote({ id: "n3", createdAt: 30 })]
          : [makeNote({ id: "a0", createdAt: 1 })];
      }
      if (cmd === "capture_notes") return null;
      throw new Error(`unexpected command: ${cmd}`);
    });

    await app.fillRemainingGap(tab.id);

    expect(calls).toHaveLength(2);
    for (const args of calls) {
      expect(args).toMatchObject({ columnId: tab.id, bypassCache: true });
    }
    expect(calls[0]).toMatchObject({ untilId: "n4" });
    expect(calls[1]).toMatchObject({ untilId: "n3" });
  });
```

既存の `fillRemainingGap` テスト(`expect(args).toMatchObject({ columnId: "tab1", untilId: "n4" })`)は `toMatchObject` なので、`bypassCache` が増えても通る。変更不要。

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm test -- store.svelte.test.ts`
Expected: 追加した2テストが FAIL(`bypassCache` が `undefined`。`fetchBackfill` が第3引数を渡していないため)。

- [ ] **Step 3: 実装する**

`frontend/src/lib/store.svelte.ts` を2か所変える。

`loadMore`:
```ts
        const older = await unwrap(commands.fetchBackfill(tab.id, oldest, false));
```

`fillRemainingGap`:
```ts
        // ギャップ区間 (targetId, boundaryId) は未取得なので、キャッシュ優先を避けて必ずAPIから取る(Issue #427)。
        const fetched = await unwrap(commands.fetchBackfill(tabId, boundaryId, true));
```

`docs/design/phase0-scaffold.md:352` の行を、次のように引数と説明を更新する。

old:
```
| `fetch_backfill` | `column_id, until_id: Option<String>` | `Vec<Note>` | 上スクロールで過去ページ取得 |
```
new:
```
| `fetch_backfill` | `column_id, until_id: String, bypass_cache: bool` | `Vec<Note>` | 上スクロール/ギャップ埋めで過去ページ取得。`bypass_cache=true` はキャッシュ優先を使わず常にAPI(Issue #427) |
```

- [ ] **Step 4: テストと型検査が通ることを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし(0 errors)

Run: `cd frontend && pnpm test`
Expected: 全テスト PASS

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS(Task 1 以降で変化なしの確認)

- [ ] **Step 5: コミットする**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts docs/design/phase0-scaffold.md
git commit -m "$(cat <<'EOF'
fix: fillRemainingGapがキャッシュ優先を避けてAPIからギャップを埋める (#427)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: 実機確認と PR

**Files:** なし(確認と PR 作成のみ)

- [ ] **Step 1: 実機確認(Xvfb 越し)**

Spec の「実機確認」に従う。要点:
- `Xvfb` を自分で起動し、`WAYLAND_DISPLAY` を unset、`DISPLAY=:<n>` を指定、`dbus-run-session` 経由でリポジトリルートから `cargo tauri dev` を起動する(ユーザーの実画面・実データに触れない)。
- 再接続などでギャップマーカーが残る状況を作り、マーカー押下後に区間のノートが埋まること、`bypass_cache=true` の呼び出しでキャッシュ Hit メトリクスが増えないことを debug bridge で確認する(手順は memory の `reference-verify-cache-metrics-via-debug-bridge` を参照)。
- 確認後、自分で起動した Xvfb / dev サーバ / dbus-run-session を正確な PID で kill する。

実機でギャップ状況を作れない場合は、その旨をユーザーに報告し、省略したことを明示する(確認済みと言わない)。

- [ ] **Step 2: PR を作成する**

`git push -u origin fix/427-gap-fill-bypass-cache` のあと、`.github/pull_request_template.md` の構成に沿って `gh pr create --body-file` で作る。本文に `Fixes #427` を入れ、末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。マージは `gh pr merge --merge`(squash しない)で、ユーザーの指示があるまで行わない。push 後に CI を Monitor/ポーリングで待たない。
