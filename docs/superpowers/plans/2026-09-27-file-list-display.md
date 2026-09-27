# 非メディア添付ファイルのリスト表示 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 非メディア添付ファイルを、グリッドではなく `[DLアイコン] ファイル名 サイズ` のリスト行で表示し、クリックで保存ダイアログを開く (Issue #387)。

**Architecture:** Rust の `DriveFile` に `size` を追加してフロントへ渡す。フロントでは整形関数 `formatFileSize` と新規 `FileList.svelte` を作り、`MediaGrid.svelte` でメディアと非メディアを分けて描画する。保存は既存の `saveMediaToDisk` を使い、`openUrl` は使わない。

**Tech Stack:** Rust (serde, specta), Svelte 5, Vitest, @testing-library/svelte, @lucide/svelte

## Global Constraints

- 設計 spec: `docs/superpowers/specs/2026-09-27-file-list-display-design.md`
- 作業ブランチ: `feat/387-file-list-display` (main へ直接コミットしない)
- コミットメッセージは件名のみ (本文・箇条書きなし)。末尾に `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` を付ける
- `frontend/src/bindings/tauri.gen.ts` は手編集しない。`cd src-tauri && cargo test` で再生成する
- 見た目の値は `docs/design/style-guide.md` のスケールに従い、`rounded-[Npx]` のような一回限りの値を使わない
- `openUrl` は非メディアファイルに使わない
- 実 UI の確認は Xvfb 越し + `WAYLAND_DISPLAY` unset、`dbus-run-session` 必須。起動したプロセスはタスク完了前に正確な PID で kill する (`pkill`/`killall` 禁止)

---

### Task 1: Rust — `DriveFile.size` を追加する

**Files:**
- Modify: `src-tauri/src/domain/note.rs:112-122`
- Modify: `src-tauri/src/api/normalize.rs:162-176` (RawFile), `:264-274` (From)
- Modify (構造体リテラルに `size: None` を追加): `src-tauri/src/store/sqlite_backend.rs:322`, `src-tauri/src/store/mysql_backend.rs:1113`, `src-tauri/src/store/postgres_backend.rs:1076`, `src-tauri/src/filter/mod.rs:98`, `src-tauri/src/store/note_cache.rs:621`, `src-tauri/src/filter/eval.rs:303`
- Test: `src-tauri/src/api/normalize.rs` 内の既存 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `DriveFile.size: Option<u64>` (TS では `size?: number | null`)

- [ ] **Step 1: 失敗するテストを書く**

`normalize.rs` の tests モジュールに追加する。

```rust
#[test]
fn raw_file_size_is_mapped() {
    let raw: RawFile = serde_json::from_str(
        r#"{"id":"f1","type":"application/pdf","url":"http://x/f1","name":"a.pdf","size":1234}"#,
    )
    .unwrap();
    let f: DriveFile = raw.into();
    assert_eq!(f.size, Some(1234));

    let raw: RawFile = serde_json::from_str(r#"{"id":"f2"}"#).unwrap();
    let f: DriveFile = raw.into();
    assert_eq!(f.size, None);
}
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test raw_file_size_is_mapped`
Expected: コンパイルエラー (`size` フィールドが無い)

- [ ] **Step 3: 実装する**

`domain/note.rs` の `DriveFile` の末尾に追加:

```rust
    /// バイト数。古いキャッシュや Misskey が返さない場合は None
    #[serde(default)]
    pub size: Option<u64>,
```

`RawFile` の末尾に追加:

```rust
    #[serde(default)]
    pub size: Option<u64>,
```

`From<RawFile> for DriveFile` に `size: f.size,` を追加する。前述の 6 箇所のテスト用リテラルには `size: None` を追加する。

- [ ] **Step 4: テストとバインディング再生成を確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS。`generates_frontend_bindings` により `tauri.gen.ts` の `DriveFile` に `size?: number | null` が現れる。`u64` の specta エクスポートでエラーが出る場合は、型を `Option<i64>` に変えて (`created_at: i64` が number で出ている実績がある) 再実行する。

Run: `git diff frontend/src/bindings/tauri.gen.ts`
Expected: `DriveFile` への `size` 追加のみ

- [ ] **Step 5: コミット**

```bash
git add src-tauri frontend/src/bindings/tauri.gen.ts
git commit -m "feat: DriveFileにファイルサイズを追加する"
```

---

### Task 2: フロント — `formatFileSize` を追加する

**Files:**
- Create: `frontend/src/lib/fileSize.ts`
- Test: `frontend/src/lib/fileSize.test.ts`

**Interfaces:**
- Produces: `formatFileSize(bytes: number | null | undefined): string` — 未指定・負数・NaN は `""`、`1024` 未満は `"N B"`、以降は 1024 進で `KB`/`MB`/`GB` を小数第 1 位 (`.0` は省略しない: `"1.0 KB"`)

- [ ] **Step 1: 失敗するテストを書く**

```ts
import { describe, expect, it } from "vitest";
import { formatFileSize } from "./fileSize";

describe("formatFileSize", () => {
  it("未指定・不正値は空文字", () => {
    expect(formatFileSize(undefined)).toBe("");
    expect(formatFileSize(null)).toBe("");
    expect(formatFileSize(-1)).toBe("");
    expect(formatFileSize(NaN)).toBe("");
  });
  it("1024未満はB", () => {
    expect(formatFileSize(0)).toBe("0 B");
    expect(formatFileSize(1023)).toBe("1023 B");
  });
  it("1024進でKB/MB/GB", () => {
    expect(formatFileSize(1024)).toBe("1.0 KB");
    expect(formatFileSize(1536)).toBe("1.5 KB");
    expect(formatFileSize(1024 * 1024 * 1.2)).toBe("1.2 MB");
    expect(formatFileSize(1024 ** 3 * 2)).toBe("2.0 GB");
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm test fileSize`
Expected: FAIL (モジュールが無い)

- [ ] **Step 3: 実装する**

```ts
const UNITS = ["B", "KB", "MB", "GB"] as const;

/// バイト数を "1.2 MB" 形式に整形する。サイズ不明・不正値は空文字（呼び出し側で非表示にする）。
export function formatFileSize(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(1)} ${UNITS[unit]}`;
}
```

- [ ] **Step 4: 成功を確認する**

Run: `cd frontend && pnpm test fileSize`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/fileSize.ts frontend/src/lib/fileSize.test.ts
git commit -m "feat: ファイルサイズ整形関数を追加する"
```

---

### Task 3: フロント — `FileList.svelte` と `MediaGrid` の分離

**Files:**
- Create: `frontend/src/render/FileList.svelte`
- Modify: `frontend/src/render/MediaGrid.svelte` (`{:else}` の 📄 ボタン分岐を削除、`openUrl` import 削除、非メディアを分けて描画)
- Modify: `frontend/src/lib/mediaViewer.svelte.ts:8-12` (コメントの「openUrlする」記述を更新)
- Test: `frontend/src/render/FileList.test.ts`

**Interfaces:**
- Consumes: `formatFileSize` (Task 2), `saveMediaToDisk(url, suggestedName, onError)` from `../lib/mediaDownload`, `fileName`/`isImage`/`isVideo`/`isAudio`/`isRevealed`/`reveal` from `../lib/mediaViewer.svelte`, `app.reportError` from `../lib/store.svelte`
- Produces: `FileList.svelte` — props `{ files: DriveFile[], revealed: Record<string, boolean>, onreveal: (f: DriveFile) => void }`

- [ ] **Step 1: 失敗するテストを書く**

`MediaViewer.test.ts` と同じモック構成を使う。

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { DriveFile } from "../bindings/tauri.gen";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("../lib/mediaDownload", () => ({ saveMediaToDisk: vi.fn() }));

const { default: FileList } = await import("./FileList.svelte");
const { saveMediaToDisk } = await import("../lib/mediaDownload");
const { openUrl } = await import("@tauri-apps/plugin-opener");

function file(overrides: Partial<DriveFile>): DriveFile {
  return {
    id: "f1",
    mimeType: "application/pdf",
    isSensitive: false,
    url: "https://example.com/f1",
    thumbnailUrl: null,
    name: "report.pdf",
    size: 1536,
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("FileList", () => {
  it("ファイル名とサイズを表示する", () => {
    const { getByText } = render(FileList, { files: [file({})], revealed: {}, onreveal: vi.fn() });
    getByText("report.pdf");
    getByText("1.5 KB");
  });

  it("sizeが無ければサイズを表示しない", () => {
    const { queryByText } = render(FileList, {
      files: [file({ size: undefined })],
      revealed: {},
      onreveal: vi.fn(),
    });
    expect(queryByText(/KB|MB|GB/)).toBeNull();
  });

  it("クリックでsaveMediaToDiskを呼び、openUrlは呼ばない", async () => {
    const { getByRole } = render(FileList, { files: [file({})], revealed: {}, onreveal: vi.fn() });
    await fireEvent.click(getByRole("button", { name: /report\.pdf/ }));
    expect(saveMediaToDisk).toHaveBeenCalledWith("https://example.com/f1", "report.pdf", expect.any(Function));
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("閲覧注意ファイルは開示するまでファイル名を出さず、クリックでonrevealを呼ぶ", async () => {
    const onreveal = vi.fn();
    const f = file({ isSensitive: true });
    const { getByText, queryByText } = render(FileList, { files: [f], revealed: {}, onreveal });
    expect(queryByText("report.pdf")).toBeNull();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    expect(onreveal).toHaveBeenCalledWith(f);
    expect(saveMediaToDisk).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm test FileList`
Expected: FAIL (コンポーネントが無い)

- [ ] **Step 3: `FileList.svelte` を実装する**

`docs/design/style-guide.md` を読み、角丸・フォントサイズ・アイコンサイズの既定スケールに合わせて class を決める (下記は `MediaGrid` / `MediaControlBar` の既存値に揃えた初期案。style-guide と食い違う場合は style-guide を優先)。

```svelte
<script lang="ts">
  import { Download } from "@lucide/svelte";
  import type { DriveFile } from "../bindings/tauri.gen";
  import { formatFileSize } from "../lib/fileSize";
  import { saveMediaToDisk } from "../lib/mediaDownload";
  import { fileName, isRevealed } from "../lib/mediaViewer.svelte";
  import { app } from "../lib/store.svelte";

  let {
    files,
    revealed,
    onreveal,
  }: {
    files: DriveFile[];
    revealed: Record<string, boolean>;
    onreveal: (f: DriveFile) => void;
  } = $props();
</script>

{#if files.length > 0}
  <ul class="mt-2 flex flex-col gap-1">
    {#each files as f (f.id)}
      <li>
        {#if !isRevealed(revealed, f)}
          <button
            class="file-row w-full rounded-md border-0 px-2 py-1.5 text-left text-sm text-muted-foreground"
            onclick={() => onreveal(f)}
          >
            閲覧注意（クリックで表示）
          </button>
        {:else}
          <button
            class="file-row flex w-full items-center gap-2 rounded-md border-0 px-2 py-1.5 text-left text-sm"
            onclick={() => saveMediaToDisk(f.url, fileName(f), (e) => app.reportError(e))}
            aria-label={`${fileName(f)} を保存`}
          >
            <Download size={16} class="flex-none text-primary" />
            <span class="min-w-0 flex-1 truncate">{fileName(f)}</span>
            {#if formatFileSize(f.size)}
              <span class="flex-none text-xs text-muted-foreground">{formatFileSize(f.size)}</span>
            {/if}
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .file-row {
    background: color-mix(in srgb, var(--surface-2) var(--column-opacity, 100%), transparent);
  }
</style>
```

注意: テストの `getByRole("button", { name: /report\.pdf/ })` は `aria-label` にファイル名が含まれるため一致する。`getByText("report.pdf")` は `<span>` に一致する。

- [ ] **Step 4: `MediaGrid.svelte` を修正する**

1. `import { openUrl } from "@tauri-apps/plugin-opener";` を削除する。
2. `import FileList from "./FileList.svelte";` を追加し、`mediaViewer.svelte` の import から使わなくなったものを整理する (`isImage`/`isVideo`/`isAudio` は残る)。
3. `<script>` に以下を追加する:

```ts
  const isMedia = (f: DriveFile) => isImage(f) || isVideo(f) || isAudio(f);
  const mediaFiles = $derived(files.filter(isMedia));
  const otherFiles = $derived(files.filter((f) => !isMedia(f)));
```

4. グリッドの `class` 判定と `{#each files ...}` を `mediaFiles` に置き換え (`mediaFiles.length === 1` で `grid-cols-1`)、グリッド全体を `{#if mediaFiles.length > 0}` で囲む。
5. `{:else}` の `📄` ボタン分岐 (`{:else} <button ... openUrl(f.url)...>📄 ...</button>`) を丸ごと削除する。`{:else if isAudio(f)}` が最後の分岐になる。
6. グリッドの直後に追加する:

```svelte
<FileList files={otherFiles} {revealed} onreveal={(f) => (revealed = reveal(revealed, f))} />
```

7. 外側の `{#if files.length > 0}` は、上記の分離後は不要なので削除してよい (各部分が自前で空判定する)。

- [ ] **Step 5: `mediaViewer.svelte.ts` のコメントを更新する**

`deriveViewItems` の上のコメントを「その他のファイル(非メディア)は MediaGrid 側で FileList に分離して表示するためここでは除外する。」に書き換える。

- [ ] **Step 6: 検証する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: すべて PASS (既存の `MediaViewer`/`NoteCard` テストが壊れていないこと)

- [ ] **Step 7: コミット**

```bash
git add frontend/src
git commit -m "feat: 非メディア添付ファイルをリスト表示して保存ダイアログで開く"
```

---

### Task 4: 実 UI 確認とドキュメント

**Files:**
- Modify (該当記述があれば): `docs/guide/user-guide.md`

- [ ] **Step 1: user-guide の添付ファイル記述を確認する**

Run: `grep -n "添付\|📄\|ダウンロード" docs/guide/user-guide.md`
該当箇所が「グリッド表示」「ブラウザで開く」と読める場合は、リスト表示・保存ダイアログに直す。無ければ変更しない。

- [ ] **Step 2: 実 UI で確認する**

Xvfb 越しに `dbus-run-session` 配下で `cargo tauri dev` をリポジトリルートから起動する (Global Constraints 参照)。非メディア添付 (PDF など) を持つノートで次を確認する: リスト行の見た目、DL アイコン、ファイル名、サイズ、クリックで保存ダイアログが出て外部ブラウザが開かないこと、閲覧注意ファイルの開示。確認後、起動したプロセスを正確な PID で kill する。実 Misskey に該当ノートが無い場合は、その旨をユーザーに報告する (確認済みとは書かない)。

- [ ] **Step 3: ドキュメントを変更した場合のみコミット**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: 添付ファイルのリスト表示をユーザーガイドに反映する"
```
