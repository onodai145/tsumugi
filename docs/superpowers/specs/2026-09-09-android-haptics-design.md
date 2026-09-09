# スマホ版ハプティクス対応 設計 (Issue #26)

## 背景・要件

Issue #26より:
- OS通知はOS側の通知機構に任せる（アプリ内で通知時の振動は実装しない）
- **ノートを投稿したとき**と**リアクションを付けたとき**に振動させる

tsumugiのモバイル対応はAndroidのみ（`src-tauri/gen/android` は存在するが `gen/ios` は未生成）。以下の設計はAndroidのみを対象とする。

## アーキテクチャ

自作Tauriプラグイン `tauri-plugin-haptics` を `src-tauri/plugins/tauri-plugin-haptics/` にワークスペースクレートとして追加し、`src-tauri/Cargo.toml` からpath依存で参照する。crates.io公開や別リポジトリへの切り出しは行わない（再利用予定がなくYAGNIに反するため）。

- Android実装（Kotlin）で `Vibrator`/`VibratorManager` から取得した `Vibrator.vibrate(VibrationEffect)` を呼ぶ。`HapticFeedbackConstants`（`View.performHapticFeedback`）は使わない — OSの「タッチフィードバック」設定に連動してしまい、機種や設定次第で震え方が変わる/震えないことがあるため、`VibrationEffect.createOneShot(durationMs, amplitude)` でduration/振幅を直接指定する
- デスクトップ実装（Rust側のno-op fallback）はビルドを通すためだけに存在し、何もしない

### Rust API

パターンは今回使う分だけでなく、将来の用途（投稿失敗時のフィードバック、破壊的操作の確認など）を見越して5種を先に定義する。実装対象は`Medium`と`Light`の2箇所のみで、`Success`/`Warning`/`Error`は今回未使用（呼び出し元なし）。

```rust
pub enum HapticPattern {
    Light,   // 10ms, 単発 — リアクション付与
    Medium,  // 35ms, 単発 — ノート投稿完了
    Success, // 15msを2回, 40ms間隔のパルス — (将来用、今回は未使用)
    Warning, // 25msを2回, 80ms間隔のパルス — (将来用、今回は未使用。削除/ブロックなど破壊的操作の確認用を想定)
    Error,   // 60ms, 単発 — (将来用、今回は未使用)
}

pub fn vibrate(app: &AppHandle, pattern: HapticPattern) -> Result<()>;
```

振幅はいずれも `VibrationEffect.DEFAULT_AMPLITUDE` に任せる（機種ごとの最大振幅に依存させ、特定の値に固定しない）。

### `#[tauri::command]`

`src-tauri/src/commands/haptics.rs` に以下を追加し、`specta_builder()` に登録してTSバインディングを生成する:

```rust
#[tauri::command]
#[specta::specta]
pub fn vibrate(app: AppHandle, pattern: HapticPattern) -> Result<(), String>
```

`HapticPattern` は `specta::Type` を derive し、`domain/` ではなく `commands/haptics.rs`内にコマンド専用の型として定義する（永続化されない一時的な値のため `domain/` には置かない）。

### 設定

`UiPrefs`（`src-tauri/src/domain/ui.rs`）に `haptics_enabled: bool` を追加（`#[serde(default = "default_true")]` 相当、デフォルト `true`）。

設定画面（`frontend/src/ui/Settings.svelte`）にトグルを追加。`isMobilePlatform`（`frontend/src/lib/platform.ts`）が `true` のときのみ表示する（デスクトップでは振動しようがないため設定自体を隠す）。

### 呼び出し箇所

いずれも `isMobilePlatform && uiPrefs.hapticsEnabled` をガード条件とし、失敗しても握りつぶす（`void unwrap(commands.vibrate(...)).catch(() => {})` パターン、他の非致命的副作用と同様）。

1. **ノート投稿時**: `frontend/src/ui/ComposeBar.svelte` の `submit()` 内、`await app.postNote(accountId, draft)` が成功した直後 → `Medium`
2. **リアクション付与時**: `frontend/src/lib/store.svelte.ts` の `toggleReaction()` 内、`already === reaction` ではなく新規リアクションを付与する分岐（`targets.forEach((n) => addReaction(n, reaction))` 側）でAPI呼び出し成功後 → `Light`
   - リアクション取り消し時（`already === reaction`）や、他人のリアクション受信時は振動させない

### Android権限

`src-tauri/gen/android/app/src/main/AndroidManifest.xml` に `<uses-permission android:name="android.permission.VIBRATE" />` を追加する。`VIBRATE` はnormal permissionのためランタイム許可は不要。

このファイルは `cargo tauri android build`/`cargo tauri android init` で再生成されうる生成物だが、CLAUDE.mdの記載どおりリポジトリにコミットされ手で編集されている前提のため、直接編集する。

## エラーハンドリング

振動失敗（非対応デバイス、権限欠如など）はUXに影響しないため、フロント側では例外を握りつぶす。Rust側もAndroid以外のプラットフォームでは常に成功を返すno-opとする。

## テスト

- Rust: `HapticPattern` → `VibrationEffect` のマッピングはAndroid実機/エミュレータ依存のため自動テストは行わない（手動確認）。プラグインのRust側コマンドがpanicしないことのみ最小限のユニットテストで担保。`Success`/`Warning`/`Error`は未使用のため、呼び出しのモックテストは`Light`/`Medium`のみ書く。
- フロントエンド:
  - `ComposeBar.test.ts`: 投稿成功時に `commands.vibrate` が `Medium` で呼ばれることをモックで検証（`isMobilePlatform` をモックしてtrueにするケース）
  - `store.svelte.test.ts`: `toggleReaction` で新規リアクション付与時のみ `commands.vibrate` が `Light` で呼ばれ、取り消し時は呼ばれないことを検証

## 手動確認

自動テストではAndroid実機の振動は検証できないため、Android実機/エミュレータでの動作確認をリリース前に手動で行う（`cargo tauri android build --debug --target aarch64` でビルド）。
