#!/usr/bin/env bash
# opener プラグインが呼ぶ「デフォルトブラウザ起動コマンド」の差し替え先。
# 実際にはブラウザを新規起動せず、miauthBridge.ts が立てたCDPセッションに
# 新規タブとしてURLを開かせる。
set -euo pipefail
URL="$1"
CDP_PORT="${E2E_MIAUTH_CDP_PORT:-9333}"
# Chrome >=111 requires PUT (GET returns 405), and the target URL must be
# percent-encoded onto the query string as a whole (curl -G --data-urlencode
# "=$URL" appends it raw-value, no "name=" prefix) so that MiAuth URLs
# carrying their own "?name=...&callback=...&permission=..." query params
# aren't misparsed as query params of /json/new itself.
curl -sf -X PUT -G "http://localhost:${CDP_PORT}/json/new" --data-urlencode "=${URL}" > /dev/null
