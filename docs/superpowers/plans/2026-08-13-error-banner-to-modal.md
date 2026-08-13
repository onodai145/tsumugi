# エラーバナー廃止→モーダル/ログ振り分け Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Issue #183 対応。投稿欄の下のグローバルエラーバナー(`app.error`)を廃止し、ユーザー直接操作の失敗はモーダルで、それ以外はBackstageログのみで通知する。

**Architecture:** `store.svelte.ts` に `errorModal` state と `#failModal()` を新設し、既存の `#fail()` 呼び出し28箇所を「ユーザー直接操作(7箇所)→`#failModal()`」「それ以外(21箇所)→`#logFailure()`」に振り分けたのち `#fail()` と `error` state を削除する。`App.svelte` はバナーを削除し、`errorModal` を使った `Modal.svelte` ベースの表示に置き換える。

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest。

## Global Constraints

- 対象範囲は仕様書 `docs/superpowers/specs/2026-08-13-error-banner-to-modal-design.md` に一致させる。新規モーダル対象はユーザー直接操作(Renote/リアクション/お気に入り/投票/クリップ追加/タブ名変更/カラム通知設定変更)の7箇所のみ。
- re-throw の有無、楽観的更新のロールバック処理、各メソッドのシグネチャは変更しない。変更するのは失敗時に呼ぶメソッド(`#fail` → `#failModal` または `#logFailure`)のみ。
- ComposeBar.svelte / AddColumnModal.svelte / NoteMenu.svelte 等、既に独自のエラー表示を持つコンポーネントの表示ロジックは変更しない。
- コミットメッセージは件名のみ(本文・箇条書きなし)。ブランチは `feat/issue-183-error-banner-to-modal`(作成済み)。

---

### Task 1: `errorModal` state と `#failModal()` を追加し、`#fail()` を削除する

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:120`(`error` state 削除、`errorModal` 追加)
- Modify: `frontend/src/lib/store.svelte.ts:373-379`(`#fail()` 削除、`#failModal()` 追加)
- Modify: `frontend/src/lib/store.svelte.ts:249,254,391,420,440,623,649,661,671,685,702,766,775,1020,1067,1076,1085,1095,1460,1501,1511,1521,1580,1608,1616,1627,1637,1652`(全 `#fail(e)` 呼び出し箇所)
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: 既存の `#log(level, text, reauthAccountId?)`, `#logFailure(e)`, `ForbiddenError`(いずれも同ファイル内で定義済み、変更なし)
- Produces: `errorModal: string | null`(state)、`#failModal(e: unknown): void`(privateメソッド。Task 2以降で呼び出す)。`error` state と `#fail()` は以後存在しない。

- [ ] **Step 1: `error` state を `errorModal` に置き換える**

`frontend/src/lib/store.svelte.ts:120` を以下に変更:

```ts
  errorModal = $state<string | null>(null);
```

(元の `error = $state<string | null>(null);` を置き換え)

- [ ] **Step 2: `#fail()` を `#failModal()` に置き換える**

`frontend/src/lib/store.svelte.ts:373-379` の

```ts
  /// エラーをバナー表示＋Backstage へ記録する共通処理。
  /// ForbiddenError なら「再認証」アクションをログ行に付与する。
  #fail(e: unknown) {
    const msg = String(e);
    this.error = msg;
    this.#log("error", msg, e instanceof ForbiddenError ? e.accountId : undefined);
  }
```

を以下に置き換える:

```ts
  /// エラーをモーダル表示＋Backstage へ記録する共通処理。ユーザーの直接操作(Renote・
  /// リアクション・お気に入り・投票・クリップ追加・タブ名変更・カラム通知設定変更)の
  /// 失敗にのみ使う。それ以外は #logFailure を使う(Issue #183: 投稿欄下のグローバル
  /// バナーは廃止し、ユーザー注意が必要な場合のみモーダルで通知する)。
  /// ForbiddenError なら「再認証」アクションをログ行に付与する。
  #failModal(e: unknown) {
    const msg = String(e);
    this.errorModal = msg;
    this.#log("error", msg, e instanceof ForbiddenError ? e.accountId : undefined);
  }
```

- [ ] **Step 3: `reportError()` を `#logFailure` ベースに変更する**

`frontend/src/lib/store.svelte.ts:389-392` の

```ts
  /// store の非同期フロー外(単発の子コンポーネント操作等)から失敗を報告する共通口。
  reportError(e: unknown) {
    this.#fail(e);
  }
```

を以下に置き換える(MediaGridの画像読み込み失敗など、背景処理からの呼び出し用。モーダルは出さない):

```ts
  /// store の非同期フロー外(単発の子コンポーネント操作等)から失敗を報告する共通口。
  /// 背景処理からの呼び出しのためモーダルは出さず、Backstageログにのみ記録する。
  reportError(e: unknown) {
    this.#logFailure(e);
  }
```

- [ ] **Step 4: モーダル対象7箇所を `#failModal(e)` に置き換える**

以下の行の `this.#fail(e);` を `this.#failModal(e);` に置き換える(該当メソッド名をコメントで併記):

- `:420`(`renameTab`)
- `:440`(`setColumnNotify`)
- `:1511`(`renote`)
- `:1580`(`toggleReaction`)
- `:1608`(`toggleFavorite`)
- `:1637`(`addNoteToClip`)
- `:1652`(`votePoll`)

各箇所は `this.#fail(e);` → `this.#failModal(e);` の1行置換のみ(前後のロジックは変更しない)。

- [ ] **Step 5: 残り21箇所を `#logFailure(e)` に置き換える**

以下の行の `this.#fail(e);` を `this.#logFailure(e);` に置き換える:

- `:249`, `:254`(起動時のカラム再開)
- `:623`(`moveTab`)
- `:649`(`endDragGroup`)
- `:661`(`persistGroupWidth`)
- `:671`(`setGroupAuto`)
- `:685`(`splitPane`)
- `:702`(`discardEmptyGroup`)
- `:766`(`setPaneAuto`)
- `:775`(`resizePane`)
- `:1020`(`addColumn`, re-throw)
- `:1067`(`fetchUserLists`, re-throw)
- `:1076`(`fetchAntennas`, re-throw)
- `:1085`(`fetchChannels`, re-throw)
- `:1095`(`resolveUser`, re-throw)
- `:1460`(`loadMore`)
- `:1501`(`postNote`, re-throw)
- `:1521`(`deleteNote`, re-throw)
- `:1616`(`listClips`, re-throw)
- `:1627`(`createClip`, re-throw)

各箇所は `this.#fail(e);` → `this.#logFailure(e);` の1行置換のみ(re-throwの有無・前後のロジックは変更しない)。

- [ ] **Step 6: `#fail` が完全に置き換わったことを確認する**

```bash
grep -n "#fail(e)" frontend/src/lib/store.svelte.ts
```

Expected: マッチなし(0件)。もし `#fail(` を定義するメソッド自体の宣言行が残っていたら、Step 2 で削除し忘れているので確認する。

```bash
grep -n "#fail(" frontend/src/lib/store.svelte.ts
```

Expected: マッチなし(0件)。

- [ ] **Step 7: 既存テストを新state名に合わせて更新する**

`frontend/src/lib/store.svelte.test.ts:222-234` の

```ts
  // ProfileModal/FollowListModal は自前のエラー表示を持つため、これらのメソッドは失敗時に
  // グローバルバナー(app.error)を出さずBackstageログ(app.logs)にのみ記録する必要がある
  // (投稿欄の下に重複してエラーが出る、というユーザー報告への修正の回帰テスト)。
  it("getUserProfileが失敗してもapp.errorは変化せずBackstageに記録される", async () => {
    app.error = null;
    const logsBefore = app.logs.length;
    invokeMock.mockRejectedValueOnce(new Error("boom"));
    await expect(app.getUserProfile(ACCOUNT_ID, "u1")).rejects.toThrow("boom");
    expect(app.error).toBeNull();
    expect(app.logs.length).toBe(logsBefore + 1);
    // #log は新しいログを先頭に追加するため、最新エントリはlogs[0]。
    expect(app.logs[0].level).toBe("error");
  });
```

を以下に置き換える:

```ts
  // ProfileModal/FollowListModal は自前のエラー表示を持つため、これらのメソッドは失敗時に
  // グローバルエラーモーダル(app.errorModal)を出さずBackstageログ(app.logs)にのみ記録する
  // 必要がある(投稿欄の下に重複してエラーが出る、というユーザー報告への修正の回帰テスト)。
  it("getUserProfileが失敗してもapp.errorModalは変化せずBackstageに記録される", async () => {
    app.errorModal = null;
    const logsBefore = app.logs.length;
    invokeMock.mockRejectedValueOnce(new Error("boom"));
    await expect(app.getUserProfile(ACCOUNT_ID, "u1")).rejects.toThrow("boom");
    expect(app.errorModal).toBeNull();
    expect(app.logs.length).toBe(logsBefore + 1);
    // #log は新しいログを先頭に追加するため、最新エントリはlogs[0]。
    expect(app.logs[0].level).toBe("error");
  });
```

- [ ] **Step 8: テストを実行する**

```bash
cd frontend && pnpm exec vitest run src/lib/store.svelte.test.ts
```

Expected: 全テストPASS。

- [ ] **Step 9: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "refactor: エラー通知をバナーからモーダル/ログ振り分けに変更"
```

---

### Task 2: ユーザー直接操作7メソッドの `errorModal` セットを検証する回帰テストを追加する

**Files:**
- Modify: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: Task 1 で追加した `app.errorModal`(state)。既存テストファイル内の `invokeMock`, `ACCOUNT_ID`, `makeNote`(既存ヘルパ、ファイル冒頭で定義済み)。
- Produces: なし(テストのみ追加)。

- [ ] **Step 1: 既存ヘルパを確認する**

`frontend/src/lib/store.svelte.test.ts:1-108` には次のヘルパが既にある(新規に作らず、これらを使う):
- `makeUser(overrides)`, `makeNote(overrides)`, `makeNotification(overrides)`
- `makeNotificationOnlyTab(note)`: `notes: []` / `notifications: [makeNotification({ note })]` の `TabView` を返す
- `makeGroup(tabs)`: `GroupView` を返す(`activeTabId` は `tabs[0]?.id`)

`notes` に直接ノートを持つ通常のタブが必要な場合は `makeNotificationOnlyTab` と同じ形の `TabView` リテラルを、`notes: [note]`, `notifications: []` にして直接書く(下記Step 2のコードで使用)。

- [ ] **Step 2: 失敗テストを書く(モーダル対象7メソッド)**

`frontend/src/lib/store.svelte.test.ts` の末尾(既存の最後の `describe` ブロックの後)に以下を追加する:

```ts
function makeNormalTab(overrides: Partial<TabView> = {}): TabView {
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
    notes: [],
    notifications: [],
    state: "connected",
    loadingMore: false,
    selectedNoteId: null,
    ...overrides,
  };
}

describe("ユーザー直接操作の失敗はerrorModalに記録される(Issue #183)", () => {
  beforeEach(() => {
    app.errorModal = null;
  });

  it("renoteが失敗するとerrorModalがセットされる", async () => {
    invokeMock.mockRejectedValueOnce(new Error("renote failed"));
    await app.renote(ACCOUNT_ID, "note1");
    expect(app.errorModal).toContain("renote failed");
  });

  it("toggleReactionが失敗するとerrorModalがセットされる", async () => {
    const note = makeNote({ id: "note-react", myReaction: null });
    app.groups = [makeGroup([makeNormalTab({ notes: [note] })])];
    invokeMock.mockRejectedValueOnce(new Error("react failed"));
    await app.toggleReaction(ACCOUNT_ID, "note-react", "👍");
    expect(app.errorModal).toContain("react failed");
  });

  it("toggleFavoriteが失敗するとerrorModalがセットされる", async () => {
    const note = makeNote({ id: "note-fav", isFavoritedByMe: false });
    app.groups = [makeGroup([makeNormalTab({ notes: [note] })])];
    invokeMock.mockRejectedValueOnce(new Error("favorite failed"));
    await app.toggleFavorite(ACCOUNT_ID, "note-fav");
    expect(app.errorModal).toContain("favorite failed");
  });

  it("votePollが失敗するとerrorModalがセットされる", async () => {
    const note = makeNote({
      id: "note-poll",
      poll: { choices: [{ text: "a", votes: 0, isVoted: false }], multiple: false, expiresAt: null },
    });
    app.groups = [makeGroup([makeNormalTab({ notes: [note] })])];
    invokeMock.mockRejectedValueOnce(new Error("vote failed"));
    await app.votePoll(ACCOUNT_ID, "note-poll", 0);
    expect(app.errorModal).toContain("vote failed");
  });

  it("addNoteToClipが失敗するとerrorModalがセットされる", async () => {
    invokeMock.mockRejectedValueOnce(new Error("clip failed"));
    await app.addNoteToClip(ACCOUNT_ID, "clip1", "note1");
    expect(app.errorModal).toContain("clip failed");
  });

  it("renameTabが失敗するとerrorModalがセットされる", async () => {
    app.groups = [makeGroup([makeNormalTab()])];
    invokeMock.mockRejectedValueOnce(new Error("rename failed"));
    await app.renameTab("tab1", "新しい名前");
    expect(app.errorModal).toContain("rename failed");
  });

  it("setColumnNotifyが失敗するとerrorModalがセットされる", async () => {
    app.groups = [makeGroup([makeNormalTab()])];
    invokeMock.mockRejectedValueOnce(new Error("notify failed"));
    await app.setColumnNotify("tab1", true, true, "default");
    expect(app.errorModal).toContain("notify failed");
  });
});
```

`kind: { type: "home" }` や `filter: { kind: "keywords", value: [] }` が `ColumnKind` / `FilterQuery` の型と一致しない場合は、`pnpm check` のエラーに従い `frontend/src/bindings/tauri.gen.ts` の実際の型定義を見て値を合わせる(既存の `makeNotificationOnlyTab` が使っている値と揃えれば型は一致するはず)。

- [ ] **Step 3: テストを実行し、全てPASSすることを確認する**

```bash
cd frontend && pnpm exec vitest run src/lib/store.svelte.test.ts
```

Expected: 追加した7件を含め全テストPASS。

- [ ] **Step 4: ドラッグ/ペイン系・loadMore・起動時再開は`errorModal`を変化させないことを確認する回帰テストを1件追加する**

同じ `describe` ブロック内に追加:

```ts
  it("persistGroupWidthが失敗してもerrorModalは変化しない(レイアウト操作は対象外)", async () => {
    invokeMock.mockRejectedValueOnce(new Error("width failed"));
    await app.persistGroupWidth("g1", 300);
    expect(app.errorModal).toBeNull();
  });
```

- [ ] **Step 5: テストを再実行する**

```bash
cd frontend && pnpm exec vitest run src/lib/store.svelte.test.ts
```

Expected: 全テストPASS。

- [ ] **Step 6: コミット**

```bash
git add frontend/src/lib/store.svelte.test.ts
git commit -m "test: エラーモーダル対象/非対象操作の回帰テストを追加"
```

---

### Task 3: `App.svelte` のバナーをモーダルに置き換える

**Files:**
- Modify: `frontend/src/App.svelte:129-138`(バナー削除)
- Modify: `frontend/src/App.svelte:1-17`(import追加)
- Modify: `frontend/src/App.svelte:220-234`付近(モーダル追加)

**Interfaces:**
- Consumes: `app.errorModal: string | null`(Task 1で追加)、`./ui/Modal.svelte`(`title`, `onclose`, `children` snippet props。`ComposeBar.svelte:757-766` と同じ使い方)、`$lib/components/ui/button` の `Button`(既にApp.svelteでimport済み)。
- Produces: なし(末端のUIコンポーネント)。

- [ ] **Step 1: `Modal` をimportする**

`frontend/src/App.svelte:13` の直後に追加:

```svelte
  import Modal from "./ui/Modal.svelte";
```

- [ ] **Step 2: バナーを削除する**

`frontend/src/App.svelte:129-138` の

```svelte
  {#if app.error}
    <div
      class="flex-none overflow-hidden text-ellipsis whitespace-nowrap bg-destructive/15 px-2.5 py-1 text-[0.78rem] text-destructive"
      title={app.error}
      onclick={() => (app.error = null)}
      role="presentation"
    >
      {app.error}
    </div>
  {/if}
```

を削除する(ブロックごと削除。前後の `</header>` と `<main ...>` はそのまま残す)。

- [ ] **Step 3: エラーモーダルを追加する**

`frontend/src/App.svelte:228-234` 付近(`{#if currentProfileTarget()} ... {/if}` の直後、`</div>`(ファイル末尾のルートdiv閉じ)の直前)に追加:

```svelte
  {#if app.errorModal}
    <Modal title="エラー" onclose={() => (app.errorModal = null)}>
      {#snippet children()}
        <p class="mb-3.5 mt-0 whitespace-pre-wrap break-words text-[0.9rem] text-foreground">{app.errorModal}</p>
        <div class="flex justify-end">
          <Button onclick={() => (app.errorModal = null)}>わかった</Button>
        </div>
      {/snippet}
    </Modal>
  {/if}
```

- [ ] **Step 4: 型・構文チェック**

```bash
cd frontend && pnpm check
```

Expected: エラーなし。`app.error` を参照している箇所が残っていれば型エラーになるので、その場合は該当箇所を確認して修正する。

- [ ] **Step 5: 手動確認**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi && cargo tauri dev
```

起動後、任意のノートに対して「Renote失敗」を再現できる状況が用意しにくい場合は、DevToolsのコンソールから以下を実行してモーダルが表示されることを確認する(Tauri v2 webviewでも `window` 経由でアプリのstoreに触れないため、代わりにネットワークを切った状態でRenote/リアクション操作を行い、モーダルが表示されることを確認する。または `pnpm check` 通過後、Task 1/2 のユニットテストで担保された挙動を信頼し、UIとしては「投稿欄の下に赤いバナーが出ないこと」「Backstageバーは従来通り動作すること」を目視確認する)。

- [ ] **Step 6: コミット**

```bash
git add frontend/src/App.svelte
git commit -m "feat: 投稿欄下のエラーバナーを廃止しエラーモーダルに置き換え"
```

---

## Self-Review Notes

- **Spec coverage:** design specの1〜5の各項目(#failModal新設、7箇所のモーダル振り分け、21箇所のlogFailure振り分け、#fail削除、App.svelteのUI置き換え、テスト更新)は Task 1〜3 でそれぞれ対応済み。
- **Type consistency:** `errorModal: string | null` の名称・型はTask 1で定義し、Task 2・Task 3で同名・同型のまま参照している。`#failModal(e: unknown)` もTask 1定義のシグネチャのまま。
- **Placeholder scan:** 「後で実装」「TODO」等は含まれない。Task 2 Step 2 の `makeTab` は既存ヘルパの有無が実行前に不明なため、Step 1で確認した上で実際の型に合わせて調整する指示を明記した(コード自体は具体的に記載済み)。
