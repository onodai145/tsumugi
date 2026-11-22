# 内蔵 VOICEVOX CORE(段階3: Android)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Android(arm64・x86_64)でも、公式の VOICEVOX CORE(C API)を実行時に取得して動的ロードする内蔵エンジンで、外部エンジンなしにカラムのノートを読み上げられるようにする。Issue #625 の最終段階。

**Architecture:** 段階2の `tts/voicevox/` をそのまま使う。変えるのは、(1) `catalog.rs` に Android の取得物(ONNX Runtime だけ 1.23.2)と `ort_version`、(2) `ffi.rs` / `mod.rs` / `Cargo.toml` の `cfg(desktop)` を `cfg(not(target_os = "ios"))` に広げる、(3) アイドル解放を Android は 3 分、(4) Gradle で NDK の `libc++_shared.so` を APK に入れる、(5) フロントで、Android のときだけ取得の前に回線の確認を出す、の 5 点。まず実機・エミュレーターで「アプリ内の dlopen」の関門を通してから、UI に進む。

**Tech Stack:** Rust(`libloading` 0.8、wiremock は既存のテストのまま)、Gradle(Kotlin DSL)、Svelte 5 + Vitest、`adb` / CDP(WebView のリモートデバッグ)。

**Spec:** `docs/superpowers/specs/2026-11-21-issue-625-voicevox-core-stage3-android-design.md`(段階2: `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`)

## plan 作成時に実測して分かったこと(spec への訂正は Task 1 で反映する)

- **`downloadWarning` はデスクトップでも確認を出す**: 回線が `unknown`/`unknown`(デスクトップはこれ)だと「回線の種類を判定できませんでした…続けますか?」の確認文を返す(翻訳モデルの取得は、デスクトップでも出している)。spec の「デスクトップは `Unknown` なので出ない」は誤り。読み上げの取得は、デスクトップの挙動を変えない(非目標)ので、**スマホ(`isMobilePlatform`)のときだけ**確認を出す。
- **`ENGINE_SET` は定数のまま `cfg(target_os = "android")` で切り替える**(spec は「`EngineAssets` から作る」)。`Store` の API(引数なしの `engine_dir()` など)を変えずに済む。`EngineAssets.ort_version` と一致することをテストで確かめる。
- **`libc++_shared.so`(NDK r27c)の LOAD 整列は 16KB(0x4000)**(arm64・x86_64とも確認済み)。APK に入れても 16KB ページの警告は出ない。
- **x86_64 のアーカイブ内のパスも arm64 と同じ形**(`voicevox_core-android-x86_64-0.17.1/lib/libvoicevox_core.so`、`voicevox_onnxruntime-android-x64-1.23.2/lib/libvoicevox_onnxruntime.so`)。`packs.rs` は `lib/<名前>` だけを選んで展開するので、トップのディレクトリ名は関係しない。
- **Rust の `std::env::consts::ARCH`**: Android の armv7 は `"arm"`、i686 は `"x86"`、arm64 は `"aarch64"`、x86_64 は `"x86_64"`。
- 実機(Pixel 8 Pro)に、ユーザーの `com.onodai.tsumugi.debug` が入っている。**上書きインストール(署名が違うと、アンインストールでデータが消える)は、ユーザーの確認なしにしない。**関門の確認は、エミュレーター(AVD のコピー)で行う。
- 実機のプローブで測った値(スパイク): スレッド 4 で 8.1 秒の音声を約 4.7 秒、ピークメモリ 485〜510 MB。

## Global Constraints

- CORE 0.17.1、辞書 1.11、VVM 0.17.1(27 個)は段階2のまま。ONNX Runtime は**デスクトップが 1.17.3、Android だけ 1.23.2**。
- 対応する環境は、段階2の 5 組(`linux`+`x86_64`/`aarch64`、`macos`+`x86_64`/`aarch64`、`windows`+`x86_64`)に、`android`+`aarch64`/`x86_64` を足した 7 組。`android`+`arm`・`android`+`x86`・iOS は `supported = false`。
- デスクトップのディスク上の名前・取得物・同意版(`terms_version()`)は変えない。デスクトップでは、取得の前に回線の確認を出さない。
- `unsafe` は `ffi.rs` だけ。`gen/android/.../generated/` は編集しない(`app/build.gradle.kts` は編集してよい)。
- Android のビルドは、必ず `ANDROID_NDK_ROOT=$HOME/Android/Sdk/ndk/27.2.12479018` を export する(CLAUDE.md)。JDK は `JAVA_HOME=/usr/lib/jvm/java-21-openjdk`。メモリは `free` とスワップを見てから、`systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G` で上限を付ける。ビルド出力を `/tmp` に置かない。
- コミットは件名だけ(Conventional Commits、`type(scope): 件名`)。本文は書かない。`Co-Authored-By` のトレーラーは別の `-m` で付ける。`git commit` が失敗・タイムアウトしたら止めて報告する。`--no-verify` は使わない。
- プロセスの掃除は、正確な PID だけ(`pkill` / `killall` は使わない)。自分で起動したエミュレーターは、終わる前に `adb -s emulator-5570 emu kill` で止める。

## Review Focus

spec が暗に求めるが、どのタスクのテストにも出てこない入力・条件(最終レビューが意識して確かめる):

1. **armv7(`"arm"`)・i686(`"x86"`)の Android は非対応**のまま(`engine_assets_for("android", "arm")` は `None`)。Task 1 のテストで固定する。
2. **デスクトップの取得物・`ENGINE_SET`・同意版が変わらない**(既存の利用者の置き場が孤児にならない)。Task 1 のテストで固定する。
3. **デスクトップ(`isMobilePlatform` が偽)では、回線を見ず、確認も出さない**。Task 6 のテストで固定する。
4. **確認で「キャンセル」しても、取得も規約の画面も始まらない**。Task 6。
5. **`libc++_shared.so` が APK に入っていない場合**、読み上げは `SPEECH_CORE_LOAD_FAILED`(「エンジン一式を取得し直してください」)になり、取得し直しでは直らない。Task 4 で APK の中身を確かめて防ぐ(文面の改善は範囲外)。

## File Structure

- 変更: `src-tauri/src/tts/voicevox/catalog.rs`(Android の行・`ort_version`・`ENGINE_SET`)、`store.rs` と `packs.rs` のテストの `EngineAssets` の組み立て(フィールド追加)、`ffi.rs` / `mod.rs`(cfg)、`engine.rs`(アイドル解放)、`src-tauri/Cargo.toml`(`libloading` の置き場)、`src-tauri/gen/android/app/build.gradle.kts`(`libc++_shared.so`)。
- 変更: `frontend/src/lib/speechCore.ts`・`speechCore.test.ts`(`coreDownloadWarning`)、`frontend/src/ui/settings/SpeechCoreSection.svelte`・`.test.ts`(確認と、非対応の文面)。
- 変更: `docs/guide/user-guide.md`、`CLAUDE.md`、spec(訂正と、関門の結果の追記)。

---

### Task 1: カタログに Android の取得物を足す(`ort_version`・`ENGINE_SET`)

**Files:**
- Modify: `src-tauri/src/tts/voicevox/catalog.rs`
- Modify: `src-tauri/src/tts/voicevox/store.rs`(テストの `assets()`、`:144` 付近)、`src-tauri/src/tts/voicevox/packs.rs`(テストの `fake_assets`、`:582` 付近)
- Modify: `docs/superpowers/specs/2026-11-21-issue-625-voicevox-core-stage3-android-design.md`

**Interfaces:**
- Consumes: 既存の `EngineAssets`・`engine_assets_for(os, arch)`・`engine_assets()`。
- Produces: `EngineAssets.ort_version: &'static str`、`pub const ORT_VERSION_ANDROID: &str = "1.23.2"`、`ENGINE_SET`(`cfg(target_os = "android")` で値が変わる定数)。`engine_assets_for("android", "aarch64" | "x86_64")` が `Some`。

- [ ] **Step 1: 失敗するテストを書く(`catalog.rs` の `mod tests`)**

`every_supported_platform_resolves_assets_and_others_do_not` を次に置き換える(対応は 7 組。`ort_version` が URL に入る。非対応に `android`+`arm`・`android`+`x86` を足し、`android`+`aarch64` を外す):

```rust
    #[test]
    fn every_supported_platform_resolves_assets_and_others_do_not() {
        for (os, arch) in [
            ("linux", "x86_64"), ("linux", "aarch64"), ("macos", "x86_64"), ("macos", "aarch64"), ("windows", "x86_64"),
            ("android", "aarch64"), ("android", "x86_64"),
        ] {
            let a = engine_assets_for(os, arch).unwrap_or_else(|| panic!("{os} {arch}"));
            for asset in [&a.core, &a.ort, &a.dict] {
                assert!(asset.url.starts_with("https://github.com/"), "{}", asset.url);
                assert_eq!(asset.sha256.len(), 64);
                assert!(asset.size > 1_000_000);
            }
            assert!(a.core.url.contains("0.17.1"), "{}", a.core.url);
            assert!(a.ort.url.contains(a.ort_version), "{}", a.ort.url);
            assert!(!a.core_lib.is_empty() && !a.ort_lib.is_empty());
        }
        for (os, arch) in [
            ("linux", "riscv64"), ("android", "arm"), ("android", "x86"), ("windows", "aarch64"), ("freebsd", "x86_64"), ("ios", "aarch64"),
        ] {
            assert!(engine_assets_for(os, arch).is_none(), "{os} {arch}");
        }
    }

    #[test]
    fn android_uses_ort_1_23_2_and_the_desktop_keeps_1_17_3() {
        for (os, arch) in [("linux", "x86_64"), ("linux", "aarch64"), ("macos", "x86_64"), ("macos", "aarch64"), ("windows", "x86_64")] {
            assert_eq!(engine_assets_for(os, arch).unwrap().ort_version, "1.17.3", "{os} {arch}");
        }
        let a = engine_assets_for("android", "aarch64").unwrap();
        assert_eq!(a.ort_version, "1.23.2");
        assert_eq!(a.core.url, "https://github.com/VOICEVOX/voicevox_core/releases/download/0.17.1/voicevox_core-android-arm64-0.17.1.zip");
        assert_eq!((a.core.size, a.core.sha256.as_str()), (1_306_346, "069f29018ea27d939e3198ef34592a489181ba3d0287819f80c518963c639d37"));
        assert_eq!(a.ort.url, "https://github.com/VOICEVOX/onnxruntime-builder/releases/download/voicevox_onnxruntime-1.23.2/voicevox_onnxruntime-android-arm64-1.23.2.tgz");
        assert_eq!((a.ort.size, a.ort.sha256.as_str()), (6_459_115, "43e6fc2a89ea6412d4cc20215042a3b34da22fdf05c1bb76256a686f82378d46"));
        assert_eq!((a.core_lib, a.ort_lib), ("libvoicevox_core.so", "libvoicevox_onnxruntime.so"));
        let x = engine_assets_for("android", "x86_64").unwrap();
        assert_eq!(x.core.url, "https://github.com/VOICEVOX/voicevox_core/releases/download/0.17.1/voicevox_core-android-x86_64-0.17.1.zip");
        assert_eq!((x.core.size, x.core.sha256.as_str()), (1_366_415, "43e8ef75b199491b6e3126be5f4328542b6a8d3a0984c5e98931b696d752cdc2"));
        assert_eq!(x.ort.url, "https://github.com/VOICEVOX/onnxruntime-builder/releases/download/voicevox_onnxruntime-1.23.2/voicevox_onnxruntime-android-x64-1.23.2.tgz");
        assert_eq!((x.ort.size, x.ort.sha256.as_str()), (7_115_144, "807e2ea50dfb2a309416adb1cee6edc53d3d8eae1d702ad4e6f199a0f91f2d24"));
        // 辞書は全環境で共通。
        assert_eq!(a.dict, engine_assets_for("linux", "x86_64").unwrap().dict);
    }

    #[test]
    fn the_engine_set_name_follows_the_build_target_and_the_desktop_name_is_unchanged() {
        #[cfg(not(target_os = "android"))]
        assert_eq!(ENGINE_SET, "core0.17.1-ort1.17.3-dict1.11", "デスクトップの置き場の名前は変えない");
        #[cfg(target_os = "android")]
        assert_eq!(ENGINE_SET, "core0.17.1-ort1.23.2-dict1.11");
        // ビルド対象の環境の取得物の ORT の版と、置き場の名前が食い違わないこと。
        let a = engine_assets().expect("このテストを走らせる環境は対応している");
        assert_eq!(ENGINE_SET, format!("core{CORE_VERSION}-ort{}-dict{DICT_VERSION}", a.ort_version));
    }
```

`the_library_names_match_the_archives` の末尾(`assert_eq!((w.core_lib, ...` の後)に追記:

```rust
        let a = engine_assets_for("android", "aarch64").unwrap();
        assert_eq!((a.core_lib, a.ort_lib), ("libvoicevox_core.so", "libvoicevox_onnxruntime.so"));
```

(規約の写しの版は、`the_terms_version_is_stable_and_ignores_line_endings` が `terms_version()` の安定を見ている。Android 用の規約の写しは足さない。ONNX Runtime 1.23.2 の `TERMS.txt` は 1.17.3 と一致する。938 バイト。)

- [ ] **Step 2: 失敗することを確かめる**

Run: `cd src-tauri && cargo test --lib tts::voicevox::catalog -- --test-threads=1 2>&1 | tail -20`
Expected: コンパイルエラー(`no field ort_version on type EngineAssets`)。

- [ ] **Step 3: 実装(`catalog.rs`)**

定数(`ORT_VERSION` の直後と `ENGINE_SET`)を次にする:

```rust
/// CORE 0.17.1 の推奨は 1.23.2(prerelease)だが、パスを明示すれば 1.17.3(正式リリースの最新)で動く(スパイクノート)。デスクトップはこれ。
pub const ORT_VERSION: &str = "1.17.3";
/// Android は 1.23.2(prerelease)。1.17.3 の Android 版は ELF が 4KB 整列で、16KB ページの端末で読めない恐れがある(段階3の spec)。
/// 1.23.2 は 16KB 整列で、Pixel 8 Pro の実機で CORE 0.17.1 と合成できた。
pub const ORT_VERSION_ANDROID: &str = "1.23.2";
pub const DICT_VERSION: &str = "1.11";
/// エンジン一式(CORE + ONNX Runtime + 辞書)の識別子。置き場のディレクトリ名に使う。版を変えるときは一緒に変える。
/// ONNX Runtime の版が Android だけ違うので、ビルド対象で値が変わる(`EngineAssets.ort_version` と一致することはテストで見ている)。
#[cfg(not(target_os = "android"))]
pub const ENGINE_SET: &str = "core0.17.1-ort1.17.3-dict1.11";
#[cfg(target_os = "android")]
pub const ENGINE_SET: &str = "core0.17.1-ort1.23.2-dict1.11";
```

`EngineAssets` に、`ort` の次の行として追加:

```rust
    /// ONNX Runtime の版(URL に入る。デスクトップは 1.17.3、Android は 1.23.2)。
    pub ort_version: &'static str,
```

`engine_assets_for` の、`let (core_plat, ort_plat, core_lib, ...` を次に直し、**既存の 5 つの腕**(`linux`/`macos`/`windows`)の 2 つ目の文字列(`ort_plat`)の直後に `ORT_VERSION,` を足し、Android の 2 つの腕を `("windows", "x86_64") => (...)` の後ろ・`_ => return None,` の前に足す:

```rust
    // (CORE の配布名の環境, ONNX Runtime の配布名の環境, ONNX Runtime の版, CORE のライブラリ, ONNX Runtime のライブラリ,
    //  CORE の (サイズ, SHA-256), ONNX Runtime の (サイズ, SHA-256))
    let (core_plat, ort_plat, ort_ver, core_lib, ort_lib, (core_size, core_sha), (ort_size, ort_sha)) = match (os, arch) {
        // 既存の腕の例(ほかの 4 つも同じ形で、`ort_plat` の直後に `ORT_VERSION,` を足す):
        // ("linux", "x86_64") => (
        //     "linux-x64", "linux-x64", ORT_VERSION, "libvoicevox_core.so", "libvoicevox_onnxruntime.so.1.17.3",
        //     (1_457_717, "844b49ce…"), (6_765_720, "72b5287f…"),
        // ),
        ("android", "aarch64") => (
            "android-arm64", "android-arm64", ORT_VERSION_ANDROID, "libvoicevox_core.so", "libvoicevox_onnxruntime.so",
            (1_306_346, "069f29018ea27d939e3198ef34592a489181ba3d0287819f80c518963c639d37"),
            (6_459_115, "43e6fc2a89ea6412d4cc20215042a3b34da22fdf05c1bb76256a686f82378d46"),
        ),
        ("android", "x86_64") => (
            "android-x86_64", "android-x64", ORT_VERSION_ANDROID, "libvoicevox_core.so", "libvoicevox_onnxruntime.so",
            (1_366_415, "43e8ef75b199491b6e3126be5f4328542b6a8d3a0984c5e98931b696d752cdc2"),
            (7_115_144, "807e2ea50dfb2a309416adb1cee6edc53d3d8eae1d702ad4e6f199a0f91f2d24"),
        ),
```

(コメントの例は貼らずに、実際の 5 つの腕を直す。上の `// (...)` の 6 行は説明用。)

`Some(EngineAssets { ... })` の中で、`ort: Asset { url: format!(...{ORT_VERSION}...{ORT_VERSION}.tgz") ...}` の 2 か所の `{ORT_VERSION}` を `{ort_ver}` にし、`ort_version: ort_ver,` を `ort: Asset {...},` の次に足す。`engine_assets` の doc コメント「対応しなければ `None`(Android など)」を「対応しなければ `None`(armv7・i686 の Android、Windows arm64、BSD、iOS など)」にする。

- [ ] **Step 4: テストの組み立てを直す**

`store.rs` の `fn assets() -> EngineAssets` と、`packs.rs` の `fn fake_assets(...) -> EngineAssets` の `EngineAssets { ... }` に `ort_version: "1.17.3",` を足す(`ort:` の次の行)。

Run: `cd src-tauri && cargo test --lib tts::voicevox -- --test-threads=1 2>&1 | tail -15`
Expected: `tts::voicevox` のテストがすべて PASS(新しい 2 つと、直した 2 つを含む。`ignored` は除く)。

- [ ] **Step 5: spec を訂正する**

`docs/superpowers/specs/2026-11-21-issue-625-voicevox-core-stage3-android-design.md` を直す:

1. 「`ORT_VERSION` と `ENGINE_SET`(定数)を、**環境ごとの値**にする。…」の箇条書きを次に置き換える:
   `- ONNX Runtime の版は **`EngineAssets.ort_version`**(環境ごと。デスクトップ 1.17.3、Android 1.23.2)で持ち、URL に使う。置き場の名前 `ENGINE_SET` は定数のまま `cfg(target_os = "android")` で値を切り替える(デスクトップは今までの `core0.17.1-ort1.17.3-dict1.11`、Android は `core0.17.1-ort1.23.2-dict1.11`)。`Store` の API は変えない。`ENGINE_SET` と `ort_version` が食い違わないことは、テストで確かめる。`store.rs` の配置・READY・古い版の掃除は、この値を使う。`
2. テストの節の「`store.rs`: `ENGINE_SET` を引数で受ける形でも、…」の行を削除し、`catalog.rs` の行に「`ENGINE_SET` がビルド対象の `ort_version` と一致する」を含める。
3. UI の節の「デスクトップは `Unknown` なので出ない。」を「デスクトップでは、回線を見ず、確認も出さない(`downloadWarning` は、回線が不明だと確認を返すので、スマホ(`isMobilePlatform`)のときだけ使う)。」にする。同じ節の対応環境の文面の更新(「この環境では…使えません(対応: …)」に Android を含める)を、1 行足す。
4. 「5. 先に確かめる関門」の見出し直後の「実装に入る前に、実機で次の 3 つを確かめる。」を「コードの最小の変更(カタログ・cfg・libc++ の同梱)のあと、UI の変更の前に、エミュレーター(AVD のコピー)で次の 3 つを確かめる。実機(Pixel 8 Pro)への入れ替えインストールは、ユーザーの確認を取ってから行う。」にする。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/tts/voicevox/catalog.rs src-tauri/src/tts/voicevox/store.rs src-tauri/src/tts/voicevox/packs.rs docs/superpowers/specs/2026-11-21-issue-625-voicevox-core-stage3-android-design.md
git commit -m "feat(tts): 内蔵 VOICEVOX CORE のカタログに Android の取得物を足す(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Android でネイティブのロードを有効にする(cfg・`libloading`)

**Files:**
- Modify: `src-tauri/src/tts/voicevox/ffi.rs`(`:2` のドキュメント、`:34`・`:37`)
- Modify: `src-tauri/src/tts/voicevox/mod.rs`(`:2`、`:13` の `cfg_attr`、`:49`・`:58`)
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: Task 1 の `EngineAssets`。
- Produces: Android(と、iOS 以外のすべて)で `ffi::NativeCore` と `mod.rs` の `native_factory`(ロードする版)がビルドされる。iOS だけ「非対応」のファクトリ。

自動テストで RED にできるのは cfg の幅だけで、振る舞い(実際に dlopen できること)は Task 5 の実機・エミュレーターで確かめる。このタスクの検証は、ホストのテストが緑のまま、4 つの Android ターゲットで `cargo check` が通ること。

- [ ] **Step 1: cfg を広げる**

`ffi.rs`: 2 行目のドキュメントの「`cfg(desktop)` だけ」を「iOS 以外(デスクトップと Android)だけ」にする。`#[cfg(desktop)]` の 2 か所(`pub use native::NativeCore;` と `mod native`)を `#[cfg(not(target_os = "ios"))]` にする。`#[cfg(all(test, desktop, target_os = "linux", target_arch = "x86_64"))]`(実物のテスト)は変えない。

`mod.rs`: 2 行目のドキュメントを「ネイティブの呼び出し(`ffi::NativeCore`)は iOS 以外(デスクトップと Android)。ほかは全環境でビルドし、armv7・i686 の Android などでは `supported = false`。」にする。`#[cfg_attr(not(desktop), allow(unused_imports))]` を `#[cfg_attr(target_os = "ios", allow(unused_imports))]` に、`#[cfg(desktop)] fn native_factory` を `#[cfg(not(target_os = "ios"))]` に、`#[cfg(not(desktop))] fn native_factory` を `#[cfg(target_os = "ios")]` にする。`#[cfg(all(test, desktop))] mod real_tests` は変えない。

- [ ] **Step 2: `libloading` を iOS 以外に入れる**

`src-tauri/Cargo.toml`:

`[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]` の中の、次の 2 行を削除する:

```toml
# 内蔵 VOICEVOX CORE(Issue #625 段階2)。取得した公式の C API ライブラリを動的ロードする。
libloading = "0.8"
```

`[target.'cfg(target_os = "macos")'.dependencies]` の前に、新しい節を足す:

```toml
# 内蔵 VOICEVOX CORE(Issue #625 段階2・3)。取得した公式の C API ライブラリを動的ロードする。iOS 以外(デスクトップと Android)。
[target.'cfg(not(target_os = "ios"))'.dependencies]
libloading = "0.8"
```

- [ ] **Step 3: ホストのテストが緑のまま**

Run: `cd src-tauri && cargo test --lib tts::voicevox -- --test-threads=1 2>&1 | tail -8`
Expected: PASS(ホストは Linux なので、挙動は変わらない)。

- [ ] **Step 4: 4 つの Android ターゲットでコンパイルが通る**

ユーザーの `cargo tauri dev` が動いている(`ss -ltn | grep 5173` が出る)間はビルドロックを取り合うので、その場合は `export CARGO_TARGET_DIR=$HOME/.cache/tsumugi-625s3/target`(ディスク側)にする。まず `free -h` で空きとスワップを確かめる。

```bash
export ANDROID_NDK_ROOT=$HOME/Android/Sdk/ndk/27.2.12479018 ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018 NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018
export ANDROID_PLATFORM=android-26 JAVA_HOME=/usr/lib/jvm/java-21-openjdk
BIN=$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin
export PATH=$BIN:$PATH
export CC_aarch64_linux_android=$BIN/aarch64-linux-android26-clang CXX_aarch64_linux_android=$BIN/aarch64-linux-android26-clang++ AR_aarch64_linux_android=$BIN/llvm-ar
export CC_x86_64_linux_android=$BIN/x86_64-linux-android26-clang CXX_x86_64_linux_android=$BIN/x86_64-linux-android26-clang++ AR_x86_64_linux_android=$BIN/llvm-ar
export CC_armv7_linux_androideabi=$BIN/armv7a-linux-androideabi26-clang CXX_armv7_linux_androideabi=$BIN/armv7a-linux-androideabi26-clang++ AR_armv7_linux_androideabi=$BIN/llvm-ar
export CC_i686_linux_android=$BIN/i686-linux-android26-clang CXX_i686_linux_android=$BIN/i686-linux-android26-clang++ AR_i686_linux_android=$BIN/llvm-ar
cd src-tauri
for t in aarch64-linux-android x86_64-linux-android armv7-linux-androideabi i686-linux-android; do
  echo "== $t"; CARGO_BUILD_JOBS=4 cargo check --lib --tests --target $t --no-default-features --features custom-protocol 2>&1 | tail -3
done
```

Expected: 4 つとも `Finished`(エラー 0)。32bit(armv7・i686)でも `NativeCore` がコンパイルされる(`usize`/`uintptr_t` の幅の違いで落ちないこと)。落ちたら、`ffi.rs` の型を直す(`systematic-debugging` で原因を見てから)。`rustup target list --installed` に 4 つが入っていることは確認済み。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/tts/voicevox/ffi.rs src-tauri/src/tts/voicevox/mod.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(tts): 内蔵 VOICEVOX CORE のネイティブのロードを Android でも有効にする(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

(`Cargo.lock` に変更が無ければ、`git add` のその 1 つは外す。)

---

### Task 3: アイドル解放を Android は 3 分にする

**Files:**
- Modify: `src-tauri/src/tts/voicevox/engine.rs`(`:16`)

**Interfaces:**
- Consumes: `translate/mod.rs` の同じ形(`#[cfg(target_os = "android")]` で 180 秒)。
- Produces: `engine::IDLE_UNLOAD`(Android 180 秒、それ以外 600 秒)。`lib.rs:459` がそのまま使う。

デスクトップのテストは、今の挙動(600 秒)を固定するもの(最初から通る)。Android のテストは Android ターゲットでしかコンパイルされないので、ホストでは RED にならない。`cargo check --tests` で Android 側のコンパイルだけ確かめる(翻訳の `idle_unload_is_three_minutes_on_android` と同じ流儀)。

- [ ] **Step 1: テストを書く(`engine.rs` の `mod tests`)**

```rust
    #[cfg(target_os = "android")]
    #[test]
    fn idle_unload_is_three_minutes_on_android() {
        assert_eq!(IDLE_UNLOAD, Duration::from_secs(180));
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn idle_unload_is_ten_minutes_on_desktop() {
        assert_eq!(IDLE_UNLOAD, Duration::from_secs(600));
    }
```

- [ ] **Step 2: 実装**

`engine.rs:16` を置き換える:

```rust
/// 使われなくなってから、ロード済みのモデルを解放するまで(翻訳モデルと同じ)。
/// Android は、合成のピークが約 500 MB(Pixel 8 Pro の実測)でメモリが厳しいので短くする。
#[cfg(target_os = "android")]
pub const IDLE_UNLOAD: Duration = Duration::from_secs(180);
#[cfg(not(target_os = "android"))]
pub const IDLE_UNLOAD: Duration = Duration::from_secs(600);
```

- [ ] **Step 3: 検証**

Run: `cd src-tauri && cargo test --lib tts::voicevox::engine 2>&1 | tail -6`
Expected: PASS(`idle_unload_is_ten_minutes_on_desktop` を含む)。

Run: Task 2 Step 4 と同じ環境変数で `cargo check --lib --tests --target aarch64-linux-android --no-default-features --features custom-protocol 2>&1 | tail -3`
Expected: `Finished`。

- [ ] **Step 4: コミット**

```bash
git add src-tauri/src/tts/voicevox/engine.rs
git commit -m "feat(tts): 内蔵エンジンのアイドル解放を Android では 3 分にする(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `libc++_shared.so` を APK に入れる(Gradle)

**Files:**
- Modify: `src-tauri/gen/android/app/build.gradle.kts`

**Interfaces:**
- Consumes: NDK r27c の `toolchains/llvm/prebuilt/<host>/sysroot/usr/lib/<triple>/libc++_shared.so`。
- Produces: APK の `lib/arm64-v8a/libc++_shared.so` と `lib/x86_64/libc++_shared.so`(armeabi-v7a・x86 には入れない)。

この変更の検証は APK のビルドが要る(Task 5 でビルドして、APK の中身を確かめる)。ここでは書くだけ。

- [ ] **Step 1: Gradle に、コピーのタスクを足す**

`app/build.gradle.kts` の `rust { rootDirRel = "../../../" }` の直前に追加:

```kotlin
// 内蔵 VOICEVOX CORE(Issue #625 段階3)の libvoicevox_core.so は、共有の libc++_shared.so を要求する。
// アプリ本体は静的 libc++ なので、NDK のものを、ビルド時にコピーして APK に入れる(バイナリはリポジトリに入れない)。
// 64bit の ABI だけ(内蔵は arm64 と x86_64 でだけ使える)。NDK は CI・ローカルとも、環境変数で r27c を指す(CLAUDE.md)。
val voicevoxLibcxxDir = layout.buildDirectory.dir("generated/voicevoxLibcxx")
val copyLibcxxShared by tasks.registering {
    val outDir = voicevoxLibcxxDir
    outputs.dir(outDir)
    doLast {
        val ndkFromEnv = listOf("ANDROID_NDK_ROOT", "ANDROID_NDK_HOME", "NDK_HOME")
            .firstNotNullOfOrNull { System.getenv(it)?.takeIf { v -> v.isNotBlank() } }
        val ndk = if (ndkFromEnv != null) File(ndkFromEnv) else android.ndkDirectory
        val prebuilt = File(ndk, "toolchains/llvm/prebuilt").listFiles()?.firstOrNull()
            ?: throw GradleException("NDK の toolchains/llvm/prebuilt が見つかりません: $ndk")
        val out = outDir.get().asFile
        mapOf("arm64-v8a" to "aarch64-linux-android", "x86_64" to "x86_64-linux-android").forEach { (abi, triple) ->
            val src = File(prebuilt, "sysroot/usr/lib/$triple/libc++_shared.so")
            if (!src.isFile) throw GradleException("libc++_shared.so が見つかりません: $src")
            File(out, abi).mkdirs()
            src.copyTo(File(out, "$abi/libc++_shared.so"), overwrite = true)
        }
    }
}
android.sourceSets.getByName("main").jniLibs.srcDir(voicevoxLibcxxDir)
tasks.configureEach {
    if (name.startsWith("merge") && name.endsWith("JniLibFolders")) dependsOn(copyLibcxxShared)
}
```

- [ ] **Step 2: コミット**

```bash
git add src-tauri/gen/android/app/build.gradle.kts
git commit -m "build(android): 内蔵 VOICEVOX CORE が要る libc++_shared.so を APK に入れる(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: 関門(エミュレーターで、アプリ内の dlopen・libc++ の同居・16KB ページ)

**Files:**
- 変更なし(結果は spec の「実機・エミュレーターでの確認」に追記する。Task 7 でまとめてコミット)。作業ファイルはスクラッチパッドに置く。

**Interfaces:**
- Consumes: Task 1〜4 の成果、`tsumugi-e2e`(API 36、x86_64)の AVD、`adb`、CDP。
- Produces: 3 つの関門の結果(通った / 通らなかった)。**どれかが通らなければ、止めて spec の見直しをユーザーに相談する**(「アプリ内で dlopen できない」なら APK 同梱へ切り替える)。

- [ ] **Step 1: ビルドの前に、資源を確かめる**

Run: `free -h; ss -ltn | grep 5173 || echo "dev server なし"; df -h /home | tail -1; ls ~/Android/Sdk/system-images/*/* -d`
Expected: 空き+スワップに余裕(目安: 空き 12 GB 以上)。ユーザーの `cargo tauri dev` が動いているなら、ビルドは別の `CARGO_TARGET_DIR=$HOME/.cache/tsumugi-625s3/target`(ディスク側。`/tmp` は不可)で、単独で先に終わらせる(エミュレーターや Misskey は、ビルドが終わってから起動する)。

- [ ] **Step 2: x86_64 の debug APK をビルドする**

```bash
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/27.2.12479018 ANDROID_NDK_HOME=$ANDROID_NDK_ROOT NDK_HOME=$ANDROID_NDK_ROOT ANDROID_PLATFORM=android-26
export CARGO_BUILD_JOBS=3   # 別の出力先を使うときは、ここで CARGO_TARGET_DIR も export する
cd src-tauri && systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G cargo tauri android build --debug --target x86_64 --apk 2>&1 | tail -15
```

Expected: ビルド成功(初回は 15 分前後。時間がかかるので、`run_in_background` で走らせて完了通知を待つ)。Gradle の設定で `android.ndkDirectory` の参照などが落ちたら、Task 4 のコードを直す(環境変数で NDK が決まるので、`ndkFromEnv` 側が使われるはず)。APK は `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk` 付近(`find src-tauri/gen/android/app/build -name '*.apk'` で確かめる)。

- [ ] **Step 3: APK の中身を確かめる(libc++ の同梱と整列)**

```bash
APK=$(find src-tauri/gen/android/app/build -name 'app-universal-debug.apk' | head -1)
unzip -l "$APK" | grep -E "libc\+\+_shared|libtsumugi_lib"
mkdir -p $SCRATCH/apk && unzip -o -q "$APK" 'lib/x86_64/*' -d $SCRATCH/apk
$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf -lW $SCRATCH/apk/lib/x86_64/libc++_shared.so | grep LOAD | awk '{print $NF}' | sort -u
```

(`SCRATCH` は、セッションのスクラッチパッドのディレクトリ。)
Expected: `lib/x86_64/libc++_shared.so` が入っていて、LOAD の整列が `0x4000`。(`--target x86_64` だけのビルドなので arm64 の分は Task 7 の arm64 ビルドで確かめる。)入っていなければ Task 4 のコードを直す(Review Focus 5)。

- [ ] **Step 4: AVD のコピーを起動して、APK を入れる**

ユーザーの AVD は触らない。`~/.config/.android/avd/tsumugi-e2e.avd` を `~/.cache/tsumugi-625s3/avd/` へ `cp -r --reflink=auto` でコピーし、`.ini` の `path=` を書き換え、`ANDROID_AVD_HOME` をそこにして、`-read-only -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect -no-snapshot -port 5570` で起動する(手順は `reference-android-emulator-environment` のメモのとおり。`adb` は必ず `-s emulator-5570`)。

```bash
e(){ adb -s emulator-5570 "$@"; }
e wait-for-device; e shell getprop sys.boot_completed   # 1 になるまで待つ
e install -r "$APK"
e shell monkey -p com.onodai.tsumugi.debug -c android.intent.category.LAUNCHER 1
```

Expected: インストール `Success`、アプリが起動する(アカウントの無い画面でよい)。

- [ ] **Step 5: CDP でコマンドを直接叩く(関門 1・2)**

```bash
PID=$(e shell pidof com.onodai.tsumugi.debug)
e forward tcp:9333 localabstract:webview_devtools_remote_$PID
curl -s localhost:9333/json/list | head -20   # webSocketDebuggerUrl を取る
```

Node 22 の標準 `WebSocket` で、`Runtime.evaluate`(`awaitPromise: true, returnByValue: true`)を送るスクリプト(`$SCRATCH/gate.mjs`。結果を出したら `process.exit(0)`)を書き、`window.__TAURI_INTERNALS__.invoke("<command>", {...})` を次の順で呼ぶ。引数の名前は `frontend/src/bindings/tauri.gen.ts` の `commands.getSpeechCoreInfo` / `startSpeechCoreDownload` / `getSpeechSettings` / `setSpeechSettings` / `testSpeech` / `getSpeechStatus` で確かめる(camelCase):

1. `get_speech_core_info` → `supported: true`、`engine`・`voices` が `notInstalled`。
2. `start_speech_core_download({ pack: "engine", termsVersion: <info.termsVersion> })` → `get_speech_core_info` を 2 秒おきに引いて、`engine.status.type` が `downloading → verifying → ready` になるまで待つ(約 31 MB)。`failed` になったら、その `reason` を記録して止める。
3. `start_speech_core_download({ pack: "0", ... })` → 同様に `ready`(約 59 MB)。
4. 設定の `engine` を `"voicevoxCore"`、既定の声を `"3"` にして保存(`get_speech_settings` の値を直して `set_speech_settings`)。
5. `test_speech` を呼び、`get_speech_status` が `ok`(エラーでない)になるのを待つ。同時に `e logcat -d | grep -E "voicevox|SIGSEGV|Fatal signal|dlopen"` を見る。

Expected(関門 1): 手順 2〜3 で取得が完了し、手順 5 が `ok`(`voicevox_core::synthesizer: CPUを利用します` がログに出る)。`SPEECH_CORE_LOAD_FAILED` になったら、logcat の `dlopen failed: ...` の文言が原因(SELinux の拒否なら `avc: denied { execute }` / `{ map }` が出る)。**通らなければ、ここで止めて、ログを添えてユーザーに相談する**(APK 同梱への切り替えは spec の改訂)。
Expected(関門 2): 同じプロセスでクラッシュしない(`libtsumugi_lib.so` は静的 libc++、CORE は共有 libc++)。続けて `test_speech` を 5 回呼んでも落ちない。ローカル翻訳のモデルが端末に入っていれば、翻訳も 1 回動かす(入っていなければ、未確認として記録する)。

- [ ] **Step 6: 16KB ページのエミュレーターで確かめる(関門 3)**

`ls ~/Android/Sdk/system-images/*/*` に `*ps16k*`(例: `google_apis_ps16k`)があれば、その AVD のコピー(なければ新しく作る。`avdmanager create avd`)を `-read-only` で起動し、`adb shell getconf PAGE_SIZE` が `16384` であることを確かめてから、Step 4〜5 を同じ手順で繰り返す(arm64 の端末は実機でないので、x86_64 の 16KB イメージ)。16KB のイメージが入っていない場合は、ダウンロード(数 GB)になるので、**ユーザーに確認してから** `sdkmanager` で入れる。断られたら「関門 3 は未確認」と記録して先へ進む。

Expected: `PAGE_SIZE` が 16384 のエミュレーターで、ONNX Runtime 1.23.2 のロードと合成が `ok`。

- [ ] **Step 7: 後片付け**

```bash
e emu kill; adb -s emulator-5570 forward --remove tcp:9333 2>/dev/null || adb forward --remove-all   # 自分で張ったものだけ
rm -rf ~/.cache/tsumugi-625s3/avd   # AVD のコピー(ビルド出力を別に置いた場合は、Task 7 の最後まで残す)
```

結果(関門ごとの通った/通らなかった、ログの要点、未確認の項目)は、次のタスクで spec に追記する。

---

### Task 6: 取得の前の回線の確認(Android のみ)と、非対応の文面

**Files:**
- Modify: `frontend/src/lib/speechCore.ts`、`frontend/src/lib/speechCore.test.ts`
- Modify: `frontend/src/ui/settings/SpeechCoreSection.svelte`、`frontend/src/ui/settings/SpeechCoreSection.test.ts`

**Interfaces:**
- Consumes: `lib/localModel.ts` の `downloadWarning(net: NetworkStatus, sizeBytes: number): string | null`・`UNKNOWN_NETWORK`、`commands.getNetworkStatus()`(失敗しない。`NetworkStatus` をそのまま返す)、`lib/platform.ts` の `isMobilePlatform`、`ui/ConfirmDialog.svelte`(`title`・`message`・`confirmLabel`・`onConfirm`・`onCancel`)。
- Produces: `coreDownloadWarning(net: NetworkStatus, sizeBytes: number, mobile: boolean): string | null`(`lib/speechCore.ts`)。`SpeechCoreSection` の取得ボタンが、スマホで確認が要るときに、確認ダイアログを先に出す。

- [ ] **Step 1: 失敗するテストを書く(純粋関数 `lib/speechCore.test.ts`)**

既存のテストファイルの末尾に追記する(import に `coreDownloadWarning` を足す。`NetworkStatus` は `import type { NetworkStatus } from "./ipc"`):

```ts
describe("coreDownloadWarning", () => {
  const cellular: NetworkStatus = { transport: "cellular", metered: "metered" };
  const wifi: NetworkStatus = { transport: "wifi", metered: "unmetered" };
  const unknown: NetworkStatus = { transport: "unknown", metered: "unknown" };

  it("デスクトップ(mobile=false)は、回線がどうでも確認を出さない", () => {
    expect(coreDownloadWarning(cellular, 59_000_000, false)).toBeNull();
    expect(coreDownloadWarning(unknown, 59_000_000, false)).toBeNull();
  });

  it("スマホでモバイル通信なら、取得するサイズ入りの確認を返す", () => {
    expect(coreDownloadWarning(cellular, 59_308_488, true)).toContain("59.3 MB");
    expect(coreDownloadWarning(cellular, 59_308_488, true)).toContain("モバイル通信");
  });

  it("スマホで Wi-Fi(非従量制)なら確認なし、判定できないときは注記つきの確認", () => {
    expect(coreDownloadWarning(wifi, 59_308_488, true)).toBeNull();
    expect(coreDownloadWarning(unknown, 59_308_488, true)).toContain("判定できませんでした");
  });
});
```

- [ ] **Step 2: 失敗することを確かめる**

Run: `cd frontend && npx vitest run src/lib/speechCore.test.ts 2>&1 | tail -12`
Expected: FAIL(`coreDownloadWarning is not a function` または import エラー)。

- [ ] **Step 3: 純粋関数を実装(`lib/speechCore.ts`)**

先頭の import に追加し、末尾に関数を足す:

```ts
import { downloadWarning } from "./localModel";
import type { NetworkStatus } from "./ipc";
```

```ts
/// 内蔵エンジンの取得の前の、回線の確認の文面。スマホ(Android)だけで出し、デスクトップは従来どおり確認しない。
/// `downloadWarning` は、回線を判定できないとき(デスクトップはこれ)にも確認を返すので、`mobile` で先に分ける。
export function coreDownloadWarning(net: NetworkStatus, sizeBytes: number, mobile: boolean): string | null {
  return mobile ? downloadWarning(net, sizeBytes) : null;
}
```

Run: `cd frontend && npx vitest run src/lib/speechCore.test.ts 2>&1 | tail -6`
Expected: PASS。

- [ ] **Step 4: 失敗するコンポーネントのテストを書く(`SpeechCoreSection.test.ts`)**

ファイル先頭の `vi.mock(...)` 群の直後に、プラットフォームのモックを足す(`const { default: SpeechCoreSection } = await import(...)` より前):

```ts
const mobile = vi.hoisted(() => ({ value: false }));
vi.mock("../../lib/platform", () => ({
  get isMobilePlatform() {
    return mobile.value;
  },
}));
```

`beforeEach` の先頭に `mobile.value = false;` を足し、`describe("SpeechCoreSection", ...)` の中に追記:

```ts
  describe("回線の確認(スマホのみ)", () => {
    const ready = () => info({ termsAccepted: true, engine: pack({ type: "ready" }) });

    it("デスクトップでは、回線を読まず、確認も出さずに開始する", async () => {
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(ready()));
      const net = vi.spyOn(commands, "getNetworkStatus");
      const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
      const { findByTestId, queryByText } = render(SpeechCoreSection);
      await fireEvent.click(await findByTestId("speech-core-voice-0-get"));
      await vi.waitFor(() => expect(start).toHaveBeenCalledWith("0", "v1"));
      expect(net).not.toHaveBeenCalled();
      expect(queryByText(/モバイル通信/)).toBeNull();
    });

    it("スマホのモバイル通信では、確認が先に出て、承認するまで開始しない", async () => {
      mobile.value = true;
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(ready()));
      vi.spyOn(commands, "getNetworkStatus").mockResolvedValue({ transport: "cellular", metered: "metered" } as never);
      const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
      const { findByTestId, findByText, getByRole } = render(SpeechCoreSection);
      await fireEvent.click(await findByTestId("speech-core-voice-0-get"));
      expect(await findByText(/モバイル通信を使用中です。約 59.3 MB/)).toBeTruthy();
      expect(start).not.toHaveBeenCalled();
      await fireEvent.click(getByRole("button", { name: "取得" }));
      await vi.waitFor(() => expect(start).toHaveBeenCalledWith("0", "v1"));
    });

    it("確認でキャンセルすると、取得も規約の画面も始まらない", async () => {
      mobile.value = true;
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
      vi.spyOn(commands, "getNetworkStatus").mockResolvedValue({ transport: "cellular", metered: "metered" } as never);
      const start = vi.spyOn(commands, "startSpeechCoreDownload").mockResolvedValue(ok(null));
      const { findByTestId, findByText, getByRole, queryByText, queryByTestId } = render(SpeechCoreSection);
      await fireEvent.click(await findByTestId("speech-core-engine-get"));
      await findByText(/モバイル通信を使用中です/);
      await fireEvent.click(getByRole("button", { name: "キャンセル" }));
      expect(queryByText(/モバイル通信を使用中です/)).toBeNull();
      expect(queryByTestId("speech-terms-dialog")).toBeNull();
      expect(start).not.toHaveBeenCalled();
    });

    it("未同意なら、確認を承認したあとに規約が出る。Wi-Fi(非従量制)なら確認は出ずに規約へ", async () => {
      mobile.value = true;
      vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info()));
      const net = vi.spyOn(commands, "getNetworkStatus").mockResolvedValue({ transport: "cellular", metered: "metered" } as never);
      const { findByTestId, findByText, getByRole, queryByText } = render(SpeechCoreSection);
      await fireEvent.click(await findByTestId("speech-core-engine-get"));
      await findByText(/モバイル通信を使用中です/);
      await fireEvent.click(getByRole("button", { name: "取得" }));
      expect(await findByTestId("speech-terms-dialog")).toBeTruthy();
      await fireEvent.click(await findByTestId("speech-terms-cancel"));
      net.mockResolvedValue({ transport: "wifi", metered: "unmetered" } as never);
      await fireEvent.click(await findByTestId("speech-core-engine-get"));
      expect(await findByTestId("speech-terms-dialog")).toBeTruthy();
      expect(queryByText(/モバイル通信を使用中です/)).toBeNull();
    });
  });

  it("対応しない環境の文面に、Android の対応も含まれる", async () => {
    vi.spyOn(commands, "getSpeechCoreInfo").mockResolvedValue(ok(info({ supported: false })));
    const { findByTestId } = render(SpeechCoreSection);
    expect((await findByTestId("speech-core-unsupported")).textContent).toContain("Android");
  });
```

(`getByRole("button", { name: "取得" })` が、ダイアログの確認ボタンだけに当たらず、一覧の「取得」ボタンとも当たって複数ヒットする場合は、`ConfirmDialog` のルート(`role="dialog"` があれば `within(getByRole("dialog"))`)に絞る。実装を見て直す。)

- [ ] **Step 5: 失敗することを確かめる**

Run: `cd frontend && npx vitest run src/ui/settings/SpeechCoreSection.test.ts 2>&1 | tail -25`
Expected: 新しい 5 つが FAIL(確認が出ない、「Android」が無い)。既存のテストは PASS のまま。

- [ ] **Step 6: コンポーネントを実装(`SpeechCoreSection.svelte`)**

import に追加:

```ts
  import { commands, events, unwrap } from "../../lib/ipc";           // 既存
  import { formatBytes, statusText, UNKNOWN_NETWORK } from "../../lib/localModel";   // UNKNOWN_NETWORK を足す
  import { coreDownloadWarning, voiceCharactersLabel } from "../../lib/speechCore";  // coreDownloadWarning を足す
  import { isMobilePlatform } from "../../lib/platform";
  import ConfirmDialog from "../ConfirmDialog.svelte";
```

状態を足す(`let starting = $state(false);` の次):

```ts
  /// スマホで、回線の確認を出している間だけ非 null。
  let networkConfirm = $state<{ pack: string; sizeBytes: number; message: string } | null>(null);
  let checking = $state(false);
```

`onGet` を次の 2 つの関数に置き換える(既存の本体は `proceed` に移す):

```ts
  /// スマホで、モバイル通信・従量制・判定できないときは、同意の前に回線の確認を出す(翻訳モデルの取得と同じ流儀)。
  /// デスクトップは回線を見ない(従来どおり)。状態を取得できなくても、不明として確認を出すだけで、続ければ開始できる。
  async function onGet(pack: string, sizeBytes: number) {
    if (!info || starting || checking) return;
    err = null;
    if (isMobilePlatform) {
      checking = true;
      let warning: string | null;
      try {
        warning = coreDownloadWarning(await commands.getNetworkStatus(), sizeBytes, true);
      } catch {
        warning = coreDownloadWarning(UNKNOWN_NETWORK, sizeBytes, true);
      } finally {
        checking = false;
      }
      if (warning) {
        networkConfirm = { pack, sizeBytes, message: warning };
        return;
      }
    }
    await proceed(pack, sizeBytes);
  }

  /// 同意済みの版なら、そのまま開始する。未同意(規約が変わったときを含む)なら、規約を出して同意を取る。
  async function proceed(pack: string, sizeBytes: number) {
    if (!info || starting) return;
    if (info.termsAccepted) {
      await start(pack);
      return;
    }
    try {
      terms = await unwrap(commands.getSpeechCoreTerms());
      pending = { pack, sizeBytes };
    } catch (e) {
      err = String(e);
    }
  }
```

(元の `/// 同意済みの版なら…` のコメントと `async function onGet` は `proceed` / 新しい `onGet` に置き換わるので、重複を残さない。)

非対応の文面(`speech-core-unsupported` の `<p>` の本文)を置き換える:

```
この環境では、内蔵の読み上げエンジンを使えません(対応: Linux・macOS の x64 と ARM64、Windows の x64、Android の ARM64 と x86_64)。外部のエンジンを使ってください。
```

テンプレートの末尾(`{#if terms && pending}` の前)に追加:

```svelte
{#if networkConfirm}
  <ConfirmDialog
    title="取得の確認"
    message={networkConfirm.message}
    confirmLabel="取得"
    onConfirm={() => {
      const c = networkConfirm!;
      networkConfirm = null;
      void proceed(c.pack, c.sizeBytes);
    }}
    onCancel={() => (networkConfirm = null)}
  />
{/if}
```

- [ ] **Step 7: 通ることを確かめる**

Run: `cd frontend && npx vitest run src/ui/settings/SpeechCoreSection.test.ts src/lib/speechCore.test.ts 2>&1 | tail -10`
Expected: PASS(ファイル内のすべて)。続けて `cd frontend && pnpm check 2>&1 | tail -5`(エラー 0)と `pnpm test 2>&1 | tail -6`(全件 PASS)。

- [ ] **Step 8: コミット**

```bash
git add frontend/src/lib/speechCore.ts frontend/src/lib/speechCore.test.ts frontend/src/ui/settings/SpeechCoreSection.svelte frontend/src/ui/settings/SpeechCoreSection.test.ts
git commit -m "feat(tts): Android で内蔵エンジンを取得する前に回線の確認を出す(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: 文書・実機の最終確認・全体のテスト

**Files:**
- Modify: `docs/guide/user-guide.md`、`CLAUDE.md`、`docs/superpowers/specs/2026-11-21-issue-625-voicevox-core-stage3-android-design.md`

**Interfaces:**
- Consumes: Task 1〜6 の成果と、Task 5 の関門の結果。
- Produces: 更新済みの文書、spec の確認結果の節、全体のテストの緑。

- [ ] **Step 1: `user-guide.md`**

「**内蔵の VOICEVOX CORE(外部のエンジンなしで読み上げる)**」の節の最初の文を置き換える:

`外部のエンジンを立てなくても読み上げられます(Linux・macOS・Windows の x64、Linux・macOS の ARM、Android の ARM64 と x86_64)。`

「- **容量と削除**」の行の前に追加:

`- **Android**: 声を読み上げている間は、約 500 MB のメモリを使います。使わなくなって 3 分たつと、メモリから解放します。モバイル通信・従量制の接続では、取得の前に確認が出ます。32 ビットの端末(armv7・i686)では内蔵を使えません(外部のエンジンは使えます)。`

- [ ] **Step 2: `CLAUDE.md`**

`tts/` の段落(`**内蔵 VOICEVOX CORE**(段階2、…)` の文)を、次のように更新する:

- 「`ffi.rs`(`CoreBackend` trait と、`cfg(desktop)` の `NativeCore`。」→「`ffi.rs`(`CoreBackend` trait と、`cfg(not(target_os = "ios"))`(デスクトップと Android)の `NativeCore`。」
- 段落の末尾の「実物のテストは `#[ignore]`(`tts::voicevox::ffi::real_tests`、`tts::voicevox::real_tests::real_end_to_end`)。」の直後に追記:
  `**Android(段階3、設計は `docs/superpowers/specs/2026-11-21-issue-625-voicevox-core-stage3-android-design.md`)**は arm64・x86_64 で対応し(armv7・i686 は `supported = false`)、同じ仕組みで取得物を足した。ONNX Runtime だけ **1.23.2(prerelease)**(1.17.3 の Android 版は 4KB 整列。`EngineAssets.ort_version`、置き場の名前 `ENGINE_SET` は `cfg(target_os = "android")` で切り替わる)。`libvoicevox_core.so` が要る `libc++_shared.so`(アプリは静的 libc++)は、`app/build.gradle.kts` が NDK からコピーして APK に入れる(`ANDROID_NDK_ROOT` で NDK を決める)。アイドル解放は Android が 3 分。取得の前の回線の確認は、スマホ(`isMobilePlatform`)のときだけ(`lib/speechCore.ts` の `coreDownloadWarning`。`downloadWarning` は回線不明でも確認を返すので、デスクトップでは使わない)。取得した `.so` を、アプリのデータディレクトリから dlopen できることは、実機・エミュレーターで確かめた(Task の結果は spec)。`

- [ ] **Step 3: arm64 の実機の確認(ユーザーの確認を取ってから)**

実機(Pixel 8 Pro)に入っている `com.onodai.tsumugi.debug` は、ユーザーのものである。**上書きインストールの前に、ユーザーに確認する**(署名が違うと、アンインストールが必要でデータが消える。`adb install -r` が `INSTALL_FAILED_UPDATE_INCOMPATIBLE` になったら、そこで止めて相談する)。承認が得られたら:

1. arm64 の APK をビルドする(Task 5 Step 2 と同じ環境変数で `--target aarch64`。`unzip -l` で `lib/arm64-v8a/libc++_shared.so` が入っていること、`llvm-readelf -lW` の整列が `0x4000` であることを確かめる)。
2. 実機に `adb -s <実機のシリアル> install -r` で入れ、実アプリで、設定 → 読み上げ →「内蔵の VOICEVOX CORE」を選び、エンジン一式と声 1 つを取得し(モバイル通信に切り替えて、確認が出ることも見る)、試し聞きで**実際に音が出る**こと(ユーザーに聞いてもらう)、カラムの読み上げ、取得中のキャンセルと再開、アプリの再起動後の状態、声の削除を確かめる。
3. 声の管理画面(検索・スタイルの `<details>`・一覧の並び)の見た目をスクリーンショット(`adb exec-out screencap -p`)で確かめ、崩れていたら、スマホ幅の見た目だけを直す(機能は変えない。直したらテストを足して Task 6 と同様にコミットする)。
4. 声の取得で増えたデータは、確認のあとに、実機のアプリの設定から削除する(ユーザーのデータを増やしたままにしない)。

承認が得られなければ、実機の確認は「未確認(エミュレーターのみ)」と spec に記録して先へ進む。

- [ ] **Step 4: spec に、確認結果の節を追記する**

spec の「リスクと未確認事項」の前に、`## 実機・エミュレーターでの確認(段階3の実装時)` を足し、関門 1〜3 の結果、arm64 の実機の確認の結果(音が出たか、取得にかかった時間、スマホ幅の見た目)、未確認の項目(例: 16KB イメージを入れなかった、実機に入れ替えなかった、ローカル翻訳との同居を動かさなかった)を、事実のとおりに書く。

- [ ] **Step 5: 全体のテストと検証**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -5`(`generates_frontend_bindings` を含む。Task 1 以降で TS バインディングは変わらないはず。変わったら、差分を確認する)
Run: `cd frontend && pnpm check 2>&1 | tail -4 && pnpm test 2>&1 | tail -5`
Run: Task 2 Step 4 と同じ環境で、4 つの Android ターゲットの `cargo check --lib --tests ... --no-default-features --features custom-protocol`。
Expected: すべて緑(Rust 1354 件以上・ignored 96 件程度、フロント 1598 件以上、`pnpm check` エラー 0、4 ターゲット `Finished`)。

- [ ] **Step 6: 後片付け**

自分で起動したエミュレーター・`adb forward`・ビルド出力(`~/.cache/tsumugi-625s3`)を、正確な PID・パスだけで止めて消す(`git clean` や `pkill` は使わない)。`git status` がきれいなことを確かめる。

- [ ] **Step 7: コミット**

```bash
git add docs/guide/user-guide.md CLAUDE.md docs/superpowers/specs/2026-11-21-issue-625-voicevox-core-stage3-android-design.md
git commit -m "docs(tts): 内蔵 VOICEVOX CORE の Android 対応を、ガイド・CLAUDE.md・確認結果に反映する(Issue #625)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
