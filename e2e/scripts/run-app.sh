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
# fetch()と違い、Rustのreqwest/tokioは自前でDNSをフックする手段を持たない。
#
# --- 経緯: /etc/hosts bind mount方式は最終的に採用しなかった ---
# 当初は非特権ユーザー名前空間(unshare(1)/bwrap(1))を使い、tsumugiプロセス
# からだけ見える/etc/hosts・/etc/nsswitch.confをbind mountで差し替える方式を
# 実装したが、実機検証の結果、この方式にはsession/secrets.rs(OS Secret
# Service経由のkeyring-coreストア)との深刻な非互換が判明した:
#   - `unshare --user --map-root-user`の中でgnome-keyring-daemonを起動すると
#     `failed dropping capabilities - -11, aborting`で即abortする(実際に
#     失敗するsyscallは`setgroups(0, NULL) = -1 EPERM`。CVE-2014-8989対策で
#     新規ユーザー名前空間は既定でsetgroupsが"deny"であり、`unshare
#     --map-root-user`はこれを"allow"にする経路を持たない)。
#   - 回避策としてbwrap(1)(Flatpak等が使う非特権サンドボックス専用ツール、
#     setgroups許可のタイミングを正しく処理する)に切り替え、gnome-keyring-
#     daemon自体は起動できるようになったが、今度は名前空間内でuid 0として
#     動くtsumugi本体のzbusクライアントが、名前空間の外(実uid 1000)で
#     動くD-Busセッションバスへの接続時に`D-Bus handshake failed: EXTERNAL
#     rejected by the server`で拒否されることが分かった。手書きのzbus単体
#     再現コードで検証した結果、libdbus系クライアント(secret-tool,
#     dbus-send)は同じ名前空間トポロジで問題なく認証できる一方、zbusは
#     常に拒否される(gnome-keyring-daemon自身の接続が先に成功した後でも
#     zbusだけ拒否され続けることまで確認済み)。zbus 5.16.0のEXTERNAL認証
#     実装(sasl_auth_id() = geteuid()の10進文字列)はDBus仕様通りに見え、
#     正確な原因(zbus固有のソケット接続シーケンスとカーネルのSO_PEERCRED
#     評価タイミングの相互作用と推測される)は特定できなかった。
# この非互換はuid remap(名前空間内でuid 0になる)そのものに起因するため、
# 名前空間を使わない方式へ切り替えた: LD_PRELOADで`getaddrinfo()`を
# フックし、"misskey.local"だけを127.0.0.1に固定解決するごく小さな共有
# ライブラリ(helpers/misskey-dns-hook.c)を使う。これによりuid remapが
# 一切不要になり、gnome-keyring-daemonは実uid 1000のまま素の
# dbus-run-session配下で起動でき(最初に検証した、最もシンプルで問題の
# 無かった構成)、mount --bind自体も不要になった。
DNS_HOOK_SRC="$REPO_ROOT/e2e/helpers/misskey-dns-hook.c"
DNS_HOOK_SO="$TMP_HOME/misskey-dns-hook.so"
gcc -shared -fPIC -O2 -o "$DNS_HOOK_SO" "$DNS_HOOK_SRC" -ldl
export LD_PRELOAD="$DNS_HOOK_SO"

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

# session/secrets.rs はOS Secret Service(D-Bus org.freedesktop.secrets)経由の
# keyring-coreストアを常時使う(本番コード分岐なし、設計として意図通り。
# docs/superpowers/specs/2026-08-17-e2e-automation-design.md 2節参照)。
# dbus-run-sessionが立てる一時セッションバスにはSecret Serviceの実装が
# 何も乗っていないため、org.freedesktop.secretsの所有者が存在せず、
# MiAuth完了後のトークン保存が
# `secret: Platform failure: zbus error: ... Process org.freedesktop.secrets
# exited with status 1` で失敗する(実機確認済み)。design specが元々
# 指定していた `gnome-keyring-daemon --unlock --daemonize` の起動が
# run-app.sh実装時に抜け落ちていたのが原因。
#
# --unlock は標準入力からログインキーリングのパスワードを読む。XDG_DATA_HOME
# は毎回mktempの新規空ディレクトリなので既存キーリングは無く、空文字列を
# 渡すだけで新規キーリングが作成されその場でアンロックされる(実機確認済み:
# `echo "" | gnome-keyring-daemon --unlock --daemonize --components=secrets`
# 後、secret-tool store/lookupが成功する)。使い捨てのテスト専用キーリング
# なので空パスワードで問題ない。dbus-run-session配下・実uid 1000のままの
# 素のプロセスとして起動する(上記のLD_PRELOAD切り替えにより、名前空間や
# uid remapが一切不要になったため、この起動もシンプルなまま保てる)。

# `xvfb-run -a pnpm e2e`(design spec/brief記載の既定の実行コマンド。変更
# しない)が使うXvfbの既定画面サイズは`xvfb-run`本体にハードコードされた
# `-screen 0 640x480x24`で、これはtauri.conf.jsonのウィンドウサイズ
# (800x600)より小さい(実機確認済み: `head -33 /usr/bin/xvfb-run`)。
# ウィンドウマネージャの無いXvfb上ではアプリのウィンドウはこの640x480の
# フレームバッファに収まる範囲でしか描画されず、モーダル等で画面下部に
# あるボタンは物理的にビューポート外へ出てしまい、WebdriverIOの
# `waitForClickable()`/`click()`が(スクロールしても)要素を掴めなくなる
# (実機確認済み: `document.elementFromPoint()`で該当ボタンの中心座標が
# ビューポート外のためnullになることを確認)。`xvfb-run`のスクリーン
# サイズは`-s`/`--server-args`でしか変更できず環境変数越しの上書きは
# 効かないため(実機確認済み)、外側の`xvfb-run -a pnpm e2e`はそのままに、
# ここ(tsumugiプロセスを実際に起動する直前)でtauri-driverから見えない
# 内側専用の、より大きなXvfbを起動する。サーバー番号は外側の既定値
# (:99)と衝突しないよう明示的に別番号を指定する。
#
# 当初はこれを`xvfb-run -a -n 88 -s "..." dbus-run-session -- ...`という
# ネストしたxvfb-run呼び出しで実装したが、実機検証の結果プロセスリーク
# することが判明した: /usr/bin/xvfb-runのソース(末尾)を読むと、ラップ
# したコマンドを`exec`ではなく素の`"$@"`(フォアグラウンド子プロセス)
# として起動し、その終了を待ってからXvfbをkillする設計になっている。
# ところがrun-app.sh自身は`exec xvfb-run ...`としていたため、
# run-app.shのPIDはxvfb-run自身になる。tauri-driverがセッション終了時に
# このPIDへシグナルを送っても、xvfb-runはtrapによるシグナル転送を
# 一切行っておらず、フォアグラウンド待機中の子プロセスツリー
# (dbus-run-session・gnome-keyring-daemon・tsumugi本体・ネストした
# Xvfb自身)を道連れにできず、initに再親化された孤児プロセスとして
# 残留することを実機で確認した。
#
# そのため、Xvfbは自前でバックグラウンド起動し、`trap`でSIGTERM/SIGINT/
# 終了時にプロセスグループ全体(`kill -TERM -- -$pgid`)へシグナルを
# 転送してから`wait`する構成に変更した(`set -m`でジョブ制御を有効化し、
# バックグラウンドで起動したプロセスグループのリーダーPIDをそのまま
# プロセスグループIDとして使う)。これにより、tauri-driverがrun-app.sh
# のPIDをどう終了させても(SIGTERM/SIGINT経由なら)、Xvfb・
# dbus-run-session・gnome-keyring-daemon・tsumugi本体を含む配下の
# プロセス全てに確実にシグナルが届く(実機検証済み: SIGTERMを送って
# `ps -ef`で残留プロセスが無いことを確認)。
set -m
Xvfb :88 -screen 0 1280x1024x24 -nolisten tcp &
XVFB_PID=$!
for _ in $(seq 1 30); do
  DISPLAY=:88 xdpyinfo >/dev/null 2>&1 && break
  sleep 0.2
done
export DISPLAY=:88

dbus-run-session -- bash -c '
  eval "$(echo "" | gnome-keyring-daemon --unlock --daemonize --components=secrets)"
  export GNOME_KEYRING_CONTROL
  exec "$0" "$@"
' "$BINARY" "$@" &
APP_PID=$!

cleanup() {
  kill -TERM -- "-$APP_PID" 2>/dev/null || true
  kill -TERM "$XVFB_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

set +e
wait "$APP_PID"
EXIT_CODE=$?
set -e
cleanup
exit "$EXIT_CODE"
