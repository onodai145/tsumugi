# カスタムCSS Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定画面で入力した任意のCSSをアプリ全体へ適用し、`TSUMUGI_SAFE_MODE` 環境変数で適用を止められるようにする（Issue #93）。

**Architecture:** `UiPrefs` に `custom_css: String` を追加して既存の `get_ui_prefs` / `set_ui_prefs` 経路で永続化する。フロントは `lib/customCss.ts` の純関数で `<head>` 末尾の `<style id="tsumugi-custom-css">` を差し替える。セーフモードは Rust の新コマンド `is_safe_mode` が環境変数から判定し、フロントは適用を止めるだけで保存値は消さない。

**Tech Stack:** Rust (Tauri v2, serde, tauri-specta), Svelte 5 + TypeScript, Vitest (jsdom)

**Spec:** `docs/superpowers/specs/2026-09-29-issue-93-custom-css-design.md`

## Global Constraints

- 作業ブランチは `feat/93-custom-css`。`main` へ直接コミットしない。
- コミットメッセージは件名1行のみ（本文・箇条書き禁止）。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` / `--no-gpg-sign` は使わない。`git commit` が失敗・タイムアウトしたら止めて報告し、リトライしない。
- `frontend/src/bindings/tauri.gen.ts` は生成物。手で編集せず `cd src-tauri && cargo test` で再生成し、差分をコミットする。
- 新コマンドは `lib.rs` の `specta_builder()` の `collect_commands![]` に登録する。
- CSP（`tauri.conf.json` の `csp: null`）は変更しない。CSSの検証・サニタイズ・`@import`/`url()` 制限はしない。
- 環境変数名は `TSUMUGI_SAFE_MODE`。設定済みで、空でも `0` でもなければ true。
- セーフモードは適用を止めるだけで `customCss` の保存値は消さない。
- 新規UIの値は `docs/design/style-guide.md` のスケールに従い、`rounded-[Npx]` などの即値を増やさない。
- 実UI確認は Xvfb 越し（`WAYLAND_DISPLAY` も unset、`dbus-run-session` 必須）。`cargo tauri dev` はリポジトリルートから実行し、起動したものは完了前に PID 指定で kill する（`pkill`/`killall` 禁止）。
- ユーザーの `cargo tauri dev` が動いていると作業ツリーの変更が実データに対してホットリビルドされる。本変更は追加のみで破壊的なマイグレーションは無い。

## Review Focus

- CSS文字列に `</style>` や `<script>` を含む: `textContent` で入れるので `<style>` 要素の外へ出ない（Task 2 のテストで確認）。
- 空文字・空白のみ: 空文字は要素を空にして元の見た目に戻る（Task 2）。
- `customCss` を持たない旧設定JSON: 空文字として読める（Task 1）。
- `TSUMUGI_SAFE_MODE` が `0` や空: セーフモードにならない（Task 1）。
- セーフモード中に他の設定を保存: `customCss` が消えない（Task 2 の `app.ui` 保持、Task 3 の手動確認）。

---

### Task 1: Rust — `customCss` フィールドと `is_safe_mode` コマンド

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`（struct 定義 / `Default` / tests）
- Modify: `src-tauri/src/commands/app.rs`（コマンドと tests）
- Modify: `src-tauri/src/lib.rs:27` 付近（`collect_commands!` へ登録）
- Regenerate: `frontend/src/bindings/tauri.gen.ts`

**Interfaces:**
- Produces: `UiPrefs.custom_css: String`（TS では `customCss?: string`）、Tauri コマンド `is_safe_mode() -> bool`（TS では `commands.isSafeMode(): Promise<boolean>`）。

- [ ] **Step 1: 失敗するテストを書く（`ui.rs`）**

`src-tauri/src/domain/ui.rs` の `mod tests` 内、`deserializes_legacy_json_without_new_fields` の直後に追加する。

```rust
    #[test]
    fn custom_css_defaults_to_empty_for_legacy_json() {
        // customCss（Issue #93）追加前に保存された JSON も空文字として読めること。
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.custom_css, "");
    }

    #[test]
    fn custom_css_roundtrips_as_camel_case() {
        let mut p = UiPrefs::default();
        p.custom_css = ".note { margin: 0 }\n/* </style> */".into();
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"customCss\":"));
        let back: UiPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(back.custom_css, p.custom_css);
    }
```

- [ ] **Step 2: `app.rs` にテストを書く**

`src-tauri/src/commands/app.rs` の `mod tests` 末尾（`get_pending_share_is_none_by_default` の後）に追加する。

```rust
    #[test]
    fn safe_mode_from_env_is_false_when_unset_empty_or_zero() {
        assert!(!safe_mode_from_env(None));
        assert!(!safe_mode_from_env(Some("")));
        assert!(!safe_mode_from_env(Some("0")));
    }

    #[test]
    fn safe_mode_from_env_is_true_for_any_other_value() {
        assert!(safe_mode_from_env(Some("1")));
        assert!(safe_mode_from_env(Some("true")));
        assert!(safe_mode_from_env(Some("yes")));
    }
```

- [ ] **Step 3: テストが失敗（コンパイルエラー）することを確認**

Run: `cd src-tauri && cargo test --lib -- custom_css_ safe_mode_from_env 2>&1 | tail -20`
Expected: FAIL（`no field custom_css` / `cannot find function safe_mode_from_env`）

- [ ] **Step 4: `UiPrefs` にフィールドを追加する**

`src-tauri/src/domain/ui.rs` の struct 末尾、`developer_options_enabled` の直後に追加する。

```rust
    /// ユーザーが書いた任意のCSS。アプリ全体に適用する（Issue #93）。空文字なら何も適用しない。
    /// 環境変数 `TSUMUGI_SAFE_MODE` で起動した場合は保存値を保ったまま適用だけを止める
    /// （`commands::app::is_safe_mode` 参照）。
    #[serde(default)]
    pub custom_css: String,
```

`impl Default for UiPrefs` の `developer_options_enabled: false,` の直後に追加する。

```rust
            custom_css: String::new(),
```

既存テスト `roundtrips_keymap` の `UiPrefs { ... }` リテラルにも、`developer_options_enabled: true,` の直後へ追加する（無いとコンパイルできない）。

```rust
            custom_css: "body { background: #000 }".into(),
```

- [ ] **Step 5: `is_safe_mode` を実装する**

`src-tauri/src/commands/app.rs` の `get_pending_share` の後、`#[cfg(test)]` の前に追加する。

```rust
/// 環境変数 `TSUMUGI_SAFE_MODE` の値からセーフモードかを判定する。未設定・空・"0" 以外なら true。
/// 環境変数の読み取りと切り離してあるのは、プロセス共有の環境変数を並列テストから触らないため。
fn safe_mode_from_env(value: Option<&str>) -> bool {
    matches!(value, Some(v) if !v.is_empty() && v != "0")
}

/// セーフモード（カスタムCSSを適用しない）で起動しているか（Issue #93）。
/// CSSで設定画面を操作不能にした場合の復旧手段で、`TSUMUGI_SAFE_MODE=1` を付けて起動する。
/// Androidは環境変数を渡せないため常に false。
#[tauri::command]
#[specta::specta]
pub fn is_safe_mode() -> bool {
    safe_mode_from_env(std::env::var("TSUMUGI_SAFE_MODE").ok().as_deref())
}
```

- [ ] **Step 6: コマンドを登録する**

`src-tauri/src/lib.rs` の `commands::app::get_pending_share,` の直後に追加する。

```rust
            commands::app::is_safe_mode,
```

- [ ] **Step 7: テストを通し、バインディングを再生成する**

Run: `cd src-tauri && cargo test 2>&1 | tail -30`
Expected: PASS（`generates_frontend_bindings` により `frontend/src/bindings/tauri.gen.ts` が更新される）

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi && git diff --stat frontend/src/bindings/tauri.gen.ts && grep -n "customCss\|isSafeMode" frontend/src/bindings/tauri.gen.ts`
Expected: `customCss?: string` と `isSafeMode: () => __TAURI_INVOKE<boolean>("is_safe_mode")` が出る。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/domain/ui.rs src-tauri/src/commands/app.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: カスタムCSS用のUiPrefs.customCssとis_safe_modeコマンドを追加"
```

（Co-Authored-By トレーラーは Global Constraints のとおり付ける。）

---

### Task 2: フロント — `applyCustomCss` と store 配線

**Files:**
- Create: `frontend/src/lib/customCss.ts`
- Create: `frontend/src/lib/customCss.test.ts`
- Modify: `frontend/src/lib/store.svelte.ts`（`ui` 組み立て2箇所、`boot()`、`setUiPrefs()`、`#applyCustomCss` 追加、`safeMode` 状態追加）

**Interfaces:**
- Consumes: Task 1 の `UiPrefs.customCss?: string` と `commands.isSafeMode(): Promise<boolean>`。
- Produces: `applyCustomCss(css: string, safeMode: boolean): void`、`app.safeMode: boolean`（Task 3 の UI が警告表示に使う）。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/customCss.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { applyCustomCss } from "./customCss";

const STYLE_ID = "tsumugi-custom-css";
const styleEl = () => document.getElementById(STYLE_ID);

afterEach(() => {
  styleEl()?.remove();
  document.head.querySelectorAll("style[data-test]").forEach((e) => e.remove());
});

describe("applyCustomCss", () => {
  it("CSSを <style id> として <head> に作成する", () => {
    applyCustomCss(".note { margin: 0 }", false);
    const el = styleEl();
    expect(el).not.toBeNull();
    expect(el?.tagName).toBe("STYLE");
    expect(el?.parentElement).toBe(document.head);
    expect(el?.textContent).toBe(".note { margin: 0 }");
  });

  it("再適用しても要素は1つのままで内容だけ差し替わる", () => {
    applyCustomCss("a { color: red }", false);
    applyCustomCss("a { color: blue }", false);
    expect(document.querySelectorAll(`#${STYLE_ID}`)).toHaveLength(1);
    expect(styleEl()?.textContent).toBe("a { color: blue }");
  });

  it("空文字なら内容を空にする", () => {
    applyCustomCss("a { color: red }", false);
    applyCustomCss("", false);
    expect(styleEl()?.textContent).toBe("");
  });

  it("セーフモードなら適用せず、解除されれば復元できる", () => {
    applyCustomCss("a { color: red }", true);
    expect(styleEl()?.textContent ?? "").toBe("");
    applyCustomCss("a { color: red }", false);
    expect(styleEl()?.textContent).toBe("a { color: red }");
  });

  it("後から追加された <style> があっても、再適用で <head> の末尾に来る", () => {
    applyCustomCss("a { color: red }", false);
    const other = document.createElement("style");
    other.dataset.test = "1";
    document.head.appendChild(other);
    applyCustomCss("a { color: blue }", false);
    expect(document.head.lastElementChild).toBe(styleEl());
  });

  it("</style> や <script> を含んでも <style> の外へ出ない", () => {
    const css = "/* </style><script>window.__pwned = 1</script> */ a { color: red }";
    const before = document.head.children.length;
    applyCustomCss(css, false);
    expect(styleEl()?.textContent).toBe(css);
    // 追加された要素は <style> 1つだけ
    expect(document.head.children.length).toBe(before + 1);
    expect(document.head.querySelector("script[src], script:not([type])")).toBeNull();
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && pnpm exec vitest run src/lib/customCss.test.ts 2>&1 | tail -15`
Expected: FAIL（`./customCss` が解決できない）

- [ ] **Step 3: 最小実装**

`frontend/src/lib/customCss.ts`:

```ts
// カスタムCSS機能（Issue #93）。UiPrefs.customCss を <head> 末尾の <style> へ反映する。
// textContent 経由なので、CSS内の "</style>" 等が <style> の外へ漏れることは無い。
const STYLE_ID = "tsumugi-custom-css";

export function applyCustomCss(css: string, safeMode: boolean): void {
  let el = document.getElementById(STYLE_ID);
  if (!el) {
    el = document.createElement("style");
    el.id = STYLE_ID;
  }
  // 常に <head> の末尾へ移す。app.css や後から挿入された <style> よりも後ろに置き、
  // 同じ詳細度ならユーザーCSSが勝つようにする。
  document.head.appendChild(el);
  el.textContent = safeMode ? "" : css;
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && pnpm exec vitest run src/lib/customCss.test.ts 2>&1 | tail -15`
Expected: PASS（6 tests）

- [ ] **Step 5: store に配線する**

`frontend/src/lib/store.svelte.ts`:

1. import に追加（`import { applyThemeColors, ... } from "./theme";` の直後）:

```ts
import { applyCustomCss } from "./customCss";
```

2. `ui = $state<UiPrefs>({` の直前に状態を追加:

```ts
  // TSUMUGI_SAFE_MODE で起動しているか。true の間はカスタムCSSを適用しない（Issue #93）。
  safeMode = $state(false);
```

3. `boot()` 内、`const ui = await unwrap(commands.getUiPrefs());` の直後に追加:

```ts
      this.safeMode = await commands.isSafeMode().catch(() => false);
```

4. `boot()` の `this.ui = { ...ui, ... }` 内、`avatarRadius: ui.avatarRadius ?? 20,` の直後に追加:

```ts
        customCss: ui.customCss ?? "",
```

同ブロックの `this.#applyAvatarRadius(this.ui.avatarRadius ?? 20);` の直後（`this.#applyMediaThumbnailHeight(...)` の前後どちらでもよい）に追加:

```ts
      this.#applyCustomCss();
```

5. `setUiPrefs()` 内、`avatarRadius: prefs.avatarRadius ?? 20,` の直後に追加:

```ts
      customCss: prefs.customCss ?? "",
```

同関数の `this.#applyMediaThumbnailHeight(this.ui.mediaThumbnailHeight ?? 200);` の直後に追加:

```ts
    this.#applyCustomCss();
```

6. `#applyAvatarRadius` メソッドの直後にメソッドを追加:

```ts
  /// カスタムCSSを <head> の <style> に反映する（Issue #93）。セーフモード中は適用しない。
  #applyCustomCss() {
    applyCustomCss(this.ui.customCss ?? "", this.safeMode);
  }
```

- [ ] **Step 6: 型チェックと全 Vitest**

Run: `cd frontend && pnpm check 2>&1 | tail -15 && pnpm test 2>&1 | tail -15`
Expected: エラー 0、既存テストを含めて PASS

- [ ] **Step 7: Commit**

```bash
git add frontend/src/lib/customCss.ts frontend/src/lib/customCss.test.ts frontend/src/lib/store.svelte.ts
git commit -m "feat: カスタムCSSを<head>へ適用するapplyCustomCssとstore配線を追加"
```

---

### Task 3: 設定UI・ドキュメント・実UI確認

**Files:**
- Modify: `frontend/src/ui/settings/AppearanceSection.svelte`
- Modify: `docs/guide/user-guide.md`（「設定画面」の外観の行、「トラブルシューティング」）

**Interfaces:**
- Consumes: `app.safeMode`、`app.ui.customCss`、`app.setUiPrefs`（Task 2）。

- [ ] **Step 1: UI に状態と入力欄を追加する**

`AppearanceSection.svelte` の `let avatarRadius = $state(app.ui.avatarRadius ?? 20);` の直後に追加:

```ts
  let customCss = $state(app.ui.customCss ?? "");
```

`save()` 内の `avatarRadius,` の直後に追加:

```ts
        customCss,
```

テンプレート末尾（ファイルの最後の要素の後）に追加する。既存の最後のブロックの書式に合わせ、値は `border-border` / `bg-background` / `text-muted-foreground` など既存トークンのみ使う。

```svelte
<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">カスタムCSS</span>
  {#if app.safeMode}
    <p class="m-0 rounded-md border border-border bg-muted px-2.5 py-2 text-sm text-[var(--warning)]">
      セーフモードで起動中のため、カスタムCSSは適用されていません(編集・保存はできます)。
    </p>
  {/if}
  <textarea
    class="min-h-[160px] w-full rounded-md border border-border bg-background px-2.5 py-[7px] font-[ui-monospace,monospace] text-sm text-foreground"
    spellcheck="false"
    placeholder={'/* 例: ノート本文の文字サイズを変える */\n[data-testid="note-text"] { font-size: 15px; }'}
    bind:value={customCss}
  ></textarea>
  <p class="m-0 text-xs text-muted-foreground">
    アプリ全体に適用されます。「保存」を押すと反映されます。
    CSSで画面が操作できなくなった場合は、環境変数 <code>TSUMUGI_SAFE_MODE=1</code> を付けて起動するとカスタムCSSが適用されません(Androidでは使えません)。
  </p>
</div>
```

> 注: placeholder の例セレクタ `[data-testid="note-text"]` は `NoteCard.svelte` に実在する（`data-testid` はテスト用属性で、将来変わりうる。例示に留める）。

- [ ] **Step 2: user-guide を更新する**

`docs/guide/user-guide.md` の「設定画面」の外観の行（`- **外観**: ...` の末尾の `MFMアニメーション（...のON/OFF）。` の直後）に追記する:

```
カスタムCSS（任意のCSSをアプリ全体に適用。Tailwindの各クラスと同じ詳細度のルールは、より具体的なセレクタか `!important` を使わないと上書きできない場合があります）。
```

「トラブルシューティング」節の末尾に小節を追加する:

```
### カスタムCSSで画面が操作できなくなった

環境変数 `TSUMUGI_SAFE_MODE=1` を付けて起動すると、カスタムCSSを適用せずに起動します（保存済みのCSSは消えません）。設定画面の「外観」→「カスタムCSS」で内容を直して保存し、通常どおり再起動してください。

```sh
TSUMUGI_SAFE_MODE=1 tsumugi
```

Androidでは環境変数を渡せないため、この方法は使えません。復旧するにはアプリのデータを消去するか、再インストールしてください。
```

- [ ] **Step 3: 静的チェック**

Run: `cd frontend && pnpm check 2>&1 | tail -15 && pnpm test 2>&1 | tail -8`
Expected: エラー 0、PASS

- [ ] **Step 4: 実UI確認（Xvfb 越し）**

`cargo tauri dev` はリポジトリルートから、`Xvfb` + `dbus-run-session` 配下で、`WAYLAND_DISPLAY` を unset して起動する（`CLAUDE.md` と作業ルール参照。実画面へ漏らさない）。起動した Xvfb / dbus / tauri / vite の PID は控えておく。

確認項目（debug bridge か Xvfb のスクリーンショットで確認）:
1. 設定 → 外観 → カスタムCSSに `body { outline: 4px solid red }` を入れて保存 → 即座に反映される。
2. アプリを再起動しても保持される。
3. 空にして保存 → 元に戻る。
4. `TSUMUGI_SAFE_MODE=1` で起動 → 適用されず、警告が出る。この状態で外観の別項目を保存しても、再起動（通常）でCSSが残っている。

- [ ] **Step 5: 起動したプロセスを PID 指定で終了する**

`ps aux` で自分が起動した Xvfb / dbus-run-session / tauri / vite の PID だけを確認して `kill <pid>`。`pkill` / `killall` は使わない。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/settings/AppearanceSection.svelte docs/guide/user-guide.md
git commit -m "feat: カスタムCSSの設定UIとユーザーガイドを追加"
```

---

## Self-Review

- Spec coverage: データ(Task 1)、セーフモード判定(Task 1)、適用(Task 2)、UI・警告(Task 3)、テスト(各Task)、ドキュメント(Task 3)、既知の制限=Android(Task 3 のガイド)、範囲外は実装しない。
- 型の一貫性: `customCss`、`isSafeMode`、`applyCustomCss(css, safeMode)`、`app.safeMode` を全Taskで同名で使用。
- プレースホルダ: なし（Task 3 の例セレクタは `NoteCard.svelte` の実在属性で確認済み）。
