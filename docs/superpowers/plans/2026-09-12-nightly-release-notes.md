# Nightly Release notes自動生成 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nightly Releaseワークフロー(`.github/workflows/nightly.yml`)に、GitHub標準のauto-generate release notes機能で本文を確実に生成する専用`release`ジョブを追加する。

**Architecture:** `cleanup`の直後・`build`/`android`の前に`release`ジョブを新設し、`gh release create --generate-notes`で空(アセット無し)のReleaseを先に確定させる。`build`(tauri-action)と`android`(action-gh-release)はどちらも「Release不在なら新規作成、存在すればアセット追加のみ」という既存ロジックのまま`release`ジョブに依存させることで、Release notes生成ロジックを1箇所に集約し、並列実行時のレース条件を構造的に排除する。日付・shortsha算出は現在`build`ジョブの`meta`ステップにあるが、`release`ジョブへ移動し`outputs`経由で`build`ジョブへ渡す。

**Tech Stack:** GitHub Actions (YAML)、GitHub CLI (`gh`)、`tauri-apps/tauri-action@fce9c6108b31ea247710505d3aaaa893ee6768d4`、`softprops/action-gh-release@7c4723f7a335432393329f8f1c564994ce50185d`。

## Global Constraints

- 変更対象は`.github/workflows/nightly.yml`のみ。`.github/workflows/release.yml`(通常リリース)は変更しない。
- Release notesの比較対象タグは明示指定しない(GitHubの自動選択に任せる)。
- `release`ジョブは`needs: [check, cleanup]`、`if: needs.check.outputs.should_run == 'true'`を`cleanup`ジョブと同じ形式で踏襲する。
- 日付・shortshaの算出はbash(`date -u +%Y-%m-%d` / `echo "${{ github.sha }}" | cut -c1-7`)で、既存の`meta`ステップと同一の算出方法を使う(全プラットフォームで同じ値になる前提を崩さない)。
- コミットメッセージは件名のみ(本文・箇条書き無し)。Co-Authored-Byトレーラーは別途付与される。

---

### Task 1: `release`ジョブの新設とmeta算出の移動

**Files:**
- Modify: `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes: `needs.check.outputs.should_run`(既存、`check`ジョブが出力)。`env.NIGHTLY_TAG`(既存)。`github.sha`、`github.repository`(GitHub Actionsコンテキスト、既存)。
- Produces: `release`ジョブの`outputs.date`・`outputs.shortsha`(`build`ジョブが`needs.release.outputs.date` / `needs.release.outputs.shortsha`として参照する)。

- [ ] **Step 1: `release`ジョブを追加する**

`.github/workflows/nightly.yml`の`cleanup`ジョブ定義の直後(`build`ジョブの直前)に以下を挿入する:

```yaml
  # buildとandroidが並列でReleaseを新規作成しようとするとRelease notes生成の
  # レースが発生する(action-gh-releaseは`body`等未指定時に既存値を保持するだけで
  # 自ら生成はしないため、先に完了した側の結果に依存してしまう)。
  # このジョブが空のReleaseを`--generate-notes`付きで先に確定させることで、
  # build/androidはどちらも「既存Releaseへのアセット追加」のみになり、
  # notes生成ロジックが1箇所に集まる。
  release:
    needs: [check, cleanup]
    if: needs.check.outputs.should_run == 'true'
    runs-on: ubuntu-latest
    outputs:
      date: ${{ steps.meta.outputs.date }}
      shortsha: ${{ steps.meta.outputs.shortsha }}
    steps:
      # Releaseの名称に使う日付・short shaを算出(build/androidジョブへ
      # outputs経由で渡すため、算出はここでのみ行う)。
      - name: Releaseメタデータ算出
        id: meta
        run: |
          echo "date=$(date -u +%Y-%m-%d)" >> "$GITHUB_OUTPUT"
          echo "shortsha=$(echo "${{ github.sha }}" | cut -c1-7)" >> "$GITHUB_OUTPUT"

      - name: 空のnightly Releaseをnotes生成付きで作成
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release create "$NIGHTLY_TAG" \
            --repo "${{ github.repository }}" \
            --title "tsumugi nightly (${{ steps.meta.outputs.date }} @ ${{ steps.meta.outputs.shortsha }})" \
            --target "${{ github.sha }}" \
            --prerelease \
            --generate-notes
```

- [ ] **Step 2: `build`ジョブの`needs`に`release`を追加する**

`.github/workflows/nightly.yml`内、`build`ジョブの`needs: [check, cleanup]`を次のように変更する:

```yaml
  build:
    needs: [check, cleanup, release]
```

- [ ] **Step 3: `build`ジョブの`meta`ステップを削除する**

`build`ジョブ内の以下のステップ全体を削除する:

```yaml
      # Releaseの名称に使う日付・short shaを算出(全プラットフォームで同じ値になるよう
      # bashで統一。Windows runnerにもGit Bash由来のbashがある)。
      - name: Releaseメタデータ算出
        id: meta
        shell: bash
        run: |
          echo "date=$(date -u +%Y-%m-%d)" >> "$GITHUB_OUTPUT"
          echo "shortsha=$(echo "${{ github.sha }}" | cut -c1-7)" >> "$GITHUB_OUTPUT"
```

- [ ] **Step 4: `build`ジョブの`tauri-action`ステップの`releaseName`を`release`ジョブの出力参照に書き換える**

```yaml
          releaseName: "tsumugi nightly (${{ needs.release.outputs.date }} @ ${{ needs.release.outputs.shortsha }})"
```

- [ ] **Step 5: `android`ジョブの`needs`に`release`を追加する**

`android`ジョブの`needs: [check, cleanup]`を次のように変更する:

```yaml
  android:
    needs: [check, cleanup, release]
```

- [ ] **Step 6: YAML構文を検証する**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/nightly.yml'))" && echo OK`
Expected: `OK`が出力される(構文エラーが無い)。

- [ ] **Step 7: 差分を目視レビューする**

Run: `git diff .github/workflows/nightly.yml`
Expected: 以下がすべて反映されていること。
- `release`ジョブが`cleanup`の後・`build`の前に追加されている
- `build`ジョブの`needs`に`release`が追加され、`meta`ステップが削除され、`releaseName`が`needs.release.outputs.*`を参照している
- `android`ジョブの`needs`に`release`が追加されている
- `release.yml`には一切変更が無いこと(`git diff --stat`でファイル一覧を確認)

- [ ] **Step 8: コミット**

```bash
git add .github/workflows/nightly.yml
git commit -m "ci: Nightly Releaseの本文を専用ジョブでauto-generateする(Issue #333)"
```

---

## 補足: 動作確認について

この変更はGitHub Actions上でのRelease作成・並列ジョブの挙動に依存するため、ローカルでは完全な検証ができない。PR作成・マージ後、以下をユーザー自身がGitHub上で確認する(CI結果のポーリングはしない):

- `workflow_dispatch`で手動トリガーし、`release`ジョブが単独で成功すること
- `build`(3プラットフォーム)・`android`のログで、Release新規作成ではなく既存Releaseへのアセット追加になっていること(例: tauri-actionのログに"Found release with tag nightly."が出る)
- 生成された`nightly` Releaseの本文が、直近の安定版タグからの妥当な差分一覧になっていること
