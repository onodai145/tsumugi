#!/usr/bin/env bash
# scripts/check-android-libcxx.sh のテスト。検査そのものが失敗したとき(.so が無い、llvm-nm が無い・壊れている)に、
# 「OK」で通さず失敗することを確かめる(レビュー指摘: `|| true` が llvm-nm の失敗まで握りつぶしていた)。
#
# 使い方: scripts/check-android-libcxx.test.sh <llvm-nm のパス>
set -uo pipefail

nm=${1:?usage: $0 <llvm-nm>}
here=$(cd "$(dirname "$0")" && pwd)
check="$here/check-android-libcxx.sh"
fail=0

expect() { # expect <期待する終了コード: ok|ng> <説明> -- <コマンド...>
  local want=$1 desc=$2; shift 3
  local out; out=$("$@" 2>&1); local rc=$?
  if { [ "$want" = ok ] && [ $rc -eq 0 ]; } || { [ "$want" = ng ] && [ $rc -ne 0 ]; }; then
    echo "PASS: $desc"
  else
    echo "FAIL: $desc (exit=$rc, 期待=$want)"; echo "$out" | head -3 | sed 's/^/    /'; fail=1
  fi
}

# libc++ の記号を持たない普通の ELF(ホストの /bin/ls)は OK になる。
expect ok "libc++ の未解決シンボルが無い ELF は OK" -- "$check" /bin/ls "$nm"
# .so が無いときは、検査できていないので NG。
expect ng ".so が存在しないときは NG" -- "$check" /nonexistent/libtsumugi_lib.so "$nm"
# llvm-nm が無い・実行できないときも、検査できていないので NG。
expect ng "llvm-nm が存在しないときは NG" -- "$check" /bin/ls /nonexistent/llvm-nm
# ELF でないファイルを渡したときも NG(nm が失敗する)。
tmp=$(mktemp); echo "not an elf" > "$tmp"
expect ng "ELF でないファイルは NG" -- "$check" "$tmp" "$nm"
rm -f "$tmp"

exit $fail
