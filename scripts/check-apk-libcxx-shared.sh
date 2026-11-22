#!/usr/bin/env bash
# APK に、64bit の ABI(arm64-v8a・x86_64)の libc++_shared.so が入っていることを確認する(Issue #625 段階3)。
#
# 内蔵 VOICEVOX CORE の libvoicevox_core.so は、共有の libc++_shared.so を要求する。アプリ本体は静的 libc++ なので、
# app/build.gradle.kts がビルド時に NDK のものを APK に入れている。そのタスクの紐付けが外れても、ビルドは成功し、
# 利用者には「取得は成功するのに、読み上げが SPEECH_CORE_LOAD_FAILED(エンジン一式を取得し直してください)」と出て、
# 取得し直しても直らない。だから、APK の中身で確かめる。
#
# 使い方: scripts/check-apk-libcxx-shared.sh <APK>
set -euo pipefail

apk=${1:?usage: $0 <apk>}

if [ ! -f "$apk" ]; then
  echo "NG: $apk が見つかりません" >&2
  exit 1
fi
if ! entries=$(unzip -Z1 "$apk" 2>/dev/null); then
  echo "NG: $apk を zip として読めませんでした" >&2
  exit 1
fi

missing=""
for abi in arm64-v8a x86_64; do
  entry="lib/$abi/libc++_shared.so"
  if ! grep -Fxq "$entry" <<<"$entries"; then
    missing+="  $entry"$'\n'
  fi
done
if [ -n "$missing" ]; then
  echo "NG: $apk に libc++_shared.so が入っていません(app/build.gradle.kts の copyLibcxxShared が実行されていない疑い):" >&2
  printf '%s' "$missing" >&2
  exit 1
fi
echo "OK: $apk に、arm64-v8a と x86_64 の libc++_shared.so が入っています"
