# テストカバレッジの可視化 設計

Issue #222(親: #73)。Rust側・フロントエンド側それぞれのテストカバレッジを計測し、Codecovで可視化する。

## 背景

Rust側(`cargo test` 202件)・フロントエンド側(Vitest 26テストファイル)ともにテストは充実してきたが、カバレッジを計測・可視化する仕組みがなく、どこがテストされていないか機械的に把握できない。

## スコープ

- Rust側: `cargo-llvm-cov` でカバレッジを計測し、lcov形式で出力
- フロントエンド側: Vitest のv8カバレッジプロバイダ(`@vitest/coverage-v8`)でlcov形式で出力
- 両方をCodecovにアップロードし、PR上でカバレッジ差分・バッジとして可視化
- カバレッジは**可視化のみ**とし、CIの必須チェックにはしない(閾値未達でCIを失敗させない)

## Rust側カバレッジツールの選定

Issueでは `cargo tarpaulin` が例挙されているが、`cargo-llvm-cov` を採用する。tarpaulinはLinux上でptraceベースの計装を行うため、Tauri/WebKitGTK依存のコードや非同期コードで不安定になることがある。`cargo-llvm-cov` はrustc/LLVM組み込みのsource-based coverageを使うため安定して動き、workspace対応・lcov出力ともに公式にサポートされている。

## アーキテクチャ

`.github/workflows/test.yml` に既存の `rust-test` / `frontend-check` とは別の新規ジョブを2つ追加する。既存ジョブの内容・必須チェック構成には変更を加えない。

### `rust-coverage` ジョブ

- `needs: changes` で既存ジョブと同じdiffガードを使う(docs/mdのみの変更ではスキップ)
- 依存インストールは `rust-test` ジョブと同様(Tauri v2 Linux依存一式)
- `taiki-e/install-action@cargo-llvm-cov` で `cargo-llvm-cov` をインストール
- `cargo llvm-cov --workspace --lcov --output-path lcov.info` を `src-tauri` で実行
  - `cargo test` と同様、`#[ignore]` 付きの実Misskey接続テストは既定でスキップされる(挙動は変わらない)
- 生成した `lcov.info` をCodecovにアップロード(`flags: rust`)

### `frontend-coverage` ジョブ

- `needs: changes` で同じdiffガード
- `frontend/package.json` に `@vitest/coverage-v8` を devDependency として追加
- `frontend/vitest.config.ts`(または既存のvite/vitest設定)に以下を追加:
  ```ts
  test: {
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      exclude: ['src/bindings/tauri.gen.ts', /* 既存のtest excludeパターンに準拠 */],
    },
  }
  ```
  - `bindings/tauri.gen.ts` は生成物のためカバレッジ対象から除外
- `pnpm test -- --coverage` でlcov生成
- 生成した `frontend/coverage/lcov.info` をCodecovにアップロード(`flags: frontend`)

### Codecovアップロード共通設定

両ジョブとも `codecov/codecov-action` を使う。`fail_ci_if_error: false` を指定し、Codecov側のアップロード失敗(トークン未設定含む)でCIジョブ自体が失敗しないようにする。トークンは `secrets.CODECOV_TOKEN` を渡す(未設定でもジョブは失敗しない)。

### `codecov.yml`(リポジトリルート新規作成)

- `flags: rust / frontend` を定義し、Codecov UI上でRust/フロントエンドのカバレッジを分けて表示
- `coverage.status.project` / `coverage.status.patch` を `informational: true` にし、Codecovのステータスチェック自体が「情報提供のみ」になるようにする(閾値未達でチェックが赤くならない)
- PRコメント(カバレッジ差分の自動コメント)は有効のままにする(可視化の主目的)

### README

カバレッジバッジ(Codecovバッジ)を追加する。

## ユーザー側の前提作業(実装側では完了できない)

1. https://codecov.io にGitHubアカウントでログインし、`onodai145/tsumugi` リポジトリを有効化する
2. 発行された `CODECOV_TOKEN` をGitHubリポジトリのSecretsに `CODECOV_TOKEN` として登録する

これが未実施の間、CodecovアップロードステップはCI上で失敗するが、`fail_ci_if_error: false` によりジョブ全体はグリーンのまま完了する。

## テスト方針

この変更自体はCIワークフロー/ツール設定の変更であり、新規プロダクトコードのユニットテストは追加しない。検証は以下で行う:

- ローカルで `cd src-tauri && cargo llvm-cov --workspace --lcov --output-path lcov.info` が正常終了し、`lcov.info` が生成されることを確認
- ローカルで `cd frontend && pnpm test -- --coverage` が正常終了し、`frontend/coverage/lcov.info` が生成されることを確認
- ブランチをpushしてCI上で `rust-coverage` / `frontend-coverage` ジョブが(Codecovトークン未設定でも)グリーンで完走することを確認
- 既存の `rust-test` / `frontend-check` ジョブの内容・実行時間に変化がないことを確認(diffで既存ジョブに変更が入っていないことを確認)

## 非スコープ

- カバレッジ閾値によるCI必須化(将来issueとして別途検討)
- E2Eテスト(Playwright)のカバレッジ計測
