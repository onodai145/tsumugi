# Nightly Build 設計 (Issue #299)

## 目的

`main` は常にビルド可能であるべき、という前提のもと、`main` への差分がある日だけ
文字通りの Nightly Build を行い、動作確認用のビルドを毎日自動で配布する。
正式リリース(`release.yml`、タグ `vX.Y.Z` 契機)とは独立した仕組みとする。

## トリガー

- `schedule`: `cron: '0 18 * * *'` (UTC 18:00 = JST 3:00)
- `workflow_dispatch`: 手動実行。手動実行時は差分チェックを無条件でスキップして
  常にビルドする(検証用途)。

ワークフロー全体に `concurrency: { group: nightly, cancel-in-progress: false }`
を設定する。削除→再作成という手順を踏むため、`workflow_dispatch` による手動実行
と定期実行が重なると互いの Release/タグ操作が競合しうるのを防ぐ。

## 差分検知

`nightly` タグが指すコミットと `main` の現在の HEAD (`github.sha`) を比較する。

- `nightly` タグが存在しない(初回実行) → ビルドする
- 存在し、かつ HEAD と異なるコミットを指している → ビルドする
- 存在し、かつ HEAD と同じコミットを指している → 何もせず終了(前日ビルドを
  そのまま維持する。Release の再作成・再アップロードは行わない)

`workflow_dispatch` の場合はこの判定を無視して常にビルドする。

タグの指すコミット SHA は `gh api repos/$GITHUB_REPOSITORY/commits/nightly
--jq .sha` で取得する(`git ls-remote --tags` はタグが annotated tag の場合に
タグオブジェクト自体の SHA を返しコミット SHA と一致しなくなるため使わない。
`commits/{ref}` エンドポイントは ref の種別によらず常にコミット SHA に解決する)。
タグが存在しない場合はこの呼び出しが失敗するので、その場合は「初回実行」として
扱う。

## ジョブ構成

`.github/workflows/nightly.yml` を新設し、`release.yml` の各ビルドジョブをベースに
以下の構成にする。以降 `NIGHTLY_TAG: nightly` をワークフロー共通の環境変数として
定義し、`release.yml` で `github.ref_name` を使っている箇所(ファイル名・
`tagName`/`tag_name`)はすべてこれに置き換える(`github.ref_name` はスケジュール
実行では `main` になり、そのまま使うとファイル名が `tsumugi-main-...` になって
しまうため)。

1. **check** — 上記の差分検知を行い、`should_run` を output する。
2. **cleanup** (`needs: check`, `if: should_run`) — 既存の `nightly` Release と
   タグを削除してから作り直す。初回実行時は削除対象が存在せず失敗するため、
   `gh release delete "$NIGHTLY_TAG" --yes --cleanup-tag || true` のように
   失敗を許容する形で実行する。
3. **build** (`needs: [check, cleanup]`, matrix: ubuntu-latest / macos-latest /
   windows-latest) — `release.yml` の `release` ジョブと同一のビルド手順
   (`tauri-apps/tauri-action` 使用)。差分点のみ:
   - `tagName: ${{ env.NIGHTLY_TAG }}`
   - `releaseName: "tsumugi nightly (<YYYY-MM-DD> @ <shortsha>)"`
   - `releaseCommitish: ${{ github.sha }}` — cleanup でタグを削除しているため
     明示しないとタグ作成時点のデフォルトブランチ HEAD に打たれてしまい、
     check が比較したコミットとズレて次回以降の差分検知が壊れる。
   - `releaseDraft: false`
   - `prerelease: true`
   - アプリのバージョン番号 (`Cargo.toml` / `tauri.conf.json`) は変更しない。
     `scripts/release.sh` によるバージョンバンプは行わない。
   - Windows portable exe の同梱は `release.yml` と同様に行う(ファイル名は
     `NIGHTLY_TAG` を使う)。
4. **android** (`needs: [check, cleanup]`) — `release.yml` の `android` ジョブと
   同一手順(署名も同じ `ANDROID_KEY_ALIAS` / `ANDROID_KEY_PASSWORD` /
   `ANDROID_KEY_BASE64` secrets を使用)。差分点のみ:
   - `softprops/action-gh-release` の `tag_name: ${{ env.NIGHTLY_TAG }}`
   - `target_commitish: ${{ github.sha }}` (build ジョブの `releaseCommitish`
     と同じ理由)
   - `draft: false`
   - `prerelease: true`

**build と android の両方が `prerelease: true` / `draft: false` を明示すること。**
`tauri-action`・`softprops/action-gh-release` はどちらも対象タグの Release が
存在しなければ新規作成する。cleanup でタグを消しているため、build と android の
どちらが先に完了して Release を新規作成するかはジョブの実行順に依存する。
片方だけ `prerelease` を明示していないと、そのジョブが先に完了した場合に
`prerelease` 未指定(=false)の Release が作られてしまい、
「更新通知への非干渉性の確認」で前提にしている `prerelease: true` が崩れる
(`/releases/latest` が nightly を返すようになり、既存ユーザーへの更新通知が
止まる)。

いずれのジョブも `if: needs.check.outputs.should_run == 'true'` を条件に持たせ、
差分なし判定時はスキップする(`workflow_dispatch` の場合は check ジョブの
output 自体を強制的に `true` にする)。

## 成果物

`nightly` タグ 1 本に紐づく単一の pre-release に以下をまとめる:

- Linux installer (deb/AppImage 等、tauri-action の既定出力)
- macOS universal installer (dmg)
- Windows installer (msi/nsis) + portable exe
- Android APK: universal / arm64-v8a / armeabi-v7a / x86 / x86_64 (署名済み。
  正式リリースと同じ署名鍵を使う)

ファイル名は `release.yml` と同じ命名規則を踏襲し、`REF_NAME` 相当の部分が
`NIGHTLY_TAG`(= `nightly`)になる(例: `tsumugi-nightly-portable-windows-x64.exe`)。

## 更新通知への非干渉性の確認

`src-tauri/src/commands/app.rs` の `check_latest_release` は
`GET /repos/{repo}/releases/latest` を叩く。この API は元々 `draft`/`prerelease`
を除外して返す仕様であり、加えて実装側でも `rel.draft || rel.prerelease` の場合は
`None` を返す二重チェックがある。`prerelease: true` で公開する nightly Release は
アプリ本体の更新チェックに一切影響しない。

## 既存ワークフローへの影響: update-pkgbuild.yml

`.github/workflows/update-pkgbuild.yml` は `release: types: [released, published]`
で起動し、`prerelease` かどうかを問わず発火する。nightly Release は
`prerelease: true, draft: false` = GitHub 上は "published" イベントとして扱われる
ため、このままでは毎晩誤発火し、`TAG=nightly` で `scripts/update-pkgbuild.py` が
実行されてしまう(存在しない URL へのアクセスで失敗する、または PKGBUILD を
誤った内容で更新する PR が毎晩作られる)。

対策として `update-pkgbuild.yml` の `update` ジョブに
`if: github.event.release.prerelease == false` を追加し、prerelease な
Release(nightly を含む将来の prerelease 運用全般)ではこのジョブをスキップする。
正式リリース(`prerelease: false`)の挙動は変わらない。

## 検証方法

GitHub は `workflow_dispatch` をデフォルトブランチ上に存在するワークフローに
対してしか公開しない。そのため PR 段階では `workflow_dispatch` による実地確認は
できない。検証は以下の順で行う:

1. ローカルで `actionlint`(バージョン固定でインストール)を `nightly.yml` に
   対して実行し、構文・式エラーを検出する。
2. `main` にマージされた後、Actions タブから `workflow_dispatch` で手動実行し、
   実際に `nightly` タグの Release が作られることを確認する。
3. 作られた Release の `prerelease` フラグが `true`、`draft` が `false`、
   アセット名に `main`(= `github.ref_name` の誤用)が混入していないことを
   目視確認する。
4. 変更なしで再度手動実行しても(`workflow_dispatch` なので差分チェックは
   スキップされ)問題なく再度ビルド・上書きできることを確認する。

## スコープ外

- Nightly ビルドの自動更新機能(アプリ内での nightly→nightly 自動アップデート)は
  作らない。
- nightly ビルドの成果物に対する自動テスト・E2E は行わない(`test.yml` の
  push/PR 契機のテストで担保する)。
- 過去の nightly ビルド履歴の保持は行わない(常に前日分を削除して上書き)。
