# 低帯域幅モード Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定 → 外観のトグル 1 つで、ノート添付・アバター・カスタム絵文字・URL プレビューの自動取得を止め、必要なものだけタップで読み込めるようにする(Issue #591)。

**Architecture:** Rust は `UiPrefs.low_bandwidth: bool` を 1 つ足すだけ(設定のエクスポート・インポートでは端末依存として扱う)。フロントは各コンポーネントが `app.ui.lowBandwidth` を直接読み、取得を保留する。メディアは既存の閲覧注意カバー(`isRevealed` / `reveal`)を拡張して再利用し、アバターは `Avatar` 枠にゲートを置く。

**Tech Stack:** Rust (serde / specta / tauri-specta), Svelte 5 (runes), Vitest + @testing-library/svelte, vidstack。

**Spec:** `docs/superpowers/specs/2026-11-18-low-bandwidth-mode-design.md`

## Global Constraints

- ブランチは `feat/issue-591-low-bandwidth-mode`(作成済み)。`main` では編集もコミットもしない。
- コミットメッセージは **件名 1 行のみ**(本文・箇条書き禁止)。形式は Conventional Commits(`type(scope): 件名`)。末尾の `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は別の `-m` で付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告する(再試行しない)。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集せず、`cd src-tauri && cargo test generates_frontend_bindings` で再生成する。
- Rust のビルドは重い(初回 15〜30 分)。`CARGO_TARGET_DIR` を `/tmp`(tmpfs)に置かない。メモリが厳しければ `CARGO_BUILD_JOBS=4` と `systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G` を付ける。`cargo run` / `./target/debug/tsumugi` は実行しない。
- Vitest は 1 ファイルずつ `cd frontend && npx vitest run <path>` で回す(`pnpm test -- <path>` はパスを無視して全件を回す)。
- 見た目の値は `docs/design/style-guide.md` の尺度を使い、`rounded-[Npx]` のような一回限りの値を作らない。
- シェルは zsh。`--include=*.svelte` のようなグロブはクォートする(`--include='*.svelte'`)。
- 検証用に自分で起動したプロセス(Xvfb・`cargo tauri dev`・e2e の Docker など)は、完了前に自分で止める。プロセスの停止は正確な PID だけで行い、`pkill` / `killall` は使わない。コンテナは `docker rm -fv <id>`(`-v` 必須)。
- 実 UI の確認は Xvfb 越しに行い、`WAYLAND_DISPLAY` も unset する(ユーザーの実画面に出さない)。

## Review Focus

1. **閲覧注意かつ低帯域幅の添付**: タップ 1 回で両方が解除され、2 回タップさせない(Task 3 の `MediaGrid` テスト)。
2. **拡大ビューア**: 1 枚開いても、同じノートの未読み込みの添付は実画像を取得しない(Task 3 の `MediaViewer` テスト)。
3. **`size` / `name` が無い添付**: プレースホルダーが崩れず、ファイル名だけ(名前も無ければ MIME タイプ)を出す(Task 3 の `MediaGrid` テスト)。
4. **キャッシュ済みの URL プレビュー**: キャッシュがあっても、押すまでカード(サムネイル)を描画しない(Task 5 のテスト)。
5. **ファイルで勝手にオンにならない**: `lowBandwidth: true` のエクスポートファイルを取り込んでも現在の値が変わらない(Task 1 の Rust テスト)。

---

### Task 1: `UiPrefs.low_bandwidth` と設定のエクスポート・インポート

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`(`UiPrefs` 構造体・`Default`・テスト)
- Modify: `src-tauri/src/store/settings_transfer.rs`(`build_export`・`plan_import`・テスト)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Produces: Rust `UiPrefs.low_bandwidth: bool`(JSON は `lowBandwidth`)。TS では `UiPrefs.lowBandwidth?: boolean`(`#[serde(default)]` の項目は specta がオプショナルで出す。既存の `translateEngine?: string` と同じ)。以降のタスクは `app.ui.lowBandwidth` を真偽として読み、`undefined` は偽として扱う。

- [ ] **Step 1: 失敗するテストを書く(`ui.rs`)**

`src-tauri/src/domain/ui.rs` の `mod tests` に、`deserializes_legacy_json_without_new_fields` の直後へ追加する。

```rust
    #[test]
    fn low_bandwidth_defaults_to_false_and_reads_true() {
        // 古い JSON(フィールドなし)も既定の false で読めること(Issue #591)。
        let legacy: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert!(!legacy.low_bandwidth);
        assert!(!UiPrefs::default().low_bandwidth);

        let on: UiPrefs = serde_json::from_str(
            r#"{"theme":"dark","defaultColumnWidth":320,"lowBandwidth":true}"#,
        )
        .unwrap();
        assert!(on.low_bandwidth);
    }
```

- [ ] **Step 2: 失敗するテストを書く(`settings_transfer.rs`)**

`mod tests` の `import_keeps_the_current_mcp_settings` の直後に追加する。Edit の `old_string` は次の 2 行。

```rust
        assert_eq!(plan.data.mcp, current.mcp, "ファイルでMCPの公開が変わってはいけない");
    }
```

`new_string` は、上の 2 行に続けて次を足したもの。

```rust

    #[test]
    fn export_does_not_include_low_bandwidth() {
        let mut d = settings_data();
        d.ui.low_bandwidth = true;

        let file = build_export(&d, "1.2.3", "2026-11-02T00:00:00Z");

        assert!(!file.data.ui.low_bandwidth, "端末依存の設定はエクスポートに含めない");
    }

    #[test]
    fn import_keeps_the_current_low_bandwidth() {
        // 現在オンなら、ファイルがオフでもオンのまま。
        let mut current = current_with_local_account();
        current.ui.low_bandwidth = true;
        let plan = plan_import(&current, file_with(empty_data())).unwrap();
        assert!(plan.data.ui.low_bandwidth);

        // 現在オフなら、ファイルがオンでもオフのまま(ファイルで勝手にオンにならない)。
        let mut current = current_with_local_account();
        current.ui.low_bandwidth = false;
        let mut data = empty_data();
        data.ui.low_bandwidth = true;
        let plan = plan_import(&current, file_with(data)).unwrap();
        assert!(!plan.data.ui.low_bandwidth);
    }
```

- [ ] **Step 3: テストが失敗(コンパイルエラー)することを確認する**

Run: `cd src-tauri && cargo test --lib low_bandwidth`
Expected: FAIL(`no field low_bandwidth on type UiPrefs`)。初回は長時間かかる。

- [ ] **Step 4: 最小の実装を書く**

`src-tauri/src/domain/ui.rs`。`translate_engine` フィールドの直後に追加する。Edit の `old_string`:

```rust
    #[serde(default = "default_translate_engine")]
    pub translate_engine: String,
```

`new_string`:

```rust
    #[serde(default = "default_translate_engine")]
    pub translate_engine: String,
    /// 低帯域幅モード(Issue #591)。オンの間、ノート添付・アバター・カスタム絵文字・URL プレビューを
    /// 自動では取得しない。端末依存のため、設定のエクスポートには含めず、インポートでも現在の値を維持する
    /// (`store/settings_transfer.rs`)。
    #[serde(default)]
    pub low_bandwidth: bool,
```

同じファイルの `Default` 実装。Edit の `old_string`: `            translate_engine: default_translate_engine(),` を次に置き換える。

```rust
            translate_engine: default_translate_engine(),
            low_bandwidth: false,
```

`src-tauri/src/store/settings_transfer.rs`。`build_export` の `ui.background_path = None;` を次に置き換える(`let mut ui = data.ui.clone();` の直後の 2 行のうち 2 行目)。

```rust
    ui.background_path = None;
    ui.low_bandwidth = false;
```

`plan_import` の `ui.gap_fill_limit = ui.gap_fill_limit.clamp(0, 1_000);` を次に置き換える。

```rust
    ui.gap_fill_limit = ui.gap_fill_limit.clamp(0, 1_000);
    // 低帯域幅モードは端末依存。ファイルで勝手にオンにならないよう、現在の値を維持する。
    ui.low_bandwidth = current.ui.low_bandwidth;
```

- [ ] **Step 5: テストが通ることを確認し、バインディングを再生成する**

Run: `cd src-tauri && cargo test --lib low_bandwidth`
Expected: PASS(3 件: `low_bandwidth_defaults_to_false_and_reads_true` / `export_does_not_include_low_bandwidth` / `import_keeps_the_current_low_bandwidth`)

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`git diff frontend/src/bindings/tauri.gen.ts` に `lowBandwidth?: boolean` の追加だけが出る。

Run: `cd src-tauri && cargo test --lib settings_transfer && cargo test --lib domain::ui`
Expected: PASS(既存テストが壊れていない)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/domain/ui.rs src-tauri/src/store/settings_transfer.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat(settings): UiPrefsに低帯域幅モードを追加し端末依存として扱う(Issue #591)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 設定 → 外観のトグル

**Files:**
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`
- Create: `frontend/src/ui/settings/AppearanceSection.test.ts`

**Interfaces:**
- Consumes: `app.ui.lowBandwidth?: boolean`、`app.setUiPrefs(prefs: UiPrefs)`(Task 1)。
- Produces: 設定画面のチェックボックス(ラベル「低帯域幅モードを有効にする」)。保存すると `app.ui.lowBandwidth` が更新される。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/settings/AppearanceSection.test.ts` を新規作成する。

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { app } from "../../lib/store.svelte";

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

const { default: AppearanceSection } = await import("./AppearanceSection.svelte");

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  app.ui = { ...app.ui, lowBandwidth: false };
});

describe("AppearanceSection 低帯域幅モード", () => {
  it("既定ではチェックが外れている", () => {
    app.ui = { ...app.ui, lowBandwidth: undefined };
    const { getByLabelText } = render(AppearanceSection);
    expect((getByLabelText("低帯域幅モードを有効にする") as HTMLInputElement).checked).toBe(false);
  });

  it("保存済みの値が反映される", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByLabelText } = render(AppearanceSection);
    expect((getByLabelText("低帯域幅モードを有効にする") as HTMLInputElement).checked).toBe(true);
  });

  it("チェックして保存すると lowBandwidth だけが変わり、他の項目は保持される", async () => {
    app.ui = { ...app.ui, lowBandwidth: false, fontFamily: "Meiryo, sans-serif" };
    const spy = vi.spyOn(app, "setUiPrefs").mockResolvedValue(undefined);
    const { getByLabelText, getByText } = render(AppearanceSection);

    await fireEvent.click(getByLabelText("低帯域幅モードを有効にする"));
    getByText("保存").click();

    await vi.waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    const saved = spy.mock.calls[0][0];
    expect(saved.lowBandwidth).toBe(true);
    expect(saved.fontFamily).toBe("Meiryo, sans-serif");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/settings/AppearanceSection.test.ts`
Expected: FAIL(`Unable to find a label with the text of: 低帯域幅モードを有効にする`)

- [ ] **Step 3: 最小の実装を書く**

`frontend/src/ui/settings/AppearanceSection.svelte`。状態を追加する。Edit の `old_string`:

```ts
  let mfmAnimationEnabled = $state(app.ui.mfmAnimationEnabled ?? true);
```

`new_string`:

```ts
  let mfmAnimationEnabled = $state(app.ui.mfmAnimationEnabled ?? true);
  let lowBandwidth = $state(app.ui.lowBandwidth ?? false);
```

`save()` の `app.setUiPrefs({...})` に追加する。`old_string`:

```ts
        mfmAnimationEnabled,
        instanceTicker,
```

`new_string`:

```ts
        mfmAnimationEnabled,
        lowBandwidth,
        instanceTicker,
```

チェックボックスを MFM アニメーションの項目の直前に置く。`old_string`(1 行目だけで一意):

```svelte
<label class="mb-2 flex items-center gap-2 text-sm"
  ><input type="checkbox" bind:checked={mfmAnimationEnabled} /> MFMアニメーション($[shake]等)を有効にする</label
>
```

`new_string`(同じブロックの前に低帯域幅モードを足す):

```svelte
<label class="mb-2 flex items-center gap-2 text-sm"
  ><input type="checkbox" bind:checked={lowBandwidth} /> 低帯域幅モードを有効にする</label
>
<p class="mb-4 mt-0 text-xs text-muted-foreground">
  回線が遅い・従量制のときに通信量を減らします。オンの間は、ノートの添付(画像・動画・音声)、アバター、
  カスタム絵文字、URLプレビューを自動では取得しません。添付とURLプレビューはタップしたときだけ読み込み、
  アバターは表示せず、カスタム絵文字は <code>:name:</code> の文字で表示します(プロフィール・フォロー一覧・
  アカウント選択のアバターは通常どおり表示します)。端末ごとの設定で、設定のエクスポートには含まれず、
  インポートしても変わりません。保存すると反映されます。
</p>

<label class="mb-2 flex items-center gap-2 text-sm"
  ><input type="checkbox" bind:checked={mfmAnimationEnabled} /> MFMアニメーション($[shake]等)を有効にする</label
>
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/settings/AppearanceSection.test.ts`
Expected: PASS(3 件)

Run: `cd frontend && pnpm check`
Expected: エラー 0 件(`lowBandwidth` が `UiPrefs` に存在するため型が通る)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/AppearanceSection.svelte frontend/src/ui/settings/AppearanceSection.test.ts
git commit -m "feat(settings): 外観に低帯域幅モードのトグルを追加(Issue #591)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ノート添付(グリッドと拡大ビューア)

**Files:**
- Modify: `frontend/src/lib/mediaViewer.svelte.ts`(`isRevealed` 拡張・`coverLabel` 追加)
- Modify: `frontend/src/lib/mediaViewer.svelte.test.ts`
- Modify: `frontend/src/render/MediaGrid.svelte`
- Modify: `frontend/src/render/MediaGrid.test.ts`
- Modify: `frontend/src/render/MediaViewer.svelte`
- Modify: `frontend/src/render/MediaViewer.test.ts`

**Interfaces:**
- Consumes: `app.ui.lowBandwidth?: boolean`、`formatFileSize(bytes)`(`lib/fileSize.ts`)、`fileName(f)`(`lib/mediaViewer.svelte.ts`)。
- Produces:
  - `isRevealed(revealed: Record<string, boolean>, file: DriveFile, lowBandwidth?: boolean): boolean`(`lowBandwidth` の既定は `false`。既存の呼び出しは変わらない)
  - `coverLabel(file: DriveFile): string`(閲覧注意なら `"閲覧注意（クリックで表示）"`、そうでなければ `"タップで読み込む"`)

- [ ] **Step 1: 失敗するテストを書く(純関数)**

`frontend/src/lib/mediaViewer.svelte.test.ts`。import に `coverLabel` を足す(`deriveViewItems,` の次の行)。

```ts
import {
  coverLabel,
  deriveViewItems,
```

ファイル末尾に追加する。

```ts
describe("isRevealed (低帯域幅モード)", () => {
  it("低帯域幅でなければ従来どおり: 閲覧注意でなければ表示可、閲覧注意は登録済みのみ表示可", () => {
    expect(isRevealed({}, file({ isSensitive: false }))).toBe(true);
    expect(isRevealed({}, file({ isSensitive: true }))).toBe(false);
    expect(isRevealed({ f1: true }, file({ isSensitive: true }))).toBe(true);
  });

  it("低帯域幅では、閲覧注意でなくても登録されるまで未読み込み", () => {
    expect(isRevealed({}, file({ isSensitive: false }), true)).toBe(false);
    expect(isRevealed({ f1: true }, file({ isSensitive: false }), true)).toBe(true);
  });

  it("閲覧注意かつ低帯域幅でも、登録は 1 回で足りる", () => {
    const f = file({ isSensitive: true });
    expect(isRevealed({}, f, true)).toBe(false);
    expect(isRevealed(reveal({}, f), f, true)).toBe(true);
  });
});

describe("coverLabel", () => {
  it("閲覧注意なら従来の文言を優先する", () => {
    expect(coverLabel(file({ isSensitive: true }))).toBe("閲覧注意（クリックで表示）");
  });

  it("閲覧注意でなければ、タップで読み込む旨を出す", () => {
    expect(coverLabel(file({ isSensitive: false }))).toBe("タップで読み込む");
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/lib/mediaViewer.svelte.test.ts`
Expected: FAIL(`coverLabel is not a function`、および低帯域幅の `isRevealed` が `true` を返す)

- [ ] **Step 3: 純関数を実装する**

`frontend/src/lib/mediaViewer.svelte.ts`。`isRevealed` を置き換える(`old_string` は現在の定義ブロック)。

```ts
/// 閲覧注意でなければ常に表示可、閲覧注意なら revealed に登録済みかどうかで判定する。
export function isRevealed(revealed: Record<string, boolean>, file: DriveFile): boolean {
  return !file.isSensitive || !!revealed[file.id];
}
```

を次にする。

```ts
/// 閲覧注意でなく、かつ低帯域幅モードでもなければ常に表示可。それ以外は revealed に登録済みかどうかで判定する
/// (閲覧注意と低帯域幅が重なっても、登録は 1 回で足りる)。
export function isRevealed(
  revealed: Record<string, boolean>,
  file: DriveFile,
  lowBandwidth = false,
): boolean {
  return (!file.isSensitive && !lowBandwidth) || !!revealed[file.id];
}

/// 未読み込みカバーの文言。閲覧注意なら従来の文言を優先する。
export function coverLabel(file: DriveFile): string {
  return file.isSensitive ? "閲覧注意（クリックで表示）" : "タップで読み込む";
}
```

Run: `cd frontend && npx vitest run src/lib/mediaViewer.svelte.test.ts`
Expected: PASS

- [ ] **Step 4: 失敗するテストを書く(`MediaGrid`)**

`frontend/src/render/MediaGrid.test.ts`。`const { default: MediaGrid } = await import("./MediaGrid.svelte");` の次の行に追加する。

```ts
const { app } = await import("../lib/store.svelte");
```

`afterEach(cleanup);` を次に置き換える。

```ts
afterEach(() => {
  cleanup();
  app.ui = { ...app.ui, lowBandwidth: false };
});
```

ファイル末尾に追加する。

```ts
describe("MediaGrid 低帯域幅モード", () => {
  it("画像は取得せず、ファイル名とサイズを添えたカバーを出し、タップでサムネイルを読み込む", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, getByText } = render(MediaGrid, {
      files: [
        file({
          id: "i1",
          mimeType: "image/png",
          name: "a.png",
          url: "https://example.com/full.png",
          thumbnailUrl: "https://example.com/thumb.png",
          size: 2048,
        }),
      ],
    });
    expect(container.querySelector("img")).toBeNull();
    getByText("a.png · 2.0 KB");
    await fireEvent.click(getByText("タップで読み込む"));
    expect(container.querySelector("img")?.getAttribute("src")).toBe("https://example.com/thumb.png");
  });

  it("動画・音声もタップするまで media-player を作らない", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, getAllByText } = render(MediaGrid, {
      files: [
        file({ id: "v1", mimeType: "video/mp4", name: "a.mp4" }),
        file({ id: "s1", mimeType: "audio/mpeg", name: "a.mp3" }),
      ],
    });
    expect(container.querySelector("media-player")).toBeNull();
    const covers = getAllByText("タップで読み込む");
    expect(covers).toHaveLength(2);
    await fireEvent.click(covers[0]);
    expect(container.querySelectorAll("media-player")).toHaveLength(1);
  });

  it("サイズが無ければファイル名だけ、名前も無ければ MIME タイプを出す", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText } = render(MediaGrid, {
      files: [
        file({ id: "i1", mimeType: "image/png", name: "a.png", size: null }),
        file({ id: "i2", mimeType: "image/webp", name: undefined, size: undefined }),
      ],
    });
    getByText("a.png");
    getByText("image/webp");
  });

  it("閲覧注意かつ低帯域幅でも、タップ 1 回で表示される(文言は閲覧注意を優先)", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, getByText } = render(MediaGrid, {
      files: [file({ id: "i1", mimeType: "image/png", name: "a.png", isSensitive: true })],
    });
    expect(container.querySelector("img")).toBeNull();
    await fireEvent.click(getByText("閲覧注意（クリックで表示）"));
    expect(container.querySelector("img")).not.toBeNull();
  });

  it("オフなら従来どおり、タップなしで画像を出す", () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    const { container, queryByText } = render(MediaGrid, {
      files: [file({ id: "i1", mimeType: "image/png", name: "a.png" })],
    });
    expect(container.querySelector("img")).not.toBeNull();
    expect(queryByText("タップで読み込む")).toBeNull();
  });

  it("オンでも非メディア(保存ボタンの行)は変わらない", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText } = render(MediaGrid, { files: [file({})] });
    expect(getByText("report.pdf").closest(".file-row")).not.toBeNull();
  });
});
```

- [ ] **Step 5: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/render/MediaGrid.test.ts`
Expected: FAIL(新しい 6 件のうちオフ・非メディア以外が、`img` が存在する等で失敗する)

- [ ] **Step 6: `MediaGrid` を実装する**

`frontend/src/render/MediaGrid.svelte`。import を差し替える。`old_string`:

```ts
  import { deriveViewItems, fileName, playerSrc, isAudio, isImage, isRevealed, isVideo, reveal } from "../lib/mediaViewer.svelte";
  let { files }: { files: DriveFile[] } = $props();
```

`new_string`:

```ts
  import { app } from "../lib/store.svelte";
  import { formatFileSize } from "../lib/fileSize";
  import { coverLabel, deriveViewItems, fileName, playerSrc, isAudio, isImage, isRevealed, isVideo, reveal } from "../lib/mediaViewer.svelte";
  let { files }: { files: DriveFile[] } = $props();

  // 低帯域幅モード(Issue #591)。オンの間は、閲覧注意と同じカバーで取得を保留する。
  const lowBandwidth = $derived(app.ui.lowBandwidth ?? false);
  const coverDetail = (f: DriveFile) => [fileName(f), formatFileSize(f.size)].filter(Boolean).join(" · ");
```

カバーの描画を差し替える。`old_string`:

```svelte
        {#if !isRevealed(revealed, f)}
          <button
            class="sensitive-cover h-full w-full border-0 text-sm text-muted-foreground"
            onclick={() => (revealed = reveal(revealed, f))}
          >
            閲覧注意（クリックで表示）
          </button>
```

`new_string`:

```svelte
        {#if !isRevealed(revealed, f, lowBandwidth)}
          <button
            class="sensitive-cover flex h-full w-full flex-col items-center justify-center gap-0.5 border-0 px-2 text-sm text-muted-foreground"
            onclick={() => (revealed = reveal(revealed, f))}
          >
            <span>{coverLabel(f)}</span>
            {#if lowBandwidth}
              <span class="max-w-full truncate text-xs">{coverDetail(f)}</span>
            {/if}
          </button>
```

Run: `cd frontend && npx vitest run src/render/MediaGrid.test.ts`
Expected: PASS(既存 4 件 + 新規 6 件)

- [ ] **Step 7: 失敗するテストを書く(`MediaViewer`)**

`frontend/src/render/MediaViewer.test.ts`。`const { saveMediaToDisk } = await import("../lib/mediaDownload");` の次の行に追加する。

```ts
const { app } = await import("../lib/store.svelte");
```

ファイル末尾に追加する。

```ts
describe("MediaViewer 低帯域幅モード", () => {
  afterEach(() => {
    app.ui = { ...app.ui, lowBandwidth: false };
  });

  it("未読み込みの添付は cropper-image を作らず、読み込み済みのものだけ作る", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText } = render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 0,
        revealed: { a: true },
        onclose: () => {},
      },
    });
    const images = document.querySelectorAll("cropper-image");
    expect(images).toHaveLength(1);
    expect(images[0].getAttribute("alt")).toBe("a.png");
    getByText("タップで読み込む");
  });

  it("開いているページのカバーをタップすると読み込む", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText } = render(MediaViewer, {
      props: { files: [file({ id: "a", name: "a.png" })], startIndex: 0, revealed: {}, onclose: () => {} },
    });
    expect(document.querySelector("cropper-image")).toBeNull();
    await fireEvent.click(getByText("タップで読み込む"));
    expect(document.querySelector("cropper-image")).not.toBeNull();
  });

  it("オフなら従来どおり、すべての画像を描画する", () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    render(MediaViewer, {
      props: {
        files: [file({ id: "a", name: "a.png" }), file({ id: "b", name: "b.png" })],
        startIndex: 0,
        revealed: {},
        onclose: () => {},
      },
    });
    expect(document.querySelectorAll("cropper-image")).toHaveLength(2);
  });
});
```

- [ ] **Step 8: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/render/MediaViewer.test.ts`
Expected: FAIL(新しい 3 件のうち、未読み込みの添付で `cropper-image` が 2 個出る等)

- [ ] **Step 9: `MediaViewer` を実装する**

`frontend/src/render/MediaViewer.svelte`。`../lib/mediaViewer.svelte` からの import に `coverLabel` を足す。`old_string`:

```ts
  import {
    deriveViewItems,
    fileName,
    initialImageTransform,
```

`new_string`:

```ts
  import {
    coverLabel,
    deriveViewItems,
    fileName,
    initialImageTransform,
```

`app` は import 済み(`import { app } from "../lib/store.svelte";`)。`<script>` 内、`function onRevealClick()` の直前に追加する。

```ts
  // 低帯域幅モード(Issue #591)。ビューアは全添付の src を一括で描画するため、ここでも
  // 未読み込みの添付はカバーのままにして、実画像の取得を保留する。
  const lowBandwidth = $derived(app.ui.lowBandwidth ?? false);
```

3 か所の `isRevealed` 呼び出しを置き換える。

```svelte
        aria-label={isRevealed(revealed, item) ? `${fileName(item)}を表示中` : undefined}
      >
        {#if !isRevealed(revealed, item)}
```

を

```svelte
        aria-label={isRevealed(revealed, item, lowBandwidth) ? `${fileName(item)}を表示中` : undefined}
      >
        {#if !isRevealed(revealed, item, lowBandwidth)}
```

に、カバーの文言

```svelte
            閲覧注意（クリックで表示）
          </button>
        {:else if isImage(item)}
```

を

```svelte
            {coverLabel(item)}
          </button>
        {:else if isImage(item)}
```

に、画像ツールバーの条件

```svelte
  {#if isRevealed(revealed, current) && isImage(current)}
```

を

```svelte
  {#if isRevealed(revealed, current, lowBandwidth) && isImage(current)}
```

に置き換える。

Run: `cd frontend && npx vitest run src/render/MediaViewer.test.ts`
Expected: PASS(既存テストと新規 3 件)

Run: `cd frontend && npx vitest run src/render/FileList.test.ts src/lib/mediaViewer.svelte.test.ts`
Expected: PASS(`FileList` は `isRevealed` を既定値で呼ぶので変わらない)

- [ ] **Step 10: Commit**

```bash
git add frontend/src/lib/mediaViewer.svelte.ts frontend/src/lib/mediaViewer.svelte.test.ts frontend/src/render/MediaGrid.svelte frontend/src/render/MediaGrid.test.ts frontend/src/render/MediaViewer.svelte frontend/src/render/MediaViewer.test.ts
git commit -m "feat(media): 低帯域幅モードで添付をタップで読み込む(Issue #591)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: アバター

**Files:**
- Modify: `frontend/src/ui/Avatar.svelte`
- Modify: `frontend/src/ui/Avatar.test.host.svelte`
- Modify: `frontend/src/ui/Avatar.test.ts`
- Modify: `frontend/src/ui/NoteCard.svelte`(`<Avatar>` に 1 属性)・`frontend/src/ui/NoteCard.test.ts`
- Modify: `frontend/src/ui/NotificationCard.svelte`・`frontend/src/ui/NotificationCard.test.ts`
- Modify: `frontend/src/ui/ReplyTargetCard.svelte`・`frontend/src/ui/ReplyTargetCard.test.ts`
- Modify: `frontend/src/ui/ReactionUsersPopover.svelte`(`<Avatar>` に 1 属性。専用テストが無いため `Avatar` のテストと Task 6 の実画面確認で担保する)

**Interfaces:**
- Consumes: `app.ui.lowBandwidth?: boolean`。
- Produces: `Avatar` の prop `deferInLowBandwidth?: boolean`(既定 `false`)。`true` かつ低帯域幅モードのとき、枠ごと何も描画しない。`ProfileModal`・`FollowListModal`・`AccountSelect`・`AccountsSection` は渡さない(通常表示)。

- [ ] **Step 1: 失敗するテストを書く(`Avatar`)**

`frontend/src/ui/Avatar.test.host.svelte` を次に置き換える。

```svelte
<script lang="ts">
  import Avatar from "./Avatar.svelte";
  let { isCat, deferInLowBandwidth = false }: { isCat: boolean; deferInLowBandwidth?: boolean } = $props();
</script>

<Avatar {isCat} {deferInLowBandwidth} class="h-8 w-8">
  <img data-testid="inner-img" src="https://example.com/a.png" alt="" />
</Avatar>
```

`frontend/src/ui/Avatar.test.ts` の末尾に追加する。

```ts
describe("Avatar 低帯域幅モード", () => {
  afterEach(() => {
    app.ui = { ...app.ui, lowBandwidth: false };
  });

  it("deferInLowBandwidth かつ低帯域幅モードなら、枠ごと何も描画しない", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, queryByTestId } = render(AvatarTestHost, {
      props: { isCat: true, deferInLowBandwidth: true },
    });
    expect(container.querySelector(".avatar-frame")).toBeNull();
    expect(container.querySelector(".ears")).toBeNull();
    expect(queryByTestId("inner-img")).toBeNull();
  });

  it("deferInLowBandwidth でも、低帯域幅モードがオフなら描画する", () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    const { queryByTestId } = render(AvatarTestHost, {
      props: { isCat: false, deferInLowBandwidth: true },
    });
    expect(queryByTestId("inner-img")).not.toBeNull();
  });

  it("低帯域幅モードがオンでも、deferInLowBandwidth を渡さない呼び出し(プロフィール等)は描画する", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { queryByTestId } = render(AvatarTestHost, { props: { isCat: false } });
    expect(queryByTestId("inner-img")).not.toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/Avatar.test.ts`
Expected: FAIL(1 件目: 枠が描画されてしまう)

- [ ] **Step 3: `Avatar` を実装する**

`frontend/src/ui/Avatar.svelte`。`<script>` を差し替える。`old_string`:

```ts
  import { effectiveIsCat } from "../lib/catMode";

  let {
    isCat = false,
    avatarBlurhash = null,
    class: className = "",
    children,
  }: {
    isCat?: boolean;
    avatarBlurhash?: string | null;
    class?: string;
    children: Snippet;
  } = $props();
```

`new_string`:

```ts
  import { effectiveIsCat } from "../lib/catMode";
  import { app } from "../lib/store.svelte";

  let {
    isCat = false,
    avatarBlurhash = null,
    class: className = "",
    /// タイムライン上のアバター(ノート・通知・返信先・リアクションユーザー)だけが true にする。
    /// 低帯域幅モード(Issue #591)では枠ごと描画しない。ユーザーが自分で開く画面は渡さない。
    deferInLowBandwidth = false,
    children,
  }: {
    isCat?: boolean;
    avatarBlurhash?: string | null;
    class?: string;
    deferInLowBandwidth?: boolean;
    children: Snippet;
  } = $props();

  const hidden = $derived(deferInLowBandwidth && (app.ui.lowBandwidth ?? false));
```

マークアップ全体を `{#if !hidden}` で包む。`<span class="avatar-frame relative inline-block {className}">` の直前に `{#if !hidden}` を、`{/if}`(耳の `{#if showEars}...{/if}`)の後の `</span>` の直後に `{/if}` を足す。つまり、

```svelte
{#if !hidden}
<span class="avatar-frame relative inline-block {className}">
  ...(既存のまま)...
</span>
{/if}
```

となる。`<style>` ブロックはそのまま。

Run: `cd frontend && npx vitest run src/ui/Avatar.test.ts`
Expected: PASS(既存の `catMode` テストも含めて)

- [ ] **Step 4: 失敗するテストを書く(呼び出し側 3 件)**

`frontend/src/ui/NoteCard.test.ts`。ファイル先頭付近の `afterEach` の `app.ui = {...}` に `lowBandwidth: false` を足す。`old_string`:

```ts
  app.ui = { ...app.ui, instanceTicker: "remote", catMode: "respect" };
```

`new_string`:

```ts
  app.ui = { ...app.ui, instanceTicker: "remote", catMode: "respect", lowBandwidth: false };
```

`describe("プロフィール導線", () => {` の直前に追加する。

```ts
describe("低帯域幅モードのアバター(Issue #591)", () => {
  it("オンでは、avatarUrl があってもアバターを描画せず、名前からプロフィールを開ける", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const note = makeNote({ user: makeUser({ avatarUrl: "https://example.com/a.png" }) });
    const { container, getByTestId } = render(NoteCard, { props: { note, accountId: "acc1" } });
    expect(container.querySelector('[data-testid="note-avatar"]')).toBeNull();
    expect(container.querySelector('img[src="https://example.com/a.png"]')).toBeNull();
    await fireEvent.click(getByTestId("note-name"));
    expect(openProfile).toHaveBeenCalledWith({ userId: note.user.id }, "acc1");
  });

  it("オフなら従来どおりアバターを描画する", () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    const note = makeNote({ user: makeUser({ avatarUrl: "https://example.com/a.png" }) });
    const { container } = render(NoteCard, { props: { note, accountId: "acc1" } });
    expect(container.querySelector('img[data-testid="note-avatar"]')).not.toBeNull();
  });
});
```

`frontend/src/ui/NotificationCard.test.ts`。末尾に追加する(`app` は既に import 済み)。

```ts
describe("NotificationCard 低帯域幅モードのアバター(Issue #591)", () => {
  afterEach(() => {
    app.ui = { ...app.ui, lowBandwidth: false };
  });

  it("オンでは、通知元のアバターを描画せず、表示名からプロフィールを開ける", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const notification = makeNotification({
      type: "follow",
      note: null,
      user: makeUser({ id: "u9", name: "Carol", avatarUrl: "https://example.com/carol.png" }),
    });
    const { container } = render(NotificationCard, { props: { notification, accountId: "a1" } });
    expect(container.querySelector('[data-testid="notification-avatar"]')).toBeNull();
    await fireEvent.click(container.querySelector('[data-testid="notification-actor"]') as HTMLElement);
    expect(currentProfileTarget()).toEqual({ userId: "u9" });
    closeProfile();
  });

  it("オフなら従来どおりアバターを描画する", () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    const notification = makeNotification({
      type: "follow",
      note: null,
      user: makeUser({ id: "u9", name: "Carol", avatarUrl: "https://example.com/carol.png" }),
    });
    const { container } = render(NotificationCard, { props: { notification, accountId: "a1" } });
    expect(container.querySelector('[data-testid="notification-avatar"]')).not.toBeNull();
  });
});
```

`frontend/src/ui/ReplyTargetCard.test.ts`。末尾に追加する(`app`・`openProfile`・`render`・`makeNote`・`makeUser` は既存)。

```ts
describe("ReplyTargetCard 低帯域幅モードのアバター(Issue #591)", () => {
  afterEach(() => {
    app.ui = { ...app.ui, lowBandwidth: false };
  });

  it("オンでは、アバターを描画せず、表示名からプロフィールを開ける", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { container, getByTestId } = render(ReplyTargetCard, {
      props: { note: makeNote(), accountId: "acc1" },
    });
    expect(container.querySelector('[data-testid="reply-card-avatar"]')).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    await fireEvent.click(getByTestId("reply-card-name"));
    expect(openProfile).toHaveBeenCalledWith({ userId: "u2" }, "acc1");
  });

  it("オフなら従来どおりアバターを描画する", () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    const { container } = render(ReplyTargetCard, { props: { note: makeNote(), accountId: "acc1" } });
    expect(container.querySelector('[data-testid="reply-card-avatar"]')).not.toBeNull();
  });
});
```

(`makeUser` の既定 `id` は `"u2"`。このファイルの既存テストと同じ。)

- [ ] **Step 5: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/ui/NoteCard.test.ts src/ui/NotificationCard.test.ts src/ui/ReplyTargetCard.test.ts`
Expected: FAIL(オンのテストで、アバターが描画されてしまう)

- [ ] **Step 6: 呼び出し 4 か所に属性を足す**

各 `<Avatar` に `deferInLowBandwidth` を 1 つ足すだけ。

`frontend/src/ui/NoteCard.svelte`:

```svelte
    <Avatar isCat={inner.user.isCat} avatarBlurhash={inner.user.avatarBlurhash} class="h-[34px] w-[34px] flex-none">
```

を

```svelte
    <Avatar isCat={inner.user.isCat} avatarBlurhash={inner.user.avatarBlurhash} deferInLowBandwidth class="h-[34px] w-[34px] flex-none">
```

`frontend/src/ui/NotificationCard.svelte`:

```svelte
      <Avatar isCat={n.user.isCat} avatarBlurhash={n.user.avatarBlurhash} class="h-6 w-6 flex-none">
```

を

```svelte
      <Avatar isCat={n.user.isCat} avatarBlurhash={n.user.avatarBlurhash} deferInLowBandwidth class="h-6 w-6 flex-none">
```

`frontend/src/ui/ReplyTargetCard.svelte`:

```svelte
  <Avatar isCat={note.user.isCat} avatarBlurhash={note.user.avatarBlurhash} class="mt-0.5 h-6 w-6 flex-none">
```

を

```svelte
  <Avatar isCat={note.user.isCat} avatarBlurhash={note.user.avatarBlurhash} deferInLowBandwidth class="mt-0.5 h-6 w-6 flex-none">
```

`frontend/src/ui/ReactionUsersPopover.svelte`:

```svelte
            <Avatar isCat={u.isCat} avatarBlurhash={u.avatarBlurhash} class="h-5 w-5 flex-shrink-0">
```

を

```svelte
            <Avatar isCat={u.isCat} avatarBlurhash={u.avatarBlurhash} deferInLowBandwidth class="h-5 w-5 flex-shrink-0">
```

- [ ] **Step 7: テストが通ることを確認する**

Run: `cd frontend && npx vitest run src/ui/Avatar.test.ts src/ui/NoteCard.test.ts src/ui/NotificationCard.test.ts src/ui/ReplyTargetCard.test.ts src/ui/ProfileModal.test.ts src/ui/FollowListModal.test.ts`
Expected: PASS(`ProfileModal` / `FollowListModal` は `deferInLowBandwidth` を渡さないので変わらない)

- [ ] **Step 8: Commit**

```bash
git add frontend/src/ui/Avatar.svelte frontend/src/ui/Avatar.test.host.svelte frontend/src/ui/Avatar.test.ts frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts frontend/src/ui/NotificationCard.svelte frontend/src/ui/NotificationCard.test.ts frontend/src/ui/ReplyTargetCard.svelte frontend/src/ui/ReplyTargetCard.test.ts frontend/src/ui/ReactionUsersPopover.svelte
git commit -m "feat(avatar): 低帯域幅モードでタイムラインのアバターを描画しない(Issue #591)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: カスタム絵文字と URL プレビュー

**Files:**
- Modify: `frontend/src/render/CustomEmoji.svelte`・`frontend/src/render/CustomEmoji.test.ts`
- Modify: `frontend/src/render/UrlPreviewCard.svelte`・`frontend/src/render/UrlPreviewCard.test.ts`

**Interfaces:**
- Consumes: `app.ui.lowBandwidth?: boolean`、`Button`(`$lib/components/ui/button`、`variant="outline"` / `size="sm"`)。
- Produces: 低帯域幅モードでは `CustomEmoji` が `url` があっても `:name:` のテキストを出し、`UrlPreviewCard` が「プレビューを読み込む」ボタンだけを出す(押すまで `fetchUrlPreview` を呼ばず、キャッシュ済みでもカードを描画しない)。

- [ ] **Step 1: 失敗するテストを書く(`CustomEmoji`)**

`frontend/src/render/CustomEmoji.test.ts` を次に置き換える(先頭に Tauri プラグインのスタブを足し、`app` を読む。`vi.mock` は巻き上げられるので、静的 import のままで効く)。

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import CustomEmoji from "./CustomEmoji.svelte";

// CustomEmoji は低帯域幅モードの判定で store.svelte.ts を読むため、Tauri ランタイム外でも import できるようスタブする。
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

const { app } = await import("../lib/store.svelte");

afterEach(() => {
  cleanup();
  app.ui = { ...app.ui, lowBandwidth: false };
});

describe("CustomEmoji", () => {
  it("renders an img with alt/title when a url is provided", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png" },
    });
    const img = getByRole("img");
    expect(img.getAttribute("src")).toBe("https://example.com/e.png");
    expect(img.getAttribute("alt")).toBe(":blob_cat:");
    expect(img.getAttribute("title")).toBe(":blob_cat:");
  });

  it("omits the title attribute when showTitle is false", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png", showTitle: false },
    });
    expect(getByRole("img").hasAttribute("title")).toBe(false);
  });

  it("renders fallback text when no url is provided", () => {
    const { getByText, queryByRole } = render(CustomEmoji, { props: { name: "blob_cat" } });
    expect(getByText(":blob_cat:")).toBeTruthy();
    expect(queryByRole("img")).toBeNull();
  });

  it("低帯域幅モードでは、url があっても画像を取得せず :name: をテキスト表示する(Issue #591)", () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    const { getByText, queryByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png" },
    });
    expect(queryByRole("img")).toBeNull();
    expect(getByText(":blob_cat:")).toBeTruthy();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/render/CustomEmoji.test.ts`
Expected: FAIL(最後の 1 件: `img` が描画されてしまう)

- [ ] **Step 3: `CustomEmoji` を実装する**

`frontend/src/render/CustomEmoji.svelte` の `<script>` とマークアップを次にする(`<style>` はそのまま)。

```svelte
<script lang="ts">
  import { app } from "../lib/store.svelte";

  // カスタム絵文字。url があれば画像、無ければ :name: をテキスト表示（Phase 2: 絵文字辞書は未取得）
  // showTitle=false でネイティブツールチップを抑制（例: リアクションバッジは独自のホバー一覧を持つため不要）
  // 低帯域幅モード(Issue #591)では、url があっても画像を取得せず :name: をテキスト表示する。
  let { name, url, showTitle = true }: { name: string; url?: string; showTitle?: boolean } = $props();
</script>

{#if url && !app.ui.lowBandwidth}
  <img class="custom-emoji" src={url} alt={`:${name}:`} title={showTitle ? `:${name}:` : undefined} loading="lazy" />
{:else}
  <span class="custom-emoji-fallback">:{name}:</span>
{/if}
```

Run: `cd frontend && npx vitest run src/render/CustomEmoji.test.ts`
Expected: PASS(4 件)

- [ ] **Step 4: 失敗するテストを書く(`UrlPreviewCard`)**

`frontend/src/render/UrlPreviewCard.test.ts`。1 行目の import を差し替える。

```ts
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
```

`vi.mock("../lib/urlPreview", ...)` ブロックの直前に Tauri プラグインのスタブを足す。

```ts
// UrlPreviewCard は低帯域幅モードの判定で store.svelte.ts を読むため、Tauri ランタイム外でも import できるようスタブする。
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

`afterEach(() => {` の直前に `app` の import を足す(動的 import は、上の `vi.mock` 群より後ろに置く)。

```ts
const { app } = await import("../lib/store.svelte");
```

`afterEach` の中に `app.ui = { ...app.ui, lowBandwidth: false };` を 1 行追加する。`old_string`:

```ts
afterEach(() => {
  cleanup();
  cachedUrlPreviewMock.mockReset();
  fetchUrlPreviewMock.mockReset();
});
```

`new_string`:

```ts
afterEach(() => {
  cleanup();
  cachedUrlPreviewMock.mockReset();
  fetchUrlPreviewMock.mockReset();
  app.ui = { ...app.ui, lowBandwidth: false };
});
```

ファイル末尾に追加する。

```ts
describe("UrlPreviewCard 低帯域幅モード(Issue #591)", () => {
  it("取得せずボタンだけを出し、押すと取得して表示する", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    cachedUrlPreviewMock.mockReturnValue(undefined);
    fetchUrlPreviewMock.mockResolvedValue(PREVIEW);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });

    expect(fetchUrlPreviewMock).not.toHaveBeenCalled();
    expect(screen.queryByText("記事タイトル")).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "プレビューを読み込む" }));
    await waitFor(() => expect(screen.getByText("記事タイトル")).toBeTruthy());
    expect(fetchUrlPreviewMock).toHaveBeenCalledTimes(1);
  });

  it("キャッシュ済みでも、押すまでカードを描画しない", async () => {
    app.ui = { ...app.ui, lowBandwidth: true };
    cachedUrlPreviewMock.mockReturnValue(PREVIEW);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });

    expect(screen.queryByText("記事タイトル")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "プレビューを読み込む" }));
    expect(screen.getByText("記事タイトル")).toBeTruthy();
    expect(fetchUrlPreviewMock).not.toHaveBeenCalled();
  });

  it("オフなら従来どおり、ボタンなしで取得する", async () => {
    app.ui = { ...app.ui, lowBandwidth: false };
    cachedUrlPreviewMock.mockReturnValue(undefined);
    fetchUrlPreviewMock.mockResolvedValue(PREVIEW);
    render(UrlPreviewCard, { props: { url: "https://example.com/a", instanceHost: undefined } });

    await waitFor(() => expect(screen.getByText("記事タイトル")).toBeTruthy());
    expect(screen.queryByRole("button", { name: "プレビューを読み込む" })).toBeNull();
  });
});
```

- [ ] **Step 5: テストが失敗することを確認する**

Run: `cd frontend && npx vitest run src/render/UrlPreviewCard.test.ts`
Expected: FAIL(`Unable to find an accessible element with the role "button" and name "プレビューを読み込む"`)

- [ ] **Step 6: `UrlPreviewCard` を実装する**

`frontend/src/render/UrlPreviewCard.svelte`。`<script>` を差し替える。`old_string`:

```ts
  import { cachedUrlPreview, fetchUrlPreview, isSafeUrl, resolvedThumbnailUrl } from "../lib/urlPreview";

  let { url, instanceHost }: { url: string; instanceHost: string | undefined } = $props();

  let preview = $state<UrlPreview | null | undefined>(cachedUrlPreview(url));
  let revealed = $state(false);
  let playing = $state(false);

  $effect(() => {
    if (preview !== undefined) return;
```

`new_string`:

```ts
  import { Button } from "$lib/components/ui/button";
  import { app } from "../lib/store.svelte";
  import { cachedUrlPreview, fetchUrlPreview, isSafeUrl, resolvedThumbnailUrl } from "../lib/urlPreview";

  let { url, instanceHost }: { url: string; instanceHost: string | undefined } = $props();

  let preview = $state<UrlPreview | null | undefined>(cachedUrlPreview(url));
  let revealed = $state(false);
  let playing = $state(false);

  // 低帯域幅モード(Issue #591)。押されるまで取得もカード(サムネイル)の描画もしない。
  // キャッシュ済みでも同じ扱いにして、キャッシュの有無で見た目を変えない。
  let loadRequested = $state(false);
  const deferred = $derived((app.ui.lowBandwidth ?? false) && !loadRequested);

  $effect(() => {
    if (deferred || preview !== undefined) return;
```

マークアップの先頭を差し替える。`old_string`:

```svelte
{#if preview}
  {@const hasPlayer = preview.player && isSafeUrl(preview.player.url)}
```

`new_string`:

```svelte
{#if deferred}
  <div class="mt-2">
    <Button type="button" variant="outline" size="sm" onclick={() => (loadRequested = true)}>プレビューを読み込む</Button>
  </div>
{:else if preview}
  {@const hasPlayer = preview.player && isSafeUrl(preview.player.url)}
```

Run: `cd frontend && npx vitest run src/render/UrlPreviewCard.test.ts`
Expected: PASS(既存テストと新規 3 件)

Run: `cd frontend && npx vitest run src/ui/NoteCard.test.ts`
Expected: PASS(`NoteCard` が `UrlPreviewCard` を使うため、回帰を確認)

- [ ] **Step 7: Commit**

```bash
git add frontend/src/render/CustomEmoji.svelte frontend/src/render/CustomEmoji.test.ts frontend/src/render/UrlPreviewCard.svelte frontend/src/render/UrlPreviewCard.test.ts
git commit -m "feat(render): 低帯域幅モードでカスタム絵文字とURLプレビューを取得しない(Issue #591)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: ユーザーガイド・全体検証・実画面確認

**Files:**
- Modify: `docs/guide/user-guide.md`

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md`。「外観」の項目(設定画面の節)に追記する。`old_string`:

```
MFMアニメーション（$[shake]等の装飾アニメーションのON/OFF）、
```

`new_string`:

```
低帯域幅モード（回線が遅い・従量制のときに通信量を減らす。オンの間は、ノートの添付（画像・動画・音声）、タイムライン上のアバター、カスタム絵文字、URLプレビューを自動では取得しません。添付とURLプレビューはタップしたときだけ読み込み、アバターは表示せず、カスタム絵文字は `:name:` の文字で表示します。プロフィール・フォロー一覧・アカウント選択のアバターは通常どおり表示します。端末ごとの設定です）、MFMアニメーション（$[shake]等の装飾アニメーションのON/OFF）、
```

「設定のバックアップ」の「エクスポート」の項目に追記する。`old_string`:

```
ローカル翻訳のモデル本体と、PLaMo のライセンスに同意した記録も含まれません(同意は端末ごとに行うため、インポートで同意済みにはなりません)。
```

`new_string`:

```
ローカル翻訳のモデル本体と、PLaMo のライセンスに同意した記録も含まれません(同意は端末ごとに行うため、インポートで同意済みにはなりません)。低帯域幅モードも端末ごとの設定なので含まれず、インポートしても変わりません。
```

- [ ] **Step 2: 全体検証(Rust)**

Run: `cd src-tauri && cargo test`
Expected: PASS(`#[ignore]` のテストは対象外)。`generates_frontend_bindings` で `tauri.gen.ts` が再生成され、`git status` に差分が出ないこと(Task 1 で反映済み)。

- [ ] **Step 3: 全体検証(フロント)**

Run: `cd frontend && pnpm check`
Expected: エラー 0 件

Run: `cd frontend && pnpm test`
Expected: PASS(全件)

- [ ] **Step 4: 実画面の確認(subagent のレビュー通過では代替しない)**

`e2e/README.md` の手順で、throwaway の Docker Compose Misskey と Xvfb(`WAYLAND_DISPLAY` は unset)上のアプリを使い、使い捨ての wdio spec(コミットしない)で次を実際に確認する。画像は自分で見る。

1. 画像・動画付きのノート、カスタム絵文字入りのノート、URL 付きのノートを投稿し、リアクションも付ける。
2. 低帯域幅モードがオフ: 画像・アバター・絵文字・URL プレビューが従来どおり出る。
3. 設定 → 外観でオンにして保存: 添付がカバー(ファイル名・サイズ付き)になる。タップで読み込み、もう一度タップで拡大ビューアが開き、同じノートの他の未読み込み画像はビューア内でもカバーのまま。アバターが消えてレイアウトが詰まる。カスタム絵文字が `:name:` になる。URL プレビューが「プレビューを読み込む」になる。リアクション数にホバーして出る一覧でも、アバターが出ない。プロフィールを開くと、そこのアバターは通常どおり出る。
4. ネットワーク面の確認: オンの間、`MediaGrid` の画像・動画のリクエストが飛ばないこと(プレースホルダーの間)。
5. 後片付け: 自分で起動した Xvfb・アプリ・Docker コンテナを、正確な PID / ID で止める(`docker rm -fv <id>`)。

- [ ] **Step 5: Commit**

```bash
git add docs/guide/user-guide.md
git commit -m "docs(guide): 低帯域幅モードの説明を追加(Issue #591)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 6: 完了手続き**

`superpowers:finishing-a-development-branch` に従う。PR は `Fixes #591` を本文に入れ、`.github/pull_request_template.md` の構成に手で合わせる(`--body` を使うとテンプレートが当たらないため)。マージは `gh pr merge --merge`(squash しない)。push 後に CI を Monitor で待たない。
