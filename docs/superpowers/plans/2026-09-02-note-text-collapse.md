# ノート本文の折りたたみ表示 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `NoteCard.svelte` で本文が300文字を超える場合、初期表示を高さ制限で折りたたみ、「もっと見る」ボタンで全文展開できるようにする。

**Architecture:** `NoteCard.svelte` 内にローカルの `$derived`（文字数判定）と `$state`（展開済みフラグ）を追加し、既存の `{#if inner.text}` ブロックのテンプレートとスタイルのみを変更する。他コンポーネント・他ファイルへの変更は無い。

**Tech Stack:** Svelte 5 (runes: `$derived` / `$state`)、Vitest + @testing-library/svelte（既存の `NoteCard.test.ts` に追加）。

## Global Constraints

- 判定基準: 本文の文字数（`[...text].length`、サロゲートペア対応）が **300文字を超える**場合に折りたたむ（300文字ちょうどは折りたたまない）。
- 展開後に再度畳み直すUIは提供しない。
- CW（Content Warning）と独立した層として重ねる。CWを開いた結果の本文が300文字超なら折りたたみを適用する。
- 引用Renoteのネスト表示（`quoted=true` で `NoteCard` が自分自身を子として描画するケース）にも同じロジックがそのまま効くこと（追加の分岐は設けない）。
- 新規のborder-radius/色トークンを増やさない。展開ボタンの見た目は既存の `.cw-toggle` クラスを流用する。
- 対象ファイルは `frontend/src/ui/NoteCard.svelte` と `frontend/src/ui/NoteCard.test.ts` のみ。

---

### Task 1: 本文折りたたみ機能の実装

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`（本文表示部の `$derived`/`$state` 追加、テンプレート、`<style>` を変更）
- Test: `frontend/src/ui/NoteCard.test.ts`（新規 `describe` ブロックを追加）

**Interfaces:**
- Consumes: 既存の `inner`（`$derived`, `Note`型。`isPureRenote ? note.renote! : note`）、既存の `handleNyaizeCopy`、既存の `emojiMap`。
- Produces: このタスクのみで完結。他タスクへの依存インターフェースは無し。

- [ ] **Step 1: 「301文字では初期状態で『もっと見る』ボタンを表示する」の失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` の末尾（最後の `describe("投稿時刻の自動更新", ...)` ブロックの後）に追加:

```ts
describe("本文の折りたたみ", () => {
  it("301文字では初期状態で「もっと見る」ボタンを表示し、本文コンテナに折りたたみクラスが付く", () => {
    const note = makeNote({ text: "あ".repeat(301) });
    const { container, getByTestId } = render(NoteCard, { props: { note } });

    expect(getByTestId("note-text-expand")).toBeTruthy();
    const textEl = getByTestId("note-text");
    expect(textEl.classList.contains("note-text-collapsed")).toBe(true);
    void container;
  });
});
```

- [ ] **Step 2: テストを実行し、失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "301文字では初期状態で"`
Expected: FAIL（`note-text-expand` の要素が見つからない、または `getByTestId` がスロー）

- [ ] **Step 3: 折りたたみ判定・状態・テンプレート・スタイルを実装する**

`frontend/src/ui/NoteCard.svelte` の `<script>` 内、`pollExpired`/`pollAlreadyVoted` の宣言（既存の179〜180行目付近）の直前に追加:

```js
// 本文が長すぎる場合に折りたたむ(Issue #252)。文字数のみで判定し、改行数は見ない。
// サロゲートペア(絵文字等)を1文字として数えるためスプレッド展開でカウントする。
const TEXT_COLLAPSE_THRESHOLD = 300;
const isLongText = $derived(!!inner.text && [...inner.text].length > TEXT_COLLAPSE_THRESHOLD);
// 一度展開したら畳み直す操作は提供しない(CWのトグルと違い往復させる必要性が薄いため)。
let textExpanded = $state(false);
```

同ファイル、本文表示部分（既存の393〜395行目付近、次の内容）:

```svelte
        {#if inner.text}
          <div class="mt-px whitespace-pre-wrap break-words text-sm leading-[1.42] [-webkit-user-select:text] select-text" data-testid="note-text" oncopy={handleNyaizeCopy}><Mfm text={inner.text} emojis={emojiMap} nyaize={inner.user.isCat} /></div>
        {/if}
```

を、次に置き換える:

```svelte
        {#if inner.text}
          <div class="relative">
            <div
              class={isLongText && !textExpanded
                ? "note-text-collapsed mt-px overflow-hidden whitespace-pre-wrap break-words text-sm leading-[1.42] [-webkit-user-select:text] select-text"
                : "mt-px whitespace-pre-wrap break-words text-sm leading-[1.42] [-webkit-user-select:text] select-text"}
              data-testid="note-text"
              oncopy={handleNyaizeCopy}
            ><Mfm text={inner.text} emojis={emojiMap} nyaize={inner.user.isCat} /></div>
            {#if isLongText && !textExpanded}
              <div class="note-text-fade pointer-events-none absolute inset-x-0 bottom-0 h-10"></div>
            {/if}
          </div>
          {#if isLongText && !textExpanded}
            <button
              type="button"
              class="cw-toggle mt-1 rounded-md border border-border px-2 py-px text-sm text-foreground"
              data-testid="note-text-expand"
              onclick={() => (textExpanded = true)}
            >
              もっと見る
            </button>
          {/if}
        {/if}
```

同ファイル末尾の `<style>` ブロック内、`.cw-toggle { ... }` の直後に追加:

```css
  .note-text-collapsed {
    max-height: 150px;
  }
  .note-text-fade {
    background: linear-gradient(
      to bottom,
      transparent,
      color-mix(in srgb, var(--surface-1) var(--column-opacity, 100%), transparent)
    );
  }
```

- [ ] **Step 4: テストを実行し、成功することを確認する**

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "301文字では初期状態で"`
Expected: PASS

- [ ] **Step 5: 「300文字ちょうどでは表示しない」のテストを追加し実行する**

Step 1で追加した `describe("本文の折りたたみ", ...)` ブロック内に追加:

```ts
  it("300文字ちょうどでは「もっと見る」ボタンを表示しない", () => {
    const note = makeNote({ text: "あ".repeat(300) });
    const { queryByTestId, getByTestId } = render(NoteCard, { props: { note } });

    expect(queryByTestId("note-text-expand")).toBeNull();
    expect(getByTestId("note-text").classList.contains("note-text-collapsed")).toBe(false);
  });
```

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "本文の折りたたみ"`
Expected: PASS（2件とも）

- [ ] **Step 6: 「クリックで全文表示になりボタンが消える」テストを追加し実行する**

同ブロックに追加:

```ts
  it("「もっと見る」をクリックすると全文表示になりボタンが消える", async () => {
    const note = makeNote({ text: "あ".repeat(301) });
    const { getByTestId, queryByTestId } = render(NoteCard, { props: { note } });

    await getByTestId("note-text-expand").click();

    expect(queryByTestId("note-text-expand")).toBeNull();
    expect(getByTestId("note-text").classList.contains("note-text-collapsed")).toBe(false);
  });
```

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "本文の折りたたみ"`
Expected: PASS（3件とも）

- [ ] **Step 7: CW併用時のテストを追加し実行する**

同ブロックに追加:

```ts
  it("CWを開いた結果の本文が長文の場合も折りたたみが効く", async () => {
    const note = makeNote({ cw: "注意書き", text: "あ".repeat(301) });
    const { getByText, getByTestId, queryByTestId } = render(NoteCard, {
      props: { note, accountId: "acc1" },
    });

    expect(queryByTestId("note-text-expand")).toBeNull(); // CWが閉じている間は本文自体が無い

    await getByText("続きを見る").click();

    expect(getByTestId("note-text-expand")).toBeTruthy();
  });
```

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "本文の折りたたみ"`
Expected: PASS（4件とも）

- [ ] **Step 8: 引用Renoteのネスト表示でも折りたたみが効くテストを追加し実行する**

同ブロックに追加:

```ts
  it("引用Renoteのネスト表示でも長文の折りたたみが効く", () => {
    const note = makeNote({
      text: "見て",
      renote: makeNote({ id: "n-quoted", text: "い".repeat(301) }),
    });
    const { container } = render(NoteCard, { props: { note } });

    expect(container.querySelectorAll('[data-testid="note-text-expand"]').length).toBe(1);
  });
```

Run: `cd frontend && pnpm vitest run src/ui/NoteCard.test.ts -t "本文の折りたたみ"`
Expected: PASS（5件とも）

- [ ] **Step 9: ファイル全体のテストとtype checkを実行する**

Run:
```bash
cd frontend && pnpm vitest run src/ui/NoteCard.test.ts
cd frontend && pnpm check
```
Expected: 両方成功（既存テストも含め回帰なし）

- [ ] **Step 10: コミットする**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat: ノート本文が長すぎるときに折りたたむ"
```
