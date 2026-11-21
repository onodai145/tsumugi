#!/usr/bin/env bash
# scripts/check-apk-libcxx-shared.sh のテスト。python3 で、中身の違う zip(APK の代わり)を作って確かめる。
#
# 使い方: scripts/check-apk-libcxx-shared.test.sh
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
check="$here/check-apk-libcxx-shared.sh"
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

mkzip() { # mkzip <出力> <エントリ...>
  local out=$1; shift
  python3 - "$out" "$@" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1], "w") as z:
    for name in sys.argv[2:]:
        z.writestr(name, b"x")
PY
}

dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
mkzip "$dir/both.apk" lib/arm64-v8a/libc++_shared.so lib/x86_64/libc++_shared.so lib/arm64-v8a/libtsumugi_lib.so
mkzip "$dir/no-x86.apk" lib/arm64-v8a/libc++_shared.so lib/x86_64/libtsumugi_lib.so
mkzip "$dir/no-arm.apk" lib/x86_64/libc++_shared.so lib/arm64-v8a/libtsumugi_lib.so
mkzip "$dir/none.apk" lib/arm64-v8a/libtsumugi_lib.so lib/x86_64/libtsumugi_lib.so
mkzip "$dir/wrong-dir.apk" assets/lib/arm64-v8a/libc++_shared.so assets/lib/x86_64/libc++_shared.so
echo "not a zip" > "$dir/bad.apk"

expect ok "arm64-v8a と x86_64 の両方に libc++_shared.so があれば OK" -- "$check" "$dir/both.apk"
expect ng "x86_64 の libc++_shared.so が無ければ NG" -- "$check" "$dir/no-x86.apk"
expect ng "arm64-v8a の libc++_shared.so が無ければ NG" -- "$check" "$dir/no-arm.apk"
expect ng "どちらも無ければ NG" -- "$check" "$dir/none.apk"
expect ng "lib/ 直下でない(別のディレクトリの)同名ファイルは数えない" -- "$check" "$dir/wrong-dir.apk"
expect ng "APK が存在しないときは NG" -- "$check" /nonexistent/app.apk
expect ng "zip でないファイルは NG" -- "$check" "$dir/bad.apk"

exit $fail
