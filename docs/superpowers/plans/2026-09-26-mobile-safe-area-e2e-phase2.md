# モバイルE2E セーフエリア検証の拡充 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** モバイルE2Eのセーフエリア検証に、カラム領域の上端（#257の本題）・左inset・メディアビューワーの上端/右/下端を追加する（Issue #384）。

**Architecture:** PR #382 の基盤（`e2e/helpers/mobile.ts`、`wdio.mobile.conf.ts`、`e2e/specs-mobile/`）を拡張する。既存 `safe-area.e2e.ts` にカラム追加後の上端・左inset検証を足し、画像付きノートを事前投稿する新規 `safe-area-media.e2e.ts` でビューワーを検証する。本番コードは変更しない（検出力確認のための一時的な破壊はコミットしない）。

**Tech Stack:** WebdriverIO 9 + tauri-driver, Mocha, TypeScript, Node 22 標準 fetch/FormData/Blob。

設計: `docs/superpowers/specs/2026-09-26-mobile-safe-area-e2e-phase2-design.md`

## Global Constraints

- ブランチは `feat/384-mobile-safe-area-e2e` を使う（main へ直接コミットしない）。push / PR / `gh` コマンドは実装者は実行しない（コントローラーがユーザー確認のうえ行う）。
- コミットメッセージは件名のみ・本文なし（Co-Authored-By トレーラーのみ別途付与）。
- 本番コード（`frontend/src/**`）の変更はコミットしない。検出力確認の一時的な破壊は必ず `git checkout <file>` で戻し、`pnpm build` と `cargo build` をやり直す。
- 各テストは「対応する本番の `var(--safe-*)` を一時的に外して RED」→「戻して GREEN」を実証し、失敗出力（数値付き）をレポートに引用する。検出力の無いテストは Important 欠陥として扱う。アサーションを弱めて通さない。
- 検証はすべて Xvfb 越し（`xvfb-run -a ...`）。実画面にウィンドウを出さない。`pkill`/`killall` 禁止、自分の実行が残したプロセスは PID 指定でのみ kill する（ユーザー自身の dev セッションのプロセスには触れない）。
- E2E 用 Misskey スタック（ホストポート 8443）はコントローラーが最後に `docker compose down -v` する。実装者は落とさない。

## 環境の起動手順（実装者が E2E を実行するとき）

スタックが落ちている場合のみ:

```sh
cd frontend && pnpm build && cd ../src-tauri && cargo build
cd ../e2e && ./scripts/gen-ca.sh && docker compose up -d --wait && pnpm seed
```

実行: `cd e2e && xvfb-run -a pnpm e2e:mobile --spec ./specs-mobile/<file>.e2e.ts`。デバッグバイナリはフロントエンドをビルド時に埋め込むため、`frontend/src` を一時変更したら `pnpm build` と `cargo build` をやり直してから実行する（戻した後も同様）。`8443` が他で使用中なら停止して報告する。

---

### Task 1: カラム領域上端・左inset の検証（既存 safe-area.e2e.ts の拡張）

**Files:**
- Modify: `e2e/helpers/mobile.ts`（`addHomeColumn` を追加）
- Modify: `e2e/specs-mobile/safe-area.e2e.ts`

**Interfaces:**
- Consumes: 既存 `addAccountAndEnableMobile`, `rect`, `setSafeArea`, `SAFE_AREA`。
- Produces（`e2e/helpers/mobile.ts`）: `export async function addHomeColumn(): Promise<void>` — メニュー → カラム追加 → 送信 → `.column-root` の表示待ち。Task 2 が使う。

- [ ] **Step 1: `addHomeColumn` を追加する**

`e2e/helpers/mobile.ts` の末尾に追記（手順は `e2e/specs-mobile/layout.e2e.ts` の3つ目のテストと同じ）:

```ts
export async function addHomeColumn(): Promise<void> {
  await $('[data-testid="app-menu-trigger"]').click();
  await $('[data-testid="app-menu-add-column"]').click();
  const submit = await $('[data-testid="add-column-submit"]');
  await submit.scrollIntoView();
  await submit.waitForClickable({ timeout: 15000 });
  await submit.click();
  await $(".column-root").waitForDisplayed({ timeout: 15000 });
}
```

- [ ] **Step 2: safe-area.e2e.ts にカラム追加とテスト2件を足す**

import に `addHomeColumn` を追加し、`before` の `addAccountAndEnableMobile(...)` の直後に `await addHomeColumn();` を追加する。既存の3テストの後ろに次を追加:

```ts
  it("keeps the column area below the top safe area", async () => {
    // モバイルUIにはheaderが無く、main自身が pt-[var(--safe-top)] でステータスバー分を確保する(Issue #257)。
    const col = await rect(".column-root");
    expect(col).not.toBeNull();
    expect(col!.top).toBeGreaterThanOrEqual(SAFE_AREA.top);
  });

  it("keeps the bottom menu button inside the left safe area", async () => {
    // 既存パディング(max(8px, var(--safe-left)))より十分大きいinsetで var(--safe-left) の効きを検出する。
    const LEFT = 30;
    await setSafeArea({ ...SAFE_AREA, left: LEFT });
    try {
      const trigger = await rect('[data-testid="app-menu-trigger"]');
      expect(trigger).not.toBeNull();
      expect(trigger!.left).toBeGreaterThanOrEqual(LEFT);
    } finally {
      await setSafeArea(SAFE_AREA);
    }
  });
```

（右insetは既存のFABテスト（right=24）と Task 2 のビューワーテストが担う。下部バー右端は対象外。）

- [ ] **Step 3: 環境を起動して実行し GREEN を確認する**

Run: `cd e2e && xvfb-run -a pnpm e2e:mobile --spec ./specs-mobile/safe-area.e2e.ts`
Expected: 5 passing（既存3件が、カラム追加後も通ること）

- [ ] **Step 4: 検出力を確認する（RED）**

2つの一時的な破壊を同時に入れて RED を確認する:
1. `frontend/src/App.svelte` の `<main ... class:pt-[var(--safe-top)]={useMobileUi}>` から `class:pt-[var(--safe-top)]={useMobileUi}` を削除する。
2. `frontend/src/ui/AppMenu.svelte` の `pl-[max(8px,var(--safe-left))]` を `pl-2` に置換する。

`cd frontend && pnpm build && cd ../src-tauri && cargo build` の後に Step 3 のコマンドを再実行し、「カラム上端」テスト（top が 0）と「左inset」テスト（left が 8 前後）が **FAIL** することを確認する（失敗出力を引用）。

- [ ] **Step 5: 元に戻して GREEN を再確認する**

`git checkout frontend/src/App.svelte frontend/src/ui/AppMenu.svelte` → `pnpm build` と `cargo build` をやり直し → Step 3 を再実行して 5 passing。`git status` で本番ファイルの変更が無いことを確認する。

- [ ] **Step 6: Commit**

```bash
git add e2e/helpers/mobile.ts e2e/specs-mobile/safe-area.e2e.ts
git commit -m "test: モバイルのカラム領域上端と左insetのセーフエリアE2Eを追加する"
```

---

### Task 2: メディアビューワーのセーフエリア検証（新規 safe-area-media.e2e.ts）

**Files:**
- Modify: `e2e/helpers/misskeyApi.ts`（`uploadImage` 追加、`createNote` に任意の `fileIds`）
- Create: `e2e/specs-mobile/safe-area-media.e2e.ts`

**Interfaces:**
- Consumes: Task 1 の `addHomeColumn`、既存 `addAccountAndEnableMobile`, `rect`, `setSafeArea`, `SAFE_AREA`, `signInAsSeededUser`。
- Produces（`e2e/helpers/misskeyApi.ts`）:
  - `export async function uploadImage(token: string): Promise<string>` — 1x1 PNG を `/api/drive/files/create`（multipart）でアップロードしファイルidを返す。
  - `export async function createNote(token: string, text: string, fileIds?: string[]): Promise<string>` — 既存呼び出しに影響しない拡張。

- [ ] **Step 1: `uploadImage` と `createNote` の拡張を実装する**

`e2e/helpers/misskeyApi.ts` の `createNote` を次に置き換え、直前に `uploadImage` を追加する:

```ts
// 1x1 PNG。E2Eでメディア付きノートを作るための最小の画像。
const PNG_1X1_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/** `drive/files/create` (multipart/form-data) で1x1 PNGをアップロードし、ファイルidを返す。 */
export async function uploadImage(token: string): Promise<string> {
  const form = new FormData();
  form.append("i", token);
  form.append("file", new Blob([Buffer.from(PNG_1X1_BASE64, "base64")], { type: "image/png" }), "e2e.png");
  const res = await fetch(`${BASE_URL}/api/drive/files/create`, { method: "POST", body: form });
  if (!res.ok) {
    throw new Error(`uploadImage: drive/files/create failed ${res.status}: ${await res.text()}`);
  }
  const body = (await res.json()) as { id: string };
  return body.id;
}

/** `notes/create` でノートを投稿し、投稿したノートのidを返す。`fileIds`指定時のみ添付する。 */
export async function createNote(token: string, text: string, fileIds?: string[]): Promise<string> {
  const res = await fetch(`${BASE_URL}/api/notes/create`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fileIds ? { i: token, text, fileIds } : { i: token, text }),
  });
  if (!res.ok) {
    throw new Error(`createNote: notes/create failed ${res.status}: ${await res.text()}`);
  }
  const body = (await res.json()) as { createdNote: { id: string } };
  return body.createdNote.id;
}
```

（元の `/** notes/create ... */` コメントと関数は上記で置換される。二重定義にしないこと。）

- [ ] **Step 2: spec を書く**

`e2e/specs-mobile/safe-area-media.e2e.ts`:

```ts
// メディアビューワー(render/MediaViewer.svelte)のセーフエリア検証(Issue #384)。
// 画像付きノートを事前にMisskeyへ投稿してHomeカラムに表示し、画像セルをタップしてビューワーを開く。
// 画像準備の失敗リスクを既存specから切り離すため、safe-area.e2e.ts とは別ファイルにしている。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { createNote, signInAsSeededUser, uploadImage } from "../helpers/misskeyApi";
import { addAccountAndEnableMobile, addHomeColumn, rect, SAFE_AREA, setSafeArea } from "../helpers/mobile";

describe("mobile safe-area: media viewer", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    const token = await signInAsSeededUser();
    const fileId = await uploadImage(token);
    await createNote(token, `e2e media viewer ${Date.now()}`, [fileId]);

    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileSafeAreaMedia");
    await addHomeColumn();

    const image = await $(".media-cell img");
    await image.waitForDisplayed({ timeout: 30000 });
    await image.click();
    await $('button[aria-label="閉じる"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await bridge.teardown();
  });

  it("keeps the close button below the top safe area", async () => {
    // 既定padding(0.5rem=8px)より大きい SAFE_AREA.top(47) で var(--safe-top) の効きを検出する。
    const close = await rect('button[aria-label="閉じる"]');
    expect(close).not.toBeNull();
    expect(close!.top).toBeGreaterThanOrEqual(SAFE_AREA.top);
  });

  it("keeps the close button inside the right safe area", async () => {
    const RIGHT = 24;
    await setSafeArea({ ...SAFE_AREA, right: RIGHT });
    try {
      const close = await rect('button[aria-label="閉じる"]');
      const vw = await browser.execute(() => window.innerWidth);
      expect(close).not.toBeNull();
      expect(close!.right).toBeLessThanOrEqual(vw - RIGHT);
    } finally {
      await setSafeArea(SAFE_AREA);
    }
  });

  it("keeps the image toolbar buttons above the bottom safe area", async () => {
    // ツールバー(group)自体はpaddingを含めて下端まで届くため、内側のボタンで測る。
    const zoomIn = await rect('button[aria-label="ズームイン"]');
    const vh = await browser.execute(() => window.innerHeight);
    expect(zoomIn).not.toBeNull();
    expect(zoomIn!.bottom).toBeLessThanOrEqual(vh - SAFE_AREA.bottom);
  });
});
```

- [ ] **Step 3: 実行して結果を確認する（実機挙動の確認）**

Run: `cd e2e && xvfb-run -a pnpm e2e:mobile --spec ./specs-mobile/safe-area-media.e2e.ts`
Expected: 3 passing。失敗した場合は原因を特定する（設計書 §6）: `.media-cell img` が表示/クリックできない（画像が読み込めない・セルが0サイズ等）、ビューワーが開かない、ツールバーが出ない（`MediaViewer.svelte:30` は `isRevealed && isImage(current)` で判定、画像の読み込み成否には依存しない想定）。アサーションを弱めて通さない。解決できなければ BLOCKED として、調べた事実（DOMの状態・スクリーンショット/ログ）を添えて報告する。

- [ ] **Step 4: 検出力を確認する（RED）**

`frontend/src/render/MediaViewer.svelte` に次の3つの一時的な破壊を同時に入れる:
1. 上端の行: `p-[max(0.5rem,var(--safe-top))_max(0.5rem,var(--safe-right))_0.5rem_0.5rem]` → `p-2`
2. 下端のツールバー: `p-[0.5rem_0.5rem_max(0.5rem,var(--safe-bottom))]` → `p-2`

`pnpm build` と `cargo build` の後に Step 3 を再実行し、3テストすべて（上端・右・下端）が **FAIL** することを確認する（失敗出力を引用）。（1 の置換で右paddingも8pxになるため、上端と右の両方が失敗する。）

- [ ] **Step 5: 元に戻して GREEN を再確認する**

`git checkout frontend/src/render/MediaViewer.svelte` → `pnpm build` と `cargo build` をやり直し → Step 3 を再実行して 3 passing。`git status` で本番ファイルの変更が無いことを確認する。

- [ ] **Step 6: Commit**

```bash
git add e2e/helpers/misskeyApi.ts e2e/specs-mobile/safe-area-media.e2e.ts
git commit -m "test: メディアビューワーのセーフエリアE2Eを追加する"
```

---

### Task 3: ドキュメント更新と全体回帰

**Files:**
- Modify: `e2e/README.md`
- Modify: `docs/superpowers/specs/2026-09-26-mobile-e2e-design.md`

**Interfaces:**
- Consumes: Task 1・2 の成果（新規テストの実在と挙動）。

- [ ] **Step 1: README を更新する**

`e2e/README.md` のモバイルUI E2E節を読み、次を反映する: `safe-area` の検証対象にカラム領域上端・左inset・メディアビューワー（`safe-area-media.e2e.ts`、画像付きノートを事前投稿してビューワーを開く）を追加し、「メディアビューワーは未カバー」の記述を削除する。限界の節から該当項目を外し、残る限界（Android WebView固有の挙動、下部バー右端の右insetは未検証）は残す。

- [ ] **Step 2: 旧設計書の限界節を更新する**

`docs/superpowers/specs/2026-09-26-mobile-e2e-design.md` の §2 の `safe-area.e2e.ts` 説明にある「メディアビューワーは…未検証」を「メディアビューワーは Issue #384 で `safe-area-media.e2e.ts` として追加」に、§3（限界）の「メディアビューワーのセーフエリア」項目を削除または同様に更新する。

- [ ] **Step 3: 全体回帰を実行する**

Run: `cd e2e && xvfb-run -a pnpm e2e:mobile` と `xvfb-run -a pnpm e2e`
Expected: どちらも全 PASS（モバイルは、既存8件 + 新規 `safe-area` 2件 + 新規 `safe-area-media` 3件 = 13件）。件数と所要時間を記録する。CI の `e2e` ジョブは `pnpm e2e:mobile` で `specs-mobile/` 全体を実行するため、追加の CI 変更は不要。所要時間の増加が大きい（目安: 追加分が数分規模）場合のみ報告する。

- [ ] **Step 4: 残留プロセスを確認する**

`ps aux | grep -E "Xvfb|gnome-keyring|dbus-run-session|tsumugi"` で自分の実行が残したプロセスを PID 指定で kill する（ユーザー自身の dev セッションのプロセスは触らない）。

- [ ] **Step 5: Commit**

```bash
git add e2e/README.md docs/superpowers/specs/2026-09-26-mobile-e2e-design.md
git commit -m "docs: モバイルE2Eのセーフエリア検証範囲の記述を更新する"
```
