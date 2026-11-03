# IME 変換候補ウィンドウの位置補正 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Linux の Wayland 環境で、IME の変換候補ウィンドウが入力位置のすぐ下に出るよう、フロントで求めた実キャレット座標を WebKitGTK の IM コンテキストに上書きして渡す。

**Architecture:** フロントの `caretRect.ts`(DOM から viewport 座標のキャレット矩形を求める純関数)と `imeCursorArea.ts`(`document` のイベントを監視して間引き・遅延して送る)で矩形を求め、Rust の薄いコマンド `set_ime_cursor_area` に渡す。Rust は Wayland セッションのときだけ、webview の `zoom_level()` を掛けてウィジェット座標に換算し、`notify_cursor_area` を呼ぶ。既存の入力欄コンポーネントは改修しない。

**Tech Stack:** Svelte 5 + TypeScript、Vitest + jsdom、Rust(Tauri v2、`webkit2gtk` 2.0.2、tauri-specta)。

**Spec:** `docs/superpowers/specs/2026-10-19-issue-502-ime-cursor-area-design.md`

## Global Constraints

- 上書きは Linux かつ Wayland セッションのときだけ。判定は `WAYLAND_DISPLAY` が設定済み(空でない)で、かつ `GDK_BACKEND` をカンマで分けた先頭が `x11` でないこと。起動後 1 回だけ判定してキャッシュする。
- `set_ime_cursor_area` は全 OS に存在させ、Linux 以外は no-op(`Ok(())`)。生成される TS バインディングを OS で変えない。
- Rust は受け取った CSS px の座標に `webview.zoom_level()` を掛けて整数に丸め、`notify_cursor_area` に渡す(UI スケールは `Webview::set_zoom` で実現されているため)。
- 送信は「対象イベント → `requestAnimationFrame` → 30ms 待つ → その時点のキャレット矩形を求めて送信」。待機中の追加イベントは 1 回の送信にまとめる(待機は延長しない)。**同じ矩形でも毎回送る**(重複排除しない。WebKit が編集のたびに自分の座標を送り直すため)。
- 対象イベント: `focusin`、`input`、`compositionstart` / `compositionupdate` / `compositionend`、`keyup`、`mouseup`、`selectionchange`、`scroll`(capture)、`resize`。
- 対象要素: `input`(type が text / search / url / email / password / tel / number)、`textarea`、contenteditable。それ以外にフォーカスがあるときは何もしない。
- 変換中のアンカーは `compositionstart` で記録した位置(text 系は `selectionStart`、contenteditable は選択範囲の先頭に畳んだ Range)。それ以外は現在のキャレット(`selectionEnd` / 選択範囲の末尾)。
- フロントのリスナーは DOM を読むだけ。`preventDefault` は呼ばず、passive にし、全体を `try/catch` で囲む。NaN・無限大の矩形は送らない。値は整数に丸める(負の値は許容)。`invoke` の失敗は握りつぶす。
- 既存の入力欄コンポーネントと、PR #18 の preedit 再有効化(`lib.rs` の `set_enable_preedit(true)`)は変更しない。
- 変換中の下線のズレ(#502 の症状②)は直さない。#502 はクローズしない。
- 新規コマンドの追加で `specta_builder()` への登録が必要(`tauri::Builder` だけでは不足)。`frontend/src/bindings/tauri.gen.ts` は手編集せず、`cargo test` で再生成する。
- コミットメッセージは件名のみ(本文・箇条書きなし)。ハーネスの指示どおり末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` トレーラーを付ける。`--no-verify` は使わない。`git commit` が失敗・タイムアウトしたら止めてユーザーに報告する(再試行しない)。
- 作業ブランチは `fix/issue-502-ime-candidate-position`(作成済み)。`main` には触らない。
- 以降のコマンドは絶対パスで書く。リポジトリルートは `/home/onodai145/repos/github.com/onodai145/tsumugi`。
- `cargo tauri dev` はリポジトリルートから実行する。`./target/debug/tsumugi` や `cargo run` は直接実行しない。

## Review Focus

spec が暗黙に求めているが、各タスクの主テストが直接は検証しない入力・条件。それぞれ、担当タスクにテストを追加してある。

1. 空の入力欄(キャレット位置 0)でも矩形が出る → Task 2。
2. `type="email"` / `"number"` など `selectionEnd` が `null` の入力欄でも落ちず、入力欄の左下にフォールバックする → Task 2。
3. 文章の途中で変換を始め、変換中に DOM のキャレットが動いても、アンカーは変換開始位置に固定される → Task 3。
4. 選択範囲がある状態で変換を始める(選択を置き換える変換)と、アンカーは選択の先頭になる → Task 3。
5. 送信側(`send`)が例外を投げても、イベントリスナーから例外が漏れず、タイピングを妨げない → Task 3。

---

### Task 1: Rust コマンド `set_ime_cursor_area`

**Files:**
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/commands/ime.rs`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/commands/mod.rs`(`pub mod haptics;` の次の行に `pub mod ime;` を追加)
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/lib.rs`(`commands::haptics::vibrate,` の次の行にコマンドを登録)
- Regenerate: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/bindings/tauri.gen.ts`(`cargo test` で再生成。手編集しない)

**Interfaces:**
- Consumes: `crate::error::{Error, Result}`(`Error::Invalid(String)` を使う)。`tauri::WebviewWindow::with_webview`。
- Produces:
  - Rust: `pub fn set_ime_cursor_area(window: tauri::WebviewWindow, x: i32, y: i32, width: i32, height: i32) -> Result<()>`
  - 生成される TS: `commands.setImeCursorArea(x: number, y: number, width: number, height: number): Promise<Result<null, Error>>`(Task 3 が使う)
  - 非公開の純関数 `is_wayland_session(wayland_display: Option<&str>, gdk_backend: Option<&str>) -> bool` と `scale_by_zoom(value: i32, zoom: f64) -> i32`。

- [ ] **Step 1: 失敗するテストを書く**

`ime.rs` を作り、まずテストだけを書く。`pub mod ime;` も `mod.rs` に足す。

`/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/commands/ime.rs`:

```rust
//! IME の変換候補ウィンドウの位置補正(Issue #502)。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_when_display_set_and_backend_unset() {
        assert!(is_wayland_session(Some("wayland-1"), None));
    }

    #[test]
    fn not_wayland_when_display_unset() {
        assert!(!is_wayland_session(None, None));
    }

    #[test]
    fn not_wayland_when_display_empty() {
        assert!(!is_wayland_session(Some(""), None));
    }

    #[test]
    fn not_wayland_when_backend_forced_x11() {
        assert!(!is_wayland_session(Some("wayland-1"), Some("x11")));
    }

    #[test]
    fn not_wayland_when_x11_is_first_in_backend_list() {
        assert!(!is_wayland_session(Some("wayland-1"), Some("x11,wayland")));
    }

    #[test]
    fn wayland_when_wayland_is_first_in_backend_list() {
        assert!(is_wayland_session(Some("wayland-1"), Some("wayland,x11")));
    }

    #[test]
    fn scale_rounds_to_nearest() {
        assert_eq!(scale_by_zoom(100, 1.0), 100);
        assert_eq!(scale_by_zoom(100, 1.25), 125);
        assert_eq!(scale_by_zoom(10, 0.5), 5);
        assert_eq!(scale_by_zoom(3, 1.5), 5);
    }

    #[test]
    fn scale_keeps_negative_values() {
        assert_eq!(scale_by_zoom(-10, 2.0), -20);
    }

    #[test]
    fn scale_ignores_invalid_zoom() {
        assert_eq!(scale_by_zoom(10, 0.0), 10);
        assert_eq!(scale_by_zoom(10, -1.0), 10);
        assert_eq!(scale_by_zoom(10, f64::NAN), 10);
        assert_eq!(scale_by_zoom(10, f64::INFINITY), 10);
    }
}
```

`/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/commands/mod.rs` の `pub mod haptics;` の次に追加:

```rust
pub mod ime;
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib ime:: 2>&1 | tail -20`
Expected: コンパイルエラー(`cannot find function is_wayland_session` / `scale_by_zoom`)。

- [ ] **Step 3: 実装を書く**

`ime.rs` の先頭(`#[cfg(test)]` の前)に、モジュールコメントを置き換える形で次を書く。

```rust
//! IME の変換候補ウィンドウの位置補正(Issue #502)。
//!
//! Linux/Wayland では、WebKitGTK が IM(fcitx5 等)に渡すキャレット位置が不正確で、変換候補が
//! 入力位置からずれる。素の WebKitGTK アプリ(yad)でも再現するため tsumugi/wry の外の問題だが、
//! `webkit_input_method_context_notify_cursor_area` で座標を上書きすれば候補位置を直せる。
//! フロント(`lib/imeCursorArea.ts`)が求めた実キャレット座標を、ここで IM コンテキストに渡す。
//! WebKit は編集状態が変わるたびに自分の座標を送り直すので、フロントが遅らせて上書きする。

use crate::error::Result;

/// Wayland セッションかどうか。X11 では WebKit 本来の座標が正しい可能性があり未検証なので、
/// Wayland のときだけ上書きする。`GDK_BACKEND` は "wayland,x11" のようなカンマ区切りの優先順で、
/// 先頭が `x11` なら GTK は X11 で動く。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_wayland_session(wayland_display: Option<&str>, gdk_backend: Option<&str>) -> bool {
    let has_display = wayland_display.is_some_and(|v| !v.is_empty());
    let x11_first =
        gdk_backend.is_some_and(|v| v.split(',').next().is_some_and(|b| b.trim() == "x11"));
    has_display && !x11_first
}

/// CSS px を webview のズーム倍率でウィジェット座標(px)に換算する。UI スケール(Issue #40)は
/// `Webview::set_zoom` で実現されており、ズーム時は CSS px とウィジェット座標が食い違う。
/// 不正なズーム(0 以下・非有限)は換算せずそのまま返す。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn scale_by_zoom(value: i32, zoom: f64) -> i32 {
    if !zoom.is_finite() || zoom <= 0.0 {
        return value;
    }
    (f64::from(value) * zoom).round() as i32
}

#[cfg(target_os = "linux")]
fn wayland_session() -> bool {
    use std::sync::OnceLock;
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        is_wayland_session(
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
            std::env::var("GDK_BACKEND").ok().as_deref(),
        )
    })
}

/// IME の変換候補の位置(キャレット矩形)を上書きする。座標は viewport 座標(CSS px)。
/// Linux かつ Wayland セッションのときだけ動き、それ以外は何もしない。
/// 失敗しても入力には影響しないため、呼び出し側(フロント)で握りつぶす想定。
#[tauri::command]
#[specta::specta]
pub fn set_ime_cursor_area(
    window: tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    if wayland_session() {
        window
            .with_webview(move |w| {
                use webkit2gtk::{InputMethodContextExt, WebViewExt};
                let webview = w.inner();
                if let Some(input_context) = webview.input_method_context() {
                    let zoom = webview.zoom_level();
                    input_context.notify_cursor_area(
                        scale_by_zoom(x, zoom),
                        scale_by_zoom(y, zoom),
                        scale_by_zoom(width, zoom),
                        scale_by_zoom(height, zoom),
                    );
                }
            })
            .map_err(|e| crate::error::Error::Invalid(e.to_string()))?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (window, x, y, width, height);
    Ok(())
}
```

`/home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri/src/lib.rs` の `specta_builder()` 内、`commands::haptics::vibrate,` の次の行に追加:

```rust
            commands::ime::set_ime_cursor_area,
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib ime:: 2>&1 | tail -20`
Expected: `test result: ok. 9 passed`(警告が出ないこと)。

- [ ] **Step 5: TS バインディングを再生成して確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri && cargo test --lib generates_frontend_bindings 2>&1 | tail -8`
Expected: `1 passed`。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi && git diff frontend/src/bindings/tauri.gen.ts`
Expected: `setImeCursorArea: (x: number, y: number, width: number, height: number) => ...` の行が 1 つ増えるだけ。他の差分があれば止めて報告する。

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add src-tauri/src/commands/ime.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: IME の変換候補の位置を上書きする set_ime_cursor_area コマンドを追加(Issue #502)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: キャレット矩形を求める `caretRect.ts`(`<input>` 対応を含む)

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/caretPosition.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/caretPosition.test.ts`
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/caretRect.ts`
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/caretRect.test.ts`

**Interfaces:**
- Consumes: `getCaretCoordinates(el, position)`(`caretPosition.ts`。このタスクで `HTMLInputElement` も受けるよう拡張)。
- Produces(Task 3 が使う):
  - `export interface CaretRect { x: number; y: number; width: number; height: number }`
  - `export type CaretAnchor = number | Range | null`
  - `export function isImeTarget(el: Element | null): el is HTMLElement`
  - `export function caretRect(el: HTMLElement, anchor?: CaretAnchor): CaretRect | null`

- [ ] **Step 1: `caretPosition` の `<input>` 対応の失敗するテストを書く**

`caretPosition.test.ts` の末尾(`describe("getCaretCoordinates", ...)` の中、最後の `it` の後)に追加する。

```ts
  it("accepts an HTMLInputElement and leaves no mirror behind", () => {
    const input = document.createElement("input");
    input.value = "hello world";
    document.body.appendChild(input);
    const coords = getCaretCoordinates(input, 5);
    expect(Number.isNaN(coords.left)).toBe(false);
    expect(document.getElementById("mfm-completion-caret-mirror")).toBeNull();
  });
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/caretPosition.test.ts 2>&1 | tail -15`
Expected: 実行時は通る可能性がある(jsdom は型を見ない)。その場合も次の Step 3 の型変更前に `pnpm check` が型エラーになることで、失敗を確認する。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -15`
Expected: `caretPosition.test.ts` で `Argument of type 'HTMLInputElement' is not assignable to parameter of type 'HTMLTextAreaElement'` の型エラー。

- [ ] **Step 3: `caretPosition.ts` を `<input>` 対応にする**

`getCaretCoordinates` の引数型と、折り返しの設定だけを変える。textarea の挙動は変えない。

シグネチャを次に変える:

```ts
export function getCaretCoordinates(
  el: HTMLTextAreaElement | HTMLInputElement,
  position: number,
): CaretCoordinates {
```

`style.whiteSpace = "pre-wrap"; style.wordWrap = "break-word";` の 2 行を次に置き換える。

```ts
    // 1 行入力(<input>)は折り返さない。top/height は textarea 用の値で、<input> では使わない
    // (呼び出し側 caretRect.ts は <input> の縦方向に要素自身の矩形を使う)。
    const isInput = el instanceof HTMLInputElement;
    style.whiteSpace = isInput ? "pre" : "pre-wrap";
    style.wordWrap = isInput ? "normal" : "break-word";
```

- [ ] **Step 4: テストと型チェックが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/caretPosition.test.ts 2>&1 | tail -8`
Expected: 全件 PASS。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -8`
Expected: `caretPosition` 関連の型エラーなし。

- [ ] **Step 5: `caretRect` の失敗するテストを書く**

`/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/caretRect.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const getCaretCoordinatesMock = vi.hoisted(() => vi.fn());
vi.mock("./caretPosition", () => ({ getCaretCoordinates: getCaretCoordinatesMock }));

import { caretRect, isImeTarget } from "./caretRect";

function domRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left, top, width, height,
    right: left + width, bottom: top + height, x: left, y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

function stubBox(el: Element, left: number, top: number, width: number, height: number) {
  vi.spyOn(el, "getBoundingClientRect").mockReturnValue(domRect(left, top, width, height));
}

function makeTextarea(value: string): HTMLTextAreaElement {
  const el = document.createElement("textarea");
  el.value = value;
  document.body.appendChild(el);
  return el;
}

function makeInput(type: string, value: string): HTMLInputElement {
  const el = document.createElement("input");
  el.type = type;
  el.value = value;
  document.body.appendChild(el);
  return el;
}

beforeEach(() => {
  getCaretCoordinatesMock.mockReset();
  getCaretCoordinatesMock.mockReturnValue({ left: 5, top: 7, height: 18 });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("isImeTarget", () => {
  it("accepts textarea and text-like inputs", () => {
    expect(isImeTarget(makeTextarea(""))).toBe(true);
    for (const type of ["text", "search", "url", "email", "password", "tel", "number"]) {
      expect(isImeTarget(makeInput(type, "")), type).toBe(true);
    }
  });

  it("rejects non-text inputs and other elements", () => {
    expect(isImeTarget(makeInput("checkbox", ""))).toBe(false);
    expect(isImeTarget(makeInput("range", ""))).toBe(false);
    expect(isImeTarget(document.createElement("button"))).toBe(false);
    expect(isImeTarget(document.createElement("div"))).toBe(false);
    expect(isImeTarget(null)).toBe(false);
  });

  it("accepts a contenteditable element", () => {
    const div = document.createElement("div");
    div.setAttribute("contenteditable", "true");
    document.body.appendChild(div);
    expect(isImeTarget(div)).toBe(true);
  });
});

describe("caretRect (textarea)", () => {
  it("adds the element origin to the caret coordinates", () => {
    const el = makeTextarea("abcdef");
    el.setSelectionRange(3, 3);
    stubBox(el, 10, 20, 200, 100);
    expect(caretRect(el)).toEqual({ x: 15, y: 27, width: 1, height: 18 });
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 3);
  });

  it("prefers a numeric anchor over the current selection", () => {
    const el = makeTextarea("abcdef");
    el.setSelectionRange(5, 5);
    stubBox(el, 0, 0, 200, 100);
    caretRect(el, 1);
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 1);
  });

  it("clamps an anchor beyond the text length", () => {
    const el = makeTextarea("abcdef");
    stubBox(el, 0, 0, 200, 100);
    caretRect(el, 99);
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 6);
  });

  // Review Focus 1: 空の入力欄
  it("returns a rect for an empty textarea (caret at 0)", () => {
    const el = makeTextarea("");
    stubBox(el, 10, 20, 200, 100);
    expect(caretRect(el)).toEqual({ x: 15, y: 27, width: 1, height: 18 });
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 0);
  });

  it("passes UTF-16 indices through unchanged for surrogate pairs", () => {
    const el = makeTextarea("😀あ");
    el.setSelectionRange(2, 2);
    stubBox(el, 0, 0, 200, 100);
    caretRect(el);
    expect(getCaretCoordinatesMock).toHaveBeenCalledWith(el, 2);
  });

  it("returns null when the coordinates are not finite", () => {
    const el = makeTextarea("abc");
    stubBox(el, 0, 0, 200, 100);
    getCaretCoordinatesMock.mockReturnValue({ left: Number.NaN, top: 0, height: 18 });
    expect(caretRect(el)).toBeNull();
  });

  it("rounds fractional coordinates to integers", () => {
    const el = makeTextarea("abc");
    stubBox(el, 10.4, 20.6, 200, 100);
    getCaretCoordinatesMock.mockReturnValue({ left: 0.2, top: 0.2, height: 17.6 });
    expect(caretRect(el)).toEqual({ x: 11, y: 21, width: 1, height: 18 });
  });
});

describe("caretRect (input)", () => {
  it("uses the caret x and the full height of the field", () => {
    const el = makeInput("text", "abc");
    stubBox(el, 10, 20, 200, 30);
    expect(caretRect(el)).toEqual({ x: 15, y: 20, width: 1, height: 30 });
  });

  // Review Focus 2: selectionEnd が null の type
  it("falls back to the bottom-left of the field when the type has no selection API", () => {
    for (const type of ["email", "number"]) {
      const el = makeInput(type, "a@b");
      stubBox(el, 10, 20, 200, 30);
      expect(caretRect(el), type).toEqual({ x: 10, y: 49, width: 1, height: 1 });
    }
    expect(getCaretCoordinatesMock).not.toHaveBeenCalled();
  });
});

describe("caretRect (contenteditable)", () => {
  function makeEditable(text: string): { el: HTMLElement; node: Text } {
    const el = document.createElement("div");
    el.setAttribute("contenteditable", "true");
    const node = document.createTextNode(text);
    el.appendChild(node);
    document.body.appendChild(el);
    return { el, node };
  }

  function rangeWithRects(node: Text, offset: number, rects: Partial<DOMRect>[]): Range {
    const range = document.createRange();
    range.setStart(node, offset);
    range.collapse(true);
    (range as unknown as { getClientRects: () => unknown }).getClientRects = () => rects;
    return range;
  }

  it("uses the first client rect of the anchor range", () => {
    const { el, node } = makeEditable("hello");
    stubBox(el, 0, 0, 300, 100);
    const range = rangeWithRects(node, 2, [{ left: 30, top: 40, width: 0, height: 18 }]);
    expect(caretRect(el, range)).toEqual({ x: 30, y: 40, width: 1, height: 18 });
  });

  it("falls back to the bottom-left of the element when the range has no rect", () => {
    const { el, node } = makeEditable("hello");
    stubBox(el, 10, 20, 300, 100);
    const range = rangeWithRects(node, 2, [{ left: 0, top: 0, width: 0, height: 0 }]);
    expect(caretRect(el, range)).toEqual({ x: 10, y: 119, width: 1, height: 1 });
  });

  it("falls back to the bottom-left of the element when there is no selection", () => {
    const { el } = makeEditable("hello");
    stubBox(el, 10, 20, 300, 100);
    document.getSelection()?.removeAllRanges();
    expect(caretRect(el)).toEqual({ x: 10, y: 119, width: 1, height: 1 });
  });

  it("ignores an anchor range that lies outside the element", () => {
    const { el } = makeEditable("hello");
    stubBox(el, 10, 20, 300, 100);
    const other = document.createElement("p");
    const otherNode = document.createTextNode("elsewhere");
    other.appendChild(otherNode);
    document.body.appendChild(other);
    const range = rangeWithRects(otherNode, 1, [{ left: 99, top: 99, width: 0, height: 18 }]);
    expect(caretRect(el, range)).toEqual({ x: 10, y: 119, width: 1, height: 1 });
  });
});
```

- [ ] **Step 6: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/caretRect.test.ts 2>&1 | tail -15`
Expected: FAIL(`Failed to resolve import "./caretRect"`)。

- [ ] **Step 7: `caretRect.ts` を実装する**

`/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/caretRect.ts`:

```ts
// IME の変換候補ウィンドウの位置補正(Issue #502)に使う、キャレット矩形の算出。
// 座標は viewport 座標(CSS px)。DOM を読むだけで、何も書き換えない。
import { getCaretCoordinates } from "./caretPosition";

export interface CaretRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/// 位置の基準。数値は text 系入力欄の文字位置、Range は contenteditable の位置、null は現在のキャレット。
export type CaretAnchor = number | Range | null;

const TEXT_INPUT_TYPES = new Set(["text", "search", "url", "email", "password", "tel", "number"]);

function isContentEditable(el: HTMLElement): boolean {
  if (el.isContentEditable) return true;
  // jsdom など isContentEditable を実装しない環境向けに属性でも見る。
  const attr = el.getAttribute("contenteditable");
  return attr !== null && attr !== "false";
}

/// IME の候補位置を補正する対象の入力欄か。
export function isImeTarget(el: Element | null): el is HTMLElement {
  if (!(el instanceof HTMLElement)) return false;
  if (el instanceof HTMLTextAreaElement) return true;
  if (el instanceof HTMLInputElement) return TEXT_INPUT_TYPES.has(el.type);
  return isContentEditable(el);
}

function toRect(x: number, y: number, width: number, height: number): CaretRect | null {
  if (![x, y, width, height].every(Number.isFinite)) return null;
  return {
    x: Math.round(x),
    y: Math.round(y),
    width: Math.max(1, Math.round(width)),
    height: Math.max(1, Math.round(height)),
  };
}

/// キャレットが求められないときの代わり。入力欄の左下(候補は欄のすぐ下に出る)。
function fallbackRect(el: HTMLElement): CaretRect | null {
  const box = el.getBoundingClientRect();
  return toRect(box.left, box.bottom - 1, 1, 1);
}

function textFieldRect(
  el: HTMLInputElement | HTMLTextAreaElement,
  anchor: CaretAnchor,
): CaretRect | null {
  let position: number | null;
  try {
    // type=email / number などは selectionEnd が null になる。
    position = typeof anchor === "number" ? anchor : el.selectionEnd;
  } catch {
    position = null;
  }
  if (position === null) return fallbackRect(el);

  const box = el.getBoundingClientRect();
  const caret = getCaretCoordinates(el, Math.min(position, el.value.length));
  if (el instanceof HTMLInputElement) {
    // 1 行入力は縦方向を欄いっぱいに取る(候補は欄の下端のすぐ下に出る)。
    return toRect(box.left + caret.left, box.top, 1, box.height);
  }
  return toRect(box.left + caret.left, box.top + caret.top, 1, caret.height);
}

function rangeRect(range: Range): CaretRect | null {
  if (typeof range.getClientRects !== "function") return null;
  const rects = range.getClientRects();
  const first =
    rects.length > 0
      ? rects[0]
      : typeof range.getBoundingClientRect === "function"
        ? range.getBoundingClientRect()
        : null;
  if (!first || (first.width === 0 && first.height === 0)) return null;
  return toRect(first.left, first.top, 1, first.height);
}

function editableRect(el: HTMLElement, anchor: CaretAnchor): CaretRect | null {
  let range: Range | null = anchor instanceof Range ? anchor : null;
  if (!range) {
    const selection = el.ownerDocument.getSelection();
    if (selection && selection.rangeCount > 0) {
      range = selection.getRangeAt(0).cloneRange();
      range.collapse(false);
    }
  }
  if (range && el.contains(range.startContainer)) {
    const rect = rangeRect(range);
    if (rect) return rect;
  }
  return fallbackRect(el);
}

/// アクティブな入力欄のキャレット矩形(viewport 座標)を返す。求められなければ null。
export function caretRect(el: HTMLElement, anchor: CaretAnchor = null): CaretRect | null {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    return textFieldRect(el, anchor);
  }
  return editableRect(el, anchor);
}
```

- [ ] **Step 8: テストと型チェックが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/caretRect.test.ts src/lib/caretPosition.test.ts 2>&1 | tail -15`
Expected: 全件 PASS。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -8`
Expected: `caretRect` 関連の型エラーなし(Task 1 前から存在した無関係な診断は別途報告)。

- [ ] **Step 9: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/caretPosition.ts frontend/src/lib/caretPosition.test.ts frontend/src/lib/caretRect.ts frontend/src/lib/caretRect.test.ts
git commit -m "feat: 入力欄のキャレット矩形を求める caretRect を追加(Issue #502)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: イベント監視と送信 `imeCursorArea.ts`、配線

**Files:**
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/imeCursorArea.ts`
- Create: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/imeCursorArea.test.ts`
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/ipc.ts`(`vibrate` の直後に `setImeCursorArea` を追加)
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/platform.ts`(`isLinuxPlatform` を追加)
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/main.ts`(起動時に `installImeCursorArea()` を呼ぶ)

**Interfaces:**
- Consumes: Task 1 の `commands.setImeCursorArea(x, y, width, height)`、Task 2 の `caretRect` / `isImeTarget` / `CaretRect` / `CaretAnchor`、`ipc.ts` の `unwrap` / `commands`。
- Produces:
  - `export function installImeCursorArea(options?: { doc?: Document; send?: (rect: CaretRect) => void; delayMs?: number }): () => void`(戻り値は解除関数。何度呼んでも安全)
  - `export function setImeCursorArea(rect: { x: number; y: number; width: number; height: number }): void`(`ipc.ts`)
  - `export const isLinuxPlatform: boolean`(`platform.ts`)

- [ ] **Step 1: 失敗するテストを書く**

`/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/imeCursorArea.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const caretRectMock = vi.hoisted(() => vi.fn());
vi.mock("./caretRect", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./caretRect")>()),
  caretRect: caretRectMock,
}));
vi.mock("./ipc", () => ({ setImeCursorArea: vi.fn() }));

import { installImeCursorArea } from "./imeCursorArea";

const RECT = { x: 1, y: 2, width: 1, height: 3 };

const send = vi.fn();
let uninstall: () => void;

function fire(target: EventTarget, type: string) {
  target.dispatchEvent(new Event(type, { bubbles: true }));
}

/// requestAnimationFrame(≈16ms)と送信の遅延(30ms)が過ぎるまで進める。
function settle() {
  vi.advanceTimersByTime(100);
}

function focusTextarea(value = ""): HTMLTextAreaElement {
  const el = document.createElement("textarea");
  el.value = value;
  document.body.appendChild(el);
  el.focus();
  settle(); // focus 自体が起こすイベントの分を流す
  send.mockClear();
  caretRectMock.mockClear();
  return el;
}

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame", "cancelAnimationFrame"],
  });
  caretRectMock.mockReset();
  caretRectMock.mockReturnValue(RECT);
  send.mockReset();
  uninstall = installImeCursorArea({ send });
});

afterEach(() => {
  uninstall();
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("installImeCursorArea", () => {
  it("sends the caret rect after a delay, not immediately", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    expect(send).not.toHaveBeenCalled();
    settle();
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(RECT);
    expect(caretRectMock).toHaveBeenLastCalledWith(el, null);
  });

  it("coalesces events arriving while a send is pending", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    fire(el, "keyup");
    fire(document, "selectionchange");
    settle();
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("does not postpone the send when events keep arriving", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    vi.advanceTimersByTime(30);
    fire(el, "input");
    vi.advanceTimersByTime(30);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("sends again even when the rect is unchanged", () => {
    const el = focusTextarea("abc");
    fire(el, "input");
    settle();
    fire(el, "input");
    settle();
    expect(send).toHaveBeenCalledTimes(2);
  });

  it("does nothing when the focused element is not a text field", () => {
    const button = document.createElement("button");
    document.body.appendChild(button);
    button.focus();
    settle();
    send.mockClear();
    fire(button, "input");
    settle();
    expect(send).not.toHaveBeenCalled();
  });

  it("does not send when no rect can be computed", () => {
    const el = focusTextarea("abc");
    caretRectMock.mockReturnValue(null);
    fire(el, "input");
    settle();
    expect(send).not.toHaveBeenCalled();
  });

  it("reacts to window resize and to scroll anywhere in the document", () => {
    const el = focusTextarea("abc");
    fire(window, "resize");
    settle();
    expect(send).toHaveBeenCalledTimes(1);

    const scroller = document.createElement("div");
    document.body.appendChild(scroller);
    scroller.dispatchEvent(new Event("scroll")); // scroll は bubble しない
    settle();
    expect(send).toHaveBeenCalledTimes(2);
    expect(caretRectMock).toHaveBeenLastCalledWith(el, null);
  });

  it("stops listening after uninstall", () => {
    const el = focusTextarea("abc");
    uninstall();
    fire(el, "input");
    settle();
    expect(send).not.toHaveBeenCalled();
  });

  // Review Focus 3: 変換中は開始位置に固定される
  it("keeps the anchor at the composition start while the caret moves", () => {
    const el = focusTextarea("abcdef");
    el.setSelectionRange(2, 2);
    fire(el, "compositionstart");
    el.value = "abXYcdef";
    el.setSelectionRange(4, 4);
    fire(el, "compositionupdate");
    settle();
    expect(caretRectMock).toHaveBeenLastCalledWith(el, 2);

    fire(el, "compositionend");
    settle();
    expect(caretRectMock).toHaveBeenLastCalledWith(el, null);
  });

  // Review Focus 4: 選択範囲の置き換え
  it("anchors at the start of the selection when composing over a selection", () => {
    const el = focusTextarea("abcdef");
    el.setSelectionRange(1, 4);
    fire(el, "compositionstart");
    settle();
    expect(caretRectMock).toHaveBeenLastCalledWith(el, 1);
  });

  it("anchors a contenteditable composition with a collapsed Range at the selection start", () => {
    const el = document.createElement("div");
    el.setAttribute("contenteditable", "true");
    el.tabIndex = 0;
    const node = document.createTextNode("hello");
    el.appendChild(node);
    document.body.appendChild(el);
    el.focus();
    settle();
    send.mockClear();
    caretRectMock.mockClear();

    document.getSelection()?.setBaseAndExtent(node, 1, node, 4);
    fire(el, "compositionstart");
    settle();

    const anchor = caretRectMock.mock.lastCall?.[1] as Range;
    expect(anchor).toBeInstanceOf(Range);
    expect(anchor.collapsed).toBe(true);
    expect(anchor.startContainer).toBe(node);
    expect(anchor.startOffset).toBe(1);
  });

  // Review Focus 5: 送信失敗がタイピングを妨げない
  it("does not let a failing send escape the listener", () => {
    const el = focusTextarea("abc");
    send.mockImplementation(() => {
      throw new Error("ipc down");
    });
    fire(el, "input");
    expect(() => settle()).not.toThrow();
    expect(send).toHaveBeenCalledTimes(1);

    send.mockReset();
    fire(el, "input");
    settle();
    expect(send).toHaveBeenCalledTimes(1); // 失敗のあとも動き続ける
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/imeCursorArea.test.ts 2>&1 | tail -15`
Expected: FAIL(`Failed to resolve import "./imeCursorArea"`)。

- [ ] **Step 3: `imeCursorArea.ts` を実装する**

`/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/imeCursorArea.ts`:

```ts
// IME の変換候補ウィンドウの位置補正(Issue #502)。
// Linux/Wayland では WebKitGTK が IM に渡すキャレット位置が不正確なため、document のイベントを
// 監視して実キャレット矩形を求め、Rust の set_ime_cursor_area で IM コンテキストに上書きする。
// 起動するかの判定(Linux か)は呼び出し側(main.ts)の責務。Wayland かの判定は Rust 側で行う。
import { caretRect, isImeTarget, type CaretAnchor, type CaretRect } from "./caretRect";
import { setImeCursorArea } from "./ipc";

/// WebKit は編集状態が変わるたびに自前(不正確)の座標を送り直すので、それより後に上書きする。
/// 実機のプローブで効いた値。
const SEND_DELAY_MS = 30;

const DOCUMENT_EVENTS = [
  "focusin",
  "input",
  "keyup",
  "mouseup",
  "selectionchange",
  "compositionupdate",
] as const;

export interface ImeCursorAreaOptions {
  doc?: Document;
  send?: (rect: CaretRect) => void;
  delayMs?: number;
}

/// 変換開始時点の位置。text 系は文字位置、contenteditable は選択範囲の先頭に畳んだ Range。
function recordAnchor(doc: Document): CaretAnchor {
  const el = doc.activeElement;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    try {
      return el.selectionStart;
    } catch {
      return null;
    }
  }
  const selection = doc.getSelection();
  if (selection && selection.rangeCount > 0) {
    const range = selection.getRangeAt(0).cloneRange();
    range.collapse(true);
    return range;
  }
  return null;
}

/// 監視を始める。戻り値は解除関数(何度呼んでも安全)。
export function installImeCursorArea(options: ImeCursorAreaOptions = {}): () => void {
  const doc = options.doc ?? document;
  const send = options.send ?? setImeCursorArea;
  const delayMs = options.delayMs ?? SEND_DELAY_MS;

  let composing = false;
  let anchor: CaretAnchor = null;
  let pending = false;
  let rafId: number | undefined;
  let timerId: ReturnType<typeof setTimeout> | undefined;

  const flush = () => {
    pending = false;
    try {
      const el = doc.activeElement;
      if (!isImeTarget(el)) return;
      const rect = caretRect(el, composing ? anchor : null);
      if (rect) send(rect);
    } catch {
      // 位置補正の失敗でタイピングを妨げない。
    }
  };

  // 待機中に来たイベントは 1 回の送信にまとめる。待機は延長しない(キー連打で先送りされ続けない)。
  const schedule = () => {
    if (pending) return;
    pending = true;
    rafId = requestAnimationFrame(() => {
      timerId = setTimeout(flush, delayMs);
    });
  };

  const onCompositionStart = () => {
    composing = true;
    anchor = recordAnchor(doc);
    schedule();
  };
  const onCompositionEnd = () => {
    composing = false;
    anchor = null;
    schedule();
  };

  const passive = { passive: true } as const;
  const capturePassive = { capture: true, passive: true } as const;
  for (const type of DOCUMENT_EVENTS) doc.addEventListener(type, schedule, passive);
  doc.addEventListener("scroll", schedule, capturePassive);
  doc.addEventListener("compositionstart", onCompositionStart, passive);
  doc.addEventListener("compositionend", onCompositionEnd, passive);
  window.addEventListener("resize", schedule, passive);

  return () => {
    for (const type of DOCUMENT_EVENTS) doc.removeEventListener(type, schedule, passive);
    doc.removeEventListener("scroll", schedule, capturePassive);
    doc.removeEventListener("compositionstart", onCompositionStart, passive);
    doc.removeEventListener("compositionend", onCompositionEnd, passive);
    window.removeEventListener("resize", schedule, passive);
    if (rafId !== undefined) cancelAnimationFrame(rafId);
    if (timerId !== undefined) clearTimeout(timerId);
    pending = false;
  };
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && npx vitest run src/lib/imeCursorArea.test.ts 2>&1 | tail -25`
Expected: 全件 PASS。

もし `requestAnimationFrame is not defined` や、フェイクタイマーが rAF を進めず `send` が呼ばれない場合は、実装ではなくテストの前提の問題。`vi.useFakeTimers` の `toFake` に `requestAnimationFrame` が効いているか、`typeof requestAnimationFrame` をテスト内で出力して確かめ、原因をユーザーに報告してから進める(実装を rAF なしに変えない)。

- [ ] **Step 5: `ipc.ts` にラッパーを追加する**

`/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/ipc.ts` の `vibrate` 関数の直後に追加:

```ts

/// IME の変換候補ウィンドウの位置を補正する(Issue #502)。座標は viewport 座標(CSS px)。
/// Linux/Wayland 以外では Rust 側が何もしない。位置補正の失敗は入力に影響しないため握りつぶす
/// (vibrateと同じfire-and-forgetパターン)。
export function setImeCursorArea(rect: { x: number; y: number; width: number; height: number }): void {
  void unwrap(commands.setImeCursorArea(rect.x, rect.y, rect.width, rect.height)).catch(() => {});
}
```

- [ ] **Step 6: `platform.ts` に Linux 判定を追加する**

`/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/lib/platform.ts` の末尾に追加:

```ts

// Linux(WebKitGTK)実行時だけ有効にする処理の判定(IME の候補位置補正など)。
export const isLinuxPlatform = current === "linux";
```

- [ ] **Step 7: `main.ts` から起動する**

`/home/onodai145/repos/github.com/onodai145/tsumugi/frontend/src/main.ts` のインポートに追加(このファイルの流儀に合わせてセミコロンなし・シングルクォート):

```ts
import { installImeCursorArea } from './lib/imeCursorArea'
import { isLinuxPlatform } from './lib/platform'
```

`const app = mount(...)` の直前に追加:

```ts
// Linux/Wayland: WebKitGTK が IM に渡すキャレット位置が不正確で、変換候補が入力位置からずれる
// (Issue #502)。実キャレット位置を求めて IM に上書きする。Wayland かの判定は Rust 側。
if (isLinuxPlatform) installImeCursorArea()
```

- [ ] **Step 8: 全体のテストと型チェックを通す**

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm check 2>&1 | tail -10`
Expected: 今回の変更に関する型エラーなし。

Run: `cd /home/onodai145/repos/github.com/onodai145/tsumugi/frontend && pnpm test 2>&1 | tail -15`
Expected: 全件 PASS。

- [ ] **Step 9: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add frontend/src/lib/imeCursorArea.ts frontend/src/lib/imeCursorArea.test.ts frontend/src/lib/ipc.ts frontend/src/lib/platform.ts frontend/src/main.ts
git commit -m "feat: 入力欄のキャレット位置を IME の候補位置に反映する(Issue #502)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 実機検証とガイドへの反映

このタスクは Hyprland + fcitx5 の実機でユーザーが確認する。自動化できない(実 Wayland + fcitx5 が必要で、既存の E2E 基盤では再現できない)。検証用の `cargo tauri dev` はユーザーが自分の端末で起動する。エージェントは起動しない(実画面に出るため)。

**Files:**
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/docs/guide/user-guide.md`(トラブルシューティングに IME の項目を追加)
- Modify: `/home/onodai145/repos/github.com/onodai145/tsumugi/docs/superpowers/specs/2026-10-19-issue-502-ime-cursor-area-design.md`(検証結果を「実機検証」の末尾に追記)

**Interfaces:**
- Consumes: Task 1〜3 の実装一式。
- Produces: 検証結果(spec に追記)。調整が必要なら `SEND_DELAY_MS`(`imeCursorArea.ts`)の変更。

- [ ] **Step 1: 実機で起動してもらう**

ユーザーに、リポジトリルートで `cargo tauri dev` を起動してもらう(`GTK_IM_MODULE` は現在の環境のままでよい)。

- [ ] **Step 2: 確認項目を実施してもらう**

候補ウィンドウが入力位置のすぐ下に出るかを、次の場面で見てもらう。

- ComposeBar:単行、折り返した複数行、スクロール後。
- 文章の途中で変換した場合(候補が変換開始位置に出る)。
- 設定画面の `<input>`。
- MML エディタ(CodeMirror)。
- ウィンドウを動かした後。
- UI スケールを 100% 以外(例: 150%)にして ComposeBar で変換した場合(`zoom_level()` による換算)。
- 候補ウィンドウが一瞬ずれた位置に出てから動く(ちらつく)ことがないか。
- `GDK_BACKEND=x11 cargo tauri dev`(または X11 セッション)で、挙動が以前と変わっていないこと。

- [ ] **Step 3: 結果に応じて調整する**

- ちらつく場合: `imeCursorArea.ts` の `SEND_DELAY_MS` を調整する(短くして WebKit の上書きより遅れすぎないか、長くして WebKit の上書きに負けないかを実機で比べる)。それでも直らなければ、設計を見直す必要があるのでユーザーに報告して止まる。
- 位置が合わない入力欄がある場合: その入力欄の種類と状況を記録し、`caretRect.ts` のその種類の処理を見直す。
- 調整したら、その変更を単独でコミットする(件名のみ)。

- [ ] **Step 4: ガイドの「トラブルシューティング」に項目を追加する**

`/home/onodai145/repos/github.com/onodai145/tsumugi/docs/guide/user-guide.md` の、`**カスタムCSSで画面が操作できなくなった**` の直前に追加:

```markdown
**Linux/Wayland で、日本語入力の変換中の下線が変換中の文字の位置に出ない**

WebKitGTK(GTK)側の既知の制限で、tsumugi では直せません。変換候補のウィンドウは、入力位置のすぐ下に出るよう補正しています。変換中の文字は表示され、確定した文字は正しく入力されます。

```

- [ ] **Step 5: spec に検証結果を追記する**

spec の「実機検証」セクションの末尾に、Step 2 の各項目の結果(○/×と気づいた点)を箇条書きで追記する。調整した場合は、調整後の `SEND_DELAY_MS` の値と理由も書く。

- [ ] **Step 6: Commit**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git add docs/guide/user-guide.md docs/superpowers/specs/2026-10-19-issue-502-ime-cursor-area-design.md
git commit -m "docs: IME の候補位置補正の実機検証結果と、既知の制限をガイドに反映(Issue #502)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-Review(計画の作成者による点検)

**Spec coverage**

| spec の要件 | タスク |
|---|---|
| コマンド `set_ime_cursor_area`、全 OS に存在、Linux 以外は no-op | Task 1 |
| Wayland 限定(`WAYLAND_DISPLAY` + `GDK_BACKEND`)、起動時 1 回キャッシュ | Task 1 |
| `zoom_level()` による換算 | Task 1 |
| `caretRect.ts`(textarea / input / contenteditable / フォールバック) | Task 2 |
| `caretPosition.ts` の `<input>` 対応 | Task 2 |
| `imeCursorArea.ts`(対象イベント、rAF + 30ms、まとめる、重複排除しない) | Task 3 |
| 変換開始位置のアンカー | Task 3 |
| `isLinuxPlatform`、`main.ts` からの起動、`ipc.ts` ラッパー | Task 3 |
| NaN 除外・整数化・失敗の握りつぶし | Task 2、Task 3 |
| `specta_builder()` 登録、バインディング再生成 | Task 1 |
| 実機検証(ちらつき、UI スケール、X11 で変わらない) | Task 4 |
| ガイドへの既知の制限の反映 | Task 4 |
| E2E は追加しない | (対応するタスクなし。意図どおり) |

**型の一貫性:** `CaretRect`(`x/y/width/height`)、`CaretAnchor`(`number | Range | null`)、`caretRect(el, anchor?)`、`isImeTarget(el)`、`installImeCursorArea(options?)`、`setImeCursorArea(rect)` は Task 2・3 の間で同じ名前・形で使っている。`commands.setImeCursorArea(x, y, width, height)` は Task 1 の生成物と `ipc.ts` で一致。

**プレースホルダ:** なし。Task 4 は手動検証のため、判断が必要な箇所(調整の方針)を手順内に条件付きで書いてある。
