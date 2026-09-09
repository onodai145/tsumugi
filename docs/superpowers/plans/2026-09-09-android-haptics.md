# Androidハプティクス対応 実装計画 (Issue #26)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Androidビルドのtsumugiで、ノート投稿時とリアクション付与時に端末を振動させる。

**Architecture:** 自作Tauriプラグイン `tauri-plugin-haptics`（`src-tauri/plugins/` にpath依存で追加、Android側はKotlinで`Vibrator`/`VibrationEffect`を直接制御）を土台に、アプリ側の`#[tauri::command] vibrate`から`HapticPattern`（Light/Medium/Success/Warning/Error）を渡す。フロントは`isMobilePlatform && uiPrefs.hapticsEnabled`をガードに、ノート投稿成功時とリアクション新規付与時にfire-and-forgetで呼び出す。

**Tech Stack:** Rust (tauri 2.11, specta =2.0.0-rc.25 pinned), Kotlin (Android, minSdk 26), Svelte 5 + TypeScript

## Global Constraints

- specta / specta-typescript / tauri-specta は `=2.0.0-rc.25` / `=0.0.12` に固定（`src-tauri/Cargo.toml`のコメント参照）。プラグイン側もこのバージョンに合わせる。
- Android minSdkVersion は 26（`src-tauri/tauri.conf.json`の`bundle.android.minSdkVersion`）。`VibrationEffect`はAPI26以降のみなので、フォールバック実装は不要。
- コミットメッセージは件名のみ。Co-Authored-By trailerは各ステップの指示通りに付与する。
- 設計ドキュメント: `docs/superpowers/specs/2026-09-09-android-haptics-design.md`（このプランの元）。

---

### Task 1: `tauri-plugin-haptics` クレートの骨格を作成し、デスクトップビルドを通す

**Files:**
- Create: `src-tauri/plugins/tauri-plugin-haptics/Cargo.toml`
- Create: `src-tauri/plugins/tauri-plugin-haptics/build.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/src/error.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/src/models.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/src/commands.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/src/desktop.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/src/mobile.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/src/lib.rs`
- Create: `src-tauri/plugins/tauri-plugin-haptics/permissions/default.toml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs:172`（`.plugin(tauri_plugin_fs::init())` の下）

**Interfaces:**
- Produces: `tauri_plugin_haptics::HapticPattern`（`Light`/`Medium`/`Success`/`Warning`/`Error`の5値enum、`specta::Type`実装済み）、`tauri_plugin_haptics::HapticsExt<R>`拡張トレイト（`fn haptics(&self) -> &Haptics<R>`）、`Haptics<R>::vibrate(&self, pattern: HapticPattern) -> tauri_plugin_haptics::Result<()>`、`tauri_plugin_haptics::init<R>() -> TauriPlugin<R>`。以降のタスクはこれらのシグネチャを使う。

- [ ] **Step 1: プラグインのCargo.tomlを作成する**

```toml
[package]
name = "tauri-plugin-haptics"
version = "0.1.0"
authors = ["onodai145"]
description = "Android haptics (vibration) support for tsumugi"
edition = "2021"
rust-version = "1.77.2"
exclude = ["/examples", "/dist-js", "/guest-js", "/node_modules"]
links = "tauri-plugin-haptics"

# tsumugi本体(src-tauri)の暗黙workspaceに巻き込まれないようにする
# (`cargo build --workspace`等の挙動を変えないため)。
[workspace]

[dependencies]
tauri = { version = "2" }
serde = { version = "1", features = ["derive"] }
# tsumugi本体でspecta/tauri-specta を =2.0.0-rc.25 に固定しているため合わせる
# (src-tauri/Cargo.toml のコメント参照。TSバインディング生成の互換性のため緩めない)。
specta = { version = "=2.0.0-rc.25", features = ["derive"] }
thiserror = "2"

[build-dependencies]
tauri-plugin = { version = "2", features = ["build"] }
```

- [ ] **Step 2: build.rsを作成する**

```rust
const COMMANDS: &[&str] = &["vibrate"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
```

- [ ] **Step 3: エラー型を作成する**

`src-tauri/plugins/tauri-plugin-haptics/src/error.rs`:

```rust
use serde::{ser::Serializer, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
```

- [ ] **Step 4: パターン定義とリクエスト型を作成する**

`src-tauri/plugins/tauri-plugin-haptics/src/models.rs`:

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

/// 振動パターン。値はAndroid Kotlin側(HapticsPlugin.kt)のVibrationEffectマッピングと対応する。
/// Light/Medium のみ現時点で呼び出し元があり、Success/Warning/Errorは将来の用途
/// (投稿失敗フィードバック、破壊的操作の確認など)を見越した先行定義（Issue #26設計参照）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HapticPattern {
    Light,
    Medium,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VibrateRequest {
    pub pattern: HapticPattern,
}
```

- [ ] **Step 5: プラグイン内部コマンド(Rust→Kotlinブリッジの入口)を作成する**

`src-tauri/plugins/tauri-plugin-haptics/src/commands.rs`:

```rust
use tauri::{command, AppHandle, Runtime};

use crate::models::VibrateRequest;
use crate::HapticsExt;
use crate::Result;

#[command]
pub(crate) async fn vibrate<R: Runtime>(app: AppHandle<R>, payload: VibrateRequest) -> Result<()> {
    app.haptics().vibrate(payload.pattern)
}
```

- [ ] **Step 6: デスクトップ実装(no-op)を作成する**

`src-tauri/plugins/tauri-plugin-haptics/src/desktop.rs`:

```rust
use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::HapticPattern;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<Haptics<R>> {
    Ok(Haptics(app.clone()))
}

/// デスクトップでは振動デバイスが無いためno-op。
pub struct Haptics<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Haptics<R> {
    pub fn vibrate(&self, _pattern: HapticPattern) -> crate::Result<()> {
        Ok(())
    }
}
```

- [ ] **Step 7: モバイル実装(Android Kotlinブリッジ)を作成する**

`src-tauri/plugins/tauri-plugin-haptics/src/mobile.rs`:

```rust
use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::{HapticPattern, VibrateRequest};

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<Haptics<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("com.onodai.tsumugi.haptics", "HapticsPlugin")?;
    Ok(Haptics(handle))
}

pub struct Haptics<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Haptics<R> {
    pub fn vibrate(&self, pattern: HapticPattern) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("vibrate", VibrateRequest { pattern })
            .map_err(Into::into)
    }
}
```

- [ ] **Step 8: lib.rsでプラグインを組み立てる**

`src-tauri/plugins/tauri-plugin-haptics/src/lib.rs`:

```rust
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

pub use models::HapticPattern;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Haptics;
#[cfg(mobile)]
use mobile::Haptics;

/// [`tauri::App`]/[`tauri::AppHandle`]/[`tauri::Window`] からハプティクスAPIへアクセスする拡張。
pub trait HapticsExt<R: Runtime> {
    fn haptics(&self) -> &Haptics<R>;
}

impl<R: Runtime, T: Manager<R>> HapticsExt<R> for T {
    fn haptics(&self) -> &Haptics<R> {
        self.state::<Haptics<R>>().inner()
    }
}

/// プラグインを初期化する。
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("haptics")
        .invoke_handler(tauri::generate_handler![commands::vibrate])
        .setup(|app, api| {
            #[cfg(mobile)]
            let haptics = mobile::init(app, api)?;
            #[cfg(desktop)]
            let haptics = desktop::init(app, api)?;
            app.manage(haptics);
            Ok(())
        })
        .build()
}
```

- [ ] **Step 9: permissions/default.tomlを作成する**

```toml
[default]
description = "Default permissions for the plugin"
permissions = ["allow-vibrate"]
```

- [ ] **Step 10: src-tauriからpath依存として参照する**

`src-tauri/Cargo.toml` の `tauri-plugin-fs = "2"` の下に追加:

```toml
tauri-plugin-haptics = { path = "plugins/tauri-plugin-haptics" }
```

- [ ] **Step 11: lib.rsでプラグインを登録する**

`src-tauri/src/lib.rs:172` 付近（`.plugin(tauri_plugin_fs::init())` の下）に追加:

```rust
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_haptics::init())
```

- [ ] **Step 12: デスクトップビルドが通ることを確認する**

Run: `cd src-tauri && cargo build`
Expected: エラーなくビルドが完了する（Android実機コードは`#[cfg(mobile)]`でこのビルドには含まれない）。

- [ ] **Step 13: コミット**

```bash
git add src-tauri/plugins/tauri-plugin-haptics src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs
git commit -m "feat: tauri-plugin-hapticsクレートの骨格を追加(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 2: Android側の振動実装とVIBRATE権限、Androidビルド確認

**Files:**
- Create: `src-tauri/plugins/tauri-plugin-haptics/android/build.gradle.kts`
- Create: `src-tauri/plugins/tauri-plugin-haptics/android/settings.gradle`
- Create: `src-tauri/plugins/tauri-plugin-haptics/android/proguard-rules.pro`
- Create: `src-tauri/plugins/tauri-plugin-haptics/android/.gitignore`
- Create: `src-tauri/plugins/tauri-plugin-haptics/android/src/main/AndroidManifest.xml`
- Create: `src-tauri/plugins/tauri-plugin-haptics/android/src/main/java/com/onodai/tsumugi/haptics/HapticsPlugin.kt`
- Modify: `src-tauri/gen/android/app/src/main/AndroidManifest.xml`

**Interfaces:**
- Consumes: Task 1で定義した`VibrateRequest{ pattern: HapticPattern }`のJSONペイロード（Kotlin側は`@InvokeArg`クラスでフィールド名`pattern`の文字列として受け取る。値は`HapticPattern`のcamelCase文字列: `"light"`/`"medium"`/`"success"`/`"warning"`/`"error"`）。

- [ ] **Step 1: Android Gradleモジュールの雛形を作成する**

`src-tauri/plugins/tauri-plugin-haptics/android/build.gradle.kts`:

```kotlin
plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.onodai.tsumugi.haptics"
    compileSdk = 36

    defaultConfig {
        minSdk = 26

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.9.0")
    implementation("androidx.appcompat:appcompat:1.6.0")
    implementation(project(":tauri-android"))
}
```

- [ ] **Step 2: 単体ビルド用のsettings.gradle等を作成する**

`src-tauri/plugins/tauri-plugin-haptics/android/settings.gradle`:

```groovy
pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
        google()
    }
    resolutionStrategy {
        eachPlugin {
            switch (requested.id.id) {
                case "com.android.library":
                    useVersion("8.0.2")
                    break
                case "org.jetbrains.kotlin.android":
                    useVersion("1.8.20")
                    break
            }
        }
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        mavenCentral()
        google()
    }
}

include ':tauri-android'
project(':tauri-android').projectDir = new File('./.tauri/tauri-api')
```

`src-tauri/plugins/tauri-plugin-haptics/android/proguard-rules.pro`:

```
# Add project specific ProGuard rules here.
```

`src-tauri/plugins/tauri-plugin-haptics/android/.gitignore`:

```
build
.gradle
.tauri
```

- [ ] **Step 3: プラグイン自身のAndroidManifestを作成する(空)**

`src-tauri/plugins/tauri-plugin-haptics/android/src/main/AndroidManifest.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
</manifest>
```

- [ ] **Step 4: HapticsPlugin.ktを実装する**

`src-tauri/plugins/tauri-plugin-haptics/android/src/main/java/com/onodai/tsumugi/haptics/HapticsPlugin.kt`:

```kotlin
package com.onodai.tsumugi.haptics

import android.app.Activity
import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class VibrateArgs {
    var pattern: String? = null
}

@TauriPlugin
class HapticsPlugin(private val activity: Activity) : Plugin(activity) {
    private val vibrator: Vibrator by lazy {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val manager = activity.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager
            manager.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            activity.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
        }
    }

    // duration(ms)は単発、timings配列は[待ち, ON, OFF, ON, ...]のパルス列(createWaveform)。
    // 値はRust側 HapticPattern のドキュメントコメント(models.rs)と対応させること。
    private fun effectFor(pattern: String?): VibrationEffect = when (pattern) {
        "light" -> VibrationEffect.createOneShot(10, VibrationEffect.DEFAULT_AMPLITUDE)
        "success" -> VibrationEffect.createWaveform(longArrayOf(0, 15, 40, 15), -1)
        "warning" -> VibrationEffect.createWaveform(longArrayOf(0, 25, 80, 25), -1)
        "error" -> VibrationEffect.createOneShot(60, VibrationEffect.DEFAULT_AMPLITUDE)
        else -> VibrationEffect.createOneShot(35, VibrationEffect.DEFAULT_AMPLITUDE) // "medium" とその他既定
    }

    @Command
    fun vibrate(invoke: Invoke) {
        val args = invoke.parseArgs(VibrateArgs::class.java)
        vibrator.vibrate(effectFor(args.pattern))
        invoke.resolve()
    }
}
```

- [ ] **Step 5: アプリ本体のAndroidManifestにVIBRATE権限を追加する**

`src-tauri/gen/android/app/src/main/AndroidManifest.xml` の `<uses-permission android:name="android.permission.INTERNET" />` の下に追加:

```xml
    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.VIBRATE" />
```

- [ ] **Step 6: Androidビルドが通ることを確認する**

Run: `cd src-tauri && JAVA_HOME=/usr/lib/jvm/java-21-openjdk cargo tauri android build --debug --target aarch64`
Expected: ビルド成功（プラグインが`gen/android/tauri.settings.gradle`に自動的に取り込まれ、Kotlinがコンパイルされる）。CLAUDE.mdの記載通りbundler段が長時間かかる場合があるので、コンパイル自体が通っていればstep完了とみなしてよい（NDK/JDKバージョン起因のエラーが出た場合はCLAUDE.mdの「Android」節を参照）。

- [ ] **Step 7: コミット**

```bash
git add src-tauri/plugins/tauri-plugin-haptics/android src-tauri/gen/android/app/src/main/AndroidManifest.xml src-tauri/gen/android/tauri.settings.gradle
git commit -m "feat: AndroidのVibrationEffect実装とVIBRATE権限を追加(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 3: アプリ側 `vibrate` コマンドを追加し、TSバインディングを生成する

**Files:**
- Create: `src-tauri/src/commands/haptics.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`（`specta_builder()`内、コマンド一覧）

**Interfaces:**
- Consumes: `tauri_plugin_haptics::{HapticPattern, HapticsExt}`（Task 1）
- Produces: `#[tauri::command] pub fn vibrate(app: AppHandle, pattern: HapticPattern) -> crate::error::Result<()>`。TSバインディングで`commands.vibrate(pattern: HapticPattern) => Promise<Result<null, Error>>`として生成される（フロントは`unwrap()`で使う）。

- [ ] **Step 1: コマンドファイルを作成する**

`src-tauri/src/commands/haptics.rs`:

```rust
//! ハプティクス(振動)を発火する Tauri コマンド(Issue #26)。
//! 実体は tauri-plugin-haptics（Android実機のみ有効、デスクトップはno-op）。

use crate::error::{Error, Result};
use tauri::AppHandle;
pub use tauri_plugin_haptics::HapticPattern;
use tauri_plugin_haptics::HapticsExt;

/// 振動を発火する。失敗してもUXに影響しないため、呼び出し側(フロント)で例外を握りつぶす想定。
#[tauri::command]
#[specta::specta]
pub fn vibrate(app: AppHandle, pattern: HapticPattern) -> Result<()> {
    app.haptics()
        .vibrate(pattern)
        .map_err(|e| Error::Invalid(e.to_string()))
}
```

- [ ] **Step 2: commands/mod.rsに登録する**

`src-tauri/src/commands/mod.rs` の `pub mod draft;` の下に追加:

```rust
pub mod draft;
pub mod haptics;
```

- [ ] **Step 3: specta_builder()にコマンドを登録する**

`src-tauri/src/lib.rs` の `commands::draft::clear_auto_draft,` の下に追加:

```rust
            commands::draft::clear_auto_draft,
            commands::haptics::vibrate,
```

- [ ] **Step 4: TSバインディングを再生成して確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `vibrate` コマンドと `HapticPattern` 型（`"light" | "medium" | "success" | "warning" | "error"`）が生成されていることを確認する。

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/commands/haptics.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: vibrateコマンドを追加してTSバインディングを生成(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 4: `UiPrefs` に `haptics_enabled` を追加する

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `UiPrefs.haptics_enabled: bool`（デフォルト`true`、TS側では`hapticsEnabled?: boolean`として生成される）。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/ui.rs` の `deserializes_legacy_json_without_new_fields` テスト内、`assert_eq!(v.avatar_radius, 20);` の下に追加:

```rust
        // haptics_enabled も同様に既定値(true, 追加前は常にON相当の挙動)へフォールバックすること。
        assert!(v.haptics_enabled);
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd src-tauri && cargo test deserializes_legacy_json_without_new_fields`
Expected: FAIL（`haptics_enabled` フィールドが存在せずコンパイルエラー）。

- [ ] **Step 3: フィールドとデフォルト値を追加する**

`src-tauri/src/domain/ui.rs` の `UiPrefs` 構造体、`pub avatar_radius: i32,` の下に追加:

```rust
    pub avatar_radius: i32,
    /// ハプティクス(振動)を有効にするか（モバイル版のみ意味を持つ。Issue #26）。既定はON。
    #[serde(default = "default_haptics_enabled")]
    pub haptics_enabled: bool,
```

`default_avatar_radius()` 関数の下に追加:

```rust
fn default_avatar_radius() -> i32 {
    20
}

fn default_haptics_enabled() -> bool {
    true
}
```

`impl Default for UiPrefs` の `avatar_radius: default_avatar_radius(),` の下に追加:

```rust
            avatar_radius: default_avatar_radius(),
            haptics_enabled: default_haptics_enabled(),
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd src-tauri && cargo test deserializes_legacy_json_without_new_fields`
Expected: PASS

- [ ] **Step 5: 全Rustテストとバインディング生成を確認する**

Run: `cd src-tauri && cargo test`
Expected: 全PASS（`generates_frontend_bindings` により `frontend/src/bindings/tauri.gen.ts` の `UiPrefs` 型に `hapticsEnabled?: boolean` が追加される）。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/domain/ui.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: UiPrefsにhaptics_enabledを追加(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 5: フロントに `vibrate()` ラッパーを追加する

**Files:**
- Modify: `frontend/src/lib/ipc.ts`

**Interfaces:**
- Consumes: `commands.vibrate(pattern: HapticPattern)`（Task 3で生成されたTSバインディング）
- Produces: `vibrate(pattern: HapticPattern): void`（fire-and-forget。呼び出し側が`isMobilePlatform`と設定を見てガードする責務を持つ。ここではガードしない）

- [ ] **Step 1: HapticPattern型のimportを追加する**

`frontend/src/lib/ipc.ts` の先頭の import 文を変更:

```ts
import { commands, type Error as ApiError, type HapticPattern } from "../bindings/tauri.gen";
```

- [ ] **Step 2: vibrate()を追加する**

`frontend/src/lib/ipc.ts` の末尾（`playNotifySound` の下）に追加:

```ts
/// ハプティクス(振動)を発火する(Issue #26)。isMobilePlatform && 設定ONの場合のみ呼ぶこと
/// (このガードは呼び出し側の責務。ここでは行わない)。IPC自体の失敗(未対応デバイス等)は
/// UXに影響しないため握りつぶす(playNotifySoundと同じfire-and-forgetパターン)。
export function vibrate(pattern: HapticPattern): void {
  void unwrap(commands.vibrate(pattern)).catch(() => {});
}
```

- [ ] **Step 3: 型チェックを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 4: コミット**

```bash
git add frontend/src/lib/ipc.ts
git commit -m "feat: フロントにvibrate()ラッパーを追加(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 6: ノート投稿時のハプティクス発火

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`
- Test: `frontend/src/ui/ComposeBar.test.ts`

**Interfaces:**
- Consumes: `vibrate(pattern: HapticPattern)`（Task 5）, `isMobilePlatform`（`frontend/src/lib/platform.ts`, 既存）

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ComposeBar.test.ts` の先頭、他の `vi.mock` の下に追加:

```ts
// このテストファイルはモバイル向けのハプティクス発火を検証するため、実機OS判定に依らず
// isMobilePlatform を true に固定する(@tauri-apps/plugin-os のモックは "linux" のまま)。
vi.mock("../lib/platform", () => ({ isMobilePlatform: true }));
```

同ファイル末尾のいずれかの `describe`/`it` の後（ファイル末尾）に追加:

```ts
describe("ハプティクス(Issue #26)", () => {
  it("投稿成功後にvibrateコマンドをmediumパターンで呼ぶ", async () => {
    setupAccount();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "post_note") return Promise.resolve({ id: "n1" });
      return Promise.resolve(null);
    });
    const { getByTestId } = render(ComposeBar);
    await fireEvent.input(getByTestId("compose-textarea"), { target: { value: "hello" } });
    await fireEvent.click(getByTestId("compose-submit"));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("vibrate", { pattern: "medium" });
    });
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm test -- ComposeBar.test.ts -t "ハプティクス"`
Expected: FAIL（`vibrate` コマンドがまだ呼ばれない）

- [ ] **Step 3: submit()にハプティクス発火を追加する**

`frontend/src/ui/ComposeBar.svelte` のimport部分を変更（`import { commands, unwrap, unwrapAcc, formatError } from "../lib/ipc";` の行）:

```ts
  import { commands, unwrap, unwrapAcc, formatError, vibrate } from "../lib/ipc";
  import { isMobilePlatform } from "../lib/platform";
```

`submit()` 内の `await app.postNote(accountId, draft);` の下に追加:

```ts
      await app.postNote(accountId, draft);
      if (isMobilePlatform && (app.ui.hapticsEnabled ?? true)) vibrate("medium");
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm test -- ComposeBar.test.ts`
Expected: 全PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts
git commit -m "feat: ノート投稿成功時にハプティクスを発火する(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 7: リアクション付与時のハプティクス発火

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Test: `frontend/src/lib/store.svelte.test.ts`

**Interfaces:**
- Consumes: `vibrate(pattern: HapticPattern)`（Task 5）, `isMobilePlatform`（既存、`store.svelte.ts`は既にimport済み）

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/store.svelte.test.ts` の先頭、他の `vi.mock` の下に追加:

```ts
// このテストファイルはモバイル向けのハプティクス発火を検証するため、実機OS判定に依らず
// isMobilePlatform を true に固定する(@tauri-apps/plugin-os のモックは "linux" のまま)。
vi.mock("./platform", () => ({ isMobilePlatform: true }));
```

`describe("notification-only note actions (Issue #50 follow-up)", ...)` ブロックの下に追加:

```ts
describe("ハプティクス(Issue #26)", () => {
  it("新規リアクション付与時にvibrateコマンドをlightパターンで呼ぶ", async () => {
    const note = makeNote({ id: "note-haptics-add" });
    await app.toggleReaction(ACCOUNT_ID, note.id, "👍", note);
    expect(invokeMock).toHaveBeenCalledWith("vibrate", { pattern: "light" });
  });

  it("リアクション取り消し時はvibrateコマンドを呼ばない", async () => {
    const note = makeNote({ id: "note-haptics-remove", myReaction: "👍" });
    await app.toggleReaction(ACCOUNT_ID, note.id, "👍", note);
    expect(invokeMock).not.toHaveBeenCalledWith("vibrate", expect.anything());
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd frontend && pnpm test -- store.svelte.test.ts -t "ハプティクス"`
Expected: FAIL（1件目が「vibrateが呼ばれない」で失敗する）

- [ ] **Step 3: toggleReaction()にハプティクス発火を追加する**

`frontend/src/lib/store.svelte.ts:3` の既存importに `vibrate` を追加する:

```ts
import { commands, events, playNotifySound, unwrap, unwrapAcc, formatError, ForbiddenError, vibrate } from "./ipc";
```

`toggleReaction()` 内、新規リアクション付与の分岐の `this.#log("success", \`リアクション ${reaction}\`);` の下に追加:

```ts
        await unwrapAcc(accountId, commands.react(accountId, noteId, reaction));
        this.#log("success", `リアクション ${reaction}`);
        if (isMobilePlatform && (this.ui.hapticsEnabled ?? true)) vibrate("light");
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd frontend && pnpm test -- store.svelte.test.ts`
Expected: 全PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/lib/store.svelte.test.ts
git commit -m "feat: リアクション新規付与時にハプティクスを発火する(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 8: 設定画面にハプティクスON/OFFトグルを追加する

**Files:**
- Modify: `frontend/src/ui/settings/LayoutSection.svelte`

**Interfaces:**
- Consumes: `app.ui.hapticsEnabled`（Task 4）, `app.setUiPrefs()`（既存）, `isMobilePlatform`（既存）

- [ ] **Step 1: isMobilePlatformのimportとstateを追加する**

`frontend/src/ui/settings/LayoutSection.svelte` のimport部分を変更:

```svelte
<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import { Button } from "$lib/components/ui/button";
  import { isMobilePlatform } from "../../lib/platform";

  let width = $state(app.ui.defaultColumnWidth);
  let uiMode = $state(app.ui.uiMode ?? "auto");
  let mediaThumbnailHeight = $state(app.ui.mediaThumbnailHeight ?? 200);
  let hapticsEnabled = $state(app.ui.hapticsEnabled ?? true);
```

- [ ] **Step 2: save()に反映する**

`save()` 内の `await app.setUiPrefs({...})` を変更:

```ts
      await app.setUiPrefs({
        ...app.ui,
        defaultColumnWidth: w,
        uiMode,
        mediaThumbnailHeight: thumbHeight,
        hapticsEnabled,
      });
```

- [ ] **Step 3: トグルUIを追加する**

UIモード選択ブロック（`<p class="mb-4 mt-0 text-xs text-muted-foreground">モバイル版は投稿欄がFAB+モーダルに、PC版は投稿欄が常時表示になります。</p>` の直後）の下に追加:

```svelte
{#if isMobilePlatform}
  <label class="mb-2 flex items-center gap-2 text-sm"
    ><input type="checkbox" bind:checked={hapticsEnabled} /> ハプティクス(振動)を有効にする</label
  >
  <p class="mb-4 mt-0 text-xs text-muted-foreground">ノート投稿時とリアクション付与時に端末を振動させます(Issue #26)。</p>
{/if}
```

- [ ] **Step 4: 型チェックを確認する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 5: コミット**

```bash
git add frontend/src/ui/settings/LayoutSection.svelte
git commit -m "feat: 設定画面にハプティクスON/OFFトグルを追加(Issue #26)
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 9: 全体確認

**Files:** なし（検証のみ）

- [ ] **Step 1: フロントの全テストを実行する**

Run: `cd frontend && pnpm test`
Expected: 全PASS

- [ ] **Step 2: フロントの型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: エラーなし

- [ ] **Step 3: Rustの全テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: 全PASS

- [ ] **Step 4: Android実機/エミュレータで手動確認する（自動テスト不可）**

`cargo tauri android build --debug --target aarch64` で生成したAPKを実機/エミュレータにインストールし、以下を確認する:
- ノートを投稿すると振動する
- リアクションを付けると振動する（軽め）
- リアクションを取り消しても振動しない
- 設定画面のトグルをOFFにすると、投稿/リアクションいずれも振動しなくなる

Expected: 上記すべて期待通りに動作する。手動確認の結果をPR説明に記載する。
