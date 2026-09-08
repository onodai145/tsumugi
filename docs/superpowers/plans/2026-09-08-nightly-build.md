# Nightly Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a scheduled GitHub Actions workflow (`.github/workflows/nightly.yml`) that builds and publishes a `nightly` pre-release (Linux/macOS/Windows/Android, matching `release.yml`'s artifacts) only on days `main` has changed, and guard `update-pkgbuild.yml` so it doesn't misfire on that pre-release.

**Architecture:** A `check` job resolves the commit the `nightly` tag currently points to (via `gh api repos/{repo}/commits/nightly`) and compares it to `github.sha`; its `should_run` output gates a `cleanup` job (deletes the stale `nightly` release+tag) and two build jobs (`build`: desktop matrix via `tauri-action`, `android`: signed APKs via `softprops/action-gh-release`), both jobs re-created from `release.yml` with `tagName`/`tag_name` fixed to a workflow-level `NIGHTLY_TAG` env var instead of `github.ref_name`, `releaseCommitish`/`target_commitish` pinned to `github.sha`, and `prerelease: true` / `draft: false` on both. A `concurrency` group serializes overlapping runs.

**Tech Stack:** GitHub Actions YAML, `gh` CLI, `tauri-apps/tauri-action`, `softprops/action-gh-release`, bash/pwsh.

## Global Constraints

- Design doc: `docs/superpowers/specs/2026-09-08-nightly-build-design.md` — this plan implements it verbatim; deviate only if the code disagrees with what the spec assumed.
- `NIGHTLY_TAG` must be used everywhere `release.yml` uses `github.ref_name` (tag names, asset filenames) — never `github.ref_name` itself, since on `schedule`/`workflow_dispatch` runs against `main` it resolves to `main`, not `nightly`.
- Every job capable of creating the `nightly` Release (`build`'s `tauri-action` step, `android`'s `softprops/action-gh-release` step) must explicitly set `prerelease: true` and `draft: false` — omitting it on either risks a race where the other job wins and creates a non-prerelease Release, silently breaking the app's update-check (`check_latest_release` in `src-tauri/src/commands/app.rs`).
- `releaseCommitish` (tauri-action) / `target_commitish` (softprops) must be `${{ github.sha }}` on every Release-creating step — `cleanup` deletes the tag first, so without this the tag lands on whatever the default branch HEAD is when GitHub processes the request, which can drift from the commit `check` compared against and corrupts the next run's diff detection.
- `cleanup`'s delete command must tolerate "release doesn't exist" (first-ever run) without failing the job.
- Existing workflows (`release.yml`, `test.yml`, `gitleaks.yml`) must not change behavior.
- Commit messages: subject line only (repo-wide rule from `CLAUDE.md`/global workflow rules).

---

## File Structure

- Modify: `.github/workflows/update-pkgbuild.yml` — add a `prerelease == false` guard so nightly (and any future prerelease) doesn't trigger a PKGBUILD-update PR.
- Create: `.github/workflows/nightly.yml` — the new scheduled workflow, built up job-by-job across Tasks 2–5.

There is no meaningful unit-test story for GitHub Actions YAML itself; "tests" here are (a) `actionlint` static validation of the YAML/expression syntax, and (b) for the one piece of nontrivial branching logic (the `check` job's tag-diff bash), a local test harness that stubs `gh` and exercises all three branches before that logic goes into the workflow file.

---

### Task 1: Guard update-pkgbuild.yml against prerelease Releases

**Files:**
- Modify: `.github/workflows/update-pkgbuild.yml`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: nothing consumed by other tasks (independent, can be done in any order relative to Tasks 2–6).

- [ ] **Step 1: Add the guard**

Edit `.github/workflows/update-pkgbuild.yml`'s `update` job to add an `if:` condition. Current job:

```yaml
jobs:
  update:
    runs-on: ubuntu-latest
    steps:
```

Change to:

```yaml
jobs:
  update:
    runs-on: ubuntu-latest
    # nightly build(Issue #299)はprerelease:trueで公開されるが、release イベントの
    # types: [released, published] はprereleaseかどうかを問わず発火するため、
    # ここでガードしないと毎晩 TAG=nightly でこのジョブが誤って走ってしまう。
    if: github.event.release.prerelease == false
    steps:
```

- [ ] **Step 2: Sanity-check the YAML parses**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/update-pkgbuild.yml'))" && echo OK`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/update-pkgbuild.yml
git commit -m "ci: nightly Releaseによるupdate-pkgbuild.ymlの誤発火を防ぐ"
```

---

### Task 2: `check` job — tag-diff detection logic, tested standalone first

**Files:**
- Create: `.github/workflows/nightly.yml`
- Test (scratch, not committed): a local bash harness exercising the same branching logic as the `check` job's script, run manually in this task and then discarded — GitHub Actions has no local unit-test runner, so this is how the logic gets exercised before it's embedded in YAML `run:` blocks.

**Interfaces:**
- Consumes: nothing.
- Produces: `.github/workflows/nightly.yml` with `name`, `on`, `env`, `concurrency`, `permissions`, and the `check` job. Later tasks append more jobs to this same file. Produces job output `check.outputs.should_run` (string `"true"`/`"false"`), consumed by Tasks 3–5's `if:` conditions.

- [ ] **Step 1: Write and run the standalone logic test**

This isn't a `pytest`/`cargo test` style test — GitHub Actions expressions can't run outside a workflow — so first prove the shell logic is correct by extracting it into a throwaway script and driving it with a fake `gh`.

Create `/tmp/nightly-check-test.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# 本番の check ジョブと同じロジック。GH_SHA/NIGHTLY_SHA/EVENT_NAME を
# 環境変数で与えて should_run を標準出力する。
run_check() {
  if [ "$EVENT_NAME" = "workflow_dispatch" ]; then
    echo "true"
    return
  fi
  if [ -n "${NIGHTLY_SHA:-}" ]; then
    if [ "$NIGHTLY_SHA" = "$GH_SHA" ]; then
      echo "false"
    else
      echo "true"
    fi
  else
    echo "true"
  fi
}

fail=0

# ケース1: nightlyタグが存在しない(初回実行) → true
out=$(EVENT_NAME=schedule GH_SHA=abc123 NIGHTLY_SHA="" run_check)
[ "$out" = "true" ] || { echo "FAIL case1: got $out"; fail=1; }

# ケース2: nightlyタグがHEADと同じコミットを指す → false
out=$(EVENT_NAME=schedule GH_SHA=abc123 NIGHTLY_SHA=abc123 run_check)
[ "$out" = "false" ] || { echo "FAIL case2: got $out"; fail=1; }

# ケース3: nightlyタグがHEADと異なるコミットを指す → true
out=$(EVENT_NAME=schedule GH_SHA=abc123 NIGHTLY_SHA=def456 run_check)
[ "$out" = "true" ] || { echo "FAIL case3: got $out"; fail=1; }

# ケース4: workflow_dispatch は差分に関わらず true
out=$(EVENT_NAME=workflow_dispatch GH_SHA=abc123 NIGHTLY_SHA=abc123 run_check)
[ "$out" = "true" ] || { echo "FAIL case4: got $out"; fail=1; }

if [ "$fail" = 0 ]; then
  echo "ALL PASS"
else
  exit 1
fi
```

Run: `chmod +x /tmp/nightly-check-test.sh && /tmp/nightly-check-test.sh`
Expected: `ALL PASS`

If it fails, fix `run_check` until all four cases pass — this is the exact logic Step 3 below embeds into the workflow, so it must be right here first.

- [ ] **Step 2: Discard the scratch harness**

Run: `rm /tmp/nightly-check-test.sh`

- [ ] **Step 3: Write `.github/workflows/nightly.yml` with header + `check` job**

Create `.github/workflows/nightly.yml`:

```yaml
name: nightly

on:
  schedule:
    # UTC 18:00 = JST 3:00
    - cron: "0 18 * * *"
  workflow_dispatch:

env:
  NIGHTLY_TAG: nightly

concurrency:
  group: nightly
  cancel-in-progress: false

permissions:
  contents: write

jobs:
  # mainのHEADとnightlyタグが指すコミットを比較し、差分が無ければ以降の
  # ビルド・Release作成を全てスキップする(Issue #299: 文字通りのNightly Build)。
  check:
    runs-on: ubuntu-latest
    outputs:
      should_run: ${{ steps.diff.outputs.should_run }}
    steps:
      - name: mainとnightlyタグの差分を確認
        id: diff
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          if [ "${{ github.event_name }}" = "workflow_dispatch" ]; then
            echo "should_run=true" >> "$GITHUB_OUTPUT"
            exit 0
          fi
          # commits/{ref} は ref の種別(lightweight/annotated tag等)によらず
          # 常にコミットSHAへ解決される。タグが無ければコマンドは失敗するので、
          # その場合は初回実行として扱う。
          if nightly_sha=$(gh api "repos/${{ github.repository }}/commits/${NIGHTLY_TAG}" --jq .sha 2>/dev/null); then
            if [ "$nightly_sha" = "${{ github.sha }}" ]; then
              echo "should_run=false" >> "$GITHUB_OUTPUT"
            else
              echo "should_run=true" >> "$GITHUB_OUTPUT"
            fi
          else
            echo "should_run=true" >> "$GITHUB_OUTPUT"
          fi
```

- [ ] **Step 4: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/nightly.yml'))" && echo OK`
Expected: `OK`

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/nightly.yml
git commit -m "ci: nightly buildワークフローのcheckジョブを追加"
```

---

### Task 3: `cleanup` job

**Files:**
- Modify: `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes: `check.outputs.should_run` (from Task 2).
- Produces: nothing new consumed by name — Tasks 4–5 just need the `cleanup` job to exist so they can `needs: [check, cleanup]`.

- [ ] **Step 1: Append the `cleanup` job**

Add to `.github/workflows/nightly.yml`, after the `check` job:

```yaml

  # 前日以前のnightly Release/タグを削除してから作り直す(前日分のアセットを
  # 残さないため)。初回実行時はReleaseが存在せず削除コマンドが失敗するので
  # `|| true` で許容する。
  cleanup:
    needs: check
    if: needs.check.outputs.should_run == 'true'
    runs-on: ubuntu-latest
    steps:
      - name: 既存のnightly Releaseとタグを削除
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release delete "$NIGHTLY_TAG" --yes --cleanup-tag --repo "${{ github.repository }}" || true
```

- [ ] **Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/nightly.yml'))" && echo OK`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/nightly.yml
git commit -m "ci: nightly buildワークフローのcleanupジョブを追加"
```

---

### Task 4: `build` job (Linux/macOS/Windows desktop matrix)

**Files:**
- Modify: `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes: `check.outputs.should_run` (Task 2), existence of `cleanup` job (Task 3).
- Produces: nothing consumed by other tasks — `android` (Task 5) is independent of `build`, both only depend on `check`/`cleanup`.

- [ ] **Step 1: Append the `build` job**

Add to `.github/workflows/nightly.yml`, after the `cleanup` job. This mirrors `release.yml`'s `release` job; differences from it are called out inline.

```yaml

  # release.ymlのreleaseジョブと同じビルド手順。tagName/releaseNameとポータブルexeの
  # ファイル名だけ NIGHTLY_TAG ベースに、releaseCommitish/prerelease/releaseDraftを
  # nightly向けに変えている。
  build:
    needs: [check, cleanup]
    if: needs.check.outputs.should_run == 'true'
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: ubuntu-latest
            args: ""
          - platform: macos-latest
            args: "--target universal-apple-darwin"
          - platform: windows-latest
            args: ""

    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4

      - name: Install system dependencies (Tauri v2 Linux)
        if: matrix.platform == 'ubuntu-latest'
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

      # dtolnay/rust-toolchain@stable is an intentionally moving branch that always
      # resolves the current stable Rust release, so it is left unpinned by design.
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.platform == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}

      - uses: Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae # v2
        with:
          workspaces: src-tauri

      # package.json の packageManager は frontend/ 配下にあり action-setup のデフォルト
      # 検出(リポジトリルート)では拾えないため version を明示指定する。
      - uses: pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa # v4
        with:
          version: 11.3.0

      - uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: frontend/pnpm-lock.yaml

      - name: pnpm install
        working-directory: frontend
        run: pnpm install --frozen-lockfile

      # Releaseの名称に使う日付・short shaを算出(全プラットフォームで同じ値になるよう
      # bashで統一。Windows runnerにもGit Bash由来のbashがある)。
      - name: Releaseメタデータ算出
        id: meta
        shell: bash
        run: |
          echo "date=$(date -u +%Y-%m-%d)" >> "$GITHUB_OUTPUT"
          echo "shortsha=$(echo "${{ github.sha }}" | cut -c1-7)" >> "$GITHUB_OUTPUT"

      - uses: tauri-apps/tauri-action@fce9c6108b31ea247710505d3aaaa893ee6768d4 # v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ env.NIGHTLY_TAG }}
          releaseName: "tsumugi nightly (${{ steps.meta.outputs.date }} @ ${{ steps.meta.outputs.shortsha }})"
          releaseCommitish: ${{ github.sha }}
          releaseDraft: false
          prerelease: true
          args: ${{ matrix.args }}

      # Windows は既定で MSI/NSIS インストーラのみが生成されるため、
      # インストーラ無しでそのまま実行できるポータブル版exeを別途リリース資産へ追加する。
      - name: Prepare portable exe (Windows only)
        if: matrix.platform == 'windows-latest'
        shell: pwsh
        run: Copy-Item src-tauri/target/release/tsumugi.exe "tsumugi-$env:NIGHTLY_TAG-portable-windows-x64.exe"

      - name: Upload portable exe to release (Windows only)
        if: matrix.platform == 'windows-latest'
        uses: softprops/action-gh-release@7c4723f7a335432393329f8f1c564994ce50185d # v3
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tag_name: ${{ env.NIGHTLY_TAG }}
          target_commitish: ${{ github.sha }}
          draft: false
          prerelease: true
          files: tsumugi-${{ env.NIGHTLY_TAG }}-portable-windows-x64.exe
```

- [ ] **Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/nightly.yml'))" && echo OK`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/nightly.yml
git commit -m "ci: nightly buildワークフローのbuildジョブ(Linux/macOS/Windows)を追加"
```

---

### Task 5: `android` job

**Files:**
- Modify: `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes: `check.outputs.should_run` (Task 2), existence of `cleanup` job (Task 3).
- Produces: nothing consumed by other tasks.

- [ ] **Step 1: Append the `android` job**

Add to `.github/workflows/nightly.yml`, after the `build` job. This mirrors `release.yml`'s `android` job; differences called out inline.

```yaml

  # release.ymlのandroidジョブと同じビルド・署名手順。ファイル名とtag_name/
  # target_commitish/prerelease/draftのみnightly向けに変えている。
  android:
    needs: [check, cleanup]
    if: needs.check.outputs.should_run == 'true'
    runs-on: ubuntu-latest
    environment: "Android Build"
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4

      - uses: actions/setup-java@c1e323688fd81a25caa38c78aa6df2d33d3e20d9 # v4
        with:
          distribution: temurin
          java-version: "17"

      - uses: nttld/setup-ndk@ed92fe6cadad69be94a966a7ee3271275e62f779 # v1
        id: setup-ndk
        with:
          ndk-version: r27c
          local-cache: true

      # setup-ndk が展開する toolchain 内のシンボリックリンクは相対パスのままで、
      # 展開先ディレクトリが変わるとリンク切れになる(clang ラッパーが exit 127 で失敗する)。
      # tauri-apps/tauri の公式CIワークフローと同じ手当てで絶対パスに直す。
      - name: Restore Android NDK symlinks
        run: |
          directory="${{ steps.setup-ndk.outputs.ndk-path }}/toolchains/llvm/prebuilt/linux-x86_64/bin"
          find "$directory" -type l | while read -r link; do
              current_target=$(readlink "$link")
              new_target="$directory/$(basename "$current_target")"
              ln -sf "$new_target" "$link"
          done

      # dtolnay/rust-toolchain@stable is an intentionally moving branch that always
      # resolves the current stable Rust release, so it is left unpinned by design.
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-linux-android,armv7-linux-androideabi,i686-linux-android,x86_64-linux-android

      - uses: Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae # v2
        with:
          workspaces: src-tauri

      - name: Cache cargo-tauri CLI
        id: cache-tauri-cli
        uses: actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4
        with:
          path: ~/.cargo/bin/cargo-tauri
          key: cargo-tauri-cli-v2-${{ runner.os }}

      - name: Install tauri-cli
        if: steps.cache-tauri-cli.outputs.cache-hit != 'true'
        run: cargo install tauri-cli --version "^2" --locked

      # package.json の packageManager は frontend/ 配下にあり action-setup のデフォルト
      # 検出(リポジトリルート)では拾えないため version を明示指定する。
      - uses: pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa # v4
        with:
          version: 11.3.0

      - uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: frontend/pnpm-lock.yaml

      - name: pnpm install
        working-directory: frontend
        run: pnpm install --frozen-lockfile

      # https://tauri.app/distribute/sign/android/ の CI 手順どおり、署名鍵を
      # 都度 keystore.properties として生成する(リポジトリには含めない・.gitignore済み)。
      - name: Set up Android signing
        env:
          ANDROID_KEY_ALIAS: ${{ secrets.ANDROID_KEY_ALIAS }}
          ANDROID_KEY_PASSWORD: ${{ secrets.ANDROID_KEY_PASSWORD }}
          ANDROID_KEY_BASE64: ${{ secrets.ANDROID_KEY_BASE64 }}
        run: |
          cd src-tauri/gen/android
          echo "keyAlias=$ANDROID_KEY_ALIAS" > keystore.properties
          echo "password=$ANDROID_KEY_PASSWORD" >> keystore.properties
          base64 -d <<< "$ANDROID_KEY_BASE64" > "$RUNNER_TEMP/keystore.jks"
          echo "storeFile=$RUNNER_TEMP/keystore.jks" >> keystore.properties

      # cargo-mobile2 の実装上、--split-per-abi はuniversalビルドと排他
      # (付けるとassembleUniversal*系ではなくassemble{Arch}*系のみが走る)。
      # universal・アーキテクチャ別の両方が欲しいため2回に分けて実行する。
      # 2回目のRustクロスコンパイルは1回目でuniversal向けに全arch分ビルド済み
      # (buildSrcのuniversalフレーバーが全archのrustBuildタスクに依存しているため)
      # キャッシュが効きほぼ再ビルドは発生しない。
      - name: cargo tauri android build (release, 署名済み, universal)
        working-directory: src-tauri
        env:
          NDK_HOME: ${{ steps.setup-ndk.outputs.ndk-path }}
        run: cargo tauri android build

      - name: cargo tauri android build (release, 署名済み, per-ABI)
        working-directory: src-tauri
        env:
          NDK_HOME: ${{ steps.setup-ndk.outputs.ndk-path }}
        run: cargo tauri android build --split-per-abi

      # 全部入りのuniversal APKはファイルサイズが大きくなるため(Issue #69)、
      # アーキテクチャ別APK(cargo tauri android build が productFlavor 経由で
      # 同時に生成済み。src-tauri/gen/android/buildSrc の RustPlugin.kt 参照)も
      # 個別にリネームし、universalと併せてリリース資産に含める。
      - name: Rename Android artifacts
        run: |
          cp src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk \
            "tsumugi-$NIGHTLY_TAG-android-universal.apk"
          cp src-tauri/gen/android/app/build/outputs/apk/arm64/release/app-arm64-release.apk \
            "tsumugi-$NIGHTLY_TAG-android-arm64-v8a.apk"
          cp src-tauri/gen/android/app/build/outputs/apk/arm/release/app-arm-release.apk \
            "tsumugi-$NIGHTLY_TAG-android-armeabi-v7a.apk"
          cp src-tauri/gen/android/app/build/outputs/apk/x86/release/app-x86-release.apk \
            "tsumugi-$NIGHTLY_TAG-android-x86.apk"
          cp src-tauri/gen/android/app/build/outputs/apk/x86_64/release/app-x86_64-release.apk \
            "tsumugi-$NIGHTLY_TAG-android-x86_64.apk"

      - name: Upload Android artifacts to release
        uses: softprops/action-gh-release@7c4723f7a335432393329f8f1c564994ce50185d # v3
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tag_name: ${{ env.NIGHTLY_TAG }}
          target_commitish: ${{ github.sha }}
          draft: false
          prerelease: true
          files: |
            tsumugi-${{ env.NIGHTLY_TAG }}-android-universal.apk
            tsumugi-${{ env.NIGHTLY_TAG }}-android-arm64-v8a.apk
            tsumugi-${{ env.NIGHTLY_TAG }}-android-armeabi-v7a.apk
            tsumugi-${{ env.NIGHTLY_TAG }}-android-x86.apk
            tsumugi-${{ env.NIGHTLY_TAG }}-android-x86_64.apk
```

- [ ] **Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/nightly.yml'))" && echo OK`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/nightly.yml
git commit -m "ci: nightly buildワークフローのandroidジョブを追加"
```

---

### Task 6: actionlint verification

**Files:**
- No file changes — this task only verifies Tasks 1–5's output.

**Interfaces:**
- Consumes: the finished `.github/workflows/nightly.yml` (Tasks 2–5) and `.github/workflows/update-pkgbuild.yml` (Task 1).
- Produces: nothing (verification-only task; if it finds problems, fix them in place and re-run).

- [ ] **Step 1: Install pinned actionlint**

Run:
```bash
mkdir -p /tmp/actionlint-bin
curl -sSL https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_linux_amd64.tar.gz \
  | tar -xz -C /tmp/actionlint-bin actionlint
/tmp/actionlint-bin/actionlint --version
```
Expected: prints `1.7.12`.

- [ ] **Step 2: Run actionlint against the new/changed workflows**

Run: `/tmp/actionlint-bin/actionlint .github/workflows/nightly.yml .github/workflows/update-pkgbuild.yml`
Expected: no output, exit code 0.

If it reports errors, fix `.github/workflows/nightly.yml` or `.github/workflows/update-pkgbuild.yml` in place (amend the relevant Task's commit) and re-run this step until clean.

- [ ] **Step 3: Run actionlint against the whole repo's workflows as a regression check**

Run: `/tmp/actionlint-bin/actionlint`
Expected: no output, exit code 0 (confirms Task 1's edit didn't break `update-pkgbuild.yml` for its normal, non-prerelease path, and that pre-existing workflows are still clean).

- [ ] **Step 4: Clean up the scratch binary**

Run: `rm -rf /tmp/actionlint-bin`

(No commit — this task made no file changes.)

---

## Post-merge manual verification (not automatable pre-merge)

GitHub only exposes `workflow_dispatch` for workflows present on the default branch, so `nightly.yml` cannot be dispatched from this feature branch. After this plan's branch is merged to `main`:

1. Actions tab → `nightly` workflow → **Run workflow** (manual `workflow_dispatch`).
2. Confirm all jobs (`check`, `cleanup`, `build` ×3, `android`) succeed. (The `Android Build` environment's deployment branch policy must allow `main` for the `android` job to run at all — this was added to the live repo settings during final review; if it's ever missing, add it via `gh api -X POST repos/onodai145/tsumugi/environments/Android%20Build/deployment-branch-policies -f name=main -f type=branch`.)
3. Open the resulting `nightly` Release on GitHub and confirm: tagged `nightly`, marked **Pre-release** (not Latest), title reads `tsumugi nightly (<date> @ <shortsha>)` (not a bare `nightly` — a cold race between the `build` and `android` jobs on Release creation could otherwise leave the tauri-action-set title unset), the portable exe and five Android APKs are named `tsumugi-nightly-...`, and the Android APKs are signed (installable, not "package appears to be corrupt"). The Linux/macOS/Windows installer bundles from tauri-action are named after the **app version** (e.g. `tsumugi_0.10.0_amd64.deb`), not the tag — this matches `release.yml`'s existing behavior and is expected, not a bug; it does mean a nightly desktop installer is filename-identical to the real latest release's installer, so don't rely on the filename to tell them apart, only the Release's Pre-release badge.
4. Run **Run workflow** a second time with no intervening commits to `main`; confirm every job is skipped (visible as "skipped" in the Actions run summary). Note: a manual `workflow_dispatch` always forces `should_run=true` by design (see nightly.yml's `check` job) — the `should_run=false` skip path can only be observed on a real `schedule` run with no intervening commits, not via a second manual dispatch. To actually see a skip, wait for the next scheduled run after step 2 with no commits to `main` in between, and confirm `check` reports `should_run=false` and `cleanup`/`build`/`android` show as skipped.
5. Push a trivial commit to `main` and re-run; confirm `should_run=true` again and the Release's assets get regenerated with the new commit's short SHA in the release name.
6. Check the Actions tab for the `update-pkgbuild` workflow: confirm it did **not** run for the nightly Release's `published` event (or, if it did run, confirm its single job shows as skipped). This is what Task 1's `prerelease == false` guard exists to prevent.
