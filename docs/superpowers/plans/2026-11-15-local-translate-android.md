# ローカル翻訳（段階3: Android）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** PLaMo 3 Translate のローカル翻訳を Android（ビルドできる ABI すべて）でも動かし、ダウンロード前に回線の種類と従量制を警告する。

**Architecture:** llama.cpp のビルド可否を build.rs の cfg（`llama_target`）に集約し、デスクトップと、実測でビルドできた Android の ABI（絞らず、通るものは全部）で有効にする。回線の状態は Kotlin の `ConnectivityManager` から JNI で Rust に push し（`mobile_intent.rs` と同じ構造）、`get_network_status` で読む。フロントはダウンロード開始の前に状態を読み、警告が要るときだけ確認ダイアログを出す。

**Tech Stack:** Rust（`llama-cpp-2` git rev 固定、`jni` 0.22、tauri-specta）、Kotlin（`MainActivity.kt`）、Svelte 5 + Vitest、GitHub Actions（`android-build`）。

**Spec:** `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`（§6、「Android とビルド」、「段階」）。段階1（#626）・段階2（#628）は `main` にマージ済み。この plan は段階3。

## Global Constraints

- コミットは件名だけ・Conventional Commits・`(#603)` 付き。`Refs #603`（Issue は閉じない。Android の実機確認が終わるまで開けておく）。
- `gen/android/.../generated/` は編集しない。編集するのは `MainActivity.kt` とマニフェストだけ。
- llama.cpp のビルドは `CARGO_BUILD_JOBS=4 CMAKE_BUILD_PARALLEL_LEVEL=4 CARGO_TARGET_DIR=$HOME/.cache/tsumugi-603/target` と `systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G` を付ける。`/tmp` に target を置かない。
- 対応する ABI は絞らない（趣味のプロジェクトのため）。ビルドできた ABI はすべて対象にし、ビルドできなかった ABI だけ対象外にする。
- 回線が不明のときは、安全側に倒して、注記つきの確認を出す（ブロックはしない）。
- Android: NDK r27c（ローカルは `~/Android/Sdk/ndk/27.2.12479018`）、JDK は 21（`JAVA_HOME=/usr/lib/jvm/java-21-openjdk`）、minSdk 26。
- aarch64 の最適化は既定の `armv8-a` のまま（dotprod / i8mm を固定で有効にしない）。
- 通信種別の判定はブロックしない。警告を出して、続ける / キャンセルをユーザーが選ぶ。
- ダウンロードにフォアグラウンドサービスは使わない。`onTrimMemory` でのアンロードは入れない（フォローアップ）。
- UI の視覚値は `docs/design/style-guide.md` のスケールに従う。
- 実機・エミュレーターは Xvfb / dbus 隔離と同様、実画面や実データを触らない。エミュレーターは `~/.config/.android/avd` の AVD をコピーして使う（memory: android emulator environment）。
- 検証で自分が起動したプロセス（emulator、adb、Xvfb 等）は、完了前に PID 指定で止める（`pkill` 禁止）。

## Review Focus

spec が暗に含むが、どのタスクのテストも直接は踏まない、または踏みにくい入力・条件。最終レビューで確認する。

1. **回線なし・切断**（`onLost`）: `Unknown / Unknown` に戻し、「回線の種類を判定できません」の注記つきの確認を出す（続ければダウンロードを試み、失敗は既存の `Network` エラーで表示される）。→ Task 2 の `from_raw` と Task 3 の `downloadWarning` のテスト。
2. **起動直後に `get_network_status` が先に呼ばれる**（JNI の push 前）: `Unknown / Unknown` を返す。→ Task 2 の `current` の初期値のテスト。
3. **`get_network_status` が失敗する**（IPC エラー）: 不明として扱い、注記つきの確認を出す。確認で続ければ開始できる（取得の失敗がダウンロードを妨げない）。→ Task 3 のカードのテスト。
4. **ビルドできなかった ABI**（Task 1 の実測で外れたもの。通れば無し）: `LocalModelInfo.supported` が false になり、カードは「この環境では PLaMo による翻訳は使えません」を出す。ビルドは llama.cpp 抜きで通る。32 ビットの ABI は、アドレス空間と速度が厳しい可能性がある（実機は無いので、ビルドの確認までで、動作は保証しない）。→ Task 1 のビルド確認と、`is_supported` のテスト。
5. **VPN 経由**: 回線の種類は `Other`（`TRANSPORT_VPN` のみで下層の種類が見えない場合）。従量制が分かればそれで、不明なら注記つきの確認にする。→ Task 3 の `downloadWarning` のテスト。Pixel 8 Pro での実測は Task 6。
6. **メモリ逼迫**（低メモリ端末でモデルのロード中に OOM kill）: この plan では緩和しない（spec のフォローアップ）。アイドル 3 分での解放だけ入れる。Task 6 で Pixel 8 Pro のメモリを実測して、結果を spec の「実装時に確定する値」に書く。

---

## File Structure

- Modify: `src-tauri/build.rs` — `llama_target` cfg を出す。
- Modify: `src-tauri/Cargo.toml` — Android 用の `llama-cpp-2`（`android-static-stdcxx`）。
- Modify: `src-tauri/src/translate/mod.rs`、`manager.rs`、`commands/local_model.rs` — `desktop` 条件を `llama_target` に置き換える。アイドル時間を Android で短くする。
- Create: `src-tauri/src/network_status.rs` — `NetworkStatus` と JNI の受け口、現在値。
- Modify: `src-tauri/src/lib.rs`、`commands/app.rs` — モジュールとコマンド `get_network_status` の登録。
- Modify: `src-tauri/gen/android/app/src/main/AndroidManifest.xml`、`.../MainActivity.kt` — 権限と `ConnectivityManager` の監視。
- Modify: `frontend/src/lib/localModel.ts`、`ui/settings/LocalModelCard.svelte`、`ui/settings/LicenseConsentDialog.svelte`（+ 各テスト）。
- Modify: `.github/workflows/test.yml`（`android-build`）。
- Modify: `docs/guide/user-guide.md`、`CLAUDE.md`、spec。

---

### Task 1: Android で llama.cpp をビルドする（cfg の集約と ABI の確定）

spec が「未検証」としている部分。**最初にこのタスクで、どの ABI でビルドできるかを実測して確定する。** 以降のタスクはこの結果（対応する ABI の一覧）を前提にする。

**Files:**
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/translate/mod.rs:10,38`、`src-tauri/src/translate/manager.rs:36,50`、`src-tauri/src/commands/local_model.rs:150`
- Test: `src-tauri/src/translate/mod.rs`（`is_supported` のテスト）

**Interfaces:**
- Produces: cfg `llama_target`（`local-translate` feature が有効で、デスクトップ、または対応した Android の ABI のとき）。Task 2 以降は `desktop` ではなくこれで判定する。`translate::is_supported()` は `cfg!(llama_target)`。

- [ ] **Step 1: llama-cpp-2 に `android-static-stdcxx` feature があることを確認する**

Run:
```sh
cd src-tauri && CARGO_TARGET_DIR=$HOME/.cache/tsumugi-603/target cargo metadata --format-version 1 2>/dev/null \
  | python3 -c "import json,sys; m=json.load(sys.stdin); p=[x for x in m['packages'] if x['name']=='llama-cpp-2'][0]; print(sorted(p['features']))"
```
Expected: 出力に `android-static-stdcxx` が含まれる。無ければ停止して、spike の feature 名（`~/.claude/projects/.../memory/project-plamo3-translate-issue-603-spike.md`）と git の rev を確認する。

- [ ] **Step 2: 失敗するテストを書く（`is_supported` が cfg に従う）**

`src-tauri/src/translate/mod.rs` の末尾に足す:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_supported_follows_the_llama_target_cfg() {
        assert_eq!(is_supported(), cfg!(llama_target));
    }
}
```

- [ ] **Step 3: 失敗を確認する**

Run: `cd src-tauri && cargo test --lib translate::tests::is_supported_follows_the_llama_target_cfg` （上の環境変数・`systemd-run` を付ける）
Expected: FAIL（`left: true, right: false`）。`is_supported()` はまだ `desktop` で決まり、`llama_target` は未定義なので `cfg!(llama_target)` が false になる。

- [ ] **Step 4: build.rs で `llama_target` を出す**

`src-tauri/build.rs` を次のように変える（`fn main` の先頭に足し、判定を関数に分ける）:
```rust
/// llama.cpp（ローカル翻訳）をビルドするターゲットか。`local-translate` feature が有効で、
/// デスクトップ、または Android でビルドできることを確認した ABI のとき。コード側は `#[cfg(llama_target)]` で判定する。
/// Android の ABI を絞る・外すときは、`Cargo.toml` の Android 用 `llama-cpp-2` の条件も同じにする。
/// 初期値は、Step 8 の実測で通った ABI（絞らない方針なので、通ったものは全部）に合わせる。ここは通る見込みの全 ABI。
fn llama_target(feature: bool, os: &str, arch: &str) -> bool {
  feature
    && (matches!(os, "linux" | "macos" | "windows")
      || (os == "android" && matches!(arch, "aarch64" | "x86_64" | "arm" | "x86")))
}

fn main() {
  println!("cargo:rustc-check-cfg=cfg(llama_target)");
  let feature = std::env::var_os("CARGO_FEATURE_LOCAL_TRANSLATE").is_some();
  let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
  if llama_target(feature, &os, &arch) {
    println!("cargo:rustc-cfg=llama_target");
  }

  // （既存の git_hash の処理と tauri_build::build() はそのまま続ける）
```

- [ ] **Step 5: `desktop` の条件を `llama_target` に置き換える**

`translate/mod.rs`:
```rust
#[cfg(llama_target)]
pub mod llama;
```
```rust
pub const fn is_supported() -> bool {
    cfg!(llama_target)
}
```
`translate/manager.rs:36` を `#[cfg(llama_target)]`、`:50` を `#[cfg(not(llama_target))]` に。`commands/local_model.rs:150` の `#[cfg(all(feature = "local-translate", desktop))]` を `#[cfg(llama_target)]` に。
`grep -rn 'feature = "local-translate"' src-tauri/src` で、他に残りが無いことを確認する。

- [ ] **Step 6: Cargo.toml に Android 用の依存を足す**

既存の `[target.'cfg(target_os = "android")'.dependencies]`（`android-native-keyring-store`、`jni` がある表）の末尾に、次を足す（同じ条件の表見出しを二重に書かない）:
```toml
# ローカル翻訳（Issue #603）。Android は ABI を絞らず、Step 8 でビルドできた ABI すべてで有効にする。
# 実測で外れた ABI があれば、build.rs の `llama_target` から外し、ここは `cfg(target_os = "android")` のまま、
# 外れた ABI では `local-translate` を feature で無効にできないため、その ABI 用に別の表へ移す（Step 9）。
# OpenMP と共有 libc++ は使わない。
llama-cpp-2 = { git = "https://github.com/utilityai/llama-cpp-rs", rev = "955aa2f72463c2535cc5ab894d5d52f27a38cde2", default-features = false, features = ["common", "android-static-stdcxx"], optional = true }
```

- [ ] **Step 7: デスクトップのテストが通ることを確認する**

Run: `cd src-tauri && cargo test --lib translate:: ` （環境変数・`systemd-run` 付き）
Expected: PASS（`is_supported_follows_the_llama_target_cfg` を含む）。既存の translate のテストも通る。

- [ ] **Step 8: Android の全 ABI でビルドを確かめる**

NDK のツールチェーンを環境変数で渡して、`cargo check`（リンクなし、llama.cpp の cmake ビルドは走る）を ABI ごとに行う。ABI は絞らない方針なので、4 つすべてを試す:
```sh
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018
export CARGO_BUILD_JOBS=4 CMAKE_BUILD_PARALLEL_LEVEL=4 CARGO_TARGET_DIR=$HOME/.cache/tsumugi-603/target
rustup target add armv7-linux-androideabi i686-linux-android
cd src-tauri
for t in aarch64-linux-android x86_64-linux-android armv7-linux-androideabi i686-linux-android; do
  echo "=== $t"; systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G \
    cargo check --lib --target $t 2>&1 | tail -15
done
```
Expected: ABI ごとに `Finished` か、失敗の原因が分かるエラー。cmake が NDK を見つけられず失敗する場合は、`CMAKE_TOOLCHAIN_FILE=$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake` と `ANDROID_ABI` / `ANDROID_PLATFORM=android-26` を渡す（spike でビルドできた構成）。結果（各 ABI の成否と、失敗なら原因）を、台帳に書く。

- [ ] **Step 9: 結果に応じて対象の ABI を確定する**

- 4 つとも通った → このまま（`llama_target` は `aarch64 | x86_64 | arm | x86`）。
- 一部だけ失敗 → 失敗した ABI を `build.rs` の `matches!` から外し、`Cargo.toml` の `llama-cpp-2` を、通った ABI だけの条件の表へ移す（例: `[target.'cfg(all(target_os = "android", any(target_arch = "aarch64", target_arch = "x86_64")))'.dependencies]`。同じ名前の依存を複数の表に書いてよい）。失敗の原因をコメントと台帳に残す。外した ABI は `cargo check --target <abi>` が llama.cpp 抜きで通ることと、`cargo tree --target <abi> -i llama-cpp-2` が空であることも確かめる。
- aarch64 が失敗 → **停止**して原因を調べる（spike では通っていた。git rev・NDK・feature のどれかが違う）。解決しなければ、ユーザーに報告して相談する。

- [ ] **Step 10: コミットする**

```sh
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 add src-tauri/build.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 commit -m "feat(translate): Androidでもllama.cppをビルドする (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 通信種別（Rust と Kotlin）

**Files:**
- Create: `src-tauri/src/network_status.rs`
- Modify: `src-tauri/src/lib.rs`（`mod network_status;`、`specta_builder()` にコマンド）、`src-tauri/src/commands/app.rs`
- Modify: `src-tauri/gen/android/app/src/main/AndroidManifest.xml`
- Modify: `src-tauri/gen/android/app/src/main/java/com/onodai/tsumugi/MainActivity.kt`
- Test: `src-tauri/src/network_status.rs`（`#[cfg(test)]`）

**Interfaces:**
- Produces: Rust `network_status::{Transport, Metered, NetworkStatus}`（TS では `{ transport: "wifi"|"ethernet"|"cellular"|"other"|"unknown", metered: "metered"|"unmetered"|"unknown" }`）、`NetworkStatus::from_raw(transport: i32, metered: i32)`、`network_status::current() -> NetworkStatus`、コマンド `get_network_status() -> NetworkStatus`、JNI `Java_com_onodai_tsumugi_MainActivity_nativeNetworkChanged(transport: jint, metered: jint)`。
- Kotlin → Rust の整数の対応: transport `0=Unknown 1=Wifi 2=Ethernet 3=Cellular 4=Other`、metered `0=Unknown 1=Metered 2=Unmetered`。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/network_status.rs`:
```rust
//! 回線の種類と従量制（Issue #603 段階3）。Android の `MainActivity.kt` が `ConnectivityManager` の変化を
//! JNI で push し、フロントは `get_network_status` で読む（`mobile_intent.rs` と同じ構造）。
//! Android 以外は常に `Unknown`（デスクトップの判定は範囲外）。

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Wifi,
    Ethernet,
    Cellular,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Metered {
    Metered,
    Unmetered,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub transport: Transport,
    pub metered: Metered,
}

impl NetworkStatus {
    pub const UNKNOWN: NetworkStatus = NetworkStatus { transport: Transport::Unknown, metered: Metered::Unknown };

    /// Kotlin から渡る整数を変換する。範囲外は `Unknown`（将来 Kotlin 側に値を足しても Rust が落ちない）。
    pub fn from_raw(transport: i32, metered: i32) -> Self {
        NetworkStatus {
            transport: match transport {
                1 => Transport::Wifi,
                2 => Transport::Ethernet,
                3 => Transport::Cellular,
                4 => Transport::Other,
                _ => Transport::Unknown,
            },
            metered: match metered {
                1 => Metered::Metered,
                2 => Metered::Unmetered,
                _ => Metered::Unknown,
            },
        }
    }
}

static CURRENT: Mutex<NetworkStatus> = Mutex::new(NetworkStatus::UNKNOWN);

/// 最後に Kotlin から届いた値。まだ届いていなければ `Unknown`。
pub fn current() -> NetworkStatus {
    *CURRENT.lock().unwrap()
}

#[cfg(any(target_os = "android", test))]
fn set(s: NetworkStatus) {
    *CURRENT.lock().unwrap() = s;
}

#[cfg(target_os = "android")]
mod android {
    use super::{set, NetworkStatus};
    use jni::objects::JObject;
    use jni::sys::jint;
    use jni::EnvUnowned;

    /// `MainActivity.kt` の `private external fun nativeNetworkChanged(transport: Int, metered: Int)` から呼ばれる。
    #[no_mangle]
    pub extern "system" fn Java_com_onodai_tsumugi_MainActivity_nativeNetworkChanged<'local>(
        _env: EnvUnowned<'local>,
        _this: JObject<'local>,
        transport: jint,
        metered: jint,
    ) {
        set(NetworkStatus::from_raw(transport, metered));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_maps_every_known_value() {
        assert_eq!(NetworkStatus::from_raw(1, 2), NetworkStatus { transport: Transport::Wifi, metered: Metered::Unmetered });
        assert_eq!(NetworkStatus::from_raw(2, 2).transport, Transport::Ethernet);
        assert_eq!(NetworkStatus::from_raw(3, 1), NetworkStatus { transport: Transport::Cellular, metered: Metered::Metered });
        assert_eq!(NetworkStatus::from_raw(4, 1).transport, Transport::Other);
        assert_eq!(NetworkStatus::from_raw(0, 0), NetworkStatus::UNKNOWN);
    }

    #[test]
    fn from_raw_turns_out_of_range_values_into_unknown() {
        assert_eq!(NetworkStatus::from_raw(99, -1), NetworkStatus::UNKNOWN);
        assert_eq!(NetworkStatus::from_raw(i32::MAX, i32::MIN), NetworkStatus::UNKNOWN);
    }

    #[test]
    fn serializes_with_camel_case_values() {
        let j = serde_json::to_value(NetworkStatus::from_raw(3, 1)).unwrap();
        assert_eq!(j, serde_json::json!({"transport": "cellular", "metered": "metered"}));
    }

    #[test]
    fn current_is_unknown_until_set_and_then_follows_the_last_value() {
        // プロセス共有の静的変数なので、初期値の確認と更新の確認を 1 つのテストにまとめる（並列テストと競合させない）。
        assert_eq!(current(), NetworkStatus::UNKNOWN);
        set(NetworkStatus::from_raw(3, 1));
        assert_eq!(current().transport, Transport::Cellular);
        set(NetworkStatus::UNKNOWN);
        assert_eq!(current(), NetworkStatus::UNKNOWN);
    }
}
```

- [ ] **Step 2: モジュールを登録して失敗を確認する**

`src-tauri/src/lib.rs` に `mod network_status;` を足す（`mod mobile_intent;` の近く）。
Run: `cd src-tauri && cargo test --lib network_status::` （環境変数付き）
Expected: 4 件とも PASS（Step 1 で型と実装を同じファイルに書いたため）。**RED を確認するため**、いったん `from_raw` の `3 => Transport::Cellular` を `3 => Transport::Other` に変えてテストを走らせ、`from_raw_maps_every_known_value` が FAIL するのを見てから、元に戻して再度 PASS を確認する。

- [ ] **Step 3: コマンドを足す**

`src-tauri/src/commands/app.rs` の `get_pending_share` の近くに:
```rust
/// Android の回線の種類と従量制（Issue #603 段階3）。Kotlin から届いた最新の値で、まだ届いていなければ、
/// および Android 以外では `Unknown / Unknown`。
#[tauri::command]
#[specta::specta]
pub fn get_network_status() -> crate::network_status::NetworkStatus {
    crate::network_status::current()
}
```
同ファイルのテストに:
```rust
    #[test]
    fn get_network_status_is_unknown_by_default() {
        assert_eq!(get_network_status(), crate::network_status::NetworkStatus::UNKNOWN);
    }
```
`lib.rs` の `specta_builder()` の `commands::app::get_pending_share,` の次に `commands::app::get_network_status,` を足す。

- [ ] **Step 4: バインディングを再生成して通す**

Run: `cd src-tauri && cargo test` （全体。環境変数・`systemd-run` 付き）
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `getNetworkStatus` と型 `NetworkStatus` / `Transport` / `Metered` が生成される（`git diff --stat` で確認）。

- [ ] **Step 5: マニフェストの権限を足す**

`AndroidManifest.xml` の `VIBRATE` の次の行に:
```xml
    <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
```

- [ ] **Step 6: Kotlin の監視を足す**

`MainActivity.kt`:
imports に追加:
```kotlin
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
```
クラスの先頭（`nativeShareReceived` の次）に:
```kotlin
    /// Rust側(network_status.rs)へ回線の状態を渡す(Issue #603)。値の対応は network_status.rs の `from_raw` と同じ:
    /// transport 0=不明 1=Wi-Fi 2=Ethernet 3=モバイル 4=その他、metered 0=不明 1=従量制 2=非従量制。
    private external fun nativeNetworkChanged(transport: Int, metered: Int)

    private var networkCallback: ConnectivityManager.NetworkCallback? = null

    private fun publishNetwork(caps: NetworkCapabilities?) {
        if (caps == null) {
            nativeNetworkChanged(0, 0)
            return
        }
        // VPN はTRANSPORT_VPN のみで下層の種類が見えないことがある。その場合は「その他」とし、従量制の値だけで判断する。
        val transport = when {
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> 1
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> 2
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> 3
            else -> 4
        }
        val metered = if (caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)) 2 else 1
        nativeNetworkChanged(transport, metered)
    }

    /// 既定のネットワークの変化を監視する(ダウンロード前の警告に使う)。権限が無い・取得できないときは
    /// 何も渡さず、Rust側は不明のまま(警告が出ないだけで、ダウンロードは妨げない)。
    private fun startNetworkMonitoring() {
        try {
            val cm = getSystemService(ConnectivityManager::class.java) ?: return
            val callback = object : ConnectivityManager.NetworkCallback() {
                override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) = publishNetwork(caps)
                override fun onLost(network: Network) = publishNetwork(null)
            }
            cm.registerDefaultNetworkCallback(callback)
            networkCallback = callback
        } catch (e: Exception) {
            Log.w("MainActivity", "network monitoring unavailable", e)
        }
    }
```
`onCreate` の `File(cacheDir, "shared-intents")...` の前に `startNetworkMonitoring()`、`onNewIntent` の前に:
```kotlin
    override fun onDestroy() {
        networkCallback?.let {
            try {
                getSystemService(ConnectivityManager::class.java)?.unregisterNetworkCallback(it)
            } catch (_: Exception) {
            }
        }
        networkCallback = null
        super.onDestroy()
    }
```

- [ ] **Step 7: Android ターゲットがコンパイルできることを確認する**

Run: `cd src-tauri && cargo check --lib --target aarch64-linux-android --no-default-features --features custom-protocol`（llama.cpp を避ける。環境変数付き）
Expected: `Finished`（JNI 関数がコンパイルできる）。`MainActivity.kt` の構文は Task 6 の `cargo tauri android build --debug` で確かめる。

- [ ] **Step 8: コミットする**

```sh
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 add src-tauri frontend/src/bindings/tauri.gen.ts
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 commit -m "feat(android): 回線の種類と従量制をKotlinからRustへ渡し、get_network_statusで読めるようにする (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ダウンロード前の警告（フロントエンド）

**Files:**
- Modify: `frontend/src/lib/localModel.ts`、`frontend/src/lib/localModel.test.ts`
- Modify: `frontend/src/ui/settings/LocalModelCard.svelte`、`LocalModelCard.test.ts`
- Modify: `frontend/src/ui/settings/LicenseConsentDialog.svelte`、`LicenseConsentDialog.test.ts`

**Interfaces:**
- Consumes: `commands.getNetworkStatus(): Promise<Result<NetworkStatus, …>>`（Task 2 のバインディング）、`NetworkStatus` 型。
- Produces: `downloadWarning(net: NetworkStatus, sizeBytes: number): string | null`（`localModel.ts`）。`LicenseConsentDialog` の任意の prop `networkWarning?: string | null`。

挙動（spec §6 から、不明の扱いだけ変更。Ruling: 回線が不明なら、安全側に倒して注記つきの確認を出す — 作者の指示。デスクトップは判定しないので常に不明で、ダウンロードのたびに（初回の 1 回だけの大きな取得なので許容）サイズの確認が出る）:

| 状態 | 確認の文面 |
|---|---|
| `cellular`（従量制の値に関わらず） | 「モバイル通信を使用中です。約 573.1 MB を取得します。続けますか?」 |
| `cellular` 以外で `metered` | 「従量制の接続です。約 573.1 MB を取得します。続けますか?」 |
| 従量制が `unmetered` | なし（確認せず開始） |
| 従量制が `unknown` で、種類が `wifi` / `ethernet` | なし |
| 従量制が `unknown` で、種類が `other` / `unknown`（回線なし・VPN・デスクトップ・取得失敗） | 「回線の種類を判定できませんでした。約 573.1 MB を取得します。モバイル通信の場合はご注意ください。続けますか?」 |

- [ ] **Step 1: `downloadWarning` の失敗するテストを書く**

`localModel.test.ts` に追加:
```ts
import { downloadWarning, UNKNOWN_NETWORK } from "./localModel";
import type { NetworkStatus } from "./ipc";

const net = (transport: NetworkStatus["transport"], metered: NetworkStatus["metered"]): NetworkStatus => ({ transport, metered });
const SIZE = 573_126_848;
const UNSURE = "回線の種類を判定できませんでした。約 573.1 MB を取得します。モバイル通信の場合はご注意ください。続けますか?";

describe("downloadWarning", () => {
  it("モバイル通信なら、従量制の値に関わらずモバイル通信の警告", () => {
    for (const m of ["metered", "unmetered", "unknown"] as const) {
      expect(downloadWarning(net("cellular", m), SIZE)).toBe("モバイル通信を使用中です。約 573.1 MB を取得します。続けますか?");
    }
  });
  it("モバイル通信以外の従量制は、従量制の警告（VPN などで種類が other のときも）", () => {
    expect(downloadWarning(net("wifi", "metered"), SIZE)).toBe("従量制の接続です。約 573.1 MB を取得します。続けますか?");
    expect(downloadWarning(net("other", "metered"), SIZE)).toBe("従量制の接続です。約 573.1 MB を取得します。続けますか?");
  });
  it("非従量制なら、種類に関わらず警告なし", () => {
    for (const t of ["wifi", "ethernet", "other", "unknown"] as const) expect(downloadWarning(net(t, "unmetered"), SIZE)).toBeNull();
  });
  it("従量制が不明でも、Wi-Fi・Ethernet だと分かっていれば警告なし", () => {
    expect(downloadWarning(net("wifi", "unknown"), SIZE)).toBeNull();
    expect(downloadWarning(net("ethernet", "unknown"), SIZE)).toBeNull();
  });
  it("従量制も種類も当てにならない（other / unknown）なら、安全側に倒して注記つきの確認", () => {
    expect(downloadWarning(net("other", "unknown"), SIZE)).toBe(UNSURE);
    expect(downloadWarning(net("unknown", "unknown"), SIZE)).toBe(UNSURE);
    expect(downloadWarning(UNKNOWN_NETWORK, SIZE)).toBe(UNSURE);
  });
});
```
Run: `cd frontend && npx vitest run src/lib/localModel.test.ts`
Expected: FAIL（`downloadWarning` / `UNKNOWN_NETWORK` が無い）。

- [ ] **Step 2: 実装する**

`localModel.ts` に追加（`NetworkStatus` を import に足す: `import type { LocalModelStatus, NetworkStatus } from "./ipc";`）:
```ts
/// 回線の状態を取得できなかったとき（IPC の失敗）の値。「不明」として、安全側の注記つきの確認になる。
export const UNKNOWN_NETWORK: NetworkStatus = { transport: "unknown", metered: "unknown" };

/// ダウンロード前の確認の文面。確認が要らないとき（非従量制の Wi-Fi など）は null。
/// モバイル通信・従量制は警告、判定できないとき（回線なし・VPN・デスクトップ）は、安全側に倒して注記つきの確認にする。
export function downloadWarning(net: NetworkStatus, sizeBytes: number): string | null {
  const size = formatBytes(sizeBytes);
  if (net.transport === "cellular") return `モバイル通信を使用中です。約 ${size} を取得します。続けますか?`;
  if (net.metered === "metered") return `従量制の接続です。約 ${size} を取得します。続けますか?`;
  if (net.metered === "unmetered") return null;
  if (net.transport === "wifi" || net.transport === "ethernet") return null;
  return `回線の種類を判定できませんでした。約 ${size} を取得します。モバイル通信の場合はご注意ください。続けますか?`;
}
```
Run: `npx vitest run src/lib/localModel.test.ts` → PASS。

- [ ] **Step 3: 同意ダイアログの警告 prop の失敗するテストを書く**

`LicenseConsentDialog.test.ts`（既存のテストの描画方法に合わせる）に追加:
```ts
  it("networkWarning があれば、同意の文面の中に警告を出す", () => {
    const { getByTestId } = render(LicenseConsentDialog, {
      props: { sizeBytes: 573_126_848, networkWarning: "モバイル通信を使用中です。", onAccept: vi.fn(), onCancel: vi.fn(), onOpenLicense: vi.fn() },
    });
    expect(getByTestId("license-dialog").textContent).toContain("モバイル通信を使用中です。");
  });
  it("networkWarning が無ければ、警告の領域を出さない", () => {
    const { queryByTestId } = render(LicenseConsentDialog, {
      props: { sizeBytes: 573_126_848, onAccept: vi.fn(), onCancel: vi.fn(), onOpenLicense: vi.fn() },
    });
    expect(queryByTestId("license-network-warning")).toBeNull();
  });
```
Run → FAIL。

- [ ] **Step 4: 同意ダイアログに prop を足す**

`LicenseConsentDialog.svelte` の props に `networkWarning = null` を足し（型 `networkWarning?: string | null`）、最後の `</ul>` の直後（「ライセンス全文（日本語）を開く」の段落の前）に:
```svelte
    {#if networkWarning}
      <p class="mb-2 mt-0 rounded-md bg-muted p-2 text-sm text-foreground" role="status" data-testid="license-network-warning">{networkWarning}</p>
    {/if}
```
（`rounded-md` / `bg-muted` が `docs/design/style-guide.md` の角丸スケールに合うことを確認する。合わなければガイドの値に直す。）
Run → PASS。

- [ ] **Step 5: カードの失敗するテストを書く**

`LocalModelCard.test.ts` に追加（既存の `info()` と `ok` を使う。`ConfirmDialog` の確認ボタンの取り方は、既存の削除のテストに合わせる）:
```ts
const netStatus = (transport: string, metered: string) => ({ status: "ok" as const, data: { transport, metered } }) as never;

  it("同意済み・非従量制の Wi-Fi: 確認なしでそのまま開始する", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ licenseAccepted: true })));
    vi.spyOn(commands, "getNetworkStatus").mockResolvedValue(netStatus("wifi", "unmetered"));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null) as never);
    const { findByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("2026-11-09"));
  });

  it("同意済み・モバイル通信: 警告つきの確認を出し、続けるまで開始しない", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ licenseAccepted: true })));
    vi.spyOn(commands, "getNetworkStatus").mockResolvedValue(netStatus("cellular", "metered"));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null) as never);
    const { findByTestId, findByText } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    expect(await findByText(/モバイル通信を使用中です/)).toBeTruthy();
    expect(start).not.toHaveBeenCalled();
    await fireEvent.click(await findByText("ダウンロード", { selector: "[role=dialog] button" }));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("2026-11-09"));
  });

  it("同意済み・モバイル通信: キャンセルすると開始しない", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ licenseAccepted: true })));
    vi.spyOn(commands, "getNetworkStatus").mockResolvedValue(netStatus("cellular", "metered"));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null) as never);
    const { findByTestId, findByText, queryByText } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    await fireEvent.click(await findByText("キャンセル", { selector: "[role=dialog] button" }));
    expect(queryByText(/モバイル通信を使用中です/)).toBeNull();
    expect(start).not.toHaveBeenCalled();
  });

  it("未同意・従量制: 同意ダイアログの中に警告を出す", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ licenseAccepted: false })));
    vi.spyOn(commands, "getNetworkStatus").mockResolvedValue(netStatus("wifi", "metered"));
    const { findByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    expect((await findByTestId("license-network-warning")).textContent).toContain("従量制の接続です");
  });

  it("通信種別の取得に失敗したら、不明として注記つきの確認を出し、続ければ開始できる（取得の失敗がダウンロードを妨げない）", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ licenseAccepted: true })));
    vi.spyOn(commands, "getNetworkStatus").mockRejectedValue(new Error("ipc down"));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null) as never);
    const { findByTestId, findByText } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    expect(await findByText(/回線の種類を判定できませんでした/)).toBeTruthy();
    expect(start).not.toHaveBeenCalled();
    await fireEvent.click(await findByText("ダウンロード", { selector: "[role=dialog] button" }));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("2026-11-09"));
  });
```
Run: `npx vitest run src/ui/settings/LocalModelCard.test.ts` → FAIL（`getNetworkStatus` を呼んでいない、確認が出ない）。

- [ ] **Step 5b: カードを実装する**

`LocalModelCard.svelte`:
import に `downloadWarning` を足す（`import { downloadWarning, formatBytes, progressPercent, statusText, UNKNOWN_NETWORK } from "../../lib/localModel";`）。状態に:
```ts
  let confirmOpen = $state(false);
  // ダウンロード前の警告（モバイル通信・従量制）。無ければ null。確認ダイアログと同意ダイアログの両方が使う。
  let networkWarning = $state<string | null>(null);
  // 通信種別を読んでいる間 true（連打で二重に確認を出さない）。
  let checking = $state(false);
```
`onDownload` を置き換える:
```ts
  // 回線の状態を読み、確認が要るとき（モバイル通信・従量制・判定できないとき）か、同意が要るときだけダイアログを出す。
  // 状態を取得できなくても、不明として確認を出すだけで、続ければ開始できる。
  async function onDownload() {
    if (!info || starting || checking) return;
    checking = true;
    let warning: string | null = null;
    try {
      warning = downloadWarning(await unwrap(commands.getNetworkStatus()), info.sizeBytes);
    } catch {
      // 取得できなかったときは、不明として、安全側（注記つきの確認）に倒す。確認で続ければ開始できる。
      warning = downloadWarning(UNKNOWN_NETWORK, info.sizeBytes);
    } finally {
      checking = false;
    }
    networkWarning = warning;
    if (info.licenseAccepted && !warning) void start();
    else if (info.licenseAccepted) confirmOpen = true;
    else consentOpen = true;
  }
```
`start()` の中の `consentOpen = false;`（成功と失敗の両方）の隣に `confirmOpen = false;` を足す。ボタンの `disabled={starting}` を `disabled={starting || checking}` に。テンプレートの末尾（`LicenseConsentDialog` の前）に:
```svelte
{#if confirmOpen && info}
  <ConfirmDialog
    title="モデルをダウンロード"
    message={networkWarning ?? ""}
    confirmLabel="ダウンロード"
    onConfirm={start}
    onCancel={() => (confirmOpen = false)}
  />
{/if}
```
`LicenseConsentDialog` に `networkWarning={networkWarning}` を渡す。
Run: `npx vitest run src/ui/settings/LocalModelCard.test.ts` → PASS。既存テストのうち、同意済みでダウンロードを押してすぐ開始を期待するものは、`getNetworkStatus` を mock していないと、取得失敗＝不明として注記つきの確認が出るようになり落ちる。それらのテストに `vi.spyOn(commands, "getNetworkStatus").mockResolvedValue(netStatus("wifi", "unmetered"))` を足し、`vi.waitFor` / `findBy*` に直す。

- [ ] **Step 6: フロント全体を通す**

Run: `cd frontend && pnpm check && pnpm test`
Expected: 0 errors、全テスト PASS。

- [ ] **Step 7: コミットする**

```sh
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 add frontend/src
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 commit -m "feat(settings): モバイル通信・従量制のとき、ダウンロードの前に警告して確認する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Android のアイドル解放を 3 分にする

**Files:**
- Modify: `src-tauri/src/translate/mod.rs:19-20`
- Test: `src-tauri/src/translate/mod.rs`

**Interfaces:**
- Produces: `IDLE_UNLOAD` がターゲットで変わる（既存の参照元は変えない）。

- [ ] **Step 1: 失敗するテストを書く**

`translate/mod.rs` の `tests` に追加:
```rust
    #[test]
    fn idle_unload_is_three_minutes_on_android_and_ten_elsewhere() {
        let want = if cfg!(target_os = "android") { 180 } else { 600 };
        assert_eq!(IDLE_UNLOAD, std::time::Duration::from_secs(want));
    }
```
Run: `cargo test --lib translate::tests::idle_unload`
Expected: 現状は PASS（デスクトップの 600 秒のまま。Android 側の分岐はデスクトップのテストでは走らない）。**RED を確認するため**、いったん `IDLE_UNLOAD` を `from_secs(601)` にして FAIL を見てから、Step 2 で正しい実装にする。Android 側の 180 秒は、Task 6 の実機で確認する（アイドル 3 分での解放）。

- [ ] **Step 2: 実装する**

```rust
/// アイドルがこれだけ続いたらモデルを解放する。Android はメモリが厳しいので短くする（実測で見直す）。
#[cfg(target_os = "android")]
pub const IDLE_UNLOAD: std::time::Duration = std::time::Duration::from_secs(180);
#[cfg(not(target_os = "android"))]
pub const IDLE_UNLOAD: std::time::Duration = std::time::Duration::from_secs(600);
```
Run: `cargo test --lib translate::` → PASS。

- [ ] **Step 3: コミットする**

```sh
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 add src-tauri/src/translate/mod.rs
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 commit -m "feat(translate): Androidではモデルのアイドル解放を3分にする (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: CI（`android-build`）とドキュメント

**Files:**
- Modify: `.github/workflows/test.yml`（`android-build`）
- Modify: `docs/guide/user-guide.md`、`CLAUDE.md`
- Modify: `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`

**Interfaces:**
- Consumes: Task 1 で確定した対応 ABI の一覧。

- [ ] **Step 1: `android-build` で、対応する ABI をすべてビルドする**

`test.yml` の `android-build` の `dtolnay/rust-toolchain@stable` の `targets:` を `aarch64-linux-android,armv7-linux-androideabi,i686-linux-android,x86_64-linux-android` に、`cargo tauri android build --debug ...` の行を `--target aarch64 --target armv7 --target x86 --target x86_64` に変える（Task 1 で外れた ABI があれば、それは除く）。理由: リリース・nightly の Android は全 ABI（universal）をビルドするので、どれかの llama.cpp が壊れても、リリースまで気付けないため。
ビルド時間が、ABI の数だけ（各 約 2 分）延びることを、コメントに書く。ジョブの所要時間が長すぎる（目安 30 分超）なら、aarch64 と x86_64 だけにして、残りはリリース時の確認に任せる、と台帳に Ruling を残す。

- [ ] **Step 2: ユーザーガイドを直す**

`docs/guide/user-guide.md` の「ローカル翻訳(PLaMo・端末内)」の「デスクトップ版で使えます(Android版は今後対応します)。」を「デスクトップ版と、Android版で使えます。」に変える。手順の次に、通信量の注意を足す:
```markdown
- ダウンロードの前に、モバイル通信や従量制の接続のとき、または回線の種類を判定できないときは、取得するサイズ(約 573MB)の確認が出ます。Wi-Fi などで取得することをおすすめします。デスクトップ版は回線を判定しないので、初回の取得の前に、サイズの確認が出ます。
- Android では、翻訳しない時間がしばらく続くと、メモリを空けるためにモデルを解放します(次の翻訳のときに、1〜2 秒余計にかかります)。
```

- [ ] **Step 3: CLAUDE.md を直す**

`translate/` の説明に、`llama_target`（`build.rs` が出す cfg。デスクトップと Android の aarch64 / x86_64）、`network_status.rs`（Kotlin から JNI で push、`get_network_status`）を追記する。「Android」の節に、`ACCESS_NETWORK_STATE` と `MainActivity.kt` の `ConnectivityManager` の監視を一文で足す。Android で llama.cpp をローカルビルドするときの環境変数（`ANDROID_NDK_HOME`、並列度 4）も書く。

- [ ] **Step 4: spec を直す**

- 「Android とビルド」に、対応する ABI（Task 1 の実測の結果。絞らず、ビルドできたものはすべて。外れたものがあれば、その理由と、`is_supported` が false になること）を書く。
- §6 の不明の扱いを、作者の指示（2026-11-15）に合わせて直す: 回線が不明（回線なし・VPN で従量制も不明・デスクトップ・取得失敗）のときは、安全側に倒して、注記つきの確認を出す。表は Task 3 の表のとおり。
- 「実装時に確定する値」に、Task 6 の実測値（メモリ、tok/s）を書く欄を作る（実測後に埋める）。

- [ ] **Step 5: コミットする**

```sh
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 add .github docs CLAUDE.md
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p3 commit -m "docs(translate): Android対応をCI・ユーザーガイド・CLAUDE.md・specに反映する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 実環境での検証（エミュレーターと Pixel 8 Pro）

コードの変更はない（見つかった不具合は、その場で直して RED→GREEN のテストを足す）。結果は PR の本文と spec の「実装時に確定する値」に書く。

**Files:** なし（結果の記録のみ: spec、PR 本文）

- [ ] **Step 1: Android のデバッグビルドが通る**

Run:
```sh
cd src-tauri && JAVA_HOME=/usr/lib/jvm/java-21-openjdk ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018 \
  CARGO_BUILD_JOBS=4 CMAKE_BUILD_PARALLEL_LEVEL=4 CARGO_TARGET_DIR=$HOME/.cache/tsumugi-603/target \
  systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G \
  cargo tauri android build --debug --target aarch64 --target x86_64 2>&1 | tail -20
```
Expected: ビルド成功（`MainActivity.kt` のコンパイルを含む）。Gradle の `buildSrc` エラーなら `JAVA_HOME` を JDK 21 にする（CLAUDE.md）。

- [ ] **Step 2: エミュレーター（x86_64）で UI の流れを確認する**

AVD をコピーして起動し（memory: android emulator environment の手順。実機がある場合は `adb devices` のシリアルを `-s` で指定し、実機に触らない）、APK を入れて、次を確認する:
  1. 設定 →「外部連携」→「翻訳エンジン」を PLaMo にすると、モデルカードが出る（「この環境では使えません」ではない）。
  2. Wi-Fi（エミュレーターの既定、非従量制）: 「ダウンロード」を押すと、同意ダイアログ（未同意なら）、同意後に確認なしで始まる。
  3. `adb shell svc wifi disable; adb shell svc data enable` でモバイル通信に切り替えて「ダウンロード」を押すと、「モバイル通信を使用中です…」の警告が出る。キャンセルで開始しない。「ダウンロード」で開始する。
  4. 切り替えは、操作のたびに `adb shell dumpsys connectivity | grep -i -E "NetworkAgentInfo|METERED"` の出力と、画面の警告が合うことを確かめる。
  5. ダウンロードを途中で止めて、アプリを終了（`adb shell am force-stop`）してから開き直すと、「途中まで取得済み」が出て、続きから再開できる。
  実画面の確認は、debug bridge ではなく、`adb shell input tap` の本物のタップで行う（CLAUDE.md）。スクリーンショットは `adb exec-out screencap -p` で取り、自分の目で見る。

- [ ] **Step 3: エミュレーターでモデルが動く**

取得済みのモデルで、ノートのメニューの「PLaMo で翻訳」が成功する（x86_64 はベースラインの命令で遅い。速度は記録だけして、基準にしない）。

- [ ] **Step 4: Pixel 8 Pro の実機での確認（ユーザーの操作が必要）**

ユーザーに、実機を USB / adb 接続して、デバッグ APK（aarch64）を入れてもらい、次を確認する。結果は、ユーザーに画面の確認を頼むか、`adb` で取れる値（`dumpsys meminfo`、`logcat`）を自分で取る:
  1. Wi-Fi: 確認なしでダウンロードが始まる。
  2. モバイル通信（Wi-Fi をオフ）: 警告が出る。
  3. VPN（Wi-Fi + VPN、モバイル + VPN）: 回線の種類（`Other` か、下層の種類か）と従量制の値、警告の有無を記録する。**ここで spec の推測（VPN では `Other`）が合っているかを確かめる。**
  4. 翻訳の速度（spike の約 13.8 tok/s と同程度か）、初回のロード時間、`dumpsys meminfo <pid>` の PSS（モデルのロード前後）、3 分のアイドルで解放されること、アプリを前面に戻して再翻訳できること。
  5. 他のアプリを多数開いた状態での翻訳で、OOM kill が起きないか（起きるなら、spec のフォローアップ `onTrimMemory` の優先度を上げる）。

- [ ] **Step 5: 結果を記録する**

spec の「実装時に確定する値」に、測った値（メモリ、tok/s、ロード時間、VPN の回線の種類）を書く。見つかった問題は、直すか、Issue（フォローアップ）にする。VPN の結果が推測と違えば、§6 の記述と `downloadWarning` を直す（テストを先に）。

- [ ] **Step 6: 後始末と PR**

自分で起動した emulator / adb / Xvfb のプロセスを PID 指定で止め、コピーした AVD を消す。全体のテスト（`cd src-tauri && cargo test`、`cd frontend && pnpm check && pnpm test`）が通ることを確認して、PR を作る（`Refs #603`。**実機の確認が終わってから** `#603` を閉じるかをユーザーに聞く）。PR 本文の「Android / E2E に影響あり」にチェックを入れ、`android-build` ジョブの結果を確認事項に書く。
