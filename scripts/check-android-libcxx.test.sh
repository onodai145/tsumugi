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

# 本物の libtsumugi_lib.so を用意せずに、記号の並び(`nm -D --undefined-only` の出力)だけを差し替えて確かめる。
# 偽の nm は、決まった出力を返す(.so は存在すればよい)。
fake_dir=$(mktemp -d)
trap 'rm -rf "$fake_dir"' EXIT
fake_nm="$fake_dir/nm"
dummy_so="$fake_dir/libdummy.so"
: > "$dummy_so"
nm_prints() { printf '#!/usr/bin/env bash\ncat <<'"'"'EOF'"'"'\n%s\nEOF\n' "$1" > "$fake_nm"; chmod +x "$fake_nm"; }

# 強参照(U)の libc++ の記号は、名前の形によらず NG。レビュー指摘: 以前は `_ZNSt…` 始まりしか拾わなかった。
nm_prints '                 U _ZNSt6__ndk113__hash_memoryEPKvm'
expect ng "_ZNSt6__ndk1(非 const メンバ)は NG" -- "$check" "$dummy_so" "$fake_nm"
nm_prints '                 U _ZNKSt6__ndk112basic_stringIcNS_11char_traitsIcEENS_9allocatorIcEEE4sizeEv'
expect ng "_ZNKSt6__ndk1(const メンバ)は NG" -- "$check" "$dummy_so" "$fake_nm"
nm_prints '                 U _ZTVNSt6__ndk118basic_stringstreamIcNS_11char_traitsIcEENS_9allocatorIcEEEE'
expect ng "_ZTVNSt6__ndk1(vtable)は NG" -- "$check" "$dummy_so" "$fake_nm"
nm_prints '                 U _ZNKSt3__112basic_stringIcNS_11char_traitsIcEENS_9allocatorIcEEE4sizeEv'
expect ng "_ZNKSt3__1(素の __1 の const メンバ)は NG" -- "$check" "$dummy_so" "$fake_nm"
# libc・libm の記号や operator new は端末の libc などで解決される。弱参照(w)は、無くても dlopen は失敗しない。
nm_prints '                 U memcpy
                 U __cxa_atexit
                 U _Znwm
                 w _ZNSt6__ndk113__hash_memoryEPKvm'
expect ok "libc・libm・operator new と、弱参照の記号は OK" -- "$check" "$dummy_so" "$fake_nm"

exit $fail
