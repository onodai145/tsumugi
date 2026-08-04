# 通知カードでのノートアクション有効化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 通知カード(`NotificationCard.svelte`)に埋め込まれたノートに対して、返信・Renote・引用・リアクション・その他メニューのアクションフッターを表示し、通知一覧から直接操作できるようにする。

**Architecture:** `NoteCard.svelte`のアクションフッター表示条件は現在`{#if !quoted && accountId}`となっており、コンパクト表示用の`quoted` propがアクション表示可否も兼ねている。新規prop `showActions?: boolean`(デフォルト `!quoted`)を追加してこの2つの役割を分離し、`NotificationCard.svelte`から`accountId`と`showActions={true}`を明示的に渡すことでアクションフッターを有効化する。バックエンド(Rust)の変更は不要 — `post_note`/`renote`/`react`/`unreact`は既存。

**Tech Stack:** Svelte 5(runes)、TypeScript、vitest + @testing-library/svelte(フロントエンドテスト)。

## Global Constraints

- 対象通知種別は`notification.note`を含むもの全て(`mention`/`reply`/`renote`/`quote`/`reaction`/`pollEnded`)。種別による出し分けは行わない。
- `note`を含まない通知種別(`follow`等)は既存の`{#if n.note}`ガードでそもそも対象外 — 変更不要。
- 既存リアクション一覧の表示可否(`hideReactions`)は変更しない(非表示のまま)。
- リノート内にネストされた引用元ノート(`NoteCard.svelte:331`の`Self`呼び出し)の挙動(アクション非表示)は変更しない。
- バックエンド(Rust)は変更不要。
- テストコマンド: `cd frontend && pnpm test`(vitest)、`cd frontend && pnpm check`(svelte-check + tsc)。

---

### Task 1: `NoteCard.svelte`に`showActions` propを追加してアクション表示可否を`quoted`から分離する

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte:29-47`(props定義)、`frontend/src/ui/NoteCard.svelte:366`(アクションフッター条件)
- Test: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: なし(既存コンポーネントの内部変更)
- Produces: `NoteCard`の新規prop `showActions?: boolean`(デフォルト `!quoted`)。Task 2で`NotificationCard.svelte`から`showActions={true}`として利用される。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts`の末尾、`describe("NoteCard action banner", ...)`ブロックの後に新しい`describe`ブロックを追加する:

```ts
describe("NoteCard showActions", () => {
  it("hides the action footer for a quoted note by default even with accountId", () => {
    const note = makeNote({ id: "n1" });
    const { queryByLabelText } = render(NoteCard, {
      props: { note, quoted: true, accountId: "a1" },
    });
    expect(queryByLabelText("返信")).toBeNull();
  });

  it("shows the action footer for a quoted note when showActions is set", () => {
    const note = makeNote({ id: "n1" });
    const { getByLabelText } = render(NoteCard, {
      props: { note, quoted: true, accountId: "a1", showActions: true },
    });
    expect(getByLabelText("返信")).toBeTruthy();
  });

  it("still shows the action footer for a non-quoted note with accountId (unchanged default)", () => {
    const note = makeNote({ id: "n1" });
    const { getByLabelText } = render(NoteCard, {
      props: { note, accountId: "a1" },
    });
    expect(getByLabelText("返信")).toBeTruthy();
  });
});
```

- [ ] **Step 2: テストを実行して失敗することを確認する**

Run: `cd frontend && pnpm test -- NoteCard`
Expected: 1つ目のテストはPASS(既存挙動)、2つ目のテストはFAIL(`showActions`未実装のため`quoted:true`でフッターが出ない)、3つ目はPASS。

- [ ] **Step 3: `showActions` propを実装する**

`frontend/src/ui/NoteCard.svelte:29-47`のprops定義を変更する(変更箇所のみ抜粋、`quoted`と`accountId`の間に`showActions`を追加):

```svelte
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
  } = $props();
```

`showActions`の実効値を導出する`$derived`を、`isPureRenote`/`inner`の定義(`NoteCard.svelte:50-51`)の直後に追加する:

```svelte
  // quoted はスタイリング(コンパクト表示)専用。アクション表示可否は showActions で制御し、
  // 未指定時は従来通り !quoted にフォールバックする。
  const effectiveShowActions = $derived(showActions ?? !quoted);
```

`frontend/src/ui/NoteCard.svelte:366`のアクションフッター条件を変更する:

```svelte
      {#if effectiveShowActions && accountId}
        <footer class="actions">
```

- [ ] **Step 4: テストを実行してパスすることを確認する**

Run: `cd frontend && pnpm test -- NoteCard`
Expected: `NoteCard action banner`・`NoteCard showActions`の全テストがPASS。

- [ ] **Step 5: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 6: コミット**

```bash
cd frontend && git add src/ui/NoteCard.svelte src/ui/NoteCard.test.ts
git commit -m "feat: NoteCardにshowActions propを追加してquotedからアクション表示を分離"
```

---

### Task 2: `NotificationCard.svelte`から`accountId`と`showActions`を渡してアクションフッターを有効化する

**Files:**
- Modify: `frontend/src/ui/NotificationCard.svelte:92`
- Test: `frontend/src/ui/NotificationCard.test.ts`(新規作成)

**Interfaces:**
- Consumes: Task 1で追加した`NoteCard`の`showActions?: boolean` prop。
- Produces: なし(末端のUI配線)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/NotificationCard.test.ts`を新規作成する(`NoteCard.test.ts`と同じmock/helperパターンに倣う):

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import type { Note, Notification, User } from "../bindings/tauri.gen";

// store.svelte.ts が起動時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外(jsdom)で import が失敗しないようスタブする。
vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: NotificationCard } = await import("./NotificationCard.svelte");

afterEach(() => cleanup());

function makeUser(overrides: Partial<User> = {}): User {
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
    ...overrides,
  };
}

function makeNote(overrides: Partial<Note> = {}): Note {
  return {
    id: "n1",
    createdAt: 0,
    text: "hello",
    cw: null,
    visibility: "public",
    localOnly: false,
    user: makeUser({ id: "u2", name: "Bob" }),
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
    ...overrides,
  };
}

function makeNotification(overrides: Partial<Notification> = {}): Notification {
  return {
    id: "notif1",
    createdAt: 0,
    type: "mention",
    user: makeUser({ id: "u2", name: "Bob" }),
    note: makeNote(),
    reaction: null,
    ...overrides,
  };
}

describe("NotificationCard note actions", () => {
  it("shows the action footer for a mention notification when accountId is given", () => {
    const notification = makeNotification({ type: "mention" });
    const { getByLabelText } = render(NotificationCard, {
      props: { notification, accountId: "a1" },
    });
    expect(getByLabelText("返信")).toBeTruthy();
  });

  it("does not show the action footer without accountId", () => {
    const notification = makeNotification({ type: "mention" });
    const { queryByLabelText } = render(NotificationCard, {
      props: { notification },
    });
    expect(queryByLabelText("返信")).toBeNull();
  });

  it("does not render a note preview for note-less notifications like follow", () => {
    const notification = makeNotification({ type: "follow", note: null });
    const { container } = render(NotificationCard, {
      props: { notification, accountId: "a1" },
    });
    expect(container.querySelector(".note-preview")).toBeNull();
  });
});
```

- [ ] **Step 2: テストを実行して失敗することを確認する**

Run: `cd frontend && pnpm test -- NotificationCard`
Expected: 1つ目のテスト`shows the action footer for a mention notification when accountId is given`がFAIL(現状`NoteCard`呼び出しに`accountId`/`showActions`が渡っていないためフッターが出ない)。他の2テストはPASS。

- [ ] **Step 3: `NotificationCard.svelte`の`NoteCard`呼び出しを修正する**

`frontend/src/ui/NotificationCard.svelte:92`を変更する:

```svelte
      <NoteCard
        note={n.note}
        quoted={true}
        showActions={true}
        hideReactions
        hideActionBanner
        accountId={accountId}
        emojiAccountId={accountId}
      />
```

- [ ] **Step 4: テストを実行してパスすることを確認する**

Run: `cd frontend && pnpm test -- NotificationCard`
Expected: `NotificationCard note actions`の全テストがPASS。

- [ ] **Step 5: フロントエンド全体のテストと型チェックを実行する**

Run: `cd frontend && pnpm test && pnpm check`
Expected: 全テストPASS、型エラーなし(既存のNoteCard呼び出し箇所である`Column.svelte`および`NoteCard.svelte:331`の`Self`呼び出しは変更していないため、既存テストに影響しないはず)。

- [ ] **Step 6: コミット**

```bash
cd frontend && git add src/ui/NotificationCard.svelte src/ui/NotificationCard.test.ts
git commit -m "feat: 通知カードのノートに返信/Renote/リアクションボタンを表示"
```

---

### Task 3: 実機確認(`cargo tauri dev`)

**Files:** なし(手動検証のみ)

**Interfaces:**
- Consumes: Task 1・Task 2の変更一式。
- Produces: なし。

- [ ] **Step 1: dev環境を起動する**

Run: `cargo tauri dev`(リポジトリルートで実行。Wayland環境で`Gdk Error 71`が出る場合は`GDK_BACKEND=x11 cargo tauri dev`にフォールバック)

- [ ] **Step 2: 通知カラムを表示し、各通知種別でアクションフッターを確認する**

通知カラムを開き、`mention`/`reply`/`renote`/`quote`/`reaction`/`pollEnded`の各通知タイプ(実際に自アカウントへの言及・返信・Renote・引用・リアクション・投票終了を発生させて確認、またはテスト用インスタンスの既存通知で代替)で、ノートプレビューの下にアクションフッター(返信・Renote・引用・リアクション・その他メニュー)が表示されることを目視確認する。

- [ ] **Step 3: 各アクションの動作を確認する**

- 返信ボタン押下 → コンポーズが正しい返信先(通知内のノート)で開くこと。
- Renote/引用ボタン押下 → 正常に投稿されること(可視性が`followers`/`specified`のノートでは非表示になること)。
- リアクションボタン押下 → ピッカーが開き、選択したリアクションが送信されること。
- `follow`など`note`を含まない通知でアクションフッターが出ないこと(regressionなし)。

- [ ] **Step 4: タイムラインでのリグレッション確認**

任意のカラムでRenote(本文なし)を含むノートを表示し、ネストされた引用元ノートにアクションボタンが出ていないこと(`NoteCard.svelte:331`の`Self`呼び出しの挙動が変わっていないこと)を目視確認する。

問題があればTask 1・2に戻って修正する。問題なければ完了。
