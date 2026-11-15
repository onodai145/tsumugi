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

# 検査そのものができないとき(.so が無い・llvm-nm が無い、壊れている)は、「OK」にせず失敗させる。
# (`nm | grep || true` と書くと、nm の失敗まで握りつぶして、何も検査しないまま緑になる。)
if [ ! -f "$so" ]; then
  echo "NG: $so が見つかりません(出力先の変更や ABI 名の誤りの疑い)" >&2
  exit 1
fi
if ! out=$("$nm" -D --undefined-only "$so"); then
  echo "NG: $nm で $so を読めませんでした" >&2
  exit 1
fi

# 未解決(U)の libc++ の記号(inline namespace `std::__ndk1::` と、素の `std::__1::`)。マングル名のどこに
# `6__ndk1` / `St3__1` が出ても拾う(`_ZNSt…` のほか、const メンバ `_ZNKSt…`、vtable `_ZTVNSt…` など)。
# 弱参照(w)は、無くても dlopen は失敗しないので見ない。grep の「該当なし」(終了コード 1)だけを許す。
bad=$(grep -E ' U [^ ]*(6__ndk1|St3__1)' <<<"$out" || true)
if [ -n "$bad" ]; then
  echo "NG: $so に、libc++ の未解決シンボルが残っています(llama.cpp を別の版の NDK でコンパイルした疑い):" >&2
  echo "$bad" | head -10 >&2
  exit 1
fi
echo "OK: $so に libc++ の未解決シンボルはありません"
