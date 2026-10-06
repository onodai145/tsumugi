#!/usr/bin/env bash
# 一時HOME(E2E_REUSE_HOME_FILE)を再利用しながらwdioを実行し、終わったらそのHOMEを消す。
# 再起動をまたぐシナリオと、アプリのキャッシュDB/設定JSONを検査するシナリオ用(run-app.shの
# E2E_REUSE_HOME_FILEは、指定されたHOMEを消さずに残すため、ここで後始末する)。
#
# 使い方: scripts/run-reuse-home.sh <名前> <spec>...    (e2e/ で実行)
set -euo pipefail
name="$1"
shift
mkdir -p wdio-logs
export E2E_REUSE_HOME_FILE="./wdio-logs/${name}-home.txt"
rm -f "$E2E_REUSE_HOME_FILE" wdio-logs/restart-state.json
cleanup() {
  if [ -s "$E2E_REUSE_HOME_FILE" ]; then
    home="$(cat "$E2E_REUSE_HOME_FILE")"
    case "$home" in
      /tmp/tsumugi-e2e-?*) rm -rf -- "$home" ;;
    esac
  fi
  rm -f "$E2E_REUSE_HOME_FILE"
}
trap cleanup EXIT
specs=()
for s in "$@"; do specs+=(--spec "$s"); done
NODE_EXTRA_CA_CERTS=./certs/ca.pem pnpm exec wdio run wdio.restart.conf.ts "${specs[@]}"
