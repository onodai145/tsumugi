# 強制猫化 / 強制人間化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定で全ユーザーを猫化/人間化でき、ノート本文・CWのnyaizeとアバターの猫耳の両方に反映される。

**Architecture:** `UiPrefs` に `catMode`("respect"|"cat"|"human")を追加し、フロントの `effectiveIsCat()` で `isCat` を上書き判定する。`Avatar.svelte` 内部と `NoteCard.svelte` のnyaize2か所でこの関数を通す。

**Tech Stack:** Rust (serde/specta, tauri-specta), Svelte 5, Vitest, Testing Library

## Global Constraints

- `catMode` の値は `"respect"`(既定) / `"cat"` / `"human"` の3つ。
- `catMode` を持たない既存設定JSONは `"respect"` として読める(`#[serde(default)]`)。
- `frontend/src/bindings/tauri.gen.ts` は手編集しない。`cd src-tauri && cargo test` で再生成する。
- UI文言は日本語。コミットメッセージは件名のみ(本文なし)。
- 既存の `Avatar` 呼び出し元(NoteCard以外)は変更しない。

---

### Task 1: Rust `UiPrefs.cat_mode`

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`(フィールド追加 ~190行、default関数 ~260行、`Default` ~303行、テスト用フルインスタンス ~442行、テスト追加 ~493行)
- Regenerated: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Produces: `UiPrefs.cat_mode: String`(TS側 `UiPrefs.catMode: string`)

- [ ] **Step 1: 失敗するテストを書く**

`instance_ticker_defaults_to_remote_for_legacy_json` の直後に追加:

```rust
    #[test]
    fn cat_mode_defaults_to_respect_for_legacy_json() {
        // cat_mode 追加前に保存された JSON も読めること（#[serde(default)]）。
        let v: UiPrefs = serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.cat_mode, "respect");
    }
```

- [ ] **Step 2: 失敗を確認**

Run: `cd src-tauri && cargo test cat_mode_defaults`
Expected: コンパイルエラー(`no field cat_mode`)

- [ ] **Step 3: 実装**

`instance_ticker` フィールド(`pub instance_ticker: String,`)の直後に:

```rust
    /// 猫化モード（Issue #42）。"respect" = ユーザーの isCat に従う(既定) /
    /// "cat" = 全ユーザーを猫扱い / "human" = 全ユーザーを人間扱い。
    /// nyaize と アバターの猫耳の両方に影響する。
    #[serde(default = "default_cat_mode")]
    pub cat_mode: String,
```

`default_instance_ticker` の直後に:

```rust
fn default_cat_mode() -> String {
    "respect".into()
}
```

`Default` 実装の `instance_ticker: default_instance_ticker(),` の直後に `cat_mode: default_cat_mode(),` を追加。
テスト用フルインスタンス(`instance_ticker: "always".into(),` の箇所)の直後に `cat_mode: "cat".into(),` を追加。

- [ ] **Step 4: テストとバインディング再生成**

Run: `cd src-tauri && cargo test`
Expected: 全PASS。`git diff frontend/src/bindings/tauri.gen.ts` に `catMode: string` が現れる。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsにcatModeを追加 (#42)"
```

---

### Task 2: `effectiveIsCat` と Avatar / NoteCard への適用

**Files:**
- Create: `frontend/src/lib/catMode.ts`
- Create: `frontend/src/lib/catMode.test.ts`
- Modify: `frontend/src/lib/store.svelte.ts:260`(`instanceTicker` の行の直後)
- Modify: `frontend/src/ui/Avatar.svelte`
- Modify: `frontend/src/ui/Avatar.test.ts`
- Modify: `frontend/src/ui/NoteCard.svelte:405,421`
- Modify: `frontend/src/ui/NoteCard.test.ts`(「猫耳アバター」describeの後に追加)

**Interfaces:**
- Consumes: `app.ui.catMode: string`(Task 1 で生成)
- Produces: `effectiveIsCat(isCat: boolean): boolean`(`frontend/src/lib/catMode.ts`)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/catMode.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";

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

const { app } = await import("./store.svelte");
const { effectiveIsCat } = await import("./catMode");

afterEach(() => {
  app.ui = { ...app.ui, catMode: "respect" };
});

describe("effectiveIsCat", () => {
  it("respect: isCat をそのまま返す", () => {
    app.ui = { ...app.ui, catMode: "respect" };
    expect(effectiveIsCat(true)).toBe(true);
    expect(effectiveIsCat(false)).toBe(false);
  });

  it("cat: 常に true", () => {
    app.ui = { ...app.ui, catMode: "cat" };
    expect(effectiveIsCat(false)).toBe(true);
  });

  it("human: 常に false", () => {
    app.ui = { ...app.ui, catMode: "human" };
    expect(effectiveIsCat(true)).toBe(false);
  });

  it("未設定(undefined)は respect 扱い", () => {
    app.ui = { ...app.ui, catMode: undefined as unknown as string };
    expect(effectiveIsCat(true)).toBe(true);
    expect(effectiveIsCat(false)).toBe(false);
  });
});
```

`frontend/src/ui/Avatar.test.ts` — 先頭のモック・import を NoteCard.test.ts 同様に整えるため、既存ファイルの import 群の下に以下を追加し、`describe("Avatar", ...)` の後ろに新しい describe を足す:

```ts
import { vi } from "vitest";
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
```

(`vi` は既存の `import { afterEach, describe, expect, it } from "vitest"` に `vi` を足す形にし、重複importにしないこと。`AvatarTestHost` と `app` は `const { app } = await import("../lib/store.svelte");` で読み込む。`vi.mock` はホイストされるため位置は問わない。)

```ts
describe("Avatar catMode", () => {
  afterEach(() => {
    app.ui = { ...app.ui, catMode: "respect" };
  });

  it("cat: isCat=falseでも耳を描画する", () => {
    app.ui = { ...app.ui, catMode: "cat" };
    const { container } = render(AvatarTestHost, { props: { isCat: false } });
    expect(container.querySelector(".ears")).not.toBeNull();
  });

  it("human: isCat=trueでも耳を描画しない", () => {
    app.ui = { ...app.ui, catMode: "human" };
    const { container } = render(AvatarTestHost, { props: { isCat: true } });
    expect(container.querySelector(".ears")).toBeNull();
  });
});
```

`frontend/src/ui/NoteCard.test.ts` の「猫耳アバター」describe の後に追加(`afterEach` で `catMode` も戻すため、既存 `afterEach` の `app.ui = { ...app.ui, instanceTicker: "remote" };` を `app.ui = { ...app.ui, instanceTicker: "remote", catMode: "respect" };` に変更する):

```ts
describe("catMode によるnyaize切替", () => {
  it("cat: isCatでない投稿者の本文もにゃん語化される", () => {
    app.ui = { ...app.ui, catMode: "cat" };
    const note = makeNote({ text: "こんな", user: makeUser({ isCat: false }) });
    const { container } = render(NoteCard, { props: { note, accountId: "a1" } });
    expect(container.textContent).toContain("こんにゃ");
  });

  it("human: isCatの投稿者の本文もにゃん語化されない", () => {
    app.ui = { ...app.ui, catMode: "human" };
    const note = makeNote({ text: "こんな", user: makeUser({ isCat: true }) });
    const { container } = render(NoteCard, { props: { note, accountId: "a1" } });
    expect(container.textContent).toContain("こんな");
    expect(container.textContent).not.toContain("こんにゃ");
  });
});
```

- [ ] **Step 2: 失敗を確認**

Run: `cd frontend && pnpm test -- catMode Avatar NoteCard`
Expected: `catMode.ts` 不在で FAIL

- [ ] **Step 3: 実装**

`frontend/src/lib/catMode.ts`:

```ts
import { app } from "./store.svelte";

/**
 * 設定(catMode)を加味して「猫として扱うか」を返す（Issue #42）。
 * "cat" = 常に猫 / "human" = 常に人間 / それ以外("respect"・未設定) = ユーザーの isCat に従う。
 */
export function effectiveIsCat(isCat: boolean): boolean {
  switch (app.ui.catMode) {
    case "cat":
      return true;
    case "human":
      return false;
    default:
      return isCat;
  }
}
```

`store.svelte.ts` の `instanceTicker: ui.instanceTicker ?? "remote",` の直後に `catMode: ui.catMode ?? "respect",` を追加。

`Avatar.svelte`: import に `import { effectiveIsCat } from "../lib/catMode";` を追加し、`earColor` の行の後に

```ts
  const showEars = $derived(effectiveIsCat(isCat));
```

を追加、テンプレートの `{#if isCat}` を `{#if showEars}` に変更。

`NoteCard.svelte`: import に `import { effectiveIsCat } from "../lib/catMode";` を追加し、405行・421行の `nyaize={inner.user.isCat}` を `nyaize={effectiveIsCat(inner.user.isCat)}` に変更。

- [ ] **Step 4: テストと型チェック**

Run: `cd frontend && pnpm test && pnpm check`
Expected: 全PASS、型エラーなし

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat: catModeに応じてnyaizeと猫耳を強制切替する (#42)"
```

---

### Task 3: 設定UIとユーザーガイド

**Files:**
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`(state ~15行、options ~27行、`save()` ~221行、Instance Tickerブロックの直後 ~266行)
- Modify: `docs/guide/user-guide.md:129`

**Interfaces:**
- Consumes: `app.ui.catMode`

- [ ] **Step 1: UI実装**

state(`let instanceTicker = ...` の直後):

```ts
  let catMode = $state(app.ui.catMode ?? "respect");
```

options(`instanceTickerOptions` の直後):

```ts
  const catModeOptions: { id: string; label: string }[] = [
    { id: "respect", label: "ユーザー設定に従う" },
    { id: "cat", label: "全員を猫化" },
    { id: "human", label: "全員を人間化" },
  ];
```

`save()` 内の `instanceTicker,` の直後に `catMode,` を追加。

Instance Ticker ブロック(`</div>` で閉じる `mb-3 flex flex-col` の div)の直後に追加:

```svelte
<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">猫化</span>
  <div class="inline-flex w-fit overflow-hidden rounded-md border border-border">
    {#each catModeOptions as t (t.id)}
      <button
        type="button"
        class={catMode === t.id
          ? "border-r border-border bg-primary px-3.5 py-1.5 text-sm text-primary-foreground last:border-r-0"
          : "border-r border-border bg-muted px-3.5 py-1.5 text-sm text-foreground last:border-r-0"}
        onclick={() => (catMode = t.id)}
      >{t.label}</button>
    {/each}
  </div>
  <p class="mb-4 mt-0 text-xs text-muted-foreground">
    ノート本文・CWのにゃん語化とアバターの猫耳に反映されます。
    「全員を猫化」「全員を人間化」は各ユーザーの猫設定より優先されます。
  </p>
</div>
```

- [ ] **Step 2: ユーザーガイド追記**

`docs/guide/user-guide.md:129` の「Instance Ticker（...）、」の直後に
`猫化（ユーザー設定に従う/全員を猫化/全員を人間化。ノート本文のにゃん語化とアバターの猫耳に反映）、` を挿入。

- [ ] **Step 3: 検証**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 全PASS

実UI確認(必須): メモリの方針どおり Xvfb + `dbus-run-session` 越しで `cargo tauri dev`(リポジトリルートから、`WAYLAND_DISPLAY` は unset、実画面に出さない)を起動し、設定→外観で3モードを切り替えて猫耳・本文変換が変わることを確認する。確認後、自分で起動したプロセスはPID指定でkillする(pkill/killall禁止)。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/settings/AppearanceSection.svelte docs/guide/user-guide.md
git commit -m "feat: 外観設定に猫化モードの選択を追加 (#42)"
```
