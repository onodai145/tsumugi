# ローカル翻訳 段階2（モデルの取得・ライセンス同意・設定 UI）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定画面から、PLaMo 3 Translate のモデル（573,126,848 バイト）を Hugging Face からダウンロード・検証・削除でき、ライセンスへの同意が Rust 側で強制され、翻訳エンジンを設定画面で切り替えられる状態にする（デスクトップ）。

**Architecture:** `translate/model_store.rs` に固定値（URL・リビジョン・SHA-256・ライセンス版）と状態の型、`translate/download.rs` に Range 再開・SHA-256 検証付きの純粋に近いダウンロード処理、`translate/local_models.rs` に「同時に1つ・状態保持・進捗の間引き・削除」を持つ `LocalModels` を置く。コマンドは `commands/local_model.rs`、進捗は tauri-specta の Event。フロントは `LocalModelCard` と `LicenseConsentDialog` を `ExternalIntegrationSection` に足す。

**Tech Stack:** Rust（tokio、reqwest のストリーム、`sha2`、`fs4`）、tauri-specta、wiremock、Svelte 5 + Vitest。

**Spec:** `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`（Issue #603）の §2・§3・§7・§「ドキュメント」。段階1の plan: `docs/superpowers/plans/2026-11-14-local-translate-core.md`。

## Scope

- この plan（段階2）: モデルの取得、ライセンス同意、設定 UI、ユーザーガイド、`pt` など非対応言語の注記。デスクトップのみ。
- 段階3（別 plan）: Android（通信種別 `get_network_status`、ビルド、CI、実機検証）。**通信種別の警告ダイアログは段階3**で、この plan のダウンロード確認はサイズ表示だけ。
- 前提: 段階1（PR #626、ブランチ `feat/issue-603-local-translate-core`）のコードに依存する。この plan のブランチは、そこから切った `feat/issue-603-local-translate-download`（worktree `../tsumugi-603-p2`）。**#626 が `main` にマージされるまで、PR のベースは `feat/issue-603-local-translate-core`**、マージ後に `main` へ付け替える。

## Rulings（plan 段階での判断。spec が決めていなかったもの）

- **R1 キャンセルは `.part` を残す。** 「途中のファイルは Range で再開する」を、キャンセルにも適用する（再度ダウンロードを押すと途中から）。`.part` を消すのは、削除ボタン・ハッシュ不一致・サイズ超過のとき。
- **R2 同意ダイアログの出し方。** 保存済みの同意版が現行のライセンス版と同じなら、ダイアログを出さずに開始する。違う・未同意のときだけ出す（spec「版が変わったときは次のダウンロード時にだけ再同意」）。強制は常に Rust 側（引数の版が現行と一致しなければ拒否）。
- **R3 同意版は、ダウンロードの開始に成功したあとで保存する。** 非対応環境などで開始できなかったのに「同意済み」が残らないようにする。
- **R4 `get_local_model_status` は `LocalModelInfo` を返す**（`status`、`size_bytes`、`license_version`、`license_accepted`、`supported`）。Event は `LocalModelStatus` だけを運ぶ。同意の有無は開始後にフロントが再取得する。
- **R5 非対応言語の判定は Rust のコマンド `is_translate_lang_supported(lang)` に一本化する**（`lang::to_plamo_lang`）。フロントに言語表を複製しない。
- **R6 ダウンロード用の HTTP クライアントは `net_guard::build_fetch_client(USER_AGENT, FetchAllowlist::default())`**（私的アドレス拒否つき、許可リストは空）。Hugging Face は `huggingface.co` から `*.hf.co` の CDN へリダイレクトする（2026-11-14 に `curl -I` で確認、`Range` は 206 で通る）。どちらも公開 IP なのでガードを通る。
- **R7 About 画面には "Built with PLaMo" を足さない。** About の「ライセンス」はアプリ自身（MIT）の行で、第三者モデルの表示は不適切。表示先はモデルカード・結果欄の見出し（段階1で実装済み）・ユーザーガイド。

## Global Constraints

- モデル: `pfnet/plamo-3-translate-0.8b-GGUF` の `plamo-3-translate-0.8b-Q4_K_M.gguf`、`573126848` バイト、SHA-256 `70891ec70dc7ab4205745a38307867c2881b99239f20327927633abfe4bc9ec6`（Task 2 の定数が正。HF API の `lfs.oid` から取得済み）、リビジョン `1cf96ae02133958a20074bfdfab76517bb737d00`（2026-11-09 の最新コミット）。
- ライセンス版 `2026-11-09`（PLaMo Community License）。全文の写しは `https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/blob/<リビジョン>/LICENSE/ja`。
- 保存先 `app_data_dir/models/`。書き込みは `<ファイル名>.part` に行い、ハッシュ検証後にリネーム。読み込み時の検査はサイズだけ（ハッシュはダウンロード完了時だけ）。
- 同時に走るダウンロードは1つ。設定画面を閉じても続く。起動時・設定を開いただけでは通信しない。
- コマンドと Event は `specta_builder()` に登録する（`tauri::Builder` だけではなく）。`u64` は `#[specta(type = specta_typescript::Number)]` で TS の `number` にする（`domain/note.rs` の `i64` と同じ）。
- 同意版は `SettingsData` の新フィールド。設定のエクスポートに含めず、インポートでは現在の値を維持する。
- エラーは `Error::Invalid` の文言。ダウンロードの失敗理由（`LocalModelStatus::Failed.reason`）は利用者向けの日本語で、URL・ファイルパス・本文を含めない。
- 視覚値は `docs/design/style-guide.md` のスケールに従い、一回限りの値（`rounded-[Npx]` など）は使わない。
- コミットメッセージは**件名1行のみ**（`type(scope): 件名 (#603)`）。`Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は2つ目の `-m` で付ける。`--no-verify` は使わない。失敗・タイムアウトしたらリトライせずユーザーに報告する。**このセッションの cwd は `main` のチェックアウトなので、コミットは必ず `git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit ...` で行う**（hook が `main` 上の cwd からのコミットを拒否する）。コミットは1コマンドずつ。
- ブランチ: `feat/issue-603-local-translate-download`（作成済み）。
- **ビルドのメモリ**（段階1と同じ。C++ のビルドで OOM になった）:

  ```bash
  export CARGO_BUILD_JOBS=4 CMAKE_BUILD_PARALLEL_LEVEL=4
  export CARGO_TARGET_DIR=$HOME/.cache/tsumugi-603/target   # 段階1と共有。/tmp は tmpfs なので使わない
  systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G <コマンド>
  ```

  Rust の作業は `cd /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2/src-tauri`、フロントは `.../tsumugi-603-p2/frontend`。`frontend` で初回に `pnpm install` が要る。

## Review Focus

- ダウンロード中にアプリが落ちた・回線が切れた → `.part` が残り、次回は途中から再開する。再開後のハッシュが合わなければ `.part` を捨てて失敗にする（Task 3）。
- サーバーが `Range` を無視して 200 を返す、`.part` が期待サイズ以上、416 が返る → 壊れた継ぎ足しにならず、最初からやり直す（Task 3）。
- ディスクが足りない・書き込み中にディスクが満杯 → 開始前に `NoSpace`、途中の失敗は `Io` で、`Failed` になる（Task 3）。
- 「ダウンロード」の連打・二重起動 → 2本目は拒否し、1本目に影響しない（Task 4）。
- Windows でモデルを読み込んだまま「削除」→ 先にアンロードしてから消す（読み込み中のファイルは消せない）。ダウンロード中の削除は拒否（Task 4）。
- 同意していない・古い版のまま `start_local_model_download` を直接呼ぶ → Rust が拒否する。設定ファイルのインポートで同意済みにはならない（Task 1、Task 4）。
- 設定画面を閉じてから開き直す → 進行中のダウンロードの進捗がそのまま見える（Task 6）。
- 翻訳先が `pt`（PLaMo 非対応）のときにエンジンを PLaMo にする → 注記が出る（Task 6）。

---

### Task 1: 同意版の保存（`SettingsData`）

**Files:**
- Modify: `src-tauri/src/store/settings.rs`（`SettingsData` にフィールド、`SettingsStore` に getter/setter、テストの網羅リテラル）
- Modify: `src-tauri/src/store/settings_transfer.rs`（インポートで現在の値を維持）

**Interfaces:**
- Consumes: なし
- Produces: `SettingsStore::local_model_license_version(&self) -> Option<String>`、`SettingsStore::set_local_model_license_version(&self, version: &str) -> Result<()>`。`SettingsData.local_model_license_version: Option<String>`（`#[serde(default)]`、エクスポート対象外）。

- [ ] **Step 1: 失敗するテストを書く**

`src-tauri/src/store/settings.rs` の `mod tests` に追加（`new_in_memory()` と `mcp` のテストの隣）:

```rust
    #[test]
    fn old_settings_file_without_local_model_license_loads_as_not_accepted() {
        let d: SettingsData = serde_json::from_str("{}").unwrap();
        assert_eq!(d.local_model_license_version, None);
    }

    #[test]
    fn set_local_model_license_version_is_kept() {
        let s = SettingsStore::new_in_memory();
        assert_eq!(s.local_model_license_version(), None);
        s.set_local_model_license_version("2026-11-09").unwrap();
        assert_eq!(s.local_model_license_version().as_deref(), Some("2026-11-09"));
    }
```

`src-tauri/src/store/settings_transfer.rs` の `mod tests` の MCP のテストの隣に追加（`current` の作り方は `import_keeps_the_current_mcp_settings` と同じ形に合わせる）:

```rust
    #[test]
    fn import_keeps_the_current_local_model_license_version() {
        let mut current = SettingsData::default();
        current.local_model_license_version = Some("2026-11-09".into());
        // ファイル側に同意版の項目はない（エクスポートに含めない）ので、取り込みで消えず、勝手に増えもしない。
        let plan = plan_import(&export_of(&SettingsData::default()), &current).unwrap();
        assert_eq!(plan.data.local_model_license_version.as_deref(), Some("2026-11-09"));
        let plan = plan_import(&export_of(&SettingsData::default()), &SettingsData::default()).unwrap();
        assert_eq!(plan.data.local_model_license_version, None, "インポートで同意済みになってはいけない");
    }

    #[test]
    fn export_does_not_include_the_local_model_license_version() {
        let mut d = SettingsData::default();
        d.local_model_license_version = Some("2026-11-09".into());
        let json = serde_json::to_string(&build_export(&d, "0.0.0", "2026-11-14T00:00:00Z")).unwrap().to_lowercase();
        assert!(!json.contains("local_model") && !json.contains("localmodel"), "同意はエクスポートに含めない: {json}");
    }
```

`export_of` は、既存のテストが `plan_import` に渡すファイルの作り方に合わせる。既存テスト（`import_keeps_the_current_mcp_settings` の本文）を読み、同じヘルパー・同じ引数の形で書く（ヘルパーがなければ `build_export(&d, "0.0.0", "t")` を `serde_json::to_vec` して `parse_export` に通した値）。

- [ ] **Step 2: 失敗を確認**

Run: `cd src-tauri && cargo test --lib local_model_license 2>&1 | tail -15`
Expected: コンパイルエラー（`local_model_license_version` が無い）。

- [ ] **Step 3: 実装**

`SettingsData`（`mcp` フィールドの下）:

```rust
    /// 同意した PLaMo Community License の版（Issue #603）。未同意は `None`。**エクスポートには含めない**。
    /// インポートでは現在の値を維持する（同意は端末ごとの行為で、ファイルで同意済みにならないようにする）。
    #[serde(default)]
    pub(crate) local_model_license_version: Option<String>,
```

`SettingsStore`（`update_mcp` の下）:

```rust
    pub fn local_model_license_version(&self) -> Option<String> {
        self.data.lock().unwrap().local_model_license_version.clone()
    }

    pub fn set_local_model_license_version(&self, version: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.local_model_license_version = Some(version.to_string());
        self.save(&guard)
    }
```

`settings.rs` のテスト内の網羅リテラル（`mcp: Default::default(),` がある箇所）に `local_model_license_version: None,` を足す。`settings_transfer.rs` の `let data = SettingsData { ... mcp, }` に `local_model_license_version: current.local_model_license_version.clone(),` を足す。`build_export` は `ExportData` を明示的に組み立てるので、足さなければ自然に含まれない。

- [ ] **Step 4: 通ることを確認**

Run: `cd src-tauri && cargo test --lib store:: 2>&1 | tail -8`
Expected: すべて PASS（`error[E0063]: missing field` が出たら、`SettingsData { ... }` の網羅リテラルを `grep -rn "summaly_proxy_imported" src` で探して足す）。

- [ ] **Step 5: コミット**

```bash
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 add src-tauri/src/store
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit -m "feat(settings): ライセンスの同意版を保存し、エクスポートとインポートの対象外にする (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: モデルの固定値・状態の型・空き容量の判定

**Files:**
- Modify: `src-tauri/Cargo.toml`（`sha2`、`fs4`）
- Modify: `src-tauri/src/translate/model_store.rs`

**Interfaces:**
- Consumes: 既存の `MODEL_FILE_NAME` / `MODEL_SIZE_BYTES` / `model_path` / `check_ready`
- Produces（Task 3・4・6 が使う）:
  - `pub const MODEL_REVISION: &str`、`MODEL_SHA256: &str`、`MODEL_URL: &str`、`LICENSE_VERSION: &str`、`LICENSE_URL: &str`
  - `pub fn part_path(models_dir: &Path) -> PathBuf`
  - `pub(crate) fn check_ready_with(path: &Path, expected_size: u64) -> Result<()>`（既存の private 関数を `pub(crate)` に）
  - `pub enum LocalModelStatus { NotInstalled, Downloading { downloaded: u64, total: u64 }, Verifying, Ready, Failed { reason: String } }`（`Serialize, Type`、`#[serde(tag = "type", rename_all = "camelCase")]`）
  - `pub fn status_on_disk(models_dir: &Path, expected_size: u64) -> LocalModelStatus`（`Ready` か `NotInstalled`）
  - `pub const FREE_SPACE_MARGIN: u64`、`pub fn has_enough_space(available: u64, expected_size: u64, part_len: u64) -> bool`、`pub fn available_space(dir: &Path) -> Option<u64>`

- [ ] **Step 1: 依存を足し、`fs4` の API を確かめる**

Run:
```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2/src-tauri
cargo add sha2@0.10 fs4
cargo doc -p fs4 --no-deps 2>&1 | tail -3
grep -rn "pub fn available_space" ~/.cargo/registry/src/*/fs4-*/src | head
```
Expected: `fs4` の `available_space(path) -> io::Result<u64>`（同期版）が見つかる。feature（`sync` など）が必要なら `Cargo.toml` の `fs4` を `features = [...]` つきにする。関数名・戻り値の型が違う場合は、Step 3 の `available_space` ラッパー1か所だけを実際の API に合わせる（テストはこの関数を直接は踏まず、`has_enough_space` を踏む）。`sha2` は 0.10 系を使う（`format!("{:x}", Sha256::digest(..))` が使える。ロックファイルには 0.10.9 が既にある）。

- [ ] **Step 2: 失敗するテストを書く**

`model_store.rs` の `mod tests` に追加:

```rust
    #[test]
    fn the_pinned_constants_match_the_official_values() {
        assert_eq!(MODEL_REVISION.len(), 40);
        assert_eq!(MODEL_SHA256.len(), 64);
        assert!(MODEL_SHA256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        // URL はリビジョンとファイル名を含み、公式リポジトリを指す（黙って別物に変わらない）。
        assert_eq!(
            MODEL_URL,
            format!("https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/resolve/{MODEL_REVISION}/{MODEL_FILE_NAME}")
        );
        assert!(LICENSE_URL.contains(MODEL_REVISION));
        assert_eq!(LICENSE_VERSION, "2026-11-09");
    }

    #[test]
    fn part_path_sits_next_to_the_model() {
        let p = part_path(Path::new("/data/models"));
        assert_eq!(p, Path::new("/data/models").join(format!("{MODEL_FILE_NAME}.part")));
    }

    #[test]
    fn status_on_disk_is_ready_only_for_the_exact_size() {
        let dir = temp_dir();
        assert_eq!(status_on_disk(&dir, 10), LocalModelStatus::NotInstalled);
        std::fs::write(model_path(&dir), b"123456789").unwrap();
        assert_eq!(status_on_disk(&dir, 10), LocalModelStatus::NotInstalled);
        std::fs::write(model_path(&dir), b"1234567890").unwrap();
        assert_eq!(status_on_disk(&dir, 10), LocalModelStatus::Ready);
    }

    #[test]
    fn free_space_needs_the_remaining_bytes_plus_a_margin() {
        let need = 1000 - 400 + FREE_SPACE_MARGIN;
        assert!(has_enough_space(need, 1000, 400));
        assert!(!has_enough_space(need - 1, 1000, 400));
        // `.part` が期待サイズ以上なら、残りは 0（検証だけ）。余裕分は要求しない。
        assert!(has_enough_space(0, 1000, 1000));
    }

    #[test]
    fn status_serializes_with_a_camel_case_tag() {
        let j = serde_json::to_value(LocalModelStatus::Downloading { downloaded: 1, total: 2 }).unwrap();
        assert_eq!(j, serde_json::json!({"type": "downloading", "downloaded": 1, "total": 2}));
        let j = serde_json::to_value(LocalModelStatus::NotInstalled).unwrap();
        assert_eq!(j, serde_json::json!({"type": "notInstalled"}));
    }
```

`LocalModelStatus` は `PartialEq, Debug` も導出する（上のテストが `assert_eq!` する）。

- [ ] **Step 3: 実装して通す**

`model_store.rs` の先頭の `use` を `use serde::Serialize; use specta::Type;` を足して、定数・型・関数を追加（`check_ready_with` は `pub(crate)` にする）:

```rust
/// PFN 側の更新で重みが黙って変わらないよう、リビジョン（コミット）を固定する。更新はアプリのリリースで追従する。
/// 2026-11-14 に HF API（`/api/models/pfnet/plamo-3-translate-0.8b-GGUF?blobs=true`）の `sha` と `lfs.oid` から取得した。
pub const MODEL_REVISION: &str = "1cf96ae02133958a20074bfdfab76517bb737d00";
pub const MODEL_SHA256: &str = "70891ec70dc7ab4205745a38307867c2881b99239f20327927633abfe4bc9ec6";
pub const MODEL_URL: &str = "https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/resolve/1cf96ae02133958a20074bfdfab76517bb737d00/plamo-3-translate-0.8b-Q4_K_M.gguf";
/// 同意を求めるライセンス（PLaMo Community License、2026-11-09 改定版）の版と、全文（日本語）の写し。
pub const LICENSE_VERSION: &str = "2026-11-09";
pub const LICENSE_URL: &str =
    "https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/blob/1cf96ae02133958a20074bfdfab76517bb737d00/LICENSE/ja";
/// 空き容量に足す余裕（`.part` の書き込みのあとにリネームや他のアプリの書き込みがあっても足りるように）。
pub const FREE_SPACE_MARGIN: u64 = 256 * 1024 * 1024;
```

`MODEL_SHA256` の値は 2026-11-14 に HF API から取得した 64 文字（小文字）。実装時に、次のコマンドの出力と一致することを確かめてから定数にする: `curl -s "https://huggingface.co/api/models/pfnet/plamo-3-translate-0.8b-GGUF?blobs=true" | python3 -c "import json,sys; print([s['lfs']['sha256'] for s in json.load(sys.stdin)['siblings'] if s['rfilename'].endswith('Q4_K_M.gguf')][0])"`。`the_pinned_constants_match_the_official_values`（64 文字・小文字 16 進）が通ること、Task 4 Step 7 の実ダウンロードのテストで検証が通ることで、値の正しさを確認する。

```rust
pub fn part_path(models_dir: &Path) -> PathBuf {
    models_dir.join(format!("{MODEL_FILE_NAME}.part"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LocalModelStatus {
    NotInstalled,
    Downloading {
        #[specta(type = specta_typescript::Number)]
        downloaded: u64,
        #[specta(type = specta_typescript::Number)]
        total: u64,
    },
    Verifying,
    Ready,
    Failed { reason: String },
}

/// ディスク上の状態。ダウンロード中・失敗は、実行中の `LocalModels` が持つので、ここでは `Ready` か `NotInstalled`。
pub fn status_on_disk(models_dir: &Path, expected_size: u64) -> LocalModelStatus {
    match check_ready_with(&model_path(models_dir), expected_size) {
        Ok(()) => LocalModelStatus::Ready,
        Err(_) => LocalModelStatus::NotInstalled,
    }
}

/// 残りのバイト数 + 余裕が空いているか。`.part` が期待サイズ以上なら、検証だけなので余裕は要らない。
pub fn has_enough_space(available: u64, expected_size: u64, part_len: u64) -> bool {
    let remaining = expected_size.saturating_sub(part_len);
    remaining == 0 || available >= remaining + FREE_SPACE_MARGIN
}

/// `dir` があるボリュームの空き容量。調べられなければ `None`（その場合は確認を省く）。
pub fn available_space(dir: &Path) -> Option<u64> {
    fs4::available_space(dir).ok()
}
```

Run: `cd src-tauri && cargo test --lib translate::model_store 2>&1 | tail -12`
Expected: PASS。

- [ ] **Step 4: コミット**

```bash
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/translate/model_store.rs
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit -m "feat(translate): モデルの固定値・状態の型・空き容量の判定を追加する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: ダウンロード処理（Range 再開・SHA-256 検証）

**Files:**
- Create: `src-tauri/src/translate/download.rs`
- Modify: `src-tauri/src/translate/mod.rs`（`pub mod download;`）

**Interfaces:**
- Consumes: Task 2 の `part_path`、`has_enough_space`、`available_space`、`MODEL_URL`、`MODEL_SHA256`、`MODEL_SIZE_BYTES`
- Produces:
  - `pub struct ModelSpec { pub url: String, pub size: u64, pub sha256: String }` と `ModelSpec::official()`
  - `pub enum Phase { Downloading { downloaded: u64, total: u64 }, Verifying }`
  - `pub enum DownloadError { Cancelled, Http(u16), Network(String), HashMismatch, SizeMismatch, NoSpace, Io(String) }` と `DownloadError::reason(&self) -> String`（利用者向けの日本語）
  - `pub async fn download(client: &reqwest::Client, spec: &ModelSpec, part: &Path, dest: &Path, cancel: &AtomicBool, free_space: &(dyn Fn(&Path) -> Option<u64> + Sync), on_phase: &mut (dyn FnMut(Phase) + Send)) -> Result<(), DownloadError>`

- [ ] **Step 1: 失敗するテストを書く**

`download.rs` を作り、まずテストと型の骨組みだけを置く。テストは wiremock で、`Range` を解釈する自前の `Respond` を使う（wiremock は Range を自動処理しない）:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tsumugi_dl_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }

    fn body(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    /// `Range: bytes=N-` を解釈して 206 を返す。`honor_range = false` なら無視して常に 200 全体を返す。
    /// 受け取った Range ヘッダを記録する。
    struct Server {
        body: Vec<u8>,
        honor_range: bool,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
        hits: Arc<AtomicUsize>,
    }
    impl Respond for Server {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            self.hits.fetch_add(1, Ordering::SeqCst);
            let range = req.headers.get("range").map(|v| v.to_str().unwrap().to_string());
            self.ranges.lock().unwrap().push(range.clone());
            match (&range, self.honor_range) {
                (Some(r), true) => {
                    let start: usize = r.trim_start_matches("bytes=").trim_end_matches('-').parse().unwrap();
                    if start >= self.body.len() {
                        return ResponseTemplate::new(416);
                    }
                    ResponseTemplate::new(206)
                        .insert_header("content-range", format!("bytes {}-{}/{}", start, self.body.len() - 1, self.body.len()))
                        .set_body_bytes(self.body[start..].to_vec())
                }
                _ => ResponseTemplate::new(200).set_body_bytes(self.body.clone()),
            }
        }
    }

    struct Fixture {
        server: MockServer,
        dir: PathBuf,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
        hits: Arc<AtomicUsize>,
        spec: ModelSpec,
    }

    async fn fixture(data: Vec<u8>, honor_range: bool, expected_sha: Option<String>) -> Fixture {
        let server = MockServer::start().await;
        let ranges = Arc::new(Mutex::new(vec![]));
        let hits = Arc::new(AtomicUsize::new(0));
        let spec = ModelSpec {
            url: format!("{}/m.gguf", server.uri()),
            size: data.len() as u64,
            sha256: expected_sha.unwrap_or_else(|| sha(&data)),
        };
        Mock::given(method("GET"))
            .respond_with(Server { body: data, honor_range, ranges: ranges.clone(), hits: hits.clone() })
            .mount(&server)
            .await;
        Fixture { server, dir: temp_dir(), ranges, hits, spec }
    }

    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    async fn run(f: &Fixture, cancel: &AtomicBool) -> (Result<(), DownloadError>, Vec<Phase>) {
        let mut phases = vec![];
        let part = f.dir.join("m.gguf.part");
        let dest = f.dir.join("m.gguf");
        let r = download(&client(), &f.spec, &part, &dest, cancel, &|_| Some(u64::MAX), &mut |p| phases.push(p)).await;
        (r, phases)
    }

    #[tokio::test]
    async fn downloads_verifies_and_renames() {
        let data = body(300_000);
        let f = fixture(data.clone(), true, None).await;
        let (r, phases) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data);
        assert!(!f.dir.join("m.gguf.part").exists(), ".part は残さない");
        assert!(matches!(phases.last(), Some(Phase::Verifying)));
        // 進捗は単調に増え、最後に total へ届く。
        let downloaded: Vec<u64> = phases.iter().filter_map(|p| match p { Phase::Downloading { downloaded, .. } => Some(*downloaded), _ => None }).collect();
        assert!(downloaded.windows(2).all(|w| w[0] <= w[1]), "{downloaded:?}");
        assert_eq!(*downloaded.last().unwrap(), data.len() as u64);
    }

    #[tokio::test]
    async fn resumes_an_existing_part_with_a_range_request() {
        let data = body(300_000);
        let f = fixture(data.clone(), true, None).await;
        std::fs::write(f.dir.join("m.gguf.part"), &data[..100_000]).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data, "継ぎ足した結果がハッシュ検証を通る");
        assert_eq!(f.ranges.lock().unwrap().as_slice(), &[Some("bytes=100000-".to_string())]);
    }

    #[tokio::test]
    async fn restarts_from_zero_when_the_server_ignores_range() {
        let data = body(200_000);
        let f = fixture(data.clone(), false, None).await;
        std::fs::write(f.dir.join("m.gguf.part"), &data[..50_000]).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data, "200 の全体を、.part に継ぎ足さず上書きする");
    }

    #[tokio::test]
    async fn a_416_discards_the_part_and_restarts_once() {
        let data = body(100_000);
        let f = fixture(data.clone(), true, None).await;
        // 期待サイズより大きい .part は、リクエスト前に捨てて最初から取り直す。
        std::fs::write(f.dir.join("m.gguf.part"), body(150_000)).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data);
        assert_eq!(f.ranges.lock().unwrap().as_slice(), &[None], "Range なしで取り直す");
    }

    #[tokio::test]
    async fn a_complete_part_is_verified_without_any_request() {
        let data = body(80_000);
        let f = fixture(data.clone(), true, None).await;
        std::fs::write(f.dir.join("m.gguf.part"), &data).unwrap();
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        r.unwrap();
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
        assert_eq!(std::fs::read(f.dir.join("m.gguf")).unwrap(), data);
    }

    #[tokio::test]
    async fn a_hash_mismatch_discards_the_part_and_fails() {
        let data = body(80_000);
        let f = fixture(data, true, Some(sha(b"something else"))).await;
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        assert_eq!(r, Err(DownloadError::HashMismatch));
        assert!(!f.dir.join("m.gguf.part").exists(), "壊れた .part を再開に使わせない");
        assert!(!f.dir.join("m.gguf").exists(), "検証に通らないファイルをモデルとして置かない");
    }

    #[tokio::test]
    async fn an_http_error_fails_without_touching_the_destination() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&server).await;
        let dir = temp_dir();
        let spec = ModelSpec { url: format!("{}/m.gguf", server.uri()), size: 10, sha256: sha(b"x") };
        let r = download(&client(), &spec, &dir.join("p"), &dir.join("d"), &AtomicBool::new(false), &|_| None, &mut |_| {}).await;
        assert_eq!(r, Err(DownloadError::Http(500)));
        assert!(!dir.join("d").exists());
    }

    #[tokio::test]
    async fn not_enough_space_fails_before_any_request() {
        let f = fixture(body(1000), true, None).await;
        let mut phases = vec![];
        let r = download(&client(), &f.spec, &f.dir.join("p"), &f.dir.join("d"), &AtomicBool::new(false), &|_| Some(0), &mut |p| phases.push(p)).await;
        assert_eq!(r, Err(DownloadError::NoSpace));
        assert_eq!(f.hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cancelling_keeps_the_part_for_resuming() {
        let f = fixture(body(300_000), true, None).await;
        let cancel = AtomicBool::new(true);
        let (r, _) = run(&f, &cancel).await;
        assert_eq!(r, Err(DownloadError::Cancelled));
        assert!(!f.dir.join("m.gguf").exists());
        // 再開できる（R1）。次の実行は完走する。
        cancel.store(false, Ordering::SeqCst);
        let (r, _) = run(&f, &cancel).await;
        r.unwrap();
    }

    #[tokio::test]
    async fn a_body_shorter_than_expected_is_a_network_error_and_keeps_the_part() {
        // サイズが合わない（途中で切れた）ときは、再開できるよう .part を残す。
        let mut f = fixture(body(1000), true, None).await;
        f.spec.size = 2000; // サーバーは 1000 バイトしか返さない
        let (r, _) = run(&f, &AtomicBool::new(false)).await;
        assert!(matches!(r, Err(DownloadError::Network(_))), "{r:?}");
        assert_eq!(std::fs::metadata(f.dir.join("m.gguf.part")).unwrap().len(), 1000);
    }

    #[test]
    fn failure_reasons_are_japanese_and_leak_nothing() {
        for e in [
            DownloadError::Cancelled,
            DownloadError::Http(503),
            DownloadError::Network("connection reset https://secret.example/x?token=1".into()),
            DownloadError::HashMismatch,
            DownloadError::SizeMismatch,
            DownloadError::NoSpace,
            DownloadError::Io("/home/me/.local/share/x: disk full".into()),
        ] {
            let r = e.reason();
            assert!(!r.is_empty());
            assert!(!r.contains("secret.example") && !r.contains("/home/"), "{r}");
        }
        assert!(DownloadError::Http(503).reason().contains("503"));
    }
}
```

`DownloadError` は `Debug, PartialEq` を導出する（`assert_eq!` で比較）。`Phase` は `Debug, Clone, PartialEq`。

- [ ] **Step 2: 失敗を確認**

骨組み（`ModelSpec`・`Phase`・`DownloadError`・`download` を `todo!()` で）と `mod.rs` の `pub mod download;` を置き、先頭に `use super::model_store::{...}; use std::path::{Path, PathBuf}; use std::sync::atomic::{AtomicBool, Ordering};` を書く。

Run: `cd src-tauri && cargo test --lib translate::download 2>&1 | tail -15`
Expected: コンパイルが通り、`not yet implemented` で FAIL。

- [ ] **Step 3: 実装**

```rust
//! モデルのダウンロード（Range 再開・SHA-256 検証、Issue #603）。`LocalModels` から使う。
//! 本文は扱わない。エラーの理由（`reason`）は URL・パスを含めない。

use super::model_store::{has_enough_space, MODEL_SHA256, MODEL_SIZE_BYTES, MODEL_URL};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// 1チャンクを待つ上限。接続が黙って止まったときに、いつまでも進捗が止まったままにしない。
const CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

impl ModelSpec {
    pub fn official() -> Self {
        Self { url: MODEL_URL.into(), size: MODEL_SIZE_BYTES, sha256: MODEL_SHA256.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Downloading { downloaded: u64, total: u64 },
    Verifying,
}

#[derive(Debug, PartialEq)]
pub enum DownloadError {
    Cancelled,
    Http(u16),
    Network(String),
    HashMismatch,
    SizeMismatch,
    NoSpace,
    Io(String),
}

impl DownloadError {
    /// 利用者向けの理由。詳細（URL・パス）は含めない。
    pub fn reason(&self) -> String {
        match self {
            Self::Cancelled => "キャンセルしました".into(),
            Self::Http(s) => format!("サーバーがエラーを返しました（HTTP {s}）。時間をおいて再度お試しください"),
            Self::Network(_) => "通信が途切れました。再度ダウンロードすると途中から再開します".into(),
            Self::HashMismatch => "ダウンロードしたファイルの検証に失敗しました。もう一度お試しください".into(),
            Self::SizeMismatch => "ダウンロードしたファイルのサイズが想定と違います。もう一度お試しください".into(),
            Self::NoSpace => "ディスクの空き容量が足りません".into(),
            Self::Io(_) => "ファイルの書き込みに失敗しました（ディスクの空き容量を確認してください）".into(),
        }
    }
}

fn io_err(e: std::io::Error) -> DownloadError {
    DownloadError::Io(e.to_string())
}

fn net_err(e: reqwest::Error) -> DownloadError {
    // 詳細はログにも出さない（URL に署名つきのクエリが付くことがある）。種別だけ持つ。
    let kind = if e.is_timeout() { "timeout" } else if e.is_connect() { "connect" } else { "request" };
    DownloadError::Network(kind.into())
}

/// `Content-Range: bytes 100-999/1000` の開始位置。
fn content_range_start(v: &str) -> Option<u64> {
    v.strip_prefix("bytes ")?.split('-').next()?.parse().ok()
}

pub async fn download(
    client: &reqwest::Client,
    spec: &ModelSpec,
    part: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    free_space: &(dyn Fn(&Path) -> Option<u64> + Sync),
    on_phase: &mut (dyn FnMut(Phase) + Send),
) -> Result<(), DownloadError> {
    let mut restarted = false;
    loop {
        let mut have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if have > spec.size {
            std::fs::remove_file(part).map_err(io_err)?;
            have = 0;
        }
        if have < spec.size {
            if cancel.load(Ordering::SeqCst) {
                return Err(DownloadError::Cancelled);
            }
            let dir = part.parent().unwrap_or(Path::new("."));
            if let Some(avail) = free_space(dir) {
                if !has_enough_space(avail, spec.size, have) {
                    return Err(DownloadError::NoSpace);
                }
            }
            let mut req = client.get(&spec.url);
            if have > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
            }
            let resp = req.send().await.map_err(net_err)?;
            let append = match resp.status().as_u16() {
                206 => {
                    let start = resp
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                        .and_then(content_range_start);
                    if start != Some(have) {
                        // 想定した位置と違う続きは、継ぎ足せない。捨てて最初から。
                        std::fs::remove_file(part).ok();
                        if restarted {
                            return Err(DownloadError::Http(206));
                        }
                        restarted = true;
                        continue;
                    }
                    true
                }
                // Range を無視して全体を返した。続きではなく、最初から書き直す。
                200 => false,
                416 => {
                    std::fs::remove_file(part).ok();
                    if restarted {
                        return Err(DownloadError::Http(416));
                    }
                    restarted = true;
                    continue;
                }
                s => return Err(DownloadError::Http(s)),
            };
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(part)
                .map_err(io_err)?;
            let mut downloaded = if append { have } else { 0 };
            on_phase(Phase::Downloading { downloaded, total: spec.size });
            let mut stream = resp.bytes_stream();
            loop {
                if cancel.load(Ordering::SeqCst) {
                    return Err(DownloadError::Cancelled);
                }
                match tokio::time::timeout(CHUNK_TIMEOUT, stream.next()).await {
                    Err(_) => return Err(DownloadError::Network("timeout".into())),
                    Ok(None) => break,
                    Ok(Some(Err(e))) => return Err(net_err(e)),
                    Ok(Some(Ok(chunk))) => {
                        file.write_all(&chunk).map_err(io_err)?;
                        downloaded += chunk.len() as u64;
                        on_phase(Phase::Downloading { downloaded, total: spec.size });
                    }
                }
            }
            file.flush().map_err(io_err)?;
        }
        break;
    }

    let len = std::fs::metadata(part).map_err(io_err)?.len();
    if len < spec.size {
        // 途中で切れた。再開できるよう .part は残す。
        return Err(DownloadError::Network("incomplete".into()));
    }
    if len > spec.size {
        std::fs::remove_file(part).ok();
        return Err(DownloadError::SizeMismatch);
    }
    on_phase(Phase::Verifying);
    let part_buf = part.to_path_buf();
    let digest = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        let mut f = std::fs::File::open(part_buf)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(format!("{:x}", h.finalize()))
    })
    .await
    .map_err(|e| DownloadError::Io(e.to_string()))?
    .map_err(io_err)?;
    if !digest.eq_ignore_ascii_case(&spec.sha256) {
        std::fs::remove_file(part).ok();
        return Err(DownloadError::HashMismatch);
    }
    std::fs::rename(part, dest).map_err(io_err)?;
    Ok(())
}
```

注意: `available_space` は Task 4 の呼び出し側が `free_space` 引数として渡す。`futures-util` は `StreamExt` のために `Cargo.toml` の既存依存（features: `sink`, `std`）で足りる。`reqwest` の `bytes_stream` は `stream` feature が必要なので、`Cargo.toml` の `reqwest` の features に `"stream"` を足す。

Run: `cd src-tauri && cargo test --lib translate::download 2>&1 | tail -20`
Expected: 全 PASS。`cancelling_keeps_the_part...` は `cancel = true` のとき最初のリクエスト前に `Cancelled` を返し、`.part` が無いまま2回目で完走する。途中キャンセル後の `.part` 保持は、Task 4 のマネージャーのテストで（実際に途中で止めて）確認する。

- [ ] **Step 4: コミット**

```bash
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/translate
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit -m "feat(translate): Range再開とSHA-256検証つきのモデルのダウンロード処理を追加する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `LocalModels`・コマンド・Event・配線

**Files:**
- Create: `src-tauri/src/translate/local_models.rs`
- Create: `src-tauri/src/commands/local_model.rs`
- Modify: `src-tauri/src/translate/mod.rs`（`pub mod local_models;`、`pub use local_models::LocalModels;`）
- Modify: `src-tauri/src/translate/manager.rs`（`LocalTranslate::unload`）
- Modify: `src-tauri/src/events.rs`（`LocalModelStatusChanged`）
- Modify: `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`（登録・配線）、`src-tauri/src/state.rs`（`local_models`、`with_local_models`）
- Modify: `frontend/src/bindings/tauri.gen.ts`（`cargo test` で再生成）

**Interfaces:**
- Consumes: Task 1 の `local_model_license_version` 系、Task 2・3 の型と `download`
- Produces:
  - `LocalModels::{unconfigured, new(models_dir, http, translator), status, start, cancel, delete}`。`start(self: &Arc<Self>, notify: Notify) -> Result<()>`、`type Notify = Arc<dyn Fn(LocalModelStatus) + Send + Sync>`、`async fn delete(&self) -> Result<()>`
  - `LocalTranslate::unload(&self)`（async）
  - コマンド: `get_local_model_status() -> LocalModelInfo`、`start_local_model_download(accepted_license_version: String)`、`cancel_local_model_download()`、`delete_local_model()`、`is_translate_lang_supported(lang: String) -> bool`
  - `LocalModelInfo { status: LocalModelStatus, size_bytes: u64, license_version: String, license_accepted: bool, supported: bool }`
  - Event `LocalModelStatusChanged { status: LocalModelStatus }`（TS では `events.localModelStatusChanged`）

- [ ] **Step 1: `LocalTranslate::unload` のテストと実装**

`manager.rs` の `mod tests`（`Fake` を使う既存テストの形に合わせる）に追加:

```rust
    #[tokio::test]
    async fn unload_releases_the_model_and_the_next_translate_loads_again() {
        let (lt, loads) = counting(); // 既存のテストで「ロードが一度だけ」を数えているヘルパーと同じ形のものを使う
        lt.translate("a", "en").await.unwrap();
        assert!(lt.is_loaded().await);
        lt.unload().await;
        assert!(!lt.is_loaded().await);
        lt.translate("b", "en").await.unwrap();
        assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
```

`counting()` は、既存の `LocalTranslate` のテスト（遅延ロードが一度だけであることを確かめるテスト）が使っているヘルパー・構築方法を読んで、同じ作りで書く（ヘルパーが無ければ、`Loader` が `AtomicUsize` を増やして `Fake` を返す形にする）。

実装（`unload_if_idle` の下）:

```rust
    /// モデルを解放する。ファイルを消す前に呼ぶ（Windows は読み込み中のファイルを消せない）。
    /// 翻訳中は `translate` がロックを持っているので、終わるまで待つ。
    pub async fn unload(&self) {
        *self.loaded.lock().await = None;
    }
```

Run: `cd src-tauri && cargo test --lib translate::manager 2>&1 | tail -8` → PASS。

- [ ] **Step 2: `LocalModels` の失敗するテストを書く**

`local_models.rs` に、テストとインターフェースの骨組みを書く。テストの肝は次のとおり（Task 3 の `Server`/`fixture` と同じ wiremock の作りをテスト内に持つ。モジュールをまたいで共有するより、短い複製のほうが読みやすいので複製してよい）:

```rust
#[cfg(test)]
mod tests {
    // ...Task 3 の body / sha / temp_dir / Server(Respond) と同じヘルパー...

    fn models(spec: ModelSpec, dir: PathBuf) -> (Arc<LocalModels>, Arc<Mutex<Vec<LocalModelStatus>>>, Notify) {
        let translator = Arc::new(LocalTranslate::unconfigured());
        let m = Arc::new(LocalModels::with_spec(dir, reqwest::Client::new(), translator, spec));
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let notify: Notify = Arc::new(move |s| ev.lock().unwrap().push(s));
        (m, events, notify)
    }

    async fn wait_idle(m: &LocalModels) {
        // バックグラウンドのタスクが終わる（running が空になる）まで待つ。
        tokio::time::timeout(Duration::from_secs(20), async { while m.is_running() { tokio::time::sleep(Duration::from_millis(10)).await } })
            .await
            .expect("ダウンロードが終わらない");
    }

    #[tokio::test]
    async fn a_successful_download_ends_ready_and_reports_progress_then_ready() {
        // 小さな body で ModelSpec を作り、start → wait_idle。
        // - m.status() == Ready、ファイルが model_path にある
        // - events の最後は Ready、途中に Downloading が1つ以上、Verifying が Ready の前にある
        // - Downloading の downloaded は単調増加
    }

    #[tokio::test]
    async fn a_second_start_while_running_is_rejected_and_does_not_disturb_the_first() {
        // サーバーを遅延させる（ResponseTemplate::set_delay(300ms)）。start を2回続けて呼ぶ。
        // 2回目は Err(Error::Invalid("すでにダウンロード中です"))。1回目は完走して Ready。
    }

    #[tokio::test]
    async fn start_is_rejected_when_the_model_is_already_ready() {
        // model_path に spec.size ちょうどのファイルを置く → Err(Invalid("モデルは取得済みです"))、通信しない
    }

    #[tokio::test]
    async fn cancel_stops_a_running_download_keeps_the_part_and_reports_not_installed() {
        // 大きめの body をゆっくり返す（set_delay）か、cancel を start の直後に立てる。
        // cancel() → wait_idle → status() == NotInstalled、.part は残る、model_path は無い。
        // もう一度 start → 完走して Ready（再開できる）。
    }

    #[tokio::test]
    async fn a_failed_download_keeps_the_failed_status_until_the_next_start() {
        // sha256 を違う値にして start → wait_idle → status() は Failed { reason } で、reason は日本語（HashMismatch の文）。
        // events の最後も同じ Failed。次に正しい spec... ではなく、同じ m で start し直すと Downloading に戻る（拒否されない）。
    }

    #[tokio::test]
    async fn delete_unloads_the_model_and_removes_the_file_and_the_part() {
        // Ready のファイルと .part を置く。LocalTranslate に fake を読み込ませる（translate を1回呼ぶ）→ is_loaded()==true。
        // delete().await → is_loaded()==false、model_path も part も無い、status()==NotInstalled。
    }

    #[tokio::test]
    async fn delete_is_rejected_while_downloading() {
        // 遅い download の最中に delete → Err(Invalid)。ダウンロードは続いて完走する。
    }

    #[tokio::test]
    async fn the_unconfigured_instance_reports_unsupported_on_start() {
        // LocalModels::unconfigured().start(..) → Err(Invalid(ERR_UNSUPPORTED))、status() == NotInstalled
    }
}
```

各テストの本文は、上のコメントの検証項目をそのまま `assert!` にして書き切る（`LocalModels::with_spec`、`is_running`、`Notify` の型は Step 3 の実装と同じシグネチャ）。「遅い」サーバーは `Server` の `ResponseTemplate` に `.set_delay(Duration::from_millis(300))` を付けた版を使う。

- [ ] **Step 3: 実装**

```rust
//! モデルの取得・状態・削除の管理（Issue #603）。同時に走るダウンロードは1つ。進捗は `Notify` で通知する
//! （Tauri の Event への変換は `commands/local_model.rs`。ここは Tauri に依存しない）。

use super::download::{download, DownloadError, ModelSpec, Phase};
use super::manager::LocalTranslate;
use super::model_store::{
    available_space, model_path, part_path, status_on_disk, LocalModelStatus,
};
use super::{invalid, ERR_UNSUPPORTED};
use crate::error::{Error, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub type Notify = Arc<dyn Fn(LocalModelStatus) + Send + Sync>;

/// 進捗の通知の最小間隔（Verifying と終了状態は間引かない）。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

struct Inner {
    /// 実行中のダウンロードのキャンセルフラグ。`Some` の間は新しい開始・削除を拒否する。
    running: Option<Arc<AtomicBool>>,
    /// ダウンロード中・検証中・失敗の状態。`None` ならディスクの状態（Ready / NotInstalled）。
    transient: Option<LocalModelStatus>,
}

pub struct LocalModels {
    /// `None` は、ローカル翻訳に対応しない構成（`LocalTranslate::unconfigured` と同じ）。
    models_dir: Option<PathBuf>,
    http: reqwest::Client,
    translator: Arc<LocalTranslate>,
    spec: ModelSpec,
    inner: Mutex<Inner>,
}

impl LocalModels {
    pub fn unconfigured() -> Self {
        Self {
            models_dir: None,
            http: reqwest::Client::new(),
            translator: Arc::new(LocalTranslate::unconfigured()),
            spec: ModelSpec::official(),
            inner: Mutex::new(Inner { running: None, transient: None }),
        }
    }

    pub fn new(models_dir: PathBuf, http: reqwest::Client, translator: Arc<LocalTranslate>) -> Self {
        Self::with_spec(models_dir, http, translator, ModelSpec::official())
    }

    // with_spec・size_bytes・is_running・status・start・run・finish・cancel・delete は、下のブロックのとおり。
}
```

**設計の確定:** `LocalModels::new` / `with_spec` は常に `Some(models_dir)` を持つ（非対応ビルドでの拒否は持たない）。非対応かどうかは `translate::is_supported()` を**コマンド層**で見る: `get_local_model_status` は `supported: is_supported()` を返し、`start_local_model_download` は `!is_supported()` なら `ERR_UNSUPPORTED` を返す（Step 5）。`LocalModels::unconfigured()` だけが `models_dir: None` で、`start` は `ERR_UNSUPPORTED`、`status` は `NotInstalled`、`delete` は何もしない。`impl LocalModels` の残りは次のとおり（`with_spec` から `delete` まで）:

```rust
impl LocalModels {
    pub(crate) fn with_spec(models_dir: PathBuf, http: reqwest::Client, translator: Arc<LocalTranslate>, spec: ModelSpec) -> Self {
        Self { models_dir: Some(models_dir), http, translator, spec, inner: Mutex::new(Inner { running: None, transient: None }) }
    }

    pub fn size_bytes(&self) -> u64 {
        self.spec.size
    }

    pub(crate) fn is_running(&self) -> bool {
        self.inner.lock().unwrap().running.is_some()
    }

    pub fn status(&self) -> LocalModelStatus {
        let Some(dir) = &self.models_dir else { return LocalModelStatus::NotInstalled };
        let g = self.inner.lock().unwrap();
        match &g.transient {
            Some(t) => t.clone(),
            None => status_on_disk(dir, self.spec.size),
        }
    }

    /// ダウンロードを始める。同時に走るのは1つだけ。取得済みなら拒否する。
    pub fn start(self: &Arc<Self>, notify: Notify) -> Result<()> {
        let Some(dir) = self.models_dir.clone() else { return Err(invalid(ERR_UNSUPPORTED)) };
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut g = self.inner.lock().unwrap();
            if g.running.is_some() {
                return Err(Error::Invalid("すでにダウンロード中です".into()));
            }
            if status_on_disk(&dir, self.spec.size) == LocalModelStatus::Ready {
                return Err(Error::Invalid("モデルは取得済みです".into()));
            }
            g.running = Some(cancel.clone());
            g.transient = Some(LocalModelStatus::Downloading { downloaded: 0, total: self.spec.size });
        }
        notify(LocalModelStatus::Downloading { downloaded: 0, total: self.spec.size });
        let this = self.clone();
        tokio::spawn(async move { this.run(dir, cancel, notify).await });
        Ok(())
    }

    async fn run(self: Arc<Self>, dir: PathBuf, cancel: Arc<AtomicBool>, notify: Notify) {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.finish(Err(DownloadError::Io(e.to_string())), &notify);
            return;
        }
        let (part, dest) = (part_path(&dir), model_path(&dir));
        let mut last = Instant::now() - PROGRESS_INTERVAL;
        let this = self.clone();
        let n = notify.clone();
        let r = download(&self.http, &self.spec, &part, &dest, &cancel, &available_space, &mut |p| {
            let status = match p {
                Phase::Downloading { downloaded, total } => LocalModelStatus::Downloading { downloaded, total },
                Phase::Verifying => LocalModelStatus::Verifying,
            };
            // 状態は常に更新し、通知だけ間引く（Verifying は必ず通知する）。
            this.inner.lock().unwrap().transient = Some(status.clone());
            if matches!(status, LocalModelStatus::Verifying) || last.elapsed() >= PROGRESS_INTERVAL {
                last = Instant::now();
                n(status);
            }
        })
        .await;
        self.finish(r, &notify);
    }

    fn finish(&self, r: std::result::Result<(), DownloadError>, notify: &Notify) {
        let final_status = {
            let mut g = self.inner.lock().unwrap();
            g.running = None;
            match r {
                Ok(()) => {
                    g.transient = None;
                    LocalModelStatus::Ready
                }
                // キャンセルは失敗ではない。.part は残る（R1）。ディスクの状態（NotInstalled）に戻す。
                Err(DownloadError::Cancelled) => {
                    g.transient = None;
                    LocalModelStatus::NotInstalled
                }
                Err(e) => {
                    let f = LocalModelStatus::Failed { reason: e.reason() };
                    g.transient = Some(f.clone());
                    f
                }
            }
        };
        notify(final_status);
    }

    /// 実行中のダウンロードを止める。`.part` は残す。
    pub fn cancel(&self) {
        if let Some(c) = &self.inner.lock().unwrap().running {
            c.store(true, Ordering::SeqCst);
        }
    }

    /// モデルを削除する（`.part` も）。読み込み中なら先にアンロードする。ダウンロード中は拒否する。
    pub async fn delete(&self) -> Result<()> {
        let Some(dir) = &self.models_dir else { return Ok(()) };
        if self.is_running() {
            return Err(Error::Invalid("ダウンロード中は削除できません".into()));
        }
        self.translator.unload().await;
        for p in [model_path(dir), part_path(dir)] {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::Invalid(format!("モデルを削除できませんでした: {}", e.kind()))),
            }
        }
        self.inner.lock().unwrap().transient = None;
        Ok(())
    }
}
```

`mod.rs` に `pub mod download; pub mod local_models; pub use local_models::LocalModels;` を足す。`Error::Invalid` を使うので `crate::error::Error` は `use` 済み。

Run: `cd src-tauri && cargo test --lib translate::local_models 2>&1 | tail -20` → 全 PASS。`cancel_stops...` が不安定なら、サーバーに `set_delay` を入れて、`start` のあとに `Downloading` の通知を受けてから `cancel()` する形にする（`sleep` で時間を合わせない）。

- [ ] **Step 4: Event・コマンド・配線のテストを先に書く**

`commands/local_model.rs` に、`translate_note_core` と同じ作り（Tauri を使わず `&AppState` を受ける `_core` 関数を、コマンドが薄く呼ぶ）で書く。テスト:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_wrong_license_version_is_rejected_and_nothing_is_recorded() {
        let s = AppState::new_for_test(); // 既存のテストが AppState を作る方法に合わせる（`note.rs` のテストを参照）
        let r = start_core(&s, "1999-01-01", noop_notify()).await;
        assert!(matches!(r, Err(Error::Invalid(ref m)) if m.contains("ライセンス")), "{r:?}");
        assert_eq!(s.settings.local_model_license_version(), None);
    }

    #[tokio::test]
    async fn an_empty_license_version_is_rejected() {
        let s = AppState::new_for_test();
        assert!(start_core(&s, "", noop_notify()).await.is_err());
    }

    #[tokio::test]
    async fn the_right_version_on_an_unconfigured_state_reports_unsupported_and_is_not_recorded() {
        let s = AppState::new_for_test();
        let r = start_core(&s, LICENSE_VERSION, noop_notify()).await;
        assert!(matches!(r, Err(Error::Invalid(ref m)) if m == ERR_UNSUPPORTED), "{r:?}");
        assert_eq!(s.settings.local_model_license_version(), None, "R3: 開始できなかったら同意を記録しない");
    }

    #[tokio::test]
    async fn the_right_version_starts_and_records_the_acceptance() {
        // LocalModels::with_spec（wiremock の小さな body）を state.local_models に差し込んで start_core。
        // 戻りは Ok、settings の同意版は LICENSE_VERSION、info.license_accepted == true。
    }

    #[test]
    fn info_reports_supported_size_license_and_acceptance() {
        let s = AppState::new_for_test();
        let i = info(&s);
        assert_eq!(i.license_version, LICENSE_VERSION);
        assert!(!i.license_accepted);
        assert_eq!(i.size_bytes, MODEL_SIZE_BYTES);
        s.settings.set_local_model_license_version("1999-01-01").unwrap();
        assert!(!info(&s).license_accepted, "古い版の同意は、現行の同意ではない");
        s.settings.set_local_model_license_version(LICENSE_VERSION).unwrap();
        assert!(info(&s).license_accepted);
    }

    #[test]
    fn lang_support_follows_the_plamo_table() {
        assert!(is_lang_supported("ja"));
        assert!(is_lang_supported("en-US"));
        assert!(is_lang_supported("zh-TW"));
        assert!(!is_lang_supported("pt"));
        assert!(!is_lang_supported(""));
    }
}
```

`AppState::new_for_test()` は存在しないかもしれない。`src-tauri/src/commands/note.rs` の `translate_note_core` のテストが `AppState` を作っている方法（`AppState::new(...)` の引数）を読み、同じ手順で作るヘルパーをこのテストモジュール内に書く。`noop_notify()` は `Arc::new(|_| {})`。

- [ ] **Step 5: コマンド・Event・配線の実装**

`events.rs`:

```rust
/// ローカル翻訳のモデルの状態が変わった（ダウンロードの進捗・検証・完了・失敗）。約 250ms に間引く（Issue #603）。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelStatusChanged {
    pub status: crate::translate::model_store::LocalModelStatus,
}
```

`LocalModelStatus` は `Serialize, Type` だけなので、Event に必要な `Deserialize` も導出に足す（`model_store.rs` の derive に `Deserialize`）。

`commands/local_model.rs`:

```rust
//! ローカル翻訳のモデルの取得・状態・削除のコマンド（Issue #603）。

use crate::error::{Error, Result};
use crate::events::LocalModelStatusChanged;
use crate::state::AppState;
use crate::translate::local_models::Notify;
use crate::translate::model_store::{LocalModelStatus, LICENSE_VERSION};
use crate::translate::{invalid, is_supported, lang, ERR_UNSUPPORTED};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_specta::Event as _;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelInfo {
    pub status: LocalModelStatus,
    #[specta(type = specta_typescript::Number)]
    pub size_bytes: u64,
    /// 現行のライセンス版（`start_local_model_download` の引数に渡す）。
    pub license_version: String,
    /// 現行の版に同意済みか。
    pub license_accepted: bool,
    /// このビルド・プラットフォームがローカル翻訳に対応しているか。
    pub supported: bool,
}

pub(crate) fn info(s: &AppState) -> LocalModelInfo {
    LocalModelInfo {
        status: s.local_models.status(),
        size_bytes: s.local_models.size_bytes(),
        license_version: LICENSE_VERSION.into(),
        license_accepted: s.settings.local_model_license_version().as_deref() == Some(LICENSE_VERSION),
        supported: is_supported(),
    }
}

/// ライセンスへの同意を Rust 側で強制してから、ダウンロードを始める。同意は、開始できたあとに記録する。
pub(crate) async fn start_core(s: &AppState, accepted_license_version: &str, notify: Notify) -> Result<()> {
    if accepted_license_version != LICENSE_VERSION {
        return Err(Error::Invalid("ライセンスへの同意が必要です（版が一致しません）".into()));
    }
    if !is_supported() {
        return Err(invalid(ERR_UNSUPPORTED));
    }
    s.local_models.start(notify)?;
    s.settings.set_local_model_license_version(LICENSE_VERSION)
}

pub(crate) fn is_lang_supported(lang: &str) -> bool {
    lang::to_plamo_lang(lang).is_some()
}

#[tauri::command]
#[specta::specta]
pub async fn get_local_model_status(state: State<'_, AppState>) -> Result<LocalModelInfo> {
    Ok(info(&state))
}

/// モデルのダウンロードを始める。`accepted_license_version` が現行のライセンス版と一致しなければ拒否する。
/// 進捗は Event `LocalModelStatusChanged`。設定画面を閉じても続く。
#[tauri::command]
#[specta::specta]
pub async fn start_local_model_download(
    app: AppHandle,
    state: State<'_, AppState>,
    accepted_license_version: String,
) -> Result<()> {
    let notify: Notify = Arc::new(move |status| {
        let _ = LocalModelStatusChanged { status }.emit(&app);
    });
    start_core(&state, &accepted_license_version, notify).await
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_local_model_download(state: State<'_, AppState>) -> Result<()> {
    state.local_models.cancel();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_local_model(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.local_models.delete().await?;
    let _ = LocalModelStatusChanged { status: state.local_models.status() }.emit(&app);
    Ok(())
}

/// 翻訳先の言語コードが PLaMo の対応言語か（`pt` などは非対応）。
#[tauri::command]
#[specta::specta]
pub async fn is_translate_lang_supported(lang: String) -> Result<bool> {
    Ok(is_lang_supported(&lang))
}
```

`commands/mod.rs` に `pub mod local_model;`。`lib.rs` の `collect_commands!` に5つ、`collect_events!` に `events::LocalModelStatusChanged` を足す。

`state.rs`: `AppState` に

```rust
    /// ローカル翻訳のモデルの取得・状態・削除（Issue #603）。既定は非対応（`unconfigured`）。起動時に `with_local_models` で差し替える。
    pub local_models: std::sync::Arc<crate::translate::LocalModels>,
```

を足し、`new` の初期値は `Arc::new(crate::translate::LocalModels::unconfigured())`、`with_local_translate` の隣に

```rust
    pub fn with_local_models(mut self, local_models: std::sync::Arc<crate::translate::LocalModels>) -> Self {
        self.local_models = local_models;
        self
    }
```

`lib.rs` の setup（`LocalTranslate::with_models_dir` の直後）:

```rust
            let models_dir = app.path().app_data_dir().expect("no app data dir").join("models");
            let download_http = net_guard::build_fetch_client(state::USER_AGENT, net_guard::FetchAllowlist::default())
                .expect("failed to build the model download client");
            let local_models = std::sync::Arc::new(translate::LocalModels::new(models_dir, download_http, local_translate.clone()));
```

そして `.with_local_models(local_models)` を `AppState` の組み立てのチェーンに足す。`FetchAllowlist` と `build_fetch_client` は `pub(crate)` なので `lib.rs`（同一クレート）から使える。`state::USER_AGENT` の可視性が足りなければ `pub(crate)` にする（`pub const` のはず）。

- [ ] **Step 6: バインディングを再生成して全体を確認**

Run:
```bash
cd src-tauri && cargo test 2>&1 | tail -15
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 diff --stat frontend/src/bindings/tauri.gen.ts
grep -n "getLocalModelStatus\|localModelStatusChanged\|isTranslateLangSupported\|LocalModelInfo\|LocalModelStatus" ../frontend/src/bindings/tauri.gen.ts | head
```
Expected: `cargo test` 全 PASS（`generates_frontend_bindings` がバインディングを更新）。バインディングに `getLocalModelStatus` / `startLocalModelDownload` / `cancelLocalModelDownload` / `deleteLocalModel` / `isTranslateLangSupported` / `events.localModelStatusChanged` / `LocalModelInfo` / `LocalModelStatus`（`downloaded: number`）が出る。`bigint` が出ていたら、`#[specta(type = specta_typescript::Number)]` の付け忘れ。

Run: `cargo check --lib --no-default-features --features custom-protocol 2>&1 | tail -5` → OK（`local-translate` 無効でも通る）。

- [ ] **Step 7: 実モデルのダウンロードの確認テスト（`#[ignore]`）**

`local_models.rs` の `mod tests` の末尾に、公式 URL から実際に取得するテストを足す（CI では走らない）:

```rust
    #[tokio::test]
    #[ignore = "実際に Hugging Face から約 573MB を取得する: TSUMUGI_TEST_DOWNLOAD_DIR=<ディスク上の空のディレクトリ> cargo test --lib translate::local_models::tests::downloads_the_real_model -- --ignored --nocapture"]
    async fn downloads_the_real_model() {
        // /tmp は tmpfs（メモリ）なので、必ずディスク上のディレクトリを指定させる。
        let dir = PathBuf::from(std::env::var_os("TSUMUGI_TEST_DOWNLOAD_DIR").expect("TSUMUGI_TEST_DOWNLOAD_DIR を指定する"));
        let http = crate::net_guard::build_fetch_client("tsumugi-test", crate::net_guard::FetchAllowlist::default()).unwrap();
        let m = Arc::new(LocalModels::new(dir.clone(), http, Arc::new(LocalTranslate::unconfigured())));
        let last = Arc::new(Mutex::new(None));
        let l = last.clone();
        m.start(Arc::new(move |s| *l.lock().unwrap() = Some(s))).unwrap();
        tokio::time::timeout(Duration::from_secs(1800), async { while m.is_running() { tokio::time::sleep(Duration::from_millis(200)).await } })
            .await
            .expect("30分で終わらない");
        assert_eq!(m.status(), LocalModelStatus::Ready, "最後の通知: {:?}", last.lock().unwrap());
        assert_eq!(std::fs::metadata(model_path(&dir)).unwrap().len(), super::super::model_store::MODEL_SIZE_BYTES);
    }
```

Run（ユーザーの回線で約 573MB。**実行前にユーザーへ一言知らせる**）:
```bash
mkdir -p $HOME/.cache/tsumugi-603/dl && cd src-tauri && TSUMUGI_TEST_DOWNLOAD_DIR=$HOME/.cache/tsumugi-603/dl cargo test --lib translate::local_models::tests::downloads_the_real_model -- --ignored --nocapture 2>&1 | tail -8
```
Expected: PASS（`Ready`）。公式の SHA-256 定数がここで実データと一致することが確認される。続けて、取得したファイルで段階1の実モデルのテストを動かし、取得物が翻訳に使えることを確認する:
`TSUMUGI_TEST_MODEL=$HOME/.cache/tsumugi-603/dl/plamo-3-translate-0.8b-Q4_K_M.gguf cargo test --lib translate::llama -- --ignored --test-threads=1` → 7 passed。

- [ ] **Step 8: コミット**

```bash
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 add src-tauri frontend/src/bindings/tauri.gen.ts
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit -m "feat(translate): モデルの取得・状態・削除のコマンドと進捗イベントを追加する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: フロントの純粋関数（表示用）

**Files:**
- Create: `frontend/src/lib/localModel.ts`
- Create: `frontend/src/lib/localModel.test.ts`

**Interfaces:**
- Consumes: 生成バインディングの `LocalModelStatus`（`import type { LocalModelStatus } from "./ipc"`。`ipc.ts` が型を再エクスポートしていなければ `../bindings/tauri.gen` から）
- Produces: `formatBytes(n: number): string`、`progressPercent(downloaded: number, total: number): number`（0–100 の整数）、`statusText(s: LocalModelStatus): string`

- [ ] **Step 1: 失敗するテストを書く**

```ts
import { describe, expect, it } from "vitest";
import { formatBytes, progressPercent, statusText } from "./localModel";

describe("formatBytes", () => {
  it("MB 単位で小数1桁（1MB = 1,000,000 バイト）", () => {
    expect(formatBytes(573_126_848)).toBe("573.1 MB");
    expect(formatBytes(0)).toBe("0.0 MB");
  });
});

describe("progressPercent", () => {
  it("0〜100 に収め、total が 0 なら 0", () => {
    expect(progressPercent(0, 100)).toBe(0);
    expect(progressPercent(50, 100)).toBe(50);
    expect(progressPercent(100, 100)).toBe(100);
    expect(progressPercent(150, 100)).toBe(100);
    expect(progressPercent(1, 0)).toBe(0);
  });
});

describe("statusText", () => {
  it("状態ごとの文言", () => {
    expect(statusText({ type: "notInstalled" })).toBe("未取得");
    expect(statusText({ type: "ready" })).toBe("取得済み");
    expect(statusText({ type: "verifying" })).toBe("検証中…");
    expect(statusText({ type: "downloading", downloaded: 286_563_424, total: 573_126_848 })).toBe(
      "ダウンロード中 286.6 / 573.1 MB（50%）",
    );
    expect(statusText({ type: "failed", reason: "通信が途切れました" })).toBe("失敗: 通信が途切れました");
  });
});
```

- [ ] **Step 2: 失敗を確認**

Run: `cd frontend && npx vitest run src/lib/localModel.test.ts 2>&1 | tail -8` → import 解決エラーで FAIL。

- [ ] **Step 3: 実装**

```ts
import type { LocalModelStatus } from "./ipc";

/// MB（1MB = 1,000,000 バイト）で小数1桁。モデルのサイズ表示用。
export function formatBytes(n: number): string {
  return `${(n / 1_000_000).toFixed(1)} MB`;
}

export function progressPercent(downloaded: number, total: number): number {
  if (total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.floor((downloaded / total) * 100)));
}

export function statusText(s: LocalModelStatus): string {
  switch (s.type) {
    case "notInstalled":
      return "未取得";
    case "ready":
      return "取得済み";
    case "verifying":
      return "検証中…";
    case "downloading":
      return `ダウンロード中 ${(s.downloaded / 1_000_000).toFixed(1)} / ${formatBytes(s.total)}（${progressPercent(s.downloaded, s.total)}%）`;
    case "failed":
      return `失敗: ${s.reason}`;
  }
}
```

`ipc.ts` に `LocalModelStatus` の型が出ていなければ、既存の型の再エクスポートの書き方（`McpStatus` の行）に合わせて足す。

- [ ] **Step 4: 通ることを確認して、コミット**

Run: `cd frontend && npx vitest run src/lib/localModel.test.ts 2>&1 | tail -6 && pnpm check 2>&1 | tail -3` → PASS / 0 errors。

```bash
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 add frontend/src/lib
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit -m "feat(frontend): モデルの状態表示用の関数を追加する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 設定 UI（エンジン選択・モデルカード・ライセンス同意）

**Files:**
- Create: `frontend/src/ui/settings/LicenseConsentDialog.svelte`
- Create: `frontend/src/ui/settings/LocalModelCard.svelte`
- Create: `frontend/src/ui/settings/LocalModelCard.test.ts`
- Modify: `frontend/src/ui/settings/ExternalIntegrationSection.svelte`
- Modify: `frontend/src/ui/settings/ExternalIntegrationSection.test.ts`

**Interfaces:**
- Consumes: Task 4 のコマンド（`commands.getLocalModelStatus` / `startLocalModelDownload` / `cancelLocalModelDownload` / `deleteLocalModel` / `isTranslateLangSupported`）と `events.localModelStatusChanged`、Task 5 の `statusText` / `progressPercent` / `formatBytes`、`lib/translateLang.ts` の `isLocalTranslateEngine`
- Produces: 設定画面の「翻訳エンジン」と、エンジンが PLaMo のときのモデルカード。testid: `translate-engine`（Dropdown）、`local-model-card`、`local-model-status`、`local-model-progress`、`local-model-download`、`local-model-cancel`、`local-model-delete`、`license-dialog`、`license-accept`、`license-cancel`、`translate-lang-unsupported`

**UI の仕様（spec §7、Task の rulings R2・R7）:**
- 「翻訳エンジン」は `Dropdown`（サーバー / PLaMo（端末内））。保存は既存の保存バー（`app.setUiPrefs({ ...app.ui, translateEngine })`）。モデル未取得でも切り替え自体はできる。
- PLaMo を選んでいる（未保存の選択でもよい）ときだけモデルカードを出す。`info.supported === false` のときはカードの代わりに「この環境では PLaMo による翻訳は使えません」。
- モデルカード: 状態（`statusText`）、サイズ、`downloading` のときだけ進捗バー（`role="progressbar"` と `aria-valuenow`）、ボタン（未取得・失敗 → 「ダウンロード」、ダウンロード中 → 「キャンセル」、取得済み → 「削除」。検証中は無効）、"Built with PLaMo" と、ライセンス全文へのリンク（`openUrl`）。
- 「ダウンロード」: `info.licenseAccepted` なら直接 `startLocalModelDownload(info.licenseVersion)`。未同意なら `LicenseConsentDialog` を出し、「同意してダウンロード」で `startLocalModelDownload(info.licenseVersion)`。開始後に `getLocalModelStatus` で `licenseAccepted` を再取得する。
- ダイアログの本文（日本語の要約）: ①個人・非商用は無料、商用は事前登録と年商 10 億円以下が条件、②モデルを第三者に提供する場合は全文の写し・"Built with PLaMo" の表示・派生モデル名への "PLaMo" の含有が必要、③PFN が一方的に改定できる、④取得するファイルは約 573MB（`formatBytes(info.sizeBytes)`）。全文へのリンク（`openUrl`）。キャンセルボタンに初期フォーカス（`ConfirmDialog` と同じ。Enter 一発で同意にならない）。
- 削除は `ConfirmDialog`（`danger`）で確認してから `deleteLocalModel`。
- マウント時に `getLocalModelStatus` で現在値を取得し、`events.localModelStatusChanged.listen` で更新する。アンマウントで `unlisten`。設定を閉じて開き直しても、進行中の進捗が見える。
- 翻訳先言語が PLaMo 非対応（`isTranslateLangSupported(translateTargetLang)` が false）かつエンジンが PLaMo のとき、「この言語には対応していません」の注記（`translate-lang-unsupported`）を翻訳先の下に出す。
- 「翻訳先言語」の下の説明文を、エンジン別にする: サーバー → 既存の文。PLaMo → 「ノートのメニューの「PLaMo で翻訳」で使う言語です。翻訳は端末内で行われ、ノートの本文は外部へ送信されません。」

- [ ] **Step 1: `LocalModelCard` の失敗するテストを書く**

`LocalModelCard.test.ts`（`McpSection.test.ts` と同じモックの作り。`events.localModelStatusChanged.listen` はハンドラを捕まえて、テストから状態を流す）:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { commands, events } from "../../lib/ipc";
import type { LocalModelInfo, LocalModelStatus } from "../../lib/ipc";

vi.mock("@tauri-apps/plugin-os", () => ({ platform: () => "linux" }));
const openUrl = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (u: string) => openUrl(u) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const { default: LocalModelCard } = await import("./LocalModelCard.svelte");

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const info = (over: Partial<LocalModelInfo> = {}): LocalModelInfo => ({
  status: { type: "notInstalled" },
  sizeBytes: 573_126_848,
  licenseVersion: "2026-11-09",
  licenseAccepted: false,
  supported: true,
  ...over,
});

let emit: (s: LocalModelStatus) => void = () => {};
const unlisten = vi.fn();

beforeEach(() => {
  openUrl.mockReset();
  unlisten.mockReset();
  vi.spyOn(events.localModelStatusChanged, "listen").mockImplementation(async (cb: never) => {
    emit = (status) => (cb as (e: { payload: { status: LocalModelStatus } }) => void)({ payload: { status } });
    return unlisten;
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LocalModelCard", () => {
  it("未取得: サイズと Built with PLaMo を出し、ダウンロードだけが押せる", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    expect((await findByTestId("local-model-status")).textContent).toContain("未取得");
    expect(getByTestId("local-model-card").textContent).toContain("573.1 MB");
    expect(getByTestId("local-model-card").textContent).toContain("Built with PLaMo");
    expect(getByTestId("local-model-download")).toBeTruthy();
    expect(queryByTestId("local-model-cancel")).toBeNull();
    expect(queryByTestId("local-model-delete")).toBeNull();
  });

  it("未同意でダウンロードを押すと同意ダイアログが出て、同意するまで開始しない。キャンセルでも開始しない", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    expect(getByTestId("license-dialog").textContent).toContain("商用");
    expect(start).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("license-cancel"));
    expect(queryByTestId("license-dialog")).toBeNull();
    expect(start).not.toHaveBeenCalled();
  });

  it("同意してダウンロード: 現行のライセンス版を渡して開始し、同意済みを再取得する", async () => {
    const get = vi
      .spyOn(commands, "getLocalModelStatus")
      .mockResolvedValueOnce(ok(info()))
      .mockResolvedValue(ok(info({ status: { type: "downloading", downloaded: 0, total: 573_126_848 }, licenseAccepted: true })));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    await fireEvent.click(getByTestId("license-accept"));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("2026-11-09"));
    await vi.waitFor(() => expect(get).toHaveBeenCalledTimes(2));
  });

  it("同意済みなら、ダイアログを出さずに開始する", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ licenseAccepted: true })));
    const start = vi.spyOn(commands, "startLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, queryByTestId } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-download"));
    await vi.waitFor(() => expect(start).toHaveBeenCalledWith("2026-11-09"));
    expect(queryByTestId("license-dialog")).toBeNull();
  });

  it("進行中のダウンロード: 開き直しても進捗が見え、イベントで更新され、キャンセルできる", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(
      ok(info({ status: { type: "downloading", downloaded: 100_000_000, total: 573_126_848 }, licenseAccepted: true })),
    );
    const cancel = vi.spyOn(commands, "cancelLocalModelDownload").mockResolvedValue(ok(null));
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    const bar = await findByTestId("local-model-progress");
    expect(bar.getAttribute("aria-valuenow")).toBe("17");
    emit({ type: "downloading", downloaded: 286_563_424, total: 573_126_848 });
    await vi.waitFor(() => expect(getByTestId("local-model-progress").getAttribute("aria-valuenow")).toBe("50"));
    expect(queryByTestId("local-model-download")).toBeNull();
    await fireEvent.click(getByTestId("local-model-cancel"));
    expect(cancel).toHaveBeenCalled();
  });

  it("検証中はボタンを押せない。完了イベントで取得済みになり、削除が出る", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(
      ok(info({ status: { type: "verifying" }, licenseAccepted: true })),
    );
    const { findByTestId, getByTestId, queryByTestId } = render(LocalModelCard);
    expect((await findByTestId("local-model-status")).textContent).toContain("検証中");
    expect(queryByTestId("local-model-download")).toBeNull();
    expect(queryByTestId("local-model-cancel")).toBeNull();
    emit({ type: "ready" });
    await vi.waitFor(() => expect(getByTestId("local-model-status").textContent).toContain("取得済み"));
    expect(getByTestId("local-model-delete")).toBeTruthy();
  });

  it("失敗: 理由を出し、もう一度ダウンロードできる", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(
      ok(info({ status: { type: "failed", reason: "通信が途切れました" }, licenseAccepted: true })),
    );
    const { findByTestId, getByTestId } = render(LocalModelCard);
    expect((await findByTestId("local-model-status")).textContent).toContain("通信が途切れました");
    expect(getByTestId("local-model-download")).toBeTruthy();
  });

  it("削除は確認を挟む。確認してから deleteLocalModel を呼ぶ", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ status: { type: "ready" }, licenseAccepted: true })));
    const del = vi.spyOn(commands, "deleteLocalModel").mockResolvedValue(ok(null));
    const { findByTestId, findByText } = render(LocalModelCard);
    await fireEvent.click(await findByTestId("local-model-delete"));
    expect(del).not.toHaveBeenCalled();
    await fireEvent.click(await findByText("削除"));
    await vi.waitFor(() => expect(del).toHaveBeenCalled());
  });

  it("非対応の環境では、カードの代わりに非対応の文を出す", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info({ supported: false })));
    const { findByTestId, queryByTestId } = render(LocalModelCard);
    expect((await findByTestId("local-model-card")).textContent).toContain("この環境では PLaMo による翻訳は使えません");
    expect(queryByTestId("local-model-download")).toBeNull();
  });

  it("アンマウントでイベントの購読を解除する", async () => {
    vi.spyOn(commands, "getLocalModelStatus").mockResolvedValue(ok(info()));
    const { findByTestId, unmount } = render(LocalModelCard);
    await findByTestId("local-model-status");
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
```

削除の確認テストの `findByText("削除")` は、`ConfirmDialog` の `confirmLabel="削除"` のボタンを指す。カードの「削除」ボタン自体も「削除」という文言なので、`getAllByText("削除")` の最後の要素（ダイアログのボタン、portal で body 直下）を押す形に直す（実装後に実際の DOM で確かめる）。

- [ ] **Step 2: 失敗を確認**

Run: `cd frontend && npx vitest run src/ui/settings/LocalModelCard.test.ts 2>&1 | tail -8` → `LocalModelCard.svelte` が無くて FAIL。

- [ ] **Step 3: `LicenseConsentDialog.svelte` を実装**

`ConfirmDialog.svelte` の作り（portal で body 直下、`pushModalHistory`、キャンセルに初期フォーカス）に揃え、本文だけ構造化する。props: `licenseUrl`、`sizeBytes`、`onAccept`、`onCancel`、`onOpenLicense`。

```svelte
<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { app } from "../../lib/store.svelte";
  import { pushModalHistory } from "../../lib/modalHistory";
  import { formatBytes } from "../../lib/localModel";

  let {
    sizeBytes,
    onAccept,
    onCancel,
    onOpenLicense,
  }: { sizeBytes: number; onAccept: () => void; onCancel: () => void; onOpenLicense: () => void } = $props();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
  let cancelButton = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    cancelButton?.focus();
  });
  $effect(() => {
    if (!app.useMobileUi()) return;
    return pushModalHistory(() => onCancel());
  });
</script>

<div
  class="fixed inset-0 z-[1000] grid items-start justify-items-center bg-black/45 pt-[max(8vh,var(--safe-top))]"
  use:portal
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
  role="presentation"
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[min(420px,92vw)] rounded-xl border border-border bg-background p-4"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-labelledby="license-dialog-title"
    tabindex="-1"
    data-testid="license-dialog"
  >
    <header id="license-dialog-title" class="mb-2.5 font-semibold">PLaMo Community License への同意</header>
    <p class="mb-2 mt-0 text-sm text-foreground">
      翻訳モデル（PLaMo 3 Translate）は、Preferred Networks の PLaMo Community License で提供されています。ダウンロードするには、次の内容への同意が必要です。
    </p>
    <ul class="mb-2 mt-0 list-disc pl-5 text-sm text-foreground">
      <li>個人・非商用の利用は無料です。商用で利用する場合は、事前の登録と、年商 10 億円以下であることが条件です。</li>
      <li>モデルを第三者に提供する場合は、ライセンス全文の写しの提供、"Built with PLaMo" の表示、派生モデル名への "PLaMo" の含有が必要です。</li>
      <li>ライセンスは PFN が一方的に改定できます。版が変わったときは、次のダウンロードの前に再度同意を求めます。</li>
      <li>取得するファイルは約 {formatBytes(sizeBytes)} です。</li>
    </ul>
    <p class="mb-4 mt-0 text-sm">
      <button type="button" class="border-0 bg-transparent p-0 text-left text-sm text-primary hover:underline" onclick={onOpenLicense}>
        ライセンス全文（日本語）を開く
      </button>
    </p>
    <div class="flex justify-end gap-2">
      <Button bind:ref={cancelButton} variant="secondary" size="sm" onclick={onCancel} data-testid="license-cancel">キャンセル</Button>
      <Button size="sm" onclick={onAccept} data-testid="license-accept">同意してダウンロード</Button>
    </div>
  </div>
</div>
```

（`Button` が `data-testid` を透過するかは、`ConfirmDialog.test.ts` か他のテストの実績で確認し、透過しなければ `<Button>` を包む要素に付けるか、既存コードの testid の付け方に合わせる。`z-[1000]` は `ConfirmDialog` が `style` で与えている値と同じ層。`z-[...]` の角括弧が style-guide に反するなら、`ConfirmDialog` と同じ `style={`z-index:1000`}` にする。）

- [ ] **Step 4: `LocalModelCard.svelte` を実装**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { Button } from "$lib/components/ui/button";
  import { commands, events, unwrap } from "../../lib/ipc";
  import type { LocalModelInfo, LocalModelStatus } from "../../lib/ipc";
  import { formatBytes, progressPercent, statusText } from "../../lib/localModel";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import LicenseConsentDialog from "./LicenseConsentDialog.svelte";

  const LICENSE_URL =
    "https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/blob/1cf96ae02133958a20074bfdfab76517bb737d00/LICENSE/ja";

  let info = $state<LocalModelInfo | null>(null);
  let status = $state<LocalModelStatus>({ type: "notInstalled" });
  let err = $state<string | null>(null);
  let consentOpen = $state(false);
  let deleteOpen = $state(false);

  async function refresh() {
    info = await unwrap(commands.getLocalModelStatus());
    status = info.status;
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    refresh().catch((e) => (err = String(e)));
    events.localModelStatusChanged
      .listen((e) => (status = e.payload.status))
      .then((u) => (disposed ? u() : (unlisten = u)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  async function start() {
    err = null;
    if (!info) return;
    try {
      await unwrap(commands.startLocalModelDownload(info.licenseVersion));
      consentOpen = false;
      await refresh();
    } catch (e) {
      consentOpen = false;
      err = String(e);
    }
  }

  function onDownload() {
    if (info?.licenseAccepted) void start();
    else consentOpen = true;
  }

  async function cancel() {
    try {
      await unwrap(commands.cancelLocalModelDownload());
    } catch (e) {
      err = String(e);
    }
  }

  async function remove() {
    deleteOpen = false;
    err = null;
    try {
      await unwrap(commands.deleteLocalModel());
      await refresh();
    } catch (e) {
      err = String(e);
    }
  }
</script>

<div class="mb-3 rounded-md border border-border p-3 text-sm" data-testid="local-model-card">
  {#if info && !info.supported}
    <p class="m-0 text-muted-foreground">この環境では PLaMo による翻訳は使えません。</p>
  {:else}
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex flex-col gap-0.5">
        <span class="font-semibold">PLaMo 3 Translate（0.8B・Q4_K_M）</span>
        <span class="text-xs text-muted-foreground" data-testid="local-model-status">{statusText(status)}</span>
        <span class="text-xs text-muted-foreground">サイズ {formatBytes(info?.sizeBytes ?? 0)}</span>
      </div>
      <div class="flex gap-2">
        {#if status.type === "notInstalled" || status.type === "failed"}
          <Button size="sm" onclick={onDownload} data-testid="local-model-download">ダウンロード</Button>
        {:else if status.type === "downloading"}
          <Button size="sm" variant="secondary" onclick={cancel} data-testid="local-model-cancel">キャンセル</Button>
        {:else if status.type === "ready"}
          <Button size="sm" variant="secondary" onclick={() => (deleteOpen = true)} data-testid="local-model-delete">削除</Button>
        {/if}
      </div>
    </div>
    {#if status.type === "downloading"}
      <div
        class="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={progressPercent(status.downloaded, status.total)}
        data-testid="local-model-progress"
      >
        <div class="h-full bg-primary" style={`width:${progressPercent(status.downloaded, status.total)}%`}></div>
      </div>
    {/if}
    {#if err}
      <p class="mb-0 mt-2 text-xs text-destructive" role="alert">{err}</p>
    {/if}
    <p class="mb-0 mt-2 text-xs text-muted-foreground">
      Built with PLaMo ·
      <button type="button" class="border-0 bg-transparent p-0 text-xs text-primary hover:underline" onclick={() => openUrl(LICENSE_URL)}>
        PLaMo Community License
      </button>
    </p>
  {/if}
</div>

{#if consentOpen && info}
  <LicenseConsentDialog
    sizeBytes={info.sizeBytes}
    onAccept={start}
    onCancel={() => (consentOpen = false)}
    onOpenLicense={() => openUrl(LICENSE_URL)}
  />
{/if}
{#if deleteOpen}
  <ConfirmDialog
    title="モデルを削除"
    message="取得した PLaMo のモデルを削除します。再び使うには、もう一度ダウンロードが必要です。"
    confirmLabel="削除"
    danger
    onConfirm={remove}
    onCancel={() => (deleteOpen = false)}
  />
{/if}
```

`LICENSE_URL` はフロントに複製せず、`LocalModelInfo` に `license_url` を足して Rust の定数 `LICENSE_URL`（Task 2）を一本化する。Task 4 の `LocalModelInfo` に `pub license_url: String` を足し（`info()` で `LICENSE_URL.into()`）、バインディングを再生成して、上のコンポーネントでは `info.licenseUrl` を使う（定数 `LICENSE_URL` は削除）。`LocalModelCard.test.ts` の `info()` ヘルパーにも `licenseUrl: "https://example.com/license"` を足し、`openUrl` が `licenseUrl` で呼ばれることを「全文を開く」のテストに加える。

- [ ] **Step 5: カードのテストを通す**

Run: `cd frontend && npx vitest run src/ui/settings/LocalModelCard.test.ts 2>&1 | tail -20`
Expected: 全 PASS。削除のテストは Step 1 の注記のとおり実際の DOM に合わせて直す。

- [ ] **Step 6: `ExternalIntegrationSection` の失敗するテストを書く**

`ExternalIntegrationSection.test.ts` の既存の作り（モック・`app.ui` の初期化）に合わせて、次を足す。既存テストが `commands` の何をスパイしているかを読み、このセクションが新たに呼ぶ `getLocalModelStatus` と `isTranslateLangSupported` を既存の `beforeEach` でモックする:

```ts
  it("翻訳エンジンの既定はサーバーで、PLaMo を選ぶまでモデルカードは出ない", async () => {
    const { findByTestId, queryByTestId } = render(ExternalIntegrationSection);
    await findByTestId("translate-engine");
    expect(queryByTestId("local-model-card")).toBeNull();
  });

  it("PLaMo を選んで保存すると translateEngine=local になり、他の項目を消さない", async () => {
    // 既存の「他の項目を消さない」テストの形に合わせる（app.ui に複数の項目を入れ、setUiPrefs の引数を検査）。
    // 期待: { ...app.ui, translateEngine: "local" }
  });

  it("保存済みが local なら、最初からモデルカードを出す。大文字や空白付きの保存値も local として扱う", async () => {
    app.ui = { ...app.ui, translateEngine: " Local " } as never;
    const { findByTestId } = render(ExternalIntegrationSection);
    expect(await findByTestId("local-model-card")).toBeTruthy();
  });

  it("エンジンが PLaMo で翻訳先が非対応の言語なら、注記を出す", async () => {
    vi.spyOn(commands, "isTranslateLangSupported").mockResolvedValue(ok(false));
    app.ui = { ...app.ui, translateEngine: "local", translateTargetLang: "pt" } as never;
    const { findByTestId } = render(ExternalIntegrationSection);
    expect((await findByTestId("translate-lang-unsupported")).textContent).toContain("この言語には対応していません");
  });

  it("エンジンがサーバーなら、翻訳先が pt でも注記は出ない", async () => {
    vi.spyOn(commands, "isTranslateLangSupported").mockResolvedValue(ok(false));
    app.ui = { ...app.ui, translateEngine: "server", translateTargetLang: "pt" } as never;
    const { findByTestId, queryByTestId } = render(ExternalIntegrationSection);
    await findByTestId("translate-engine");
    expect(queryByTestId("translate-lang-unsupported")).toBeNull();
  });
```

- [ ] **Step 7: `ExternalIntegrationSection` を実装して通す**

`<script>` に追加:

```ts
  import { isLocalTranslateEngine } from "../../lib/translateLang";
  import LocalModelCard from "./LocalModelCard.svelte";

  const TRANSLATE_ENGINE_OPTIONS = [
    { value: "server", label: "サーバー" },
    { value: "local", label: "PLaMo（端末内）" },
  ];
  // 保存値は「local だけがローカル、それ以外はサーバー」（Rust の effective_translate_engine と同じ規則）に整える。
  let translateEngine = $state(isLocalTranslateEngine(app.ui.translateEngine) ? "local" : "server");
  let langSupported = $state(true);
  $effect(() => {
    const lang = translateTargetLang;
    unwrap(commands.isTranslateLangSupported(lang))
      .then((v) => (langSupported = v))
      .catch(() => (langSupported = true));
  });
```

`save()` の `setUiPrefs` に `translateEngine,` を足す。テンプレートの「翻訳先言語」ブロックの前に「翻訳エンジン」ブロックを置く:

```svelte
<div class="mb-3 flex flex-col gap-1.5 text-sm">
  <span class="text-muted-foreground">翻訳エンジン</span>
  <div class="w-48">
    <Dropdown bind:value={translateEngine} options={TRANSLATE_ENGINE_OPTIONS} testid="translate-engine" />
  </div>
  {#if translateEngine === "local"}
    <LocalModelCard />
  {/if}
</div>
```

「翻訳先言語」ブロックの説明文を、`{#if translateEngine === "local"}`（PLaMo の説明）/ `{:else}`（既存の文）に分け、その下に:

```svelte
  {#if translateEngine === "local" && !langSupported}
    <p class="mb-0 mt-0 text-xs text-destructive" data-testid="translate-lang-unsupported">
      この言語には対応していません。PLaMo は、日本語・英語・中国語・韓国語など 53 言語に対応しています（ポルトガル語は対応していません）。
    </p>
  {/if}
```

Run: `cd frontend && npx vitest run src/ui/settings 2>&1 | tail -12 && pnpm check 2>&1 | tail -3 && pnpm test 2>&1 | tail -6`
Expected: すべて PASS、`0 errors`。

- [ ] **Step 8: コミット（Rust の `license_url` を含む）**

Run: `cd src-tauri && cargo test --lib commands::local_model 2>&1 | tail -4`（`info` のテストに `license_url == LICENSE_URL` を足して PASS）

```bash
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 add frontend src-tauri
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit -m "feat(settings): 翻訳エンジンの選択、モデルカード、ライセンス同意ダイアログを追加する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: ドキュメントと最終確認

**Files:**
- Modify: `docs/guide/user-guide.md`
- Modify: `CLAUDE.md`
- Modify: `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`

- [ ] **Step 1: ユーザーガイド**

`docs/guide/user-guide.md` の翻訳の節（「翻訳」で検索）を読み、同じ文体・見出しの深さで、次を足す:

- 翻訳エンジンは「設定 → 外部連携」の「翻訳エンジン」で、サーバー（既定）か PLaMo（端末内）を選べる。PLaMo はノートの本文を外部へ送らず、端末内で翻訳する。
- PLaMo を使うには、モデル（約 573MB）のダウンロードが必要。「ダウンロード」を押し、ライセンスに同意する。回線が切れても、もう一度押すと途中から再開する。取得後はオフラインでも翻訳できる。「削除」で消せる（ディスクの空き容量を戻す）。
- ライセンス: PLaMo Community License。個人・非商用は無料。商用利用は事前登録と年商 10 億円以下が条件。**Built with PLaMo**。全文へのリンク。
- 翻訳先が PLaMo 非対応（ポルトガル語など）の言語のときは、サーバー翻訳に切り替える。
- 初回の翻訳はモデルの読み込みで 1〜2 秒余計にかかる。長いノートは翻訳できない（`LOCAL_TEXT_TOO_LONG` の表示）。しばらく使わないとメモリから解放される。

- [ ] **Step 2: `CLAUDE.md` と spec**

`CLAUDE.md` の `translate/` の bullet の `model_store.rs` の記述（「ダウンロードは未実装」）を、次の内容に直す: `model_store.rs`（固定値=URL・リビジョン・SHA-256・ライセンス版、`LocalModelStatus`、空き容量の判定）、`download.rs`（Range 再開・SHA-256 検証・`.part`。キャンセルは `.part` を残す）、`local_models.rs`（`LocalModels`: 同時に1つ、状態保持、進捗は `Notify` で通知、削除は先にアンロード）。コマンドは `commands/local_model.rs`、進捗は Event `LocalModelStatusChanged`。ライセンス同意は Rust 側で強制し（`start_local_model_download` の引数が `LICENSE_VERSION` と一致しなければ拒否）、同意版は `SettingsData.local_model_license_version`（エクスポート対象外・インポートで維持）。モデルのリビジョンを更新するときは `MODEL_REVISION` / `MODEL_SHA256` / `MODEL_SIZE_BYTES` / `LICENSE_VERSION` を HF API（`/api/models/pfnet/plamo-3-translate-0.8b-GGUF?blobs=true`）の値で同時に更新する。

spec §2 の末尾に「キャンセルは `.part` を残し、次回は途中から再開する」、§3 に「保存済みの同意版が現行と同じならダイアログを出さず開始する。同意版はダウンロードの開始に成功したあとに保存する」、§2 のコマンド一覧に `is_translate_lang_supported`（非対応言語の判定を Rust に一本化）を追記する。

- [ ] **Step 3: 全体のテスト**

Run:
```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2/src-tauri && cargo test 2>&1 | tail -8
cargo check --lib --no-default-features --features custom-protocol 2>&1 | tail -3
cd ../frontend && pnpm check 2>&1 | tail -3 && pnpm test 2>&1 | tail -6
```
Expected: `cargo test` 全 PASS（段階 1 の 1119 に段階 2 が加わる）、`--no-default-features` の check が OK、`pnpm check` 0 errors、`pnpm test` 全 PASS。

- [ ] **Step 4: 実環境での手動確認（デスクトップ）**

`feedback-dev-server-verification-must-use-virtual-display` に従い、**Xvfb 越し**（`WAYLAND_DISPLAY` も unset、`dbus-run-session`、隔離した `XDG_*` は `/tmp` 直下の短いパス）で `cargo tauri dev`（リポジトリルートから）を起動し、次を確認する。デバッグブリッジ（`curl --unix-socket ... http://localhost/`）と、必要ならスクリーンショットで確認する。起動した自分のプロセスは、完了前に PID を指定して止める。

1. 設定 → 外部連携で「翻訳エンジン」を PLaMo にすると、モデルカードが出る（未取得）。
2. 「ダウンロード」→ 同意ダイアログ（キャンセルにフォーカス）→「同意してダウンロード」で進捗バーが進む。途中で設定画面を閉じて開き直しても、進捗が続いている。
3. 「キャンセル」→ 状態が「未取得」に戻る。もう一度「ダウンロード」で、同意ダイアログなしで開始し、途中から再開する（ダウンロードの総量が減る）。
4. 完了で「取得済み」。ノートのメニューの「PLaMo で翻訳」で、実際に訳文が出る（ローカルのモデルでの通し確認）。
5. 「削除」→ 確認 →「未取得」。`app_data_dir/models/` からファイルが消えている。
6. 翻訳先を `pt` にして PLaMo を選ぶと、非対応の注記が出る。

確認できなかった項目は、PR 本文の「未確認」に書く。画面の確認ができない環境なら、その旨を明記し、`LocalModelCard.test.ts` で代替したと書く（「実機/実UIでの動作確認を行った」にはチェックしない）。

- [ ] **Step 5: 後片付けとコミット**

Run: `rm -rf $HOME/.cache/tsumugi-603/dl`（Task 4 Step 7 で取得したモデル。段階 1 の実モデルのテストで使い終えていること）。

```bash
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 add docs CLAUDE.md
git -C /home/onodai145/repos/github.com/onodai145/tsumugi-603-p2 commit -m "docs(translate): ローカル翻訳のモデル取得と同意をユーザーガイドとCLAUDE.mdに書く (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

その後 `superpowers:finishing-a-development-branch` に従う。PR は `gh pr create --draft --base feat/issue-603-local-translate-core`（#626 が先にマージされたら `--base main`）。本文は `.github/pull_request_template.md` の構造に合わせ、`Refs #603`（段階 3 が残るので `Fixes` は使わない）、リリース前の確認事項（PFN への確認、商用利用の事前登録）を再掲する。
