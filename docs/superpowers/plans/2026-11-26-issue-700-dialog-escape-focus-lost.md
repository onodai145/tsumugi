# 個別ダイアログの、フォーカス喪失後の Escape Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 4つの個別ダイアログが、フォーカスが `body` に移った後も Escape で閉じるようにする(Issue #700)。

**Architecture:** `Modal.svelte` の window 側の受け口を Svelte アクション `escapeOnLostFocus` に切り出し、`Modal` と4ダイアログの `role="dialog"` 要素に付ける。

**Tech Stack:** Svelte 5(runes)、TypeScript、Vitest + @testing-library/svelte(jsdom)、WebdriverIO(実画面確認のみ)

**Spec:** `docs/superpowers/specs/2026-11-26-issue-700-dialog-escape-focus-lost-design.md`

## Global Constraints

- 作業ブランチは `fix/issue-700-dialog-escape-focus-lost`(作成済み)。
- 閉じる条件は4つすべて: キーが Escape / `defaultPrevented` でない / 対象が `document.body` / `[aria-modal="true"], [data-escape-layer]` の DOM 上で最後の要素が自分。
- 各コンポーネントの外側の `onkeydown` は残す。
- 通報の送信中は、Escape で閉じない(下の `Modal` も閉じない)。
- コミットは件名1行のみ(Conventional Commits)。本文は書かず、末尾に `Co-Authored-By` の行だけを付ける。`--no-verify` は使わない。コミットが失敗したら止めて報告する。
- 実アプリの確認は Xvfb 上の隔離インスタンスだけで行う。プロセスの終了は正確な PID だけを指定する。
- 重いビルドと Docker の前に `free -h` で空きとスワップを確認する。

## Review Focus

- 重なった画面で、`window` リスナーの登録順と DOM 上の順が逆でも、手前だけが閉じる(Task 2)。
- 手前を閉じた後の次の Escape は、下の画面に届く(Task 2)。
- 消えた画面のリスナーが残って反応しない(Task 2)。
- 通報の送信に失敗した後、`body` での Escape で閉じる(Task 3)。
- フォーカスがダイアログ内にあるときの Escape で、キャンセルが2回呼ばれない(`ConfirmDialog.test.ts` の既存テストが `toHaveBeenCalledTimes(1)` で固定している)。

---

### Task 1: 修正前の実画面で再現する

コードは変えない。いまのブランチのフロントエンドは `main` と同じなので、これが修正前の記録になる。

**Files:**
- Create(コミットしない): `e2e/tmp-issue-700/` の使い捨て spec と wdio config

- [ ] **Step 1: 環境を用意する**

```sh
free -h; ss -ltn | grep 8443   # 8443 が空いていること
cd frontend && pnpm build
cd ../src-tauri && systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G env CARGO_BUILD_JOBS=4 cargo build
cd ../e2e && ./scripts/gen-ca.sh && docker compose up -d --wait && pnpm seed
```

- [ ] **Step 2: 使い捨て spec を書く**

`e2e/specs/account-post-reaction.e2e.ts` と `e2e/helpers/` の既存のログイン手順を使う。

1. 管理者でない2人目のユーザーを作り、アプリでそのアカウントにログインする。
2. 管理者のプロフィールを開き、「…」→ 通報を選ぶ。
3. 本文を入力して「通報する」を押す。管理者は通報できないので、`CANNOT_REPORT_THE_ADMIN` で失敗する。
4. `document.activeElement` の `tagName` と `disabled` を記録する。
5. `browser.keys("Escape")` を送り、通報ダイアログとプロフィールが残っているかを記録する。
6. 各段階のスクリーンショットを scratchpad に保存し、画像を Read で確認する。

`browser.execute` に渡す関数の中に、名前付きの内部関数を書かない(`__name` が注入されて失敗する)。

- [ ] **Step 3: 実行する**

```sh
cd e2e && NODE_EXTRA_CA_CERTS=./certs/ca.pem xvfb-run -a pnpm exec wdio run tmp-issue-700/wdio.conf.ts
```

期待: エラー表示の後、`activeElement` が `BODY` で、Escape を送ってもダイアログが残る。

**再現しなかった場合は、ここで止めて、記録した `activeElement` を添えて報告する。** spec の前提が変わるため、先へ進まない。

---

### Task 2: アクション `escapeOnLostFocus` と `Modal` の置き換え

**Files:**
- Create: `frontend/src/lib/escapeOnLostFocus.ts`
- Create: `frontend/src/lib/escapeOnLostFocus.test.ts`
- Modify: `frontend/src/ui/Modal.svelte`(`onWindowKeydown` と `<svelte:window>` を削除し、`role="dialog"` の要素にアクションを付ける)

**Interfaces:**
- Produces: `escapeOnLostFocus(node: HTMLElement, close: () => void): { update(next: () => void): void; destroy(): void }`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/escapeOnLostFocus.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { escapeOnLostFocus } from "./escapeOnLostFocus";

const cleanups: (() => void)[] = [];

function layer(close = vi.fn()) {
  const node = document.createElement("div");
  node.setAttribute("aria-modal", "true");
  document.body.appendChild(node);
  const action = escapeOnLostFocus(node, close);
  const remove = () => {
    action.destroy();
    node.remove();
  };
  cleanups.push(remove);
  return { node, close, action, remove };
}

function press(key = "Escape", target: EventTarget = document.body) {
  const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  target.dispatchEvent(e);
  return e;
}

afterEach(() => {
  for (const remove of cleanups.splice(0)) remove();
  document.body.innerHTML = "";
});

describe("escapeOnLostFocus(Issue #700)", () => {
  it("bodyに届いたEscapeで閉じる処理を呼び、preventDefaultする", () => {
    const { close } = layer();
    const e = press();
    expect(close).toHaveBeenCalledOnce();
    expect(e.defaultPrevented).toBe(true);
  });

  it("Escape以外のキー・body以外が対象のイベントでは呼ばない", () => {
    const { close, node } = layer();
    press("Enter");
    press("Escape", node);
    expect(close).not.toHaveBeenCalled();
  });

  it("処理済み(defaultPrevented)のEscapeでは呼ばない", () => {
    const { close } = layer();
    const consume = (e: Event) => e.preventDefault();
    document.addEventListener("keydown", consume);
    press();
    document.removeEventListener("keydown", consume);
    expect(close).not.toHaveBeenCalled();
  });

  it("手前にdata-escape-layerの要素があれば呼ばない", () => {
    const { close } = layer();
    const viewer = document.createElement("div");
    viewer.setAttribute("data-escape-layer", "");
    document.body.appendChild(viewer);
    press();
    expect(close).not.toHaveBeenCalled();
  });

  it.each([
    ["先に登録した方が手前", true],
    ["後に登録した方が手前", false],
  ])("%sでも、手前だけが閉じ、消えた後は下に届く", (_name, firstOnTop) => {
    const first = layer();
    const second = layer();
    if (firstOnTop) document.body.appendChild(first.node);
    const [top, bottom] = firstOnTop ? [first, second] : [second, first];
    press();
    expect(top.close).toHaveBeenCalledOnce();
    expect(bottom.close).not.toHaveBeenCalled();
    top.remove();
    press();
    expect(top.close).toHaveBeenCalledOnce();
    expect(bottom.close).toHaveBeenCalledOnce();
  });

  it("destroy後は、要素がDOMに残っていても呼ばない", () => {
    const { close, action } = layer();
    action.destroy();
    press();
    expect(close).not.toHaveBeenCalled();
  });

  it("updateで差し替えた処理を呼ぶ", () => {
    const { close, action } = layer();
    const next = vi.fn();
    action.update(next);
    press();
    expect(next).toHaveBeenCalledOnce();
    expect(close).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/lib/escapeOnLostFocus.test.ts`
Expected: FAIL(`./escapeOnLostFocus` を解決できない)

- [ ] **Step 3: 実装する**

`frontend/src/lib/escapeOnLostFocus.ts`:

```ts
// ダイアログの要素(role="dialog" aria-modal="true")に付けるSvelteアクション(Issue #698 / #700)。
// フォーカス中の要素が消える・無効になると、フォーカスは body に移り、ダイアログ側の onkeydown には
// Escape が届かなくなる。その場合だけ window で受け、一番手前の画面が自分なら閉じる。
// data-escape-layer は、aria-modal を持たずに自分で Escape を処理する画面(MediaViewer)の目印。
export function escapeOnLostFocus(node: HTMLElement, close: () => void) {
  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented || e.target !== document.body) return;
    const layers = document.querySelectorAll('[aria-modal="true"], [data-escape-layer]');
    if (layers[layers.length - 1] !== node) return;
    // 閉じた直後に、下の画面が同じ Escape で自分を一番手前と判定しないようにする。
    e.preventDefault();
    close();
  }
  window.addEventListener("keydown", onKeydown);
  return {
    update(next: () => void) {
      close = next;
    },
    destroy() {
      window.removeEventListener("keydown", onKeydown);
    },
  };
}
```

- [ ] **Step 4: `Modal.svelte` を置き換える**

import を追加する。

```ts
  import { escapeOnLostFocus } from "../lib/escapeOnLostFocus";
```

`let modalEl` は残し、その下の3行のコメント、`onWindowKeydown` 関数、`<svelte:window onkeydown={onWindowKeydown} />` を削除する。`role="dialog"` の要素で、`bind:this={modalEl}` の次の行に追加する。

```svelte
    use:escapeOnLostFocus={() => onclose()}
```

- [ ] **Step 5: テストを通す**

Run: `cd frontend && npx vitest run src/lib/escapeOnLostFocus.test.ts src/ui/Modal.test.ts`
Expected: PASS(`Modal.test.ts` の #698 のテストは変更せずに通る)

- [ ] **Step 6: コミット**

```sh
git add frontend/src/lib/escapeOnLostFocus.ts frontend/src/lib/escapeOnLostFocus.test.ts frontend/src/ui/Modal.svelte
git commit -m "refactor(ui): フォーカス喪失後の Escape の受け口をアクションに切り出す(Issue #700)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 4つのダイアログに適用する

**Files:**
- Modify: `frontend/src/ui/ConfirmDialog.svelte`, `frontend/src/ui/ReportUserDialog.svelte`, `frontend/src/ui/settings/LicenseConsentDialog.svelte`, `frontend/src/ui/settings/SpeechCoreTermsDialog.svelte`
- Test: 同名の `.test.ts` 4ファイル

**Interfaces:**
- Consumes: `escapeOnLostFocus(node, close)`(Task 2)

- [ ] **Step 1: 失敗するテストを書く**

`ConfirmDialog.test.ts` の末尾に追加する。

```ts
describe("ConfirmDialog のフォーカスが外れた後の Escape(Issue #700)", () => {
  const body = createRawSnippet(() => ({ render: () => `<span>profile</span>` }));

  it("bodyに移った後のEscapeは、手前のダイアログだけをキャンセルする", async () => {
    const onclose = vi.fn();
    render(Modal, { props: { title: "親", onclose, children: body } });
    await tick();
    const onCancel = vi.fn();
    render(ConfirmDialog, { props: { message: "削除しますか?", onConfirm: vi.fn(), onCancel } });
    await tick();
    (document.activeElement as HTMLElement).blur();
    expect(document.activeElement).toBe(document.body);
    await fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onclose).not.toHaveBeenCalled();
  });
});
```

`ReportUserDialog.test.ts` の末尾に追加する。

```ts
describe("ReportUserDialog のフォーカスが外れた後の Escape(Issue #700)", () => {
  const body = createRawSnippet(() => ({ render: () => `<span>profile</span>` }));

  async function submitOverModal(onsubmit: () => Promise<void>) {
    const onclose = vi.fn();
    render(Modal, { props: { title: "プロフィール", onclose, children: body } });
    await tick();
    const m = mount(onsubmit as never);
    await tick();
    await fireEvent.input(m.getByLabelText("通報の内容"), { target: { value: "スパムです" } });
    await fireEvent.click(m.getByRole("button", { name: "通報する" }));
    return { ...m, onclose };
  }

  async function escapeFromBody() {
    (document.activeElement as HTMLElement | null)?.blur();
    expect(document.activeElement).toBe(document.body);
    await fireEvent.keyDown(document.body, { key: "Escape" });
  }

  it("送信に失敗した後は、bodyでのEscapeでダイアログだけ閉じる", async () => {
    const { findByText, oncancel, onclose } = await submitOverModal(() =>
      Promise.reject(new Error("users/report-abuse: FAILED")),
    );
    await findByText(/FAILED/);
    await escapeFromBody();
    expect(oncancel).toHaveBeenCalledTimes(1);
    expect(onclose).not.toHaveBeenCalled();
  });

  it("送信中は、bodyでのEscapeでダイアログも親のモーダルも閉じない", async () => {
    const { oncancel, onclose } = await submitOverModal(() => new Promise<void>(() => {}));
    await escapeFromBody();
    expect(oncancel).not.toHaveBeenCalled();
    expect(onclose).not.toHaveBeenCalled();
  });
});
```

`settings/LicenseConsentDialog.test.ts` は、import を次のように変える。

```ts
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet, tick } from "svelte";
```

`const { default: LicenseConsentDialog } = ...` の次の行に `const { default: Modal } = await import("../Modal.svelte");` を足し、末尾に追加する。

```ts
describe("LicenseConsentDialog のフォーカスが外れた後の Escape(Issue #700)", () => {
  const body = createRawSnippet(() => ({ render: () => `<span>settings</span>` }));

  it("bodyに移った後のEscapeは、手前のダイアログだけをキャンセルする", async () => {
    const onclose = vi.fn();
    render(Modal, { props: { title: "親", onclose, children: body } });
    await tick();
    const onCancel = vi.fn();
    render(LicenseConsentDialog, { props: { ...base, onCancel } });
    await tick();
    (document.activeElement as HTMLElement).blur();
    expect(document.activeElement).toBe(document.body);
    await fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onclose).not.toHaveBeenCalled();
  });
});
```

`settings/SpeechCoreTermsDialog.test.ts` も同じく、`import { createRawSnippet, tick } from "svelte";` と `const { default: Modal } = await import("../Modal.svelte");` を足し、末尾に追加する。

```ts
describe("SpeechCoreTermsDialog のフォーカスが外れた後の Escape(Issue #700)", () => {
  const body = createRawSnippet(() => ({ render: () => `<span>settings</span>` }));

  it("bodyに移った後のEscapeは、手前のダイアログだけをキャンセルする", async () => {
    const onclose = vi.fn();
    render(Modal, { props: { title: "親", onclose, children: body } });
    await tick();
    const onCancel = vi.fn();
    render(SpeechCoreTermsDialog, { terms, sizeBytes: 1, onAccept: vi.fn(), onCancel });
    await tick();
    (document.activeElement as HTMLElement).blur();
    expect(document.activeElement).toBe(document.body);
    await fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onclose).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && npx vitest run src/ui/ConfirmDialog.test.ts src/ui/ReportUserDialog.test.ts src/ui/settings/LicenseConsentDialog.test.ts src/ui/settings/SpeechCoreTermsDialog.test.ts`
(`npx vitest run` は複数のパスを受け付ける)
Expected: 追加したうち、キャンセルが呼ばれることを期待する4件が FAIL(呼び出し0回)。「送信中は閉じない」は、修正前でも PASS する(修正後に壊さないための固定)。

- [ ] **Step 3: 実装する**

4ファイルそれぞれに import を足し、`role="dialog"` の要素(`onclick={(e) => e.stopPropagation()}` の次の行)にアクションを付ける。

`ConfirmDialog.svelte`:

```ts
  import { escapeOnLostFocus } from "../lib/escapeOnLostFocus";
```

```svelte
    use:escapeOnLostFocus={() => onCancel()}
```

`ReportUserDialog.svelte`(送信中は `cancel` が何もしない):

```ts
  import { escapeOnLostFocus } from "../lib/escapeOnLostFocus";
```

```svelte
    use:escapeOnLostFocus={cancel}
```

`settings/LicenseConsentDialog.svelte` と `settings/SpeechCoreTermsDialog.svelte`:

```ts
  import { escapeOnLostFocus } from "../../lib/escapeOnLostFocus";
```

```svelte
    use:escapeOnLostFocus={() => onCancel()}
```

- [ ] **Step 4: テストを通す**

Run: Step 2 と同じコマンド
Expected: すべて PASS

- [ ] **Step 5: コミット**

```sh
git add frontend/src/ui/ConfirmDialog.svelte frontend/src/ui/ConfirmDialog.test.ts frontend/src/ui/ReportUserDialog.svelte frontend/src/ui/ReportUserDialog.test.ts frontend/src/ui/settings/LicenseConsentDialog.svelte frontend/src/ui/settings/LicenseConsentDialog.test.ts frontend/src/ui/settings/SpeechCoreTermsDialog.svelte frontend/src/ui/settings/SpeechCoreTermsDialog.test.ts
git commit -m "fix(ui): 個別ダイアログもフォーカスが外れた後の Escape で閉じる(Issue #700)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 規約の更新と検証

**Files:**
- Modify: `docs/design/style-guide.md:133`

- [ ] **Step 1: style-guide の該当行を置き換える**

```markdown
- フォーカス中の要素が消える・無効になるとフォーカスは`body`に移る。`role="dialog" aria-modal="true"`の要素には`use:escapeOnLostFocus={閉じる処理}`(`lib/escapeOnLostFocus.ts`)を付ける。この場合のEscapeをwindowで受け、`[aria-modal="true"], [data-escape-layer]`のうちDOM上で最後のものが自分なら閉じる(Issue #698 / #700)。`aria-modal`を持たずにEscapeを自分で処理する全画面の重ね表示(`MediaViewer`)には`data-escape-layer`を付ける(付けないと下の`Modal`も同時に閉じる)。
```

- [ ] **Step 2: 全テストと型検査**

Run: `cd frontend && pnpm test && pnpm check`
Expected: テストはすべて PASS、`pnpm check` はエラー0件

- [ ] **Step 3: コミット**

```sh
git add docs/design/style-guide.md
git commit -m "docs(style-guide): ダイアログの Escape の受け口の規約を更新する(Issue #700)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 4: 修正後の実画面を確認する**

`pnpm build` と `cargo build` をやり直し(フロントエンドは埋め込み)、Task 1 の spec を再実行する。

期待: 送信失敗後の Escape で通報ダイアログだけが閉じ、プロフィールは残る。もう一度 Escape を送るとプロフィールが閉じる。スクリーンショットを Read で確認する。

- [ ] **Step 5: 後片付け**

```sh
cd e2e && docker compose down -v && rm -rf tmp-issue-700 && git status --short
```

期待: `git status --short` が空。
