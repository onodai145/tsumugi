# 背景に動画を設定できるようにする Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 背景設定を Base64 data URL 埋め込み方式からファイルパス参照方式に一本化し、動画ファイルを背景として設定・再生できるようにする（Issue #46）。

**Architecture:** `UiPrefs.background_image`(Base64) を `background_kind`(Image|Video) + `background_path`(app_data_dir/backgrounds/ 配下の絶対パス) に置き換える。ファイルは選択時に Rust コマンドで `backgrounds/` へコピーし、フロントは Tauri の `convertFileSrc` (asset プロトコル) でそのファイルを直接参照する。画像は既存の CSS `body::before` 描画を流用し、動画は新規に DOM 上の `<video>` 要素を管理して再生する。既存ユーザーの Base64 背景画像は起動時に自動でファイル化して移行する。

**Tech Stack:** Rust (Tauri v2 command, serde, specta), Svelte 5 (`$state`, imperative DOM), Tauri asset protocol (`@tauri-apps/api/core` の `convertFileSrc`)。

## Global Constraints

- Rust の新規コマンドは必ず `src-tauri/src/lib.rs` の `specta_builder()` 内 `collect_commands![...]` に登録すること（`tauri::Builder` だけでは TS バインディングが生成されない）。
- `UiPrefs` の全フィールドは `#[serde(rename_all = "camelCase")]` の対象であり、追加フィールドには必ず `#[serde(default = "...")]` を付け、旧バージョンJSONとの後方互換を壊さないこと。
- `frontend/src/bindings/tauri.gen.ts` はハンドコピーしない。`cd src-tauri && cargo test` を実行して再生成する。
- 動画背景のファイルサイズ上限は設けない（ユーザー判断に委ねる）。
- 動画背景では配置方法の Tile は選択肢に出さない（意味を持たないため）。
- コミットメッセージは件名のみ（本文・箇条書きは書かない）。

---

### Task 1: Rust — `UiPrefs` のデータモデルを `background_kind`/`background_path` に変更

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`

**Interfaces:**
- Produces: `pub enum BackgroundKind { Image, Video }` (`#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq)]`, `#[serde(rename_all = "camelCase")]`)
- Produces: `UiPrefs.background_kind: Option<BackgroundKind>`（未設定は `None`）
- Produces: `UiPrefs.background_path: Option<String>`（`backgrounds/` にコピーされた絶対パス）
- Consumes: なし（このタスクは domain 型のみ）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/domain/ui.rs` の `#[cfg(test)] mod tests` 内、既存の `roundtrips_keymap` テストの直後に追加する。

```rust
    #[test]
    fn background_kind_and_path_roundtrip() {
        let mut p = UiPrefs::default();
        p.background_kind = Some(BackgroundKind::Video);
        p.background_path = Some("/home/user/.local/share/tsumugi/backgrounds/a.mp4".into());
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"backgroundKind\":\"video\""));
        let back: UiPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(back.background_kind, Some(BackgroundKind::Video));
        assert_eq!(
            back.background_path,
            Some("/home/user/.local/share/tsumugi/backgrounds/a.mp4".to_string())
        );
    }

    #[test]
    fn background_kind_and_path_default_to_none_for_legacy_json() {
        // background_image(Base64)時代のJSONにも background_kind/background_path 追加後の
        // UiPrefs でそのまま読める(移行処理自体は settings.rs の責務、ここではデフォルト値のみ確認)。
        let v: UiPrefs = serde_json::from_str(
            r#"{"theme":"dark","defaultColumnWidth":320,"backgroundImage":"data:image/png;base64,AAAA"}"#,
        )
        .unwrap();
        assert_eq!(v.background_kind, None);
        assert_eq!(v.background_path, None);
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test domain::ui::tests::background_kind_and_path -- --nocapture`
Expected: FAIL（`background_kind`/`background_path` フィールドも `BackgroundKind` 型も存在しない、コンパイルエラー）

- [ ] **Step 3: 最小実装を書く**

`src-tauri/src/domain/ui.rs` の `ThemeColors` 定義の直前(ファイル先頭付近)に enum を追加する。

```rust
/// 背景メディアの種類。ファイル拡張子から判定して保持する。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundKind {
    Image,
    Video,
}
```

`UiPrefs` 内、`background_image: String` フィールド(92-94行目)を削除し、以下に置き換える。

```rust
    /// 背景メディアの種類。None なら背景メディア未設定。
    #[serde(default)]
    pub background_kind: Option<BackgroundKind>,
    /// 背景メディアのファイルパス（app_data_dir()/backgrounds/ 配下にコピーした絶対パス）。
    /// None なら背景メディア未設定。
    #[serde(default)]
    pub background_path: Option<String>,
```

`impl Default for UiPrefs` 内の `background_image: String::new(),` を以下に置き換える。

```rust
            background_kind: None,
            background_path: None,
```

既存の `roundtrips_keymap` テスト（`background_image: "data:image/png;base64,AAAA".into(),` を含む箇所）を以下に置き換える。

```rust
            background_kind: Some(BackgroundKind::Video),
            background_path: Some("/tmp/backgrounds/a.mp4".into()),
```

既存の `deserializes_legacy_json_without_new_fields` テスト内の `assert_eq!(v.background_image, "");` を削除し、代わりに以下を追加する。

```rust
        assert_eq!(v.background_kind, None);
        assert_eq!(v.background_path, None);
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test --lib domain::ui`
Expected: PASS（既存テスト・新規テストとも全て成功）

- [ ] **Step 5: コミット**

```bash
git add src-tauri/src/domain/ui.rs
git commit -m "feat: UiPrefsの背景画像フィールドをパス参照方式に変更"
```

---

### Task 2: Rust — `import_background_media` コマンドの追加

**Files:**
- Modify: `src-tauri/src/commands/mute.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `domain::BackgroundKind`（Task 1）
- Produces: `pub struct BackgroundMedia { pub kind: BackgroundKind, pub absolute_path: String }`（`#[derive(Serialize, Type)]`, `#[serde(rename_all = "camelCase")]`）
- Produces: `pub async fn import_background_media(app: AppHandle, path: String, previous_path: Option<String>) -> Result<BackgroundMedia>`

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/commands/mute.rs` の末尾、既存の `#[cfg(test)]` ブロックがなければ新設して追加する（無ければファイル末尾に新規追加）。

```rust
#[cfg(test)]
mod background_media_tests {
    use super::*;

    #[test]
    fn classify_background_kind_recognizes_common_extensions() {
        assert_eq!(classify_background_kind("a.png"), Some(BackgroundKind::Image));
        assert_eq!(classify_background_kind("a.GIF"), Some(BackgroundKind::Image));
        assert_eq!(classify_background_kind("a.mp4"), Some(BackgroundKind::Video));
        assert_eq!(classify_background_kind("a.webm"), Some(BackgroundKind::Video));
        assert_eq!(classify_background_kind("a.txt"), None);
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test --lib commands::mute::background_media_tests -- --nocapture`
Expected: FAIL（`classify_background_kind` 関数も `BackgroundKind` の import も無い、コンパイルエラー）

- [ ] **Step 3: 最小実装を書く**

`src-tauri/src/commands/mute.rs` の `use crate::domain::{MuteConfig, NotifyConfig, UiPrefs};` を以下に置き換える。

```rust
use crate::domain::{BackgroundKind, MuteConfig, NotifyConfig, UiPrefs};
```

`MAX_BACKGROUND_IMAGE_BYTES` 定数の直後に以下を追加する。

```rust
/// 背景メディアとして許容する拡張子から種類を判定する。未知の拡張子は None。
fn classify_background_kind(path: &str) -> Option<BackgroundKind> {
    match extension_lower(path).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "svg" => {
            Some(BackgroundKind::Image)
        }
        "mp4" | "webm" | "mov" | "m4v" | "mkv" | "avi" | "ogv" => Some(BackgroundKind::Video),
        _ => None,
    }
}

/// `import_background_media` の戻り値。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundMedia {
    pub kind: BackgroundKind,
    pub absolute_path: String,
}

/// 背景画像/動画として選んだファイルを `app_data_dir()/backgrounds/` にコピーし、
/// 種類判定した上でコピー後の絶対パスを返す。`previous_path` が指定されていれば、
/// コピー成功後に削除して backgrounds/ にゴミが溜まらないようにする。
/// サイズ上限は設けない（コピーは非同期コマンドなのでUIをブロックしない）。
#[tauri::command]
#[specta::specta]
pub async fn import_background_media(
    app: AppHandle,
    path: String,
    previous_path: Option<String>,
) -> Result<BackgroundMedia> {
    let kind = classify_background_kind(&path)
        .ok_or_else(|| Error::Invalid(format!("対応していないファイル形式です: {path}")))?;

    let backgrounds_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Invalid(format!("no app data dir: {e}")))?
        .join("backgrounds");
    tokio::fs::create_dir_all(&backgrounds_dir).await?;

    let ext = extension_lower(&path);
    let file_name = format!("{}.{ext}", uuid::Uuid::new_v4());
    let dest_path = backgrounds_dir.join(&file_name);

    let bytes = read_file_bytes(&app, &path).await?;
    tokio::fs::write(&dest_path, &bytes).await?;

    if let Some(prev) = previous_path {
        let _ = tokio::fs::remove_file(&prev).await;
    }

    Ok(BackgroundMedia {
        kind,
        absolute_path: dest_path.to_string_lossy().into_owned(),
    })
}
```

`src-tauri/src/lib.rs` の `commands::mute::read_audio_data_url,` の直後（`collect_commands![...]` 内）に以下を追加する。

```rust
            commands::mute::import_background_media,
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test --lib commands::mute`
Expected: PASS

- [ ] **Step 5: TSバインディングを再生成して確認する**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `BackgroundKind` / `BackgroundMedia` / `importBackgroundMedia` が生成されていることを確認する。

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/commands/mute.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: 背景メディアをbackgrounds/へコピーするコマンドを追加"
```

---

### Task 3: Rust — 既存 Base64 背景画像の自動移行

**Files:**
- Modify: `src-tauri/src/store/settings.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `domain::BackgroundKind`（Task 1）
- Produces: `pub fn SettingsStore::new(path: PathBuf, backgrounds_dir: PathBuf) -> Result<Self>`（引数を1つ追加、既存呼び出し元2箇所を更新）
- Produces: `pub fn migrate_from_legacy_sqlite(json_path: &Path, legacy_conn: &rusqlite::Connection, backgrounds_dir: &Path) -> Result<SettingsStore>`（引数を1つ追加）

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/settings.rs` の `#[cfg(test)] mod tests` 内、`persists_to_plain_text_json_file_and_reloads` テストの直後に追加する。

```rust
    /// backgroundImage(Base64 data URL)を含む旧形式JSONを読み込むと、backgrounds/ にファイルが
    /// 書き出され、backgroundKind/backgroundPath に自動移行されること。
    #[test]
    fn migrates_legacy_base64_background_image_to_file() {
        let path = std::env::temp_dir()
            .join(format!("tsumugi-legacy-bg-{}.json", uuid::Uuid::new_v4()));
        let backgrounds_dir = std::env::temp_dir()
            .join(format!("tsumugi-legacy-bg-dir-{}", uuid::Uuid::new_v4()));
        // 1x1 の透明PNGのBase64(実データでなくてもデコードできれば十分)。
        let legacy_json = r#"{
            "ui": {
                "theme": "dark",
                "defaultColumnWidth": 300,
                "backgroundImage": "data:image/png;base64,iVBORw0KGgo="
            }
        }"#;
        std::fs::write(&path, legacy_json).unwrap();

        let s = SettingsStore::new(path.clone(), backgrounds_dir.clone()).unwrap();
        let ui = s.load_ui().unwrap();
        assert_eq!(ui.background_kind, Some(crate::domain::BackgroundKind::Image));
        let bg_path = ui.background_path.expect("background_path should be set");
        assert!(std::path::Path::new(&bg_path).exists());
        assert!(bg_path.starts_with(backgrounds_dir.to_str().unwrap()));

        // 書き戻され、再読込でも同じ結果になること(次回起動時に再移行が走らない)。
        let reloaded = SettingsStore::new(path.clone(), backgrounds_dir.clone()).unwrap();
        assert_eq!(reloaded.load_ui().unwrap().background_path, Some(bg_path));

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir_all(&backgrounds_dir).ok();
    }

    /// backgroundImage が空文字/未設定の場合は移行処理が何もしないこと。
    #[test]
    fn no_migration_when_legacy_background_image_absent() {
        let path = std::env::temp_dir()
            .join(format!("tsumugi-no-bg-{}.json", uuid::Uuid::new_v4()));
        let backgrounds_dir = std::env::temp_dir()
            .join(format!("tsumugi-no-bg-dir-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, r#"{"ui":{"theme":"dark","defaultColumnWidth":300}}"#).unwrap();

        let s = SettingsStore::new(path.clone(), backgrounds_dir.clone()).unwrap();
        let ui = s.load_ui().unwrap();
        assert_eq!(ui.background_kind, None);
        assert_eq!(ui.background_path, None);
        assert!(!backgrounds_dir.exists());

        std::fs::remove_file(&path).ok();
    }
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd src-tauri && cargo test --lib store::settings::tests::migrates_legacy_base64_background_image_to_file`
Expected: FAIL（`SettingsStore::new` が2引数を受け付けない、コンパイルエラー）

- [ ] **Step 3: 最小実装を書く**

`src-tauri/src/store/settings.rs` 冒頭の `use` 群に以下を追加する。

```rust
use base64::{engine::general_purpose::STANDARD, Engine as _};
```

`SettingsStore::new` を以下に置き換える。

```rust
    /// 指定パスの設定ファイル(JSON)を読み込む。存在しなければ空の設定から始める。
    /// `backgrounds_dir` は旧形式(Base64背景画像)からの自動移行時にファイルを書き出す先。
    pub fn new(path: PathBuf, backgrounds_dir: PathBuf) -> Result<Self> {
        let data = load_json_or_default(&path, &backgrounds_dir)?;
        Ok(Self {
            backing: Backing::File(path),
            data: Mutex::new(data),
        })
    }
```

`load_json_or_default` を以下に置き換える。

```rust
fn load_json_or_default(path: &Path, backgrounds_dir: &Path) -> Result<SettingsData> {
    if !path.exists() {
        return Ok(SettingsData::default());
    }
    let s = std::fs::read_to_string(path)?;
    let mut value: serde_json::Value = serde_json::from_str(&s)?;
    migrate_legacy_pane_group_id(&mut value);
    let migrated = if let Some(ui) = value.get_mut("ui") {
        migrate_legacy_background_image(ui, backgrounds_dir)?
    } else {
        false
    };
    let data: SettingsData = serde_json::from_value(value)?;
    if migrated {
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, serde_json::to_string_pretty(&data)?)?;
        std::fs::rename(&tmp_path, path)?;
    }
    Ok(data)
}

/// 旧バージョンが `UiPrefs.backgroundImage` に Base64 data URL として保存していた背景画像を、
/// `backgrounds_dir` 配下にファイルとして書き出し、`backgroundKind`/`backgroundPath` へ移行する。
/// `backgroundImage` が存在しない/空文字なら何もしない。移行を行った場合は true を返す。
fn migrate_legacy_background_image(
    ui: &mut serde_json::Value,
    backgrounds_dir: &Path,
) -> Result<bool> {
    let Some(map) = ui.as_object_mut() else { return Ok(false) };
    let Some(data_url) = map.get("backgroundImage").and_then(|v| v.as_str().map(str::to_string))
    else {
        return Ok(false);
    };
    map.remove("backgroundImage");
    if data_url.is_empty() {
        return Ok(true);
    }
    let Some(rest) = data_url.strip_prefix("data:") else { return Ok(true) };
    let Some((mime, b64)) = rest.split_once(";base64,") else { return Ok(true) };

    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => "bin",
    };
    let bytes = STANDARD
        .decode(b64)
        .map_err(|e| Error::Invalid(format!("legacy backgroundImage base64 decode failed: {e}")))?;
    std::fs::create_dir_all(backgrounds_dir)?;
    let file_path = backgrounds_dir.join(format!("{}.{ext}", uuid::Uuid::new_v4()));
    std::fs::write(&file_path, bytes)?;

    map.insert("backgroundKind".into(), serde_json::json!("image"));
    map.insert(
        "backgroundPath".into(),
        serde_json::json!(file_path.to_string_lossy()),
    );
    Ok(true)
}
```

`migrate_from_legacy_sqlite` の引数と `ui` 取得部分を以下に置き換える。

```rust
pub fn migrate_from_legacy_sqlite(
    json_path: &Path,
    legacy_conn: &rusqlite::Connection,
    backgrounds_dir: &Path,
) -> Result<SettingsStore> {
```

（同関数内、既存の `let ui = get_kv("ui")? ... .unwrap_or_default();` を以下に置き換える）

```rust
    let ui: UiPrefs = match get_kv("ui")? {
        Some(s) => {
            let mut ui_value: serde_json::Value = serde_json::from_str(&s)?;
            migrate_legacy_background_image(&mut ui_value, backgrounds_dir)?;
            serde_json::from_value(ui_value)?
        }
        None => UiPrefs::default(),
    };
```

`src-tauri/src/lib.rs` の呼び出し元3箇所を更新する(190-229行目付近)。

```rust
            // Task 2 (import_background_media, commands/mute.rs) は app_data_dir()/backgrounds を
            // 使っているため、ここも同じディレクトリに揃える(config_dir にすると2つのタスクが
            // 別々のディレクトリを見てしまい、is_within_dir による削除保護が機能しなくなる)。
            let backgrounds_dir =
                app.path().app_data_dir().expect("no app data dir").join("backgrounds");
            let settings_path = config_dir.join("settings.json");
            let settings = if settings_path.exists() {
                SettingsStore::new(settings_path, backgrounds_dir.clone())
                    .expect("failed to open settings file")
            } else {
                let legacy_path = app.path().app_data_dir().ok().map(|d| d.join("tsumugi.db"));
                match legacy_path.filter(|p| p.exists()) {
                    Some(legacy_path) => {
                        let legacy_conn = db::open_settings(&legacy_path)
                            .expect("failed to open legacy settings db");
                        let settings = store::settings::migrate_from_legacy_sqlite(
                            &settings_path,
                            &legacy_conn,
                            &backgrounds_dir,
                        )
                        .expect("failed to migrate legacy settings");
                        drop(legacy_conn);
                        let backup_path = legacy_path.with_extension("db.bak");
                        std::fs::rename(&legacy_path, &backup_path)
                            .expect("failed to back up legacy db");
                        log::info!(
                            "migrated legacy settings from {} to {} (backed up old db to {})",
                            legacy_path.display(),
                            settings_path.display(),
                            backup_path.display()
                        );
                        settings
                    }
                    None => SettingsStore::new(settings_path, backgrounds_dir.clone())
                        .expect("failed to create settings file"),
                }
            };
```

`SettingsStore::new_in_memory()`(テスト専用)は変更不要。以下の既存テスト内の呼び出しは、それぞれ第2引数 `std::env::temp_dir()` を追加する(いずれも移行対象の`backgroundImage`を含まないJSONのため、実際に`backgrounds/`へ書き出されることはない)。

- `cache_backend_postgres_config_persists_across_restart_via_json_file`: `SettingsStore::new(path.clone()).unwrap()` が2箇所(`s`/`reloaded`)あり、いずれも `SettingsStore::new(path.clone(), std::env::temp_dir()).unwrap()` に置き換える。
- `persists_to_plain_text_json_file_and_reloads`: 同様に `s`/`reloaded` の2箇所を `SettingsStore::new(path.clone(), std::env::temp_dir()).unwrap()` に置き換える。
- `loads_settings_with_legacy_snake_case_pane_layout_group_id`: `SettingsStore::new(path.clone()).unwrap()` を `SettingsStore::new(path.clone(), std::env::temp_dir()).unwrap()` に置き換える。
- `migrates_from_legacy_sqlite_to_json`: `migrate_from_legacy_sqlite(&json_path, &legacy_conn).unwrap()` を `migrate_from_legacy_sqlite(&json_path, &legacy_conn, &std::env::temp_dir()).unwrap()` に置き換える。

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd src-tauri && cargo test --lib store::settings`
Expected: PASS（新規2件＋既存全件）

- [ ] **Step 5: 全体ビルドを確認する**

Run: `cd src-tauri && cargo build`
Expected: 成功（`lib.rs`側の呼び出し元修正も含めてコンパイル通過）

- [ ] **Step 6: コミット**

```bash
git add src-tauri/src/store/settings.rs src-tauri/src/lib.rs
git commit -m "feat: 旧Base64背景画像を起動時に自動でファイルへ移行する"
```

---

### Task 4: Rust — asset プロトコルの有効化

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: なし
- Produces: なし（設定ファイルのみ。フロントの `convertFileSrc` 呼び出し (Task 6) が動作する前提条件）

- [ ] **Step 1: 設定を変更する**

`src-tauri/tauri.conf.json` の `"app"."security"` を以下に置き換える。

```json
    "security": {
      "csp": null,
      "assetProtocol": {
        "enable": true,
        "scope": ["$APPDATA/backgrounds/*"]
      }
    }
```

- [ ] **Step 2: 動作確認**

Run: `cargo tauri dev`（リポジトリルートから、CLAUDE.md記載の通り）を起動し、コンソールに `assetProtocol` 関連のエラーが出ないことを確認する（この時点では背景動画UIはまだ無いため、起動できることのみ確認）。確認後は必ず自分で起動したプロセスを終了する。

- [ ] **Step 3: コミット**

```bash
git add src-tauri/tauri.conf.json
git commit -m "feat: 背景メディア配信用にassetProtocolを有効化"
```

---

### Task 5: フロントエンド — 配置方法マッピングに動画用 object-fit を追加

**Files:**
- Modify: `frontend/src/lib/backgroundFitMode.ts`
- Test: `frontend/src/lib/backgroundFitMode.test.ts`

**Interfaces:**
- Produces: `export const BACKGROUND_FIT_MODE_OBJECT_FIT: Record<"cover" | "contain" | "fill", "cover" | "contain" | "fill">`
- Produces: `export const BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO: { value: BackgroundFitMode; label: string }[]`（Tileを除いたもの）

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/lib/backgroundFitMode.test.ts` を新規作成する。

```typescript
import { describe, expect, it } from "vitest";
import {
  BACKGROUND_FIT_MODE_OBJECT_FIT,
  BACKGROUND_FIT_MODE_OPTIONS,
  BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO,
} from "./backgroundFitMode";

describe("backgroundFitMode", () => {
  it("maps cover/contain/fill to matching object-fit keywords", () => {
    expect(BACKGROUND_FIT_MODE_OBJECT_FIT.cover).toBe("cover");
    expect(BACKGROUND_FIT_MODE_OBJECT_FIT.contain).toBe("contain");
    expect(BACKGROUND_FIT_MODE_OBJECT_FIT.fill).toBe("fill");
  });

  it("excludes tile from the video options list", () => {
    expect(BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO.map((o) => o.value)).toEqual(["cover", "contain", "fill"]);
    // 元のリストにはtileが含まれ続けること(画像用は変更しない)
    expect(BACKGROUND_FIT_MODE_OPTIONS.map((o) => o.value)).toContain("tile");
  });
});
```

- [ ] **Step 2: テストを実行して失敗を確認する**

Run: `cd frontend && pnpm test -- backgroundFitMode`
Expected: FAIL（`BACKGROUND_FIT_MODE_OBJECT_FIT`/`BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO` が存在しない）

- [ ] **Step 3: 最小実装を書く**

`frontend/src/lib/backgroundFitMode.ts` の末尾に追加する。

```typescript
// object-fit は background-size と構文が異なる("fill"の縦横比無視表現がキーワード自体)ため、
// 動画の<video>要素向けに別途マッピングする。
export const BACKGROUND_FIT_MODE_OBJECT_FIT: Record<"cover" | "contain" | "fill", "cover" | "contain" | "fill"> = {
  cover: "cover",
  contain: "contain",
  fill: "fill",
};

// Tile(並べて繰り返し)は動画では意味を持たないため、動画選択時のUIからは除外する。
export const BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO = BACKGROUND_FIT_MODE_OPTIONS.filter(
  (o) => o.value !== "tile",
);
```

- [ ] **Step 4: テストを実行して成功を確認する**

Run: `cd frontend && pnpm test -- backgroundFitMode`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/backgroundFitMode.ts frontend/src/lib/backgroundFitMode.test.ts
git commit -m "feat: 背景動画用のobject-fitマッピングを追加"
```

---

### Task 6: フロントエンド — `store.svelte.ts` を新フィールド・動画描画対応に変更

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.importBackgroundMedia(path, previousPath)`（Task 2 の生成バインディング）, `BACKGROUND_FIT_MODE_OBJECT_FIT`（Task 5）
- Produces: `AppStore.pickBackgroundMedia(): Promise<{ kind: "image" | "video"; absolutePath: string } | null>`（旧 `pickBackgroundImage` を置き換え）
- Produces: `AppStore.ui.backgroundKind: "image" | "video" | null`, `AppStore.ui.backgroundPath: string | null`（旧 `backgroundImage: string` を置き換え）

- [ ] **Step 1: 失敗するテストを書く**

このタスクは既存の手書きDOM操作コードの書き換えが中心で、Vitestでのユニットテストが薄いファイルのため、既存のフロントエンドテスト方針に合わせて`#applyBackground`のロジックを検証しやすい形に保つ。`frontend/src/lib/store.svelte.ts` には既存のユニットテストが無いため、代わりに Task 8 の `BackgroundSection.svelte` 側の手動確認とTask 9の手動確認で検証する。ここでは型チェック(`pnpm check`)を先に失敗させて確認する。

Run: `cd frontend && pnpm check`
Expected: この時点ではまだ変更していないため PASS。Step 3 のコード変更後に一旦 `backgroundImage` 参照が残っていればここでエラーになる想定を確認するためのベースライン取得。

- [ ] **Step 2: (スキップ理由の確認のみ)**

型チェックがベースラインで通ることを確認した上で Step 3 に進む。

- [ ] **Step 3: 実装を書く**

`frontend/src/lib/store.svelte.ts` の import 群、`import { open as openDialog } from "@tauri-apps/plugin-dialog";` の直後に追加する。

```typescript
import { convertFileSrc } from "@tauri-apps/api/core";
```

`import { BACKGROUND_FIT_MODE_CSS } from "./backgroundFitMode";` を以下に置き換える。

```typescript
import { BACKGROUND_FIT_MODE_CSS, BACKGROUND_FIT_MODE_OBJECT_FIT } from "./backgroundFitMode";
```

`ui = $state<UiPrefs>({...})` 初期値内の `backgroundImage: "",` を以下に置き換える。

```typescript
    backgroundKind: null,
    backgroundPath: null,
```

`#bgVideoEl` 用のプライベートフィールドを、`#themeMediaCleanup` の直後に追加する。

```typescript
  // 背景が動画の間だけ生成し、document.body に固定配置する <video> 要素。
  // #applyBackground が呼ばれるたびに再利用し、種類が変わったら display を切り替える。
  #bgVideoEl: HTMLVideoElement | null = null;
```

`boot()` 内の以下の行を置き換える。

```typescript
        backgroundImage: ui.backgroundImage ?? "",
```
↓
```typescript
        backgroundKind: ui.backgroundKind ?? null,
        backgroundPath: ui.backgroundPath ?? null,
```

`setUiPrefs()` 内の以下の行を置き換える。

```typescript
      backgroundImage: prefs.backgroundImage ?? "",
```
↓
```typescript
      backgroundKind: prefs.backgroundKind ?? null,
      backgroundPath: prefs.backgroundPath ?? null,
```

`pickBackgroundImage()` を以下に置き換える。

```typescript
  /// 画像/動画ファイルを選んで backgrounds/ へコピーする（保存は setUiPrefs で）。
  /// 戻り値は種類とコピー後の絶対パス。previousPath を渡すと旧ファイルを削除する。
  async pickBackgroundMedia(previousPath: string | null): Promise<{ kind: "image" | "video"; absolutePath: string } | null> {
    const path = await openDialog({
      multiple: false,
      filters: [
        { name: "画像", extensions: ["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"] },
        { name: "動画", extensions: ["mp4", "webm", "mov", "m4v", "mkv", "avi", "ogv"] },
      ],
    });
    if (!path || Array.isArray(path)) return null;
    const media = await unwrap(commands.importBackgroundMedia(path, previousPath ?? null));
    return { kind: media.kind, absolutePath: media.absolutePath };
  }
```

`#applyBackground` を以下に置き換える。

```typescript
  /// 背景メディア(画像/動画)/オーバーレイ/カラム不透明度を <html> と <video> に反映する。
  #applyBackground(
    prefs: Pick<
      UiPrefs,
      | "backgroundKind"
      | "backgroundPath"
      | "backgroundDim"
      | "backgroundBlur"
      | "columnOpacity"
      | "backgroundFitMode"
      | "backgroundPosition"
    >,
  ) {
    const root = document.documentElement;
    const kind = prefs.backgroundKind ?? null;
    const path = prefs.backgroundPath ?? "";
    const assetUrl = path ? convertFileSrc(path) : "";
    // Tileは動画では意味を持たないため、動画のときはcoverへフォールバックする。
    const rawFitMode = prefs.backgroundFitMode ?? "cover";
    const fitMode = kind === "video" && rawFitMode === "tile" ? "cover" : rawFitMode;

    if (kind === "video" && assetUrl) {
      root.style.removeProperty("--bg-image");
      if (!this.#bgVideoEl) {
        const v = document.createElement("video");
        v.autoplay = true;
        v.loop = true;
        v.muted = true;
        v.playsInline = true;
        v.className = "bg-video";
        document.body.prepend(v);
        this.#bgVideoEl = v;
      }
      if (this.#bgVideoEl.src !== assetUrl) this.#bgVideoEl.src = assetUrl;
      this.#bgVideoEl.style.display = "";
      const objectFit =
        BACKGROUND_FIT_MODE_OBJECT_FIT[fitMode as keyof typeof BACKGROUND_FIT_MODE_OBJECT_FIT] ?? "cover";
      this.#bgVideoEl.style.objectFit = objectFit;
      this.#bgVideoEl.style.filter = `blur(${prefs.backgroundBlur ?? 0}px)`;
    } else {
      if (this.#bgVideoEl) this.#bgVideoEl.style.display = "none";
      if (kind === "image" && assetUrl) {
        root.style.setProperty("--bg-image", `url("${assetUrl}")`);
      } else {
        root.style.removeProperty("--bg-image");
      }
    }

    root.style.setProperty("--bg-dim", String((prefs.backgroundDim ?? 0) / 100));
    root.style.setProperty("--bg-blur", `${prefs.backgroundBlur ?? 0}px`);
    root.style.setProperty("--column-opacity", `${prefs.columnOpacity ?? 100}%`);
    const [bgSize, bgRepeat] = BACKGROUND_FIT_MODE_CSS[fitMode] ?? BACKGROUND_FIT_MODE_CSS.cover;
    root.style.setProperty("--bg-size", bgSize);
    root.style.setProperty("--bg-repeat", bgRepeat);
    const bgPosition = BACKGROUND_POSITION_CSS[prefs.backgroundPosition ?? "center"] ??
      BACKGROUND_POSITION_CSS.center;
    root.style.setProperty("--bg-position", bgPosition);
    if (this.#bgVideoEl) this.#bgVideoEl.style.objectPosition = bgPosition;
  }
```

- [ ] **Step 4: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: PASS（`BackgroundSection.svelte` はまだ旧APIを参照しているため、この時点ではまだ型エラーが残る想定。Task 8 で解消する。ここでは `store.svelte.ts` 側にエラーが無いことのみ確認する）

- [ ] **Step 5: コミット**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: storeの背景状態をbackgroundKind/backgroundPathに置き換え"
```

---

### Task 7: フロントエンド — `app.css` の背景描画をダミング分離＋動画対応に変更

**Files:**
- Modify: `frontend/src/app.css`

**Interfaces:**
- Consumes: `--bg-image`/`--bg-dim`/`--bg-blur`/`--bg-size`/`--bg-position`/`--bg-repeat`（Task 6 が設定するCSS変数、変数名は変更なし）
- Produces: `.bg-video` クラス（Task 6 が生成する `<video>` 要素に付与）

- [ ] **Step 1: 実装を書く**

`frontend/src/app.css` の `body::before` ブロックを以下に置き換える(コメントも実情に合わせて更新)。

```css
/* 背景メディア(設定→表示で選択)。#applyBackground が --bg-image 等を <html> にセットする。
   画像は body ではなく ::before に描画し、blur を背景だけに掛ける(body 自体を blur すると
   子要素まで巻き込まれるため)。位置は fixed で viewport 全体、z-index で最背面に固定。
   暗さ(dim)オーバーレイは ::after に分離している。動画では background-image が使えず
   <video class="bg-video"> (JSが動的に挿入する)を使うため、::before の役割を
   「画像描画専用」に絞り、暗さはメディアの種類に関わらず ::after で共通に掛けられるようにしている。 */
body {
  position: relative;
}
body::before {
  content: "";
  position: fixed;
  inset: 0;
  z-index: -1;
  background-image: var(--bg-image, none);
  background-size: var(--bg-size, cover);
  background-position: var(--bg-position, center);
  background-repeat: var(--bg-repeat, no-repeat);
  filter: blur(var(--bg-blur, 0px));
}
body::after {
  content: "";
  position: fixed;
  inset: 0;
  z-index: -1;
  background-color: rgba(0, 0, 0, var(--bg-dim, 0));
  pointer-events: none;
}
/* 背景動画用。store.svelte.ts の #applyBackground が document.body の先頭に挿入し、
   backgroundKind に応じて表示/非表示(display)を切り替える。 */
.bg-video {
  position: fixed;
  inset: 0;
  z-index: -1;
  width: 100%;
  height: 100%;
  object-position: center;
  pointer-events: none;
}
```

- [ ] **Step 2: 手動確認**

Run: `cargo tauri dev`（リポジトリルートから）を起動し、既存の背景画像設定（画像1枚を設定）で暗さ・ぼかし・配置方法が従来どおり表示されることを目視確認する（この時点ではまだ動画選択UIは無いため、画像側のみ回帰確認）。確認後は自分で起動したプロセスを終了する。

- [ ] **Step 3: コミット**

```bash
git add frontend/src/app.css
git commit -m "feat: 背景の暗さオーバーレイを分離し動画レイヤーのCSSを追加"
```

---

### Task 8: フロントエンド — `BackgroundSection.svelte` を新API・動画UIに対応

**Files:**
- Modify: `frontend/src/ui/settings/BackgroundSection.svelte`

**Interfaces:**
- Consumes: `app.pickBackgroundMedia(previousPath)`（Task 6）, `BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO`（Task 5）
- Produces: なし（UIコンポーネントの末端）

- [ ] **Step 1: 実装を書く**

`frontend/src/ui/settings/BackgroundSection.svelte` の `<script>` ブロックを以下に置き換える。

```svelte
<script lang="ts">
  import { app } from "../../lib/store.svelte";
  import {
    BACKGROUND_FIT_MODE_OPTIONS,
    BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO,
    type BackgroundFitMode,
  } from "../../lib/backgroundFitMode";
  import { BACKGROUND_POSITION_GRID, type BackgroundPosition } from "../../lib/backgroundPosition";
  import { Button } from "$lib/components/ui/button";
  import { convertFileSrc } from "@tauri-apps/api/core";

  let backgroundKind = $state<"image" | "video" | null>(app.ui.backgroundKind ?? null);
  let backgroundPath = $state<string | null>(app.ui.backgroundPath ?? null);
  let backgroundDim = $state(app.ui.backgroundDim ?? 0);
  let backgroundBlur = $state(app.ui.backgroundBlur ?? 0);
  let columnOpacity = $state(app.ui.columnOpacity ?? 100);
  let backgroundFitMode = $state<BackgroundFitMode>(
    (app.ui.backgroundFitMode as BackgroundFitMode) ?? "cover",
  );
  let backgroundPosition = $state<BackgroundPosition>(
    (app.ui.backgroundPosition as BackgroundPosition) ?? "center",
  );
  let pickingImage = $state(false);
  let busy = $state(false);
  let err = $state<string | null>(null);
  let saved = $state(false);

  const previewUrl = $derived(backgroundPath ? convertFileSrc(backgroundPath) : "");
  const fitModeOptions = $derived(
    backgroundKind === "video" ? BACKGROUND_FIT_MODE_OPTIONS_FOR_VIDEO : BACKGROUND_FIT_MODE_OPTIONS,
  );

  // 背景メディアの基準点（9点グリッド、Issue #76）。position→アクセシブルラベル。
  const positionLabels: Record<BackgroundPosition, string> = {
    "top-left": "左上",
    top: "上",
    "top-right": "右上",
    left: "左",
    center: "中央",
    right: "右",
    "bottom-left": "左下",
    bottom: "下",
    "bottom-right": "右下",
  };

  async function pickMedia() {
    err = null;
    pickingImage = true;
    try {
      const media = await app.pickBackgroundMedia(backgroundPath);
      if (media) {
        backgroundKind = media.kind;
        backgroundPath = media.absolutePath;
        // 動画選択時、直前がTileだった場合はUIから選択肢が消えるためcoverへ揃える。
        if (media.kind === "video" && backgroundFitMode === "tile") backgroundFitMode = "cover";
      }
    } catch (e) {
      err = String(e);
    } finally {
      pickingImage = false;
    }
  }

  function clearMedia() {
    backgroundKind = null;
    backgroundPath = null;
  }

  async function save() {
    err = null;
    saved = false;
    busy = true;
    try {
      // このセクションが編集しないフィールド(表示・テーマ等)を保存で消さないよう、
      // 現在の app.ui をベースに編集項目だけ上書きする。
      await app.setUiPrefs({
        ...app.ui,
        backgroundKind,
        backgroundPath,
        backgroundDim,
        backgroundBlur,
        columnOpacity,
        backgroundFitMode,
        backgroundPosition,
      });
      saved = true;
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }
</script>
```

続けて、テンプレート部分の背景画像プレビュー・ボタン部分(`{#if backgroundImage}`〜`</div>` までのプレビュー行)を以下に置き換える。

```svelte
<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">背景メディア（画像/動画）</span>
  <div class="flex items-center gap-2.5">
    {#if backgroundKind === "image" && previewUrl}
      <img class="h-9 w-14 rounded-md border border-border object-cover" src={previewUrl} alt="背景プレビュー" />
    {:else if backgroundKind === "video" && previewUrl}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video class="h-9 w-14 rounded-md border border-border object-cover" src={previewUrl} muted autoplay loop playsinline></video>
    {/if}
    <Button type="button" variant="outline" size="sm" disabled={pickingImage} onclick={pickMedia}>
      {pickingImage ? "読み込み中…" : backgroundKind ? "メディアを変更" : "画像/動画を選択"}
    </Button>
    {#if backgroundKind}
      <Button type="button" variant="outline" size="sm" onclick={clearMedia}>解除</Button>
    {/if}
  </div>
</div>
```

以降、`{#if backgroundImage}` で始まる配置方法/基準点/スライダー群のブロックの `{#if backgroundImage}` を `{#if backgroundKind}` に置き換え、内部の `BACKGROUND_FIT_MODE_OPTIONS as m` を `fitModeOptions as m` に置き換える。

- [ ] **Step 2: 型チェックを実行する**

Run: `cd frontend && pnpm check`
Expected: PASS（Task 6 で残っていた `backgroundImage` 参照の型エラーが解消される）

- [ ] **Step 3: コミット**

```bash
git add frontend/src/ui/settings/BackgroundSection.svelte
git commit -m "feat: 背景設定UIを画像/動画選択に対応させる"
```

---

### Task 9: 手動確認・CHANGELOG更新

**Files:**
- Modify: `CHANGELOG.md`（存在する場合。無ければこのステップはスキップし、リリース時の `scripts/release.sh` が生成する `git-cliff` に委ねる）

**Interfaces:**
- Consumes: Task 1〜8 の全実装
- Produces: なし（最終確認タスク）

- [ ] **Step 1: Rust全テストを実行する**

Run: `cd src-tauri && cargo test`
Expected: PASS（全テストスイート）

- [ ] **Step 2: フロントエンド全テスト・型チェックを実行する**

Run: `cd frontend && pnpm check && pnpm test`
Expected: PASS

- [ ] **Step 3: Xvfb越しに実UIで動作確認する**

`dev server verification must use virtual display` のメモリ通り、`WAYLAND_DISPLAY`をunsetし、Xvfb越しに`cargo tauri dev`（リポジトリルートから）を起動する。以下を目視確認する。

- 動画ファイル(mp4)を背景に設定→保存後、ループ再生・ミュートされていること
- Cover/Fit/Fillの各配置で動画の見た目が変わること（Tileの選択肢が出ないこと）
- 基準点(9点グリッド)を変えると動画の表示位置が変わること
- 暗さ・ぼかしスライダーが動画にも効くこと
- 画像を設定した場合の見た目が変更前と変わらないこと(回帰確認)
- 設定→表示で「解除」を押すと背景が消えること

確認後は自分で起動した `cargo tauri dev` プロセスを終了する。

- [ ] **Step 4: 旧設定ファイルからの移行を手動確認する**

アプリを終了した状態で、設定ファイル(`app_config_dir()/settings.json`、Linuxでは通常 `~/.local/share/com.onodai.tsumugi/` 配下だが実際のパスはOS依存のため起動ログで確認する)の `ui.backgroundImage` にテスト用のBase64 data URLを手動で仕込み、`cargo tauri dev` を起動して背景が正しく表示され、`ui.backgroundKind`/`ui.backgroundPath` に自動移行されていることを確認する。

- [ ] **Step 5: CHANGELOGへの記載を確認する**

このリポジトリのCHANGELOGは `scripts/release.sh` が `git-cliff` でコミットログから生成する運用のため、個別の追記は不要。Task 1〜8 のコミットメッセージが `feat:` prefixで一貫していることを確認するのみでよい。

- [ ] **Step 6: 最終コミット（あれば）**

手動確認のみで差分が無い場合、このタスクはコミット不要。確認中に見つかった不具合を直した場合は、その修正を都度コミットする。
