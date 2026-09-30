# UIスケール設定 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定画面の「外観」から、UI全体（文字・余白・アイコン・画像・カラム幅）の倍率を50〜200%で変更でき、再起動後も保持される（デスクトップのみ）。

**Architecture:** `UiPrefs.ui_scale`（i32, %）をRustの設定に追加して永続化し、フロントの`lib/uiScale.ts`が正規化（clamp）と`getCurrentWebview().setZoom()`の呼び出しを担う。`store.svelte.ts`が起動時・保存時に`applyUiScale`を呼び、設定UIはスライダーで値を編集する。

**Tech Stack:** Rust（serde, tauri-specta）, Svelte 5 + TypeScript, Vitest, `@tauri-apps/api/webview`

**Spec:** `docs/superpowers/specs/2026-09-30-issue-40-ui-scale-design.md`

## Global Constraints

- 対象はデスクトップのみ。Android/iOS（`isMobilePlatform`）では`setZoom`を呼ばず、設定項目も表示しない。
- 範囲は50〜200%、既定は100%、スライダーは10刻み。
- `UiPrefs.ui_scale`は`#[serde(default = "default_ui_scale")]`で、旧JSONは100として読める。Rust側でclamp/検証はしない（他の数値項目と同じ流儀）。
- clamp/正規化はフロントの純関数`normalizeUiScale`で行い、適用時に必ず通す。
- `frontend/src/bindings/tauri.gen.ts`は手編集しない。`cd src-tauri && cargo test`で再生成する。
- capabilityに`core:webview:allow-set-webview-zoom`を追加する（`core:default`には含まれない）。
- コミットメッセージは件名のみ（本文なし）。末尾に`Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`を付ける。`--no-verify`は使わない。コミットが失敗/タイムアウトしたら止めて報告する。
- フィーチャーブランチは`feat/40-ui-scale`（作成済み）。mainには直接コミットしない。
- 実UIの確認はXvfb + `dbus-run-session`の隔離環境で行い、`WAYLAND_DISPLAY`もunsetする。自分で起動した`cargo tauri dev`は完了前に自分でkillする（PID指定のみ、`pkill`/`killall`は使わない）。

## Review Focus

- `ui_scale`が無い旧設定JSON → 100%で読める（Task 1）
- 保存値が範囲外（0、負数、1000など）や`NaN`/`undefined` → 壊れたズームにならず、範囲内に丸められる／既定100になる（Task 2）
- 小数（例: 125.4）→ 整数%に丸められる（Task 2）
- `setZoom`が失敗（未対応環境・IPCエラー）→ 設定の保存と他の反映を止めず、警告ログだけ残す（Task 3）
- モバイルでは`setZoom`が呼ばれず、設定項目も出ない（Task 2・Task 4）

---

### Task 1: Rust設定フィールドとcapability

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`（フィールド追加: `custom_css`の直後 約224行、既定値関数: `default_haptics_enabled`の近く 約294行、`Default`実装 約335行、テスト）
- Modify: `src-tauri/capabilities/default.json`
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Consumes: なし
- Produces: `UiPrefs.ui_scale: i32`（TSでは`uiScale?: number`）。`default_ui_scale() -> i32`（100）。capability `core:webview:allow-set-webview-zoom`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/ui.rs`のテストモジュール末尾（`avatar_radius_defaults_to_20_for_legacy_json`の後）に追加:

```rust
    #[test]
    fn ui_scale_defaults_to_100_for_legacy_json() {
        // ui_scale 追加前に保存された JSON も読めること（#[serde(default)]）。
        // 0 ではなく 100（等倍）にフォールバックすること（Issue #40）。
        let v: UiPrefs = serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.ui_scale, 100);
        assert_eq!(UiPrefs::default().ui_scale, 100);
    }

    #[test]
    fn ui_scale_roundtrips_as_camel_case() {
        let mut p = UiPrefs::default();
        p.ui_scale = 130;
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"uiScale\":130"));
        let back: UiPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(back.ui_scale, 130);
    }
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib ui_scale_`
Expected: コンパイルエラー（`no field ui_scale on type UiPrefs`）

- [ ] **Step 3: 最小実装**

`custom_css`フィールドの直後（`pub custom_css: String,`の次の行）に追加:

```rust
    /// UI全体の拡大率（%、50〜200）。既定は100（Issue #40）。デスクトップのみ意味を持つ
    /// （Tauri の Webview::set_zoom は Android 非対応）。値の clamp はフロント（lib/uiScale）が
    /// 適用時に行うため、Rust 側では不透明に永続化する。
    #[serde(default = "default_ui_scale")]
    pub ui_scale: i32,
```

`default_haptics_enabled`の直後に追加:

```rust
fn default_ui_scale() -> i32 {
    100
}
```

`Default`実装の`custom_css: String::new(),`の次の行に追加:

```rust
            ui_scale: default_ui_scale(),
```

既存テスト`roundtrips_keymap`のUiPrefsリテラルの`custom_css: "body { background: #000 }".into(),`の次の行に追加（構造体リテラルが全フィールド必須のため、これが無いとコンパイルが通らない）:

```rust
            ui_scale: 130,
```

- [ ] **Step 4: テストが通ることと、バインディングが再生成されることを確認する**

Run: `cd src-tauri && cargo test --lib ui_scale_ && cargo test generates_frontend_bindings`
Expected: PASS。続けて`git diff frontend/src/bindings/tauri.gen.ts`に`uiScale?: number`の追加のみが出る。

- [ ] **Step 5: capabilityを追加する**

`src-tauri/capabilities/default.json`の`"core:default",`の次の行に追加:

```json
    "core:webview:allow-set-webview-zoom",
```

- [ ] **Step 6: Rust全体のテストが通ることを確認する**

Run: `cd src-tauri && cargo test`
Expected: PASS（`#[ignore]`のテストは除く）

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/domain/ui.rs src-tauri/capabilities/default.json frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsにui_scaleを追加しWebViewズームの権限を許可

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `lib/uiScale.ts`（正規化と適用）

**Files:**
- Create: `frontend/src/lib/uiScale.ts`
- Test: `frontend/src/lib/uiScale.test.ts`

**Interfaces:**
- Consumes: `isMobilePlatform`（`./platform`）、`getCurrentWebview`（`@tauri-apps/api/webview`）
- Produces:
  - `UI_SCALE_MIN = 50`, `UI_SCALE_MAX = 200`, `UI_SCALE_DEFAULT = 100`, `UI_SCALE_STEP = 10`
  - `normalizeUiScale(value: number | null | undefined): number` — 整数%を返す。`null`/`undefined`/`NaN`/`Infinity`は100。それ以外は四捨五入して50〜200にclamp。
  - `applyUiScale(value: number | null | undefined): Promise<void>` — モバイルでは何もしない。それ以外は`getCurrentWebview().setZoom(normalizeUiScale(value) / 100)`を呼ぶ。失敗時は例外をそのまま投げる（呼び出し側がログする）。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/uiScale.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";

// platform.ts は import 時に @tauri-apps/plugin-os の platform() を呼ぶため、
// Tauri ランタイム外で import が失敗しないよう、isMobilePlatform を差し替え可能にしてスタブする。
const platformMock = vi.hoisted(() => ({ isMobilePlatform: false }));
vi.mock("./platform", () => platformMock);

const setZoomMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ setZoom: setZoomMock }),
}));

const { normalizeUiScale, applyUiScale, UI_SCALE_MIN, UI_SCALE_MAX, UI_SCALE_DEFAULT } = await import("./uiScale");

beforeEach(() => {
  setZoomMock.mockReset();
  setZoomMock.mockResolvedValue(undefined);
  platformMock.isMobilePlatform = false;
});

describe("normalizeUiScale", () => {
  it("範囲内の整数はそのまま返す", () => {
    expect(normalizeUiScale(100)).toBe(100);
    expect(normalizeUiScale(130)).toBe(130);
    expect(normalizeUiScale(UI_SCALE_MIN)).toBe(50);
    expect(normalizeUiScale(UI_SCALE_MAX)).toBe(200);
  });

  it("範囲外はクランプする", () => {
    expect(normalizeUiScale(0)).toBe(50);
    expect(normalizeUiScale(-30)).toBe(50);
    expect(normalizeUiScale(49)).toBe(50);
    expect(normalizeUiScale(201)).toBe(200);
    expect(normalizeUiScale(1000)).toBe(200);
  });

  it("小数は四捨五入して整数%にする", () => {
    expect(normalizeUiScale(125.4)).toBe(125);
    expect(normalizeUiScale(125.5)).toBe(126);
  });

  it("null/undefined/NaN/Infinity は既定の100にする", () => {
    expect(normalizeUiScale(undefined)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(null)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(Number.NaN)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(Number.POSITIVE_INFINITY)).toBe(UI_SCALE_DEFAULT);
    expect(normalizeUiScale(Number.NEGATIVE_INFINITY)).toBe(UI_SCALE_DEFAULT);
  });
});

describe("applyUiScale", () => {
  it("正規化した倍率(%÷100)で setZoom を呼ぶ", async () => {
    await applyUiScale(150);
    expect(setZoomMock).toHaveBeenCalledTimes(1);
    expect(setZoomMock).toHaveBeenCalledWith(1.5);
  });

  it("範囲外の値でも正規化後の倍率で呼ぶ", async () => {
    await applyUiScale(1000);
    expect(setZoomMock).toHaveBeenCalledWith(2);
    await applyUiScale(undefined);
    expect(setZoomMock).toHaveBeenLastCalledWith(1);
  });

  it("モバイルでは setZoom を呼ばない", async () => {
    platformMock.isMobilePlatform = true;
    await applyUiScale(150);
    expect(setZoomMock).not.toHaveBeenCalled();
  });

  it("setZoom が失敗したら例外をそのまま投げる", async () => {
    setZoomMock.mockRejectedValue(new Error("unsupported"));
    await expect(applyUiScale(150)).rejects.toThrow("unsupported");
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/uiScale.test.ts`
Expected: FAIL（`./uiScale`が解決できない）

- [ ] **Step 3: 最小実装**

`frontend/src/lib/uiScale.ts`:

```ts
// UI全体のスケール（Issue #40）。Tauri の Webview::set_zoom（ブラウザのズームと同じ仕組み）で
// 文字・余白・アイコン・画像をまとめて拡縮する。set_zoom は Android 非対応のため、モバイルでは何もしない。
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { isMobilePlatform } from "./platform";

export const UI_SCALE_MIN = 50;
export const UI_SCALE_MAX = 200;
export const UI_SCALE_DEFAULT = 100;
export const UI_SCALE_STEP = 10;

/// 保存値を適用可能な整数%（50〜200）にする。壊れた値（null/NaN/Infinity）は既定の100に倒す。
export function normalizeUiScale(value: number | null | undefined): number {
  if (value == null || !Number.isFinite(value)) return UI_SCALE_DEFAULT;
  return Math.min(UI_SCALE_MAX, Math.max(UI_SCALE_MIN, Math.round(value)));
}

/// WebView のズームを反映する。失敗（未対応環境・IPCエラー）は投げるので、呼び出し側でログする。
export async function applyUiScale(value: number | null | undefined): Promise<void> {
  if (isMobilePlatform) return;
  await getCurrentWebview().setZoom(normalizeUiScale(value) / 100);
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm vitest run src/lib/uiScale.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/uiScale.ts frontend/src/lib/uiScale.test.ts
git commit -m "feat: UIスケールの正規化とWebViewズーム適用を追加

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ストアへの組み込み

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`（import 約40行、`ui`初期値 約175行、`boot()` 約294〜301行、`setUiPrefs` 約1423〜1430行、`#applyAvatarRadius`の直後に`#applyUiScale`）
- Test: `frontend/src/lib/store.svelte.test.ts`（ファイル先頭のモック群と、avatarRadiusのdescribeブロック 約675〜702行の直後）

**Interfaces:**
- Consumes: `applyUiScale(value: number | null | undefined): Promise<void>`（Task 2）、`UiPrefs.uiScale?: number`（Task 1）
- Produces: `app.ui.uiScale`（既定100）。`setUiPrefs`/`boot`が`applyUiScale`を呼ぶ。`applyUiScale`の失敗は`warn`ログにして握りつぶす。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts`のファイル先頭のモック群（`vi.mock("@tauri-apps/api/event", ...)`の直後）に追加:

```ts
const applyUiScaleMock = vi.hoisted(() => vi.fn());
vi.mock("./uiScale", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./uiScale")>()),
  applyUiScale: applyUiScaleMock,
}));
```

avatarRadiusのdescribeブロックの直後（`describe("矢印キー選択移動とselectionMoveSeq(Issue #363)"`の直前）に追加。このブロックのbeforeEach/afterEachの構成（`mockPrefersColorSchemeDark`、`app.ui = { ...app.ui, theme: "auto" }`）を踏襲する:

```ts
describe("UIスケール(Issue #40)", () => {
  beforeEach(() => {
    applyUiScaleMock.mockReset();
    applyUiScaleMock.mockResolvedValue(undefined);
    app.ui = { ...app.ui, theme: "auto" };
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("setUiPrefsでuiScaleを指定するとapplyUiScaleに渡される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, uiScale: 130 });
    expect(applyUiScaleMock).toHaveBeenCalledWith(130);
    expect(app.ui.uiScale).toBe(130);
  });

  it("uiScaleが未設定(旧データ)なら100で適用される", async () => {
    mockPrefersColorSchemeDark(false);
    await app.setUiPrefs({ ...app.ui, uiScale: undefined });
    expect(applyUiScaleMock).toHaveBeenCalledWith(100);
    expect(app.ui.uiScale).toBe(100);
  });

  it("applyUiScaleが失敗しても設定の保存は成功し、警告ログが残る", async () => {
    mockPrefersColorSchemeDark(false);
    applyUiScaleMock.mockRejectedValue(new Error("unsupported"));
    await expect(app.setUiPrefs({ ...app.ui, uiScale: 150 })).resolves.toBeUndefined();
    await vi.waitFor(() => {
      expect(app.logs.some((l) => l.level === "warn" && l.text.includes("UIスケール"))).toBe(true);
    });
  });
});
```

- [ ] **Step 2: 失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts -t "UIスケール"`
Expected: FAIL（`applyUiScaleMock`が呼ばれない）

- [ ] **Step 3: 実装**

`store.svelte.ts`のimport群（`import { BACKGROUND_POSITION_CSS } from "./backgroundPosition";`の直後）に追加:

```ts
import { applyUiScale, UI_SCALE_DEFAULT } from "./uiScale";
```

`ui = $state<UiPrefs>({...})`の初期値の`avatarRadius: 20,`の次の行に追加:

```ts
    uiScale: UI_SCALE_DEFAULT,
```

`boot()`内の`this.ui = {...}`の`avatarRadius: ui.avatarRadius ?? 20,`の次の行に追加:

```ts
        uiScale: ui.uiScale ?? UI_SCALE_DEFAULT,
```

`boot()`内の`this.#applyAvatarRadius(this.ui.avatarRadius ?? 20);`の次の行に追加:

```ts
      this.#applyUiScale(this.ui.uiScale);
```

`setUiPrefs`内の`this.ui = {...}`の`avatarRadius: prefs.avatarRadius ?? 20,`の次の行に追加:

```ts
      uiScale: prefs.uiScale ?? UI_SCALE_DEFAULT,
```

`setUiPrefs`内の`this.#applyAvatarRadius(this.ui.avatarRadius ?? 20);`の次の行に追加:

```ts
    this.#applyUiScale(this.ui.uiScale);
```

`#applyAvatarRadius`メソッドの直後に追加:

```ts
  /// UI全体のスケールを WebView のズームに反映する（Issue #40）。
  /// 未対応環境などで失敗しても、設定の保存や他の反映を止めないよう警告ログだけ残す。
  #applyUiScale(scale: number | undefined) {
    applyUiScale(scale).catch((e) => this.#log("warn", `UIスケールを適用できませんでした: ${formatError(e)}`));
  }
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm vitest run src/lib/store.svelte.test.ts`
Expected: PASS（既存テストも含めて全て）

- [ ] **Step 5: 型チェックと全テスト**

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: 起動時と保存時にUIスケールをWebViewへ反映

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 設定UI・ドキュメント・実機確認

**Files:**
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`（import 約10行、state 約18行、保存 約237行、テンプレート: 「アイコンの丸み」ブロックの直後 約318行）
- Modify: `docs/guide/user-guide.md`（129行目の「外観」の箇条書き）

**Interfaces:**
- Consumes: `UI_SCALE_MIN/MAX/DEFAULT/STEP`（Task 2）、`isMobilePlatform`（`lib/platform.ts`）、`app.setUiPrefs`（Task 3）
- Produces: 設定画面の「UIスケール(N%)」スライダーと「100%に戻す」ボタン（モバイルでは非表示）

- [ ] **Step 1: 設定UIを実装する**

`AppearanceSection.svelte`のimport群（`customCss`のimportの直後）に追加:

```ts
  import { UI_SCALE_DEFAULT, UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP } from "../../lib/uiScale";
  import { isMobilePlatform } from "../../lib/platform";
```

`let avatarRadius = $state(app.ui.avatarRadius ?? 20);`の次の行に追加:

```ts
  let uiScale = $state(app.ui.uiScale ?? UI_SCALE_DEFAULT);
```

保存処理の`setUiPrefs({...})`内、`avatarRadius,`の次の行に追加:

```ts
        uiScale,
```

テンプレートの「アイコンの丸み」の説明文（`0%が直角、100%が真円です。`を含む`<p>`）の直後、`{#snippet swatchStrip`の直前に追加:

```svelte
{#if !isMobilePlatform}
  <label class="mb-2.5 flex flex-col gap-1 text-sm">
    <span class="text-muted-foreground">UIスケール({uiScale}%)</span>
    <span class="flex items-center gap-2">
      <input
        class="w-full max-w-[320px] accent-primary"
        type="range"
        min={UI_SCALE_MIN}
        max={UI_SCALE_MAX}
        step={UI_SCALE_STEP}
        bind:value={uiScale}
      />
      <Button type="button" variant="outline" size="sm" onclick={() => (uiScale = UI_SCALE_DEFAULT)}>
        100%に戻す
      </Button>
    </span>
  </label>
  <p class="mb-4 mt-0 text-xs text-muted-foreground">
    文字・余白・アイコン・画像を含むUI全体の大きさを変えます。保存すると反映されます。
    拡大すると同時に表示できるカラム数は減ります。
  </p>
{/if}
```

- [ ] **Step 2: ドキュメントを更新する**

`docs/guide/user-guide.md`の129行目、「アイコンの丸み（0～100%。アバター画像の角丸。既定20%）、」の直後に次を挿入:

```
UIスケール（50～200%。文字・余白・アイコン・画像を含むUI全体の拡大縮小。既定100%。デスクトップ版のみ）、
```

- [ ] **Step 3: 型チェックと全テスト**

Run: `cd frontend && pnpm check && pnpm test && cd ../src-tauri && cargo test`
Expected: PASS

- [ ] **Step 4: 実UIで確認する（省略しない）**

`feedback-dev-server-verification-must-use-virtual-display`と`tauri-single-instance-blocks-xdg-isolated-verification`に従い、ユーザーの実画面・実データに触れない隔離環境で起動する。リポジトリルートから`dbus-run-session`配下で、`DISPLAY`をXvfbに向け、`WAYLAND_DISPLAY`をunsetし、`XDG_*`の設定/データ/キャッシュディレクトリを一時ディレクトリに向けて`cargo tauri dev`を起動する。起動したプロセスのPIDを控える。デバッグブリッジ（`curl --unix-socket <app_cache_dir>/debug-bridge.sock http://localhost/ --data-binary '<js>'`、本文は`return ...`形式、応答は2回デコード）で以下を確認する:

1. 設定画面「外観」に「UIスケール(100%)」スライダーが表示される。
2. 150%にして保存 → `window.innerWidth`が保存前の約2/3になる（ズームでCSSピクセル幅が縮む）。`devicePixelRatio`が約1.5倍になる。
3. ポップオーバー（リアクションピッカー、ドロップダウン）が150%でも正しい位置に開く。
4. アプリを終了して同じ隔離環境で再起動 → 起動後に150%のまま（`devicePixelRatio`で確認）。
5. 「100%に戻す」→保存 → 等倍に戻る。
6. 50%と200%の両端でレイアウトが極端に崩れない（目視・スクリーンショット）。

確認後、自分で起動した`cargo tauri dev`/Xvfb/dbus-run-sessionを、控えたPIDだけを指定して終了する（`pkill`/`killall`は使わない）。隔離用の一時ディレクトリを削除する。

結果（確認できた点と、できなかった点）を報告する。macOS/Windowsは実機が無いため未確認と明記する。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/settings/AppearanceSection.svelte docs/guide/user-guide.md
git commit -m "feat: 外観設定にUIスケールのスライダーを追加

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-Review（作成者メモ）

- **Spec coverage**: 設定値（Task 1）、正規化と適用・モバイル除外・失敗時の扱い（Task 2・3）、capability（Task 1）、設定UI・モバイル非表示（Task 4）、テスト方針の各層（Task 1〜3のテスト、Task 4の実UI確認）、ドキュメント（Task 4）に対応。スコープ外（ショートカット、ライブプレビュー、カラム幅補正、Android）は計画に含めていない。
- **Placeholder scan**: TBD/TODOなし。
- **Type consistency**: `normalizeUiScale`/`applyUiScale`/`UI_SCALE_*`は Task 2 の定義と Task 3・4 の使用で一致。`UiPrefs.uiScale?: number`は Task 1 の生成バインディングと一致（`#[serde(default)]`のためTS側はoptional）。
- **既知のリスク**: Task 4 の実機確認が最も不確か（WebKitGTKでの`setZoom`実挙動）。`store.svelte.test.ts`の`vi.mock("./uiScale")`はファイル全体に効くが、既存テストは`uiScale`を参照しないため影響しない。
