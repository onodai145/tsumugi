# clippy と ESLint を CI に追加する(Issue #707)

## 目的

今後の変更への機械的な歯止めを足す。今あるバグを探すことは目的にしない。アプリの挙動は変えない。

成功条件:

- clippy の警告を増やす PR と、ESLint の指摘を増やす PR は CI で落ちる。
- 手元でも push の前に同じ指摘に気付ける。
- 掃除の差分で挙動が変わらない(`cargo test`、`pnpm test`、E2E が通る)。
- Rust や lint の新版が出ただけでは、無関係な PR の CI が落ちない。

## 現状(2026-11-26、main = 650050e)

### clippy(1.100.0、既定の feature)

| 対象 | コマンド | 警告(重複を除く) |
|---|---|---|
| Linux | `cargo clippy --all-targets` | 64 件 / 31 ファイル(自動修正できるもの 19) |
| Android | `cargo clippy --lib --target aarch64-linux-android` | 31 件 / 22 ファイル |

- 両方を合わせると 71 件。Android にしか出ないのは、MCP 関連(デスクトップ専用)の rustc の `dead_code` 7 件。
- 多いものは `field_reassign_with_default`(19 件、すべてテストモジュール内)、`dead_code`(17 件)、`doc_lazy_continuation` と `err_expect`(各 6 件)。
- 指摘の種類と、確認した箇所を見る限り、挙動に影響するバグは無い。全箇所のコードは確認していない。
- Windows / macOS 専用のコードは数行で、今回の確認の対象にしない。

### ESLint(10.15.0、typescript-eslint 8.74.0、eslint-plugin-svelte 3.24.1)

下記「ESLint のルール」の設定で、62 件 / 31 ファイル(テストは 8 件)。所要時間は 45 秒。多いものは `no-floating-promises`(29 件)と `no-explicit-any`(10 件)。

typescript-eslint が対応する TypeScript は `<6.1.0`。今の `~6.0.2` は範囲内。

## 決定事項

### PR は 2 つに分ける

1. clippy(ブランチ `chore/issue-707-clippy`、この spec を含む。本文は `Refs #707`)
2. ESLint(1 のマージ後に main から切る。本文は `Closes #707`)

どちらも `.pre-commit-config.yaml` と `CLAUDE.md` を触るので、順番に進める。

### 掃除は挙動を変えない

指摘を直すと挙動が変わりうる箇所は、直さずに理由を添えて行単位で無効にする。掃除の途中でバグと分かったものは、この PR では直さず Issue にする。

## clippy

### ツールチェーンの固定

`src-tauri/clippy-toolchain` にバージョン番号(`1.100.0`)だけを書き、CI と pre-push の両方がこれを読む。`rust-toolchain.toml` は置かない(ビルドとテストのツールチェーンまで変わるため)。ビルドとテストは今のまま浮動の stable。

版を上げるのは手作業で、上げる PR で新しい指摘を直す。Dependabot は追従しない。

### lint の扱い

`-D warnings` を渡し、rustc の警告も失敗にする。全体で無効にする lint は作らない(`Cargo.toml` の `[lints]` は足さない)。

| lint | 扱い |
|---|---|
| `field_reassign_with_default` | `lib.rs` の `#![cfg_attr(test, allow(...))]` でテストビルドだけ無効 |
| `too_many_arguments`(1 件) | 既存の 9 箇所と同じく、関数に `#[allow]` |
| `result_large_err`(3 件) | 送れなかったコマンドを返す設計なので、関数に `#[allow]` |
| `type_complexity`(2 件) | 型エイリアスにする |
| `enum_variant_names`(`Metered::Metered`) | 名前は TS に出るので変えず、型に `#[allow]` |
| `dead_code` | 下記 |
| それ以外 | 直す |

`dead_code` は、使われる構成に合わせた最小の条件を付ける。

- テストだけが使うもの: `#[cfg(test)]`
- デスクトップだけが使うもの: `#[cfg(any(desktop, test))]`
- Android とテストだけが使うもの: `#[cfg(any(target_os = "android", test))]`
- TS に出る enum の変種(`commands/mcp.rs`): 削らず `#[cfg_attr(not(desktop), allow(dead_code))]`

生成される `frontend/src/bindings/tauri.gen.ts` は変わらないこと。

### CI

`test.yml` に独立した `clippy` ジョブを足す。

- `changes` の結果で中のステップを飛ばす(他の必須ジョブと同じ)
- `ubuntu-24.04`、`timeout-minutes: 45`
- apt の依存は `rust-test` と同じ
- `rust-cache` の保存は `main` でだけ(Actions のキャッシュが上限の 10GB に達しているため)
- `dtolnay/rust-toolchain`(`master` の SHA で固定)に、`clippy-toolchain` の版、`clippy` コンポーネント、`aarch64-linux-android` ターゲットを渡す
- `frontend/dist` は空の `index.html` を置くだけにする(`local-translate-build.yml` と同じ)
- NDK r27c の用意(`android-build` の `setup-ndk` とシンボリックリンクの復元を流用)
- Linux: `cargo clippy --all-targets -- -D warnings`
- Android: `cargo clippy --lib --target aarch64-linux-android -- -D warnings`。環境変数は `NDK_HOME` / `ANDROID_NDK_ROOT` / `ANDROID_PLATFORM=android-26` / `CC_` `CXX_` `AR_aarch64_linux_android`

Android のテストは、既存の `mcp` と `ts_config` のテストがデスクトップ専用でコンパイルできないので、`--lib` にする。

未確認: CI のまっさらな checkout で、`cargo tauri android build` を経ずに Android 向けの clippy が通るか(手元では生成済みの `gen/android` がある状態で通った)。CI の所要時間(手元は Linux 約 5 分、Android 約 3 分)。

### 必須チェック

PR のマージ後に、ruleset `main-1` の必須ステータスチェックに `clippy` を足す(ユーザーが行うか、指示を受けて Claude が行う)。

## ESLint

### 設定

`frontend/eslint.config.js`(flat config)と `pnpm lint`(`eslint . --max-warnings 0`)。依存は `eslint` / `@eslint/js` / `typescript-eslint` / `eslint-plugin-svelte` / `globals`。

### ESLint のルール

- `@eslint/js` の推奨、`typescript-eslint` の推奨(型情報なし)、`eslint-plugin-svelte` の推奨
- 型情報を使うルールは `no-floating-promises` と `no-misused-promises` の 2 つだけ
- 対象外: `dist/`、`coverage/`、`public/`、`src/bindings/`、リポジトリの `e2e/`

無効にするもの・条件を緩めるもの:

| ルール | 理由 |
|---|---|
| `svelte/require-each-key` | キーを足すと、重複があれば実行時エラーになる(挙動が変わる) |
| `svelte/no-unused-svelte-ignore` | 誤検知。指摘されたコメントを消すと `svelte-check` の警告が 43 件増える |
| `no-unused-expressions`(`.svelte` / `.svelte.ts` のみ) | `trigger;` や `app.now;` は、依存を追跡させるための式 |
| `no-irregular-whitespace` の正規表現内(`skipRegExps`) | 全角空白を意図して書いている |
| `svelte/no-useless-mustaches` の文字列エスケープを含むもの(`ignoreStringEscape`) | `{"ネタバレ\nspoiler"}` を属性に直すと、改行が文字の `\n` に変わる |
| `no-unused-vars` の `_` で始まる引数と変数(`argsIgnorePattern` / `varsIgnorePattern`) | 使わないことを名前で示している |

### 掃除の方針

- `no-floating-promises`: 呼びっぱなしの箇所は `void` を付ける(挙動は変わらない)。`catch` が要るバグと分かったものは Issue にする。
- `no-misused-promises`(2 件): コールバックを `void` を返す形にする。
- `prefer-svelte-reactivity`、`no-at-html-tags`、`no-control-regex`: 意図した書き方なので、理由を添えて行単位で無効にする。
- `no-explicit-any`: 型を付ける。型の付け方が大がかりになる箇所は、行単位で無効にする。
- それ以外: 直す。

### CI

必須の `frontend-check` に `pnpm lint` のステップを足す。ruleset は変えない。

### 注意点

TypeScript を 6.1 以上に上げる更新は、typescript-eslint が対応するまで取り込めない。

## 手元のフック(pre-push)

`.pre-commit-config.yaml` に pre-push のフックを足す。

- `default_install_hook_types: [pre-commit, pre-push]` と `default_stages: [pre-commit]`。`pre-commit install` をもう一度実行する必要がある。
- clippy: `src-tauri/` の変更があるとき。固定した版で Linux 向けだけを確認する(Android は NDK の環境変数が要るので CI だけ)。固定した版のツールチェーンと `frontend/dist` が必要。cargo の並列度は、指定が無ければ 4。macOS / Windows では、その OS 向けのコードの指摘も出る。
- ESLint: `frontend/` の変更があるとき `pnpm lint`(ESLint の PR で追加)。

コミット時のフックは今のまま(gitleaks と rustfmt)。

## ドキュメント

`CLAUDE.md` の Commands に、clippy と lint のコマンド、版の固定の場所、pre-push フックの用意を書く。

## 確認方法

- clippy: Linux と Android の両方が `-D warnings` で通る。警告を 1 つ入れると失敗する(確認後に戻す)。
- `cargo test`、`cargo fmt --all --check`、4 ABI のビルド(CI の `android-build`)が通り、`tauri.gen.ts` に差分が無い。
- ESLint: `pnpm lint` / `pnpm check` / `pnpm test` が通る。取りこぼした Promise を 1 つ入れると失敗する(確認後に戻す)。
- E2E は CI で確認する。
- pre-push フックは、手元で実行して成功と失敗の両方を確認する。

## 対象外

- 型情報を使う推奨ルール一式(`no-unsafe-*` など。同じ構成で試すと全体で約 490 件)
- `svelte/require-each-key` への対応
- Windows / macOS 向けの clippy
- `e2e/` の lint
- `svelte-check` の既存の警告 1 件(`UrlPreviewCard.svelte`)
