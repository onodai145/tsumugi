#!/usr/bin/env bash
# リリース準備を自動化するスクリプト。
# frontend/package.json / src-tauri/Cargo.toml / src-tauri/tauri.conf.json /
# src-tauri/Cargo.lock のバージョンを揃えて更新し、CHANGELOG.md を生成した上で
# release/vX.Y.Z ブランチを作成してコミットする。
# PR作成・マージ・タグpushは手動のまま。
#
# Usage: scripts/release.sh <X.Y.Z>
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: scripts/release.sh <X.Y.Z>" >&2
  exit 1
fi

VERSION="$1"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version must be in X.Y.Z format (got: $VERSION)" >&2
  exit 1
fi
TAG="v$VERSION"

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

if [ -n "$(git status --porcelain)" ]; then
  echo "error: working tree is not clean" >&2
  exit 1
fi

OLD_VERSION="$(grep -m1 '^version = "' src-tauri/Cargo.toml | sed -E 's/^version = "(.*)"$/\1/')"

git checkout -b "release/$TAG"

sed -i "s/^  \"version\": \".*\",\$/  \"version\": \"$VERSION\",/" frontend/package.json
grep -q "\"version\": \"$VERSION\"," frontend/package.json || { echo "error: failed to bump frontend/package.json" >&2; exit 1; }

sed -i "s/^  \"version\": \".*\",\$/  \"version\": \"$VERSION\",/" src-tauri/tauri.conf.json
grep -q "\"version\": \"$VERSION\"," src-tauri/tauri.conf.json || { echo "error: failed to bump src-tauri/tauri.conf.json" >&2; exit 1; }

sed -i "s/^version = \".*\"\$/version = \"$VERSION\"/" src-tauri/Cargo.toml
grep -q "^version = \"$VERSION\"\$" src-tauri/Cargo.toml || { echo "error: failed to bump src-tauri/Cargo.toml" >&2; exit 1; }

(cd src-tauri && cargo update -p tsumugi)

sed -i "/<!-- release-download-links:start -->/,/<!-- release-download-links:end -->/ s/$OLD_VERSION/$VERSION/g" README.md
grep -q "$VERSION" README.md || { echo "error: failed to bump README.md download links" >&2; exit 1; }

git-cliff --tag "$TAG" --unreleased --prepend CHANGELOG.md

git add frontend/package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock CHANGELOG.md README.md
git commit -m "chore(release): $TAG"

echo "done: release/$TAG committed. Push, open a PR, merge, then tag manually."
