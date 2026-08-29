# ノート本文コピー機能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ノートの「…」メニューに「内容をコピー」項目を追加し、`note.text`（MFM記法込みの生テキスト、nyaize変換前）をクリップボードにコピーできるようにする。

**Architecture:** `frontend/src/ui/NoteMenu.svelte` に新しいボタンを1つ追加するだけの単一コンポーネント変更。`navigator.clipboard.writeText()` を直接呼ぶフロントエンド完結の機能で、Rust側・bindings側の変更は不要。

**Tech Stack:** Svelte 5 (runes)、`@lucide/svelte`（アイコン）、Vitest + `@testing-library/svelte`（テスト）。

## Global Constraints

- 対象テキストは `note.text` のみ。CW (`note.cw`) は含めない。
- `note.text` が `null` または空文字列の場合はメニュー項目自体を表示しない。
- メニュー項目のラベルは「内容をコピー」（他のラベルは不可）。
- アイコンは `@lucide/svelte` の `Copy`。
- 配置は既存の「お気に入り登録」ボタンの直前（メニュー先頭）。
- コミットメッセージは subject line のみ（本文・箇条書き禁止、Co-Authored-Byは別途付与される）。

---

### Task 1: NoteMenuに「内容をコピー」項目を追加

**Files:**
- Modify: `frontend/src/ui/NoteMenu.svelte`
- Test: `frontend/src/ui/NoteCard.test.ts`（既存ファイルに追記。NoteMenu単体のテストファイルはこのリポジトリに前例がなく、NoteCard経由で「その他」メニューを開いて検証する既存パターンに合わせる）

**Interfaces:**
- Consumes: `NoteMenu.svelte` の既存props `{ accountId: string; note: Note; onclose: () => void }`（変更なし）。`Note` 型（`frontend/src/bindings/tauri.gen.ts`）の `text: string | null` フィールド。
- Produces: なし（末端のUI変更、他タスクへの依存なし）。

- [ ] **Step 1: 失敗するテストを書く（本文がある場合に項目を表示し、クリックでクリップボードに書き込む）**

`frontend/src/ui/NoteCard.test.ts` の既存の削除系テスト群の近くに追記する（`makeNote`/`makeUser`ヘルパーは既存のものをそのまま使う）。`navigator.clipboard.writeText` は jsdom に実装がないため `vi.fn()` でスタブする。

```ts
  it("本文がある投稿では「内容をコピー」項目を表示し、クリックでクリップボードにコピーする", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const note = makeNote({ text: "**bold** です" });
    const { getByLabelText, getByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();
    await getByText("内容をコピー").click();

    expect(writeText).toHaveBeenCalledWith("**bold** です");
  });

  it("本文が空のノートでは「内容をコピー」項目を表示しない", async () => {
    const note = makeNote({ text: null });
    const { getByLabelText, queryByText } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    await getByLabelText("その他").click();

    expect(queryByText("内容をコピー")).toBeNull();
  });
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "内容をコピー"`
Expected: FAIL（"内容をコピー" というテキストの要素が見つからない）

- [ ] **Step 3: NoteMenu.svelteに実装を追加する**

`frontend/src/ui/NoteMenu.svelte` の import 部分を変更:

```ts
  import { Star, Paperclip, ChevronRight, Trash2, Copy } from "@lucide/svelte";
```

`toggleFavorite` 関数の直前に関数を追加:

```ts
  function copyText() {
    if (note.text) {
      navigator.clipboard.writeText(note.text);
    }
    onclose();
  }
```

テンプレートの一番上（`<div class="w-[200px] ...">` の直下、「お気に入り登録」ボタンの直前）にボタンを追加:

```svelte
  {#if note.text}
    <button type="button" class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted" onclick={copyText}>
      <Copy size={16} />
      内容をコピー
    </button>
  {/if}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts`
Expected: PASS（追加した2件を含め全件成功）

- [ ] **Step 5: フロントエンド全体のチェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 6: コミット**

```bash
git add frontend/src/ui/NoteMenu.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: ノートメニューに内容コピー項目を追加"
```

---

## Manual Verification (実装後、人手で確認)

`cargo tauri dev` で実機起動し、以下を確認する:
- MFM記法（太字・リンク・カスタム絵文字など）を含むノートの「…」メニューから「内容をコピー」を選び、任意のテキストフィールドに貼り付けて生のMFM記法が得られることを確認する。
- 猫アカウント（`isCat: true`）のノートで、貼り付け結果が語尾変換前（nyaize前）であることを確認する。
- メディアのみでテキストが空のノートでは「内容をコピー」がメニューに出ないことを確認する。
