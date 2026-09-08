# Nightly Build 設計 (Issue #299)

## 目的

`main` は常にビルド可能であるべき、という前提のもと、`main` への差分がある日だけ
文字通りの Nightly Build を行い、動作確認用の未署名/開発版バイナリを毎日自動で
配布する。正式リリース(`release.yml`、タグ `vX.Y.Z` 契機)とは独立した仕組みとする。

## トリガー

- `schedule`: `cron: '0 18 * * *'` (UTC 18:00 = JST 3:00)
- `workflow_dispatch`: 手動実行。手動実行時は差分チェックを無条件でスキップして
  常にビルドする(検証用途)。

## 差分検知

`nightly` タグが指すコミット SHA と `main` の現在の HEAD を比較する。

- `nightly` タグが存在しない(初回実行) → ビルドする
- 存在し、かつ HEAD と異なる SHA を指している → ビルドする
- 存在し、かつ HEAD と同じ SHA を指している → 何もせず終了(前日ビルドをそのまま
  維持する。Release の再作成・再アップロードは行わない)

`workflow_dispatch` の場合はこの判定を無視して常にビルドする。

比較は `git ls-remote` でタグの SHA を取得し、`github.sha` (= `main` HEAD)と文字列
比較するだけで行う。フルチェックアウトは不要。

## ジョブ構成

`.github/workflows/nightly.yml` を新設し、`release.yml` の各ビルドジョブをベースに
以下の構成にする。

1. **check** — 上記の差分検知を行い、`should_run` を output する。
2. **cleanup** (`needs: check`, `if: should_run`) — 既存の `nightly` Release と
   タグを `gh release delete nightly --yes --cleanup-tag` で完全に削除してから
   作り直す。前日以前のアセットを残さないため。
3. **build** (`needs: [check, cleanup]`, matrix: ubuntu-latest / macos-latest /
   windows-latest) — `release.yml` の `release` ジョブと同一のビルド手順
   (`tauri-apps/tauri-action` 使用)。差分点のみ:
   - `tagName: nightly` (固定値)
   - `releaseName: "tsumugi nightly (<YYYY-MM-DD> @ <shortsha>)"`
   - `releaseDraft: false`
   - `prerelease: true`
   - アプリのバージョン番号 (`Cargo.toml` / `tauri.conf.json`) は変更しない。
     `scripts/release.sh` によるバージョンバンプは行わない。
   - Windows portable exe の同梱は `release.yml` と同様に行う。
4. **android** (`needs: [check, cleanup]`) — `release.yml` の `android` ジョブと
   同一手順(署名も同じ `ANDROID_KEY_ALIAS` / `ANDROID_KEY_PASSWORD` /
   `ANDROID_KEY_BASE64` secrets を使用)。差分点のみ:
   - `softprops/action-gh-release` の `tag_name: nightly`
   - `draft: false`

いずれのジョブも `if: needs.check.outputs.should_run == 'true'` を条件に持たせ、
差分なし判定時はスキップする(`workflow_dispatch` の場合は check ジョブの
output 自体を強制的に `true` にする)。

## 成果物

`nightly` タグ 1 本に紐づく単一の pre-release に以下をまとめる:

- Linux installer (deb/AppImage 等、tauri-action の既定出力)
- macOS universal installer (dmg)
- Windows installer (msi/nsis) + portable exe
- Android APK: universal / arm64-v8a / armeabi-v7a / x86 / x86_64 (署名済み)

ファイル名は `release.yml` と同じ命名規則を踏襲し、`REF_NAME` 部分が `nightly`
になる(例: `tsumugi-nightly-portable-windows-x64.exe`)。

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

## スコープ外

- Nightly ビルドの自動更新機能(アプリ内での nightly→nightly 自動アップデート)は
  作らない。
- nightly ビルドの成果物に対する自動テスト・E2E は行わない(`test.yml` の
  push/PR 契機のテストで担保する)。
- 過去の nightly ビルド履歴の保持は行わない(常に前日分を削除して上書き)。
