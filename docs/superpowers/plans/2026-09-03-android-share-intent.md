# Android共有インテント受信 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Android の共有シートから tsumugi にテキスト・画像・動画を渡し、コンポーズ欄に反映できるようにする(Issue #116)。

**Architecture:** `MainActivity.kt` が `onCreate`/`onNewIntent` で `ACTION_SEND`/`ACTION_SEND_MULTIPLE` を検出し、テキストはそのまま、画像/動画は `content://` を `cacheDir/shared-intents/` に一時コピーした上でファイルパスとして、JNI (`external fun`) 経由で Rust のプロセスグローバルな1件分の保留領域(`mobile_intent.rs`)に格納する。フロントは起動時と `document.visibilitychange` のたびに `get_pending_share` コマンドをポーリングし、内容があれば既存の `app.openCompose()` 経由でコンポーズバーに流し込む。プラグインクレートの新設やイベントプッシュ(`trigger`/`addPluginListener`)は使わない — 詳細は `docs/superpowers/specs/2026-09-03-android-share-intent-design.md` 参照。

**Tech Stack:** Rust (`jni` crate 0.21) / Kotlin (Android, JNI `external fun`) / Svelte 5 (runes) / Vitest

## Global Constraints

- 対象は Android のみ。iOS・デスクトップでは `get_pending_share` は常に `None` を返す no-op。
- 外部の共有系プラグイン(tauri-plugin-sharetarget 等)や新規プラグインクレートは追加しない。JNI直結方式で自前実装する。
- 対応 MIME は `text/plain`, `image/*`, `video/*`。`ACTION_SEND` と `ACTION_SEND_MULTIPLE` の両方に対応する。
- 共有ファイルの名前は `ContentResolver` の `OpenableColumns.DISPLAY_NAME` から取得する。取得できない場合のみ機械的なフォールバック名にする(拡張子の当て推量はしない)。
- 未対応 MIME・パース失敗・一時ファイルコピー失敗はすべて「その1件だけ無視して継続」。エラーダイアログは出さない。
- 既存の `app.compose` / `openCompose()` の消費フロー(`frontend/src/ui/ComposeBar.svelte` の `$effect`)に相乗りする。新しいモーダルや新しい prop は作らない。

---

## Task 1: Rust — `ShareReceived` ドメイン型

**Files:**
- Create: `src-tauri/src/domain/share.rs`
- Modify: `src-tauri/src/domain/mod.rs`

**Interfaces:**
- Produces: `pub struct ShareReceived { pub text: Option<String>, pub file_paths: Vec<String> }`(`domain::ShareReceived` として公開。`Debug, Clone, Serialize, Deserialize, Type, PartialEq` 付き、`#[serde(rename_all = "camelCase")]`)。Task 2/3 がこれを使う。

- [ ] **Step 1: `ShareReceived` を定義する**

`src-tauri/src/domain/share.rs` を新規作成:

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

/// 他アプリの共有シート(Android の ACTION_SEND/ACTION_SEND_MULTIPLE)から受け取った内容
/// (Issue #116)。`text` はテキスト共有時のみ、`file_paths` は画像/動画共有時のみ埋まる
/// (アプリの一時キャッシュディレクトリへコピー済みの絶対パス)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShareReceived {
    pub text: Option<String>,
    pub file_paths: Vec<String>,
}
```

- [ ] **Step 2: `domain/mod.rs` に登録する**

`src-tauri/src/domain/mod.rs` の `mod` 宣言群に `mod share;` を追加(アルファベット順で `reaction` と `ui` の間):

```rust
mod reaction;
mod share;
mod ui;
```

`pub use` 群に `pub use share::ShareReceived;` を追加(同じくアルファベット順で `reaction` の再エクスポートの次):

```rust
pub use reaction::{EmojiDef, ReactionSummary, ReactionUser};
pub use share::ShareReceived;
pub use ui::UiPrefs;
```

- [ ] **Step 3: ビルド確認**

Run: `cd src-tauri && cargo build`
Expected: エラー無くビルドが通る(この時点では `ShareReceived` はまだどこからも参照されないため `dead_code` 警告が出ても良い — `domain/mod.rs` 冒頭に既に `#![allow(dead_code, unused_imports)]` がある)。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/domain/share.rs src-tauri/src/domain/mod.rs
git commit -m "feat: ShareReceivedドメイン型を追加(Issue #116)"
```

---

## Task 2: Rust — JNI橋渡し(`mobile_intent`)と `get_pending_share` コマンド

**Files:**
- Create: `src-tauri/src/mobile_intent.rs`
- Modify: `src-tauri/src/lib.rs` (mod宣言 + specta_builder登録)
- Modify: `src-tauri/src/commands/app.rs` (`get_pending_share` コマンド追加)
- Modify: `src-tauri/Cargo.toml` (`jni` 依存追加)

**Interfaces:**
- Consumes: `domain::ShareReceived`(Task 1)
- Produces:
  - `mobile_intent::take_pending_share() -> Option<ShareReceived>`(Android: 保留領域から取り出して消費、他OS: 常に `None`)。Task 3 (Kotlin) は Android 版のこのモジュール内 JNI export `Java_com_onodai_tsumugi_MainActivity_nativeShareReceived` を呼ぶ。
  - `commands::app::get_pending_share() -> Option<ShareReceived>`(specta command)。Task 5 (フロントエンド) がこれを呼ぶ。

- [ ] **Step 1: `mobile_intent.rs` を作成する**

`src-tauri/src/mobile_intent.rs` を新規作成:

```rust
//! 共有インテント(Android の ACTION_SEND/ACTION_SEND_MULTIPLE)受信用の橋渡し。
//! `MainActivity.kt` から JNI 経由で呼ばれ、プロセスグローバルな1件分の保留領域に
//! 格納する。フロントは `commands::app::get_pending_share` でポーリングして取り出す
//! (Issue #116)。他OSでは常に `None` を返す no-op。

use crate::domain::ShareReceived;

#[cfg(target_os = "android")]
mod android {
    use super::ShareReceived;
    use jni::objects::{JObject, JObjectArray, JString};
    use jni::JNIEnv;
    use std::sync::Mutex;

    static PENDING_SHARE: Mutex<Option<ShareReceived>> = Mutex::new(None);

    /// `MainActivity.kt` の `private external fun nativeShareReceived(...)` から呼ばれる。
    /// `text` は無ければ Java 側で null、`file_paths` は要素0件の配列で渡ってくる想定。
    #[no_mangle]
    pub extern "system" fn Java_com_onodai_tsumugi_MainActivity_nativeShareReceived<'local>(
        mut env: JNIEnv<'local>,
        _this: JObject<'local>,
        text: JString<'local>,
        file_paths: JObjectArray<'local>,
    ) {
        let text = if text.is_null() {
            None
        } else {
            env.get_string(&text).ok().map(|s| s.into())
        };

        let len = env.get_array_length(&file_paths).unwrap_or(0).max(0);
        let mut paths = Vec::with_capacity(len as usize);
        for i in 0..len {
            let Ok(obj) = env.get_object_array_element(&file_paths, i) else {
                continue;
            };
            let jstr = JString::from(obj);
            if let Ok(s) = env.get_string(&jstr) {
                paths.push(s.into());
            }
        }

        if text.is_none() && paths.is_empty() {
            return;
        }

        *PENDING_SHARE.lock().unwrap() = Some(ShareReceived {
            text,
            file_paths: paths,
        });
    }

    pub fn take_pending_share() -> Option<ShareReceived> {
        PENDING_SHARE.lock().unwrap().take()
    }
}

#[cfg(target_os = "android")]
pub fn take_pending_share() -> Option<ShareReceived> {
    android::take_pending_share()
}

#[cfg(not(target_os = "android"))]
pub fn take_pending_share() -> Option<ShareReceived> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "android"))]
    #[test]
    fn take_pending_share_is_noop_off_android() {
        assert_eq!(take_pending_share(), None);
    }
}
```

- [ ] **Step 2: テストを実行して確認する(非Android環境)**

Run: `cd src-tauri && cargo test take_pending_share_is_noop_off_android`
Expected: PASS(開発機は非Androidなので `#[cfg(not(target_os = "android"))]` 側だけがコンパイル・実行される)

- [ ] **Step 3: `lib.rs` にモジュール登録する**

`src-tauri/src/lib.rs` の `mod` 宣言群に `mod mobile_intent;` を追加(アルファベット順で `mod filter;` と `mod session;` の間):

```rust
mod filter;
mod mobile_intent;
mod session;
```

- [ ] **Step 4: `Cargo.toml` に `jni` を追加する**

`src-tauri/Cargo.toml` の `[target.'cfg(target_os = "android")'.dependencies]` に追記(`Cargo.lock` には既に `android-native-keyring-store` の推移的依存として `jni 0.21.1` が解決済み):

```toml
[target.'cfg(target_os = "android")'.dependencies]
android-native-keyring-store = "1.0.0"
jni = "0.21"
```

- [ ] **Step 5: `get_pending_share` コマンドを追加する**

`src-tauri/src/commands/app.rs` の先頭付近(`use` 群)に `use crate::domain::ShareReceived;` を追加し、ファイル末尾(既存の `#[cfg(test)] mod tests` より前)に追加:

```rust
/// 他アプリの共有シート(Android の ACTION_SEND/ACTION_SEND_MULTIPLE)から受け取った
/// テキスト/添付ファイルパスを取り出す。フロントは起動時と `visibilitychange` のたびに
/// これを呼びポーリングする(Issue #116)。無ければ `None`。一度取り出すと消費され、
/// 同じ内容が2回返ることはない。非Androidでは常に `None`。
#[tauri::command]
#[specta::specta]
pub fn get_pending_share() -> Option<ShareReceived> {
    crate::mobile_intent::take_pending_share()
}
```

既存の `#[cfg(test)] mod tests` ブロックに以下を追加(`is_newer_compares_numerically` の次):

```rust
#[test]
fn get_pending_share_is_none_by_default() {
    assert_eq!(get_pending_share(), None);
}
```

- [ ] **Step 6: テストを実行して確認する**

Run: `cd src-tauri && cargo test get_pending_share_is_none_by_default`
Expected: PASS

- [ ] **Step 7: `specta_builder()` に登録する**

`src-tauri/src/lib.rs` の `collect_commands!` 内、`commands::app::log_frontend_event,` の次に追加:

```rust
commands::app::log_frontend_event,
commands::app::get_pending_share,
```

- [ ] **Step 8: TSバインディングが生成されることを確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `getPendingShare` と `ShareReceived` 型が追加される。

- [ ] **Step 9: 全体テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: 既存テストを含め全て PASS

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/mobile_intent.rs src-tauri/src/lib.rs src-tauri/src/commands/app.rs src-tauri/Cargo.toml frontend/src/bindings/tauri.gen.ts
git commit -m "feat: 共有インテントの保留領域とget_pending_shareコマンドを追加(Issue #116)"
```

---

## Task 3: Android — `MainActivity.kt` でのインテント受信と `AndroidManifest.xml`

**Files:**
- Modify: `src-tauri/gen/android/app/src/main/java/com/onodai/tsumugi/MainActivity.kt`
- Modify: `src-tauri/gen/android/app/src/main/AndroidManifest.xml`

**Interfaces:**
- Consumes: Task 2 の JNI export `Java_com_onodai_tsumugi_MainActivity_nativeShareReceived`(Kotlin側からは `private external fun nativeShareReceived(text: String?, filePaths: Array<String>)` として宣言)
- Produces: なし(このタスクはUI/Rustから見て起点)。Kotlin側の自動テストは無いため、末尾に手動確認手順を置く。

- [ ] **Step 1: `AndroidManifest.xml` に intent-filter を追加する**

`src-tauri/gen/android/app/src/main/AndroidManifest.xml` の既存 `<activity>` 内、既存の `<intent-filter>`(MAIN/LAUNCHER)ブロックの直後に追加:

```xml
            <intent-filter>
                <action android:name="android.intent.action.SEND" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="text/plain" />
            </intent-filter>
            <intent-filter>
                <action android:name="android.intent.action.SEND" />
                <action android:name="android.intent.action.SEND_MULTIPLE" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="image/*" />
            </intent-filter>
            <intent-filter>
                <action android:name="android.intent.action.SEND" />
                <action android:name="android.intent.action.SEND_MULTIPLE" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="video/*" />
            </intent-filter>
```

- [ ] **Step 2: `MainActivity.kt` を書き換える**

`src-tauri/gen/android/app/src/main/java/com/onodai/tsumugi/MainActivity.kt` を全体置き換え:

```kotlin
package com.onodai.tsumugi

import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Log
import io.crates.keyring.Keyring
import java.io.File

class MainActivity : TauriActivity() {
    /// Rust側(mobile_intent.rs)へ共有インテントの内容を渡す(Issue #116)。
    /// `libtsumugi_lib.so` は generated/Rust.kt が既にロード済みなので追加ロード不要。
    private external fun nativeShareReceived(text: String?, filePaths: Array<String>)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // android-native-keyring-store (トークンの安全な保存先) が使う ndk-context を
        // 初期化する。Tauri は自動で行わないため、ここで明示的に呼ぶ必要がある。
        Keyring.initializeNdkContext(applicationContext)

        // 前回起動分の共有インテント一時ファイルの残骸を掃除する(Issue #116)。
        File(cacheDir, "shared-intents").deleteRecursively()
        handleShareIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleShareIntent(intent)
    }

    /// 他アプリの共有シート(ACTION_SEND/ACTION_SEND_MULTIPLE)からのテキスト/画像/動画を
    /// 拾い、Rust側へ JNI 経由で渡す(Issue #116)。未対応MIME・パース失敗時は何もしない。
    private fun handleShareIntent(intent: Intent) {
        val type = intent.type ?: return
        val text = if (intent.action == Intent.ACTION_SEND && type == "text/plain") {
            intent.getStringExtra(Intent.EXTRA_TEXT)
        } else {
            null
        }

        val isMedia = type.startsWith("image/") || type.startsWith("video/")
        val uris: List<Uri> = if (!isMedia) {
            emptyList()
        } else when (intent.action) {
            Intent.ACTION_SEND -> listOfNotNull(getStreamExtra(intent))
            Intent.ACTION_SEND_MULTIPLE -> getStreamArrayExtra(intent)
            else -> emptyList()
        }

        val filePaths = uris.mapNotNull { copyToCache(it) }

        if (text == null && filePaths.isEmpty()) return
        nativeShareReceived(text, filePaths.toTypedArray())
    }

    private fun getStreamExtra(intent: Intent): Uri? {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM)
        }
    }

    private fun getStreamArrayExtra(intent: Intent): List<Uri> {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java) ?: emptyList()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM) ?: emptyList()
        }
    }

    /// `content://` の共有ファイルを `cacheDir/shared-intents/` にコピーし、通常のファイル
    /// パスとして扱えるようにする。失敗時はこの1件だけ諦めて null を返す。
    private fun copyToCache(uri: Uri): String? {
        return try {
            val name = queryDisplayName(uri) ?: "shared-${System.currentTimeMillis()}"
            val dir = File(cacheDir, "shared-intents").apply { mkdirs() }
            val dest = File(dir, name)
            val copied = contentResolver.openInputStream(uri)?.use { input ->
                dest.outputStream().use { output -> input.copyTo(output) }
                true
            } ?: false
            if (!copied) return null
            dest.absolutePath
        } catch (e: Exception) {
            Log.w("MainActivity", "failed to copy shared file: $uri", e)
            null
        }
    }

    /// `ContentResolver` から元のファイル名を引く。拡張子の当て推量はしない
    /// (誤った拡張子でMisskeyドライブに入るのを避けるため)。
    private fun queryDisplayName(uri: Uri): String? {
        var cursor: Cursor? = null
        try {
            cursor = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            if (cursor != null && cursor.moveToFirst()) {
                val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (idx >= 0) return cursor.getString(idx)
            }
        } finally {
            cursor?.close()
        }
        return null
    }
}
```

- [ ] **Step 3: ビルド確認**

Run: `cd src-tauri && cargo tauri android build --debug --no-bundle`(CLAUDE.md 記載の通り、Android実機/エミュレータが無い環境でもコンパイル・リンクの確認はここまで可能)
Expected: エラー無くビルドが通る(JNI シンボル名の綴りミスはリンクエラーにならない — Kotlin の `external fun` 未解決は実行時 `UnsatisfiedLinkError` になる点に注意。Task 6 Step 1 の実機確認で検出する)

このタスク単体では自動テストが無く、フロントの受け口(Task 5)が無いと実機確認しても目に見える結果が出ない。実機/エミュレータでの確認は Task 6 Step 1 にまとめて行う。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/gen/android/app/src/main/AndroidManifest.xml src-tauri/gen/android/app/src/main/java/com/onodai/tsumugi/MainActivity.kt
git commit -m "feat: Android共有インテントの受信処理を追加(Issue #116)"
```

---

## Task 4: フロントエンド — `ComposeState` 拡張と `ComposeBar.svelte`

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Modify: `frontend/src/ui/ComposeBar.svelte`
- Modify: `frontend/src/ui/ComposeBar.test.ts`

**Interfaces:**
- Consumes: なし(このタスク単体で完結。Task 5 がこの `openCompose` の新オプションを呼ぶ)
- Produces: `app.openCompose(accountId: string, opts?: { replyTo?: Note; quoteOf?: Note; text?: string; filePaths?: string[] })`。`text` は本文欄が空の時だけ反映、`filePaths` は常に添付として追加。

- [ ] **Step 1: `ComposeState` に `text`/`filePaths` を追加する**

`frontend/src/lib/store.svelte.ts` の `ComposeState` を変更:

```ts
/// 投稿フォーム(返信/引用の文脈、または共有インテント由来の初期値つき)
export interface ComposeState {
  accountId: string;
  replyTo?: Note;
  quoteOf?: Note;
  /// 共有インテント等から受け取った初期本文(Issue #116)。既存の入力があれば上書きしない。
  text?: string;
  /// 共有インテント等から受け取った添付ファイルのローカルパス(Issue #116)。
  filePaths?: string[];
}
```

- [ ] **Step 2: `openCompose` のシグネチャを拡張する**

`frontend/src/lib/store.svelte.ts` の `openCompose` を変更:

```ts
openCompose(
  accountId: string,
  opts: { replyTo?: Note; quoteOf?: Note; text?: string; filePaths?: string[] } = {},
) {
  this.compose = { accountId, ...opts };
  // モバイル版UIは投稿欄がモーダル内にしか無いため、シグナルを消費できるよう先に開いておく
  // (PC版は常時表示なので不要)。isMobilePlatform(OS生判定)ではなく実効UIモード
  // (設定→表示のuiModeで上書き可能。Issue #51)で判定する。
  if (this.useMobileUi()) this.showComposeModal = true;
}
```

- [ ] **Step 3: `ComposeBar.svelte` の失敗するテストを書く**

`frontend/src/ui/ComposeBar.test.ts` の既存 `describe` ブロック内(末尾)に追加(このファイルは既に `app` / `invokeMock` のセットアップを持つ前提。`beforeEach`/`setupAccount` は既存のものをそのまま使う):

```ts
it("app.composeのtextとfilePathsをコンポーズ欄に反映する(Issue #116)", async () => {
  setupAccount();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "read_attachment_preview") return Promise.resolve("data:image/png;base64,xx");
    return Promise.resolve(null);
  });
  render(ComposeBar);
  app.openCompose("acc1", { text: "共有されたテキスト", filePaths: ["/tmp/shared-intents/photo.png"] });
  await waitFor(() => {
    expect(screen.getByDisplayValue("共有されたテキスト")).toBeInTheDocument();
  });
  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith(
      "read_attachment_preview",
      expect.objectContaining({ path: "/tmp/shared-intents/photo.png" }),
    );
  });
});

it("app.composeのtextは既存の入力があれば上書きしない(Issue #116)", async () => {
  setupAccount();
  invokeMock.mockResolvedValue(null);
  const { container } = render(ComposeBar);
  const textarea = container.querySelector("textarea") as HTMLTextAreaElement;
  await fireEvent.input(textarea, { target: { value: "書きかけの本文" } });
  app.openCompose("acc1", { text: "共有されたテキスト" });
  await waitFor(() => {
    expect(screen.getByDisplayValue("書きかけの本文")).toBeInTheDocument();
  });
});
```

このテストファイルの先頭 import に `screen` が無ければ追加する:

```ts
import { cleanup, render, fireEvent, waitFor, screen } from "@testing-library/svelte";
```

- [ ] **Step 4: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/ui/ComposeBar.test.ts`
Expected: 上記2件のテストが FAIL する(まだ `c.text`/`c.filePaths` を消費するコードが無いため)

- [ ] **Step 5: `pickFiles` を共通関数に分割する**

`frontend/src/ui/ComposeBar.svelte` の既存 `pickFiles` 関数を分割:

```ts
async function addLocalAttachment(path: string) {
  const name = path.split(/[\\/]/).pop() ?? path;
  let previewUrl: string | null = null;
  if (IMAGE_EXTENSIONS.has(extLower(name))) {
    try {
      previewUrl = await unwrap(commands.readAttachmentPreview(path));
    } catch {
      previewUrl = null;
    }
  }
  attachments = [...attachments, { kind: "local", id: crypto.randomUUID(), path, name, previewUrl }];
}

async function pickFiles() {
  err = null;
  // filters は付けない: Misskey のドライブは画像/動画に限らず任意のファイル種別を
  // 添付できる。加えて Android では画像/動画の MIME タイプに絞ると OS が自動的に
  // フォトピッカーへリダイレクトし、選択後の content:// URI から本来のファイル名を
  // 復元できなくなる(Google Issue Tracker #268079113, #330118234)。filters を外して
  // 汎用の "*/*" にすることで通常のドキュメント選択になり、ファイル名も正しく解決される。
  const picked = await open({ multiple: true });
  if (!picked) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  for (const p of paths) {
    await addLocalAttachment(p);
  }
}
```

- [ ] **Step 6: `app.compose` 消費の `$effect` に `text`/`filePaths` の反映を足す**

`frontend/src/ui/ComposeBar.svelte` の既存 `$effect` ブロック(`app.compose` を消費する箇所)内、`if (c.replyTo && !text.trim()) { ... }` の直後に追加:

```ts
    if (c.replyTo && !text.trim()) {
      text = `${acctOf(c.replyTo.user)} `;
    }
    // 共有インテント等からの初期本文(Issue #116)。返信のメンション挿入と同様、
    // 既に何か入力中ならそちらを優先し上書きしない。
    if (c.text && !text.trim()) {
      text = c.text;
    }
    for (const p of c.filePaths ?? []) {
      void addLocalAttachment(p);
    }
    app.compose = null;
```

(`app.compose = null;` は既存の行をそのまま使う。上記コードブロックはその直前に挿入する形になる。)

- [ ] **Step 7: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/ui/ComposeBar.test.ts`
Expected: 全テスト PASS

- [ ] **Step 8: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無く完了

- [ ] **Step 9: Commit**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "feat: ComposeStateに共有インテント由来の初期値を追加(Issue #116)"
```

---

## Task 5: フロントエンド — `pendingShare.ts` ポーリングと `App.svelte` 配線

**Files:**
- Create: `frontend/src/lib/pendingShare.ts`
- Create: `frontend/src/lib/pendingShare.test.ts`
- Modify: `frontend/src/App.svelte`

**Interfaces:**
- Consumes: `commands.getPendingShare()`(Task 2, `ShareReceived | null` を返す)、`app.openCompose()`(Task 4)、`app.defaultAccountId()`(既存)
- Produces: `setupPendingShareListener(): () => void`(呼ぶと即座に1回ポーリングし、以後 `visibilitychange` のたびにポーリングするリスナーを登録する。戻り値は登録解除用のクリーンアップ関数)

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/pendingShare.test.ts` を新規作成:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";

const getPendingShare = vi.fn();
const openCompose = vi.fn();
const defaultAccountId = vi.fn(() => "acc1");

vi.mock("./ipc", () => ({
  commands: {
    getPendingShare: (...args: unknown[]) => getPendingShare(...args),
  },
}));

vi.mock("./store.svelte", () => ({
  app: {
    openCompose: (...args: unknown[]) => openCompose(...args),
    defaultAccountId: (...args: unknown[]) => defaultAccountId(...args),
  },
}));

const { pollPendingShare, setupPendingShareListener } = await import("./pendingShare");

afterEach(() => {
  vi.clearAllMocks();
});

describe("pollPendingShare", () => {
  it("保留中の共有があればopenComposeに渡す", async () => {
    getPendingShare.mockResolvedValue({ text: "共有テキスト", filePaths: ["/tmp/a.png"] });
    await pollPendingShare();
    expect(openCompose).toHaveBeenCalledWith("acc1", {
      text: "共有テキスト",
      filePaths: ["/tmp/a.png"],
    });
  });

  it("nullならopenComposeを呼ばない", async () => {
    getPendingShare.mockResolvedValue(null);
    await pollPendingShare();
    expect(openCompose).not.toHaveBeenCalled();
  });

  it("textもfilePathsも空ならopenComposeを呼ばない", async () => {
    getPendingShare.mockResolvedValue({ text: null, filePaths: [] });
    await pollPendingShare();
    expect(openCompose).not.toHaveBeenCalled();
  });
});

describe("setupPendingShareListener", () => {
  it("登録直後に1回ポーリングする", () => {
    getPendingShare.mockResolvedValue(null);
    const cleanup = setupPendingShareListener();
    expect(getPendingShare).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it("visibilitychangeでvisibleになるたびポーリングする", () => {
    getPendingShare.mockResolvedValue(null);
    const cleanup = setupPendingShareListener();
    getPendingShare.mockClear();
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));
    expect(getPendingShare).toHaveBeenCalledTimes(1);
    cleanup();
  });

  it("戻り値のクリーンアップでリスナーを解除できる", () => {
    getPendingShare.mockResolvedValue(null);
    const cleanup = setupPendingShareListener();
    cleanup();
    getPendingShare.mockClear();
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));
    expect(getPendingShare).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm vitest run src/lib/pendingShare.test.ts`
Expected: FAIL(`./pendingShare` モジュールがまだ存在しない)

- [ ] **Step 3: `pendingShare.ts` を実装する**

`frontend/src/lib/pendingShare.ts` を新規作成:

```ts
// Android の共有シートから受け取ったテキスト/添付を取り込む(Issue #116)。
// 実行中の共有はイベントプッシュではなく、起動時と可視化のたびのポーリングで拾う
// (singleTask のため、共有経由でタスクが前面に戻る=可視化イベントが必ず起きる)。
import { commands } from "./ipc";
import { app } from "./store.svelte";

export async function pollPendingShare(): Promise<void> {
  const share = await commands.getPendingShare();
  if (!share) return;
  if (!share.text && share.filePaths.length === 0) return;
  app.openCompose(app.defaultAccountId(), {
    text: share.text ?? undefined,
    filePaths: share.filePaths,
  });
}

export function setupPendingShareListener(): () => void {
  void pollPendingShare();
  const onVisibilityChange = () => {
    if (document.visibilityState === "visible") void pollPendingShare();
  };
  document.addEventListener("visibilitychange", onVisibilityChange);
  return () => document.removeEventListener("visibilitychange", onVisibilityChange);
}
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm vitest run src/lib/pendingShare.test.ts`
Expected: 全テスト PASS

- [ ] **Step 5: `App.svelte` に配線する**

`frontend/src/App.svelte` の import 群に追加:

```ts
import { setupPendingShareListener } from "./lib/pendingShare";
```

既存の `onMount` を変更(既存の `keydown` リスナーのクリーンアップと合わせて両方解除する):

```ts
onMount(() => {
  app.boot();
  window.addEventListener("keydown", onGlobalKey);
  const stopPendingShareListener = setupPendingShareListener();
  return () => {
    window.removeEventListener("keydown", onGlobalKey);
    stopPendingShareListener();
  };
});
```

- [ ] **Step 6: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラー無く完了

- [ ] **Step 7: フロントエンド全体のテストを実行する**

Run: `cd frontend && pnpm test`
Expected: 全テスト PASS

- [ ] **Step 8: Commit**

```bash
git add frontend/src/lib/pendingShare.ts frontend/src/lib/pendingShare.test.ts frontend/src/App.svelte
git commit -m "feat: 起動時/可視化時に共有インテントをポーリングする(Issue #116)"
```

---

## Task 6: 実機確認とドキュメント更新

**Files:**
- Modify: `docs/guide/user-guide.md`

**Interfaces:**
- Consumes: Task 1-5 の全成果物
- Produces: なし(最終確認とユーザー向けドキュメント)

- [ ] **Step 1: Task 3 で保留した実機確認を実施する**

`cd src-tauri && cargo tauri android dev` でエミュレータ/実機にインストールし、以下を全て確認する:
1. ブラウザアプリでテキストを選択→共有→tsumugiを選択→コンポーズ欄にテキストが入る(コールドスタート: tsumugiを完全終了させた状態から)
2. 同じ操作をウォームスタート(tsumugiをバックグラウンドに置いた状態)で確認
3. ギャラリーアプリで画像1枚を共有→添付として反映される
4. ギャラリーアプリで画像を複数選択して共有(`ACTION_SEND_MULTIPLE`)→複数添付として反映される
5. 共有→コンポーズ欄に既に文章を書きかけの状態でテキスト共有→書きかけの本文が保持され添付だけ増える(このケースはテキスト共有ではなく画像共有で確認するのが自然)

いずれかで問題が見つかった場合は Task 1-5 に戻って修正し、再度全ステップを確認する。

- [ ] **Step 2: user-guide.md に使い方を追記する**

`docs/guide/user-guide.md` の適切なセクション(既存の投稿・添付まわりの説明の近く)に、他アプリからの共有でテキスト/画像/動画をコンポーズに取り込める旨を追記する。

- [ ] **Step 3: Commit**

```bash
git add docs/guide/user-guide.md
git commit -m "docs: Android共有インテントの使い方を追記(Issue #116)"
```

- [ ] **Step 4: PRを作成する**

CLAUDE.md の運用ルールに従い、`Fixes #116` を本文に含めた PR を作成する(直接 main へのマージはしない)。
