#!/usr/bin/env bash
# Android(64bit)の libtsumugi_lib.so の LOAD セグメントが、16KB 以上でアラインされていることを確認する(Issue #603)。
#
# 4KB アラインのままだと、16KB ページの端末(Android 15 以降)で「16 KB ページサイズに未対応」の警告ダイアログが
# 起動のたびに出て、アプリは互換モードで動く(Google Play も、新しいアプリには 16KB 対応を求める)。
# NDK r27 は 16KB を既定にしないので、`src-tauri/.cargo/config.toml` の `-Wl,-z,max-page-size=16384` で揃えている。
# 32bit の ABI(armeabi-v7a / x86)は対象外。
#
# 使い方: scripts/check-android-page-size.sh <libtsumugi_lib.so> [llvm-readelf のパス]
set -euo pipefail

so=${1:?usage: $0 <libtsumugi_lib.so> [llvm-readelf]}
readelf=${2:-llvm-readelf}
need=16384

# 検査そのものができないとき(.so が無い・llvm-readelf が無い、壊れている)は、「OK」にせず失敗させる。
if [ ! -f "$so" ]; then
  echo "NG: $so が見つかりません" >&2
  exit 1
fi
if ! out=$("$readelf" -lW "$so"); then
  echo "NG: $readelf で $so を読めませんでした" >&2
  exit 1
fi

# `LOAD ... <flags> <align>` の行の、最後の列(16 進)を見る。LOAD が 1 つも読めなければ、検査できていないので NG。
loads=0
bad=""
while read -r kind rest; do
  [ "$kind" = LOAD ] || continue
  loads=$((loads + 1))
  align=${rest##* }
  if ! [[ $align =~ ^0x[0-9a-fA-F]+$ ]] || (( align < need )); then
    bad+="  LOAD align=$align"$'\n'
  fi
done <<<"$out"

if [ "$loads" -eq 0 ]; then
  echo "NG: $so から LOAD セグメントを読めませんでした" >&2
  exit 1
fi
if [ -n "$bad" ]; then
  echo "NG: $so に、16KB($need)未満でアラインされた LOAD セグメントがあります(max-page-size の指定漏れの疑い):" >&2
  printf '%s' "$bad" >&2
  exit 1
fi
echo "OK: $so の LOAD セグメントは、すべて 16KB 以上でアラインされています"
