# モバイルE2E overflow検査のカラム内容対応 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** モバイルUIで、ノート内容（長いURL・長い単語・コードブロック・画像）を含むカラムの意図しない横はみ出しを自動検出する（Issue #383）。

**Architecture:** 既存の `overflow.e2e.ts`（クロム・モーダル検査）は変えず、新規 `overflow-column.e2e.ts` を追加する。`before` でシード管理者から4種のノートを投稿してHomeカラムに表示し、アクティブslot（ノート一覧のスクロール要素）の `scrollWidth <= clientWidth` とslotの右端がviewport内であることを検証する。本番コードは変更しない。

**Tech Stack:** WebdriverIO 9 + tauri-driver, Mocha, TypeScript。

設計: `docs/superpowers/specs/2026-09-26-overflow-column-e2e-design.md`

## Global Constraints

- ブランチは `feat/383-overflow-column-content` を使う（main へ直接コミットしない）。push / PR / `gh` コマンドは実装者は実行しない（コントローラーがユーザー確認のうえ行う）。
- コミットメッセージは件名のみ・本文なし（Co-Authored-By トレーラーのみ別途付与）。
- 本番コード（`frontend/src/**`）の変更はコミットしない。検出力確認の一時的な破壊は必ず `git checkout <file>` で戻し、`pnpm build` と `cargo build` をやり直す。
- 各ケースは「対応する本番CSSを一時的に壊して RED」→「戻して GREEN」を実証し、失敗出力（数値付き）をレポートに引用する。検出力の無いテストは Important 欠陥。アサーションを弱めて通さない。
- 新しいチェックが本番の**実際のはみ出しバグ**を見つけた場合（テストの誤りではなく、本番CSSが原因）、アサーションを弱めたり本番を修正したりせず、実装者は停止して `NEEDS_CONTEXT` で報告する（DOMの状態・offenders出力・スクリーンショットを添える）。コントローラーが別Issueを起票し、該当ケースを `it.skip`（Issue番号コメント付き）にする指示を出す。
- 検証はすべて Xvfb 越し（`xvfb-run -a ...`）。実画面にウィンドウを出さない。`pkill`/`killall` 禁止、自分の実行が残したプロセスは PID 指定でのみ kill する（ユーザー自身の dev セッションのプロセスには触れない）。
- E2E 用 Misskey スタック（ホストポート 8443）はコントローラーが最後に `docker compose down -v` する。実装者は落とさない。

## 環境の起動手順（実装者が E2E を実行するとき）

スタックが落ちている場合のみ（`files-init` により新規スタックでも画像アップロードが通る）:

```sh
cd frontend && pnpm build && cd ../src-tauri && cargo build
cd ../e2e && ./scripts/gen-ca.sh && docker compose up -d --wait && pnpm seed
```

実行: `cd e2e && xvfb-run -a pnpm e2e:mobile --spec ./specs-mobile/overflow-column.e2e.ts`。デバッグバイナリはフロントエンドをビルド時に埋め込むため、`frontend/src` を一時変更したら `pnpm build` と `cargo build` をやり直してから実行する（戻した後も同様）。`8443` が他で使用中なら停止して報告する。

---

### Task 1: overflow-column.e2e.ts の追加

**Files:**
- Create: `e2e/specs-mobile/overflow-column.e2e.ts`

**Interfaces:**
- Consumes: 既存 `startMiauthBridge`（`../helpers/miauthBridge`）、`createNote` / `signInAsSeededUser` / `uploadImage`（`../helpers/misskeyApi`）、`addAccountAndEnableMobile` / `addHomeColumn`（`../helpers/mobile`）。
- Produces: なし（新規spec）。

- [ ] **Step 1: spec を書く**

`e2e/specs-mobile/overflow-column.e2e.ts`:

```ts
// カラム内容(ノート本文・URL・コードブロック・画像)による意図しない横はみ出しを検出する(Issue #383)。
// overflow.e2e.ts は [data-columns-scroll] の子孫を除外するため、カラム内は別ファイルのここで検査する。
// 各カラムのノート一覧は Column.svelte のslot(h-full w-full overflow-y-auto)で、overflow-yを指定すると
// overflow-xもautoになるため、中身が幅を超えると意図しない横スクロール(scrollWidth > clientWidth)として現れる。
// コードブロック等、自身でoverflowを持つ要素は slot の scrollWidth に影響しないので、意図したスクロールは除外される。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { createNote, signInAsSeededUser, uploadImage } from "../helpers/misskeyApi";
import { addAccountAndEnableMobile, addHomeColumn } from "../helpers/mobile";

const SLOT = ".column-root .mobile-scroll-snap > div";
const MARKERS = ["E2E-OVF-URL", "E2E-OVF-WORD", "E2E-OVF-CODE", "E2E-OVF-IMG"];

const measureSlot = () =>
  browser.execute((sel) => {
    const slot = document.querySelector(sel) as HTMLElement | null;
    if (!slot) return null;
    const slotRect = slot.getBoundingClientRect();
    const offenders: string[] = [];
    for (const el of Array.from(slot.querySelectorAll("*"))) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      if (r.right > slotRect.right + 1) {
        offenders.push(`${el.tagName.toLowerCase()}.${String(el.getAttribute("class") ?? "").slice(0, 60)} right=${Math.round(r.right)} slotRight=${Math.round(slotRect.right)}`);
      }
    }
    return {
      scrollWidth: slot.scrollWidth,
      clientWidth: slot.clientWidth,
      slotRight: slotRect.right,
      vw: window.innerWidth,
      offenders: offenders.slice(0, 10),
    };
  }, SLOT);

describe("mobile horizontal overflow: column content", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    const token = await signInAsSeededUser();
    const fileId = await uploadImage(token);
    await createNote(token, `E2E-OVF-URL https://example.com/${"a".repeat(300)}`);
    await createNote(token, `E2E-OVF-WORD ${"W".repeat(300)}`);
    await createNote(token, `E2E-OVF-CODE\n\`\`\`\nconst x = '${"c".repeat(300)}';\n\`\`\``);
    await createNote(token, "E2E-OVF-IMG", [fileId]);

    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileOverflowColumn");
    await addHomeColumn();

    // ノートが描画される前に測ると空振りするため、4件すべてが描画されるまで待つ。
    await browser.waitUntil(
      async () => {
        const text = await browser.execute((sel) => (document.querySelector(sel) as HTMLElement | null)?.innerText ?? "", SLOT);
        return MARKERS.every((m) => text.includes(m));
      },
      { timeout: 30000, interval: 500, timeoutMsg: "posted notes did not render in the column" },
    );
  });

  after(async () => {
    await bridge?.teardown();
  });

  it("has no unintended horizontal scroll in the column note list", async () => {
    const m = await measureSlot();
    expect(m).not.toBeNull();
    const over = m!.scrollWidth - m!.clientWidth > 1;
    // 失敗時に原因ノートを特定できるよう、幅を超えている要素の一覧をメッセージに含める。
    expect({ over, offenders: over ? m!.offenders : [] }).toEqual({ over: false, offenders: [] });
  });

  it("keeps the column note list inside the viewport", async () => {
    const m = await measureSlot();
    expect(m).not.toBeNull();
    expect(m!.slotRight).toBeLessThanOrEqual(m!.vw + 1);
  });
});
```

- [ ] **Step 2: 環境を起動して実行し GREEN を確認する**

Run: `cd e2e && xvfb-run -a pnpm e2e:mobile --spec ./specs-mobile/overflow-column.e2e.ts`
Expected: 2 passing。

失敗した場合は原因を切り分ける:
- テスト側の誤り（セレクタ `.column-root .mobile-scroll-snap > div` が空、ノートが描画されない、マーカーがMFM変換で欠ける等）: 実DOMを確認して修正し、逸脱をレポートに記録する。
- 本番の実際のはみ出し: 上記 Global Constraints のとおり停止して `NEEDS_CONTEXT` で報告する（`offenders` 出力を添える）。

- [ ] **Step 3: 検出力を確認する（RED）**

次の一時的な破壊を1つずつ入れ、そのたびに `cd frontend && pnpm build && cd ../src-tauri && cargo build` の後に Step 2 のコマンドを実行して、1つ目のテストが **FAIL** することと、`offenders` が該当要素を指していることを確認する（失敗出力を引用）。

1. **長い単語・URL**: `frontend/src/ui/NoteCard.svelte` のノート本文コンテナ `<div class="min-w-0 flex-1">`（`<header class="flex flex-wrap ...">` の親、356行付近）を `<div class="flex-1">` にする。
2. **コードブロック**: `frontend/src/app.css` の `.mfm-codeblock` 配下でコードブロックの `pre` に `overflow-x: auto;` を与えているルール（290行付近）の `overflow-x: auto;` を `overflow-x: visible;` にする（shikiのHTML出力用ルールが別にある場合は、実際にコードブロックへ適用されている方を対象にする。DOMで確認すること）。
3. **画像**: `frontend/src/render/MediaGrid.svelte` の1枚用グリッド `mt-2 grid grid-cols-1 gap-1` の `grid-cols-1` を `grid-cols-[600px]` にする。

各破壊は独立して確認し（1つ壊す→build→実行→戻す）、3つとも RED になること。RED にならない破壊がある場合は、その理由（DOM上でその要素が実際に使われているか、slotのscrollWidthに反映されるか）を調べ、検出力のあるケースに直す（アサーションを弱めない）。

- [ ] **Step 4: 元に戻して GREEN を再確認する**

`git checkout frontend/src` → `pnpm build` と `cargo build` をやり直し → Step 2 を再実行して 2 passing。`git status` で本番ファイルの変更が無いことを確認する。

- [ ] **Step 5: Commit**

```bash
git add e2e/specs-mobile/overflow-column.e2e.ts
git commit -m "test: モバイルのカラム内容による横はみ出しを検出するE2Eを追加する"
```

---

### Task 2: ドキュメント更新と全体回帰

**Files:**
- Modify: `e2e/README.md`
- Modify: `docs/superpowers/specs/2026-09-26-mobile-e2e-design.md`

**Interfaces:**
- Consumes: Task 1 の成果（`overflow-column.e2e.ts` の挙動）。

- [ ] **Step 1: README を更新する**

`e2e/README.md` のモバイルUI E2E節を読み、overflow検査の説明に、`overflow-column.e2e.ts` を追加する: ノート内容（長いURL・長い単語・コードブロック・画像）を含むカラムで、アクティブslot（ノート一覧）が意図しない横スクロール（`scrollWidth > clientWidth`）を起こしていないことを検証する。コードブロック等の自前overflow要素は除外される。既存の「overflow」の説明にある「カラム内は未検査」の趣旨の記述があれば更新する（実物を読んで、実際の記述に合わせる）。

- [ ] **Step 2: 旧設計書を更新する**

`docs/superpowers/specs/2026-09-26-mobile-e2e-design.md` の §2 の `overflow.e2e.ts` の説明（`[data-columns-scroll]` の外側のみ検査、という記述）に、カラム内容は Issue #383 で `overflow-column.e2e.ts` として追加した旨を1文で追記する。§3（限界）にカラム内容の未検査を示す項目があれば更新する。

- [ ] **Step 3: 全体回帰を実行する**

Run: `cd e2e && xvfb-run -a pnpm e2e:mobile` と `xvfb-run -a pnpm e2e`
Expected: どちらも全 PASS（モバイルは既存13件 + 新規2件 = 15件、デスクトップは16件）。件数と所要時間を記録する。CI の `e2e` ジョブは `pnpm e2e:mobile` で `specs-mobile/` 全体を実行するため、CI 変更は不要。

- [ ] **Step 4: 残留プロセスを確認する**

`ps aux | grep -E "Xvfb|gnome-keyring|dbus-run-session|tsumugi"` で自分の実行が残したプロセスを PID 指定で kill する（ユーザー自身の dev セッションのプロセスは触らない）。

- [ ] **Step 5: Commit**

```bash
git add e2e/README.md docs/superpowers/specs/2026-09-26-mobile-e2e-design.md
git commit -m "docs: モバイルE2Eのoverflow検査範囲の記述を更新する"
```
