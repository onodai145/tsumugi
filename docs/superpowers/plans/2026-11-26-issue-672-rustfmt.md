# rustfmt の適用と CI チェック Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `src-tauri/` を rustfmt の既定で整形し、以後の崩れを CI で落とす(Issue #672)。

**Architecture:** 整形だけのコミットを 1 本作り、その hash を `.git-blame-ignore-revs` に登録する。必須チェックの `rust-test` ジョブの先頭に `--check` を足す。

**Tech Stack:** rustfmt(stable)、GitHub Actions

**Spec:** `docs/superpowers/specs/2026-11-26-issue-672-rustfmt-design.md`

## Global Constraints

- `rustfmt.toml` を作らない。
- ロジックを変えない。整形コミットに手作業の編集を混ぜない。
- 整形は `+stable` で行う(この環境の既定は nightly)。
- コミットは件名 1 行(Conventional Commits)と `Co-Authored-By` の trailer のみ。
- `git commit` が失敗・拒否されたら、打ち直さずに報告する。`--no-verify` などの回避フラグは使わない。
- PR は `--merge` でマージする。整形コミットを rebase・squash・amend しない。
- 作業用のコピーは scratchpad(以下 `$S`)に置く。

## Review Focus

- `llama_target.rs` だけが崩れた PR: `cargo fmt` は見ないので、個別のチェックで落ちること(Task 3 Step 3)。
- plugin だけが崩れた PR: `--all` で落ちること(Task 3 Step 3)。
- docs だけの PR: 整形チェックがスキップされ、`rust-test` が success になること(Task 3 Step 1 の `if`)。
- `blame.ignoreRevsFile` を設定したまま、ファイルが無い古いコミットを checkout した場合: `git blame` がエラーになる見込み。確かめて `CLAUDE.md` に書く(Task 3 Step 4)。
- 整形で `cargo test` が書き出す `tauri.gen.ts` が変わらないこと(Task 1 Step 4)。

---

### Task 1: 整形コミット

**Files:**
- Modify: `src-tauri/**/*.rs`(約 110 ファイル + `src-tauri/llama_target.rs`)

**Interfaces:**
- Produces: 整形コミットの 40 桁の hash(Task 2 で使う)

- [ ] **Step 1: spec の修正と plan をコミットする**

```bash
git add docs/superpowers/specs/2026-11-26-issue-672-rustfmt-design.md docs/superpowers/plans/2026-11-26-issue-672-rustfmt.md
git commit -m "docs: rustfmt 適用の実装計画を追加する(Issue #672)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git status --short   # 何も出ないこと
```

- [ ] **Step 2: 整形する**

```bash
cd src-tauri
cargo +stable fmt --all
rustfmt +stable --edition 2021 llama_target.rs
cargo +stable fmt --all --check && rustfmt +stable --edition 2021 --check llama_target.rs && echo CLEAN
cd .. && git status --short | grep -vc '\.rs$'   # 0 であること
```

Expected: `CLEAN`、`.rs` 以外の変更は 0。

- [ ] **Step 3: 追跡中の `.rs` に漏れが無いことを確かめる**

```bash
for f in $(git ls-files '*.rs'); do
  rustfmt +stable --edition 2021 --check --config skip_children=true "$f" >/dev/null 2>&1 || echo "$f"
done
```

Expected: 出力なし。

- [ ] **Step 4: テストする**

先に `free -h` で空きとスワップを確かめる。`frontend/dist` が無ければ `cd frontend && pnpm build`。

```bash
cd src-tauri
CARGO_BUILD_JOBS=4 systemd-run --user --scope -p MemoryMax=10G -p MemoryHigh=8G cargo test
cd .. && git status --short | grep -v '^ M .*\.rs$'; git diff --quiet -- frontend && echo BINDINGS-UNCHANGED
```

Expected: 全テスト pass(`#[ignore]` を除く)、`BINDINGS-UNCHANGED`。失敗したら、main でも失敗するかを確かめてから報告する。

- [ ] **Step 5: コミットする**

```bash
git add -u src-tauri
git commit -m "style: Rust のコードを rustfmt で整形する(Issue #672)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 6: 手作業の変更が無いことを確かめる**

```bash
S=<scratchpad>; rm -rf "$S/repro" && mkdir -p "$S/repro/a" "$S/repro/b"
git archive HEAD~ src-tauri | tar -x -C "$S/repro/a"
git archive HEAD  src-tauri | tar -x -C "$S/repro/b"
(cd "$S/repro/a/src-tauri" && cargo +stable fmt --all && rustfmt +stable --edition 2021 llama_target.rs)
diff -r "$S/repro/a" "$S/repro/b" && echo REPRODUCIBLE
```

Expected: `REPRODUCIBLE`。

### Task 2: `.git-blame-ignore-revs`

**Files:**
- Create: `.git-blame-ignore-revs`

**Interfaces:**
- Consumes: Task 1 Step 5 のコミットの hash

- [ ] **Step 1: ファイルを作る**

```bash
printf '# cargo fmt の初回の一括適用(Issue #672)\n%s\n' "$(git rev-parse HEAD)" > .git-blame-ignore-revs
cat .git-blame-ignore-revs
git log -1 --format='%H %s'   # hash が一致し、件名が style: であること
```

- [ ] **Step 2: 効果を測る**

```bash
h=$(git rev-parse HEAD); with=0; without=0
for f in $(git diff --name-only HEAD~ HEAD); do
  without=$((without + $(git blame -l -- "$f" | grep -c "^$h")))
  with=$((with + $(git blame -l --ignore-revs-file .git-blame-ignore-revs -- "$f" | grep -c "^$h")))
done
echo "without=$without with=$with"
```

Expected: `with` が `without` より小さい。両方の数を控え、PR の本文に書く。

- [ ] **Step 3: コミットする**

```bash
git add .git-blame-ignore-revs
git commit -m "chore: 整形コミットを git blame の対象から外す(Issue #672)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 3: CI と CLAUDE.md

**Files:**
- Modify: `.github/workflows/test.yml`(`rust-test` ジョブ)
- Modify: `CLAUDE.md`(`## Commands` のコードブロック)

- [ ] **Step 1: `rust-test` の先頭を書き換える**

`rust-test` の `actions/checkout` の直後(`Install system dependencies` の前)に次を置き、同じジョブの元の `dtolnay/rust-toolchain@stable` ステップ(コメント 2 行を含む)は削除する。他のジョブは変えない。

```yaml
      # dtolnay/rust-toolchain@stable is an intentionally moving branch that always
      # resolves the current stable Rust release, so it is left unpinned by design.
      - uses: dtolnay/rust-toolchain@stable
        if: needs.changes.outputs.code == 'true'
        with:
          components: rustfmt

      # 整形崩れは、時間のかかる apt のインストールとビルドの前に落とす(Issue #672)。
      # llama_target.rs は include! で読むので、cargo fmt の対象に入らない。
      - name: rustfmt --check
        if: needs.changes.outputs.code == 'true'
        working-directory: src-tauri
        run: |
          cargo fmt --all --check
          rustfmt --edition 2021 --check llama_target.rs

```

- [ ] **Step 2: YAML を確かめる**

```bash
python3 -I -c 'import yaml,sys; j=yaml.safe_load(open(".github/workflows/test.yml"))["jobs"]["rust-test"]["steps"]; [print(s.get("name") or s.get("uses")) for s in j]'
```

Expected: checkout、rust-toolchain、`rustfmt --check`、apt の順で、rust-toolchain は 1 回だけ。

- [ ] **Step 3: 崩れたコードでチェックが落ちることを確かめる**

```bash
S=<scratchpad>
for f in src/lib.rs plugins/tauri-plugin-haptics/src/lib.rs llama_target.rs; do
  rm -rf "$S/neg" && mkdir -p "$S/neg" && git archive HEAD src-tauri | tar -x -C "$S/neg"
  printf 'fn  __fmt_probe( ){ }\n' >> "$S/neg/src-tauri/$f"
  (cd "$S/neg/src-tauri" && bash -e -c 'cargo +stable fmt --all --check >/dev/null; rustfmt +stable --edition 2021 --check llama_target.rs >/dev/null') && echo "MISSED $f" || echo "CAUGHT $f"
done
```

Expected: 3 ファイルとも `CAUGHT`。plugin が `MISSED` なら、CI に `cargo fmt --manifest-path plugins/tauri-plugin-haptics/Cargo.toml --check` を足して再確認する。

- [ ] **Step 4: ファイルが無いコミットでの blame の挙動を確かめる**

```bash
git -c blame.ignoreRevsFile=does-not-exist blame -L1,1 CLAUDE.md; echo "exit=$?"
```

Expected: エラー終了。エラーにならなければ、Step 5 の 2 行目の「ファイルが無い〜」の一文を書かない。

- [ ] **Step 5: `CLAUDE.md` に追記する**

`## Commands` のコードブロックの、`cd frontend  && pnpm check` の行の直前に足す。

```sh
cd src-tauri && cargo +stable fmt --all && rustfmt +stable --edition 2021 llama_target.rs  # コミット前の整形(CI の rust-test が --check で確認する)。llama_target.rs は include! で読むので cargo fmt の対象外
git config blame.ignoreRevsFile .git-blame-ignore-revs  # 手元で1回。整形だけのコミットを git blame で飛ばす(GitHub はファイルを自動で読む)。ファイルが無い古いコミットを checkout している間は git blame がエラーになる
```

- [ ] **Step 6: コミットする**

```bash
git add .github/workflows/test.yml CLAUDE.md
git commit -m "ci: rustfmt のチェックを追加する(Issue #672)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
git status --short; git log --oneline main..HEAD
```

### Task 4: pre-commit のフック

**Files:**
- Modify: `.pre-commit-config.yaml`
- Modify: `CLAUDE.md`(Task 3 で足した行の直後)

- [ ] **Step 1: フックが無いことを確かめる**

```bash
pre-commit run rustfmt --all-files; echo "exit=$?"
```

Expected: `No hook with id` で失敗する。

- [ ] **Step 2: フックを足す**

`.pre-commit-config.yaml` の `repos:` の末尾に足す。

```yaml
  # CI の rust-test と同じ確認(Issue #672)。ファイルは書き換えない。
  # llama_target.rs は include! で読むので、cargo fmt の対象に入らない。
  - repo: local
    hooks:
      - id: rustfmt
        name: rustfmt --check
        entry: bash -c 'cd src-tauri && cargo +stable fmt --all --check && rustfmt +stable --edition 2021 --check llama_target.rs'
        language: system
        types: [rust]
        pass_filenames: false
```

- [ ] **Step 3: 通る場合と落ちる場合を確かめる**

```bash
pre-commit run rustfmt --all-files
S=<scratchpad>
for f in src/lib.rs plugins/tauri-plugin-haptics/src/lib.rs llama_target.rs; do
  rm -rf "$S/hook" && git clone -q . "$S/hook" && cp .pre-commit-config.yaml "$S/hook/"
  printf 'fn  __fmt_probe( ){ }\n' >> "$S/hook/src-tauri/$f"
  (cd "$S/hook" && git add -u && before=$(git diff HEAD | sha256sum) && { pre-commit run rustfmt >/dev/null && echo "MISSED $f" || echo "CAUGHT $f"; } && [ "$before" = "$(git diff HEAD | sha256sum)" ] && echo UNMODIFIED)
done
```

Expected: 1 つ目は `Passed`。3 ファイルとも `CAUGHT` と `UNMODIFIED`。

- [ ] **Step 4: `CLAUDE.md` に追記する**

Task 3 で足した `git config blame.ignoreRevsFile` の行の直後に足す。

```sh
pre-commit install  # 手元で1回。コミット時に gitleaks と rustfmt --check が動く(rustfmt は stable toolchain が必要)
```

- [ ] **Step 5: コミットする**

```bash
git add .pre-commit-config.yaml CLAUDE.md
git commit -m "chore: pre-commit に rustfmt のチェックを追加する(Issue #672)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

## 完了後

push と PR の作成は、ユーザーの確認を取ってから行う。PR の本文は `.github/pull_request_template.md` の構成に従い、次を含める。

- `Closes #672`
- `--merge` でマージすること(squash・rebase は不可)
- Task 2 Step 2 で測った行数
- #510 は、ブランチで整形してから main を取り込むこと
