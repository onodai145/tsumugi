#!/usr/bin/env bash
# Android の libtsumugi_lib.so に、C++ ランタイム(libc++)の未解決シンボルが残っていないことを確認する(Issue #603)。
#
# llama.cpp は libc++ を静的にリンクする(`android-static-stdcxx`)ので、リンク後に libc++ の記号は残らないはず。
# 残っているのは、llama.cpp を、リンク先とは別の版の NDK(libc++)でコンパイルしたとき(例: llama-cpp-sys-2 は
# ANDROID_NDK / ANDROID_NDK_ROOT / NDK_ROOT が無いと SDK 内で最新の NDK を使う)。リンクは通るが、端末で
# `dlopen failed: cannot locate symbol "_ZNSt6__ndk113__hash_memoryEPKvm"` となって、起動時にクラッシュする。
#
# 使い方: scripts/check-android-libcxx.sh <libtsumugi_lib.so> [llvm-nm のパス]
set -euo pipefail

so=${1:?usage: $0 <libtsumugi_lib.so> [llvm-nm]}
nm=${2:-llvm-nm}

# 未解決(U)の libc++ の記号(`std::__ndk1::` と、素の `std::__1::`)。
bad=$("$nm" -D --undefined-only "$so" | grep -E ' U _ZNSt(6__ndk1|3__1)' || true)
if [ -n "$bad" ]; then
  echo "NG: $so に、libc++ の未解決シンボルが残っています(llama.cpp を別の版の NDK でコンパイルした疑い):" >&2
  echo "$bad" | head -10 >&2
  exit 1
fi
echo "OK: $so に libc++ の未解決シンボルはありません"
