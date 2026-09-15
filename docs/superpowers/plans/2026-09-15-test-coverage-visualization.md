# テストカバレッジ可視化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rust側(`cargo-llvm-cov`)・フロントエンド側(Vitest v8 coverage)のテストカバレッジを計測し、Codecovにアップロードして可視化する。CIの必須チェックには追加しない(informational)。

**Architecture:** `.github/workflows/test.yml` に既存の `rust-test` / `frontend-check` とは別の新規ジョブ `rust-coverage` / `frontend-coverage` を追加する。それぞれlcov形式のカバレッジを生成し、`codecov/codecov-action` でCodecovにアップロードする。ルートに `codecov.yml` を追加し、Codecovのステータスチェック自体を informational にする。

**Tech Stack:** `cargo-llvm-cov`(Rust)、`@vitest/coverage-v8`(フロントエンド)、`codecov/codecov-action`、GitHub Actions。

## Global Constraints

- 既存の `rust-test` / `frontend-check` ジョブの内容・必須チェック構成は変更しない(spec: アーキテクチャ節)
- カバレッジは可視化のみ。閾値未達でCIを失敗させない(spec: スコープ節)
- Codecovアップロードは `fail_ci_if_error: false` で行い、`CODECOV_TOKEN` 未設定でもジョブ全体はグリーンのまま完走する(spec: Codecovアップロード共通設定節)
- 生成物 `src/bindings/tauri.gen.ts` はフロントエンドのカバレッジ集計対象から除外する(spec: frontend-coverageジョブ節)
- Rust側カバレッジツールは `cargo-llvm-cov` を使う(tarpaulinではない)(spec: Rust側カバレッジツールの選定節)
- GitHub Actionsのサードパーティactionは、このリポジトリの既存の書き方に合わせてタグのコミットSHAで pin し、`# vX.Y.Z` コメントを付与する(既存 `.github/workflows/test.yml` の書き方に準拠)

---

### Task 1: Rust側カバレッジ計測(`cargo-llvm-cov`)をCIに追加

**Files:**
- Modify: `.github/workflows/test.yml`(`rust-test` ジョブの直後に新規ジョブ `rust-coverage` を追加)
- Modify: `.gitignore`(ルートの `.gitignore`。lcov出力を無視)

**Interfaces:**
- Consumes: なし(既存の `changes` ジョブの `outputs.code` を条件分岐に使う。既存 `rust-test` ジョブと同じパターン)
- Produces: `src-tauri/lcov.info`(CI実行時に生成される一時ファイル。Task 3でCodecovアップロードのinputとして使う)

- [ ] **Step 1: ローカルで `cargo-llvm-cov` をインストールし、コマンドが動くことを確認する**

```bash
rustup component add llvm-tools
cargo install cargo-llvm-cov --locked
```

Run:
```bash
cd src-tauri && cargo llvm-cov --version
```
Expected: バージョン文字列が出力される(エラーなし)

- [ ] **Step 2: ローカルで実際にカバレッジ計測が通ることを確認する**

Run:
```bash
cd src-tauri && cargo llvm-cov --workspace --lcov --output-path lcov.info
```
Expected: 既存の `cargo test` と同じテストが実行され(`#[ignore]` 付きテストはスキップ)、成功終了。`src-tauri/lcov.info` が生成される。

Run:
```bash
test -s src-tauri/lcov.info && echo "OK: lcov.info generated"
```
Expected: `OK: lcov.info generated`

- [ ] **Step 3: 生成された `lcov.info` を `.gitignore` に追加する**

`.gitignore` に以下を追記(既存の `/src-tauri/target` の近くに追加):

```
/src-tauri/lcov.info
```

- [ ] **Step 4: `.github/workflows/test.yml` に `rust-coverage` ジョブを追加する**

`rust-test` ジョブ(既存)の直後、`frontend-check` ジョブの直前に以下を挿入する:

```yaml
  rust-coverage:
    needs: changes
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4

      - name: Install system dependencies (Tauri v2 Linux)
        if: needs.changes.outputs.code == 'true'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libjavascriptcoregtk-4.1-dev \
            libsoup-3.0-dev \
            libgtk-3-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            libasound2-dev \
            patchelf

      - uses: dtolnay/rust-toolchain@stable
        if: needs.changes.outputs.code == 'true'
        with:
          components: llvm-tools

      - uses: Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae # v2
        if: needs.changes.outputs.code == 'true'
        with:
          workspaces: src-tauri

      - uses: taiki-e/install-action@26e9283f268b880168bdbd2c545dfcd60ec2c6ab # v2.87.13
        if: needs.changes.outputs.code == 'true'
        with:
          tool: cargo-llvm-cov

      # package.json の packageManager は frontend/ 配下にあり action-setup のデフォルト
      # 検出(リポジトリルート)では拾えないため version を明示指定する。
      - uses: pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa # v4
        if: needs.changes.outputs.code == 'true'
        with:
          version: 11.3.0

      - uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
        if: needs.changes.outputs.code == 'true'
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: frontend/pnpm-lock.yaml

      - name: pnpm install
        if: needs.changes.outputs.code == 'true'
        working-directory: frontend
        run: pnpm install --frozen-lockfile

      # cargo test/llvm-cov は tauri::generate_context!() をコンパイル時に評価するため、
      # tauri.conf.json の frontendDist(../frontend/dist) が実在している必要がある。
      - name: pnpm build (frontendDist生成)
        if: needs.changes.outputs.code == 'true'
        working-directory: frontend
        run: pnpm build

      - name: cargo llvm-cov
        if: needs.changes.outputs.code == 'true'
        working-directory: src-tauri
        run: cargo llvm-cov --workspace --lcov --output-path lcov.info
```

(Codecovへのアップロードステップは Task 3 で末尾に追加する)

- [ ] **Step 5: YAML構文が正しいことを確認する**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test.yml'))" && echo "OK: valid YAML"
```
Expected: `OK: valid YAML`

- [ ] **Step 6: コミット**

```bash
git add .github/workflows/test.yml .gitignore
git commit -m "ci: Rust側のテストカバレッジ計測(cargo-llvm-cov)を追加"
```

---

### Task 2: フロントエンド側カバレッジ計測(Vitest v8)を追加

**Files:**
- Modify: `frontend/package.json`(devDependency追加、`test:coverage` script追加)
- Modify: `frontend/vite.config.ts`(`test.coverage` 設定追加)
- Modify: `.github/workflows/test.yml`(`frontend-coverage` ジョブ追加)
- Modify: `frontend/.gitignore`(`coverage` ディレクトリを無視)

**Interfaces:**
- Consumes: なし
- Produces: `frontend/coverage/lcov.info`(CI実行時に生成。Task 3でCodecovアップロードのinputとして使う)

- [ ] **Step 1: `@vitest/coverage-v8` を追加する**

```bash
cd frontend && pnpm add -D @vitest/coverage-v8@^4.1.11
```

Expected: `frontend/package.json` の `devDependencies` に `@vitest/coverage-v8` が追加され、`pnpm-lock.yaml` が更新される。

- [ ] **Step 2: `frontend/package.json` に `test:coverage` scriptを追加する**

`frontend/package.json` の `scripts` を編集:

```json
    "test": "vitest run",
    "test:coverage": "vitest run --coverage",
```

- [ ] **Step 3: `frontend/vite.config.ts` の `test` セクションにcoverage設定を追加する**

現在の `test: { environment: "jsdom" }` を以下に変更する:

```ts
  test: {
    environment: "jsdom",
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov"],
      // 生成物のためカバレッジ集計から除外
      exclude: ["src/bindings/tauri.gen.ts"],
    },
  },
```

- [ ] **Step 4: ローカルでカバレッジ計測が通ることを確認する**

Run:
```bash
cd frontend && pnpm test:coverage
```
Expected: 既存の全テストが成功し、`frontend/coverage/lcov.info` が生成される。ターミナルにファイル別カバレッジのテキストサマリが表示される。

Run:
```bash
test -s frontend/coverage/lcov.info && echo "OK: lcov.info generated"
```
Expected: `OK: lcov.info generated`

- [ ] **Step 5: `frontend/.gitignore` にカバレッジ出力を追加する**

`frontend/.gitignore` に以下を追記(`dist-ssr` の下に追加):

```
coverage
```

- [ ] **Step 6: `.github/workflows/test.yml` に `frontend-coverage` ジョブを追加する**

`frontend-check` ジョブの直後、`e2e` ジョブの直前に以下を挿入する:

```yaml
  frontend-coverage:
    needs: changes
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4

      - uses: pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa # v4
        if: needs.changes.outputs.code == 'true'
        with:
          version: 11.3.0

      - uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
        if: needs.changes.outputs.code == 'true'
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: frontend/pnpm-lock.yaml

      - name: pnpm install
        if: needs.changes.outputs.code == 'true'
        working-directory: frontend
        run: pnpm install --frozen-lockfile

      - name: vitest (coverage)
        if: needs.changes.outputs.code == 'true'
        working-directory: frontend
        run: pnpm test:coverage
```

(Codecovへのアップロードステップは Task 3 で末尾に追加する)

- [ ] **Step 7: YAML構文が正しいことを確認する**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test.yml'))" && echo "OK: valid YAML"
```
Expected: `OK: valid YAML`

- [ ] **Step 8: コミット**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml frontend/vite.config.ts frontend/.gitignore .github/workflows/test.yml
git commit -m "ci: フロントエンド側のテストカバレッジ計測(Vitest v8)を追加"
```

---

### Task 3: Codecovアップロード統合(`codecov.yml` + 両ジョブへのアップロードステップ)

**Files:**
- Create: `codecov.yml`(リポジトリルート)
- Modify: `.github/workflows/test.yml`(Task 1/2で追加した `rust-coverage` / `frontend-coverage` ジョブそれぞれの末尾にCodecovアップロードステップを追加)

**Interfaces:**
- Consumes: `src-tauri/lcov.info`(Task 1の出力)、`frontend/coverage/lcov.info`(Task 2の出力)
- Produces: なし(Codecov外部サービスへのアップロードが最終出力)

- [ ] **Step 1: `codecov.yml` を作成する**

```yaml
codecov:
  require_ci_to_pass: false

coverage:
  status:
    project:
      default:
        informational: true
    patch:
      default:
        informational: true

flags:
  rust:
    paths:
      - src-tauri/
  frontend:
    paths:
      - frontend/src/

comment: true
```

- [ ] **Step 2: `codecov.yml` の構文をCodecovのvalidateエンドポイントで検証する**

Run:
```bash
curl -s --max-time 10 -X POST --data-binary @codecov.yml https://codecov.io/validate
```
Expected: 出力の先頭行が `Valid!`

- [ ] **Step 3: `rust-coverage` ジョブの末尾にCodecovアップロードステップを追加する**

Task 1で追加した `rust-coverage` ジョブの最後のステップ(`cargo llvm-cov`)の後に追加:

```yaml
      - name: Upload coverage to Codecov
        if: needs.changes.outputs.code == 'true'
        uses: codecov/codecov-action@046562be8d17331600874a09d2c2062c27752c20 # v7.1.0
        with:
          token: ${{ secrets.CODECOV_TOKEN }}
          files: src-tauri/lcov.info
          flags: rust
          fail_ci_if_error: false
```

- [ ] **Step 4: `frontend-coverage` ジョブの末尾にCodecovアップロードステップを追加する**

Task 2で追加した `frontend-coverage` ジョブの最後のステップ(`vitest (coverage)`)の後に追加:

```yaml
      - name: Upload coverage to Codecov
        if: needs.changes.outputs.code == 'true'
        uses: codecov/codecov-action@046562be8d17331600874a09d2c2062c27752c20 # v7.1.0
        with:
          token: ${{ secrets.CODECOV_TOKEN }}
          files: frontend/coverage/lcov.info
          flags: frontend
          fail_ci_if_error: false
```

- [ ] **Step 5: YAML構文が正しいことを確認する**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/test.yml'))" && echo "OK: valid YAML"
```
Expected: `OK: valid YAML`

- [ ] **Step 6: コミット**

```bash
git add codecov.yml .github/workflows/test.yml
git commit -m "ci: Codecovへのカバレッジアップロードを統合"
```

---

### Task 4: READMEにCodecovバッジを追加

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: なし
- Produces: なし(表示のみ)

- [ ] **Step 1: 既存バッジ列の1行目(testバッジ)の直後にCodecovバッジを追加する**

`README.md` の1行目のバッジ:
```
[![test](https://github.com/onodai145/tsumugi/actions/workflows/test.yml/badge.svg)](https://github.com/onodai145/tsumugi/actions/workflows/test.yml)
```
の直後に以下を追加:
```
[![codecov](https://codecov.io/gh/onodai145/tsumugi/branch/main/graph/badge.svg)](https://codecov.io/gh/onodai145/tsumugi)
```

- [ ] **Step 2: バッジのMarkdown記法に誤りがないことを目視確認する**

Run:
```bash
head -5 README.md
```
Expected: 追加した行が他のバッジ行と同じ `[![alt](img)](link)` 形式で表示される

- [ ] **Step 3: コミット**

```bash
git add README.md
git commit -m "docs: READMEにCodecovバッジを追加"
```

---

### Task 5: 最終確認とハンドオフ

**Files:** なし(検証のみ)

**Interfaces:**
- Consumes: Task 1〜4の全変更
- Produces: なし

- [ ] **Step 1: 既存の必須チェックジョブに差分が入っていないことを確認する**

Run:
```bash
git diff main -- .github/workflows/test.yml | grep -E "^[+-]" | grep -v "^+++\|^---" | grep -B2 -A2 "rust-test:\|frontend-check:\|e2e:\|changes:" || true
```
Expected: `rust-test` / `frontend-check` / `e2e` / `changes` ジョブの既存ステップに変更が無く、追加分(`rust-coverage` / `frontend-coverage` ジョブと、両ジョブ末尾のCodecovアップロードステップ)のみが `+` として出ることを目視確認する

- [ ] **Step 2: ローカルで最終確認として両方のカバレッジコマンドを再実行する**

Run:
```bash
cd src-tauri && cargo llvm-cov --workspace --lcov --output-path lcov.info && cd ../frontend && pnpm test:coverage
```
Expected: 両方成功終了。それぞれの `lcov.info` が生成される。

- [ ] **Step 3: ブランチをpushしてPRを作成する**

```bash
git push -u origin feature/222-test-coverage-visualization
gh pr create --title "テストカバレッジの可視化(Rust/フロントエンド)" --body "$(cat <<'EOF'
## 概要
Issue #222 の対応。Rust側(cargo-llvm-cov)・フロントエンド側(Vitest v8 coverage)のテストカバレッジを計測し、Codecovで可視化する。

## 変更内容
- `.github/workflows/test.yml` に `rust-coverage` / `frontend-coverage` ジョブを新規追加(既存の `rust-test` / `frontend-check` は変更なし)
- 両ジョブでlcov形式のカバレッジを生成し、Codecovにアップロード
- ルートに `codecov.yml` を追加(ステータスチェックはinformationalで必須化しない)
- READMEにCodecovバッジを追加

## 前提作業(マージ後、リポジトリ管理者側で実施が必要)
1. https://codecov.io にログインし、`onodai145/tsumugi` リポジトリを有効化
2. 発行された `CODECOV_TOKEN` をGitHub Secretsに登録

トークン未設定の間はCodecovアップロードステップが失敗するが、`fail_ci_if_error: false` のためジョブ全体はグリーンで完走する。

## 設計ドキュメント
`docs/superpowers/specs/2026-09-15-test-coverage-visualization-design.md`

Closes #222
EOF
)"
```

- [ ] **Step 4: ユーザーに前提作業を伝える**

PR作成後、ユーザーに以下を伝える: 「マージ後、CodecovサイトでリポジトリをActivateし `CODECOV_TOKEN` をGitHub Secretsに登録してください。それまではCodecovアップロードステップは失敗しますが、CI全体は失敗しません」

CI結果はユーザー自身が確認する(pushしたらMonitorで待たない)。
