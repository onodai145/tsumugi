#!/usr/bin/env bash
# tsumugi本体を、本番の設定ディレクトリ・OS keyringから完全に分離した状態で起動する。
# 引数はそのままtsumugiバイナリへ渡す（tauri-driver/wdio-tauri-serviceが付与する引数を透過）。
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BINARY="$REPO_ROOT/src-tauri/target/debug/tsumugi"

TMP_HOME="$(mktemp -d /tmp/tsumugi-e2e-XXXXXX)"
export XDG_CONFIG_HOME="$TMP_HOME/config"
export XDG_CACHE_HOME="$TMP_HOME/cache"
export XDG_DATA_HOME="$TMP_HOME/data"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"

export BROWSER="$REPO_ROOT/e2e/helpers/browser-open.sh"

exec dbus-run-session -- "$BINARY" "$@"
