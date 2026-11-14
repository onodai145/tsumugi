# ローカル翻訳 段階1（Rust コア・デスクトップ）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 端末内の PLaMo 3 Translate（0.8B、Q4_K_M）でノートを翻訳できる Rust コア（プロンプト、推論エンジン、マネージャー、翻訳エンジン選択、コマンド）と、それを使うフロントエンドの最小限の配線を、デスクトップで動く状態にする。

**Architecture:** `src-tauri/src/translate/` に、純粋関数（`prompt` / `lang`）、`LocalTranslator` trait の背後にある推論エンジン（`llama-cpp-2` 実装は `llama.rs`）、遅延ロード・アイドル解放・直列化を担う `LocalTranslate`（`manager.rs`）を置く。`translate_note` が設定 `translate_engine` で Misskey サーバー翻訳とローカル翻訳を振り分ける。モデルは `app_data_dir/models/` に手で置いたものを読む（ダウンロードと設定 UI は段階2）。

**Tech Stack:** Rust（tauri v2、tokio、`llama-cpp-2` の git 依存）、tauri-specta（TS バインディング生成）、Svelte 5 + Vitest。

**Spec:** `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`（Issue #603）。spec の `engine.rs` は、trait を `engine.rs`、`llama-cpp-2` を使う実装を `llama.rs` に分けて置く（`llama-cpp-2` を使わないビルドでも trait とマネージャーをコンパイルするため）。

## Scope

spec の段階は3つで、この plan は**段階1だけ**を扱う。

- 段階1（この plan）: Rust コア、デスクトップのビルド・CI の検証、フロントの最小限の配線。設定 UI がまだ無いので、有効にするには設定ファイルの `translateEngine` を `"local"` に書き、モデルを手で置く。
- 段階2（別 plan）: モデルの取得（ダウンロード）、ライセンス同意、設定 UI、ユーザーガイド。
- 段階3（別 plan）: Android（通信種別、ビルド、CI、実機検証）。

段階2・3 の plan は、段階1が入り、Windows / macOS のビルド結果（Task 1）が分かってから書く（結果次第で設計が変わりうるため）。

## Global Constraints

spec の要求のうち、段階1の全タスクに共通するもの。

- モデルは `pfnet/plamo-3-translate-0.8b-GGUF` の `Q4_K_M`、ファイル名 `plamo-3-translate-0.8b-Q4_K_M.gguf`、サイズ `573,126,848` バイト。置き場は `app_data_dir/models/`。
- `llama-cpp-2` は git 依存で `https://github.com/utilityai/llama-cpp-rs` の rev `955aa2f72463c2535cc5ab894d5d52f27a38cde2`、`default-features = false, features = ["common"]`（OpenMP は使わない）。デスクトップ（`cfg(not(any(target_os = "android", target_os = "ios")))`）だけに入れる。
- Cargo feature `local-translate`（既定で有効）。無効なビルドとモバイルでは、ローカル翻訳は `LOCAL_UNSUPPORTED` を返す。
- プロンプトは PFML3 の生文字列 `<|plamo:tag|>dataset<|plamo:msg|>translation<|plamo:tag|>input<|plamo:msg|>{本文}<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>{翻訳先}<|plamo:msg|>`（BOS は llama.cpp が付ける）。固定部分はスペシャルトークンとして、**本文はスペシャルトークンとして解釈させず**に、別々にトークン化して連結する。
- 推論は greedy、スレッド数 `min(4, available_parallelism)`、CPU のみ（`n_gpu_layers = 0`）、`n_ctx = 4096`、プロンプトが 2048 トークンを超えたら `LOCAL_TEXT_TOO_LONG`。停止は EOG または出力に `<|plamo:` が現れたとき。
- ノート本文と訳文はログに出さない。llama.cpp 自体のログも抑止する。
- エラーは専用 variant を足さず `Error::Invalid` の文言で区別する: `LOCAL_MODEL_NOT_READY` / `LOCAL_TEXT_TOO_LONG` / `LOCAL_LANG_UNSUPPORTED` / `LOCAL_UNSUPPORTED`。
- 設定 `UiPrefs.translate_engine` は `"server"`（既定）か `"local"`。未知の値は `"server"` として扱う。`Translation.source_lang` は `Option<String>`（ローカルは `None`）。
- 翻訳結果のキャッシュはしない。コピー・共有・投稿の専用導線は作らない。
- UI 表記: サーバーはメニュー「翻訳」・結果欄「{言語} から翻訳」、ローカルはメニュー・結果欄とも「PLaMo で翻訳」。視覚値は `docs/design/style-guide.md` のスケールに従う。
- コミットメッセージは**件名1行のみ**（`type(scope): 件名 (#603)` の Conventional Commits）。`Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` は2つ目の `-m` で付ける。`--no-verify` は使わない。コミットが失敗・タイムアウトしたら、リトライせず止めてユーザーに報告する。
- `main` に直接コミットしない。`feat/issue-603-local-translate-core` ブランチで作業する（Task 1 で作成）。
- **ビルドのメモリ**: llama.cpp の C++ ビルドは素の並列度で OOM になった（16 並列、この機のユーザーのアプリまで落ちた）。長いビルド・テストは次の環境で実行する。

  ```bash
  export CARGO_BUILD_JOBS=4 CMAKE_BUILD_PARALLEL_LEVEL=4
  # ユーザーの cargo tauri dev が動いている間（ss -ltn | grep -q 5173）は、ビルドロックを取り合わないよう別の出力先を使う。
  # 動いていなければ既存の src-tauri/target を使い回してよい。
  export CARGO_TARGET_DIR=$HOME/.cache/tsumugi-603/target
  # 長いコマンドはメモリ上限付きで:
  systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G <コマンド>
  ```

  `/tmp` は tmpfs（メモリ）なので、ビルド出力や大きなファイルは `/tmp` に置かない。別の出力先を使った場合は、最後（Task 8）に `rm -rf` する。初回のフルビルドは 15〜30 分かかる。

## Review Focus

spec が前提にしているが、どのタスクのテストも素直には踏まない入力・失敗のうち、利用者が踏みやすいもの。各行のテストは、担当タスクに入っている。

- 本文に `<|plamo:tag|>` などのスペシャルトークンを書いたノート → 本文はただの文字として扱い、プロンプトの構造を壊さない（Task 2 の構造テスト、Task 4 の実モデルテスト）。
- 非常に長いノート（2048 トークン超）→ ハングや強制終了ではなく `LOCAL_TEXT_TOO_LONG`（Task 3 のエラー伝播テスト、Task 4 の実モデルテスト）。
- 翻訳先が PLaMo 非対応（`pt` など）、地域付きコード（`en-US`、`zh-TW`）→ 非対応は `LOCAL_LANG_UNSUPPORTED`、地域付きは該当言語へ（Task 2、Task 6）。
- モデル未配置 → `LOCAL_MODEL_NOT_READY` を返し、失敗を覚えない。後からモデルを置けば再起動なしで翻訳できる（Task 3 の「ロード失敗は覚えない」テスト）。
- 複数のノートで続けて「翻訳」を押す → 順番に処理し、結果が混ざらない（Task 3 の直列化テスト）。

（本文が空白のみのときに `None`、旧 `settings.json` に `translateEngine` が無いとき `server`、も Task 5・6 で確認する。）

---

### Task 1: ビルド基盤（Cargo feature・並列度の制限・CI）

**Files:**
- Modify: `src-tauri/Cargo.toml`（`[features]` と、デスクトップ向けの `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`）
- Create: `src-tauri/.cargo/config.toml`
- Modify: `.github/workflows/test.yml`（`rust-test` と `rust-coverage` の apt の一覧）、`.github/workflows/release.yml`（Linux の apt の一覧）
- Create: `.github/workflows/local-translate-build.yml`

**Interfaces:**
- Produces: Cargo feature `local-translate`（既定で有効）。`#[cfg(all(feature = "local-translate", desktop))]` が真のとき、クレート `llama_cpp_2` が使える。

- [ ] **Step 1: ブランチを作る**

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi
git checkout docs/issue-603-local-translate-spec
git checkout -b feat/issue-603-local-translate-core
git branch --show-current
```

Expected: `feat/issue-603-local-translate-core`（spec と plan のコミットを含む docs ブランチから分岐する）。

- [ ] **Step 2: `Cargo.toml` に feature と依存を追加する**

`src-tauri/Cargo.toml` の `[features]` を次のとおり変更する（`default` の行を置き換え、`local-translate` を追加）。

```toml
default = ["custom-protocol", "local-translate"]
custom-protocol = ["tauri/custom-protocol"]
# ローカル翻訳（PLaMo 3 Translate、Issue #603）。llama.cpp の C++ ビルドが走るので、重いビルドを避けたい
# 開発時は無効にできる。無効のとき（とモバイル）は、ローカル翻訳は「非対応」を返す。
local-translate = ["dep:llama-cpp-2"]
```

デスクトップ向けの依存表（`getrandom = "0.3"` の下）に追加する。

```toml
# ローカル翻訳（Issue #603）。crates.io の 0.1.161 までは、同梱の llama.cpp に plamo3 トークナイザが無く
# 公式の GGUF（tokenizer.ggml.model = plamo3）を読めない。llama.cpp v0.9.0 を同梱する git の rev を固定する。
# crates.io の次のリリースに plamo3 が入ったら、バージョン指定へ戻す。OpenMP は使わない（default-features を切る）。
llama-cpp-2 = { git = "https://github.com/utilityai/llama-cpp-rs", rev = "955aa2f72463c2535cc5ab894d5d52f27a38cde2", default-features = false, features = ["common"], optional = true }
```

- [ ] **Step 3: 依存が解決されることを確認する**

```bash
cd src-tauri && cargo tree -i llama-cpp-2 -e features 2>&1 | head -12
```

Expected: `llama-cpp-2 v0.1.162 (https://github.com/utilityai/llama-cpp-rs?rev=955aa2f72463c2535cc5ab894d5d52f27a38cde2#955aa2f7)` と feature `common` が出る。`openmp` は出ない。`Cargo.lock` が更新される。

- [ ] **Step 4: cmake の並列度を `.cargo/config.toml` で抑える**

`llama-cpp-sys-2` の `build.rs` は、`CMAKE_BUILD_PARALLEL_LEVEL` が未設定だと全コアを使う。次のファイルを作る。

```toml
# llama.cpp（ローカル翻訳、Issue #603）の C++ ビルドは、並列度が既定で全コアになり、メモリを使い切って
# OOM になることがある（16 コアの開発機で実際に発生）。cmake に渡す並列度を抑える。
[env]
CMAKE_BUILD_PARALLEL_LEVEL = "4"
```

- [ ] **Step 5: 並列度の制限が実際に効くことを測る**

環境変数を外し、`.cargo/config.toml` だけが効く状態で、クリーンなビルド中の同時コンパイラ数を数える。

```bash
cd /home/onodai145/repos/github.com/onodai145/tsumugi/src-tauri
LOG=$(mktemp)
( while :; do ps -eo comm= | grep -c -E '^(cc1plus|c\+\+|g\+\+|clang\+\+)$' >> "$LOG"; sleep 2; done ) &
SAMPLER=$!
env -u CARGO_BUILD_JOBS -u CMAKE_BUILD_PARALLEL_LEVEL CARGO_TARGET_DIR=$HOME/.cache/tsumugi-603/target-par \
  systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G cargo build -p llama-cpp-sys-2
kill "$SAMPLER"
sort -n "$LOG" | tail -1
rm -rf "$LOG" "$HOME/.cache/tsumugi-603/target-par"
```

Expected: 最大値が 6 以下。**6 を超えたら** `.cargo/config.toml` は効いていない。その場合は、このファイルを削除し、Task 8 で `CLAUDE.md` に「llama.cpp のビルドは `CARGO_BUILD_JOBS=4` を付ける」と書くことにし、**ユーザーに、このブランチを取り込む前に伝える**（ユーザーの `cargo tauri dev` がブランチの変更でフルパラレルのビルドを始めて OOM になりうるため）。

- [ ] **Step 6: Linux の CI とリリースに cmake と libclang を足す**

`.github/workflows/test.yml` の `rust-test` と `rust-coverage` の両方で、apt の一覧の末尾（`patchelf` が最後の行）を置き換える（同じ文字列が2か所ある）。

```yaml
            librsvg2-dev \
            libasound2-dev \
            patchelf
```

を次にする。

```yaml
            librsvg2-dev \
            libasound2-dev \
            patchelf \
            cmake \
            libclang-dev
```

`.github/workflows/release.yml` の「Install system dependencies (Tauri v2 Linux)」にも同じ変更をする。`android-build` ジョブ（`patchelf \` で終わる別の一覧）は、デスクトップ向けの依存を入れないので変更しない。

- [ ] **Step 7: Windows・macOS のビルド確認ワークフローを作る**

`.github/workflows/local-translate-build.yml`:

```yaml
name: local-translate-build

# ローカル翻訳（Issue #603）の llama.cpp のビルドが Windows / macOS で通るかを確認する。
# 通常の PR では走らせず、関係するファイルを触ったときと手動のときだけ走る。
on:
  workflow_dispatch:
  pull_request:
    paths:
      - "src-tauri/Cargo.toml"
      - "src-tauri/Cargo.lock"
      - "src-tauri/.cargo/**"
      - "src-tauri/src/translate/**"
      - ".github/workflows/local-translate-build.yml"

permissions:
  contents: read

jobs:
  check:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: windows-latest
            target: ""
          - os: macos-latest
            target: ""
          # リリースは macOS を universal（aarch64 + x86_64）でビルドするため、x86_64 も確認する。
          - os: macos-latest
            target: x86_64-apple-darwin
    runs-on: ${{ matrix.os }}
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4

      # dtolnay/rust-toolchain@stable is an intentionally moving branch that always
      # resolves the current stable Rust release, so it is left unpinned by design.
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae # v2
        with:
          workspaces: src-tauri

      # tauri::generate_context!() は frontendDist の実在を要求する（test.yml と同じ事情）。
      # cargo check にビルド済みのフロントは要らないので、空の index.html で足りる。
      - name: Create empty frontendDist
        shell: bash
        run: |
          mkdir -p frontend/dist
          echo '<!doctype html>' > frontend/dist/index.html

      - name: cargo check (llama.cpp のビルドを含む)
        working-directory: src-tauri
        shell: bash
        run: |
          if [ -n "${{ matrix.target }}" ]; then
            cargo check --lib --target "${{ matrix.target }}"
          else
            cargo check --lib
          fi
```

- [ ] **Step 8: ローカルでビルドを確認する**

```bash
cd src-tauri
systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G cargo check --lib
systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G cargo check --lib --no-default-features --features custom-protocol
```

Expected: どちらも `Finished`。1つ目は llama.cpp のコンパイルを含む（約2分）。2つ目は llama.cpp をビルドしない。

- [ ] **Step 9: コミットする**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/.cargo/config.toml .github/workflows/test.yml .github/workflows/release.yml .github/workflows/local-translate-build.yml
git commit -m "build(translate): ローカル翻訳用のCargo featureとビルド基盤を追加 (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 10: Windows・macOS の結果を確認する（ユーザーの許可を得てから）**

Windows / macOS のビルドは、このマシンでは確かめられない。ユーザーに「Task 1 だけを push して draft PR を作り、`local-translate-build` と `rust-test` の結果を見てよいか」を確認する。許可があれば:

PR の本文は `.github/pull_request_template.md` の構成（概要 / 関連Issue / 修正内容 / 影響範囲 / 検証）に従って書く（`--body` を使うとテンプレートは自動では入らない）。段階1の PR は #603 を**閉じない**ので、関連Issue は `Refs #603`（`Fixes` / `Closes` は使わない。#603 は段階2・3 まで続く）。本文の末尾に `🤖 Generated with [Claude Code](https://claude.com/claude-code)` を付ける。

```bash
git push -u origin feat/issue-603-local-translate-core
cat .github/pull_request_template.md   # 構成を確認してから、本文を書く
gh pr create --draft --title "feat(translate): ローカル翻訳（PLaMo 3 Translate）段階1 (#603)" --body-file <本文のファイル>
```

「影響範囲」は、TS バインディング生成に影響あり（Task 5・6 で `translate_note` と `UiPrefs` / `Translation` を変更）にチェックを付け、「検証」には、この時点で実行済みのコマンドと結果、「実機/実UIでの動作確認」は未実施（設定 UI は段階2）と書く。

CI の結果は**待たず・ポーリングしない**（ユーザーが見る）。失敗したら、ユーザーの報告に従って Task 1 を直す（例: Windows の libclang パス、macOS の x86_64 の cmake 設定）。許可が得られなければ、このステップは飛ばして次の Task へ進み、結果が未確認であることを最後（Task 8）の報告に書く。

---

### Task 2: プロンプトと言語コード（純粋関数）

**Files:**
- Create: `src-tauri/src/translate/mod.rs`
- Create: `src-tauri/src/translate/prompt.rs`
- Create: `src-tauri/src/translate/lang.rs`
- Modify: `src-tauri/src/lib.rs`（`mod translate;` を追加）

**Interfaces:**
- Produces（`crate::translate`）:
  - `pub const ERR_MODEL_NOT_READY / ERR_TEXT_TOO_LONG / ERR_LANG_UNSUPPORTED / ERR_UNSUPPORTED: &str`
  - `pub fn invalid(code: &str) -> crate::error::Error`（`Error::Invalid(code.to_string())`）
  - `pub const fn is_supported() -> bool`
  - `prompt::PromptParts { pub prefix: String, pub suffix: String }`、`prompt::build_parts(plamo_lang: &str) -> PromptParts`、`prompt::join(&PromptParts, &str) -> String`、`prompt::is_stop(piece: &str) -> bool`、`prompt::STOP_MARKER: &str`
  - `lang::to_plamo_lang(code: &str) -> Option<&'static str>`

この Task の途中のコミットでは、使われていない関数の `dead_code` 警告が出る（Task 6 で使われて消える）。

- [ ] **Step 1: モジュールの骨組みを作る**

`src-tauri/src/translate/mod.rs`:

```rust
//! ローカル翻訳（PLaMo 3 Translate、Issue #603）。設計は
//! `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`。

use crate::error::Error;

pub mod lang;
pub mod prompt;

/// ローカル翻訳のエラー文言。専用の `Error` variant は足さず、#440 の `UNAVAILABLE` と同じく
/// `Error::Invalid` の文言で区別する（フロントは文言で判別する）。
pub const ERR_MODEL_NOT_READY: &str = "LOCAL_MODEL_NOT_READY";
pub const ERR_TEXT_TOO_LONG: &str = "LOCAL_TEXT_TOO_LONG";
pub const ERR_LANG_UNSUPPORTED: &str = "LOCAL_LANG_UNSUPPORTED";
/// このビルド・プラットフォームがローカル翻訳に対応していない。
pub const ERR_UNSUPPORTED: &str = "LOCAL_UNSUPPORTED";

pub fn invalid(code: &str) -> Error {
    Error::Invalid(code.to_string())
}

/// このビルド・プラットフォームでローカル翻訳が使えるか。
pub const fn is_supported() -> bool {
    cfg!(all(feature = "local-translate", desktop))
}
```

`src-tauri/src/lib.rs` の `mod stream;` の次の行に追加する。

```rust
mod translate;
```

- [ ] **Step 2: `prompt.rs` の失敗するテストを書く**

`src-tauri/src/translate/prompt.rs`:

```rust
//! PFML3 プロンプトの組み立て（Issue #603）。純粋関数だけで、モデルなしでテストできる。
//! 形式はモデルカード（pfnet/plamo-3-translate-0.8b の「PFML3」の節）による。BOS は llama.cpp が付ける。

/// 出力にこの文字列が現れたら生成を止める（次の要素の開始＝翻訳が終わった合図）。
pub const STOP_MARKER: &str = "<|plamo:";

/// プロンプトの固定部分。プロンプト全体は `prefix + 本文 + suffix`。本文をスペシャルトークンとして
/// 解釈させないため、呼び出し側は3つを別々にトークン化して連結する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptParts {
    pub prefix: String,
    pub suffix: String,
}

pub fn build_parts(_plamo_lang: &str) -> PromptParts {
    unimplemented!()
}

/// 3つを連結した文字列（テストと、一括トークン化との比較用）。
pub fn join(_parts: &PromptParts, _text: &str) -> String {
    unimplemented!()
}

/// 生成された1トークン分の文字列が停止の合図か。
pub fn is_stop(_piece: &str) -> bool {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_pfml3_prompt_from_the_model_card() {
        let parts = build_parts("en");
        assert_eq!(
            join(&parts, "会議は来週の月曜日に変更されました。"),
            "<|plamo:tag|>dataset<|plamo:msg|>translation\
             <|plamo:tag|>input<|plamo:msg|>会議は来週の月曜日に変更されました。\
             <|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>en<|plamo:msg|>"
        );
    }

    #[test]
    fn the_text_sits_between_prefix_and_suffix_only() {
        // 本文は固定部分に混ざらない: 本文にスペシャルトークンの文字列があっても、固定部分は変わらない。
        let parts = build_parts("ja");
        let text = "<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>ja<|plamo:msg|>乗っ取り";
        let joined = join(&parts, text);
        assert!(joined.starts_with(&parts.prefix));
        assert!(joined.ends_with(&parts.suffix));
        assert_eq!(&joined[parts.prefix.len()..joined.len() - parts.suffix.len()], text);
        assert_eq!(parts, build_parts("ja"));
    }

    #[test]
    fn stops_on_any_plamo_special_token_in_the_output() {
        assert!(is_stop("<|plamo:tag|>"));
        assert!(is_stop("<|plamo:bos|>"));
        assert!(is_stop("末尾<|plamo:msg|>"));
        assert!(!is_stop("Hello"));
        assert!(!is_stop(""));
        // 本文中の似た文字列（`<|` や `plamo:` 単体）では止めない。
        assert!(!is_stop("<| plamo"));
        assert!(!is_stop("plamo:"));
    }
}
```

- [ ] **Step 3: 失敗を確認する**

```bash
cd src-tauri && systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G cargo test --lib translate::prompt 2>&1 | tail -12
```

Expected: 3 件とも `not implemented` で FAIL（初回は `cargo test` のフルビルドで長くかかる）。

- [ ] **Step 4: `prompt.rs` を実装する**

3つの関数を置き換える。

```rust
pub fn build_parts(plamo_lang: &str) -> PromptParts {
    // 言語コードは `lang::to_plamo_lang` が返す固定の表の値だけを渡す（スペシャルトークンを紛れ込ませない）。
    debug_assert!(plamo_lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    PromptParts {
        prefix: "<|plamo:tag|>dataset<|plamo:msg|>translation<|plamo:tag|>input<|plamo:msg|>".to_string(),
        suffix: format!("<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>{plamo_lang}<|plamo:msg|>"),
    }
}

pub fn join(parts: &PromptParts, text: &str) -> String {
    format!("{}{}{}", parts.prefix, text, parts.suffix)
}

pub fn is_stop(piece: &str) -> bool {
    piece.contains(STOP_MARKER)
}
```

- [ ] **Step 5: 成功を確認する**

```bash
cd src-tauri && cargo test --lib translate::prompt 2>&1 | tail -8
```

Expected: `test result: ok. 3 passed`。

- [ ] **Step 6: `lang.rs` の失敗するテストを書く**

`src-tauri/src/translate/lang.rs`:

```rust
//! 設定の翻訳先言語コードを、PLaMo 3 Translate のコードへ変換する（Issue #603）。
//! 対応は 53 言語（モデルカードの「Supported Languages」で確認済み）。

/// PLaMo 3 Translate が対応する言語コード。
const SUPPORTED: &[&str] = &[
    "ja", "en", "zh-hans", "zh-hant", "ko", "ar", "az", "be", "bg", "bn", "ca", "ceb", "de", "es", "et",
    "eu", "fa", "fr", "gu", "he", "hi", "hr", "hu", "hy", "id", "it", "jv", "ka", "kk", "km", "lt", "lv",
    "ml", "mr", "ms", "my", "ne", "nl", "pa", "pl", "ru", "si", "sk", "sl", "sr", "sw", "ta", "te", "th",
    "tl", "ur", "uz", "vi",
];

/// 設定の言語コードを PLaMo のコードへ変換する。非対応は `None`。
/// `zh` は簡体字（`zh-hans`）、`zh-tw` / `zh-hk` は繁体字。地域付きのコード（`en-US` など）は地域を除いて照合する。
pub fn to_plamo_lang(_code: &str) -> Option<&'static str> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_the_53_languages_of_the_model_card() {
        assert_eq!(SUPPORTED.len(), 53);
        let mut sorted = SUPPORTED.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 53, "重複がある");
    }

    #[test]
    fn maps_the_settings_presets() {
        // frontend/src/lib/translateLang.ts のプリセット（pt を除く）。
        for (code, want) in [
            ("ja", "ja"),
            ("en", "en"),
            ("zh", "zh-hans"),
            ("ko", "ko"),
            ("fr", "fr"),
            ("de", "de"),
            ("es", "es"),
            ("ru", "ru"),
        ] {
            assert_eq!(to_plamo_lang(code), Some(want), "{code}");
        }
    }

    #[test]
    fn portuguese_is_not_supported() {
        // プリセットの中で、ポルトガル語だけが PLaMo の対応言語に含まれない。
        assert_eq!(to_plamo_lang("pt"), None);
        assert_eq!(to_plamo_lang("pt-BR"), None);
    }

    #[test]
    fn maps_chinese_variants() {
        assert_eq!(to_plamo_lang("zh-CN"), Some("zh-hans"));
        assert_eq!(to_plamo_lang("zh-hans"), Some("zh-hans"));
        assert_eq!(to_plamo_lang("zh-TW"), Some("zh-hant"));
        assert_eq!(to_plamo_lang("zh-HK"), Some("zh-hant"));
        assert_eq!(to_plamo_lang("zh-hant"), Some("zh-hant"));
        assert_eq!(to_plamo_lang("zh-xx"), None);
    }

    #[test]
    fn reduces_region_codes_to_the_language() {
        assert_eq!(to_plamo_lang("en-US"), Some("en"));
        assert_eq!(to_plamo_lang("EN-GB"), Some("en"));
        assert_eq!(to_plamo_lang("ja-JP"), Some("ja"));
        assert_eq!(to_plamo_lang("fr_CA"), Some("fr"));
    }

    #[test]
    fn trims_and_ignores_case() {
        assert_eq!(to_plamo_lang("  JA "), Some("ja"));
    }

    #[test]
    fn rejects_blank_and_unknown_codes() {
        assert_eq!(to_plamo_lang(""), None);
        assert_eq!(to_plamo_lang("   "), None);
        assert_eq!(to_plamo_lang("xx"), None);
        assert_eq!(to_plamo_lang("tlh"), None);
    }
}
```

`mod.rs` は Step 1 で `pub mod lang;` を宣言済み。

- [ ] **Step 7: 失敗を確認する**

```bash
cd src-tauri && cargo test --lib translate::lang 2>&1 | tail -14
```

Expected: `has_the_53_languages_of_the_model_card` 以外が `not implemented` で FAIL。

- [ ] **Step 8: `lang.rs` を実装する**

`to_plamo_lang` を置き換え、`find` を足す。

```rust
pub fn to_plamo_lang(code: &str) -> Option<&'static str> {
    let c = code.trim().to_ascii_lowercase().replace('_', "-");
    if c.is_empty() {
        return None;
    }
    match c.as_str() {
        "zh" | "zh-cn" | "zh-sg" => return Some("zh-hans"),
        "zh-tw" | "zh-hk" | "zh-mo" => return Some("zh-hant"),
        _ => {}
    }
    if let Some(s) = find(&c) {
        return Some(s);
    }
    let base = c.split('-').next().unwrap_or("");
    // 中国語は上で扱った地域だけを受ける（`zh-xx` を簡体字に落とさない）。
    if base == "zh" {
        return None;
    }
    find(base)
}

fn find(code: &str) -> Option<&'static str> {
    SUPPORTED.iter().copied().find(|s| *s == code)
}
```

- [ ] **Step 9: 成功を確認してコミットする**

```bash
cd src-tauri && cargo test --lib translate:: 2>&1 | tail -6
git add src-tauri/src/translate src-tauri/src/lib.rs
git commit -m "feat(translate): PFML3プロンプトと言語コード変換を追加 (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: `test result: ok. 10 passed`（prompt 3 + lang 7）。

---

### Task 3: モデル検査・推論 trait・マネージャー

**Files:**
- Create: `src-tauri/src/translate/model_store.rs`
- Create: `src-tauri/src/translate/engine.rs`
- Create: `src-tauri/src/translate/manager.rs`
- Modify: `src-tauri/src/translate/mod.rs`

**Interfaces:**
- Consumes: Task 2 の `invalid` / `ERR_*`。
- Produces:
  - `model_store::MODEL_FILE_NAME: &str`、`MODEL_SIZE_BYTES: u64`、`model_path(models_dir: &Path) -> PathBuf`、`check_ready(path: &Path) -> Result<()>`（欠落・サイズ違いは `LOCAL_MODEL_NOT_READY`）
  - `engine::LocalTranslator`（`trait: Send + Sync + 'static`、`fn translate(&self, text: &str, plamo_lang: &str) -> Result<String>`、ブロッキング）
  - `manager::Loader = Arc<dyn Fn() -> Result<Arc<dyn LocalTranslator>> + Send + Sync>`
  - `manager::LocalTranslate`: `new(Loader)`、`unconfigured()`、`with_models_dir(PathBuf)`（Task 4 で本体を足す）、`async translate(&self, text, plamo_lang) -> Result<String>`、`async unload_if_idle(&self, now: Instant, idle: Duration) -> bool`、`async is_loaded(&self) -> bool`
  - `manager::spawn_idle_sweeper(this: Arc<LocalTranslate>, idle: Duration, interval: Duration)`
  - `translate::{IDLE_UNLOAD, SWEEP_INTERVAL}: Duration`、`translate::LocalTranslate`（再エクスポート）

- [ ] **Step 1: `model_store.rs` の失敗するテストを書く**

`src-tauri/src/translate/model_store.rs`:

```rust
//! ローカル翻訳のモデルファイル（Issue #603）。段階1は、置かれたファイルの検査だけを持つ
//! （ダウンロードと状態管理は段階2）。

use super::{invalid, ERR_MODEL_NOT_READY};
use crate::error::Result;
use std::path::{Path, PathBuf};

/// `pfnet/plamo-3-translate-0.8b-GGUF` の Q4_K_M。
pub const MODEL_FILE_NAME: &str = "plamo-3-translate-0.8b-Q4_K_M.gguf";
pub const MODEL_SIZE_BYTES: u64 = 573_126_848;

pub fn model_path(_models_dir: &Path) -> PathBuf {
    unimplemented!()
}

/// モデルが使える状態か（置かれていて、サイズが期待どおりか）。ハッシュ検証はダウンロード完了時だけ行う。
pub fn check_ready(_path: &Path) -> Result<()> {
    unimplemented!()
}

fn check_ready_with(_path: &Path, _expected_size: u64) -> Result<()> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tsumugi_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn assert_not_ready(r: Result<()>) {
        match r {
            Err(Error::Invalid(m)) => assert_eq!(m, ERR_MODEL_NOT_READY),
            other => panic!("expected LOCAL_MODEL_NOT_READY, got {other:?}"),
        }
    }

    #[test]
    fn model_path_is_under_the_models_dir() {
        let p = model_path(Path::new("/data/models"));
        assert_eq!(p, Path::new("/data/models").join(MODEL_FILE_NAME));
    }

    #[test]
    fn missing_file_is_not_ready() {
        let dir = temp_dir();
        assert_not_ready(check_ready_with(&dir.join("none.gguf"), 10));
    }

    #[test]
    fn wrong_size_is_not_ready() {
        let dir = temp_dir();
        let f = dir.join("m.gguf");
        std::fs::write(&f, b"123456789").unwrap();
        assert_not_ready(check_ready_with(&f, 10));
    }

    #[test]
    fn exact_size_is_ready() {
        let dir = temp_dir();
        let f = dir.join("m.gguf");
        std::fs::write(&f, b"1234567890").unwrap();
        check_ready_with(&f, 10).unwrap();
    }

    #[test]
    fn a_directory_is_not_ready() {
        let dir = temp_dir();
        assert_not_ready(check_ready_with(&dir, 0));
    }
}
```

`mod.rs` に `pub mod model_store;` を足す。

- [ ] **Step 2: 失敗を確認する**

```bash
cd src-tauri && cargo test --lib translate::model_store 2>&1 | tail -12
```

Expected: 5 件とも `not implemented` で FAIL。

- [ ] **Step 3: `model_store.rs` を実装する**

3つの関数を置き換える。

```rust
pub fn model_path(models_dir: &Path) -> PathBuf {
    models_dir.join(MODEL_FILE_NAME)
}

pub fn check_ready(path: &Path) -> Result<()> {
    check_ready_with(path, MODEL_SIZE_BYTES)
}

fn check_ready_with(path: &Path, expected_size: u64) -> Result<()> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() == expected_size => Ok(()),
        _ => Err(invalid(ERR_MODEL_NOT_READY)),
    }
}
```

- [ ] **Step 4: 成功を確認する**

```bash
cd src-tauri && cargo test --lib translate::model_store 2>&1 | tail -6
```

Expected: `test result: ok. 5 passed`。

- [ ] **Step 5: `engine.rs` を作る**

```rust
//! ローカル翻訳の推論エンジンの境界（Issue #603）。実体は `llama.rs`（`llama-cpp-2`）で、テストでは偽の実装に
//! 差し替える（CI にはモデルが無い）。

use crate::error::Result;

/// 1つの文を翻訳する。ブロッキングで時間がかかるので、呼び出し側は `spawn_blocking` で呼ぶ。
pub trait LocalTranslator: Send + Sync + 'static {
    /// `plamo_lang` は `lang::to_plamo_lang` が返す PLaMo の言語コード。
    /// 入力が長すぎるときは `LOCAL_TEXT_TOO_LONG` の `Error::Invalid` を返す。
    fn translate(&self, text: &str, plamo_lang: &str) -> Result<String>;
}
```

`mod.rs` に `pub mod engine;` を足す。

- [ ] **Step 6: `manager.rs` の失敗するテストを書く**

`src-tauri/src/translate/manager.rs`:

```rust
//! モデルの遅延ロード、アイドル時のアンロード、翻訳要求の直列化（Issue #603）。

use super::engine::LocalTranslator;
use super::{invalid, ERR_UNSUPPORTED};
use crate::error::{Error, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 推論エンジンを作る関数。ブロッキング（モデルの読み込み）なので `spawn_blocking` で呼ぶ。
pub type Loader = Arc<dyn Fn() -> Result<Arc<dyn LocalTranslator>> + Send + Sync>;

struct Loaded {
    translator: Arc<dyn LocalTranslator>,
    last_used: Instant,
}

pub struct LocalTranslate {
    loader: Loader,
    loaded: Mutex<Option<Loaded>>,
}

impl LocalTranslate {
    pub fn new(_loader: Loader) -> Self {
        unimplemented!()
    }

    /// ローカル翻訳に対応していない構成（ビルドで無効、モバイル、モデルの置き場が未設定）。
    pub fn unconfigured() -> Self {
        unimplemented!()
    }

    pub async fn translate(&self, _text: &str, _plamo_lang: &str) -> Result<String> {
        unimplemented!()
    }

    /// 最後に使ってから `idle` 以上たっていたらモデルを解放する。解放したら true。
    pub async fn unload_if_idle(&self, _now: Instant, _idle: Duration) -> bool {
        unimplemented!()
    }

    pub async fn is_loaded(&self) -> bool {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// `"{lang}:{text}"` を返す偽の翻訳器。同時に動いた最大数を記録する。
    struct Fake {
        active: AtomicUsize,
        max_active: Arc<AtomicUsize>,
        fail_with: Option<&'static str>,
    }

    impl LocalTranslator for Fake {
        fn translate(&self, text: &str, plamo_lang: &str) -> Result<String> {
            let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            self.active.fetch_sub(1, Ordering::SeqCst);
            match self.fail_with {
                Some(code) => Err(invalid(code)),
                None => Ok(format!("{plamo_lang}:{text}")),
            }
        }
    }

    fn counting_loader(loads: Arc<AtomicUsize>, max_active: Arc<AtomicUsize>) -> Loader {
        Arc::new(move || {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Fake { active: AtomicUsize::new(0), max_active: max_active.clone(), fail_with: None })
                as Arc<dyn LocalTranslator>)
        })
    }

    fn assert_invalid(r: Result<String>, code: &str) {
        match r {
            Err(Error::Invalid(m)) => assert_eq!(m, code),
            other => panic!("expected Invalid({code}), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn loads_lazily_and_only_once() {
        let loads = Arc::new(AtomicUsize::new(0));
        let lt = LocalTranslate::new(counting_loader(loads.clone(), Arc::new(AtomicUsize::new(0))));
        assert!(!lt.is_loaded().await);
        assert_eq!(loads.load(Ordering::SeqCst), 0);

        assert_eq!(lt.translate("a", "en").await.unwrap(), "en:a");
        assert_eq!(lt.translate("b", "ja").await.unwrap(), "ja:b");
        assert!(lt.is_loaded().await);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn load_failure_is_not_remembered() {
        // モデルが無い→後から置かれた、を再起動なしで拾えること。
        let attempts = Arc::new(AtomicUsize::new(0));
        let a = attempts.clone();
        let loader: Loader = Arc::new(move || {
            if a.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(invalid(super::super::ERR_MODEL_NOT_READY))
            } else {
                Ok(Arc::new(Fake {
                    active: AtomicUsize::new(0),
                    max_active: Arc::new(AtomicUsize::new(0)),
                    fail_with: None,
                }) as Arc<dyn LocalTranslator>)
            }
        });
        let lt = LocalTranslate::new(loader);

        assert_invalid(lt.translate("a", "en").await, "LOCAL_MODEL_NOT_READY");
        assert!(!lt.is_loaded().await);
        assert_eq!(lt.translate("a", "en").await.unwrap(), "en:a");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn translation_error_is_passed_through_and_keeps_the_model_loaded() {
        let loader: Loader = Arc::new(|| {
            Ok(Arc::new(Fake {
                active: AtomicUsize::new(0),
                max_active: Arc::new(AtomicUsize::new(0)),
                fail_with: Some("LOCAL_TEXT_TOO_LONG"),
            }) as Arc<dyn LocalTranslator>)
        });
        let lt = LocalTranslate::new(loader);
        assert_invalid(lt.translate("long", "en").await, "LOCAL_TEXT_TOO_LONG");
        assert!(lt.is_loaded().await, "入力が長すぎるだけでモデルを捨てない");
    }

    #[tokio::test]
    async fn concurrent_requests_run_one_at_a_time_without_mixing_results() {
        let max_active = Arc::new(AtomicUsize::new(0));
        let lt = Arc::new(LocalTranslate::new(counting_loader(
            Arc::new(AtomicUsize::new(0)),
            max_active.clone(),
        )));
        let (a, b, c, d) = tokio::join!(
            lt.translate("one", "en"),
            lt.translate("two", "ja"),
            lt.translate("three", "fr"),
            lt.translate("four", "de"),
        );
        assert_eq!(a.unwrap(), "en:one");
        assert_eq!(b.unwrap(), "ja:two");
        assert_eq!(c.unwrap(), "fr:three");
        assert_eq!(d.unwrap(), "de:four");
        assert_eq!(max_active.load(Ordering::SeqCst), 1, "同時に2つ以上の推論が走った");
    }

    #[tokio::test]
    async fn unloads_only_after_the_idle_time_and_reloads_on_next_use() {
        let loads = Arc::new(AtomicUsize::new(0));
        let lt = LocalTranslate::new(counting_loader(loads.clone(), Arc::new(AtomicUsize::new(0))));
        let idle = Duration::from_secs(600);

        // ロード前は解放するものがない。
        assert!(!lt.unload_if_idle(Instant::now() + idle * 2, idle).await);

        lt.translate("a", "en").await.unwrap();
        assert!(!lt.unload_if_idle(Instant::now() + Duration::from_secs(10), idle).await);
        assert!(lt.is_loaded().await);

        assert!(lt.unload_if_idle(Instant::now() + idle + Duration::from_secs(1), idle).await);
        assert!(!lt.is_loaded().await);

        lt.translate("b", "en").await.unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 2, "解放後の翻訳で再ロードされる");
    }

    #[tokio::test]
    async fn unconfigured_reports_unsupported() {
        let lt = LocalTranslate::unconfigured();
        assert_invalid(lt.translate("a", "en").await, "LOCAL_UNSUPPORTED");
        assert!(!lt.is_loaded().await);
    }
}
```

`mod.rs` に追加する。

```rust
pub mod manager;

pub use manager::LocalTranslate;

/// アイドルがこれだけ続いたらモデルを解放する（初期値。Android は段階3で短くする）。
pub const IDLE_UNLOAD: std::time::Duration = std::time::Duration::from_secs(600);
/// アイドル解放の確認間隔。
pub const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
```

- [ ] **Step 7: 失敗を確認する**

```bash
cd src-tauri && cargo test --lib translate::manager 2>&1 | tail -16
```

Expected: 6 件とも `not implemented` で FAIL。

- [ ] **Step 8: `manager.rs` を実装する**

`impl LocalTranslate` を次に置き換え、`join_err` と `spawn_idle_sweeper` を足す。

```rust
impl LocalTranslate {
    pub fn new(loader: Loader) -> Self {
        Self { loader, loaded: Mutex::new(None) }
    }

    /// ローカル翻訳に対応していない構成（ビルドで無効、モバイル、モデルの置き場が未設定）。
    pub fn unconfigured() -> Self {
        Self::new(Arc::new(|| Err(invalid(ERR_UNSUPPORTED))))
    }

    pub async fn translate(&self, text: &str, plamo_lang: &str) -> Result<String> {
        // ロックを推論の終わりまで持つ: 翻訳要求を直列に処理し、ロード中の二重ロードも防ぐ。
        let mut guard = self.loaded.lock().await;
        let translator = match guard.as_ref() {
            Some(l) => l.translator.clone(),
            None => {
                let loader = self.loader.clone();
                // ロードの失敗は覚えない（モデルが後から置かれたら、再起動なしで次の翻訳から使える）。
                let t = tokio::task::spawn_blocking(move || loader()).await.map_err(join_err)??;
                *guard = Some(Loaded { translator: t.clone(), last_used: Instant::now() });
                t
            }
        };
        let (text, lang) = (text.to_string(), plamo_lang.to_string());
        let out = tokio::task::spawn_blocking(move || translator.translate(&text, &lang))
            .await
            .map_err(join_err)?;
        if let Some(l) = guard.as_mut() {
            l.last_used = Instant::now();
        }
        out
    }

    /// 最後に使ってから `idle` 以上たっていたらモデルを解放する。解放したら true。
    pub async fn unload_if_idle(&self, now: Instant, idle: Duration) -> bool {
        let mut guard = self.loaded.lock().await;
        match guard.as_ref() {
            Some(l) if now.saturating_duration_since(l.last_used) >= idle => {
                *guard = None;
                true
            }
            _ => false,
        }
    }

    pub async fn is_loaded(&self) -> bool {
        self.loaded.lock().await.is_some()
    }
}

fn join_err(e: tokio::task::JoinError) -> Error {
    // 本文は含めない（ログ・エラーに本文を出さない）。
    Error::Invalid(format!("local translate task failed: {e}"))
}

/// アイドル解放の定期確認を始める。`translate` がロックを持っている間は待つので、翻訳中に解放されない。
pub fn spawn_idle_sweeper(this: Arc<LocalTranslate>, idle: Duration, interval: Duration) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            this.unload_if_idle(Instant::now(), idle).await;
        }
    });
}
```

- [ ] **Step 9: 成功を確認してコミットする**

```bash
cd src-tauri && cargo test --lib translate:: 2>&1 | tail -6
git add src-tauri/src/translate
git commit -m "feat(translate): ローカル翻訳のマネージャーとモデル検査を追加 (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: `test result: ok. 21 passed`（prompt 3 + lang 7 + model_store 5 + manager 6）。

---

### Task 4: llama.cpp の推論エンジン（実モデル）

**Files:**
- Create: `src-tauri/src/translate/llama.rs`
- Modify: `src-tauri/src/translate/mod.rs`（`#[cfg(all(feature = "local-translate", desktop))] pub mod llama;`）
- Modify: `src-tauri/src/translate/manager.rs`（`LocalTranslate::with_models_dir`）

**Interfaces:**
- Consumes: Task 2 の `prompt::*` / `invalid` / `ERR_TEXT_TOO_LONG`、Task 3 の `LocalTranslator` / `model_store::{model_path, check_ready}` / `LocalTranslate::new` / `unconfigured`。
- Produces:
  - `llama::LlamaTranslator`（`pub fn load(path: &Path) -> Result<Self>`、`pub fn prompt_tokens(&self, text: &str, plamo_lang: &str) -> Vec<LlamaToken>`、`impl LocalTranslator`）、`llama::{N_CTX: u32, MAX_PROMPT_TOKENS: usize}`
  - `LocalTranslate::with_models_dir(models_dir: PathBuf) -> LocalTranslate`（対応していないビルドでは `unconfigured()` と同じ）

実モデルのテストは `#[ignore]` で、モデルのパスを環境変数 `TSUMUGI_TEST_MODEL` で受ける。

- [ ] **Step 1: テスト用にモデルを取得する**

```bash
mkdir -p $HOME/.cache/tsumugi-603/models
curl -L -C - -o $HOME/.cache/tsumugi-603/models/plamo-3-translate-0.8b-Q4_K_M.gguf \
  https://huggingface.co/pfnet/plamo-3-translate-0.8b-GGUF/resolve/main/plamo-3-translate-0.8b-Q4_K_M.gguf
stat -c %s $HOME/.cache/tsumugi-603/models/plamo-3-translate-0.8b-Q4_K_M.gguf
```

Expected: `573126848`（約 573MB。ディスクに置く。`/tmp` は使わない）。

- [ ] **Step 2: `llama.rs` の失敗する実モデルテストを書く**

`src-tauri/src/translate/llama.rs`（実装部分はスタブにして、テストを先に書く）:

```rust
//! llama.cpp（`llama-cpp-2`）によるローカル翻訳の実体（Issue #603）。
//! `local-translate` が有効なデスクトップのビルドだけでコンパイルされる（`mod.rs` の cfg）。

use super::engine::LocalTranslator;
use super::{invalid, prompt, ERR_TEXT_TOO_LONG};
use crate::error::{Error, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{send_logs_to_tracing, LogOptions};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::OnceLock;

/// 文脈の長さ。プロンプトは `MAX_PROMPT_TOKENS` まで、残りが出力の上限。
pub const N_CTX: u32 = 4096;
/// これを超えるプロンプトは `LOCAL_TEXT_TOO_LONG` で拒否する（`n_batch` の既定値と同じ）。
pub const MAX_PROMPT_TOKENS: usize = 2048;

pub struct LlamaTranslator {
    model: LlamaModel,
}

impl LlamaTranslator {
    pub fn load(_path: &Path) -> Result<Self> {
        unimplemented!()
    }

    /// プロンプトのトークン列。固定部分はスペシャルトークンとして、本文はただの文字として解釈する。
    pub fn prompt_tokens(&self, _text: &str, _plamo_lang: &str) -> Vec<LlamaToken> {
        unimplemented!()
    }
}

impl LocalTranslator for LlamaTranslator {
    fn translate(&self, _text: &str, _plamo_lang: &str) -> Result<String> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn load() -> LlamaTranslator {
        let path = std::env::var_os("TSUMUGI_TEST_MODEL")
            .expect("TSUMUGI_TEST_MODEL に GGUF のパスを指定する");
        LlamaTranslator::load(Path::new(&path)).expect("モデルを読み込めない")
    }

    #[test]
    #[ignore = "実モデルが要る: TSUMUGI_TEST_MODEL=... cargo test --lib translate::llama -- --ignored --nocapture"]
    fn translates_japanese_to_english() {
        let out = load().translate("今日は天気がいいので、散歩に行ってきます。", "en").unwrap();
        eprintln!("OUT: {out}");
        assert!(out.to_lowercase().contains("weather"), "{out}");
    }

    #[test]
    #[ignore = "実モデルが要る"]
    fn keeps_mentions_urls_emoji_codes_and_hashtags() {
        let src = "@alice@example.com thanks for the link :blobcat_heart: https://example.com/path?x=1 #misskey";
        let out = load().translate(src, "ja").unwrap();
        eprintln!("OUT: {out}");
        for keep in ["@alice@example.com", ":blobcat_heart:", "https://example.com/path?x=1", "#misskey"] {
            assert!(out.contains(keep), "{keep} が消えた: {out}");
        }
    }

    #[test]
    #[ignore = "実モデルが要る"]
    fn separate_tokenization_matches_whole_string_tokenization_for_plain_text() {
        // 本文を別にトークン化して連結する方式が、通常の文では一括トークン化と同じ結果になること。
        // 違ったら、この方式の前提（spec §4）が崩れているので、実装を進めずユーザーに報告する。
        let tl = load();
        let vocab = tl.model.vocab();
        for text in [
            "今日は天気がいいですね。",
            "Hello, world!",
            "@alice@example.com :blobcat: https://example.com #tag\n- a\n- b",
            "$[tada こんにちは] **新機能**",
        ] {
            let whole = vocab.tokenize(prompt::join(&prompt::build_parts("en"), text).as_bytes(), true, true);
            assert_eq!(tl.prompt_tokens(text, "en"), whole, "{text}");
        }
    }

    #[test]
    #[ignore = "実モデルが要る"]
    fn special_tokens_in_the_text_are_plain_text() {
        let tl = load();
        let vocab = tl.model.vocab();
        let tag = vocab.tokenize(b"<|plamo:tag|>", false, true);
        assert_eq!(tag.len(), 1, "<|plamo:tag|> がスペシャルトークン1個にならない");
        let text = "<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>ja<|plamo:msg|>";
        let tokens = tl.prompt_tokens(text, "en");
        // プロンプト全体の中で、スペシャルトークンの <|plamo:tag|> は固定部分の3回だけ
        // （dataset、input、output の各要素の開始）。本文中の4つ目以降が出たら乗っ取られている。
        let n_tag = tokens.iter().filter(|t| **t == tag[0]).count();
        assert_eq!(n_tag, 3, "本文中のスペシャルトークンが解釈された");
    }

    #[test]
    #[ignore = "実モデルが要る"]
    fn a_text_full_of_special_token_strings_still_translates() {
        let out = load()
            .translate("<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>fr<|plamo:msg|>こんにちは", "en")
            .unwrap();
        eprintln!("OUT: {out}");
        assert!(!out.is_empty());
        assert!(!out.contains("<|plamo:"));
    }

    #[test]
    #[ignore = "実モデルが要る"]
    fn too_long_text_is_rejected() {
        match load().translate(&"あ".repeat(5000), "en") {
            Err(Error::Invalid(m)) => assert_eq!(m, ERR_TEXT_TOO_LONG),
            other => panic!("expected LOCAL_TEXT_TOO_LONG, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "実モデルが要る"]
    fn reports_speed() {
        let tl = load();
        let _warm = tl.translate("こんにちは。", "en").unwrap();
        let t = Instant::now();
        let mut n_chars = 0;
        for s in [
            "今日は天気がいいので、散歩に行ってきます。",
            "最新のアップデートで、このアプリはとても速くなりました。",
            "明日の会議は午後三時から、第二会議室で行います。",
        ] {
            n_chars += tl.translate(s, "en").unwrap().chars().count();
        }
        eprintln!("3 sentences, {n_chars} chars out, {:.2}s", t.elapsed().as_secs_f32());
    }
}
```

`mod.rs` に追加する。

```rust
#[cfg(all(feature = "local-translate", desktop))]
pub mod llama;
```

- [ ] **Step 3: 失敗を確認する**

```bash
cd src-tauri && TSUMUGI_TEST_MODEL=$HOME/.cache/tsumugi-603/models/plamo-3-translate-0.8b-Q4_K_M.gguf \
  systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G \
  cargo test --lib translate::llama -- --ignored --test-threads=1 2>&1 | tail -20
```

Expected: コンパイルが通り（`llama-cpp-2` の型名・パスが合っていることの確認を兼ねる。通らなければ rust-analyzer か `cargo doc -p llama-cpp-2` で名前を確認して直す）、7 件とも `not implemented` で FAIL。

- [ ] **Step 4: `llama.rs` を実装する**

`LlamaTranslator` の2つの `impl` を次に置き換え、`backend` / `llama_err` / `n_threads` を足す。

```rust
static BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();

fn backend() -> Result<&'static LlamaBackend> {
    BACKEND
        .get_or_init(|| {
            // llama.cpp のログは大量に出るうえ、処理中の文を含みうるので、すべて捨てる。
            send_logs_to_tracing(LogOptions::default().with_logs_enabled(false));
            LlamaBackend::init().map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| Error::Invalid(format!("llama backend: {e}")))
}

/// llama.cpp 側の失敗。本文は含めない。
fn llama_err(what: &str, e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("local model {what}: {e}"))
}

/// 割り当てられたコア数を超えるスレッドは極端に遅くなる（Android 実機の spike で 0.4 tok/s まで落ちた）ので、
/// `available_parallelism` を上限にする。
fn n_threads() -> i32 {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(4) as i32
}

impl LlamaTranslator {
    pub fn load(path: &Path) -> Result<Self> {
        let backend = backend()?;
        let params = LlamaModelParams::default().with_n_gpu_layers(0);
        let model = LlamaModel::load_from_file(backend, path, &params).map_err(|e| llama_err("load", e))?;
        Ok(Self { model })
    }

    /// プロンプトのトークン列。固定部分はスペシャルトークンとして、本文はただの文字として解釈する。
    pub fn prompt_tokens(&self, text: &str, plamo_lang: &str) -> Vec<LlamaToken> {
        let vocab = self.model.vocab();
        let parts = prompt::build_parts(plamo_lang);
        // 先頭の固定部分には BOS を付け、本文には付けず・スペシャルトークンも解釈させない。
        let mut tokens = vocab.tokenize(parts.prefix.as_bytes(), true, true);
        tokens.extend(vocab.tokenize(text.as_bytes(), false, false));
        tokens.extend(vocab.tokenize(parts.suffix.as_bytes(), false, true));
        tokens
    }
}

impl LocalTranslator for LlamaTranslator {
    fn translate(&self, text: &str, plamo_lang: &str) -> Result<String> {
        let backend = backend()?;
        let tokens = self.prompt_tokens(text, plamo_lang);
        if tokens.len() > MAX_PROMPT_TOKENS {
            return Err(invalid(ERR_TEXT_TOO_LONG));
        }

        // 翻訳ごとに新しい context を作る（作るコストは小さく、前の翻訳の KV キャッシュを引きずらない）。
        let threads = n_threads();
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let mut ctx = self.model.new_context(backend, ctx_params).map_err(|e| llama_err("context", e))?;

        let mut batch = LlamaBatch::new(MAX_PROMPT_TOKENS, 1);
        let last = tokens.len() - 1;
        for (i, tok) in tokens.iter().enumerate() {
            batch.add(*tok, i as i32, &[0], i == last).map_err(|e| llama_err("batch", e))?;
        }
        ctx.decode(&mut batch).map_err(|e| llama_err("decode", e))?;

        let vocab = self.model.vocab();
        let mut sampler = LlamaSampler::greedy();
        let mut out: Vec<u8> = Vec::new();
        let mut n_cur = tokens.len() as i32;
        let max_new = N_CTX as usize - tokens.len();
        for _ in 0..max_new {
            let tok = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(tok);
            if vocab.is_eog(tok) {
                break;
            }
            let piece = vocab.token_to_piece(tok, true, None);
            if prompt::is_stop(&String::from_utf8_lossy(&piece)) {
                break;
            }
            out.extend_from_slice(&piece);
            batch.clear();
            batch.add(tok, n_cur, &[0], true).map_err(|e| llama_err("batch", e))?;
            n_cur += 1;
            ctx.decode(&mut batch).map_err(|e| llama_err("decode", e))?;
        }
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }
}
```

- [ ] **Step 5: 実モデルのテストを通す**

```bash
cd src-tauri && TSUMUGI_TEST_MODEL=$HOME/.cache/tsumugi-603/models/plamo-3-translate-0.8b-Q4_K_M.gguf \
  systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G \
  cargo test --lib translate::llama -- --ignored --test-threads=1 --nocapture 2>&1 | tail -30
```

Expected: 7 件とも ok。

**`separate_tokenization_matches_whole_string_tokenization_for_plain_text` が失敗したら、実装を進めず、トークン列の差（どの文で、どこが違うか）をユーザーに報告して止まる**（本文を別にトークン化する方式の前提が崩れており、spec の変更が要る）。

- [ ] **Step 6: 速度を確認する**

```bash
cd src-tauri && TSUMUGI_TEST_MODEL=$HOME/.cache/tsumugi-603/models/plamo-3-translate-0.8b-Q4_K_M.gguf \
  systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G \
  cargo test --lib translate::llama::tests::reports_speed -- --ignored --nocapture 2>&1 | grep -E '^3 sentences'
```

Expected: 3 文の翻訳が合計で約 1〜2 秒（spike は OpenMP ありで約 54 tok/s、1件 0.15〜0.6 秒）。**3 文で 6 秒を超えるなら**（OpenMP なしで大きく遅い）、`Cargo.toml` の features に `openmp` を足す案をユーザーに報告して止まる（spec の変更）。

- [ ] **Step 7: `with_models_dir` を足す**

`manager.rs` の `impl LocalTranslate` に追加する。

```rust
    /// モデルの置き場（`app_data_dir/models`）を指定して作る。対応していないビルド（`local-translate` 無効、
    /// モバイル）では `unconfigured()` と同じ。
    pub fn with_models_dir(models_dir: std::path::PathBuf) -> Self {
        #[cfg(all(feature = "local-translate", desktop))]
        {
            Self::new(Arc::new(move || {
                let path = super::model_store::model_path(&models_dir);
                super::model_store::check_ready(&path)?;
                let translator = super::llama::LlamaTranslator::load(&path)?;
                Ok(Arc::new(translator) as Arc<dyn LocalTranslator>)
            }))
        }
        #[cfg(not(all(feature = "local-translate", desktop)))]
        {
            let _ = models_dir;
            Self::unconfigured()
        }
    }
```

そのテストを `manager.rs` の `tests` に足す（モデル未配置のとき `LOCAL_MODEL_NOT_READY`、ロードを試みずに返る）。

```rust
    #[tokio::test]
    async fn with_models_dir_reports_not_ready_when_the_model_is_missing() {
        let dir = std::env::temp_dir().join(format!("tsumugi_test_{}", uuid::Uuid::new_v4()));
        let lt = LocalTranslate::with_models_dir(dir);
        let expected = if super::super::is_supported() { "LOCAL_MODEL_NOT_READY" } else { "LOCAL_UNSUPPORTED" };
        assert_invalid(lt.translate("a", "en").await, expected);
    }
```

- [ ] **Step 8: ビルド構成の確認とコミット**

```bash
cd src-tauri
cargo test --lib translate:: 2>&1 | tail -4
cargo check --lib --no-default-features --features custom-protocol 2>&1 | tail -3
git add src-tauri/src/translate
git commit -m "feat(translate): llama.cppによるローカル翻訳エンジンを追加 (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: 1つ目は `ok. 22 passed; 0 failed; 7 ignored`、2つ目は feature 無効でも `Finished`（`llama.rs` がコンパイルされない構成）。

---

### Task 5: 型の変更（`translate_engine` と `Translation.source_lang`）

**Files:**
- Modify: `src-tauri/src/domain/ui.rs`
- Modify: `src-tauri/src/domain/note.rs:165-171`
- Modify: `src-tauri/src/api/notes.rs`（テストの期待値）
- Modify（生成）: `frontend/src/bindings/tauri.gen.ts`
- Modify: `frontend/src/ui/NoteCard.svelte`、`frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Produces: `UiPrefs.translate_engine: String`（TS: `translateEngine?: string`）、`Translation { source_lang: Option<String>, text: String }`（TS: `sourceLang: string | null`）。

- [ ] **Step 1: `UiPrefs` の失敗するテストを書く**

`src-tauri/src/domain/ui.rs` の `translate_target_lang_round_trips_camel_case` テストの後ろに追加する。

```rust
    #[test]
    fn translate_engine_defaults_to_server_for_legacy_json() {
        // translate_engine 追加前に保存された JSON も読めること（#[serde(default)]）。
        let v: UiPrefs = serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320}"#).unwrap();
        assert_eq!(v.translate_engine, "server");
        assert_eq!(UiPrefs::default().translate_engine, "server");
    }

    #[test]
    fn translate_engine_round_trips_camel_case() {
        let v: UiPrefs =
            serde_json::from_str(r#"{"theme":"dark","defaultColumnWidth":320,"translateEngine":"local"}"#).unwrap();
        assert_eq!(v.translate_engine, "local");
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["translateEngine"], "local");
    }
```

あわせて、既存の全フィールドのリテラル（`translate_target_lang: "en".into(),` を持つテスト）に `translate_engine: "local".into(),` を足す。

- [ ] **Step 2: 失敗を確認する**

```bash
cd src-tauri && cargo test --lib domain::ui 2>&1 | tail -10
```

Expected: コンパイルエラー（`UiPrefs` に `translate_engine` が無い）。

- [ ] **Step 3: `UiPrefs` を変更する**

`translate_target_lang` のフィールド（`pub translate_target_lang: String,`）の直後に追加する。

```rust
    /// ノート翻訳のエンジン（Issue #603）。`"server"`（Misskey サーバーの翻訳、既定）か `"local"`（端末内の
    /// PLaMo 3 Translate）。未知の値は翻訳時に `"server"` として扱う（`commands::note::effective_translate_engine`）。
    #[serde(default = "default_translate_engine")]
    pub translate_engine: String,
```

`default_translate_target_lang` の関数の後ろに追加する。

```rust
fn default_translate_engine() -> String {
    "server".into()
}
```

`impl Default for UiPrefs` の `translate_target_lang: default_translate_target_lang(),` の次の行に追加する。

```rust
            translate_engine: default_translate_engine(),
```

- [ ] **Step 4: `Translation` を変更する**

`src-tauri/src/domain/note.rs` の `Translation` を置き換える。

```rust
/// 翻訳結果（Issue #440、#603）。`source_lang` は検出された翻訳元言語コード。ローカル翻訳（PLaMo 3 Translate）は
/// 翻訳元言語を内部で判別するが、結果を返さないので `None`。
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub source_lang: Option<String>,
    pub text: String,
}
```

`src-tauri/src/api/notes.rs` の `translate_note_parses_200` の期待値を変更する。

```rust
            Some(crate::domain::Translation {
                source_lang: Some("EN".into()),
                text: "こんにちは".into()
            })
```

- [ ] **Step 5: Rust のテストを通し、バインディングを再生成する**

```bash
cd src-tauri && cargo test 2>&1 | tail -6
git diff --stat ../frontend/src/bindings/tauri.gen.ts
```

Expected: `cargo test` が全件 ok（`generates_frontend_bindings` が `tauri.gen.ts` を再生成する）。差分に `translateEngine?: string` と `sourceLang: string | null` が出る。`frontend/dist` が無くてテストが落ちる場合は、先に `cd frontend && pnpm build` する。

- [ ] **Step 6: フロントの失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` の「ノート翻訳」の `describe` 内、最後のテストの前に追加する。

```ts
  it("翻訳元言語が分からない(null)ときは、言語を出さず「翻訳」とだけ表示する", async () => {
    setup(true, async () => ({ sourceLang: null, text: "Hello" }));
    const { getByLabelText, findByText, getByTestId } = render(NoteCard, {
      props: { note: makeNote({ text: "こんにちは" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    await (await findByText("翻訳")).click();
    await waitFor(() => expect(getByTestId("note-translation").textContent).toContain("Hello"));
    expect(getByTestId("note-translation").textContent).not.toContain("null");
    expect(getByTestId("note-translation").textContent).not.toContain("から翻訳");
  });
```

```bash
cd frontend && pnpm check 2>&1 | tail -15
```

Expected: `NoteCard.svelte` の `sourceLang: string` と、bindings の `string | null` が合わず、型エラー。

- [ ] **Step 7: `NoteCard.svelte` を変更する**

`type TranslationState` の `done` の行を変更する。

```ts
    | { status: "done"; sourceLang: string | null; text: string }
```

結果欄の見出し（`{translation.sourceLang} から翻訳` の行）を置き換える。

```svelte
              <div class="mb-0.5 text-xs text-muted-foreground">{translation.sourceLang ? `${translation.sourceLang} から翻訳` : "翻訳"}</div>
```

- [ ] **Step 8: フロントのテストと型検査を通す**

```bash
cd frontend && pnpm check 2>&1 | tail -4 && npx vitest run src/ui/NoteCard.test.ts 2>&1 | tail -6
```

Expected: `svelte-check found 0 errors`、NoteCard のテストがすべて passed。

- [ ] **Step 9: コミットする**

```bash
git add src-tauri/src/domain src-tauri/src/api/notes.rs frontend/src/bindings/tauri.gen.ts frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat(translate): 翻訳エンジン設定と翻訳元言語の省略に対応する型を追加 (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 翻訳エンジンの振り分けと配線

**Files:**
- Modify: `src-tauri/src/state.rs`（`local_translate` フィールドと `with_local_translate`）
- Modify: `src-tauri/src/commands/note.rs`（`translate_note` / `get_translator_available`）
- Modify: `src-tauri/src/lib.rs`（起動時の配線）
- Modify（生成）: `frontend/src/bindings/tauri.gen.ts`
- Modify: `frontend/src/lib/store.svelte.ts`、`frontend/src/ui/NoteCard.svelte`、`frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: Task 3 の `LocalTranslate`（`translate`、`unconfigured`、`with_models_dir`）、`spawn_idle_sweeper`、`IDLE_UNLOAD`、`SWEEP_INTERVAL`、Task 2 の `lang::to_plamo_lang`、Task 5 の `UiPrefs.translate_engine` / `Translation.source_lang: Option<String>`。
- Produces:
  - `AppState.local_translate: Arc<LocalTranslate>`、`AppState::with_local_translate(self, Arc<LocalTranslate>) -> Self`
  - `commands::note::translate_note(state, account_id: String, note_id: String, text: String) -> Result<Option<Translation>>`
  - `commands::note::{translate_note_core, get_translator_available_core}`（`&AppState` を取る、テスト用に `pub(crate)`）
  - フロント: `app.translateNote(accountId: string, noteId: string, text: string)`

- [ ] **Step 1: `commands/note.rs` の失敗するテストを書く**

`src-tauri/src/commands/note.rs` の `tests` の `effective_translate_lang_falls_back_to_ja_when_blank` の後ろに追加する。

```rust
    #[test]
    fn effective_translate_engine_defaults_to_server() {
        let mut prefs = crate::domain::UiPrefs::default();
        assert_eq!(effective_translate_engine(&prefs), TranslateEngine::Server);
        for (v, want) in [
            ("server", TranslateEngine::Server),
            ("local", TranslateEngine::Local),
            (" LOCAL ", TranslateEngine::Local),
            ("", TranslateEngine::Server),
            ("bogus", TranslateEngine::Server),
        ] {
            prefs.translate_engine = v.into();
            assert_eq!(effective_translate_engine(&prefs), want, "{v:?}");
        }
    }

    /// `"[{lang}] {text}"` を返す偽のローカル翻訳器。
    struct Echo;
    impl crate::translate::engine::LocalTranslator for Echo {
        fn translate(&self, text: &str, plamo_lang: &str) -> Result<String> {
            Ok(format!("[{plamo_lang}] {text}"))
        }
    }

    fn state_with_translate_engine(engine: &str, target_lang: &str) -> AppState {
        let state = AppState::new_for_test(crate::store::SettingsStore::new_in_memory()).with_local_translate(
            std::sync::Arc::new(crate::translate::LocalTranslate::new(std::sync::Arc::new(|| {
                Ok(std::sync::Arc::new(Echo) as std::sync::Arc<dyn crate::translate::engine::LocalTranslator>)
            }))),
        );
        let mut prefs = crate::domain::UiPrefs::default();
        prefs.translate_engine = engine.into();
        prefs.translate_target_lang = target_lang.into();
        state.settings.save_ui(&prefs).unwrap();
        state
    }

    #[tokio::test]
    async fn local_engine_translates_the_given_text_without_an_account() {
        let state = state_with_translate_engine("local", "ja");
        let t = translate_note_core(&state, "no-such-account", "n1", "hello").await.unwrap();
        assert_eq!(t, Some(Translation { source_lang: None, text: "[ja] hello".into() }));
    }

    #[tokio::test]
    async fn local_engine_maps_the_target_language_to_plamo() {
        let state = state_with_translate_engine("local", "zh");
        let t = translate_note_core(&state, "a", "n1", "hello").await.unwrap().unwrap();
        assert_eq!(t.text, "[zh-hans] hello");
        let state = state_with_translate_engine("local", "en-US");
        let t = translate_note_core(&state, "a", "n1", "こんにちは").await.unwrap().unwrap();
        assert_eq!(t.text, "[en] こんにちは");
    }

    #[tokio::test]
    async fn local_engine_rejects_unsupported_target_languages() {
        let state = state_with_translate_engine("local", "pt");
        match translate_note_core(&state, "a", "n1", "hello").await {
            Err(Error::Invalid(m)) => assert_eq!(m, "LOCAL_LANG_UNSUPPORTED"),
            other => panic!("expected LOCAL_LANG_UNSUPPORTED, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn local_engine_returns_none_for_blank_text() {
        let state = state_with_translate_engine("local", "ja");
        for blank in ["", "   ", "\n\t"] {
            assert_eq!(translate_note_core(&state, "a", "n1", blank).await.unwrap(), None, "{blank:?}");
        }
    }

    #[tokio::test]
    async fn server_engine_uses_notes_translate_and_never_the_local_model() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notes/translate"))
            .and(body_partial_json(serde_json::json!({ "noteId": "n1", "targetLang": "ja" })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "sourceLang": "EN", "text": "こんにちは" })),
            )
            .mount(&mock)
            .await;
        // 未知の値("bogus")はサーバー翻訳として扱う。ローカルの偽翻訳器は呼ばれないはず。
        let state = state_with_translate_engine("bogus", "ja");
        state.register_test_account("acc1");
        state.set_test_api_base(mock.uri());

        let t = translate_note_core(&state, "acc1", "n1", "hello").await.unwrap();
        assert_eq!(t, Some(Translation { source_lang: Some("EN".into()), text: "こんにちは".into() }));
    }

    #[tokio::test]
    async fn translator_available_follows_the_engine() {
        // ローカル: アカウントやサーバーに関係なく、このビルドの対応状況。
        let state = state_with_translate_engine("local", "ja");
        assert_eq!(
            get_translator_available_core(&state, "none").await.unwrap(),
            crate::translate::is_supported()
        );
        // サーバー: 従来どおり、サーバーごとにキャッシュした値。
        let state = state_with_translate_engine("server", "ja");
        state.set_translator_available("acc1", true);
        assert!(get_translator_available_core(&state, "acc1").await.unwrap());
        state.set_translator_available("acc2", false);
        assert!(!get_translator_available_core(&state, "acc2").await.unwrap());
    }
```

- [ ] **Step 2: 失敗を確認する**

```bash
cd src-tauri && cargo test --lib commands::note 2>&1 | tail -12
```

Expected: コンパイルエラー（`effective_translate_engine` / `TranslateEngine` / `translate_note_core` / `with_local_translate` が無い）。

- [ ] **Step 3: `AppState` に `local_translate` を足す**

`src-tauri/src/state.rs` の `AppState` で、`pub scheduled_posts` ... `pub scheduler_wakeup` の後に追加する。

```rust
    /// ローカル翻訳（PLaMo 3 Translate、Issue #603）。モデルの置き場は起動時に `with_local_translate` で渡す。
    /// `AppState::new*` のシグネチャを変えずに済むよう、既定は「非対応」。
    pub local_translate: std::sync::Arc<crate::translate::LocalTranslate>,
```

`new_with_sound` の `Self { ... }` の `scheduler_wakeup: ...,` の次の行に追加する。

```rust
            local_translate: std::sync::Arc::new(crate::translate::LocalTranslate::unconfigured()),
```

`with_scheduled_posts` の後ろにメソッドを足す。

```rust
    /// ローカル翻訳のマネージャーを差し替える(起動時にモデルの置き場つきのものへ)。
    pub fn with_local_translate(mut self, local_translate: std::sync::Arc<crate::translate::LocalTranslate>) -> Self {
        self.local_translate = local_translate;
        self
    }
```

- [ ] **Step 4: `commands/note.rs` の振り分けを実装する**

`effective_translate_lang` の後ろに追加する。

```rust
/// 翻訳エンジン（Issue #603）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranslateEngine {
    Server,
    Local,
}

/// 設定の翻訳エンジン。未知の値・空は `Server`（設定を文字列で持つのは、将来の値の追加やダウングレードで
/// `UiPrefs` 全体が読めなくならないようにするため）。
pub(crate) fn effective_translate_engine(prefs: &UiPrefs) -> TranslateEngine {
    if prefs.translate_engine.trim().eq_ignore_ascii_case("local") {
        TranslateEngine::Local
    } else {
        TranslateEngine::Server
    }
}
```

`get_translator_available` と `translate_note`（`#[tauri::command]` つきの2つの関数）を次に置き換える。

```rust
/// NoteMenu の「翻訳」項目を出すか（Issue #440、#603）。サーバー翻訳のときは、接続先サーバーで翻訳が使えるとき
/// だけ true。ローカル翻訳のときは、このビルド・プラットフォームが対応しているとき true（モデル未取得でも true:
/// 押したときに「モデルを取得してください」と案内するほうが、項目が消えるより分かりやすい）。
pub(crate) async fn get_translator_available_core(state: &AppState, account_id: &str) -> Result<bool> {
    match effective_translate_engine(&state.settings.load_ui()?) {
        TranslateEngine::Server => translator_available_for(state, account_id).await,
        TranslateEngine::Local => Ok(crate::translate::is_supported()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_translator_available(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<bool> {
    get_translator_available_core(&state, &account_id).await
}

/// ノートを `UiPrefs.translate_target_lang` へ翻訳する（Issue #440、#603）。結果なしは `None`。
/// - サーバー翻訳: `notes/translate`。`text` は使わない。サーバーで翻訳が未設定なら `Error::Api`
///   （message に `UNAVAILABLE` を含む）。
/// - ローカル翻訳: `text` を端末内のモデルで翻訳する。サーバーへは何も送らない。`text` が空白のみなら `None`。
///   失敗は `Error::Invalid` の `LOCAL_MODEL_NOT_READY` / `LOCAL_TEXT_TOO_LONG` / `LOCAL_LANG_UNSUPPORTED` /
///   `LOCAL_UNSUPPORTED`。
pub(crate) async fn translate_note_core(
    state: &AppState,
    account_id: &str,
    note_id: &str,
    text: &str,
) -> Result<Option<Translation>> {
    let prefs = state.settings.load_ui()?;
    let lang = effective_translate_lang(&prefs);
    match effective_translate_engine(&prefs) {
        TranslateEngine::Server => {
            let client = state.client_for(account_id)?;
            api_translate_note(&client, note_id, &lang).await
        }
        TranslateEngine::Local => {
            if text.trim().is_empty() {
                return Ok(None);
            }
            let plamo_lang = crate::translate::lang::to_plamo_lang(&lang)
                .ok_or_else(|| crate::translate::invalid(crate::translate::ERR_LANG_UNSUPPORTED))?;
            let out = state.local_translate.translate(text, plamo_lang).await?;
            Ok(Some(Translation { source_lang: None, text: out }))
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn translate_note(
    state: State<'_, AppState>,
    account_id: String,
    note_id: String,
    text: String,
) -> Result<Option<Translation>> {
    translate_note_core(&state, &account_id, &note_id, &text).await
}
```

- [ ] **Step 5: 起動時の配線を足す**

`src-tauri/src/lib.rs` の `app.manage(AppState::new(...)...)` を置き換える。

```rust
            // ローカル翻訳(Issue #603)。モデルは app_data_dir/models/ に置く。対応していないビルドでは「非対応」を返す。
            let local_translate = std::sync::Arc::new(translate::LocalTranslate::with_models_dir(
                app.path().app_data_dir().expect("no app data dir").join("models"),
            ));
            translate::manager::spawn_idle_sweeper(
                local_translate.clone(),
                translate::IDLE_UNLOAD,
                translate::SWEEP_INTERVAL,
            );
            app.manage(
                AppState::new(Box::new(KeyringStore), settings, drafts, cache, cache_dir.clone())
                    .with_scheduled_posts(scheduled_posts)
                    .with_local_translate(local_translate),
            );
```

- [ ] **Step 6: Rust のテストを通し、バインディングを再生成する**

```bash
cd src-tauri && cargo test 2>&1 | tail -6
git diff --stat ../frontend/src/bindings/tauri.gen.ts
```

Expected: 全件 ok。`tauri.gen.ts` の `translateNote` が `(accountId, noteId, text)` になる。

- [ ] **Step 7: フロントの配線を変える（失敗するテストを先に）**

`frontend/src/ui/NoteCard.test.ts` で、`translateSpy` の呼び出しの期待値を3引数に変える。

```ts
    expect(translateSpy).toHaveBeenCalledWith("acc1", "n-tr-1", "hello");
```

```ts
    await waitFor(() => expect(translateSpy).toHaveBeenCalledWith("acc1", "n-target", "original"));
```

```bash
cd frontend && pnpm check 2>&1 | tail -10
```

Expected: `store.svelte.ts` の `translateNote` が2引数で、バインディングの3引数と合わず、型エラー。

`frontend/src/lib/store.svelte.ts` の `translateNote` を置き換える。

```ts
  /// ノートを翻訳する(Issue #440、#603)。結果なしは null。`text` はローカル翻訳が使う本文
  /// (サーバー翻訳は使わない)。失敗は throw するだけで、エラーモーダルは出さない
  /// (NoteCard の翻訳ブロックがインラインで表示するため)。
  async translateNote(accountId: string, noteId: string, text: string) {
    return await unwrapAcc(accountId, commands.translateNote(accountId, noteId, text));
  }
```

`frontend/src/ui/NoteCard.svelte` の `startTranslate` の呼び出しを変える（`inner` は純リノートのときリノート先）。

```ts
      const r = await app.translateNote(accountId, inner.id, inner.text ?? "");
```

- [ ] **Step 8: フロントのテストと型検査を通し、コミットする**

```bash
cd frontend && pnpm check 2>&1 | tail -4 && npx vitest run src/ui/NoteCard.test.ts 2>&1 | tail -6
cd .. && git add src-tauri/src frontend/src
git commit -m "feat(translate): 翻訳エンジンの振り分けとローカル翻訳の配線を追加 (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected: `0 errors`、NoteCard のテストがすべて passed。

---

### Task 7: フロントエンドのエンジン別表記とエラー表示

**Files:**
- Modify: `frontend/src/ui/NoteCard.svelte`
- Modify: `frontend/src/ui/NoteMenu.svelte`
- Test: `frontend/src/ui/NoteCard.test.ts`

**Interfaces:**
- Consumes: `app.ui.translateEngine`（`"server"` | `"local"` | 未設定）、Task 6 の `app.translateNote(accountId, noteId, text)`、Rust のエラー文言 `LOCAL_MODEL_NOT_READY` / `LOCAL_TEXT_TOO_LONG` / `LOCAL_LANG_UNSUPPORTED` / `LOCAL_UNSUPPORTED`（`unwrapAcc` が `AppError(message)` を投げるので、`String(e)` に文言が入る）。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/NoteCard.test.ts` の「ノート翻訳」の `describe` で、`afterEach` にエンジンの復元を足す。

```ts
  afterEach(() => {
    app.accounts.length = 0;
    app.ui = { ...app.ui, translateEngine: "server" };
    availSpy?.mockRestore();
    translateSpy?.mockRestore();
    availSpy = null;
    translateSpy = null;
  });
```

describe の末尾に追加する。

```ts
  it("ローカル翻訳では、メニューも結果欄も「PLaMo で翻訳」と表示する", async () => {
    app.ui = { ...app.ui, translateEngine: "local" };
    setup(true, async () => ({ sourceLang: null, text: "Hello" }));
    const { getByLabelText, findByText, getByTestId, queryByText } = render(NoteCard, {
      props: { note: makeNote({ text: "こんにちは" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    // サーバー翻訳の「翻訳」項目は出ない。
    await findByText("PLaMo で翻訳");
    expect(queryByText("翻訳")).toBeNull();
    await (await findByText("PLaMo で翻訳")).click();
    await waitFor(() => expect(getByTestId("note-translation").textContent).toContain("Hello"));
    expect(getByTestId("note-translation").textContent).toContain("PLaMo で翻訳");
  });

  it("サーバー翻訳に切り替えると、元の「翻訳」と「{言語} から翻訳」に戻る", async () => {
    app.ui = { ...app.ui, translateEngine: "server" };
    setup(true);
    const { getByLabelText, findByText, getByTestId, queryByText } = render(NoteCard, {
      props: { note: makeNote({ text: "hello" }), accountId: "acc1" },
    });
    await getByLabelText("その他").click();
    expect(queryByText("PLaMo で翻訳")).toBeNull();
    await (await findByText("翻訳")).click();
    await waitFor(() => expect(getByTestId("note-translation").textContent).toContain("EN から翻訳"));
  });

  it("ローカル翻訳のエラーを種類ごとの文言で表示する", async () => {
    app.ui = { ...app.ui, translateEngine: "local" };
    const cases: [string, string][] = [
      ["LOCAL_MODEL_NOT_READY", "PLaMo のモデルが未取得です。設定でダウンロードしてください"],
      ["LOCAL_TEXT_TOO_LONG", "ノートが長すぎるため翻訳できません"],
      ["LOCAL_LANG_UNSUPPORTED", "翻訳先の言語に PLaMo は対応していません"],
      ["LOCAL_UNSUPPORTED", "この環境では PLaMo による翻訳は使えません"],
    ];
    setup(true, async () => {
      throw new Error("x");
    });
    for (const [code, text] of cases) {
      translateSpy!.mockImplementation((async () => {
        throw new Error(code);
      }) as never);
      const r = render(NoteCard, { props: { note: makeNote({ text: "こんにちは" }), accountId: "acc1" } });
      await r.getByLabelText("その他").click();
      await (await r.findByText("PLaMo で翻訳")).click();
      await waitFor(() => expect(r.getByTestId("note-translation").textContent).toContain(text));
      cleanup();
    }
  });
```

```bash
cd frontend && npx vitest run src/ui/NoteCard.test.ts 2>&1 | tail -20
```

Expected: 新しい3件が FAIL（メニューが「翻訳」のまま、エラー文言が「翻訳に失敗しました」）。

- [ ] **Step 2: `NoteMenu.svelte` の項目名を変える**

`frontend/src/ui/NoteMenu.svelte` の「翻訳」ボタンの表示文字（`<Languages size={16} />` の次の行）を置き換える。

```svelte
      {app.ui.translateEngine === "local" ? "PLaMo で翻訳" : "翻訳"}
```

- [ ] **Step 3: `NoteCard.svelte` を変える**

`type TranslationState` を、エラーの種類と、エンジンを持つ形に置き換える。

```ts
  type TranslateErrorKind =
    | "unavailable"
    | "modelNotReady"
    | "tooLong"
    | "langUnsupported"
    | "localUnsupported"
    | "other";
  type TranslationState =
    | { status: "loading" }
    | { status: "done"; sourceLang: string | null; text: string; local: boolean }
    | { status: "empty" }
    | { status: "error"; kind: TranslateErrorKind };

  // Rust は専用のエラー型を持たず、Error::Invalid / Error::Api の文言で区別している(#440、#603)。
  function translateErrorKind(e: unknown): TranslateErrorKind {
    const m = String(e);
    if (m.includes("LOCAL_MODEL_NOT_READY")) return "modelNotReady";
    if (m.includes("LOCAL_TEXT_TOO_LONG")) return "tooLong";
    if (m.includes("LOCAL_LANG_UNSUPPORTED")) return "langUnsupported";
    if (m.includes("LOCAL_UNSUPPORTED")) return "localUnsupported";
    if (m.includes("UNAVAILABLE")) return "unavailable";
    return "other";
  }
  const TRANSLATE_ERROR_TEXT: Record<TranslateErrorKind, string> = {
    unavailable: "このサーバーは翻訳に対応していません",
    modelNotReady: "PLaMo のモデルが未取得です。設定でダウンロードしてください",
    tooLong: "ノートが長すぎるため翻訳できません",
    langUnsupported: "翻訳先の言語に PLaMo は対応していません",
    localUnsupported: "この環境では PLaMo による翻訳は使えません",
    other: "翻訳に失敗しました",
  };
```

`startTranslate` を置き換える。

```ts
  async function startTranslate() {
    if (!accountId) return;
    const seq = ++translateSeq;
    // 翻訳中に設定が切り替わっても、この翻訳を始めたときのエンジンで表示する。
    const local = app.ui.translateEngine === "local";
    translation = { status: "loading" };
    try {
      const r = await app.translateNote(accountId, inner.id, inner.text ?? "");
      if (seq !== translateSeq) return;
      translation = r
        ? { status: "done", sourceLang: r.sourceLang ?? null, text: r.text, local }
        : { status: "empty" };
    } catch (e) {
      if (seq !== translateSeq) return;
      translation = { status: "error", kind: translateErrorKind(e) };
    }
  }
```

結果欄の見出しとエラー表示を置き換える。

```svelte
              <div class="mb-0.5 text-xs text-muted-foreground">{translation.local ? "PLaMo で翻訳" : translation.sourceLang ? `${translation.sourceLang} から翻訳` : "翻訳"}</div>
```

```svelte
              <span class="text-destructive">{TRANSLATE_ERROR_TEXT[translation.kind]}</span>
```

（`{translation.unavailable ? "このサーバーは..." : "翻訳に失敗しました"}` の行を、2つ目に置き換える。）

- [ ] **Step 4: テストと型検査を通す**

```bash
cd frontend && pnpm check 2>&1 | tail -4 && npx vitest run src/ui/NoteCard.test.ts 2>&1 | tail -8 && pnpm test 2>&1 | tail -6
```

Expected: `0 errors`、NoteCard の全テストと、フロント全体のテストが passed。

- [ ] **Step 5: コミットする**

```bash
git add frontend/src/ui/NoteCard.svelte frontend/src/ui/NoteMenu.svelte frontend/src/ui/NoteCard.test.ts
git commit -m "feat(translate): 翻訳のメニューと結果欄をエンジン別に表示する (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: ドキュメントと最終確認

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/superpowers/specs/2026-10-02-note-translate-design.md`

- [ ] **Step 1: `CLAUDE.md` に構成を追記する**

`### src-tauri/src layout` の `scheduler.rs` の項目の後ろに追加する。

```markdown
- `translate/` — ローカル翻訳（PLaMo 3 Translate、Issue #603、設計は `docs/superpowers/specs/2026-11-14-local-translate-plamo-design.md`）。`prompt.rs`（PFML3 プロンプト、本文はスペシャルトークンとして解釈させず固定部分と別にトークン化する）、`lang.rs`（設定の言語コード→PLaMo の 53 言語。`pt` は非対応）、`model_store.rs`（`app_data_dir/models/` のモデル検査）、`engine.rs`（`LocalTranslator` trait）、`llama.rs`（`llama-cpp-2` の実装。`local-translate` feature が有効なデスクトップだけ）、`manager.rs`（遅延ロード・アイドル解放・直列化）。`translate_note` が `UiPrefs.translate_engine`（`"server"` | `"local"`、未知の値は `server`）で振り分け、ローカルの失敗は `Error::Invalid` の `LOCAL_MODEL_NOT_READY` / `LOCAL_TEXT_TOO_LONG` / `LOCAL_LANG_UNSUPPORTED` / `LOCAL_UNSUPPORTED`。実モデルのテストは `#[ignore]` で、`TSUMUGI_TEST_MODEL=<GGUFのパス> cargo test --lib translate::llama -- --ignored --nocapture`。`llama-cpp-2` は crates.io の 0.1.161 までは `plamo3` トークナイザが無く読めないため、git の rev を固定している（次のリリースで戻す）。
```

`## Commands` の `cd src-tauri && cargo test` の説明の近くに追加する。

```markdown
`llama.cpp`（`local-translate` feature、既定で有効）の C++ ビルドは、素の並列度だと OOM になる（16 コアで実際に発生）。`src-tauri/.cargo/config.toml` で cmake の並列度を抑えているが、メモリが厳しい環境では `CARGO_BUILD_JOBS=4 CMAKE_BUILD_PARALLEL_LEVEL=4` を付け、`systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G` で上限をかけると安全。ビルドの出力は `/tmp`（tmpfs = メモリ）に置かない。C++ を使わない開発では `--no-default-features --features custom-protocol` で `local-translate` を外せる。
```

（Task 1 Step 5 で `.cargo/config.toml` が効かなかった場合は、「`src-tauri/.cargo/config.toml` で…抑えているが」の部分を削り、`CARGO_BUILD_JOBS=4` を必須として書く。）

- [ ] **Step 2: 既存の翻訳 spec にリンクを追記する**

`docs/superpowers/specs/2026-10-02-note-translate-design.md` の「## スコープ外」の「外部翻訳 API の直接利用 / 両対応」の行の後ろに追記する。

```markdown
  （端末内のモデルによるローカル翻訳は、`2026-11-14-local-translate-plamo-design.md`（Issue #603）で追加した。サーバー翻訳とは設定で切り替える。）
```

- [ ] **Step 3: 全体の確認**

```bash
cd src-tauri && systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G cargo test 2>&1 | tail -6
TSUMUGI_TEST_MODEL=$HOME/.cache/tsumugi-603/models/plamo-3-translate-0.8b-Q4_K_M.gguf \
  systemd-run --user --scope --quiet -p MemoryMax=10G -p MemoryHigh=8G \
  cargo test --lib translate::llama -- --ignored --test-threads=1 2>&1 | tail -4
cargo check --lib --no-default-features --features custom-protocol 2>&1 | tail -2
cd ../frontend && pnpm check 2>&1 | tail -2 && pnpm test 2>&1 | tail -4
```

Expected: `cargo test` が全件 ok、実モデルのテストが 7 件 ok、`--no-default-features` が `Finished`、`pnpm check` が 0 errors、`pnpm test` が passed。

- [ ] **Step 4: 手動確認（実際の画面は段階2で行う）**

段階1には設定 UI が無いので、画面での通し確認は段階2で行う。ここでは、設定ファイルを直接書いた状態での翻訳が動くことを、デバッグブリッジ（`CLAUDE.md` の `debug_bridge.rs`）でなく、上の実モデルのテストと `translate_note_core` のテストで代替し、**画面での確認が未実施であること**を、最後の報告に書く。

- [ ] **Step 5: コミットし、後片付けをする**

```bash
git add CLAUDE.md docs/superpowers/specs/2026-10-02-note-translate-design.md
git commit -m "docs(translate): ローカル翻訳の構成とビルド時の注意をCLAUDE.mdに追記 (#603)" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
rm -rf "$HOME/.cache/tsumugi-603"
```

別の出力先（`CARGO_TARGET_DIR`）とテスト用のモデルを消す。ユーザーの `cargo tauri dev` などは、自分で起動したものでなければ触らない。**push と PR の作成（draft でないもの）は、ユーザーが依頼したときだけ行う。**

---

## Self-Review

**1. Spec coverage（段階1の範囲）:**

| spec | 担当 |
|---|---|
| §1 エンジン選択（`translate_engine`、`translate_note` に `text`、`get_translator_available`、`Translation.source_lang`） | Task 5、6 |
| §4 推論（構成、直列化、遅延ロード、アイドル解放、パラメータ、乗っ取り対策、停止、ログ、言語コード） | Task 2、3、4 |
| §5 エラー（3つ＋`LOCAL_UNSUPPORTED`） | Task 2、3、4、6、7 |
| §7 フロントのうち、メニューと結果欄の表記・エラー表示 | Task 5、7 |
| Android とビルド（Cargo feature、依存、CI、メモリ注意） | Task 1、8（Android 向けの依存は段階3） |
| テスト（Rust の `prompt` / 言語 / `manager` / 振り分け / `UiPrefs`、実モデル `#[ignore]`、bindings） | Task 2〜6 |
| ドキュメント（`CLAUDE.md`、既存 spec へのリンク） | Task 8 |
| §2 モデルの取得・§3 ライセンス同意・§6 通信種別・設定 UI・ユーザーガイド | 段階2・3（この plan の範囲外） |
| 「リリース前の確認事項」 | コード外。plan の範囲外 |

**2. Placeholder scan:** `TBD` / `TODO` / 「適切に」等の記述なし。`unimplemented!()` は TDD の「失敗するテスト」のための一時スタブで、各 Task の実装ステップで置き換える。

**3. Type consistency:** `LocalTranslator::translate(&self, &str, &str) -> Result<String>`、`LocalTranslate::{new, unconfigured, with_models_dir, translate, unload_if_idle, is_loaded}`、`spawn_idle_sweeper(Arc<LocalTranslate>, Duration, Duration)`、`prompt::{build_parts, join, is_stop, PromptParts}`、`lang::to_plamo_lang`、`translate_note_core(&AppState, &str, &str, &str)`、`get_translator_available_core(&AppState, &str)`、`app.translateNote(accountId, noteId, text)` を、定義した Task と使う Task で突き合わせた。

**4. Review Focus:** 5項目とも、担当 Task のテストに入っている（スペシャルトークン: Task 2・4、長すぎる: Task 3・4、非対応言語・地域付き: Task 2・6、モデル未配置: Task 3・4、同時要求: Task 3）。
