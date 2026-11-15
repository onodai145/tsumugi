#!/usr/bin/env bash
# scripts/check-android-page-size.sh のテスト。ホストの cc で、4KB と 16KB のアラインメントの共有ライブラリを作って確かめる。
#
# 使い方: scripts/check-android-page-size.test.sh <llvm-readelf のパス>
set -uo pipefail

readelf=${1:?usage: $0 <llvm-readelf>}
here=$(cd "$(dirname "$0")" && pwd)
check="$here/check-android-page-size.sh"
fail=0

expect() { # expect <ok|ng> <説明> -- <コマンド...>
  local want=$1 desc=$2; shift 3
  local out; out=$("$@" 2>&1); local rc=$?
  if { [ "$want" = ok ] && [ $rc -eq 0 ]; } || { [ "$want" = ng ] && [ $rc -ne 0 ]; }; then
    echo "PASS: $desc"
  else
    echo "FAIL: $desc (exit=$rc, 期待=$want)"; echo "$out" | head -3 | sed 's/^/    /'; fail=1
  fi
}

dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
echo 'int f(void){return 1;}' > "$dir/a.c"
cc -shared -fPIC -Wl,-z,max-page-size=4096 -o "$dir/p4k.so" "$dir/a.c"
cc -shared -fPIC -Wl,-z,max-page-size=16384 -o "$dir/p16k.so" "$dir/a.c"

expect ng "4KB アラインメントの .so は NG" -- "$check" "$dir/p4k.so" "$readelf"
expect ok "16KB アラインメントの .so は OK" -- "$check" "$dir/p16k.so" "$readelf"
expect ng ".so が存在しないときは NG" -- "$check" /nonexistent/libtsumugi_lib.so "$readelf"
expect ng "llvm-readelf が存在しないときは NG" -- "$check" "$dir/p16k.so" /nonexistent/llvm-readelf
echo "not an elf" > "$dir/bad.so"
expect ng "ELF でないファイルは NG(LOAD が 1 つも読めない)" -- "$check" "$dir/bad.so" "$readelf"

exit $fail
