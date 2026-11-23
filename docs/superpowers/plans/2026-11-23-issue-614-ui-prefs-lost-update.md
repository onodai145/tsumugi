# Issue #614 表示設定の保存直列化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 表示設定(`UiPrefs`)の保存を直列化し、実行時点の最新値に変更分だけをマージして、並行する別の書き込みが消えないようにする。

**Architecture:** フロントの `AppStore` に `updateUiPrefs(patch | (cur) => patch)` を足し、Promise チェーンで 1 件ずつ実行する。各タスクは、開始時点の `this.ui` に変更分をマージして `commands.setUiPrefs`(全量置換のまま)へ送る。全量置換の `setUiPrefs` は最後に公開をやめる。Rust は変えない。

**Tech Stack:** Svelte 5 (runes) / TypeScript / Vitest (jsdom)

**Spec:** `docs/superpowers/specs/2026-11-23-issue-614-ui-prefs-lost-update-design.md`

## Global Constraints

- Rust・生成バインディング(`bindings/tauri.gen.ts`)は変更しない(spec「方針」)。
- `updateUiPrefs` が失敗しても後続の保存は止めない。失敗は呼び出し元にだけ reject で返し、`this.ui` を変えない(spec「方針」)。
- `setKeymap` / `setPinnedEmojis` / `recordEmojiUsage` は、現状どおり `apply*` と「表示設定を保存しました」ログを出さない(挙動を変えない)。
- コミットメッセージは件名 1 行のみ、`type(scope): 件名` 形式(Co-Authored-By は別途付く)。
- 検証コマンド: `cd frontend && pnpm check && npx vitest run <path>`(`pnpm test -- <path>` はパスを無視して全体を走らせる)。

## Review Focus

- 保存が失敗(Rust の検証エラー)した直後の次の保存が、止まらず実行される → Task 1 のテスト 3。
- 設定の保存中にリアクションを付ける(逆順)と、保存した設定項目が消えない → Task 1 のテスト 2b。
- 同じ瞬間に 2 回リアクションを付けても、履歴が片方だけにならない → Task 1 のテスト 2。
- 削除済みテーマへのフォールバック待ちの間に利用者が別のテーマを選んだら、選んだ方が勝つ → Task 1 のテスト 6。
- `null` へのクリア(`backgroundPath: null` など)と `undefined`(`uiScale: undefined`)が、マージで落ちずにそのまま送られる → Task 1 のテスト 7。

## File Structure

- Modify: `frontend/src/lib/store.svelte.ts` — `updateUiPrefs` と private の直列化処理を追加。`setKeymap` / `setPinnedEmojis` / `recordEmojiUsage` / テーマのフォールバックを移行。最後に `setUiPrefs` を削除。
- Modify: `frontend/src/lib/store.svelte.test.ts` — 競合のテストを追加。既存の `setUiPrefs` 呼び出しを `updateUiPrefs` へ。
- Modify: `frontend/src/ui/settings/{About,Accounts,Appearance,Background,Data,Developer,ExternalIntegration,Layout,Mobile}Section.svelte` — `app.updateUiPrefs({ 編集した項目だけ })` へ。
- Modify: `frontend/src/ui/settings/{Layout,Appearance,ExternalIntegration}Section.test.ts` — spy の対象を `updateUiPrefs` へ。

---

### Task 1: `updateUiPrefs` の追加と store 内の移行

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts:1847-1888`(`setUiPrefs`)、`:1922-1943`(`setKeymap` / `setPinnedEmojis` / `recordEmojiUsage`)、`:1982` と `:2002`(フォールバック)
- Test: `frontend/src/lib/store.svelte.test.ts`(末尾に新しい `describe` を追加)

**Interfaces:**
- Produces: `export type UiPrefsPatch = Partial<UiPrefs> | ((cur: UiPrefs) => Partial<UiPrefs>)`、`AppStore.updateUiPrefs(patch: UiPrefsPatch): Promise<void>`。この Task の間は、公開の `setUiPrefs(prefs: UiPrefs): Promise<void>` も残す(中身は直列化に乗せる)。

- [ ] **Step 1: 失敗するテストを書く**

`store.svelte.test.ts` の末尾に追加する(`mockPrefersColorSchemeDark` は同ファイル内の既存ヘルパー)。

```ts
describe("updateUiPrefs: 保存の直列化(Issue #614)", () => {
  const prefsCalls = () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "set_ui_prefs")
      .map((c) => (c[1] as { prefs: UiPrefs }).prefs);

  beforeEach(async () => {
    mockPrefersColorSchemeDark(false);
    await app.updateUiPrefs({
      defaultColumnWidth: 300,
      defaultAccountId: "",
      recentEmojis: [],
      theme: "auto",
      backgroundPath: null,
    });
    invokeMock.mockClear();
    invokeMock.mockResolvedValue({ status: "ok", data: null });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("1. IPC の往復中に始めた別の保存が、先の変更を消さない", async () => {
    let release!: () => void;
    invokeMock.mockImplementationOnce(
      () => new Promise((resolve) => (release = () => resolve({ status: "ok", data: null }))),
    );
    const first = app.updateUiPrefs({ defaultColumnWidth: 311 });
    const second = app.updateUiPrefs({ defaultAccountId: "acc-x" });
    await vi.waitFor(() => expect(prefsCalls()).toHaveLength(1));
    // 先発の往復が終わるまで、後発は IPC を送らない(直列化)。
    expect(prefsCalls()).toHaveLength(1);
    release();
    await Promise.all([first, second]);
    const calls = prefsCalls();
    expect(calls).toHaveLength(2);
    expect(calls[1].defaultColumnWidth).toBe(311);
    expect(calls[1].defaultAccountId).toBe("acc-x");
    expect(app.ui.defaultColumnWidth).toBe(311);
    expect(app.ui.defaultAccountId).toBe("acc-x");
  });

  it("2. 続けて 2 回リアクションを付けても、履歴は両方残る", async () => {
    await Promise.all([app.recordEmojiUsage("🍣"), app.recordEmojiUsage("🍙")]);
    const last = prefsCalls().at(-1)!;
    expect(last.recentEmojis).toEqual(expect.arrayContaining(["🍣", "🍙"]));
    expect(app.ui.recentEmojis).toEqual(expect.arrayContaining(["🍣", "🍙"]));
  });

  it("2b. 設定の保存中にリアクションを付けても、保存した設定項目が消えない", async () => {
    let release!: () => void;
    invokeMock.mockImplementationOnce(
      () => new Promise((resolve) => (release = () => resolve({ status: "ok", data: null }))),
    );
    const settings = app.updateUiPrefs({ defaultColumnWidth: 322 });
    const reaction = app.recordEmojiUsage("🍣");
    await vi.waitFor(() => expect(prefsCalls()).toHaveLength(1));
    release();
    await Promise.all([settings, reaction]);
    const last = prefsCalls().at(-1)!;
    expect(last.defaultColumnWidth).toBe(322);
    expect(last.recentEmojis).toContain("🍣");
  });

  it("3. 1 件が失敗しても後続は実行され、失敗は呼んだ側だけに返り、this.ui は変わらない", async () => {
    invokeMock.mockRejectedValueOnce(new Error("boom"));
    const first = app.updateUiPrefs({ defaultColumnWidth: 333 });
    const second = app.updateUiPrefs({ defaultAccountId: "acc-y" });
    await expect(first).rejects.toThrow();
    await expect(second).resolves.toBeUndefined();
    expect(prefsCalls()[1].defaultColumnWidth).toBe(300);
    expect(app.ui.defaultColumnWidth).toBe(300);
    expect(app.ui.defaultAccountId).toBe("acc-y");
  });

  it("4. 関数形のパッチは、直前のタスクの結果を見る", async () => {
    const a = app.updateUiPrefs({ defaultColumnWidth: 340 });
    const b = app.updateUiPrefs((cur) => ({ defaultColumnWidth: cur.defaultColumnWidth + 1 }));
    await Promise.all([a, b]);
    expect(app.ui.defaultColumnWidth).toBe(341);
  });

  it("5. 関数形のパッチが空を返したら、IPC を送らない", async () => {
    await app.updateUiPrefs(() => ({}));
    expect(prefsCalls()).toHaveLength(0);
  });

  it("6. フォールバック待ちの間に別のテーマが選ばれたら、選ばれた方が勝つ", async () => {
    await app.updateUiPrefs({ theme: "custom:gone" });
    // 上の保存の適用で「削除済みカスタムテーマ→auto」のフォールバックが積まれる。
    await vi.waitFor(() => expect(app.ui.theme).toBe("auto"));

    const gone = app.updateUiPrefs({ theme: "custom:gone" });
    const chosen = app.updateUiPrefs({ theme: "dark" });
    await Promise.all([gone, chosen]);
    await vi.waitFor(() => expect(prefsCalls().length).toBeGreaterThanOrEqual(2));
    // フォールバックは「まだ custom:gone のときだけ」auto にする。dark が選ばれた後なので何もしない。
    await new Promise((r) => setTimeout(r, 0));
    expect(app.ui.theme).toBe("dark");
  });

  it("7. null / undefined への更新は、マージで落ちずにそのまま送られる", async () => {
    await app.updateUiPrefs({ backgroundPath: "/tmp/a.png" });
    invokeMock.mockClear();
    invokeMock.mockResolvedValue({ status: "ok", data: null });
    await app.updateUiPrefs({ backgroundPath: null, uiScale: undefined });
    const sent = prefsCalls()[0];
    expect(sent.backgroundPath).toBeNull();
    expect(app.ui.backgroundPath).toBeNull();
  });
});
```

ファイル先頭の型 import に `UiPrefs` を足す: `import type { Account, ColumnKind, FilterQuery, Note, Notification, UiPrefs, User } from "../bindings/tauri.gen";`

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts -t "updateUiPrefs"`
Expected: FAIL(`app.updateUiPrefs is not a function`)。

- [ ] **Step 3: 実装する**

`store.svelte.ts` の import 付近(型定義が置ける場所)に追加:

```ts
/// `updateUiPrefs` の引数。オブジェクトは変更したいキーだけ。関数は、実行時点の最新の値を受け取って
/// 変更したいキーを返す(最新の値から派生する更新や、条件付きの更新に使う。空のオブジェクトを返すと何もしない)。
export type UiPrefsPatch = Partial<UiPrefs> | ((cur: UiPrefs) => Partial<UiPrefs>);
```

既存の `async setUiPrefs(prefs: UiPrefs) { ... }`(`this.#log("info", "表示設定を保存しました");` まで)を、次のコードで置き換える。`this.ui = {...}` の正規化ブロックは、既存の中身をそのまま `prefs` 基準で移す。

```ts
  #uiPrefsQueue: Promise<void> = Promise.resolve();

  /// 表示設定を保存し、即時反映する(Issue #614)。保存は呼び出し順に 1 件ずつ実行し、
  /// 実行時点の最新の `this.ui` に `patch` をマージして送る。`commands.setUiPrefs` は全量置換なので、
  /// 呼び出し時点のスナップショット(`{...app.ui, 変更分}`)を送ると、往復中の別の保存を消してしまう。
  updateUiPrefs(patch: UiPrefsPatch): Promise<void> {
    return this.#enqueueUiPrefs(patch, true);
  }

  /// 旧API(全量置換)。呼び出し側の移行が済むまでの間だけ残す。
  async setUiPrefs(prefs: UiPrefs) {
    await this.#enqueueUiPrefs(prefs, true);
  }

  /// `applyEffects` が偽なら、保存と `this.ui` の更新だけを行う(キー割り当て・絵文字の更新が、
  /// 背景やテーマの再適用とログを出さない現状の挙動を保つため)。
  #enqueueUiPrefs(patch: UiPrefsPatch, applyEffects: boolean): Promise<void> {
    const run = async () => {
      const delta = typeof patch === "function" ? patch(this.ui) : patch;
      if (Object.keys(delta).length === 0) return;
      const prefs: UiPrefs = { ...this.ui, ...delta };
      await unwrap(commands.setUiPrefs(prefs));
      this.ui = {
        ...prefs,
        keymap: prefs.keymap ?? {},
        // ↑ 以降は既存の正規化(fontFamily … customCss)を、元のメソッドからそのまま移す。
      };
      if (!applyEffects) return;
      this.#applyTheme(prefs.theme);
      this.#applySyntaxTheme(prefs.codeHighlightTheme ?? "auto", this.ui.customSyntaxThemes ?? []);
      this.#applyFont(prefs.fontFamily ?? "");
      this.#applyBackground(this.ui);
      this.#applyAvatarRadius(this.ui.avatarRadius ?? 20);
      this.#applyUiScale(this.ui.uiScale);
      this.#applyMediaThumbnailHeight(this.ui.mediaThumbnailHeight ?? 200);
      this.#applyCustomCss();
      this.#log("info", "表示設定を保存しました");
    };
    const result = this.#uiPrefsQueue.then(run);
    // 失敗は呼び出し元にだけ返し、後続の保存は止めない。
    this.#uiPrefsQueue = result.catch(() => {});
    return result;
  }
```

store 内の書き込みを移行する。

```ts
  async setKeymap(overrides: Record<string, string>) {
    await this.#enqueueUiPrefs({ keymap: overrides }, false);
    this.#log("info", "キー割り当てを更新しました");
  }

  async setPinnedEmojis(list: string[]) {
    await this.#enqueueUiPrefs({ pinnedEmojis: list }, false);
  }

  async recordEmojiUsage(key: string) {
    await this.#enqueueUiPrefs(
      (cur) => ({ recentEmojis: withRecentEmojiUsage(cur.recentEmojis ?? [], key) }),
      false,
    );
  }
```

フォールバック 2 か所:

```ts
        // #applyTheme 内(旧: void this.setUiPrefs({ ...this.ui, theme: "auto" }))
        void this.updateUiPrefs((cur) => (cur.theme === theme ? { theme: "auto" } : {}));
```

```ts
      // #applySyntaxTheme 内(旧: void this.setUiPrefs({ ...this.ui, codeHighlightTheme: "auto" }))
      void this.updateUiPrefs((cur) =>
        cur.codeHighlightTheme === codeHighlightTheme ? { codeHighlightTheme: "auto" } : {},
      );
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/lib/store.svelte.test.ts`
Expected: PASS(既存の `setUiPrefs` を使うテストも、旧APIが直列化に乗っているので通る)。

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "fix(store): 表示設定の保存を直列化する updateUiPrefs を追加する(Issue #614)"
```

---

### Task 2: 設定画面の保存を `updateUiPrefs` へ移す

**Files:**
- Modify: `frontend/src/ui/settings/AboutSection.svelte:31`、`AccountsSection.svelte:36`、`AppearanceSection.svelte:129,138,207,214,233`、`BackgroundSection.svelte:133`、`DataSection.svelte:29`、`DeveloperSection.svelte:17`、`ExternalIntegrationSection.svelte:78`、`LayoutSection.svelte:44`、`MobileSection.svelte:17`
- Test: `frontend/src/ui/settings/{Layout,Appearance,ExternalIntegration}Section.test.ts`

**Interfaces:**
- Consumes: `AppStore.updateUiPrefs(patch: UiPrefsPatch): Promise<void>`(Task 1)。

- [ ] **Step 1: テストの spy を先に `updateUiPrefs` へ変える(失敗させる)**

```bash
cd frontend/src/ui/settings
perl -pi -e 's/vi\.spyOn\(app, "setUiPrefs"\)/vi.spyOn(app, "updateUiPrefs")/g' LayoutSection.test.ts AppearanceSection.test.ts ExternalIntegrationSection.test.ts
perl -pi -e 's/setUiPrefs内の/updateUiPrefs内の/' AboutSection.test.ts
```

Run: `cd frontend && npx vitest run src/ui/settings`
Expected: FAIL(各節がまだ `setUiPrefs` を呼ぶので、spy が呼ばれない)。

- [ ] **Step 2: 各節を書き換える**

機械的に直せる部分(単行と、`...app.ui,` だけの行を持つ複数行):

```bash
cd frontend/src/ui/settings
perl -0pi -e 's/app\.setUiPrefs\(\{\n\s*\.\.\.app\.ui,\n/app.updateUiPrefs({\n/g; s/app\.setUiPrefs\(\{ \.\.\.app\.ui, /app.updateUiPrefs({ /g' \
  AboutSection.svelte AccountsSection.svelte AppearanceSection.svelte BackgroundSection.svelte \
  DataSection.svelte DeveloperSection.svelte ExternalIntegrationSection.svelte LayoutSection.svelte MobileSection.svelte
```

各節にある「このセクションが編集しないフィールド…現在の app.ui をベースに」のコメントを、次の文に置き換える(`updateUiPrefs` が実行時点の最新値にマージするため):

```
      // 編集した項目だけを渡す。保存は直列化され、最新の app.ui にマージされる(Issue #614)。
```

手で直す 2 か所(`AppearanceSection.svelte`)。`app.ui.theme` / `app.ui.codeHighlightTheme` を読むと、その時点のスナップショットに戻ってしまうので、選択中のテーマを消すときだけキーを含める。

```ts
    await app.updateUiPrefs({ customThemes: next, ...(clearing ? { theme: "auto" } : {}) });
```

```ts
    await app.updateUiPrefs({
      customSyntaxThemes: next,
      ...(clearing ? { codeHighlightTheme: "auto" } : {}),
    });
```

確認: `grep -rn "app\.ui\b" frontend/src/ui/settings/*.svelte` で、`updateUiPrefs` の引数の中に `app.ui.` の読み取りが残っていないこと(コンポーネントの表示用の読み取りは対象外)。`AppearanceSection.svelte:233` の呼び出しも、引数にスナップショット由来の値が混ざっていないか読んで確かめる。

- [ ] **Step 3: テストと型検査**

Run: `cd frontend && npx vitest run src/ui/settings src/lib/store.svelte.test.ts`
Expected: PASS。「他の項目は保持される」系のテストは、各節が自分の編集項目をパッチに含めるので、そのまま通る。通らないものは、パッチに含まれない項目(=送らないことが正しい項目)を検証していないか確認し、「含まれないこと」の検証(`expect(saved).not.toHaveProperty(...)`)へ直す。

Run: `cd frontend && pnpm check`
Expected: エラーなし。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/settings
git commit -m "fix(settings): 設定画面の保存を updateUiPrefs に移し、編集した項目だけ送る(Issue #614)"
```

---

### Task 3: 全量置換の `setUiPrefs` を公開から外す

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`(`setUiPrefs` の削除、コメント 1 か所)
- Test: `frontend/src/lib/store.svelte.test.ts`(既存の呼び出しの移行)

**Interfaces:**
- Consumes: `AppStore.updateUiPrefs`(Task 1)。
- Produces: `AppStore` に公開の全量置換 API が無い状態。

- [ ] **Step 1: 既存テストの呼び出しを移行する**

```bash
cd frontend/src/lib
perl -pi -e 's/app\.setUiPrefs\(\{ \.\.\.app\.ui, /app.updateUiPrefs({ /g; s/setUiPrefsで/updateUiPrefsで/g' store.svelte.test.ts
grep -n "setUiPrefs" store.svelte.test.ts
```
Expected: `grep` の結果は空(Task 1 で足したテストの名前・コメントに残っていれば、そこも直す)。

- [ ] **Step 2: `setUiPrefs` を消す**

`store.svelte.ts` から、`/// 旧API(全量置換)。…` のコメントと `async setUiPrefs(prefs: UiPrefs) { ... }` を削除する。`pickBackgroundMedia` のコメント(`保存は setUiPrefs で`)は `保存は updateUiPrefs で` に直す。

- [ ] **Step 3: 全体を検証する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: すべて PASS。

Run: `grep -rn "\.setUiPrefs(" frontend/src --include=*.ts --include=*.svelte | grep -v "bindings/"`
Expected: `commands.setUiPrefs(` が `store.svelte.ts` の `#enqueueUiPrefs` 内の 1 か所だけ。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "refactor(store): 全量置換の setUiPrefs を公開から外す(Issue #614)"
```

---

## Self-Review

- **Spec coverage:** 直列化とマージ(Task 1)、失敗しても後続が止まらない(テスト 3)、`setUiPrefs` の降格(Task 3)、呼び出し側の書き換え表(各節=Task 2、`setKeymap` 等とフォールバック=Task 1)、テスト 1〜5(spec のテスト欄)は Task 1 のテスト 1・2・3・4 と Task 2 の既存テスト更新で対応。空パッチで IPC を送らない(テスト 5)と、フォールバックの条件(テスト 6)は spec の「関数形」「条件」を具体化したもの。
- **Spec との差分(意図的):** `setKeymap` / `setPinnedEmojis` / `recordEmojiUsage` は、spec の表では `updateUiPrefs` だが、`apply*` とログの再実行を避けるため private の `#enqueueUiPrefs(…, false)` を使う(現状の挙動の維持)。直列化とマージは同じ経路。
- **Placeholder scan:** なし。`this.ui = {...}` の正規化ブロックは「既存の中身をそのまま移す」と明記(行数が多く、元のコードが唯一の正)。
- **Type consistency:** `UiPrefsPatch` / `updateUiPrefs` / `#enqueueUiPrefs(patch, applyEffects)` を全 Task で同じ名前・引数で使っている。
