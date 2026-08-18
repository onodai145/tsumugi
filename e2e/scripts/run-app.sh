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
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME" "$XDG_DATA_HOME/applications"

export BROWSER="$REPO_ROOT/e2e/helpers/browser-open.sh"
export E2E_MIAUTH_CDP_PORT="${E2E_MIAUTH_CDP_PORT:-9333}"

# xdg-open は https:// URLを開く際、BROWSER環境変数を見る前に
# x-scheme-handler/https の既定アプリ(mimeapps.list経由、通常は実機の
# システム既定ブラウザ=本物のFirefox)を優先して起動してしまう
# (/usr/bin/xdg-open: open_generic() -> open_generic_xdg_x_scheme_handler()
# が open_envvar()=BROWSER判定より先に呼ばれる。実機検証済み)。
#
# 以前はXDG_DATA_DIRSを空ディレクトリに向けてシステム既定の解決自体を
# 失敗させることでBROWSERへのフォールバックを狙ったが、これはGTKの
# アイコン/MIMEデータベース探索(/usr/share/icons, /usr/share/mime等、
# XDG_DATA_DIRS経由で見つかる)も道連れに壊してしまい、アプリ自体が
# 起動直後にGtk:ERROR(アイコンをロードできない)でabort(SIGABRT)して
# しまうことが判明した(tauri-driverからはアプリが無応答のまま見え、
# POST /sessionが2分でタイムアウトする、という形で観測された)。
#
# 代わりに、システムのXDG_DATA_DIRSはそのまま(アイコン/MIME解決用に)
# 残しつつ、x-scheme-handler/https の既定アプリ登録だけを、xdg-open/
# xdg-mimeが最優先で参照する $XDG_CONFIG_HOME/mimeapps.list で上書きする。
# 登録先は BROWSER と同じ browser-open.sh を直接叩く独自の.desktopファイル
# ($XDG_DATA_HOME/applications、こちらも最優先で探索される)にすることで、
# BROWSER環境変数のフォールバック経路に頼らず確実にCDPブリッジへ誘導する。
#
# ファイル名にハイフンを含めてはいけない: /usr/bin/xdg-mime の
# desktop_file_to_binary() は "vendor-app.desktop" という命名規則を
# 前提にハイフンで vendor/app に分割しようとするため、ハイフン入りの
# 名前(例: tsumugi-e2e-browser.desktop)だとその分割ロジックが誤動作し、
# 実際にはフラットに置いてあるファイルを見つけられずデフォルト解決に
# 失敗する(実機検証済み)。
BROWSER_DESKTOP_NAME="tsumugie2ebrowser.desktop"
cat > "$XDG_DATA_HOME/applications/$BROWSER_DESKTOP_NAME" <<EOF
[Desktop Entry]
Type=Application
Name=tsumugi e2e test browser opener
Exec=$REPO_ROOT/e2e/helpers/browser-open.sh %u
NoDisplay=true
MimeType=x-scheme-handler/https;x-scheme-handler/http;
EOF
cat > "$XDG_CONFIG_HOME/mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/https=$BROWSER_DESKTOP_NAME
x-scheme-handler/http=$BROWSER_DESKTOP_NAME
EOF

# tsumugi本体(Rust側のreqwest経由のHTTPS/WebSocket通信)もmisskey.localを
# 解決する必要があるが、このe2eサンドボックスの/etc/hostsにはエントリが無く、
# sudoにパスワードが必要で書き換えられない(実機確認済み)。BROWSERやNode側の
# fetch()と違い、Rustのreqwest/tokioは自前でDNSをフックする手段を持たない
# (getaddrinfo系。プロセスごとの上書きが効かない)ため、ここだけは
# unshare(1)の非特権ユーザー名前空間+マウント名前空間を使い、実システムの
# /etc/hostsに一切触れずに、tsumugiプロセスからだけ見える/etc/hostsを
# bind mountで差し替える(root権限は名前空間内だけの偽装で完結し、
# 実機のsudo/rootは一切不要。実機検証済み: `unshare --user --map-root-user
# --mount -- ...`でmount --bindが成功する)。
#
# /etc/hostsのbind mountだけでは不十分だった(実機検証済み): このホストの
# /etc/nsswitch.confは `hosts: mymachines mdns_minimal [NOTFOUND=return]
# resolve [!UNAVAIL=return] files myhostname dns` で、systemd-resolved経由の
# "resolve"モジュールが"files"(/etc/hosts)より先に来る。systemd-resolvedは
# 名前空間の外で動く別プロセスなので実物の/etc/hostsしか見えず、
# misskey.localについて確定的にNXDOMAINを返し、`[!UNAVAIL=return]`により
# そこで解決が打ち切られて"files"には一切到達しない。そのため
# /etc/nsswitch.confも同様にbind mountし、hostsの参照順を`files`優先に
# 上書きする(この2ファイルのbind mountだけで完結し、Rustコード変更は不要)。
TMP_HOSTS="$TMP_HOME/hosts"
cat /etc/hosts > "$TMP_HOSTS"
echo "127.0.0.1 misskey.local" >> "$TMP_HOSTS"
TMP_NSSWITCH="$TMP_HOME/nsswitch.conf"
sed 's/^hosts:.*/hosts: files mdns_minimal [NOTFOUND=return] resolve [!UNAVAIL=return] myhostname dns/' /etc/nsswitch.conf > "$TMP_NSSWITCH"
export TSUMUGI_E2E_TMP_HOSTS="$TMP_HOSTS"
export TSUMUGI_E2E_TMP_NSSWITCH="$TMP_NSSWITCH"
export TSUMUGI_E2E_BINARY="$BINARY"

# tsumugiのreqwestは rustls-tls-native-roots (rustls-native-certs) でOSの
# 信頼ストアを読む設定になっているが、e2e用の自己署名テストCA
# (e2e/certs/ca.pem)はそこには入っていない。install-ca.sh経由でOSの信頼
# ストアに登録する方法(update-ca-certificates等)はディストリ依存かつ
# sudoが要る。rustls-native-certsはSSL_CERT_FILEが設定されていれば
# OSの通常の探索を一切せずそのファイルだけを読む(実装確認済み:
# rustls-native-certs 0.8.4 の load_native_certs() は CertPaths::from_env()
# を先にチェックし、SSL_CERT_FILE/SSL_CERT_DIRが設定されていればそちらを
# 優先する)ため、実システムの信頼ストア(/etc/ssl/certs/ca-certificates.crt)
# の内容にこのテストCAを追記したファイルを作り、SSL_CERT_FILEで指すだけで
# sudo無しに完結する。
TMP_CERT_BUNDLE="$TMP_HOME/ca-bundle.pem"
cat /etc/ssl/certs/ca-certificates.crt "$REPO_ROOT/e2e/certs/ca.pem" > "$TMP_CERT_BUNDLE"
export SSL_CERT_FILE="$TMP_CERT_BUNDLE"

exec unshare --user --map-root-user --mount -- bash -c '
  mount --bind "$TSUMUGI_E2E_TMP_HOSTS" /etc/hosts
  mount --bind "$TSUMUGI_E2E_TMP_NSSWITCH" /etc/nsswitch.conf
  exec dbus-run-session -- "$TSUMUGI_E2E_BINARY" "$@"
' bash "$@"
