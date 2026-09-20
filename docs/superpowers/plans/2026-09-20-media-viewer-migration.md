# メディアビューワー移行 (Issue #295) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `frontend/src/render/MediaGrid.svelte` の `viewerjs` 依存を、画像はCropper.js v2、動画・音声はVidstack、前後送り殻・閲覧注意ゲーティング・ツールバーは自前実装、という構成の新規`MediaViewer.svelte`に置き換える。

**Architecture:** `frontend/src/lib/mediaViewer.svelte.ts` に純粋ロジック（対象アイテム抽出・前後送り・閲覧注意ゲーティング・回転/反転状態）を切り出してVitestで検証し、`frontend/src/render/MediaViewer.svelte` がそれを使ってportal表示のフルスクリーンビューワーを組み立てる。画像は`<cropper-canvas>`+`<cropper-image>`、動画・音声は`<media-player>`（Vidstackのdefault layout）を使い、custom elements内部の挙動はVitestでは検証せず`cargo tauri dev`での実機確認に委ねる。

**Tech Stack:** Svelte 5 (runes) / TypeScript / Vite / Tauri v2 / `cropperjs` v2 / `vidstack` / `@panzoom/panzoom` / Vitest + @testing-library/svelte

## Global Constraints

- 削除: `viewerjs`（`frontend/package.json`の`dependencies`から）
- 追加: `cropperjs`（^2.2.0）、`vidstack`（^0.6.15）、`@panzoom/panzoom`（^4.6.2）
- アイコンは既存の`@lucide/svelte`のみ使用し新規アイコンライブラリを追加しない
- 色はCSS変数を直書きせず`app.css`の`--accent`/`--surface-*`/`--border`トークン経由（`docs/design/style-guide.md` §1）
- 角丸は`rounded-md`（6px）系トークンに統一し、非Tailwindコンテキストでは`border-radius: 6px; /* rounded-md相当 */`とコメントを添える（`docs/design/style-guide.md` §2、既存コードの慣例）
- パン/スワイプ操作は**JSのpointerドラッグ横取り方式を使わない**（過去の実機検証で`pointercancel`により機能しないと判明済み、CSS Scroll Snapを使う）
- 実機確認は必ずXvfb越し、かつ`WAYLAND_DISPLAY`をunsetして行う（Waylandセッションでは`DISPLAY`設定だけでは実画面に描画が漏れる）
- 自分で起動した`cargo tauri dev`は正確なPIDで終了する（`pkill`/`killall`は使わない）
- コミットメッセージは件名のみ（本文・箇条書きなし）

---

## File Structure

- **Create** `frontend/src/lib/mediaViewer.svelte.ts` — 対象アイテム抽出、前後送りindex計算、閲覧注意ゲーティング、画像のrotation/flip状態管理という純粋ロジック（custom elementsに依存しない）
- **Create** `frontend/src/lib/mediaViewer.svelte.test.ts` — 上記のVitestテスト
- **Create** `frontend/src/lib/mediaDownload.ts` — `MediaGrid.svelte`に既存の`saveToDisk`相当のロジックを、`MediaViewer.svelte`からも呼べるよう切り出したもの
- **Create** `frontend/src/render/MediaViewer.svelte` — フルスクリーンビューワー本体（portal表示、Scroll Snapナビゲーション、閲覧注意ゲーティング、画像/動画/音声の切り替え表示）
- **Create** `frontend/src/render/MediaViewer.test.ts` — コンポーネントテスト（DOM構造・イベント配線のみ、custom elements内部は対象外）
- **Modify** `frontend/src/render/MediaGrid.svelte` — `viewerjs`削除、`MediaViewer`起動導線の追加（画像クリック・動画/音声の拡大ボタン）
- **Modify** `frontend/package.json` — 依存関係の追加・削除

---

### Task 1: 依存関係の入れ替え

**Files:**
- Modify: `frontend/package.json`

**Interfaces:**
- Produces: `cropperjs`、`vidstack`、`@panzoom/panzoom`がnode_modulesにインストールされ、後続タスクから`import`できる状態

- [ ] **Step 1: viewerjsを削除し、新規依存を追加する**

```bash
cd frontend
pnpm remove viewerjs
pnpm add cropperjs@^2.2.0 vidstack@^0.6.15 @panzoom/panzoom@^4.6.2
```

- [ ] **Step 2: インストール結果を確認する**

Run: `cd frontend && cat package.json | grep -E "cropperjs|vidstack|panzoom|viewerjs"`
Expected: `viewerjs`が出力されず、`cropperjs`・`vidstack`・`@panzoom/panzoom`が`dependencies`に表示される

- [ ] **Step 3: 型チェックが依存関係の変更だけで壊れていないことを確認する**

Run: `cd frontend && pnpm check`
Expected: 既存のエラーが増えていないこと（`viewerjs`未使用による警告はTask 6で解消するため、この時点では`MediaGrid.svelte`の`import Viewer from "viewerjs"`がモジュール未検出エラーになるのは想定内）

- [ ] **Step 4: コミット**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml
git commit -m "build: viewerjsをcropperjs/vidstack/panzoomに入れ替え"
```

---

### Task 2: 純粋ロジックの実装とテスト（`lib/mediaViewer.svelte.ts`）

custom elements（`<cropper-image>`/`<media-player>`）はjsdomで意味のある初期化ができないため、テスト可能なロジックをここに集約する。

**Files:**
- Create: `frontend/src/lib/mediaViewer.svelte.ts`
- Create: `frontend/src/lib/mediaViewer.svelte.test.ts`

**Interfaces:**
- Consumes: `DriveFile`型（`frontend/src/bindings/tauri.gen.ts`、フィールド: `id: string`, `mimeType: string`, `isSensitive: boolean`, `url: string`, `thumbnailUrl: string | null`, `name?: string`）
- Produces:
  - `deriveViewItems(files: DriveFile[]): DriveFile[]`
  - `nextIndex(current: number, length: number): number`
  - `prevIndex(current: number, length: number): number`
  - `isRevealed(revealed: Record<string, boolean>, file: DriveFile): boolean`
  - `reveal(revealed: Record<string, boolean>, file: DriveFile): Record<string, boolean>`
  - `type ImageTransform = { rotation: 0 | 90 | 180 | 270; flipH: boolean; flipV: boolean }`
  - `initialImageTransform: ImageTransform`
  - `rotateCW(t: ImageTransform): ImageTransform`
  - `rotateCCW(t: ImageTransform): ImageTransform`
  - `toggleFlipH(t: ImageTransform): ImageTransform`
  - `toggleFlipV(t: ImageTransform): ImageTransform`

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/mediaViewer.svelte.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { DriveFile } from "../bindings/tauri.gen";
import {
  deriveViewItems,
  initialImageTransform,
  isRevealed,
  nextIndex,
  prevIndex,
  reveal,
  rotateCCW,
  rotateCW,
  toggleFlipH,
  toggleFlipV,
} from "./mediaViewer.svelte";

function file(overrides: Partial<DriveFile>): DriveFile {
  return {
    id: "f1",
    mimeType: "image/png",
    isSensitive: false,
    url: "https://example.com/f1.png",
    thumbnailUrl: null,
    ...overrides,
  };
}

describe("deriveViewItems", () => {
  it("画像・動画・音声のみを残し、その他のファイルは除外する", () => {
    const files = [
      file({ id: "img", mimeType: "image/png" }),
      file({ id: "vid", mimeType: "video/mp4" }),
      file({ id: "aud", mimeType: "audio/mpeg" }),
      file({ id: "pdf", mimeType: "application/pdf" }),
    ];
    expect(deriveViewItems(files).map((f) => f.id)).toEqual(["img", "vid", "aud"]);
  });
});

describe("nextIndex / prevIndex", () => {
  it("nextIndexは末尾で先頭に折り返す", () => {
    expect(nextIndex(2, 3)).toBe(0);
    expect(nextIndex(0, 3)).toBe(1);
  });

  it("prevIndexは先頭で末尾に折り返す", () => {
    expect(prevIndex(0, 3)).toBe(2);
    expect(prevIndex(2, 3)).toBe(1);
  });
});

describe("閲覧注意ゲーティング", () => {
  it("isSensitiveでなければ常にrevealed扱い", () => {
    const f = file({ id: "img", isSensitive: false });
    expect(isRevealed({}, f)).toBe(true);
  });

  it("isSensitiveかつrevealed未登録ならfalse", () => {
    const f = file({ id: "img", isSensitive: true });
    expect(isRevealed({}, f)).toBe(false);
  });

  it("isSensitiveでもrevealedに登録済みならtrue", () => {
    const f = file({ id: "img", isSensitive: true });
    expect(isRevealed({ img: true }, f)).toBe(true);
  });

  it("revealで対象ファイルのidだけをtrueにした新しいRecordを返す", () => {
    const f = file({ id: "img", isSensitive: true });
    const result = reveal({ other: true }, f);
    expect(result).toEqual({ other: true, img: true });
  });
});

describe("画像のrotation/flip状態", () => {
  it("初期状態はrotation:0, flipH/flipV:false", () => {
    expect(initialImageTransform).toEqual({ rotation: 0, flipH: false, flipV: false });
  });

  it("rotateCWは90度ずつ進み、270の次は0に戻る", () => {
    let t = initialImageTransform;
    t = rotateCW(t);
    expect(t.rotation).toBe(90);
    t = rotateCW(rotateCW(t));
    expect(t.rotation).toBe(270);
    t = rotateCW(t);
    expect(t.rotation).toBe(0);
  });

  it("rotateCCWは90度ずつ戻り、0の前は270になる", () => {
    let t = initialImageTransform;
    t = rotateCCW(t);
    expect(t.rotation).toBe(270);
  });

  it("toggleFlipH/toggleFlipVはbooleanを反転する", () => {
    let t = initialImageTransform;
    t = toggleFlipH(t);
    expect(t.flipH).toBe(true);
    t = toggleFlipV(t);
    expect(t).toEqual({ rotation: 0, flipH: true, flipV: true });
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/lib/mediaViewer.svelte.test.ts`
Expected: FAIL（`./mediaViewer.svelte`が存在しない）

- [ ] **Step 3: 実装を書く**

`frontend/src/lib/mediaViewer.svelte.ts`:

```ts
import type { DriveFile } from "../bindings/tauri.gen";

const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");
const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
const isAudio = (f: DriveFile) => f.mimeType.startsWith("audio/");

/// MediaViewerで前後送りする対象（画像・動画・音声）だけを残す。
/// その他のファイル（📄表示のもの）はMediaGrid側で従来通りopenUrlするためここでは除外する。
export function deriveViewItems(files: DriveFile[]): DriveFile[] {
  return files.filter((f) => isImage(f) || isVideo(f) || isAudio(f));
}

export function nextIndex(current: number, length: number): number {
  return (current + 1) % length;
}

export function prevIndex(current: number, length: number): number {
  return (current - 1 + length) % length;
}

/// 閲覧注意でなければ常に表示可、閲覧注意なら revealed に登録済みかどうかで判定する。
export function isRevealed(revealed: Record<string, boolean>, file: DriveFile): boolean {
  return !file.isSensitive || !!revealed[file.id];
}

export function reveal(
  revealed: Record<string, boolean>,
  file: DriveFile,
): Record<string, boolean> {
  return { ...revealed, [file.id]: true };
}

export type ImageTransform = {
  rotation: 0 | 90 | 180 | 270;
  flipH: boolean;
  flipV: boolean;
};

export const initialImageTransform: ImageTransform = { rotation: 0, flipH: false, flipV: false };

const ROTATIONS = [0, 90, 180, 270] as const;

export function rotateCW(t: ImageTransform): ImageTransform {
  const i = ROTATIONS.indexOf(t.rotation);
  return { ...t, rotation: ROTATIONS[(i + 1) % ROTATIONS.length] };
}

export function rotateCCW(t: ImageTransform): ImageTransform {
  const i = ROTATIONS.indexOf(t.rotation);
  return { ...t, rotation: ROTATIONS[(i - 1 + ROTATIONS.length) % ROTATIONS.length] };
}

export function toggleFlipH(t: ImageTransform): ImageTransform {
  return { ...t, flipH: !t.flipH };
}

export function toggleFlipV(t: ImageTransform): ImageTransform {
  return { ...t, flipV: !t.flipV };
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm vitest run src/lib/mediaViewer.svelte.test.ts`
Expected: PASS（全テストケース）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mediaViewer.svelte.ts frontend/src/lib/mediaViewer.svelte.test.ts
git commit -m "feat: MediaViewer用の純粋ロジックを追加"
```

---

### Task 3: ダウンロード処理の共通化（`lib/mediaDownload.ts`）

現在`MediaGrid.svelte`内にある`saveToDisk`を、`MediaViewer.svelte`からも使えるよう切り出す。

**Files:**
- Create: `frontend/src/lib/mediaDownload.ts`
- Modify: `frontend/src/render/MediaGrid.svelte:21-29`（`saveToDisk`関数を削除し、共通関数を呼ぶように変更）

**Interfaces:**
- Consumes: `commands.saveUrlToFile`（`frontend/src/lib/ipc`経由）、`@tauri-apps/plugin-dialog`の`save`
- Produces: `saveMediaToDisk(url: string, suggestedName: string, onError: (e: unknown) => void): Promise<void>`

- [ ] **Step 1: 共通関数を実装する**

`frontend/src/lib/mediaDownload.ts`:

```ts
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { commands, unwrap } from "./ipc";

/// メディアURLをファイルダイアログで選んだ保存先に書き出す。
/// MediaGrid（グリッド内の💾ボタン）とMediaViewer（拡大ビューワーのダウンロードボタン）の両方から使う。
export async function saveMediaToDisk(
  url: string,
  suggestedName: string,
  onError: (e: unknown) => void,
): Promise<void> {
  try {
    const path = await saveDialog({ defaultPath: suggestedName });
    if (!path) return;
    await unwrap(commands.saveUrlToFile(url, path));
  } catch (e) {
    onError(e);
  }
}
```

- [ ] **Step 2: `MediaGrid.svelte`から呼び出すように書き換える**

`frontend/src/render/MediaGrid.svelte` の既存 `saveToDisk` 関数（1-29行目付近、`async function saveToDisk(url: string, suggestedName: string) { ... }`）を削除し、代わりに以下をimportして使う:

```ts
import { saveMediaToDisk } from "../lib/mediaDownload";
```

呼び出し箇所（`onclick={() => saveToDisk(f.url, fileName(f))}`が2箇所）は次のように置き換える:

```ts
onclick={() => saveMediaToDisk(f.url, fileName(f), (e) => app.reportError(e))}
```

- [ ] **Step 3: 型チェックとテストを実行する**

Run: `cd frontend && pnpm check && pnpm vitest run`
Expected: エラーなし、既存テストも通ること

- [ ] **Step 4: `cargo tauri dev`で動画・音声の💾ボタンからのダウンロードが従来通り動くことを目視確認する**

（3.11節の実機確認手順に従う: Xvfb起動、`WAYLAND_DISPLAY`をunset、`WEBKIT_DISABLE_DMABUF_RENDERER=1`は`main.rs`が既定で設定済み）

```bash
Xvfb :99 -screen 0 1280x800x24 &
XVFB_PID=$!
export DISPLAY=:99
unset WAYLAND_DISPLAY
cargo tauri dev
```

動画・音声タイルの💾ボタンでファイル保存ダイアログが開き、保存が成功することを確認したら、`kill $XVFB_PID`と`cargo tauri dev`のPIDを`ps aux`で確認して終了する。

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/mediaDownload.ts frontend/src/render/MediaGrid.svelte
git commit -m "refactor: メディアのダウンロード処理をlib/mediaDownload.tsへ共通化"
```

---

### Task 4: MediaViewerの土台（portal表示・Scroll Snapナビゲーション・画像表示）

このタスクが最大のリスク（Cropper.js v2のピンチズーム挙動未検証、パン/スワイプ競合解決）を最初に検証するスパイクを兼ねる。画像のみ対応した状態でまず動くビューワーを作る。

**Files:**
- Create: `frontend/src/render/MediaViewer.svelte`
- Create: `frontend/src/render/MediaViewer.test.ts`

**Interfaces:**
- Consumes: `frontend/src/lib/portal.ts`の`portal`アクション、`frontend/src/lib/mediaViewer.svelte.ts`の関数群、`frontend/src/lib/mediaDownload.ts`の`saveMediaToDisk`
- Produces: `MediaViewer`コンポーネント。props: `files: DriveFile[]`, `startIndex: number`, `revealed: Record<string, boolean>`（`$bindable`）, `onclose: () => void`

- [ ] **Step 1: 失敗するコンポーネントテストを書く**

`frontend/src/render/MediaViewer.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import MediaViewer from "./MediaViewer.svelte";
import type { DriveFile } from "../bindings/tauri.gen";

function file(overrides: Partial<DriveFile>): DriveFile {
  return {
    id: "f1",
    mimeType: "image/png",
    isSensitive: false,
    url: "https://example.com/f1.png",
    thumbnailUrl: null,
    name: "f1.png",
    ...overrides,
  };
}

afterEach(() => cleanup());

describe("MediaViewer", () => {
  it("Escapeキーでoncloseが呼ばれる", async () => {
    const onclose = vi.fn();
    render(MediaViewer, {
      props: {
        files: [file({ id: "a" })],
        startIndex: 0,
        revealed: {},
        onclose,
      },
    });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("背景クリックでoncloseが呼ばれる", async () => {
    const onclose = vi.fn();
    const { getByRole } = render(MediaViewer, {
      props: {
        files: [file({ id: "a" })],
        startIndex: 0,
        revealed: {},
        onclose,
      },
    });
    await fireEvent.click(getByRole("presentation"));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("次へボタンで2件目のページへscrollIntoViewする", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    const { getByLabelText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    scrollIntoView.mockClear();
    await fireEvent.click(getByLabelText("次へ"));
    expect(scrollIntoView).toHaveBeenCalledOnce();
    const [, secondPage] = document.querySelectorAll('[data-testid="media-page"]');
    expect(scrollIntoView.mock.instances[0]).toBe(secondPage);
  });

  it("閲覧注意ファイルは未表示ならカバーを表示し、クリックで表示状態になる", async () => {
    const { getByText, queryByText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", isSensitive: true })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(getByText("閲覧注意（クリックで表示）")).toBeTruthy();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    expect(queryByText("閲覧注意（クリックで表示）")).toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm vitest run src/render/MediaViewer.test.ts`
Expected: FAIL（`./MediaViewer.svelte`が存在しない）

- [ ] **Step 3: MediaViewer.svelteを実装する**

`frontend/src/render/MediaViewer.svelte`:

```svelte
<script lang="ts">
  import "cropperjs";
  import { ChevronLeft, ChevronRight, Download, FlipHorizontal, FlipVertical, RotateCcw, RotateCw, X, ZoomIn, ZoomOut } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { portal } from "../lib/portal";
  import { saveMediaToDisk } from "../lib/mediaDownload";
  import { app } from "../lib/store.svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  import {
    deriveViewItems,
    initialImageTransform,
    isRevealed,
    nextIndex,
    prevIndex,
    reveal,
    rotateCCW,
    rotateCW,
    toggleFlipH,
    toggleFlipV,
    type ImageTransform,
  } from "../lib/mediaViewer.svelte";

  let {
    files,
    startIndex,
    revealed = $bindable(),
    onclose,
  }: {
    files: DriveFile[];
    startIndex: number;
    revealed: Record<string, boolean>;
    onclose: () => void;
  } = $props();

  const viewItems = deriveViewItems(files);
  let currentIndex = $state(startIndex);
  let imageTransform = $state<ImageTransform>(initialImageTransform);

  const current = $derived(viewItems[currentIndex]);
  const fileName = (f: DriveFile) => f.name || f.mimeType || "file";
  const isImage = (f: DriveFile) => f.mimeType.startsWith("image/");

  let scrollEl: HTMLDivElement | undefined;
  let cropperImageEl: (HTMLElement & { $rotate: (a: number) => void; $scale: (x: number, y?: number) => void; $zoom: (s: number, x?: number, y?: number) => void; $resetTransform: () => void }) | undefined;

  function goTo(index: number, behavior: ScrollBehavior = "smooth") {
    currentIndex = index;
    imageTransform = initialImageTransform;
    scrollEl?.children[index]?.scrollIntoView({ behavior, inline: "start", block: "nearest" });
  }

  function goNext() {
    goTo(nextIndex(currentIndex, viewItems.length));
  }

  function goPrev() {
    goTo(prevIndex(currentIndex, viewItems.length));
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    else if (e.key === "ArrowRight") goNext();
    else if (e.key === "ArrowLeft") goPrev();
  }

  function onRevealClick() {
    revealed = reveal(revealed, current);
  }

  // Scroll Snapコンテナのスクロールが落ち着いたら、実際に表示されている位置に
  // currentIndexを合わせる（矢印ボタン以外にスワイプでもcurrentIndexが正しく追従するように）。
  let scrollEndTimer: ReturnType<typeof setTimeout> | undefined;
  function onScroll() {
    clearTimeout(scrollEndTimer);
    scrollEndTimer = setTimeout(() => {
      if (!scrollEl) return;
      const width = scrollEl.clientWidth;
      const index = Math.round(scrollEl.scrollLeft / width);
      if (index !== currentIndex && index >= 0 && index < viewItems.length) {
        currentIndex = index;
        imageTransform = initialImageTransform;
      }
    }, 120);
  }

  function applyImageTransform() {
    if (!cropperImageEl) return;
    cropperImageEl.$resetTransform();
    cropperImageEl.$rotate(imageTransform.rotation);
    cropperImageEl.$scale(imageTransform.flipH ? -1 : 1, imageTransform.flipV ? -1 : 1);
  }

  $effect(() => {
    void imageTransform;
    applyImageTransform();
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="fixed inset-0 z-[1000] flex flex-col bg-black/90"
  use:portal
  onclick={onclose}
  role="presentation"
>
  <div class="flex flex-none items-center justify-end p-[max(0.5rem,env(safe-area-inset-top))_max(0.5rem,env(safe-area-inset-right))_0.5rem_0.5rem]">
    <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={(e) => { e.stopPropagation(); onclose(); }} aria-label="閉じる">
      <X size={16} />
    </Button>
  </div>

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={scrollEl}
    onscroll={onScroll}
    onclick={(e) => e.stopPropagation()}
    class="flex flex-1 snap-x snap-mandatory overflow-x-auto overflow-y-hidden [scrollbar-width:none]"
  >
    {#each viewItems as item (item.id)}
      <div
        class="flex h-full w-full flex-none snap-start items-center justify-center"
        data-testid="media-page"
        aria-label={isRevealed(revealed, item) ? `${fileName(item)}を表示中` : undefined}
      >
        {#if !isRevealed(revealed, item)}
          <button
            class="border-0 bg-transparent text-base text-white"
            onclick={item.id === current.id ? onRevealClick : undefined}
          >
            閲覧注意（クリックで表示）
          </button>
        {:else if isImage(item)}
          <cropper-canvas class="h-full w-full" background="false">
            <cropper-image
              bind:this={item.id === current.id ? cropperImageEl : undefined}
              src={item.url}
              alt={fileName(item)}
              rotatable
              scalable
              class="h-full w-full"
            ></cropper-image>
          </cropper-canvas>
        {:else}
          <span class="text-white">{fileName(item)}（動画・音声プレーヤーは別タスクで対応）</span>
        {/if}
      </div>
    {/each}
  </div>

  {#if viewItems.length > 1}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="group absolute top-0 left-0 flex h-full w-[15%] min-w-10 cursor-w-resize items-center justify-start pl-2"
      onclick={(e) => { e.stopPropagation(); goPrev(); }}
    >
      <button
        class="rounded-full bg-black/40 p-2 text-white opacity-0 transition-opacity group-hover:opacity-100"
        onclick={(e) => { e.stopPropagation(); goPrev(); }}
        aria-label="前へ"
      >
        <ChevronLeft size={20} />
      </button>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="group absolute top-0 right-0 flex h-full w-[15%] min-w-10 cursor-e-resize items-center justify-end pr-2"
      onclick={(e) => { e.stopPropagation(); goNext(); }}
    >
      <button
        class="rounded-full bg-black/40 p-2 text-white opacity-0 transition-opacity group-hover:opacity-100"
        onclick={(e) => { e.stopPropagation(); goNext(); }}
        aria-label="次へ"
      >
        <ChevronRight size={20} />
      </button>
    </div>
  {/if}

  {#if isRevealed(revealed, current) && isImage(current)}
    <div
      class="flex flex-none items-center justify-center gap-1 p-[0.5rem_0.5rem_max(0.5rem,env(safe-area-inset-bottom))]"
      onclick={(e) => e.stopPropagation()}
      role="toolbar"
      aria-label="画像ツールバー"
    >
      <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={() => cropperImageEl?.$zoom(0.1)} aria-label="ズームイン"><ZoomIn size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={() => cropperImageEl?.$zoom(-0.1)} aria-label="ズームアウト"><ZoomOut size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={() => (imageTransform = rotateCCW(imageTransform))} aria-label="左回転"><RotateCcw size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={() => (imageTransform = rotateCW(imageTransform))} aria-label="右回転"><RotateCw size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={() => (imageTransform = toggleFlipH(imageTransform))} aria-label="左右反転"><FlipHorizontal size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={() => (imageTransform = toggleFlipV(imageTransform))} aria-label="上下反転"><FlipVertical size={16} /></Button>
      <Button variant="ghost" size="icon" class="text-white hover:bg-white/10 hover:text-white" onclick={() => saveMediaToDisk(current.url, fileName(current), (e) => app.reportError(e))} aria-label="ダウンロード"><Download size={16} /></Button>
    </div>
  {/if}
</div>
```

補足:
- `cropper-image`の`translatable`属性はこのステップでは付与していない（等倍時は無効、ズーム時のみ有効にする配線はStep 5で追加する）。
- `bind:this={item.id === current.id ? cropperImageEl : undefined}` は各`{#each}`反復ごとに評価されるSvelteの`bind:this`の書き方で、現在表示中のアイテムの`cropper-image`要素だけを`cropperImageEl`に束縛する。

- [ ] **Step 4: テストを実行し、Esc・背景クリック・次へボタン・閲覧注意ゲーティングのテストが通ることを確認する**

Run: `cd frontend && pnpm vitest run src/render/MediaViewer.test.ts`
Expected: PASS（4件とも）

- [ ] **Step 5: ズーム時のみパンを有効化する配線を追加する**

`transform`イベント（`detail.matrix`、`[a, b, c, d, e, f]`のCSS行列で`a`がX方向スケール）を購読し、`scale > 1`の間だけ`translatable`属性をつける。`MediaViewer.svelte`の`<script>`ブロックに追加:

```ts
function onCropperImageTransform(e: Event) {
  const detail = (e as CustomEvent<{ matrix: number[] }>).detail;
  const scale = detail.matrix[0];
  cropperImageEl?.toggleAttribute("translatable", scale > 1 + 1e-6);
}
```

`<cropper-image>`タグに以下を追加する:

```svelte
ontransform={onCropperImageTransform}
```

- [ ] **Step 6: `pnpm check`で型エラーがないことを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし（custom elementsの型が未知でエラーになる場合は、`frontend/src/vite-env.d.ts`または新規`frontend/src/cropperjs.d.ts`に以下の最小限のアンビエント宣言を追加する）

```ts
// frontend/src/cropperjs.d.ts
declare namespace svelteHTML {
  interface IntrinsicElements {
    "cropper-canvas": Record<string, unknown>;
    "cropper-image": Record<string, unknown>;
  }
}
```

- [ ] **Step 7: 実機で画像のズーム/パン/回転/反転を確認する（このタスクの本命の検証）**

3.11節の手順でXvfb越しに`cargo tauri dev`を起動し、以下を確認する:

1. 画像付きノートのグリッドで画像をクリックしてもまだ何も起きない（Task 6でMediaGridと配線するまでは未接続。この時点ではVitestのコンポーネントテストのみで疎通確認とする）
2. 上記のVitestテストが通っていることと、`pnpm check`が通っていることをもって本タスクの完了条件とする。ピンチズーム・パン/スワイプ競合の実機確認は、MediaGridと接続されるTask 6の実機確認ステップで行う（未接続の状態では実際にはビューワーを開けないため）

- [ ] **Step 8: コミット**

```bash
git add frontend/src/render/MediaViewer.svelte frontend/src/render/MediaViewer.test.ts frontend/src/cropperjs.d.ts
git commit -m "feat: MediaViewerの土台と画像表示(Cropper.js v2)を追加"
```

---

### Task 5: MediaGrid.svelteをMediaViewerに接続し、viewerjsを除去する

この時点で画像はCropper.js v2ベースの新ビューワーに完全移行し、`viewerjs`依存が消える。動画・音声は従来通りグリッド内インライン再生のみ（次のタスクで拡張）。

**Files:**
- Modify: `frontend/src/render/MediaGrid.svelte`

**Interfaces:**
- Consumes: `MediaViewer`（Task 4で作成）

- [ ] **Step 1: `MediaGrid.svelte`からviewerjs関連コードを削除する**

以下を削除する:
- `import Viewer from "viewerjs";`
- `import "viewerjs/dist/viewer.css";`
- `let viewer: Viewer | undefined;`
- `onMount(() => { ... viewer = new Viewer(...) ... })`ブロック全体
- `$effect(() => { void revealed; viewer?.update(); })`ブロック
- `<style>`内の`:global(.viewer-download::before)`、`:global(.viewer-container .viewer-close)`、`:global(.viewer-container .viewer-footer)`（新ビューワーは独自にセーフエリア対応するため不要）

- [ ] **Step 2: MediaViewerを起動する状態とハンドラを追加する**

```ts
import MediaViewer from "./MediaViewer.svelte";

let viewerOpenIndex = $state<number | null>(null);
```

画像の`<img>`要素に付いている`data-original={f.url}`属性を削除し（Viewer.js専用だったため）、代わりに`onclick`でインデックスを開く:

```svelte
<img
  src={f.thumbnailUrl ?? f.url}
  alt={fileName(f)}
  loading="lazy"
  class="h-full w-full cursor-zoom-in object-cover"
  onclick={() => (viewerOpenIndex = files.indexOf(f))}
/>
```

コンポーネント末尾（トップレベルの`{#if files.length > 0}`ブロックの外）に追加:

```svelte
{#if viewerOpenIndex !== null}
  <MediaViewer
    {files}
    startIndex={viewerOpenIndex}
    bind:revealed
    onclose={() => (viewerOpenIndex = null)}
  />
{/if}
```

- [ ] **Step 3: 動画・音声要素から誤って付いていた`cursor-zoom-in`クラスを外す**

現状`<video>`（99行目付近）と`<audio>`（110行目付近）にも`cursor-zoom-in`が付いているが、これはViewer.jsが画像しか自動検出しなかったため実際には何も開かない死んだ装飾だった。動画・音声の拡大ビューワー起動導線はTask 6で追加するので、ここでは一旦`cursor-zoom-in`クラスだけを削除しておく（`class="h-full w-full object-cover"`のようにズームカーソルなしにする）。

- [ ] **Step 4: `pnpm check`で`viewerjs`の未解決import等が解消されたことを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 5: 既存のフロントエンドテストを実行する**

Run: `cd frontend && pnpm vitest run`
Expected: PASS（全テスト）

- [ ] **Step 6: `pnpm remove viewerjs`が実際に完了しており依存として残っていないことを再確認する**

Run: `cd frontend && grep -r "viewerjs" package.json src`
Expected: 出力なし

- [ ] **Step 7: 実機で画像ビューワーの一連の操作を確認する**

3.11節の手順で`cargo tauri dev`を起動し:
1. 画像付きノートで画像をクリック→フルスクリーンビューワーが開く
2. ホイールズーム、（Android実機があれば）ピンチズームで拡大できる
3. **等倍時に横ドラッグ/スワイプすると次/前の画像に切り替わり、ズーム済み（拡大後）は横ドラッグで画像内をパンできる**（3.9節のパン/スワイプ競合解決の実機確認）。もし等倍時にcropper-canvasがドラッグを奪って前後送りできない場合は、この設計の前提が崩れているため、いったん実装を止めてユーザーに報告し、フォールバック方針（3.9節に記載）を相談する
4. 左回転/右回転/左右反転/上下反転ボタンで画像が変形する
5. ダウンロードボタンでファイル保存ダイアログが開く
6. 閲覧注意画像は、まずグリッド内で「クリックで表示」→表示後にクリックでビューワーが開く。ビューワー内で未表示の閲覧注意画像にスワイプで到達した場合はカバーが表示され、クリックで表示されグリッド側にも反映される
7. Escキー・背景クリックで閉じる
8. セーフエリア（閉じるボタン・ツールバーの位置）が画面端で欠けていない

- [ ] **Step 8: コミット**

```bash
git add frontend/src/render/MediaGrid.svelte
git commit -m "feat: 画像の拡大表示をviewerjsからMediaViewerへ移行"
```

---

### Task 6: 動画対応（Vidstack + panzoom）

**Files:**
- Modify: `frontend/src/render/MediaViewer.svelte`

**Interfaces:**
- Consumes: `vidstack/player`, `vidstack/player/layouts`, `vidstack/player/ui`（custom elements登録用の副作用import）、`vidstack/player/styles/default/theme.css`, `vidstack/player/styles/default/layouts/video.css`、`@panzoom/panzoom`

- [ ] **Step 1: Vidstackのスタイル・要素登録をimportする**

`MediaViewer.svelte`の`<script>`冒頭に追加:

```ts
import "vidstack/player/styles/default/theme.css";
import "vidstack/player/styles/default/layouts/video.css";
import "vidstack/player/styles/default/layouts/audio.css";
import "vidstack/player";
import "vidstack/player/layouts";
import "vidstack/player/ui";
```

- [ ] **Step 2: Vidstackのテーマ変数をアプリのトークンにマッピングするCSSを追加する**

`MediaViewer.svelte`の`<style>`ブロック（新規追加）:

```svelte
<style>
  :global(media-player) {
    --video-brand: var(--accent);
    --audio-brand: var(--accent);
    --video-focus-ring-color: var(--accent);
    --audio-focus-ring-color: var(--accent);
    --video-border-radius: 6px; /* rounded-md相当。style-guide.md §2 */
    --audio-border-radius: 6px;
  }
</style>
```

- [ ] **Step 3: 動画・音声アイテムの表示を追加する**

Task 4 Step 3で入れた仮表示ブロック全体（`{:else}` から `{/if}` の直前まで、`（動画・音声プレーヤーは別タスクで対応）`と表示していた箇所）を、以下で置き換える。`viewItems`は`deriveViewItems`により画像/動画/音声にすでに絞られているため、末尾に到達不能な`{:else}`は残さない。

`<script>`に追加:

```ts
const isVideo = (f: DriveFile) => f.mimeType.startsWith("video/");
const isAudio = (f: DriveFile) => f.mimeType.startsWith("audio/");
```

テンプレートの該当箇所（`{:else if isImage(item)} ... {:else} ... {/if}`の`{:else}`以降）を次のように書き換える:

```svelte
        {:else if isImage(item)}
          <cropper-canvas class="h-full w-full" background="false">
            <cropper-image
              bind:this={item.id === current.id ? cropperImageEl : undefined}
              src={item.url}
              alt={fileName(item)}
              rotatable
              scalable
              ontransform={onCropperImageTransform}
              class="h-full w-full"
            ></cropper-image>
          </cropper-canvas>
        {:else if isVideo(item)}
          <div class="flex h-full w-full items-center justify-center p-4" use:videoPanzoom>
            <media-player src={item.url} title={fileName(item)} playsinline crossorigin class="max-h-full max-w-full">
              <media-provider></media-provider>
              <media-video-layout></media-video-layout>
            </media-player>
          </div>
        {:else}
          <div class="w-full max-w-md px-4">
            <media-player src={item.url} title={fileName(item)} crossorigin>
              <media-provider></media-provider>
              <media-audio-layout></media-audio-layout>
            </media-player>
          </div>
        {/if}
```

（`isAudio(item)`は`{:else}`に集約している。`viewItems`は画像/動画/音声のみのため、この`{:else}`に到達するのは常に音声アイテム。）

- [ ] **Step 4: 動画のズーム時のみパン有効化するSvelteアクションを実装する**

`<script>`に追加:

```ts
import Panzoom, { type PanzoomObject } from "@panzoom/panzoom";

function videoPanzoom(node: HTMLElement) {
  const pz: PanzoomObject = Panzoom(node, { maxScale: 4, disablePan: true });
  const onWheel = (e: WheelEvent) => pz.zoomWithWheel(e);
  node.addEventListener("wheel", onWheel);

  function syncDisablePan() {
    pz.setOptions({ disablePan: pz.getScale() <= 1 + 1e-6 });
  }
  node.addEventListener("panzoomzoom", syncDisablePan);

  return {
    destroy() {
      node.removeEventListener("wheel", onWheel);
      node.removeEventListener("panzoomzoom", syncDisablePan);
      pz.destroy();
    },
  };
}
```

- [ ] **Step 5: アイテム切替時にpanzoomの倍率をリセットする**

`goTo`関数内、`imageTransform = initialImageTransform;`の下に動画用のリセットも追加する必要があるが、`videoPanzoom`アクションは要素の再マウントごとに再生成される（Scroll Snapの各ページに個別マウントのため）ため、追加のリセット処理は不要。この前提が崩れていないか、Step 7の実機確認で目視確認する。

- [ ] **Step 6: `pnpm check`を実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし（`media-player`等のcustom elements型は`vidstack/elements`が型定義を提供するため、`frontend/tsconfig.app.json`の`types`に`vidstack/globals`を追加する必要がある場合は追加する。エラーが出た場合はメッセージに従って`frontend/src/vidstack.d.ts`に以下を追加する:）

```ts
/// <reference types="vidstack/globals" />
```

- [ ] **Step 7: 実機で動画の再生・ズーム・パンを確認する**

3.11節の手順で`cargo tauri dev`を起動し、**実際にMisskeyインスタンスへアップロードした動画ファイル**（vidstack.ioのデモファイルではなく）を含むノートで:
1. グリッド内で動画がネイティブ`controls`のまま再生できる（このタスクではまだMediaGrid側に拡大ボタンを追加していないため、ビューワーでの動画確認はTask 8完了後に改めて行う）
2. 型チェック・単体テストが通っていることを本タスクの完了条件とする

- [ ] **Step 8: コミット**

```bash
git add frontend/src/render/MediaViewer.svelte frontend/src/vidstack.d.ts
git commit -m "feat: MediaViewerに動画表示(Vidstack + panzoom)を追加"
```

---

### Task 7: 音声対応の確認

音声はTask 6のStep 3ですでに`media-audio-layout`として実装済みのため、このタスクはズーム/パン非対象であることの確認とテスト追加のみ行う。

**Files:**
- Modify: `frontend/src/render/MediaViewer.test.ts`

- [ ] **Step 1: 音声アイテムがpanzoomなしで表示されることを確認するテストを追加する**

```ts
it("音声ファイルはaudio-layoutで表示され、panzoom用のラッパーが付かない", () => {
  const { container } = render(MediaViewer, {
    props: {
      files: [file({ id: "a", mimeType: "audio/mpeg", name: "a.mp3" })],
      startIndex: 0,
      revealed: {},
      onclose: () => {},
    },
  });
  expect(container.querySelector("media-audio-layout")).toBeTruthy();
});
```

- [ ] **Step 2: テストを実行する**

Run: `cd frontend && pnpm vitest run src/render/MediaViewer.test.ts`
Expected: PASS

- [ ] **Step 3: コミット**

```bash
git add frontend/src/render/MediaViewer.test.ts
git commit -m "test: 音声アイテムがaudio-layoutで表示されることを確認するテストを追加"
```

---

### Task 8: MediaGrid.svelteに動画・音声の拡大ボタンを追加する

**Files:**
- Modify: `frontend/src/render/MediaGrid.svelte`

**Interfaces:**
- Consumes: Task 5で追加した`viewerOpenIndex`状態

- [ ] **Step 1: 動画・音声タイルに拡大ボタンを追加する**

`Maximize2`アイコンをimportに追加:

```ts
import { Maximize2 } from "@lucide/svelte";
```

動画の💾ボタンの隣（99-107行目付近）に追加:

```svelte
<button
  class="absolute top-1.5 right-9 flex size-7 items-center justify-center rounded-full bg-black/50 text-sm leading-none text-white"
  onclick={() => (viewerOpenIndex = files.indexOf(f))}
  aria-label="拡大表示"
>
  <Maximize2 size={14} />
</button>
```

音声の💾ボタン（111-117行目付近）の隣にも同様に追加する。

- [ ] **Step 2: `pnpm check`とテストを実行する**

Run: `cd frontend && pnpm check && pnpm vitest run`
Expected: エラーなし、全テストPASS

- [ ] **Step 3: 実機で動画・音声の拡大ビューワー起動を確認する（Task 6・7の実機確認の本番）**

3.11節の手順で`cargo tauri dev`を起動し、**実際にMisskeyインスタンスへアップロードした動画・音声ファイル**を含むノートで:
1. 動画タイルの拡大ボタンでビューワーが開き、Vidstackの動画レイアウト（再生/一時停止/シーク/音量/再生速度/フルスクリーン）が正しく動作する
2. 動画をホイール/ピンチでズームでき、等倍時はスワイプで前後送り、ズーム時はドラッグでパンできる
3. 音声タイルの拡大ボタンでビューワーが開き、Vidstackの音声レイアウトが正しく動作する
4. Vidstackのテーマ色がアプリの`--accent`色と一致している（デフォルトの青系のままになっていないか目視確認）
5. 前後送りで画像→動画→音声とアイテム種別をまたいでも正しく切り替わる

問題があれば実装を修正し、Step 2からやり直す。

- [ ] **Step 4: コミット**

```bash
git add frontend/src/render/MediaGrid.svelte
git commit -m "feat: 動画・音声タイルに拡大ビューワー起動ボタンを追加"
```

---

### Task 9: エラーハンドリングの追加

**Files:**
- Modify: `frontend/src/render/MediaViewer.svelte`

- [ ] **Step 1: 画像読み込み失敗時のインラインエラー表示を追加する**

`<script>`に追加（`mediaLoadError`はこの後のStep 2で動画・音声にも共用する、ロード失敗フラグの汎用Record）:

```ts
let mediaLoadError = $state<Record<string, boolean>>({});
```

`<cropper-image>`タグに追加:

```svelte
onerror={() => (mediaLoadError = { ...mediaLoadError, [item.id]: true })}
```

画像表示の`{:else if isImage(item)}`ブロックの先頭に、エラー時の分岐を追加する:

```svelte
{:else if isImage(item)}
  {#if mediaLoadError[item.id]}
    <p class="text-sm text-white">画像を読み込めませんでした</p>
  {:else}
    <cropper-canvas class="h-full w-full" background="false">
      ...(既存のcropper-image)
    </cropper-canvas>
  {/if}
```

- [ ] **Step 2: Vidstackのエラーイベントでも同様のエラー表示を出す**

`<media-player>`タグに追加:

```svelte
onerror={() => (mediaLoadError = { ...mediaLoadError, [item.id]: true })}
```

動画の分岐（`{:else if isVideo(item)}`）と音声の分岐（末尾の`{:else}`）双方の`<media-player>`をそれぞれ次のように`{#if mediaLoadError[item.id]} ... {:else} ... {/if}`で囲む:

```svelte
{:else if isVideo(item)}
  {#if mediaLoadError[item.id]}
    <p class="text-sm text-white">動画を読み込めませんでした</p>
  {:else}
    <div class="flex h-full w-full items-center justify-center p-4" use:videoPanzoom>
      ...(既存のmedia-player、onerror属性を追加)
    </div>
  {/if}
{:else}
  {#if mediaLoadError[item.id]}
    <p class="text-sm text-white">音声を読み込めませんでした</p>
  {:else}
    <div class="w-full max-w-md px-4">
      ...(既存のmedia-player、onerror属性を追加)
    </div>
  {/if}
{/if}
```

- [ ] **Step 3: `pnpm check`とテストを実行する**

Run: `cd frontend && pnpm check && pnpm vitest run`
Expected: エラーなし、全テストPASS

- [ ] **Step 4: コミット**

```bash
git add frontend/src/render/MediaViewer.svelte
git commit -m "feat: MediaViewerに読み込み失敗時のエラー表示を追加"
```

---

### Task 11: グリッド内インライン再生をVidstack自前コントロールバー化（実機フィードバックによる追加タスク）

**背景**: Task 8完了後、ユーザーが実機で動画・音声タイルを確認したところ、ネイティブ`<video controls>`/`<audio controls>`が独自に描画する右上のミュートアイコン等と、Tsumugi側の拡大・保存ボタンが同じ角に重なって衝突することが判明した。Vidstackのデフォルトレイアウト（`<media-video-layout>`/`<media-audio-layout>`）へのカスタムボタン追加はWeb Componentsでは現状公式未対応（[vidstack/player#1136](https://github.com/vidstack/player/discussions/1136)、Reactのみ対応）と判明したため、デフォルトレイアウトは使わず、Vidstackの個別プリミティブ部品（`<media-play-button>`, `<media-mute-button>`, `<media-time-slider>`等）を組み合わせて、グリッドサムネイル用の自前の最小コントロールバーを構築し、そこに拡大・保存ボタンも同じ行に並べる。

**Files:**
- Modify: `frontend/src/render/MediaGrid.svelte`

**Interfaces:**
- Consumes: `vidstack/player`, `vidstack/player/ui`（Task 6でMediaViewer.svelte向けに副作用importしたものと同じcustom elements定義。MediaGrid.svelte側でも同様のimportが必要）

- [ ] **Step 1: Vidstackの個別ボタン部品のAPIを調査する**

以下を実施すること:
1. `vidstack@1.15.6`のexportsに含まれる`<media-play-button>`, `<media-mute-button>`, `<media-time-slider>`, `<media-controls>`, `<media-controls-group>`の実際の使い方を、`node_modules/vidstack`内の型定義（`.d.ts`）やGitHubの`vidstack/examples`リポジトリ（`player/svelte/tailwind-css/src/components/buttons/PlayButton.svelte`, `MuteButton.svelte`等）を参照して確認する。
2. 再生/一時停止・ミュート状態に応じたアイコンの出し分けが、Vidstack独自のCSS属性セレクタ（例: `[data-paused]`, `[data-muted]`等、Vidstack Tailwindプラグインを使わない場合の素のCSS属性セレクタ）でどう実現できるか調査する。Tailwindプラグイン（`vidstack/tailwind.cjs`）の導入は、tsumugiがTailwind v4のゼロコンフィグ構成（`app.css`の`@theme`、`tailwind.config.js`なし）であるため、既存構成との整合性を優先し、**素のCSS属性セレクタで実現できないか先に検討すること**。どうしても必要な場合のみプラグイン導入を検討し、その判断根拠をレポートに残すこと。
3. アイコンは`@lucide/svelte`の`Play`, `Pause`, `Volume2`, `VolumeX`を使い、Vidstack独自の`<media-icon>`（`vidstack/icons`の追加importが必要になる）は使わない方針とする（既存のMaximize2/Downloadアイコンとの統一のため）。

- [ ] **Step 2: 動画タイルを自前コントロールバー付きVidstackプレイヤーに置き換える**

現在の`<video src={f.url} controls preload="metadata" class="h-full w-full object-cover"></video>`を、`<media-player>` + `<media-provider>` + 自前の`<media-controls>`（再生/一時停止ボタン・ミュートボタン・拡大ボタン・保存ボタンを1行に並べる、時間表示やシークバーは最小限またはthumbnailの高さに収まる場合のみ）に置き換える。既存の拡大ボタン（`Maximize2`）・保存ボタン（`Download`）はこの新しいコントロールバー内に統合し、独立した`absolute`配置のボタンとしては配置しない（ネイティブコントロールとの衝突問題自体を構造的に解消するため）。

- [ ] **Step 3: 音声タイルも同様に自前コントロールバー付きVidstackプレイヤーに置き換える**

現在の`<audio src={f.url} controls preload="metadata" class="w-[calc(100%-16px)]"></audio>`を同様に置き換える。

- [ ] **Step 4: 動作確認**

Run: `cd frontend && pnpm check && pnpm vitest run`（終了コードも確認）
Expected: エラーなし、全テストPASS、終了コード0

既存の`MediaGrid.svelte`にテストファイルがあれば実行して問題ないことを確認する（Playタスク時点でテストファイルの有無を確認すること）。

- [ ] **Step 5: 実機確認は不要**（ユーザーが自分の環境で確認する）

- [ ] **Step 6: コミット**

コミットメッセージは件名のみ（本文・箇条書きなし）。

---

### Task 10: 最終確認とクリーンアップ

**Files:** なし（確認のみ）

- [ ] **Step 1: 依存関係にviewerjsが一切残っていないことを最終確認する**

Run: `cd frontend && grep -rn "viewerjs" package.json pnpm-lock.yaml src`
Expected: 出力なし

- [ ] **Step 2: フルテストスイートと型チェックを実行する**

Run: `cd frontend && pnpm check && pnpm vitest run`
Expected: 全PASS、エラーなし

- [ ] **Step 3: 実機で一連のシナリオを通しで確認する**

3.11節の手順で`cargo tauri dev`を起動し、画像・動画・音声が混在するノートで、クリック/拡大ボタンでビューワーを開き、前後送り・ズーム/パン・回転/反転（画像のみ）・ダウンロード・閲覧注意ゲーティング・Escキー/背景クリックでの終了・セーフエリア表示を一通り確認する。可能であればAndroid実機（`cargo tauri android build --debug --target aarch64`でビルドしたAPK）でもピンチズーム・スワイプの操作性を確認する。

確認後、自分で起動した`cargo tauri dev`のプロセスを正確なPIDで終了する（`pkill`/`killall`は使わない）。

- [ ] **Step 4: 設計ドキュメントの「検討した代替案」節が実装結果と矛盾していないか読み直す**

`docs/superpowers/specs/2026-09-19-media-viewer-migration-design.md`を読み、Cropper.js v2のピンチズーム検証結果・パン/スワイプ競合解決の実際の挙動が、ドキュメント記載の想定と異なっていた場合は追記する。

- [ ] **Step 5: コミット（ドキュメント更新があった場合のみ）**

```bash
git add docs/superpowers/specs/2026-09-19-media-viewer-migration-design.md
git commit -m "docs: 実装結果に基づき設計ドキュメントを更新"
```
