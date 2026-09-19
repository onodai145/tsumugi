# タブ切替時の意図しないスクロール修正 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** タブを切り替えて戻ったとき（デスクトップの再マウント／モバイルのScroll Snapによるrole変化）に、最後に選択していたノートの位置まで自動スクロールしてしまう問題（Issue #363）を修正する。矢印キーによる選択移動時のスクロール追従は維持し、クリック/タップ選択では一切スクロールしない。

**Architecture:** `TabView` に矢印キー選択移動専用の世代カウンタ `selectionMoveSeq` を追加し、`#moveSelection`（`note.next`/`note.prev` キーバインド）の中でのみインクリメントする。`Column.svelte` からこの値を `NoteCard` に prop として渡し、`NoteCard` の `$effect` は「`selected` かつ `selectionMoveSeq` がマウント時から変化した」場合にのみ `scrollIntoView` する。タブの再マウントやモバイルの role 変化（prev/next→active）では `selectionMoveSeq` が変わらないため、スクロールは発生しない。

**Tech Stack:** Svelte 5 (runes: `$state`/`$props`/`$effect`), TypeScript, Vitest + @testing-library/svelte

## Global Constraints

- 既存の設計ドキュメント: `docs/superpowers/specs/2026-09-19-tab-switch-scroll-fix-design.md`（この計画はこの設計に従う）
- 作業ブランチ `fix/363-tab-switch-scroll` は作成済み。新たなブランチ作成は不要。
- コミットメッセージは件名のみ（本文・箇条書き禁止）。`Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` を末尾に付ける。
- フロントエンドのテストは `cd frontend && pnpm test <対象ファイルパス>` で実行する（`vitest run` のラッパー）。
- `frontend/src/lib/store.svelte.ts` の `TabView` インターフェースを変更する場合、同ファイル内の唯一のリテラル構築箇所（`#makeTab`、347行目付近）と、`frontend/src/lib/store.svelte.test.ts` 内の `TabView` フィクスチャ関数（`makeNotificationOnlyTab`・`makeNormalTab`・`makeNoteTab`）を同時に更新しないと型エラーになる。

---

### Task 1: `TabView.selectionMoveSeq` の追加と矢印キー選択移動での増分

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:52-79`（`TabView` インターフェース）
- Modify: `frontend/src/lib/store.svelte.ts:346-367`（`#makeTab`）
- Modify: `frontend/src/lib/store.svelte.ts:563-570`（`#moveSelection`）
- Modify: `frontend/src/lib/store.svelte.test.ts:84-104`（`makeNotificationOnlyTab`）
- Modify: `frontend/src/lib/store.svelte.test.ts:358-377`（`makeNormalTab`）
- Modify: `frontend/src/lib/store.svelte.test.ts:453-471`（`makeNoteTab`）
- Test: `frontend/src/lib/store.svelte.test.ts`（新規 `describe` ブロックを追加）

**Interfaces:**
- Consumes: なし（このタスクが起点）
- Produces: `TabView.selectionMoveSeq: number`（矢印キーで `note.next`/`note.prev` を実行するたびに1ずつ増える。`selectNote` では変化しない）。Task 2 はこのフィールドを `NoteCard` へ prop として渡す。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts` の末尾に以下の `describe` ブロックを追加する（既存の `makeGroup`・`makeNote`・`makeNoteTab` をそのまま使う）。

```ts
describe("矢印キー選択移動とselectionMoveSeq(Issue #363)", () => {
  it("runKeyAction(\"note.next\")はselectedNoteIdを進め、selectionMoveSeqをインクリメントする", () => {
    const notes = [makeNote({ id: "n1" }), makeNote({ id: "n2" })];
    const tab = makeNoteTab(notes, { selectedNoteId: "n1" });
    app.groups = [makeGroup([tab])];
    app.focusedGroupId = "group1";

    expect(tab.selectionMoveSeq).toBe(0);
    app.runKeyAction("note.next");

    expect(tab.selectedNoteId).toBe("n2");
    expect(tab.selectionMoveSeq).toBe(1);
  });

  it("runKeyAction(\"note.prev\")もselectionMoveSeqをインクリメントする", () => {
    const notes = [makeNote({ id: "n1" }), makeNote({ id: "n2" })];
    const tab = makeNoteTab(notes, { selectedNoteId: "n2" });
    app.groups = [makeGroup([tab])];
    app.focusedGroupId = "group1";

    app.runKeyAction("note.prev");

    expect(tab.selectedNoteId).toBe("n1");
    expect(tab.selectionMoveSeq).toBe(1);
  });

  it("selectNote(クリック/タップ選択)ではselectionMoveSeqが変化しない", () => {
    const notes = [makeNote({ id: "n1" }), makeNote({ id: "n2" })];
    const tab = makeNoteTab(notes);
    app.groups = [makeGroup([tab])];

    app.selectNote("tab1", "n2");

    expect(tab.selectedNoteId).toBe("n2");
    expect(tab.selectionMoveSeq).toBe(0);
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm test src/lib/store.svelte.test.ts`
Expected: `selectionMoveSeq` がまだ存在しないため TypeScript 型エラー（`Property 'selectionMoveSeq' does not exist on type 'TabView'`）または `undefined` を返してアサーションが FAIL する。

- [ ] **Step 3: `TabView` に `selectionMoveSeq` を追加する**

`frontend/src/lib/store.svelte.ts:78` の直後（`selectedNoteId: string | null;` の次）に追加:

```ts
  selectedNoteId: string | null;
  /// 矢印キー(note.next/note.prev)で選択を動かすたびに増える世代カウンタ。
  /// NoteCard はこれが変化した時だけ scrollIntoView する(タブの再マウントや
  /// モバイルのrole変化(prev/next→active)では変化しないため、意図しない
  /// 自動スクロールを防ぐ／Issue #363)。selectNote(クリック/タップ選択)では
  /// 増やさない。
  selectionMoveSeq: number;
```

- [ ] **Step 4: `#makeTab` で初期値を設定する**

`frontend/src/lib/store.svelte.ts:365` の `selectedNoteId: null,` の直後に追加:

```ts
      selectedNoteId: null,
      selectionMoveSeq: 0,
```

- [ ] **Step 5: `#moveSelection` でインクリメントする**

`frontend/src/lib/store.svelte.ts:563-570` を以下に置き換える:

```ts
  /// フォーカス中タブの選択を delta 分だけ動かす（ノートタブのみ）。
  #moveSelection(delta: number) {
    const t = this.#focusedTab();
    if (!t || t.kind.type === "notifications" || t.notes.length === 0) return;
    const cur = t.notes.findIndex((n) => n.id === t.selectedNoteId);
    let next = cur < 0 ? 0 : cur + delta;
    next = Math.max(0, Math.min(t.notes.length - 1, next));
    t.selectedNoteId = t.notes[next].id;
    t.selectionMoveSeq++;
  }
```

- [ ] **Step 6: テスト用フィクスチャを更新する**

`frontend/src/lib/store.svelte.test.ts` 内、`selectedNoteId: null,` を含む3箇所（`makeNotificationOnlyTab`・`makeNormalTab`・`makeNoteTab`）それぞれの直後に `selectionMoveSeq: 0,` を追加する。

例（`makeNotificationOnlyTab`、84-104行目付近）:

```ts
function makeNotificationOnlyTab(note: Note): TabView {
  return {
    id: "tab1",
    accountId: ACCOUNT_ID,
    kind: { type: "notifications" },
    title: "通知",
    customTitle: null,
    filter: { kind: "keywords", value: [] },
    notifyDesktop: false,
    notifySound: false,
    notifySoundChoice: "",
    notes: [],
    notifications: [makeNotification({ note })],
    state: "connected",
    loadingMore: false,
    gapMarker: null,
    fillingGap: false,
    selectedNoteId: null,
    selectionMoveSeq: 0,
  };
}
```

`makeNormalTab`（358-377行目付近）と `makeNoteTab`（453-471行目付近）も同様に、`selectedNoteId: null,` の直後へ `selectionMoveSeq: 0,` を1行追加する（どちらも `...overrides` の直前）。

- [ ] **Step 7: テストを実行して成功を確認する**

Run: `cd frontend && pnpm test src/lib/store.svelte.test.ts`
Expected: 新規3件を含む全テストが PASS。

- [ ] **Step 8: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 9: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "$(cat <<'EOF'
feat: 矢印キー選択移動専用のselectionMoveSeqをTabViewに追加
EOF
)

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 2: `NoteCard` のスクロール条件を `selectionMoveSeq` ベースに変更

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte:36-56`（`$props`）
- Modify: `frontend/src/ui/NoteCard.svelte:268-272`（スクロール `$effect`）
- Modify: `frontend/src/ui/Column.svelte:561`（`NoteCard` 呼び出し）
- Modify: `frontend/src/ui/NoteCard.test.ts:1`（`beforeEach` インポート追加）
- Test: `frontend/src/ui/NoteCard.test.ts`（新規 `describe` ブロックを追加）

**Interfaces:**
- Consumes: `TabView.selectionMoveSeq: number`（Task 1で追加済み）
- Produces: `NoteCard` の新規 prop `selectionMoveSeq?: number`（デフォルト `0`）。`Column.svelte` が `tab.selectionMoveSeq` を渡す。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` の1行目を以下に変更し、`beforeEach` を追加する:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
```

ファイル末尾に以下の `describe` ブロックを追加する（既存の `makeNote`/`makeUser` ヘルパーをそのまま使う）:

```ts
describe("キーボード選択移動時のみスクロールする(Issue #363)", () => {
  let scrollIntoViewMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    scrollIntoViewMock = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoViewMock;
  });

  it("selected=trueで初回マウントしてもscrollIntoViewは呼ばれない(タブ再表示時の再マウントを再現)", () => {
    const note = makeNote();
    render(NoteCard, { props: { note, selected: true, selectionMoveSeq: 3 } });

    expect(scrollIntoViewMock).not.toHaveBeenCalled();
  });

  it("マウント後にselectionMoveSeqが変化するとscrollIntoViewが呼ばれる(矢印キー選択移動を再現)", async () => {
    const note = makeNote();
    const { rerender } = render(NoteCard, {
      props: { note, selected: true, selectionMoveSeq: 0 },
    });
    expect(scrollIntoViewMock).not.toHaveBeenCalled();

    await rerender({ note, selected: true, selectionMoveSeq: 1 });

    expect(scrollIntoViewMock).toHaveBeenCalledWith({ block: "nearest" });
  });

  it("selectionMoveSeqが変化せずselectedだけがfalse→trueになってもscrollIntoViewは呼ばれない(モバイルのrole変化: prev/next→activeを再現)", async () => {
    const note = makeNote();
    const { rerender } = render(NoteCard, {
      props: { note, selected: false, selectionMoveSeq: 5 },
    });

    await rerender({ note, selected: true, selectionMoveSeq: 5 });

    expect(scrollIntoViewMock).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm test src/ui/NoteCard.test.ts`
Expected: `selectionMoveSeq` prop が未定義のため2番目のテストが FAIL する（現行実装では `selected` の真偽だけで `scrollIntoView` が呼ばれるため、1番目のテストは意図せず PASS してしまうこともあるが、少なくとも2番目・3番目のいずれかは FAIL する）。

- [ ] **Step 3: `NoteCard` の props に`selectionMoveSeq` を追加する**

`frontend/src/ui/NoteCard.svelte:36-56` を以下に置き換える:

```ts
  // accountId があれば操作ボタンを出す（引用ネスト時は undefined = 表示のみ）
  // tabId/selected はトップレベル表示時のみ（キーボード選択のハイライト/スクロール用）
  // selectionMoveSeq は矢印キー選択移動(TabView.selectionMoveSeq)専用の世代カウンタ。
  // これが変化した時だけスクロールする(Issue #363: タブ再表示による意図しないスクロール防止)。
  // emojiAccountId は絵文字解決専用（操作性に影響しない）。未指定なら accountId を使う。
  let {
    note,
    quoted = false,
    showActions,
    hideReactions = false,
    hideActionBanner = false,
    accountId,
    emojiAccountId,
    tabId,
    selected = false,
    selectionMoveSeq = 0,
  }: {
    note: Note;
    quoted?: boolean;
    showActions?: boolean;
    hideReactions?: boolean;
    hideActionBanner?: boolean;
    accountId?: string;
    emojiAccountId?: string;
    tabId?: string;
    selected?: boolean;
    selectionMoveSeq?: number;
  } = $props();
```

- [ ] **Step 4: スクロール `$effect` を `selectionMoveSeq` ベースに変更する**

`frontend/src/ui/NoteCard.svelte:268-272` を以下に置き換える:

```ts
  // キーボード選択中はスクロールで見える位置へ。selectionMoveSeqが変化した時だけ
  // 発火させる(タブ切替の再マウント/モバイルのrole変化ではselectionMoveSeqが
  // 変わらないため、意図しない自動スクロールを起こさない／Issue #363)。
  let el = $state<HTMLElement | null>(null);
  let lastSeenSelectionMoveSeq = selectionMoveSeq;
  $effect(() => {
    if (selected && el && selectionMoveSeq !== lastSeenSelectionMoveSeq) {
      el.scrollIntoView({ block: "nearest" });
    }
    lastSeenSelectionMoveSeq = selectionMoveSeq;
  });
```

- [ ] **Step 5: `Column.svelte` から `selectionMoveSeq` を渡す**

`frontend/src/ui/Column.svelte:561` を以下に置き換える:

```svelte
        <NoteCard
          {note}
          accountId={tab.accountId}
          tabId={tab.id}
          selected={role === "active" && note.id === tab.selectedNoteId}
          selectionMoveSeq={tab.selectionMoveSeq}
        />
```

- [ ] **Step 6: テストを実行して成功を確認する**

Run: `cd frontend && pnpm test src/ui/NoteCard.test.ts`
Expected: 新規3件を含む全テストが PASS。

- [ ] **Step 7: フロントエンド全体のテストと型チェックを実行する**

Run: `cd frontend && pnpm test && pnpm check`
Expected: 全テスト PASS、型エラーなし。

- [ ] **Step 8: コミット**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/Column.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "$(cat <<'EOF'
fix: タブ切替時に選択ノート位置へ自動スクロールしないようにする
EOF
)

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 3: 実機確認（デスクトップUI / モバイルUI）

**Files:**
- なし（コード変更はなし。手動確認のみ）

**Interfaces:**
- Consumes: Task 1・Task 2 で実装した動作
- Produces: なし（検証結果をユーザーに報告する）

- [ ] **Step 1: `run` スキルでアプリを起動し、デスクトップUIで確認する**

`run` スキル（プロジェクト固有の起動手順）を使ってアプリを起動する。複数タブを持つカラムで、矢印キー（`note.next`/`note.prev` のキーバインド、デフォルトは `j`/`k` または設定を要確認）である程度下の方のノートを選択 → 別タブへ切り替え → 元のタブへ戻る、を行い、画面が選択位置までスクロールせず先頭（最新）付近のままであることを確認する。次に、矢印キーで選択移動した際は画面外のノートへスクロール追従することを確認する（従来機能のデグレがないこと）。

- [ ] **Step 2: モバイルUIで確認する**

`app.useMobileUi()` が真になる条件（ウィンドウ幅を狭める、またはAndroidビルド）でアプリを起動し、複数タブのカラムである程度下の方のノートを矢印キーまたは何らかの手段で選択した状態を作り、横スワイプで隣のタブへ移動→元のタブへスワイプで戻る、を行い、選択位置までスクロールしないことを確認する。

- [ ] **Step 3: クリック/タップ選択では発火しないことを確認する**

ノートをクリック/タップで選択し、別カラムへフォーカスを移してから元のカラムへ戻ってもスクロールが起きないことを確認する（Issueでの追加確認事項）。

- [ ] **Step 4: 起動した開発サーバーを終了する**

検証用に起動した `cargo tauri dev` 等のプロセスを終了する。

---

## Self-Review Notes

- 設計ドキュメントの「修正方針」「テスト」セクションは Task 1・Task 2 でそれぞれ実装・テストがカバーされている。
- 設計ドキュメントの「スコープ外」（スクロール位置記憶オプション）はこの計画に含めていない。
- `TabView` リテラルを構築する箇所は `store.svelte.ts`（`#makeTab`）と `store.svelte.test.ts`（3フィクスチャ関数）のみであることを事前に `grep -rln "selectedNoteId:" frontend/src` で確認済み。他ファイルへの影響なし。
- `Column.svelte` にはユニットテストファイルが存在しないため、Task 2 のテストは `NoteCard.test.ts` の `rerender` で動作確認する。
