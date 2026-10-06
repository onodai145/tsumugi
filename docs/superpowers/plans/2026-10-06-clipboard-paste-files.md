# ファイル参照クリップボード貼り付け Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ファイルマネージャでコピーしたファイル(画像・動画等、種別問わず)を投稿欄で貼り付けて添付できるようにする (Issue #117)。

**Architecture:** Rust に「OS クリップボードのファイル参照(`file://` URI)を読んで実在する通常ファイルの絶対パスに変換する」コマンド `read_clipboard_files` と、テキスト復元用の `read_clipboard_text` を足す。フロントは `paste` イベントを `clipboardData.types` で同期判定して横取りし、取得したパスを既存の `addLocalAttachment(path)`(D&D と同じ。投稿時に `uploadFile(path)`)へ渡す。優先順位は ファイル → 画像(#57) → テキスト。

**Tech Stack:** Rust (`clipboard-rs` 0.3.5 + `wayland` feature, `url`, `tauri-plugin-clipboard-manager` の既存 `ClipboardExt`), tauri-specta, Svelte 5, Vitest。

**Spec:** `docs/superpowers/specs/2026-10-06-clipboard-paste-files-design.md`(実装者は必ず先に読むこと。特に「`clipboardData` の実測」の表)

## Global Constraints

- 依存追加は `src-tauri/Cargo.toml` の `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`(`tauri-plugin-single-instance` と同じブロック)に `clipboard-rs = { version = "=0.3.5", default-features = false, features = ["wayland"] }` のみ。`url` は既存依存を使う。`@tauri-apps/plugin-clipboard-manager`(JS)は導入しない。`capabilities/default.json` は変更しない。
- コマンドは `specta_builder()`(`src-tauri/src/lib.rs`)の `collect_commands![]` に登録する。`frontend/src/bindings/tauri.gen.ts` は手編集せず `cd src-tauri && cargo test` で再生成してコミットする。
- アップロード処理は新規に書かない。取得パスは `addLocalAttachment(path)` に渡すだけ(投稿時アップロード原則、Issue #66)。
- `read_clipboard_files` / `read_clipboard_text` の読み取り失敗はエラー表示せず「無い」扱い(`Ok(vec![])` / `Ok(String::new())`)。ログは `log::debug`(パス・トークンは出さない)。
- ディレクトリ・存在しないパス・`file://` 以外・ホスト付き(リモート)URI は黙って除外する。
- モバイル(android/ios)では `read_clipboard_files` は `Ok(vec![])` を返す。
- フロントの新規 UI 値は発明しない(今回 UI 追加は無い)。
- コミットメッセージは**件名のみ**(本文・箇条書き禁止)。末尾に `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` を付ける。`--no-verify` / `--no-gpg-sign` 禁止。コミットが失敗・タイムアウトしたら中止してユーザーに報告(再試行しない)。
- 作業は `feat/issue-117-paste-clipboard-files` ブランチ上(作成済み)。`main` に直接コミットしない。
- `./target/debug/tsumugi` や `cargo run` を直接実行しない。実 UI 確認は `cargo tauri dev` をリポジトリルートから、かつ実画面に出さない(Xvfb + `WAYLAND_DISPLAY` unset)。起動した開発サーバーは完了前に自分で正確な PID 指定で kill する(`pkill`/`killall` 禁止)。
- push 後に CI を Monitor/待機ループで監視しない。

## Review Focus

1. 日本語・空白・`%` を含むファイル名の URI が、元の実パスに戻る(Task 1 のテスト)。
2. 複数ファイルを同時にコピーしたとき、全件が順序どおり添付される(Task 4 のテスト)。
3. ディレクトリ・存在しないファイル・リモートホスト付き URI・`https://` が添付に混ざらない(Task 1 のテスト)。
4. 通常のテキスト貼り付け(`types` が空・`text/plain` あり)で `paste` が横取りされず、Rust への IPC も発生しない(Task 3 / Task 4 のテスト)。
5. URL コピー等で `text/uri-list` が付いているがファイルが得られない場合、テキストが失われず `read_clipboard_text` 経由で本文に入る(Task 4 のテスト)。
6. ファイル参照の読み取りが失敗(IPC エラー)しても、エラー表示が出ずに画像 → テキストへ進む(Task 4 のテスト)。

---

### Task 1: `file_uris_to_paths`(URI → 実在ファイルパス変換)

**Files:**
- Modify: `src-tauri/src/commands/note.rs`(`clipboard_filename` の直後、現在 469 行付近の関数の後に追加。テストは同ファイル末尾の `mod tests` 内)

**Interfaces:**
- Consumes: なし(`url` クレートは既存依存)
- Produces: `fn file_uris_to_paths(uris: &[String]) -> Vec<String>`(`note.rs` 内の private 関数。Task 2 が呼ぶ)

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/note.rs` 末尾の `mod tests { use super::*; ... }` の内側(既存テストの後)に追加する。

```rust
    /// `file_uris_to_paths` 用に、テストごとに一意な一時ディレクトリを作る。
    fn clipboard_files_tmp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tsumugi-clipboard-files-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file_uri(path: &std::path::Path) -> String {
        url::Url::from_file_path(path).unwrap().to_string()
    }

    #[test]
    fn file_uris_to_paths_decodes_space_japanese_and_percent_names() {
        let dir = clipboard_files_tmp_dir();
        let names = ["a b.txt", "日本語の動画.mp4", "100%.png"];
        let mut uris = Vec::new();
        let mut expected = Vec::new();
        for n in names {
            let p = dir.join(n);
            std::fs::write(&p, b"x").unwrap();
            uris.push(file_uri(&p));
            expected.push(p.to_str().unwrap().to_string());
        }
        assert_eq!(file_uris_to_paths(&uris), expected);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_uris_to_paths_accepts_absolute_path_of_current_exe() {
        // Windows のドライブレター付きパス(`file:///C:/...`)も含め、どの OS でも
        // 「実在する絶対パス → URI → パス」の往復が一致することを確かめる。
        let exe = std::env::current_exe().unwrap();
        let got = file_uris_to_paths(&[file_uri(&exe)]);
        assert_eq!(got, vec![exe.to_str().unwrap().to_string()]);
    }

    #[test]
    fn file_uris_to_paths_accepts_localhost_host() {
        let dir = clipboard_files_tmp_dir();
        let p = dir.join("a.txt");
        std::fs::write(&p, b"x").unwrap();
        // `file:///tmp/a.txt` を `file://localhost/tmp/a.txt` に書き換える(Unix のみ意味がある形)。
        #[cfg(unix)]
        {
            let uri = file_uri(&p).replacen("file://", "file://localhost", 1);
            assert_eq!(file_uris_to_paths(&[uri]), vec![p.to_str().unwrap().to_string()]);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_uris_to_paths_skips_non_file_remote_missing_directory_and_garbage() {
        let dir = clipboard_files_tmp_dir();
        let ok = dir.join("ok.txt");
        std::fs::write(&ok, b"x").unwrap();
        let subdir = dir.join("sub");
        std::fs::create_dir_all(&subdir).unwrap();
        let missing = dir.join("missing.txt");

        let uris = vec![
            "https://example.com/a.png".to_string(),
            "ftp://example.com/a.png".to_string(),
            "file://example.com/tmp/remote.txt".to_string(),
            file_uri(&missing),
            file_uri(&subdir),
            "".to_string(),
            "   ".to_string(),
            "not a uri".to_string(),
            file_uri(&ok),
        ];
        assert_eq!(file_uris_to_paths(&uris), vec![ok.to_str().unwrap().to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_uris_to_paths_empty_input_returns_empty() {
        assert!(file_uris_to_paths(&[]).is_empty());
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test file_uris_to_paths`
Expected: コンパイルエラー(`cannot find function file_uris_to_paths in this scope`)。

- [ ] **Step 3: 最小実装を書く**

`clipboard_filename` 関数の直後に追加する。

```rust
/// クリップボードの `file://` URI 一覧を、実在する通常ファイルの絶対パスへ変換する(Issue #117)。
/// `file://` 以外・ホスト付き(リモート)・変換できないもの・存在しないパス・ディレクトリは
/// 黙って除外する(Misskey のドライブにフォルダはアップロードできないため)。
#[cfg_attr(any(target_os = "android", target_os = "ios"), allow(dead_code))]
fn file_uris_to_paths(uris: &[String]) -> Vec<String> {
    uris.iter()
        .filter_map(|uri| {
            let url = url::Url::parse(uri.trim()).ok()?;
            if url.scheme() != "file" {
                return None;
            }
            // 空ホストと localhost は許可。それ以外(file://host/... や Windows の UNC)は除外する。
            if !matches!(url.host_str(), None | Some("") | Some("localhost")) {
                return None;
            }
            let path = url.to_file_path().ok()?;
            if !path.is_file() {
                return None;
            }
            path.into_os_string().into_string().ok()
        })
        .collect()
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test file_uris_to_paths`
Expected: 5 件 PASS(Windows 以外では `localhost` テストが `#[cfg(unix)]` 内の assert を実行する)。

もし `file_uris_to_paths_skips_...` の「`file://example.com/tmp/remote.txt`」が除外されず失敗する場合は、`url.host_str()` の戻り値を `eprintln!` で確認し、`Some("example.com")` のはずなのでその分岐に入っているかを見る(`url` クレートは `file://localhost/...` を空ホストへ正規化するため、`localhost` の許可は保険であり、`example.com` は除外される想定)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/src/commands/note.rs
git commit -m "$(cat <<'EOF'
feat: クリップボードのfile:// URIを実在ファイルパスへ変換する関数を追加

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `read_clipboard_files` / `read_clipboard_text` コマンドと依存の追加

**Files:**
- Modify: `src-tauri/Cargo.toml`(`tauri-plugin-single-instance = "2"` の次の行)
- Modify: `src-tauri/src/commands/note.rs`(`read_clipboard_image` の直後に 2 コマンドを追加)
- Modify: `src-tauri/src/lib.rs:93`(`commands::note::read_clipboard_image,` の次の行に 2 行追加)
- Regenerate: `frontend/src/bindings/tauri.gen.ts`, `src-tauri/Cargo.lock`

**Interfaces:**
- Consumes: `file_uris_to_paths(&[String]) -> Vec<String>`(Task 1)
- Produces: Rust コマンド `read_clipboard_files() -> Result<Vec<String>>`、`read_clipboard_text(app: AppHandle) -> Result<String>`。TS bindings では `commands.readClipboardFiles(): Promise<Result<string[], Error>>` と `commands.readClipboardText(): Promise<Result<string, Error>>`(Task 4 が使う)。

- [ ] **Step 1: 依存を追加する**

`src-tauri/Cargo.toml` の該当ブロックを次のようにする。

```toml
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]
# 多重起動防止(Issue #53)。ノートキャッシュを SQLite に保存しており、複数プロセスが
# 同時に書き込むと SQLITE_BUSY 等の競合を起こしうるため。モバイルには存在しないプラグイン。
tauri-plugin-single-instance = "2"
# ファイルマネージャでコピーしたファイル参照(text/uri-list・CF_HDROP 等)の読み取り(Issue #117)。
# 公式 clipboard-manager プラグインにはファイル参照を読む機能が無い。Linux の Wayland は
# optional の "wayland" feature が必要(ext-data-control / wlr-data-control 対応コンポジタのみ)。
# image feature は #57 のプラグイン経由の画像読み取りと重複するため無効にする。
clipboard-rs = { version = "=0.3.5", default-features = false, features = ["wayland"] }
```

- [ ] **Step 2: 2 コマンドを追加する**

`src-tauri/src/commands/note.rs` の `read_clipboard_image` 関数(現在 333〜356 行)の直後に追加する。

```rust
/// ファイルマネージャでコピーしたファイルのパス一覧を返す(アップロードはしない。Issue #117)。
/// 取得したパスはフロントが `addLocalAttachment` に渡し、投稿時に既存の `upload_file` で
/// アップロードされる。ファイル参照が無い・読み取りに失敗した・モバイルの場合は空配列を返す
/// (「ファイルが無い」は通常の分岐であり、`read_clipboard_image` の `Error::Invalid` のような
/// エラーシグナルにはしない)。
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_files() -> Result<Vec<String>> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let read = tauri::async_runtime::spawn_blocking(|| -> std::result::Result<Vec<String>, String> {
            use clipboard_rs::Clipboard;
            let ctx = clipboard_rs::ClipboardContext::new().map_err(|e| e.to_string())?;
            ctx.get_files().map_err(|e| e.to_string())
        })
        .await;
        match read {
            Ok(Ok(uris)) => Ok(file_uris_to_paths(&uris)),
            // 「クリップボードにファイル参照が無い」だけでも Err になりうる(画像貼り付けのたびに
            // 起きる日常的な状況)ため warn にはしない。
            Ok(Err(e)) => {
                log::debug!("クリップボードのファイル参照を読めませんでした: {e}");
                Ok(Vec::new())
            }
            Err(e) => {
                log::debug!("クリップボード読み取りタスクが失敗しました: {e}");
                Ok(Vec::new())
            }
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(Vec::new())
    }
}

/// クリップボードのテキストを返す。無い・読み取りに失敗した場合は空文字列(エラーにしない)。
/// `text/uri-list` が付いたコピーでは WebKitGTK が DOM に `text/plain` を見せないため、
/// `handlePaste` が止めたテキストを復元するときにだけ使う(Issue #117)。
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_text(app: AppHandle) -> Result<String> {
    let text = tauri::async_runtime::spawn_blocking(move || app.clipboard().read_text().unwrap_or_default())
        .await
        .unwrap_or_default();
    Ok(text)
}
```

- [ ] **Step 3: コマンドを登録する**

`src-tauri/src/lib.rs` の `commands::note::read_clipboard_image,` の直後に追加する。

```rust
            commands::note::read_clipboard_files,
            commands::note::read_clipboard_text,
```

- [ ] **Step 4: ビルド・テスト・bindings 再生成**

Run: `cd src-tauri && cargo test`
Expected: 全 PASS。`generates_frontend_bindings` が `frontend/src/bindings/tauri.gen.ts` を再生成する(実 DB 接続が必要なテストは `#[ignore]` で除外済み)。

Run: `git diff --stat; grep -n "readClipboardFiles\|readClipboardText" frontend/src/bindings/tauri.gen.ts`
Expected: `tauri.gen.ts` に `readClipboardFiles: () => typedError<string[], Error>(...)` と `readClipboardText: () => typedError<string, Error>(...)` が増えている。`Cargo.lock` に `clipboard-rs` が増えている(`wl-clipboard-rs` は既存の 0.9.3 から 0.9.4 に上がる場合がある。意図しない大量のバージョン更新が無いことを `git diff src-tauri/Cargo.lock | head -80` で確認する)。

- [ ] **Step 5: コミットする**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands/note.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "$(cat <<'EOF'
feat: read_clipboard_files / read_clipboard_text コマンドを追加

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `shouldInterceptPaste`(paste 横取りの同期判定)

**Files:**
- Create: `frontend/src/lib/pasteIntent.ts`
- Test: `frontend/src/lib/pasteIntent.test.ts`

**Interfaces:**
- Consumes: なし
- Produces: `export function shouldInterceptPaste(types: readonly string[], plainText: string): boolean`(Task 4 が import する)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/pasteIntent.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { shouldInterceptPaste } from "./pasteIntent";

describe("shouldInterceptPaste", () => {
  it("text/plain のみ(types が空でも getData で取れる WebKitGTK のケース)は横取りしない", () => {
    expect(shouldInterceptPaste([], "hello")).toBe(false);
  });

  it("types に text/plain/text/html があり本文もあるなら横取りしない", () => {
    expect(shouldInterceptPaste(["text/plain", "text/html"], "hi")).toBe(false);
  });

  it("本文が空(スクリーンショット画像など)なら横取りする", () => {
    expect(shouldInterceptPaste([], "")).toBe(true);
  });

  it("text/uri-list があれば本文の有無に関わらず横取りする(WebKitGTK は text/plain を隠す)", () => {
    expect(shouldInterceptPaste(["text/uri-list"], "")).toBe(true);
    // text/plain も見える WebView(WebView2 / WKWebView)でもファイル参照を優先する
    expect(shouldInterceptPaste(["text/uri-list", "text/plain"], "a.mp4")).toBe(true);
  });

  it("Files があれば本文の有無に関わらず横取りする", () => {
    expect(shouldInterceptPaste(["Files"], "")).toBe(true);
    expect(shouldInterceptPaste(["Files", "text/plain"], "a.png")).toBe(true);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm test pasteIntent`
Expected: FAIL(`Failed to resolve import "./pasteIntent"`)。

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/pasteIntent.ts`:

```ts
/**
 * `paste` イベントを横取りして Rust 側のクリップボード読み取りに回すべきかを、
 * 同期的に判定する(Issue #117)。
 *
 * `preventDefault()` は paste ハンドラ内で同期的に呼ぶ必要があり、Rust への問い合わせ(非同期)
 * の結果を待ってからでは既定のテキスト貼り付けを止められないため、DOM から同期的に取れる
 * `types` と `text/plain` だけで判定する。WebKitGTK では `text/uri-list` があると `text/plain`
 * が DOM から隠され、`text/plain` のみのときは `types` が空になる(spec の実測表を参照)。
 */
export function shouldInterceptPaste(types: readonly string[], plainText: string): boolean {
  if (types.includes("text/uri-list") || types.includes("Files")) return true;
  return plainText === "";
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm test pasteIntent`
Expected: 5 件 PASS。

- [ ] **Step 5: コミットする**

```bash
git add frontend/src/lib/pasteIntent.ts frontend/src/lib/pasteIntent.test.ts
git commit -m "$(cat <<'EOF'
feat: paste横取りの同期判定shouldInterceptPasteを追加

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `ComposeBar.svelte` の `handlePaste` 統合

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`(import 追加、`handlePaste` 関数: 現在 551〜566 行)
- Test: `frontend/src/ui/ComposeBar.test.ts`(末尾に新しい `describe` を追加)

**Interfaces:**
- Consumes: `shouldInterceptPaste(types: readonly string[], plainText: string): boolean`(Task 3)、`commands.readClipboardFiles()` / `commands.readClipboardText()`(Task 2)、既存の `commands.readClipboardImage()` / `addLocalAttachment(path: string)` / `formatError` / `textarea` 変数
- Produces: なし

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ComposeBar.test.ts` の末尾に追加する。`fireEvent.paste` の `clipboardData` オプションは testing-library が `clipboardData` プロパティとして付与する。

```ts
describe("ComposeBar 貼り付け(Issue #117)", () => {
  function paste(textarea: HTMLElement, types: string[], plain = "") {
    return fireEvent.paste(textarea, { clipboardData: { types, getData: (t: string) => (t === "text/plain" ? plain : "") } });
  }

  // files に { kind, message } を渡すと IPC 失敗を模す。Tauri の実エラーはプレーンオブジェクトで
  // reject される(生成 bindings の typedError は Error インスタンスだけを再 throw するため、
  // new Error(...) を reject させると status: "error" にならない)。
  function mockClipboard(opts: { files?: string[] | { kind: string; message: string }; text?: string }) {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_drafts") return Promise.resolve([]);
      if (cmd === "read_clipboard_files") {
        return opts.files && !Array.isArray(opts.files) ? Promise.reject(opts.files) : Promise.resolve(opts.files ?? []);
      }
      if (cmd === "read_clipboard_image") return Promise.reject({ kind: "invalid", message: "no image" });
      if (cmd === "read_clipboard_text") return Promise.resolve(opts.text ?? "");
      if (cmd === "read_attachment_preview") return Promise.resolve("data:image/png;base64,xx");
      return Promise.resolve(null);
    });
  }

  const clipboardCalls = () =>
    invokeMock.mock.calls.map((c) => c[0] as string).filter((c) => c.startsWith("read_clipboard_"));

  it("ファイル参照を貼り付けると、複数ファイルが順序どおり添付される", async () => {
    mockClipboard({ files: ["/home/u/a b.mp4", "/home/u/日本語.png"] });
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), ["text/uri-list"]);
    await waitFor(() => expect(screen.getAllByTitle("削除")).toHaveLength(2));
    // 画像拡張子のファイルだけプレビューが読まれ、動画は拡張子バッジになる
    expect(invokeMock).toHaveBeenCalledWith("read_attachment_preview", expect.objectContaining({ path: "/home/u/日本語.png" }));
    expect(invokeMock).not.toHaveBeenCalledWith("read_attachment_preview", expect.objectContaining({ path: "/home/u/a b.mp4" }));
    expect(screen.getByText("MP4")).toBeTruthy();
    // ファイルが取れた場合は画像・テキストの読み取りに進まない
    expect(clipboardCalls()).toEqual(["read_clipboard_files"]);
  });

  it("通常のテキスト貼り付けは横取りせず、Rust への IPC も発生しない", async () => {
    mockClipboard({});
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), [], "hello");
    await Promise.resolve();
    expect(clipboardCalls()).toEqual([]);
  });

  it("ファイルが得られず text/uri-list だけ付いている場合(URL コピー等)はテキストを復元する", async () => {
    mockClipboard({ files: [], text: "https://example.com/" });
    // jsdom には execCommand("insertText") が無いため、呼び出し内容で検証する
    const exec = vi.fn().mockReturnValue(true);
    (document as unknown as { execCommand: unknown }).execCommand = exec;
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), ["text/uri-list"]);
    await waitFor(() => expect(exec).toHaveBeenCalledWith("insertText", false, "https://example.com/"));
    expect(clipboardCalls()).toEqual(["read_clipboard_files", "read_clipboard_image", "read_clipboard_text"]);
    expect(screen.queryAllByTitle("削除")).toHaveLength(0);
  });

  it("read_clipboard_files の IPC が失敗してもエラー表示せず画像・テキストへ進む", async () => {
    mockClipboard({ files: { kind: "network", message: "boom" }, text: "" });
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), ["text/uri-list"]);
    await waitFor(() => expect(clipboardCalls()).toEqual(["read_clipboard_files", "read_clipboard_image", "read_clipboard_text"]));
    expect(screen.queryByText(/boom/)).toBeNull();
  });

  it("本文が空でファイルも無ければ従来どおり画像を試す(#57)", async () => {
    mockClipboard({ files: [] });
    const { getByTestId } = render(ComposeBar);
    await paste(getByTestId("compose-textarea"), []);
    await waitFor(() => expect(clipboardCalls()).toContain("read_clipboard_image"));
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm test ComposeBar.test`
Expected: 新しい 5 件が FAIL(現行の `handlePaste` は `read_clipboard_files` を呼ばない)。既存テストは PASS のまま。

- [ ] **Step 3: 実装する**

`frontend/src/ui/ComposeBar.svelte` の import に追加する(`import { pickComposePlaceholder } from "../lib/composePlaceholder";` の次の行)。

```ts
  import { shouldInterceptPaste } from "../lib/pasteIntent";
```

`handlePaste` 関数全体(現在 551〜566 行)を次で置き換える。

```ts
  // 貼り付け: ファイルマネージャでコピーしたファイル参照(Issue #117)→ クリップボード画像(#57)→
  // テキストの優先順位で試す。preventDefault は同期的に呼ぶ必要があるため、横取りするかは
  // DOM から同期的に取れる情報だけで判定する(shouldInterceptPaste)。
  async function handlePaste(e: ClipboardEvent) {
    const types = Array.from(e.clipboardData?.types ?? []);
    const plain = e.clipboardData?.getData("text/plain") ?? "";
    if (!shouldInterceptPaste(types, plain)) return;
    e.preventDefault();

    // IPC 失敗(status: "error")は「ファイルが無い」として扱い、エラー表示せず次へ進む。
    const files = await commands.readClipboardFiles();
    if (files.status === "ok" && files.data.length > 0) {
      for (const p of files.data) await addLocalAttachment(p);
      return;
    }

    const r = await commands.readClipboardImage();
    if (r.status === "ok") {
      const blob = new Blob([new Uint8Array(r.data.bytes)], { type: "image/png" });
      const previewUrl = URL.createObjectURL(blob);
      attachments = [
        ...attachments,
        { kind: "clipboard", id: crypto.randomUUID(), name: r.data.filename, bytes: r.data.bytes, previewUrl },
      ];
      return;
    }
    if (r.error.kind !== "invalid") {
      err = formatError(r.error);
      return;
    }

    // ファイルも画像も無かった。止めてしまったテキストを復元する。text/uri-list が付いたコピーでは
    // WebKitGTK が DOM から text/plain を隠すため、clipboardData ではなく Rust 側から読む。
    const t = await commands.readClipboardText();
    if (t.status === "ok" && t.data) {
      textarea?.focus();
      document.execCommand("insertText", false, t.data);
    }
  }
```

- [ ] **Step 4: テストと型チェックが通ることを確認する**

Run: `cd frontend && pnpm test ComposeBar && pnpm check`
Expected: ComposeBar の全テスト(新 5 件を含む)PASS、`pnpm check` はエラー 0。`document.execCommand` が deprecated の hint を出すだけならエラーではないので無視してよい。

もし「複数ファイル」のテストで `getAllByTitle("削除")` が期待数にならない場合は、添付チップの `title="削除"`(`ComposeBar.svelte` の添付欄)がレンダリングされているかを確認する。`addLocalAttachment` は `readAttachmentPreview` を await してから `attachments` に追加するため、`waitFor` で待つこと。

- [ ] **Step 5: コミットする**

```bash
git add frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "$(cat <<'EOF'
feat: 投稿欄でファイルマネージャのファイル貼り付けに対応

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: ユーザーガイド更新と全体検証

**Files:**
- Modify: `docs/guide/user-guide.md:99`

**Interfaces:**
- Consumes: Task 1〜4 の成果物
- Produces: なし

- [ ] **Step 1: ユーザーガイドを更新する**

`docs/guide/user-guide.md` の 99 行の「クリップボードから画像を直接貼り付けることもできます。」を次のように置き換える。

```
クリップボードから画像を直接貼り付けることもできます。ファイルマネージャでコピーしたファイル(画像・動画など)も `Ctrl+V` で添付できます(Linux の Wayland ではコンポジタが `ext-data-control` または `wlr-data-control` に対応している必要があります。フォルダは添付できません)。
```

- [ ] **Step 2: 全テストと型チェックを実行する**

Run: `cd src-tauri && cargo test`
Expected: 全 PASS。

Run: `cd frontend && pnpm check && pnpm test`
Expected: エラー 0、全 PASS。

- [ ] **Step 3: ユーザーガイドをコミットする**

```bash
git add docs/guide/user-guide.md
git commit -m "$(cat <<'EOF'
docs: ファイル貼り付けの対応をユーザーガイドに追記

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 4: 手動確認(あなた=ユーザーの実 Hyprland セッションで行う。自動化できない)**

実装者は、ここまでの結果を報告して、ユーザーに以下の確認を依頼する。実装者自身が実画面に tsumugi を出してはならない。

ユーザーは `cargo tauri dev`(リポジトリルートから)を起動して確認する。

1. 動画ファイルをコピーして投稿欄で Ctrl+V → 添付欄に出る(投稿前にアップロードされていない)→ 投稿できる。
2. 画像ファイルをコピーして Ctrl+V → サムネイル付きで元のファイル名のまま添付される。
3. 複数ファイルを同時にコピーして Ctrl+V → 全件が添付される。
4. スクリーンショット画像の貼り付け(#57)、通常のテキスト貼り付け、ブラウザで URL をコピーして貼り付け → 従来どおり動く(URL は本文に入る)。
5. ディレクトリのみをコピーして貼り付け → 添付はされず、パス等のテキストが本文に入る。
6. 実 Wayland セッションでの `paste` イベントの `types` 確認(デバッグブリッジ。`cargo tauri dev` 中のみソケットがある)。ファイルをコピーする前に次を送る。

```sh
curl --unix-socket ~/.cache/com.onodai.tsumugi/debug-bridge.sock http://localhost/ --data-binary 'window.__pt=null; document.addEventListener("paste",function(e){window.__pt=JSON.stringify({types:Array.from(e.clipboardData.types),plain:e.clipboardData.getData("text/plain")})},{capture:true,once:true}); return "armed"'
```

   ファイルをコピーして投稿欄で Ctrl+V した後、次で読む(応答は二重に JSON 文字列なので 2 回デコードする)。

```sh
curl --unix-socket ~/.cache/com.onodai.tsumugi/debug-bridge.sock http://localhost/ --data-binary 'return String(window.__pt)'
```

   期待: `types` に `"text/uri-list"` が含まれる。含まれない場合は spec の「`types` の前提と未検証部分」に従い、この環境でファイル貼り付けが働かない原因として報告する(退行ではない)。確認後 `delete window.__pt` で一時変数を消す。

- [ ] **Step 5: 完了報告**

結果(自動テストの出力と、ユーザーの手動確認結果)を spec の「未検証」項目と照らして報告する。未確認の環境(GNOME / X11 / Windows / macOS)は未検証のままと明記する。PR 作成はユーザーの指示を待つ(本文に `Closes #117` を入れる)。
